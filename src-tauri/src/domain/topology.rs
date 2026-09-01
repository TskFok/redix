use crate::{
    domain::{ClusterConfig, ConnectionProfile, SentinelConfig},
    error::AppError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionTarget {
    Standalone,
    Sentinel(SentinelConfig),
    Cluster(ClusterConfig),
}

impl ConnectionTarget {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Standalone => "standalone",
            Self::Sentinel(_) => "sentinel",
            Self::Cluster(_) => "cluster",
        }
    }
}

impl TryFrom<&ConnectionProfile> for ConnectionTarget {
    type Error = AppError;

    fn try_from(profile: &ConnectionProfile) -> Result<Self, Self::Error> {
        profile.validate()?;
        match (&profile.sentinel, &profile.cluster) {
            (Some(sentinel), None) => Ok(Self::Sentinel(sentinel.clone())),
            (None, Some(cluster)) => Ok(Self::Cluster(cluster.clone())),
            (None, None) => Ok(Self::Standalone),
            (Some(_), Some(_)) => Err(AppError::InvalidConnection),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeScope {
    Routed,
    PrimaryNodes,
    AllNodes,
    Node(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeFailure {
    pub node_id: String,
    pub code: String,
}

use serde::{Deserialize, Serialize};
