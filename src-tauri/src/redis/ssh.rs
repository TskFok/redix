use std::{
    collections::HashMap,
    future::Future,
    io::{self, Read, Write},
    net::{Shutdown, SocketAddr, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

use crate::{
    domain::{ConnectionEndpoint, SshAuthMethod, SshConfig},
    error::AppError,
    persistence::ConnectionSecrets,
};

const SSH_CONNECT_TIMEOUT: Duration = Duration::from_secs(6);
const PROXY_IDLE_WAIT: Duration = Duration::from_millis(2);
const MAX_PROXY_WORKERS: usize = 32;
const MAX_BLOCKING_SSH_WORKERS: usize = 64;

fn blocking_ssh_limit() -> Arc<tokio::sync::Semaphore> {
    static LIMIT: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
    LIMIT
        .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(MAX_BLOCKING_SSH_WORKERS)))
        .clone()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostKeyStatus {
    Match,
    Mismatch,
    NotFound,
    Failure,
}

fn verify_host_key(status: HostKeyStatus) -> Result<(), AppError> {
    match status {
        HostKeyStatus::Match => Ok(()),
        HostKeyStatus::Mismatch | HostKeyStatus::NotFound | HostKeyStatus::Failure => {
            Err(AppError::SshTunnelFailed)
        }
    }
}

impl From<ssh2::CheckResult> for HostKeyStatus {
    fn from(value: ssh2::CheckResult) -> Self {
        match value {
            ssh2::CheckResult::Match => Self::Match,
            ssh2::CheckResult::Mismatch => Self::Mismatch,
            ssh2::CheckResult::NotFound => Self::NotFound,
            ssh2::CheckResult::Failure => Self::Failure,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum AuthSelection<'a> {
    Agent,
    Password(&'a str),
    PrivateKeyMemory {
        private_key: &'a str,
        passphrase: Option<&'a str>,
    },
    PrivateKeyFile {
        identity_file: &'a Path,
        passphrase: Option<&'a str>,
    },
}

fn select_auth<'a>(
    config: &SshConfig,
    secrets: &'a ConnectionSecrets,
) -> Result<AuthSelection<'a>, AppError> {
    match config.auth_method {
        SshAuthMethod::Agent if config.has_identity_file => {
            let identity_file = secrets
                .ssh_identity_file
                .as_deref()
                .filter(|value| valid_secret_path(Path::new(value)))
                .ok_or(AppError::SshTunnelFailed)?;
            Ok(AuthSelection::PrivateKeyFile {
                identity_file: Path::new(identity_file),
                passphrase: secrets.ssh_passphrase.as_deref(),
            })
        }
        SshAuthMethod::Agent => Ok(AuthSelection::Agent),
        SshAuthMethod::Password => secrets
            .ssh_password
            .as_deref()
            .filter(|value| !value.is_empty())
            .map(AuthSelection::Password)
            .ok_or(AppError::SshTunnelFailed),
        SshAuthMethod::PrivateKey => {
            if let Some(private_key) = secrets
                .ssh_private_key
                .as_deref()
                .filter(|value| !value.is_empty())
            {
                return Ok(AuthSelection::PrivateKeyMemory {
                    private_key,
                    passphrase: secrets.ssh_passphrase.as_deref(),
                });
            }
            let identity_file = secrets
                .ssh_identity_file
                .as_deref()
                .filter(|value| valid_secret_path(Path::new(value)))
                .ok_or(AppError::SshTunnelFailed)?;
            Ok(AuthSelection::PrivateKeyFile {
                identity_file: Path::new(identity_file),
                passphrase: secrets.ssh_passphrase.as_deref(),
            })
        }
    }
}

fn valid_secret_path(path: &Path) -> bool {
    path.is_absolute()
        && path
            .to_str()
            .is_some_and(|value| !value.contains(['\n', '\r', '\0']))
}

fn select_known_hosts_path(
    secrets: &ConnectionSecrets,
    home: Option<&Path>,
) -> Result<PathBuf, AppError> {
    if let Some(path) = secrets.ssh_known_hosts_file.as_deref() {
        let path = Path::new(path);
        return valid_secret_path(path)
            .then(|| path.to_owned())
            .ok_or(AppError::SshTunnelFailed);
    }
    home.map(|path| path.join(".ssh").join("known_hosts"))
        .ok_or(AppError::SshTunnelFailed)
}

fn platform_home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .or_else(|| {
                let drive = std::env::var_os("HOMEDRIVE")?;
                let path = std::env::var_os("HOMEPATH")?;
                Some(PathBuf::from(drive).join(path))
            })
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

trait ProxyChannel: Read + Write + Send {
    fn eof(&self) -> bool;
    fn send_eof(&mut self) -> io::Result<()>;
}

impl ProxyChannel for ssh2::Channel {
    fn eof(&self) -> bool {
        ssh2::Channel::eof(self)
    }

    fn send_eof(&mut self) -> io::Result<()> {
        ssh2::Channel::send_eof(self).map_err(io::Error::from)
    }
}

trait SessionBackend: Send + Sync {
    fn authenticate(&self, config: &SshConfig, secrets: &ConnectionSecrets)
        -> Result<(), AppError>;
    fn open_channel(
        &self,
        target: &ConnectionEndpoint,
        cancelled: &AtomicBool,
    ) -> Result<Box<dyn ProxyChannel>, AppError>;
    fn disconnect(&self);
}

struct Libssh2Backend {
    session: ssh2::Session,
    socket: TcpStream,
}

impl SessionBackend for Libssh2Backend {
    fn authenticate(
        &self,
        config: &SshConfig,
        secrets: &ConnectionSecrets,
    ) -> Result<(), AppError> {
        authenticate_session(&self.session, config, secrets)
    }

    fn open_channel(
        &self,
        target: &ConnectionEndpoint,
        cancelled: &AtomicBool,
    ) -> Result<Box<dyn ProxyChannel>, AppError> {
        let deadline = Instant::now() + SSH_CONNECT_TIMEOUT;
        loop {
            if cancelled.load(Ordering::SeqCst) {
                return Err(AppError::SshTunnelFailed);
            }
            match self
                .session
                .channel_direct_tcpip(&target.host, target.port, None)
            {
                Ok(channel) => return Ok(Box::new(channel)),
                Err(error) => {
                    if io::Error::from(error).kind() != io::ErrorKind::WouldBlock
                        || Instant::now() >= deadline
                    {
                        return Err(AppError::SshTunnelFailed);
                    }
                    std::thread::sleep(PROXY_IDLE_WAIT);
                }
            }
        }
    }

    fn disconnect(&self) {
        let _ = self.session.disconnect(None, "application shutdown", None);
        let _ = self.socket.shutdown(Shutdown::Both);
    }
}

struct SessionOwner {
    backend: Arc<dyn SessionBackend>,
}

impl Drop for SessionOwner {
    fn drop(&mut self) {
        self.backend.disconnect();
    }
}

#[derive(Clone)]
pub(super) struct SshTransport {
    session: Arc<SessionOwner>,
    proxy_limit: Arc<tokio::sync::Semaphore>,
}

impl SshTransport {
    pub async fn connect(
        config: &SshConfig,
        secrets: &ConnectionSecrets,
    ) -> Result<Self, AppError> {
        config.validate().map_err(|_| AppError::SshTunnelFailed)?;
        connect_with_resolver(
            config.clone(),
            secrets.clone(),
            blocking_ssh_limit(),
            SSH_CONNECT_TIMEOUT,
            |host, port| async move {
                tokio::net::lookup_host((host.as_str(), port))
                    .await
                    .map(|addresses| addresses.take(8).collect())
            },
        )
        .await
    }

    fn from_backend(backend: Arc<dyn SessionBackend>) -> Self {
        Self::from_backend_with_proxy_limit(backend, MAX_PROXY_WORKERS)
    }

    fn from_backend_with_proxy_limit(backend: Arc<dyn SessionBackend>, proxy_limit: usize) -> Self {
        Self {
            session: Arc::new(SessionOwner { backend }),
            proxy_limit: Arc::new(tokio::sync::Semaphore::new(proxy_limit)),
        }
    }

    pub async fn forward(&self, target: &ConnectionEndpoint) -> Result<Arc<SshForward>, AppError> {
        if !valid_forward_target(target) {
            return Err(AppError::SshTunnelFailed);
        }
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|_| AppError::SshTunnelFailed)?;
        let endpoint = ConnectionEndpoint {
            host: "127.0.0.1".into(),
            port: listener
                .local_addr()
                .map_err(|_| AppError::SshTunnelFailed)?
                .port(),
        };
        let state = Arc::new(ForwardState::default());
        let accept_task = spawn_accept_task(
            listener,
            Arc::downgrade(&self.session),
            self.proxy_limit.clone(),
            target.clone(),
            state.clone(),
        );
        Ok(Arc::new(SshForward {
            endpoint,
            state,
            accept_task,
            _session: self.session.clone(),
        }))
    }
}

fn valid_forward_target(target: &ConnectionEndpoint) -> bool {
    target.port != 0
        && target.host.len() <= 256
        && (target.host.parse::<std::net::Ipv6Addr>().is_ok()
            || (!target.host.is_empty()
                && !target.host.starts_with('-')
                && target.host.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')
                })))
}

