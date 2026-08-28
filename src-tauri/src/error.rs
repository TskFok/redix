use serde::ser::{Serialize, SerializeStruct, Serializer};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum AppError {
    #[error("连接配置无效")]
    InvalidConnection,
    #[error("无法连接到 Redis 服务器")]
    ConnectionFailed,
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
