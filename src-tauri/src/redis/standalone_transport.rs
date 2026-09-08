use std::{fmt, io::Cursor, pin::Pin, sync::Arc, time::Duration};

use futures_util::StreamExt;
use redis::aio::ConnectionLike;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    domain::ConnectionEndpoint,
    error::{AppError, ConnectionFailureReason},
};

use super::{
    monitor_transport::{self, MonitorLineStream},
    ssh::{SshForward, SshTransport},
};

pub(crate) trait RedisStream: AsyncRead + AsyncWrite {}
impl<T> RedisStream for T where T: AsyncRead + AsyncWrite {}
pub(crate) type BoxedRedisStream = Pin<Box<dyn RedisStream + Send + Sync>>;

#[derive(Clone)]
pub struct ManagedMultiplexedConnection {
    connection: redis::aio::MultiplexedConnection,
    _driver: Option<Arc<DriverGuard>>,
}

struct DriverGuard(tokio::task::AbortHandle);

impl Drop for DriverGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl ManagedMultiplexedConnection {
    fn direct(connection: redis::aio::MultiplexedConnection) -> Self {
        Self {
            connection,
            _driver: None,
        }
    }

    fn custom(
        connection: redis::aio::MultiplexedConnection,
        driver: tokio::task::JoinHandle<()>,
    ) -> Self {
        Self {
            connection,
            _driver: Some(Arc::new(DriverGuard(driver.abort_handle()))),
        }
    }

    pub fn set_response_timeout(&mut self, timeout: Duration) {
        self.connection.set_response_timeout(timeout);
    }
}

impl ConnectionLike for ManagedMultiplexedConnection {
    fn req_packed_command<'a>(
        &'a mut self,
        command: &'a redis::Cmd,
    ) -> redis::RedisFuture<'a, redis::Value> {
        self.connection.req_packed_command(command)
    }

    fn req_packed_commands<'a>(
        &'a mut self,
        pipeline: &'a redis::Pipeline,
        offset: usize,
        count: usize,
    ) -> redis::RedisFuture<'a, Vec<redis::Value>> {
        self.connection.req_packed_commands(pipeline, offset, count)
    }

    fn get_db(&self) -> i64 {
        self.connection.get_db()
    }
}

#[derive(Clone)]
pub struct TlsClientMaterial {
    pub root_cert: Option<Vec<u8>>,
    pub client_cert: Option<Vec<u8>>,
    pub client_key: Option<Vec<u8>>,
    pub verify_server_cert: bool,
}

impl Default for TlsClientMaterial {
    fn default() -> Self {
        Self {
            root_cert: None,
            client_cert: None,
            client_key: None,
            verify_server_cert: true,
        }
    }
}

impl TlsClientMaterial {
    pub(crate) fn redis_certificates(&self) -> Result<redis::TlsCertificates, AppError> {
        let client_tls = match (&self.client_cert, &self.client_key) {
            (Some(client_cert), Some(client_key)) => Some(redis::ClientTlsConfig {
                client_cert: client_cert.clone(),
                client_key: client_key.clone(),
            }),
            (None, None) => None,
            _ => return Err(AppError::InvalidInput),
        };
        Ok(redis::TlsCertificates {
            client_tls,
            root_cert: self.root_cert.clone(),
        })
    }
}

#[derive(Clone)]
pub enum StandaloneClient {
    Direct(redis::Client),
    Tunneled(TunneledClient),
}

#[derive(Clone)]
pub struct TunneledClient {
    pub redis_info: redis::RedisConnectionInfo,
    pub original_endpoint: ConnectionEndpoint,
    pub local_endpoint: ConnectionEndpoint,
    pub tls: Option<TlsClientMaterial>,
    _forward: TunnelLifetime,
}

#[allow(dead_code)]
#[derive(Clone)]
enum TunnelLifetime {
    Ssh(Arc<SshForward>),
    Guard(Arc<dyn Send + Sync>),
}

impl TunneledClient {
    pub fn new<G>(
        redis_info: redis::RedisConnectionInfo,
        original_endpoint: ConnectionEndpoint,
        local_endpoint: ConnectionEndpoint,
        tls: Option<TlsClientMaterial>,
        forward: Arc<G>,
    ) -> Self
    where
        G: Send + Sync + 'static,
    {
        Self {
            redis_info,
            original_endpoint,
            local_endpoint,
            tls,
            _forward: TunnelLifetime::Guard(forward),
        }
    }