async fn connect_with_resolver<R, F>(
    config: SshConfig,
    secrets: ConnectionSecrets,
    blocking_limit: Arc<tokio::sync::Semaphore>,
    timeout: Duration,
    resolver: R,
) -> Result<SshTransport, AppError>
where
    R: FnOnce(String, u16) -> F,
    F: Future<Output = io::Result<Vec<SocketAddr>>>,
{
    let addresses = tokio::time::timeout(timeout, resolver(config.host.clone(), config.port))
        .await
        .map_err(|_| AppError::SshTunnelFailed)?
        .map_err(|_| AppError::SshTunnelFailed)?;
    let addresses = addresses.into_iter().take(8).collect::<Vec<_>>();
    if addresses.is_empty() {
        return Err(AppError::SshTunnelFailed);
    }
    let permit = tokio::time::timeout(timeout, blocking_limit.acquire_owned())
        .await
        .map_err(|_| AppError::SshTunnelFailed)?
        .map_err(|_| AppError::SshTunnelFailed)?;
    let backend = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        connect_blocking(&config, &secrets, addresses)
    })
    .await
    .map_err(|_| AppError::SshTunnelFailed)??;
    Ok(SshTransport::from_backend(backend))
}

fn connect_blocking(
    config: &SshConfig,
    secrets: &ConnectionSecrets,
    addresses: Vec<SocketAddr>,
) -> Result<Arc<dyn SessionBackend>, AppError> {
    let socket = connect_socket(addresses)?;
    socket
        .set_read_timeout(Some(SSH_CONNECT_TIMEOUT))
        .map_err(|_| AppError::SshTunnelFailed)?;
    socket
        .set_write_timeout(Some(SSH_CONNECT_TIMEOUT))
        .map_err(|_| AppError::SshTunnelFailed)?;
    let control_socket = socket.try_clone().map_err(|_| AppError::SshTunnelFailed)?;

    let mut session = ssh2::Session::new().map_err(|_| AppError::SshTunnelFailed)?;
    session.set_tcp_stream(socket);
    session.handshake().map_err(|_| AppError::SshTunnelFailed)?;
    verify_session_host_key(&session, config, secrets)?;
    let backend = Arc::new(Libssh2Backend {
        session,
        socket: control_socket,
    });
    authenticate_backend(backend.as_ref(), config, secrets)?;
    if !backend.session.authenticated() {
        return Err(AppError::SshTunnelFailed);
    }

    backend
        .socket
        .set_nonblocking(true)
        .map_err(|_| AppError::SshTunnelFailed)?;
    backend.session.set_blocking(false);
    Ok(backend)
}

