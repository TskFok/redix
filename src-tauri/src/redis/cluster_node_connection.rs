use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};

use crate::{domain::ConnectionEndpoint, error::AppError};

use super::standalone_transport::TlsClientMaterial;

#[derive(Clone)]
pub struct ClusterNodeConnectionFactory {
    username: Option<String>,
    password: Option<String>,
    tls: Option<TlsClientMaterial>,
    successful_endpoints: Arc<RwLock<HashMap<String, ConnectionEndpoint>>>,
}

impl ClusterNodeConnectionFactory {
    pub fn new(
        username: Option<String>,
        password: Option<String>,
        tls: Option<TlsClientMaterial>,
    ) -> Result<Self, AppError> {
        if username.as_deref().is_some_and(str::is_empty)
            || password.as_deref().is_some_and(str::is_empty)
        {
            return Err(AppError::InvalidInput);
        }
        if let Some(material) = &tls {
            material.redis_certificates()?;
        }
        Ok(Self {
            username,
            password,
            tls,
            successful_endpoints: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub async fn connection(
        &self,
        endpoint: &ConnectionEndpoint,
    ) -> Result<redis::aio::MultiplexedConnection, AppError> {
        let url = node_url(
            endpoint,
            self.username.as_deref(),
            self.password.as_deref(),
            self.tls.as_ref(),
        )?;
        let client = match &self.tls {
            Some(material) => redis::Client::build_with_tls(url, material.redis_certificates()?),
            None => redis::Client::open(url),
        }
        .map_err(|_| AppError::ClusterNodeUnavailable)?;
        client
            .get_multiplexed_async_connection_with_config(
                &redis::AsyncConnectionConfig::new()
                    .set_connection_timeout(Some(Duration::from_secs(3)))
                    .set_response_timeout(Some(Duration::from_secs(3))),
            )
            .await
            .map_err(|_| AppError::ClusterNodeUnavailable)
    }

    pub async fn connection_for_node_id(
        &self,
        node_id: &str,
        endpoint: &ConnectionEndpoint,
    ) -> Result<redis::aio::MultiplexedConnection, AppError> {
        let connection = self.connection(endpoint).await?;
        self.successful_endpoints
            .write()
            .map_err(|_| AppError::ClusterNodeUnavailable)?
            .insert(node_id.to_owned(), endpoint.clone());
        Ok(connection)
    }

    pub fn connection_endpoint(&self, node_id: &str) -> Option<ConnectionEndpoint> {
        self.successful_endpoints.read().ok()?.get(node_id).cloned()
    }

    pub async fn connection_for_node(
        &self,
        node: &mut crate::domain::ClusterNode,
    ) -> Result<redis::aio::MultiplexedConnection, AppError> {
        let endpoint = node.endpoint.clone();
        let connection = self.connection_for_node_id(&node.id, &endpoint).await?;
        node.connection_endpoint = Some(endpoint);
        Ok(connection)
    }
}

fn node_url(
    endpoint: &ConnectionEndpoint,
    username: Option<&str>,
    password: Option<&str>,
    tls: Option<&TlsClientMaterial>,
) -> Result<String, AppError> {
    if endpoint.port == 0 {
        return Err(AppError::ClusterNodeUnavailable);
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
        return Err(AppError::ClusterNodeUnavailable);
    };
    let credentials = match (username, password) {
        (Some(username), Some(password)) => {
            format!("{}:{}@", percent_encode(username), percent_encode(password))
        }
        (Some(username), None) => format!("{}@", percent_encode(username)),
        (None, Some(password)) => format!(":{}@", percent_encode(password)),
        (None, None) => String::new(),
    };
    let scheme = if tls.is_some() { "rediss" } else { "redis" };
    let insecure = if tls.is_some_and(|tls| !tls.verify_server_cert) {
        "#insecure"
    } else {
        ""
    };
    Ok(format!(
        "{scheme}://{credentials}{host}:{}/0{insecure}",
        endpoint.port
    ))
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}
