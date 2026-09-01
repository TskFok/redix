use serde::ser::{Serialize, SerializeStruct, Serializer};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum AppError {
    #[error("连接配置无效")]
    InvalidConnection,
    #[error("无法连接到 Redis 服务器")]
    ConnectionFailed,
    #[error("SSH 隧道建立失败，请检查 ssh-agent、私钥及 known_hosts")]
    SshTunnelFailed,
    #[error("Redis 集群拓扑发现失败")]
    ClusterTopologyFailed,
    #[error("Redis 集群节点不可用")]
    ClusterNodeUnavailable,
    #[error("部分节点操作失败")]
    PartialFailure,
    #[error("Redis 集群键槽不一致")]
    CrossSlot,
    #[error("Redis 身份验证失败")]
    AuthenticationFailed,
    #[error("不支持的 Redis 数据类型")]
    UnsupportedDataType,
    #[error("当前 Redis 功能不可用")]
    UnsupportedFeature,
    #[error("Redis key 不存在")]
    KeyNotFound,
    #[error("Redis 命令执行失败")]
    CommandFailed,
    #[error("本地数据保存失败")]
    PersistenceFailed,
    #[error("输入参数无效")]
    InvalidInput,
    #[error("操作已取消")]
    OperationCancelled,
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidConnection => "INVALID_CONNECTION",
            Self::ConnectionFailed => "CONNECTION_FAILED",
            Self::SshTunnelFailed => "SSH_TUNNEL_FAILED",
            Self::ClusterTopologyFailed => "CLUSTER_TOPOLOGY_FAILED",
            Self::ClusterNodeUnavailable => "CLUSTER_NODE_UNAVAILABLE",
            Self::PartialFailure => "PARTIAL_FAILURE",
            Self::CrossSlot => "CROSS_SLOT",
            Self::AuthenticationFailed => "AUTHENTICATION_FAILED",
            Self::UnsupportedDataType => "UNSUPPORTED_DATA_TYPE",
            Self::UnsupportedFeature => "UNSUPPORTED_FEATURE",
            Self::KeyNotFound => "KEY_NOT_FOUND",
            Self::CommandFailed => "COMMAND_FAILED",
            Self::PersistenceFailed => "PERSISTENCE_FAILED",
            Self::InvalidInput => "INVALID_INPUT",
            Self::OperationCancelled => "OPERATION_CANCELLED",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidConnection => "连接配置无效",
            Self::ConnectionFailed => "无法连接到 Redis 服务器",
            Self::SshTunnelFailed => "SSH 隧道建立失败，请检查 ssh-agent、私钥及 known_hosts",
            Self::ClusterTopologyFailed => "Redis 集群拓扑发现失败",
            Self::ClusterNodeUnavailable => "Redis 集群节点不可用",
            Self::PartialFailure => "部分节点操作失败",
            Self::CrossSlot => "Redis 集群键槽不一致",
            Self::AuthenticationFailed => "Redis 身份验证失败",
            Self::UnsupportedDataType => "不支持的 Redis 数据类型",
            Self::UnsupportedFeature => "当前 Redis 功能不可用",
            Self::KeyNotFound => "Redis key 不存在",
            Self::CommandFailed => "Redis 命令执行失败",
            Self::PersistenceFailed => "本地数据保存失败",
            Self::InvalidInput => "输入参数无效",
            Self::OperationCancelled => "操作已取消",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("AppError", 2)?;
        state.serialize_field("code", self.code())?;
        state.serialize_field("message", self.message())?;
        state.end()
    }
}
