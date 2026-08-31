use std::{
    net::TcpListener,
    path::Path,
    process::{Child, Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};

use crate::{
    domain::{ConnectionProfile, SshConfig},
    error::AppError,
};

/// Reaps both the master and short-lived control clients on errors and cancellation.
struct ReapedChild(Mutex<Child>);

impl ReapedChild {
    fn spawn(args: Vec<String>) -> Result<Self, AppError> {
        Command::new("ssh")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|child| Self(Mutex::new(child)))
            .map_err(|_| AppError::SshTunnelFailed)
    }

    fn status(&self) -> Result<Option<std::process::ExitStatus>, AppError> {
        self.0
            .lock()
            .map_err(|_| AppError::SshTunnelFailed)?
            .try_wait()
            .map_err(|_| AppError::SshTunnelFailed)
    }

    async fn wait_success(&self, deadline: Instant) -> Result<(), AppError> {
        loop {
            if let Some(status) = self.status()? {
                return if status.success() {
                    Ok(())
                } else {
                    Err(AppError::SshTunnelFailed)
                };
            }
            if Instant::now() >= deadline {
                return Err(AppError::SshTunnelFailed);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

impl Drop for ReapedChild {
    fn drop(&mut self) {
        let child = self
            .0
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = child.kill();
        let _ = child.wait();
    }
}

// Fields drop in declaration order: terminate the master before removing its socket directory.
struct SshMaster {
    child: ReapedChild,
    directory: tempfile::TempDir,
}

/// Owns the authenticated forwarding process and its private control socket.
pub(super) struct SshTunnel {
    #[cfg(unix)]
    _master: SshMaster,
    pub port: u16,
}

#[derive(Clone, Copy)]
enum SshOperation {
    Master,
    Check,
    Forward(u16),
}

fn ssh_arguments(
    ssh: &SshConfig,
    profile: &ConnectionProfile,
    control_path: &Path,
    operation: SshOperation,
) -> Result<Vec<String>, AppError> {
    profile.validate()?;
    let mut args = vec![
        "-N",
        "-T",
        "-F",
        "/dev/null",
        "-o",
        "BatchMode=yes",
        "-o",
        "StrictHostKeyChecking=yes",
        "-o",
        "ExitOnForwardFailure=yes",
        "-o",
        "ConnectTimeout=5",
        "-o",
        "ServerAliveInterval=15",
        "-o",
        "ServerAliveCountMax=2",
        "-o",
        "ControlPersist=no",
        "-o",
        "UpdateHostKeys=no",
        "-o",
        "PermitLocalCommand=no",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    args.extend([
        "-S".into(),
        control_path
            .to_str()
            .ok_or(AppError::SshTunnelFailed)?
            .to_owned(),
        "-o".into(),
        if matches!(operation, SshOperation::Master) {
            "ControlMaster=yes"
        } else {
            "ControlMaster=no"
        }
        .into(),
    ]);
    match operation {
        SshOperation::Master => {}
        SshOperation::Check => args.extend(["-O".into(), "check".into()]),
        SshOperation::Forward(port) => {
            let target = super::connection_manager::standalone_host(&profile.host)?;
            args.extend([
                "-O".into(),
                "forward".into(),
                "-L".into(),
                format!("127.0.0.1:{port}:{target}:{}", profile.port),
            ]);
        }
    }
    args.extend([
        "-p".into(),
        ssh.port.to_string(),
        "-l".into(),
        ssh.username.clone(),
    ]);
    if let Some(identity) = &ssh.identity_file {
        args.extend(["-i".into(), identity.clone()]);
    }
    if let Some(known_hosts) = &ssh.known_hosts_file {
        // OpenSSH parses -o values itself; quote paths containing spaces and escapes.
        let escaped = known_hosts.replace('\\', "\\\\").replace('"', "\\\"");
        args.extend(["-o".into(), format!("UserKnownHostsFile=\"{escaped}\"")]);
    }
    args.extend(["--".into(), ssh.host.clone()]);
    Ok(args)
}

#[cfg(unix)]
impl SshMaster {
    async fn start(profile: &ConnectionProfile, deadline: Instant) -> Result<Self, AppError> {
        use std::os::unix::fs::PermissionsExt;
        let ssh = profile.ssh.as_ref().ok_or(AppError::InvalidConnection)?;
        profile.validate()?;
        // Set mode atomically at creation, not after an initially public directory exists.
        let directory = tempfile::Builder::new()
            .prefix("rxs-")
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .map_err(|_| AppError::SshTunnelFailed)?;
        let child = ReapedChild::spawn(ssh_arguments(
            ssh,
            profile,
            &directory.path().join("c"),
            SshOperation::Master,
        )?)?;
        let master = Self { child, directory };
        loop {
            if master.child.status()?.is_some() || Instant::now() >= deadline {
                return Err(AppError::SshTunnelFailed);
            }
            // This socket is private and OpenSSH creates it only after authentication.
            // -O explicitly forbids the client's normal fallback to a new connection.
            if master.directory.path().join("c").exists()
                && master
                    .control(profile, SshOperation::Check, deadline)
                    .await
                    .is_ok()
            {
                return Ok(master);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn control(
        &self,
        profile: &ConnectionProfile,
        operation: SshOperation,
        deadline: Instant,
    ) -> Result<(), AppError> {
        let ssh = profile.ssh.as_ref().ok_or(AppError::InvalidConnection)?;
        let command = ReapedChild::spawn(ssh_arguments(
            ssh,
            profile,
            &self.directory.path().join("c"),
            operation,
        )?)?;
        command.wait_success(deadline).await?;
        if self.child.status()?.is_some() {
            return Err(AppError::SshTunnelFailed);
        }
        Ok(())
    }
}

impl SshTunnel {
    pub async fn start(profile: &ConnectionProfile) -> Result<Self, AppError> {
        // Windows OpenSSH does not support these authenticated multiplexing controls.
        // Never substitute an unauthenticated TCP readiness probe or a direct connection.
        #[cfg(not(unix))]
        {
            let _ = profile;
            Err(AppError::SshTunnelFailed)
        }
        #[cfg(unix)]
        {
            let deadline = Instant::now() + Duration::from_secs(6);
            let master = SshMaster::start(profile, deadline).await?;
            let reservation =
                TcpListener::bind(("127.0.0.1", 0)).map_err(|_| AppError::SshTunnelFailed)?;
            let port = reservation
                .local_addr()
                .map_err(|_| AppError::SshTunnelFailed)?
                .port();
            drop(reservation);
            // OpenSSH rejects local port 0 (only remote forwards support dynamic allocation).
            // If this port is stolen, the authenticated master's bind fails and -O fails.
            // TCP connect success is never evidence that this tunnel owns the listener.
            master
                .control(profile, SshOperation::Forward(port), deadline)
                .await?;
            Ok(Self {
                _master: master,
                port,
            })
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::net::TcpStream;

    #[tokio::test]
    #[ignore = "需要 ssh、sshd、ssh-keygen 和本机端口绑定权限"]
    async fn ssh_tunnel_verifies_known_host_and_reaps_forwarding_port_on_drop() {
        use std::io::{Read, Write};
        let directory = tempfile::tempdir().unwrap();
        let host_key = directory.path().join("host_key");
        let identity = directory.path().join("identity");
        for key in [&host_key, &identity] {
            assert!(Command::new("ssh-keygen")
                .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                .arg(key)
                .status()
                .unwrap()
                .success());
        }
        let target = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let target_port = target.local_addr().unwrap().port();
        target.set_nonblocking(true).unwrap();
        let stop_echo = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let echo_stop = stop_echo.clone();
        let echo = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            while !echo_stop.load(std::sync::atomic::Ordering::Relaxed)
                && std::time::Instant::now() < deadline
            {
                if let Ok((mut stream, _)) = target.accept() {
                    let _ = stream.write_all(b"redix");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        let reservation = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let ssh_port = reservation.local_addr().unwrap().port();
        drop(reservation);
        let config = directory.path().join("sshd.conf");
        std::fs::write(&config, format!("Port {ssh_port}\nListenAddress 127.0.0.1\nHostKey {}\nPidFile {}/sshd.pid\nAuthorizedKeysFile {}.pub\nStrictModes no\nPasswordAuthentication no\nKbdInteractiveAuthentication no\nPubkeyAuthentication yes\nUsePAM no\nAllowTcpForwarding yes\n", host_key.display(), directory.path().display(), identity.display())).unwrap();
        let sshd_path = std::env::var("REDIX_TEST_SSHD_BIN").unwrap_or_else(|_| {
            if std::path::Path::new("/opt/homebrew/sbin/sshd").exists() {
                "/opt/homebrew/sbin/sshd".into()
            } else {
                "/usr/sbin/sshd".into()
            }
        });
        let daemon_child = Command::new(sshd_path)
            .args(["-D", "-e", "-f"])
            .arg(config)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let daemon = ReapedChild(Mutex::new(daemon_child));
        let mut ready = false;
        for _ in 0..100 {
            if TcpStream::connect(("127.0.0.1", ssh_port)).is_ok() {
                ready = true;
                break;
            }
            assert!(
                daemon.status().unwrap().is_none(),
                "temporary sshd exited before listening"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(ready);
        let known_hosts = directory.path().join("known_hosts");
        let host_public_key = std::fs::read_to_string(host_key.with_extension("pub")).unwrap();
        std::fs::write(
            &known_hosts,
            format!("[127.0.0.1]:{ssh_port} {host_public_key}"),
        )
        .unwrap();
        let profile: ConnectionProfile = serde_json::from_value(serde_json::json!({"id":"ssh","name":"SSH","host":"127.0.0.1","port":target_port,"username":null,"database":0,"has_password":false,"ssh":{"host":"127.0.0.1","port":ssh_port,"username":std::env::var("USER").unwrap(),"identity_file":identity,"known_hosts_file":known_hosts}})).unwrap();
        // A listener occupying the selected port must never count as SSH readiness.
        let master = SshMaster::start(&profile, Instant::now() + Duration::from_secs(6))
            .await
            .unwrap();
        let private_directory = master.directory.path().to_owned();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&private_directory)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        let occupied = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        occupied.set_nonblocking(true).unwrap();
        assert!(matches!(
            master
                .control(
                    &profile,
                    SshOperation::Forward(occupied.local_addr().unwrap().port()),
                    Instant::now() + Duration::from_secs(2),
                )
                .await,
            Err(AppError::SshTunnelFailed)
        ));
        assert_eq!(
            occupied.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock,
            "an unrelated listener must never be probed or receive application traffic"
        );
        // -O must fail closed when its authenticated socket is gone, even if sshd is reachable.
        std::fs::remove_file(private_directory.join("c")).unwrap();
        assert!(matches!(
            master
                .control(
                    &profile,
                    SshOperation::Check,
                    Instant::now() + Duration::from_secs(2),
                )
                .await,
            Err(AppError::SshTunnelFailed)
        ));
        drop(master);
        assert!(!private_directory.exists());
        drop(occupied);

        let tunnel = SshTunnel::start(&profile).await.unwrap();
        let port = tunnel.port;
        let mut connection = TcpStream::connect(("127.0.0.1", port)).unwrap();
        connection
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut reply = [0; 5];
        connection.read_exact(&mut reply).unwrap();
        assert_eq!(&reply, b"redix");
        drop(connection);
        drop(tunnel);
        assert!(TcpStream::connect(("127.0.0.1", port)).is_err());
        stop_echo.store(true, std::sync::atomic::Ordering::Relaxed);
        echo.join().unwrap();
        let wrong_host_key = std::fs::read_to_string(identity.with_extension("pub")).unwrap();
        std::fs::write(
            &known_hosts,
            format!("[127.0.0.1]:{ssh_port} {wrong_host_key}"),
        )
        .unwrap();
        assert!(matches!(
            SshTunnel::start(&profile).await,
            Err(AppError::SshTunnelFailed)
        ));
        std::fs::write(known_hosts, "").unwrap();
        assert!(matches!(
            SshTunnel::start(&profile).await,
            Err(AppError::SshTunnelFailed)
        ));
        drop(daemon);
    }

    #[tokio::test]
    #[ignore = "需要 ssh 和本机 Unix socket 绑定权限"]
    async fn stalled_control_client_is_reaped_after_timeout_and_cancellation() {
        use std::os::unix::net::UnixListener;
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("blocked");
        let listener = UnixListener::bind(&socket).unwrap();
        let profile: ConnectionProfile = serde_json::from_value(serde_json::json!({"id":"ssh","name":"SSH","host":"127.0.0.1","port":6379,"username":null,"database":0,"has_password":false,"ssh":{"host":"127.0.0.1","port":22,"username":"operator"}})).unwrap();
        let args = ssh_arguments(
            profile.ssh.as_ref().unwrap(),
            &profile,
            &socket,
            SshOperation::Check,
        )
        .unwrap();
        let child = ReapedChild::spawn(args.clone()).unwrap();
        let pid = child.0.lock().unwrap().id();
        assert!(matches!(
            child
                .wait_success(Instant::now() + Duration::from_millis(100))
                .await,
            Err(AppError::SshTunnelFailed)
        ));
        assert!(
            child.status().unwrap().is_none(),
            "fixture must leave the control client blocked"
        );
        drop(child);
        assert_process_reaped(pid);

        let (started, ready) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let child = ReapedChild::spawn(args).unwrap();
            started.send(child.0.lock().unwrap().id()).unwrap();
            child
                .wait_success(Instant::now() + Duration::from_secs(60))
                .await
        });
        let pid = ready.await.unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert_process_reaped(pid);
        drop(listener);
    }

    fn assert_process_reaped(pid: u32) {
        let output = Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "pid="])
            .output()
            .unwrap();
        assert!(output.stdout.is_empty(), "SSH child {pid} must be reaped");
    }

    #[test]
    fn ssh_argv_is_strict_and_keeps_identity_path_as_one_literal_argument() {
        let profile: ConnectionProfile = serde_json::from_value(serde_json::json!({"id":"ssh","name":"SSH","host":"::1","port":6379,"username":null,"database":0,"has_password":false,"ssh":{"host":"jump.example","port":2222,"username":"operator","identity_file":"/tmp/key $(touch no); quote' file"}})).unwrap();
        let args = ssh_arguments(
            profile.ssh.as_ref().unwrap(),
            &profile,
            Path::new("/tmp/private/c"),
            SshOperation::Master,
        )
        .unwrap();
        assert!(
            args.windows(2)
                .any(|values| values == ["-o", "ControlMaster=yes"]),
            "SSH must establish an authenticated private master before forwarding"
        );
        assert!(
            !args.iter().any(|arg| arg == "-L"),
            "the authentication stage must not create a local forward"
        );
        assert!(args
            .windows(2)
            .any(|values| values == ["-i", "/tmp/key $(touch no); quote' file"]));
        assert!(args
            .windows(2)
            .any(|values| values == ["-o", "StrictHostKeyChecking=yes"]));
        assert!(args
            .windows(2)
            .any(|values| values == ["-o", "BatchMode=yes"]));
        assert_eq!(&args[args.len() - 2..], ["--", "jump.example"]);
        let forward = ssh_arguments(
            profile.ssh.as_ref().unwrap(),
            &profile,
            Path::new("/tmp/private/c"),
            SshOperation::Forward(49152),
        )
        .unwrap();
        assert!(forward.windows(2).any(|values| values == ["-O", "forward"]));
        assert!(forward
            .windows(2)
            .any(|values| values == ["-L", "127.0.0.1:49152:[::1]:6379"]));
        assert!(forward
            .windows(2)
            .any(|values| values == ["-S", "/tmp/private/c"]));
    }
}