    #[allow(dead_code)]
    pub(super) async fn from_ssh_transport(
        redis_info: redis::RedisConnectionInfo,
        original_endpoint: ConnectionEndpoint,
        tls: Option<TlsClientMaterial>,
        transport: &SshTransport,
    ) -> Result<Self, AppError> {
        let forward = transport.forward(&original_endpoint).await?;
        Ok(Self::from_ssh_forward(
            redis_info,
            original_endpoint,
            tls,
            forward,
        ))
    }

    #[allow(dead_code)]
    pub(super) fn from_ssh_forward(
        redis_info: redis::RedisConnectionInfo,
        original_endpoint: ConnectionEndpoint,
        tls: Option<TlsClientMaterial>,
        forward: Arc<SshForward>,
    ) -> Self {
        Self {
            redis_info,
            local_endpoint: forward.local_endpoint(),
            original_endpoint,
            tls,
            _forward: TunnelLifetime::Ssh(forward),
        }
    }

    async fn open_stream(&self) -> Result<BoxedRedisStream, AppError> {
        let stream = tokio::time::timeout(
            Duration::from_secs(3),
            tokio::net::TcpStream::connect((
                self.local_endpoint.host.as_str(),
                self.local_endpoint.port,
            )),
        )
        .await
        .map_err(|_| AppError::ConnectionDiagnostic(ConnectionFailureReason::Timeout))?
        .map_err(|error| map_connection_error(error.into()))?;
        let Some(tls) = &self.tls else {
            return Ok(Box::pin(stream));
        };
        let server_name =
            rustls::pki_types::ServerName::try_from(self.original_endpoint.host.clone())
                .map_err(|_| AppError::InvalidConnection)?;
        let connector = tokio_rustls::TlsConnector::from(Arc::new(build_tls_config(tls)?));
        let stream = tokio::time::timeout(
            Duration::from_secs(3),
            connector.connect(server_name, stream),
        )
        .await
        .map_err(|_| AppError::ConnectionDiagnostic(ConnectionFailureReason::TlsTimeout))?
        .map_err(|error| {
            let mapped = map_connection_error(error.into());
            if mapped.diagnostics().is_some() {
                mapped
            } else {
                AppError::ConnectionDiagnostic(ConnectionFailureReason::TlsHandshake)
            }
        })?;
        Ok(Box::pin(stream))
    }

    pub async fn connection(&self) -> Result<ManagedMultiplexedConnection, AppError> {
        let stream = self.open_stream().await?;
        let (connection, driver) = tokio::time::timeout(
            Duration::from_secs(3),
            redis::aio::MultiplexedConnection::new_with_config(
                &self.redis_info,
                stream,
                redis::AsyncConnectionConfig::new()
                    .set_connection_timeout(Some(Duration::from_secs(3)))
                    .set_response_timeout(Some(Duration::from_secs(3))),
            ),
        )
        .await
        .map_err(|_| AppError::ConnectionDiagnostic(ConnectionFailureReason::Timeout))?
        .map_err(map_connection_error)?;
        let driver = tokio::spawn(driver);
        Ok(ManagedMultiplexedConnection::custom(connection, driver))
    }

    pub async fn pubsub(&self) -> Result<redis::aio::PubSub, AppError> {
        tokio::time::timeout(
            Duration::from_secs(3),
            redis::aio::PubSub::new(&self.redis_info, self.open_stream().await?),
        )
        .await
        .map_err(|_| AppError::ConnectionFailed)?
        .map_err(map_connection_error)
    }

    pub async fn monitor_stream(&self) -> Result<MonitorLineStream, AppError> {
        tokio::time::timeout(
            Duration::from_secs(3),
            monitor_transport::monitor_stream(self.open_stream().await?, &self.redis_info),
        )
        .await
        .map_err(|_| AppError::ConnectionFailed)?
    }
}

