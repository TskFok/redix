use std::{
    collections::HashMap,
    io::{self, Read, Write},
    net::{Shutdown, TcpStream, ToSocketAddrs},
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
    fn close(&mut self);
}

impl ProxyChannel for ssh2::Channel {
    fn eof(&self) -> bool {
        ssh2::Channel::eof(self)
    }

    fn send_eof(&mut self) -> io::Result<()> {
        ssh2::Channel::send_eof(self).map_err(io::Error::from)
    }

    fn close(&mut self) {
        let _ = ssh2::Channel::close(self);
    }
}

trait SessionBackend: Send + Sync {
    fn register_forward(&self, _target: &ConnectionEndpoint) {}
    fn open_channel(&self, target: &ConnectionEndpoint) -> Result<Box<dyn ProxyChannel>, AppError>;
    fn disconnect(&self);
}

struct Libssh2Backend {
    session: ssh2::Session,
    socket: TcpStream,
}

impl SessionBackend for Libssh2Backend {
    fn open_channel(&self, target: &ConnectionEndpoint) -> Result<Box<dyn ProxyChannel>, AppError> {
        let deadline = Instant::now() + SSH_CONNECT_TIMEOUT;
        loop {
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
        let config = config.clone();
        let secrets = secrets.clone();
        let permit = blocking_ssh_limit()
            .acquire_owned()
            .await
            .map_err(|_| AppError::SshTunnelFailed)?;
        let backend = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            connect_blocking(&config, &secrets)
        })
        .await
        .map_err(|_| AppError::SshTunnelFailed)??;
        Ok(Self::from_backend(backend))
    }

    fn from_backend(backend: Arc<dyn SessionBackend>) -> Self {
        Self {
            session: Arc::new(SessionOwner { backend }),
            proxy_limit: Arc::new(tokio::sync::Semaphore::new(MAX_PROXY_WORKERS)),
        }
    }

    pub async fn forward(&self, target: &ConnectionEndpoint) -> Result<Arc<SshForward>, AppError> {
        if target.host.is_empty() || target.port == 0 {
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
        self.session.backend.register_forward(target);
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

fn connect_blocking(
    config: &SshConfig,
    secrets: &ConnectionSecrets,
) -> Result<Arc<dyn SessionBackend>, AppError> {
    let socket = connect_socket(&config.host, config.port)?;
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
    authenticate(&session, config, secrets)?;
    if !session.authenticated() {
        return Err(AppError::SshTunnelFailed);
    }

    control_socket
        .set_nonblocking(true)
        .map_err(|_| AppError::SshTunnelFailed)?;
    session.set_blocking(false);
    Ok(Arc::new(Libssh2Backend {
        session,
        socket: control_socket,
    }))
}

fn connect_socket(host: &str, port: u16) -> Result<TcpStream, AppError> {
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|_| AppError::SshTunnelFailed)?;
    let deadline = Instant::now() + SSH_CONNECT_TIMEOUT;
    for address in addresses.take(8) {
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
    let result = if config.port == 22 {
        known_hosts.check(&config.host, key)
    } else {
        known_hosts.check_port(&config.host, config.port, key)
    };
    verify_host_key(result.into())
}

fn authenticate(
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

fn spawn_accept_task(
    listener: tokio::net::TcpListener,
    session: std::sync::Weak<SessionOwner>,
    proxy_limit: Arc<tokio::sync::Semaphore>,
    target: ConnectionEndpoint,
    state: Arc<ForwardState>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let permit = match proxy_limit.clone().acquire_owned().await {
                Ok(permit) => permit,
                Err(_) => break,
            };
            let (socket, _) = match listener.accept().await {
                Ok(value) => value,
                Err(_) => break,
            };
            let socket = match socket.into_std() {
                Ok(socket) => socket,
                Err(_) => continue,
            };
            let id = match state.register(&socket) {
                Ok(id) => id,
                Err(_) => break,
            };
            let blocking_permit = match blocking_ssh_limit().acquire_owned().await {
                Ok(permit) => permit,
                Err(_) => {
                    state.remove(id);
                    let _ = socket.shutdown(Shutdown::Both);
                    break;
                }
            };
            let Some(worker_session) = session.upgrade() else {
                state.remove(id);
                let _ = socket.shutdown(Shutdown::Both);
                break;
            };
            let worker_target = target.clone();
            let worker_state = state.clone();
            tokio::task::spawn_blocking(move || {
                let _permit = permit;
                let _blocking_permit = blocking_permit;
                if let Ok(channel) = worker_session.backend.open_channel(&worker_target) {
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
    remote.close();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
        assert_ne!(seed.local_endpoint(), primary.local_endpoint());
        assert_eq!(backend.authenticated_session_count(), 1);
        assert_eq!(
            backend.forwarded_targets(),
            vec![
                endpoint("sentinel.internal", 26379),
                endpoint("redis.internal", 6379)
            ]
        );
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
        assert!(
            tokio::net::TcpStream::connect((endpoint.host.as_str(), endpoint.port))
                .await
                .is_err()
        );
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

    struct TestBackend {
        authenticated: AtomicUsize,
        active_proxies: AtomicUsize,
        forwarded: Mutex<Vec<ConnectionEndpoint>>,
        disconnected: AtomicBool,
    }

    impl TestBackend {
        fn authenticated_session_count(&self) -> usize {
            self.authenticated.load(Ordering::SeqCst)
        }

        fn forwarded_targets(&self) -> Vec<ConnectionEndpoint> {
            self.forwarded.lock().unwrap().clone()
        }

        async fn wait_for_active_proxy(&self) {
            tokio::time::timeout(Duration::from_secs(1), async {
                while self.active_proxies.load(Ordering::SeqCst) == 0 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("test backend proxy worker did not start");
        }
    }

    impl SessionBackend for TestBackend {
        fn register_forward(&self, target: &ConnectionEndpoint) {
            self.forwarded.lock().unwrap().push(target.clone());
        }

        fn open_channel(
            &self,
            _target: &ConnectionEndpoint,
        ) -> Result<Box<dyn ProxyChannel>, AppError> {
            self.active_proxies.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(TestChannel))
        }

        fn disconnect(&self) {
            self.disconnected.store(true, Ordering::SeqCst);
        }
    }

    struct TestChannel;

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

        fn close(&mut self) {}
    }

    fn test_backend_transport() -> (SshTransport, Arc<TestBackend>) {
        let backend = Arc::new(TestBackend {
            authenticated: AtomicUsize::new(1),
            active_proxies: AtomicUsize::new(0),
            forwarded: Mutex::new(Vec::new()),
            disconnected: AtomicBool::new(false),
        });
        let transport = SshTransport::from_backend(backend.clone());
        (transport, backend)
    }
}
