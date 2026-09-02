use redis::aio::ConnectionLike;

use crate::{
    domain::{ClusterConfig, ConnectionEndpoint},
    error::AppError,
};

use super::standalone_transport::{map_connection_error, StandaloneClient, TlsClientMaterial};

#[derive(Clone)]
pub enum RoutedClient {
    Standalone(StandaloneClient),
    Cluster(redis::cluster_async::ClusterConnection),
}

impl RoutedClient {
    pub async fn cluster(
        config: &ClusterConfig,
        username: Option<&str>,
        password: Option<&str>,
        tls: Option<TlsClientMaterial>,
    ) -> Result<Self, AppError> {
        if config.nodes.is_empty() || config.nodes.len() > 32 {
            return Err(AppError::InvalidConnection);
        }
        let nodes = config
            .nodes
            .iter()
            .map(|endpoint| cluster_url(endpoint, tls.is_some()))
            .collect::<Result<Vec<_>, _>>()?;
        let mut builder = redis::cluster::ClusterClient::builder(nodes)
            .connection_timeout(std::time::Duration::from_secs(3))
            .response_timeout(std::time::Duration::from_secs(3))
            .retries(3)
            .min_retry_wait(250)
            .max_retry_wait(2_000)
            .database_id(0);
        if let Some(username) = username {
            builder = builder.username(username);
        }
        if let Some(password) = password {
            builder = builder.password(password);
        }
        if let Some(tls) = tls {
            builder = builder
                .tls(if tls.verify_server_cert {
                    redis::TlsMode::Secure
                } else {
                    redis::TlsMode::Insecure
                })
                .danger_accept_invalid_hostnames(!tls.verify_server_cert)
                .certs(tls.redis_certificates()?);
        }
        if config.read_from_replicas {
            #[allow(deprecated)]
            {
                builder = builder.read_from_replicas();
            }
        }
        let client = builder.build().map_err(map_connection_error)?;
        client
            .get_async_connection()
            .await
            .map(Self::Cluster)
            .map_err(map_connection_error)
    }

    pub async fn connection(&self) -> Result<RoutedConnection, AppError> {
        match self {
            Self::Standalone(client) => client.connection().await.map(RoutedConnection::Standalone),
            Self::Cluster(connection) => Ok(RoutedConnection::Cluster(connection.clone())),
        }
    }

    pub fn standalone_client(&self) -> Result<&StandaloneClient, AppError> {
        match self {
            Self::Standalone(client) => Ok(client),
            Self::Cluster(_) => Err(AppError::UnsupportedFeature),
        }
    }

    pub fn topology_kind(&self) -> &'static str {
        match self {
            Self::Standalone(_) => "standalone",
            Self::Cluster(_) => "cluster",
        }
    }
}

fn cluster_url(endpoint: &ConnectionEndpoint, tls: bool) -> Result<String, AppError> {
    if endpoint.port == 0 {
        return Err(AppError::InvalidConnection);
    }
    let host = if endpoint.host.parse::<std::net::Ipv6Addr>().is_ok() {
        format!("[{}]", endpoint.host)
    } else if !endpoint.host.is_empty()
        && !endpoint.host.starts_with('-')
        && endpoint
            .host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        endpoint.host.clone()
    } else {
        return Err(AppError::InvalidConnection);
    };
    Ok(format!(
        "{}://{host}:{}/",
        if tls { "rediss" } else { "redis" },
        endpoint.port
    ))
}

pub enum RoutedConnection {
    Standalone(redis::aio::MultiplexedConnection),
    Cluster(redis::cluster_async::ClusterConnection),
}

impl ConnectionLike for RoutedConnection {
    fn req_packed_command<'a>(
        &'a mut self,
        cmd: &'a redis::Cmd,
    ) -> redis::RedisFuture<'a, redis::Value> {
        match self {
            Self::Standalone(connection) => connection.req_packed_command(cmd),
            Self::Cluster(connection) => connection.req_packed_command(cmd),
        }
    }

    fn req_packed_commands<'a>(
        &'a mut self,
        pipeline: &'a redis::Pipeline,
        offset: usize,
        count: usize,
    ) -> redis::RedisFuture<'a, Vec<redis::Value>> {
        match self {
            Self::Standalone(connection) => connection.req_packed_commands(pipeline, offset, count),
            Self::Cluster(connection) => connection.req_packed_commands(pipeline, offset, count),
        }
    }

    fn get_db(&self) -> i64 {
        match self {
            Self::Standalone(connection) => connection.get_db(),
            Self::Cluster(_) => 0,
        }
    }
}