fn connect_socket(addresses: Vec<SocketAddr>) -> Result<TcpStream, AppError> {
    let deadline = Instant::now() + SSH_CONNECT_TIMEOUT;
    for address in addresses.into_iter().take(8) {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            break;
        };
        match TcpStream::connect_timeout(&address, remaining) {
            Ok(socket) => return Ok(socket),
            Err(_) if Instant::now() < deadline => continue,
            Err(_) => break,
        }
    }
    Err(AppError::SshTunnelFailed)
}

fn verify_session_host_key(
    session: &ssh2::Session,
    config: &SshConfig,
    secrets: &ConnectionSecrets,
) -> Result<(), AppError> {
    let path = select_known_hosts_path(secrets, platform_home_dir().as_deref())?;
    let mut known_hosts = session
        .known_hosts()
        .map_err(|_| AppError::SshTunnelFailed)?;
    known_hosts
        .read_file(&path, ssh2::KnownHostFileKind::OpenSSH)
        .map_err(|_| AppError::SshTunnelFailed)?;
    let (key, _) = session.host_key().ok_or(AppError::SshTunnelFailed)?;
    check_known_host(&known_hosts, &config.host, config.port, key)
}

fn check_known_host(
    known_hosts: &ssh2::KnownHosts,
    host: &str,
    port: u16,
    key: &[u8],
) -> Result<(), AppError> {
    verify_host_key(known_hosts.check_port(host, port, key).into())
}

fn authenticate_backend(
    backend: &dyn SessionBackend,
    config: &SshConfig,
    secrets: &ConnectionSecrets,
) -> Result<(), AppError> {
    backend.authenticate(config, secrets)
}

fn authenticate_session(
    session: &ssh2::Session,
    config: &SshConfig,
    secrets: &ConnectionSecrets,
) -> Result<(), AppError> {
    match select_auth(config, secrets)? {
        AuthSelection::Agent => session.userauth_agent(&config.username),
        AuthSelection::Password(password) => session.userauth_password(&config.username, password),
        AuthSelection::PrivateKeyMemory {
            private_key,
            passphrase,
        } => session.userauth_pubkey_memory(&config.username, None, private_key, passphrase),
        AuthSelection::PrivateKeyFile {
            identity_file,
            passphrase,
        } => session.userauth_pubkey_file(&config.username, None, identity_file, passphrase),
    }
    .map_err(|_| AppError::SshTunnelFailed)
}

#[derive(Default)]
struct ForwardState {
    cancelled: AtomicBool,
    next_id: AtomicU64,
    active: Mutex<HashMap<u64, TcpStream>>,
}

impl ForwardState {
    fn register(&self, socket: &TcpStream) -> Result<u64, AppError> {
        let copy = socket.try_clone().map_err(|_| AppError::SshTunnelFailed)?;
        let mut active = self.active.lock().map_err(|_| AppError::SshTunnelFailed)?;
        if self.cancelled.load(Ordering::SeqCst) {
            let _ = copy.shutdown(Shutdown::Both);
            return Err(AppError::SshTunnelFailed);
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        active.insert(id, copy);
        Ok(id)
    }

    fn remove(&self, id: u64) {
        if let Ok(mut active) = self.active.lock() {
            active.remove(&id);
        }
    }

    fn cancel_all(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Ok(mut active) = self.active.lock() {
            for socket in active.values() {
                let _ = socket.shutdown(Shutdown::Both);
            }
            active.clear();
        }
    }
}

pub(super) struct SshForward {
    endpoint: ConnectionEndpoint,
    state: Arc<ForwardState>,
    accept_task: tokio::task::JoinHandle<()>,
    _session: Arc<SessionOwner>,
}

impl SshForward {
    pub fn local_endpoint(&self) -> ConnectionEndpoint {
        self.endpoint.clone()
    }
}

impl Drop for SshForward {
    fn drop(&mut self) {
        self.state.cancel_all();
        self.accept_task.abort();
    }
}

#[cfg(test)]
pub(super) struct TestForwardingBackend {
    routes: Vec<(ConnectionEndpoint, SocketAddr)>,
    forwarded: Mutex<Vec<ConnectionEndpoint>>,
}

#[cfg(test)]
impl TestForwardingBackend {
    pub(super) fn forwarded_targets(&self) -> Vec<ConnectionEndpoint> {
        self.forwarded.lock().unwrap().clone()
    }
}

#[cfg(test)]
struct TcpProxyChannel {
    stream: TcpStream,
    eof: bool,
}

#[cfg(test)]
impl Read for TcpProxyChannel {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let size = self.stream.read(buffer)?;
        if size == 0 {
            self.eof = true;
        }
        Ok(size)
    }
}