fn build_tls_config(material: &TlsClientMaterial) -> Result<rustls::ClientConfig, AppError> {
    let mut roots = rustls::RootCertStore::empty();
    if material.verify_server_cert {
        if let Some(root_cert) = &material.root_cert {
            let certificates = rustls_pemfile::certs(&mut Cursor::new(root_cert))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| AppError::InvalidInput)?;
            if certificates.is_empty() {
                return Err(AppError::InvalidInput);
            }
            for certificate in certificates {
                roots.add(certificate).map_err(|_| AppError::InvalidInput)?;
            }
        } else {
            let native = rustls_native_certs::load_native_certs();
            for certificate in native.certs {
                roots.add(certificate).map_err(|_| AppError::InvalidInput)?;
            }
            if roots.is_empty() {
                return Err(AppError::ConnectionFailed);
            }
        }
    }

    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let builder = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|_| AppError::ConnectionFailed)?
        .with_root_certificates(roots);
    let mut config = match (&material.client_cert, &material.client_key) {
        (Some(certificate), Some(key)) => {
            let certificate = rustls_pemfile::certs(&mut Cursor::new(certificate))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| AppError::InvalidInput)?;
            let key = rustls_pemfile::private_key(&mut Cursor::new(key))
                .map_err(|_| AppError::InvalidInput)?
                .ok_or(AppError::InvalidInput)?;
            builder
                .with_client_auth_cert(certificate, key)
                .map_err(|_| AppError::InvalidInput)?
        }
        (None, None) => builder.with_no_client_auth(),
        _ => return Err(AppError::InvalidInput),
    };
    if !material.verify_server_cert {
        // Insecure mode disables trust-chain and server-name verification only. The verifier
        // still checks CertificateVerify with the certificate public key below.
        config
            .dangerous()
            .set_certificate_verifier(Arc::new(InsecureServerCertVerifier {
                supported: provider.signature_verification_algorithms,
            }));
    }
    Ok(config)
}

struct InsecureServerCertVerifier {
    supported: rustls::crypto::WebPkiSupportedAlgorithms,
}

impl fmt::Debug for InsecureServerCertVerifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InsecureServerCertVerifier")
            .finish()
    }
}

impl rustls::client::danger::ServerCertVerifier for InsecureServerCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &rustls::pki_types::CertificateDer<'_>,
        signed: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, certificate, signed, &self.supported)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &rustls::pki_types::CertificateDer<'_>,
        signed: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, certificate, signed, &self.supported)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.supported.supported_schemes()
    }
}

impl StandaloneClient {
    pub async fn connection(&self) -> Result<ManagedMultiplexedConnection, AppError> {
        match self {
            Self::Direct(client) => client
                .get_multiplexed_async_connection_with_config(
                    &redis::AsyncConnectionConfig::new()
                        .set_connection_timeout(Some(Duration::from_secs(3)))
                        .set_response_timeout(Some(Duration::from_secs(3))),
                )
                .await
                .map(ManagedMultiplexedConnection::direct)
                .map_err(map_connection_error),
            Self::Tunneled(client) => client.connection().await,
        }
    }

    pub async fn pubsub(&self) -> Result<redis::aio::PubSub, AppError> {
        match self {
            Self::Direct(client) => {
                tokio::time::timeout(Duration::from_secs(3), client.get_async_pubsub())
                    .await
                    .map_err(|_| AppError::ConnectionFailed)?
                    .map_err(map_connection_error)
            }
            Self::Tunneled(client) => client.pubsub().await,
        }
    }

    pub async fn monitor_stream(&self) -> Result<MonitorLineStream, AppError> {
        match self {
            Self::Direct(client) => {
                let monitor =
                    tokio::time::timeout(Duration::from_secs(3), client.get_async_monitor())
                        .await
                        .map_err(|_| AppError::ConnectionFailed)?
                        .map_err(map_connection_error)?;
                Ok(Box::pin(
                    monitor.into_on_message::<String>().map(Ok::<_, AppError>),
                ))
            }
            Self::Tunneled(client) => client.monitor_stream().await,
        }
    }
}

