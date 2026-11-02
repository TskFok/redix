use serde::ser::{Serialize, SerializeStruct, Serializer};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionFailureReason {
    Refused,
    Timeout,
    DnsResolution,
    NetworkUnreachable,
    Closed,
    TlsCertificate,
    TlsHandshake,
    TlsTimeout,
    Protocol,
    PermissionDenied,
    ServerUnavailable,
}

impl ConnectionFailureReason {
    fn diagnostics(self) -> &'static str {
        match self {
            Self::Refused => "TCP 连接被拒绝。请确认 Redis 已启动、主机和端口正确，并检查监听地址、防火墙及端口映射。",
            Self::Timeout => "连接或等待 Redis 响应超时。请检查网络、VPN、防火墙和服务器负载，并确认目标端口可达。",
            Self::DnsResolution => "DNS 无法解析服务器主机名。请检查主机名拼写、DNS 配置，以及是否需要连接内网或 VPN。",
            Self::NetworkUnreachable => "目标主机或网络不可达。请检查网络连接、路由、VPN 和防火墙，确认当前设备能够访问 Redis 所在网络。",
            Self::Closed => "连接已断开或被服务器重置。请确认目标端口提供 Redis 服务，并检查 TLS 设置、代理和服务器日志。",
            Self::TlsCertificate => "TLS 服务器证书验证失败。请检查证书是否过期、CA 是否受信任，以及连接主机名是否与证书一致。",
            Self::TlsHandshake => "TLS 握手失败。请确认目标端口已启用 TLS，并检查 TLS 协议、CA 证书和客户端证书配置。",
            Self::TlsTimeout => "TLS 握手超时。请确认目标端口提供 TLS 服务，并检查网络、代理及服务器 TLS 配置。",
            Self::Protocol => "服务器响应不符合预期的 Redis 协议。请确认主机和端口指向 Redis，并检查 TLS 设置与服务器协议版本。",
            Self::PermissionDenied => "Redis 账号没有执行连接检查命令的权限。请检查 ACL 配置，确认账号可以执行 PING 及所需的连接初始化命令。",
            Self::ServerUnavailable => "Redis 服务器或集群当前不可用，可能正在加载数据或缺少可用主节点。请检查服务器状态、集群节点连通性及日志。",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum AppError {
    #[error("连接配置无效")]
    InvalidConnection,
    #[error("无法连接到 Redis 服务器")]
    ConnectionFailed,
    #[error("无法连接到 Redis 服务器")]
    ConnectionDiagnostic(ConnectionFailureReason),
    #[error("SSH 隧道建立失败，请检查 SSH 网络连接及认证凭据")]
    SshTunnelFailed,
    #[error("首次连接此 SSH 主机，请确认主机指纹")]
    SshHostKeyUnknown(uuid::Uuid),
    #[error("SSH 主机密钥已变化，连接已拒绝，请联系服务器管理员核实")]
    SshHostKeyChanged,
    #[error("无法读取或保存 SSH 主机信任记录，请检查应用数据目录权限")]
    SshHostKeyStoreFailed,
    #[error("SSH 主机指纹确认已过期，请重新连接")]
    SshHostKeyChallengeExpired,
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
            Self::ConnectionFailed | Self::ConnectionDiagnostic(_) => "CONNECTION_FAILED",
            Self::SshTunnelFailed => "SSH_TUNNEL_FAILED",
            Self::SshHostKeyUnknown(_) => "SSH_HOST_KEY_UNKNOWN",
            Self::SshHostKeyChanged => "SSH_HOST_KEY_CHANGED",
            Self::SshHostKeyStoreFailed => "SSH_HOST_KEY_STORE_FAILED",
            Self::SshHostKeyChallengeExpired => "SSH_HOST_KEY_CHALLENGE_EXPIRED",
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
            Self::ConnectionFailed | Self::ConnectionDiagnostic(_) => "无法连接到 Redis 服务器",
            Self::SshTunnelFailed => "SSH 隧道建立失败，请检查 SSH 网络连接及认证凭据",
            Self::SshHostKeyUnknown(_) => "首次连接此 SSH 主机，请确认主机指纹",
            Self::SshHostKeyChanged => "SSH 主机密钥已变化，连接已拒绝，请联系服务器管理员核实",
            Self::SshHostKeyStoreFailed => {
                "无法读取或保存 SSH 主机信任记录，请检查应用数据目录权限"
            }
            Self::SshHostKeyChallengeExpired => "SSH 主机指纹确认已过期，请重新连接",
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

    pub fn diagnostics(&self) -> Option<&'static str> {
        match self {
            Self::ConnectionDiagnostic(reason) => Some(reason.diagnostics()),
            Self::AuthenticationFailed => Some("Redis 身份验证未通过。请检查用户名和密码、账号是否启用，以及 Redis ACL 或 requirepass 配置；使用 Sentinel 时还需核对 Sentinel 的独立凭据。"),
            _ => None,
        }
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let diagnostics = self.diagnostics();
        let challenge_id = match self {
            Self::SshHostKeyUnknown(id) => Some(id.to_string()),
            _ => None,
        };
        let mut state = serializer.serialize_struct(
            "AppError",
            2 + usize::from(diagnostics.is_some()) + usize::from(challenge_id.is_some()),
        )?;
        state.serialize_field("code", self.code())?;
        state.serialize_field("message", self.message())?;
        if let Some(diagnostics) = diagnostics {
            state.serialize_field("diagnostics", diagnostics)?;
        }
        if let Some(challenge_id) = challenge_id {
            state.serialize_field("challenge_id", &challenge_id)?;
        }
        state.end()
    }
}