#[cfg(test)]
impl Write for TcpProxyChannel {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.stream.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

#[cfg(test)]
impl ProxyChannel for TcpProxyChannel {
    fn eof(&self) -> bool {
        self.eof
    }

    fn send_eof(&mut self) -> io::Result<()> {
        self.stream.shutdown(Shutdown::Write)
    }
}

#[cfg(test)]
impl SessionBackend for TestForwardingBackend {
    fn authenticate(
        &self,
        _config: &SshConfig,
        _secrets: &ConnectionSecrets,
    ) -> Result<(), AppError> {
        Ok(())
    }

    fn open_channel(
        &self,
        target: &ConnectionEndpoint,
        _cancelled: &AtomicBool,
    ) -> Result<Box<dyn ProxyChannel>, AppError> {
        let address = self
            .routes
            .iter()
            .find_map(|(candidate, address)| (candidate == target).then_some(*address))
            .ok_or(AppError::SshTunnelFailed)?;
        let stream = TcpStream::connect(address).map_err(|_| AppError::SshTunnelFailed)?;
        stream
            .set_nonblocking(true)
            .map_err(|_| AppError::SshTunnelFailed)?;
        self.forwarded
            .lock()
            .map_err(|_| AppError::SshTunnelFailed)?
            .push(target.clone());
        Ok(Box::new(TcpProxyChannel { stream, eof: false }))
    }

    fn disconnect(&self) {}
}

#[cfg(test)]
pub(super) fn test_forwarding_transport(
    routes: Vec<(ConnectionEndpoint, SocketAddr)>,
) -> (SshTransport, Arc<TestForwardingBackend>) {
    let backend = Arc::new(TestForwardingBackend {
        routes,
        forwarded: Mutex::new(Vec::new()),
    });
    (SshTransport::from_backend(backend.clone()), backend)
}

fn spawn_accept_task(
    listener: tokio::net::TcpListener,
    session: std::sync::Weak<SessionOwner>,
    proxy_limit: Arc<tokio::sync::Semaphore>,
    target: ConnectionEndpoint,
    state: Arc<ForwardState>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let (socket, _) = match listener.accept().await {
                Ok(value) => value,
                Err(_) => break,
            };
            let socket = match socket.into_std() {
                Ok(socket) => socket,
                Err(_) => continue,
            };
            let permit = match proxy_limit.clone().try_acquire_owned() {
                Ok(permit) => permit,
                Err(_) => {
                    let _ = socket.shutdown(Shutdown::Both);
                    continue;
                }
            };
            let blocking_permit = match blocking_ssh_limit().try_acquire_owned() {
                Ok(permit) => permit,
                Err(_) => {
                    let _ = socket.shutdown(Shutdown::Both);
                    continue;
                }
            };
            let id = match state.register(&socket) {
                Ok(id) => id,
                Err(_) => break,
            };
            let Some(session_owner) = session.upgrade() else {
                state.remove(id);
                let _ = socket.shutdown(Shutdown::Both);
                break;
            };
            let worker_backend = session_owner.backend.clone();
            drop(session_owner);
            let worker_target = target.clone();
            let worker_state = state.clone();
            tokio::task::spawn_blocking(move || {
                let _permit = permit;
                let _blocking_permit = blocking_permit;
                if let Ok(channel) =
                    worker_backend.open_channel(&worker_target, &worker_state.cancelled)
                {
                    proxy_connection(socket, channel, &worker_state.cancelled);
                }
                worker_state.remove(id);
            });
        }
    })
}