fn transport_failure_reason(
    error: &(dyn std::error::Error + 'static),
) -> Option<ConnectionFailureReason> {
    let mut source = Some(error);
    while let Some(cause) = source {
        // redis-rs wraps transport errors in Arc before exposing the source.
        if let Some(wrapped) = cause.downcast_ref::<Arc<dyn std::error::Error + Send + Sync>>() {
            source = Some(wrapped.as_ref());
            continue;
        }
        if let Some(tls) = cause.downcast_ref::<rustls::Error>() {
            return Some(match tls {
                rustls::Error::InvalidCertificate(_) => ConnectionFailureReason::TlsCertificate,
                _ => ConnectionFailureReason::TlsHandshake,
            });
        }
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            if matches!(
                io.kind(),
                std::io::ErrorKind::NetworkUnreachable
                    | std::io::ErrorKind::HostUnreachable
                    | std::io::ErrorKind::NetworkDown
            ) {
                return Some(ConnectionFailureReason::NetworkUnreachable);
            }
            if let Some(inner) = io.get_ref() {
                source = Some(inner);
                continue;
            }
        }
        source = cause.source();
    }
    None
}

pub(crate) fn map_connection_error(error: redis::RedisError) -> AppError {
    use redis::{ErrorKind, ServerErrorKind};

    if error.kind() == ErrorKind::AuthenticationFailed
        || matches!(error.code(), Some("NOAUTH" | "WRONGPASS"))
    {
        return AppError::AuthenticationFailed;
    }
    // redis-rs flattens the first cluster seed failure into this specific wrapper.
    // Recover only known categories; the wrapped text must never enter IPC output.
    if error.kind() == ErrorKind::Io
        && error
            .to_string()
            .starts_with("Failed to create initial connections - Io")
    {
        if let Some(detail) = error.detail().map(str::to_ascii_lowercase) {
            if detail.starts_with("password authentication failed - authenticationfailed")
                || detail.starts_with("\"wrongpass\"")
                || detail.starts_with("\"noauth\"")
            {
                return AppError::AuthenticationFailed;
            }
            let reason = if detail.contains("connection refused") {
                Some(ConnectionFailureReason::Refused)
            } else if detail.contains("timed out") || detail.contains("deadline has elapsed") {
                Some(ConnectionFailureReason::Timeout)
            } else if detail.contains("network is unreachable")
                || detail.contains("host is unreachable")
                || detail.contains("no route to host")
            {
                Some(ConnectionFailureReason::NetworkUnreachable)
            } else if detail.contains("received corrupt message") {
                Some(ConnectionFailureReason::TlsHandshake)
            } else if [
                "connection reset",
                "connection aborted",
                "broken pipe",
                "unexpected eof",
            ]
            .iter()
            .any(|pattern| detail.contains(pattern))
            {
                Some(ConnectionFailureReason::Closed)
            } else {
                None
            };
            if let Some(reason) = reason {
                return AppError::ConnectionDiagnostic(reason);
            }
        }
    }
    let reason = if error.is_connection_refusal() {
        ConnectionFailureReason::Refused
    } else if error.is_timeout() {
        ConnectionFailureReason::Timeout
    } else if let Some(reason) = transport_failure_reason(&error) {
        reason
    } else if matches!(error.kind(), ErrorKind::Io | ErrorKind::Client) {
        // Only classify known transport messages. Never return this text: it can contain
        // hostnames, credentials or details supplied by a remote server.
        let description = error.to_string().to_ascii_lowercase();
        if description.contains("certificate") {
            ConnectionFailureReason::TlsCertificate
        } else if description.contains("tls") || description.contains("handshake") {
            ConnectionFailureReason::TlsHandshake
        } else if [
            "failed to lookup address",
            "name or service not known",
            "nodename nor servname",
            "no such host",
            "temporary failure in name resolution",
        ]
        .iter()
        .any(|pattern| description.contains(pattern))
        {
            ConnectionFailureReason::DnsResolution
        } else if error.is_connection_dropped() && std::error::Error::source(&error).is_some() {
            ConnectionFailureReason::Closed
        } else {
            return AppError::ConnectionFailed;
        }
    } else {
        match error.kind() {
            ErrorKind::Parse | ErrorKind::UnexpectedReturnType | ErrorKind::RESP3NotSupported => {
                ConnectionFailureReason::Protocol
            }
            ErrorKind::Server(ServerErrorKind::NoPerm) => ConnectionFailureReason::PermissionDenied,
            ErrorKind::Server(
                ServerErrorKind::BusyLoading
                | ServerErrorKind::ClusterDown
                | ServerErrorKind::MasterDown,
            ) => ConnectionFailureReason::ServerUnavailable,
            _ => return AppError::ConnectionFailed,
        }
    };
    AppError::ConnectionDiagnostic(reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_errors_include_safe_specific_diagnostics() {
        let cases = [
            (
                redis::RedisError::from(std::io::Error::new(
                    std::io::ErrorKind::ConnectionRefused,
                    "private redis://operator:redis-secret@host",
                )),
                "连接被拒绝",
            ),
            (
                redis::RedisError::from(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "private timeout detail",
                )),
                "超时",
            ),
            (
                redis::RedisError::from(std::io::Error::new(
                    std::io::ErrorKind::ConnectionReset,
                    "private reset detail",
                )),
                "连接已断开",
            ),
            (
                redis::RedisError::from(std::io::Error::other(
                    "failed to lookup address information: private hostname",
                )),
                "DNS",
            ),
            (
                redis::RedisError::from((
                    redis::ErrorKind::AuthenticationFailed,
                    "private auth detail",
                    "redis-secret".into(),
                )),
                "用户名和密码",
            ),
            (
                redis::RedisError::from(rustls::Error::InvalidCertificate(
                    rustls::CertificateError::UnknownIssuer,
                )),
                "证书",
            ),
            (
                redis::RedisError::from((redis::ErrorKind::Parse, "private response detail")),
                "Redis 协议",
            ),
            (
                redis::RedisError::from((
                    redis::ErrorKind::Server(redis::ServerErrorKind::NoPerm),
                    "private ACL detail",
                )),
                "权限",
            ),
            (
                redis::RedisError::from(std::io::Error::new(
                    std::io::ErrorKind::NetworkUnreachable,
                    "private network detail",
                )),
                "网络不可达",
            ),
            (
                redis::RedisError::from(std::io::Error::new(
                    std::io::ErrorKind::HostUnreachable,
                    "private host detail",
                )),
                "网络不可达",
            ),
            (
                redis::RedisError::from(rustls::Error::InvalidMessage(
                    rustls::InvalidMessage::InvalidContentType,
                )),
                "TLS 握手",
            ),
        ];
        for (error, expected) in cases {
            let result = serde_json::to_value(map_connection_error(error)).unwrap();
            let diagnostics = result["diagnostics"]
                .as_str()
                .expect("connection error must include diagnostic text");
            assert!(diagnostics.contains(expected), "{result}");
            assert!(!result.to_string().contains("private"), "{result}");
            assert!(!result.to_string().contains("redis-secret"), "{result}");
            assert!(!result.to_string().contains("redis://"), "{result}");
        }
    }

    #[test]
    fn cluster_initial_connection_wrappers_preserve_failure_categories_without_raw_details() {
        let cases = [
            (
                "Connection refused (os error 61)",
                "CONNECTION_FAILED",
                "连接被拒绝",
            ),
            ("timed out", "CONNECTION_FAILED", "超时"),
            (
                "Password authentication failed - AuthenticationFailed: private-secret",
                "AUTHENTICATION_FAILED",
                "用户名和密码",
            ),
            (
                "\"WRONGPASS\": invalid username-password pair: private-secret",
                "AUTHENTICATION_FAILED",
                "用户名和密码",
            ),
        ];
        for (detail, code, expected) in cases {
            let error = redis::RedisError::from((
                redis::ErrorKind::Io,
                "Failed to create initial connections",
                detail.to_owned(),
            ));
            let result = serde_json::to_value(map_connection_error(error)).unwrap();
            assert_eq!(result["code"], code, "{result}");
            assert!(
                result["diagnostics"].as_str().unwrap().contains(expected),
                "{result}"
            );
            assert!(!result.to_string().contains("private-secret"), "{result}");
        }
    }

    #[test]
    fn ssh_forward_constructor_keeps_the_exact_arc_lifetime_type_private_to_redis() {
        let _: fn(
            redis::RedisConnectionInfo,
            ConnectionEndpoint,
            Option<TlsClientMaterial>,
            Arc<SshForward>,
        ) -> TunneledClient = TunneledClient::from_ssh_forward;
    }
}
