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
        if profile.cluster.is_some() && profile.ssh.is_some() {
            return Err(AppError::UnsupportedFeature);
        }
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClusterSummary {
    pub state: String,
    pub slots_assigned: u16,
    pub slots_ok: u16,
    pub slots_pfail: u16,
    pub slots_fail: u16,
    pub current_epoch: u64,
    pub size: u16,
    pub known_nodes: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlotRange {
    pub start: u16,
    pub end: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClusterNodeRole {
    Primary,
    Replica,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClusterNodeHealth {
    Online,
    Offline,
    Loading,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ClusterNodeMetrics {
    pub server_version: Option<String>,
    pub redis_mode: Option<String>,
    pub total_keys: Option<u64>,
    pub maxmemory_bytes: Option<u64>,
    pub used_memory_bytes: Option<u64>,
    pub ops_per_second: Option<u64>,
    pub connections_received: Option<u64>,
    pub connected_clients: Option<u64>,
    pub commands_processed: Option<u64>,
    pub network_in_kbps: Option<f64>,
    pub network_out_kbps: Option<f64>,
    pub cache_hit_ratio: Option<f64>,
    pub replication_offset: Option<u64>,
    pub replication_lag: Option<u64>,
    pub uptime_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClusterNode {
    pub id: String,
    pub endpoint: crate::domain::ConnectionEndpoint,
    /// 仅在后续连接成功时由服务层填充；拓扑解析始终保持为 `None`。
    pub connection_endpoint: Option<crate::domain::ConnectionEndpoint>,
    pub role: ClusterNodeRole,
    pub health: ClusterNodeHealth,
    pub primary_id: Option<String>,
    pub slots: Vec<SlotRange>,
    pub metrics: ClusterNodeMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClusterTopology {
    pub summary: ClusterSummary,
    pub nodes: Vec<ClusterNode>,
    pub failures: Vec<NodeFailure>,
}

use serde::{Deserialize, Serialize};
