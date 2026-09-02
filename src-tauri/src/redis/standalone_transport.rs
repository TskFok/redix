use std::{fmt, io::Cursor, pin::Pin, sync::Arc, time::Duration};

use futures_util::StreamExt;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::{domain::ConnectionEndpoint, error::AppError};

use super::{
    monitor_transport::{self, MonitorLineStream},
    ssh::{SshForward, SshTransport},
};

pub(crate) trait RedisStream: AsyncRead + AsyncWrite {}
impl<T> RedisStream for T where T: AsyncRead + AsyncWrite {}
pub(crate) type BoxedRedisStream = Pin<Box<dyn RedisStream + Send + Sync>>;

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
        .map_err(|_| AppError::ConnectionFailed)?
        .map_err(|_| AppError::ConnectionFailed)?;
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
        .map_err(|_| AppError::ConnectionFailed)?
        .map_err(|_| AppError::ConnectionFailed)?;
        Ok(Box::pin(stream))
    }

    pub async fn connection(&self) -> Result<redis::aio::MultiplexedConnection, AppError> {
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
        .map_err(|_| AppError::ConnectionFailed)?
        .map_err(map_connection_error)?;
        tokio::spawn(driver);
        Ok(connection)
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

    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let verifier_roots = Arc::new(roots.clone());
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
        let inner =
            rustls::client::WebPkiServerVerifier::builder_with_provider(verifier_roots, provider)
                .build()
                .map_err(|_| AppError::InvalidInput)?;
        config
            .dangerous()
            .set_certificate_verifier(Arc::new(AcceptInvalidHostnamesVerifier { inner }));
    }
    Ok(config)
}

struct AcceptInvalidHostnamesVerifier {
    inner: Arc<rustls::client::WebPkiServerVerifier>,
}

impl fmt::Debug for AcceptInvalidHostnamesVerifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AcceptInvalidHostnamesVerifier")
            .finish()
    }
}

impl rustls::client::danger::ServerCertVerifier for AcceptInvalidHostnamesVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
        intermediates: &[rustls::pki_types::CertificateDer<'_>],
        server_name: &rustls::pki_types::ServerName<'_>,
        ocsp_response: &[u8],
        now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        self.inner
            .verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now)
            .or_else(|error| match error {
                rustls::Error::InvalidCertificate(
                    rustls::CertificateError::NotValidForName
                    | rustls::CertificateError::NotValidForNameContext { .. },
                ) => Ok(rustls::client::danger::ServerCertVerified::assertion()),
                error => Err(error),
            })
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &rustls::pki_types::CertificateDer<'_>,
        signed: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        self.inner
            .verify_tls12_signature(message, certificate, signed)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &rustls::pki_types::CertificateDer<'_>,
        signed: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        self.inner
            .verify_tls13_signature(message, certificate, signed)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

impl StandaloneClient {
    pub async fn connection(&self) -> Result<redis::aio::MultiplexedConnection, AppError> {
        match self {
            Self::Direct(client) => client
                .get_multiplexed_async_connection_with_config(
                    &redis::AsyncConnectionConfig::new()
                        .set_connection_timeout(Some(Duration::from_secs(3)))
                        .set_response_timeout(Some(Duration::from_secs(3))),
                )
                .await
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

pub(crate) fn map_connection_error(error: redis::RedisError) -> AppError {
    if error.kind() == redis::ErrorKind::AuthenticationFailed {
        AppError::AuthenticationFailed
    } else {
        AppError::ConnectionFailed
    }
}