fn proxy_connection(
    mut local: TcpStream,
    mut remote: Box<dyn ProxyChannel>,
    cancelled: &AtomicBool,
) {
    if local.set_nonblocking(true).is_err() {
        return;
    }
    let mut to_remote = [0_u8; 16 * 1024];
    let mut to_remote_range = 0..0;
    let mut to_local = [0_u8; 16 * 1024];
    let mut to_local_range = 0..0;
    let mut local_eof = false;
    let mut local_eof_sent = false;
    let mut remote_eof = false;

    while !cancelled.load(Ordering::SeqCst) {
        let mut progress = false;

        if to_remote_range.is_empty() && !local_eof {
            match local.read(&mut to_remote) {
                Ok(0) => local_eof = true,
                Ok(size) => {
                    to_remote_range = 0..size;
                    progress = true;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(_) => break,
            }
        }
        if !to_remote_range.is_empty() {
            match remote.write(&to_remote[to_remote_range.clone()]) {
                Ok(0) => {}
                Ok(size) => {
                    to_remote_range.start += size;
                    progress = true;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(_) => break,
            }
        }
        if local_eof && to_remote_range.is_empty() && !local_eof_sent {
            match remote.send_eof() {
                Ok(()) => {
                    local_eof_sent = true;
                    progress = true;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(_) => break,
            }
        }

        if to_local_range.is_empty() && !remote_eof {
            match remote.read(&mut to_local) {
                Ok(0) if remote.eof() => remote_eof = true,
                Ok(0) => {}
                Ok(size) => {
                    to_local_range = 0..size;
                    progress = true;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(_) => break,
            }
        }
        if !to_local_range.is_empty() {
            match local.write(&to_local[to_local_range.clone()]) {
                Ok(0) => break,
                Ok(size) => {
                    to_local_range.start += size;
                    progress = true;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(_) => break,
            }
        }

        if remote_eof && to_local_range.is_empty() {
            let _ = local.shutdown(Shutdown::Write);
            if local_eof {
                break;
            }
        }
        if !progress {
            std::thread::sleep(PROXY_IDLE_WAIT);
        }
    }

    let _ = local.shutdown(Shutdown::Both);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        future::pending,
        net::SocketAddr,
        sync::atomic::{AtomicUsize, Ordering},
    };

    fn ssh_config(auth_method: SshAuthMethod) -> SshConfig {
        serde_json::from_value(serde_json::json!({
            "host": "jump.example",
            "port": 22,
            "username": "operator",
            "auth_method": auth_method,
        }))
        .unwrap()
    }

    fn endpoint(host: &str, port: u16) -> ConnectionEndpoint {
        ConnectionEndpoint {
            host: host.into(),
            port,
        }
    }

    #[tokio::test]
    async fn dns_timeout_does_not_occupy_custom_blocking_permit() {
        let limit = Arc::new(tokio::sync::Semaphore::new(1));
        let (started, ready) = tokio::sync::oneshot::channel();
        let task_limit = limit.clone();
        let task = tokio::spawn(async move {
            connect_with_resolver(
                ssh_config(SshAuthMethod::Agent),
                ConnectionSecrets::default(),
                task_limit,
                Duration::from_millis(50),
                move |_, _| async move {
                    started.send(()).unwrap();
                    pending::<io::Result<Vec<SocketAddr>>>().await
                },
            )
            .await
        });

        ready.await.unwrap();
        let permit = limit
            .clone()
            .try_acquire_owned()
            .expect("DNS resolution must happen before acquiring a blocking permit");
        drop(permit);
        assert!(matches!(
            task.await.unwrap(),
            Err(AppError::SshTunnelFailed)
        ));
    }

    #[tokio::test]
    async fn blocking_permit_acquisition_is_timeout_bounded() {
        let limit = Arc::new(tokio::sync::Semaphore::new(1));
        let _held = limit.clone().acquire_owned().await.unwrap();
        let result = connect_with_resolver(
            ssh_config(SshAuthMethod::Agent),
            ConnectionSecrets::default(),
            limit,
            Duration::from_millis(20),
            |_, _| async { Ok(vec!["127.0.0.1:22".parse().unwrap()]) },
        )
        .await;
        assert!(matches!(result, Err(AppError::SshTunnelFailed)));
    }

    #[test]
    fn unknown_or_changed_host_key_is_never_accepted() {
        assert_eq!(
            verify_host_key(HostKeyStatus::NotFound),
            Err(AppError::SshTunnelFailed)
        );
        assert_eq!(
            verify_host_key(HostKeyStatus::Mismatch),
            Err(AppError::SshTunnelFailed)
        );
        assert_eq!(
            verify_host_key(HostKeyStatus::Failure),
            Err(AppError::SshTunnelFailed)
        );
        assert_eq!(verify_host_key(HostKeyStatus::Match), Ok(()));
    }

    #[test]
    fn openssh_known_hosts_entries_are_checked_with_port_for_all_ports() {
        use base64::Engine;

        let mut key = Vec::new();
        key.extend_from_slice(&(11_u32.to_be_bytes()));
        key.extend_from_slice(b"ssh-ed25519");
        key.extend_from_slice(&(32_u32.to_be_bytes()));
        key.extend_from_slice(&[7_u8; 32]);
        let encoded = base64::engine::general_purpose::STANDARD.encode(&key);
        let session = ssh2::Session::new().unwrap();

        for host_field in [
            "example.com",
            "[example.com]:22",
            "|1|AQIDBAUGBwgJCgsMDQ4PEBESExQ=|ZPCaIWkIizT/3iOVVyeg848Days=",
        ] {
            let mut known_hosts = session.known_hosts().unwrap();
            known_hosts
                .read_str(
                    &format!("{host_field} ssh-ed25519 {encoded}\n"),
                    ssh2::KnownHostFileKind::OpenSSH,
                )
                .unwrap();
            assert_eq!(
                check_known_host(&known_hosts, "example.com", 22, &key),
                Ok(()),
                "host field {host_field} must match port 22"
            );
        }

        let mut known_hosts = session.known_hosts().unwrap();
        known_hosts
            .read_str(
                &format!("other.example ssh-ed25519 {encoded}\n"),
                ssh2::KnownHostFileKind::OpenSSH,
            )
            .unwrap();
        assert_eq!(
            check_known_host(&known_hosts, "example.com", 22, &key),
            Err(AppError::SshTunnelFailed)
        );
        let mut changed_key = key.clone();
        *changed_key.last_mut().unwrap() = 8;
        assert_eq!(
            check_known_host(&known_hosts, "other.example", 22, &changed_key),
            Err(AppError::SshTunnelFailed)
        );
    }

    #[test]
    fn authentication_uses_only_the_selected_secret_material() {
        let secrets = ConnectionSecrets {
            ssh_password: Some("password-secret".into()),
            ssh_private_key: Some("inline-private-key".into()),
            ssh_passphrase: Some("key-passphrase".into()),
            ssh_identity_file: Some("/secret/identity".into()),
            ..Default::default()
        };
        assert_eq!(
            select_auth(
                &ssh_config(SshAuthMethod::Agent),
                &ConnectionSecrets::default()
            ),
            Ok(AuthSelection::Agent)
        );
        assert_eq!(
            select_auth(&ssh_config(SshAuthMethod::Password), &secrets),
            Ok(AuthSelection::Password("password-secret"))
        );
        assert_eq!(
            select_auth(&ssh_config(SshAuthMethod::PrivateKey), &secrets),
            Ok(AuthSelection::PrivateKeyMemory {
                private_key: "inline-private-key",
                passphrase: Some("key-passphrase"),
            })
        );
    }

    #[test]
    fn private_key_file_is_used_only_when_inline_key_is_absent() {
        let secrets = ConnectionSecrets {
            ssh_identity_file: Some("/secret/identity".into()),
            ssh_passphrase: Some("key-passphrase".into()),
            ..Default::default()
        };
        assert_eq!(
            select_auth(&ssh_config(SshAuthMethod::PrivateKey), &secrets),
            Ok(AuthSelection::PrivateKeyFile {
                identity_file: Path::new("/secret/identity"),
                passphrase: Some("key-passphrase"),
            })
        );
    }

    #[test]
    fn legacy_agent_config_with_migrated_identity_keeps_identity_file_authentication() {
        let mut config = ssh_config(SshAuthMethod::Agent);
        config.has_identity_file = true;
        let secrets = ConnectionSecrets {
            ssh_identity_file: Some("/secret/migrated-identity".into()),
            ssh_passphrase: Some("key-passphrase".into()),
            ..Default::default()
        };
        assert_eq!(
            select_auth(&config, &secrets),
            Ok(AuthSelection::PrivateKeyFile {
                identity_file: Path::new("/secret/migrated-identity"),
                passphrase: Some("key-passphrase"),
            })
        );
    }

    #[test]
    fn missing_selected_auth_secret_fails_closed() {
        assert_eq!(
            select_auth(
                &ssh_config(SshAuthMethod::Password),
                &ConnectionSecrets::default()
            ),
            Err(AppError::SshTunnelFailed)
        );
        assert_eq!(
            select_auth(
                &ssh_config(SshAuthMethod::PrivateKey),
                &ConnectionSecrets::default()
            ),
            Err(AppError::SshTunnelFailed)
        );
    }

    #[test]
    fn secret_paths_precede_defaults_and_legacy_profile_paths_are_ignored() {
        let mut config = ssh_config(SshAuthMethod::PrivateKey);
        config.legacy_identity_file = Some("/profile/identity".into());
        config.legacy_known_hosts_file = Some("/profile/known_hosts".into());
        let secrets = ConnectionSecrets {
            ssh_identity_file: Some("/secret/identity".into()),
            ssh_known_hosts_file: Some("/secret/known_hosts".into()),
            ..Default::default()
        };
        assert_eq!(
            select_auth(&config, &secrets),
            Ok(AuthSelection::PrivateKeyFile {
                identity_file: Path::new("/secret/identity"),
                passphrase: None,
            })
        );
        assert_eq!(
            select_known_hosts_path(&secrets, Some(Path::new("/home/user"))),
            Ok(PathBuf::from("/secret/known_hosts"))
        );
        assert_eq!(
            select_known_hosts_path(&ConnectionSecrets::default(), Some(Path::new("/home/user"))),
            Ok(PathBuf::from("/home/user/.ssh/known_hosts"))
        );
        assert_eq!(
            select_known_hosts_path(&ConnectionSecrets::default(), None),
            Err(AppError::SshTunnelFailed)
        );
    }

    #[test]
    fn relative_or_control_character_secret_paths_fail_closed() {
        for path in ["relative/key", "/secret/key\nother"] {
            let secrets = ConnectionSecrets {
                ssh_identity_file: Some(path.into()),
                ssh_known_hosts_file: Some(path.into()),
                ..Default::default()
            };
            assert_eq!(
                select_auth(&ssh_config(SshAuthMethod::PrivateKey), &secrets),
                Err(AppError::SshTunnelFailed)
            );
            assert_eq!(
                select_known_hosts_path(&secrets, Some(Path::new("/home/user"))),
                Err(AppError::SshTunnelFailed)
            );
        }
    }

    #[tokio::test]
    async fn invalid_or_overlong_forward_targets_are_rejected_before_registration() {
        let (transport, backend) = test_backend_transport();
        let overlong = "a".repeat(257);
        for host in [
            " bad",
            "bad host",
            "-leading",
            "bad\nline",
            overlong.as_str(),
        ] {
            assert!(matches!(
                transport.forward(&endpoint(host, 6379)).await,
                Err(AppError::SshTunnelFailed)
            ));
        }
        assert!(matches!(
            transport.forward(&endpoint("redis.internal", 0)).await,
            Err(AppError::SshTunnelFailed)
        ));
        assert!(backend.forwarded_targets().is_empty());
    }

    #[tokio::test]
    async fn one_authenticated_session_forwards_sentinel_seed_and_primary() {
        let (transport, backend) = test_backend_transport();
        let seed = transport
            .forward(&endpoint("sentinel.internal", 26379))
            .await
            .unwrap();
        let primary = transport
            .forward(&endpoint("redis.internal", 6379))
            .await
            .unwrap();
        let seed_endpoint = seed.local_endpoint();
        let primary_endpoint = primary.local_endpoint();
        let seed_socket =
            tokio::net::TcpStream::connect((seed_endpoint.host.as_str(), seed_endpoint.port))
                .await
                .unwrap();
        let primary_socket =
            tokio::net::TcpStream::connect((primary_endpoint.host.as_str(), primary_endpoint.port))
                .await
                .unwrap();
        backend.wait_for_active_proxy_count(2).await;
        assert_ne!(seed.local_endpoint(), primary.local_endpoint());
        assert_eq!(backend.authenticated_session_count(), 1);
        let forwarded_targets = backend.forwarded_targets();
        assert_eq!(forwarded_targets.len(), 2);
        assert!(forwarded_targets.contains(&endpoint("sentinel.internal", 26379)));
        assert!(forwarded_targets.contains(&endpoint("redis.internal", 6379)));
        drop(seed_socket);
        drop(primary_socket);
    }

    #[tokio::test]
    async fn idle_forwards_do_not_consume_proxy_worker_capacity() {
        let (transport, backend) = test_backend_transport();
        let mut forwards = Vec::new();
        for index in 0..=MAX_PROXY_WORKERS {
            forwards.push(
                transport
                    .forward(&endpoint(&format!("redis-{index}.internal"), 6379))
                    .await
                    .unwrap(),
            );
        }
        tokio::task::yield_now().await;
        let local_endpoint = forwards.last().unwrap().local_endpoint();
        let socket =
            tokio::net::TcpStream::connect((local_endpoint.host.as_str(), local_endpoint.port))
                .await
                .unwrap();
        backend.wait_for_active_proxy_count(1).await;
        assert_eq!(
            backend.forwarded_targets(),
            vec![endpoint("redis-32.internal", 6379)]
        );
        drop(socket);
    }

    #[tokio::test]
    async fn saturated_proxy_capacity_closes_newly_accepted_socket() {
        let backend = Arc::new(TestBackend::new());
        authenticate_test_backend(&backend);
        let transport = SshTransport::from_backend_with_proxy_limit(backend.clone(), 1);
        let forward = transport
            .forward(&endpoint("redis.internal", 6379))
            .await
            .unwrap();
        let endpoint = forward.local_endpoint();
        let first = tokio::net::TcpStream::connect((endpoint.host.as_str(), endpoint.port))
            .await
            .unwrap();
        backend.wait_for_active_proxy_count(1).await;
        let mut rejected = tokio::net::TcpStream::connect((endpoint.host.as_str(), endpoint.port))
            .await
            .unwrap();
        use tokio::io::AsyncReadExt;
        let mut byte = [0_u8; 1];
        let read = tokio::time::timeout(Duration::from_secs(1), rejected.read(&mut byte))
            .await
            .expect("saturated accepted socket must be closed promptly")
            .unwrap();
        assert_eq!(read, 0);
        assert_eq!(backend.active_proxies.load(Ordering::SeqCst), 1);
        drop(first);
    }

    #[tokio::test]
    async fn dropping_forward_closes_the_app_owned_listener() {
        let (transport, _) = test_backend_transport();
        let forward = transport
            .forward(&endpoint("redis.internal", 6379))
            .await
            .unwrap();
        let endpoint = forward.local_endpoint();
        drop(forward);
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if tokio::net::TcpStream::connect((endpoint.host.as_str(), endpoint.port))
                    .await
                    .is_err()
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("dropping a forward must close its listener");
    }

    #[tokio::test]
    async fn dropping_forward_closes_active_local_proxy_socket() {
        let (transport, backend) = test_backend_transport();
        let forward = transport
            .forward(&endpoint("redis.internal", 6379))
            .await
            .unwrap();
        let endpoint = forward.local_endpoint();
        let mut socket = tokio::net::TcpStream::connect((endpoint.host.as_str(), endpoint.port))
            .await
            .unwrap();
        backend.wait_for_active_proxy().await;
        drop(forward);
        backend.wait_for_channel_drop().await;
        assert!(!backend.disconnected.load(Ordering::SeqCst));

        use tokio::io::AsyncReadExt;
        let mut byte = [0_u8; 1];
        let result = tokio::time::timeout(Duration::from_secs(1), socket.read(&mut byte))
            .await
            .expect("dropping a forward must promptly close active local sockets")
            .unwrap();
        assert_eq!(result, 0);
    }

    #[tokio::test]
    async fn session_disconnects_only_after_transport_and_forwards_are_released() {
        let (transport, backend) = test_backend_transport();
        let forward = transport
            .forward(&endpoint("redis.internal", 6379))
            .await
            .unwrap();
        drop(transport);
        assert!(!backend.disconnected.load(Ordering::SeqCst));
        drop(forward);
        assert!(backend.disconnected.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn final_public_owner_disconnects_while_open_channel_worker_is_blocked() {
        let backend = Arc::new(TestBackend::new_blocked());
        authenticate_test_backend(&backend);
        let transport = SshTransport::from_backend(backend.clone());
        let forward = transport
            .forward(&endpoint("redis.internal", 6379))
            .await
            .unwrap();
        let endpoint = forward.local_endpoint();
        let mut socket = tokio::net::TcpStream::connect((endpoint.host.as_str(), endpoint.port))
            .await
            .unwrap();
        backend.wait_for_open_started().await;

        drop(transport);
        assert!(!backend.disconnected.load(Ordering::SeqCst));
        drop(forward);
        assert!(backend.disconnected.load(Ordering::SeqCst));
        backend.wait_for_open_cancelled().await;

        use tokio::io::AsyncReadExt;
        let mut byte = [0_u8; 1];
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), socket.read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0
        );
    }

    struct TestBackend {
        authenticated: AtomicUsize,
        active_proxies: AtomicUsize,
        forwarded: Mutex<Vec<ConnectionEndpoint>>,
        disconnected: AtomicBool,
        block_open: AtomicBool,
        open_started: AtomicBool,
        open_cancelled: AtomicBool,
        channel_drops: Arc<AtomicUsize>,
    }

    impl TestBackend {
        fn authenticated_session_count(&self) -> usize {
            self.authenticated.load(Ordering::SeqCst)
        }

        fn forwarded_targets(&self) -> Vec<ConnectionEndpoint> {
            self.forwarded.lock().unwrap().clone()
        }

        fn new() -> Self {
            Self {
                authenticated: AtomicUsize::new(0),
                active_proxies: AtomicUsize::new(0),
                forwarded: Mutex::new(Vec::new()),
                disconnected: AtomicBool::new(false),
                block_open: AtomicBool::new(false),
                open_started: AtomicBool::new(false),
                open_cancelled: AtomicBool::new(false),
                channel_drops: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn new_blocked() -> Self {
            let backend = Self::new();
            backend.block_open.store(true, Ordering::SeqCst);
            backend
        }

        async fn wait_for_active_proxy_count(&self, count: usize) {
            tokio::time::timeout(Duration::from_secs(1), async {
                while self.active_proxies.load(Ordering::SeqCst) < count {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("test backend proxy worker did not start");
        }

        async fn wait_for_active_proxy(&self) {
            self.wait_for_active_proxy_count(1).await;
        }

        async fn wait_for_open_started(&self) {
            tokio::time::timeout(Duration::from_secs(1), async {
                while !self.open_started.load(Ordering::SeqCst) {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("blocked open_channel did not start");
        }

        async fn wait_for_open_cancelled(&self) {
            tokio::time::timeout(Duration::from_secs(1), async {
                while !self.open_cancelled.load(Ordering::SeqCst) {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("blocked open_channel did not observe cancellation");
        }

        async fn wait_for_channel_drop(&self) {
            tokio::time::timeout(Duration::from_secs(1), async {
                while self.channel_drops.load(Ordering::SeqCst) == 0 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("active proxy channel was not dropped after cancellation");
        }
    }

    impl SessionBackend for TestBackend {
        fn authenticate(
            &self,
            _config: &SshConfig,
            _secrets: &ConnectionSecrets,
        ) -> Result<(), AppError> {
            self.authenticated.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn open_channel(
            &self,
            target: &ConnectionEndpoint,
            cancelled: &AtomicBool,
        ) -> Result<Box<dyn ProxyChannel>, AppError> {
            self.open_started.store(true, Ordering::SeqCst);
            while self.block_open.load(Ordering::SeqCst) {
                if cancelled.load(Ordering::SeqCst) {
                    self.open_cancelled.store(true, Ordering::SeqCst);
                    return Err(AppError::SshTunnelFailed);
                }
                std::thread::sleep(PROXY_IDLE_WAIT);
            }
            self.forwarded.lock().unwrap().push(target.clone());
            self.active_proxies.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(TestChannel {
                drops: self.channel_drops.clone(),
            }))
        }

        fn disconnect(&self) {
            self.disconnected.store(true, Ordering::SeqCst);
        }
    }

    struct TestChannel {
        drops: Arc<AtomicUsize>,
    }

    impl Drop for TestChannel {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl Read for TestChannel {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::ErrorKind::WouldBlock.into())
        }
    }

    impl Write for TestChannel {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl ProxyChannel for TestChannel {
        fn eof(&self) -> bool {
            false
        }

        fn send_eof(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn test_backend_transport() -> (SshTransport, Arc<TestBackend>) {
        let backend = Arc::new(TestBackend::new());
        authenticate_test_backend(&backend);
        let transport = SshTransport::from_backend(backend.clone());
        (transport, backend)
    }

    fn authenticate_test_backend(backend: &Arc<TestBackend>) {
        authenticate_backend(
            backend.as_ref(),
            &ssh_config(SshAuthMethod::Agent),
            &ConnectionSecrets::default(),
        )
        .unwrap();
    }
}
