use super::CommandResult;
use crate::error::AppError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CliSessionInput {
    pub connection_id: String,
    pub session_id: String,
}

impl CliSessionInput {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.connection_id.trim().is_empty() || uuid::Uuid::parse_str(&self.session_id).is_err()
        {
            return Err(AppError::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CliCommandInput {
    pub connection_id: String,
    pub session_id: String,
    pub command: String,
}

impl CliCommandInput {
    pub fn session(&self) -> CliSessionInput {
        CliSessionInput {
            connection_id: self.connection_id.clone(),
            session_id: self.session_id.clone(),
        }
    }

    pub fn validate(&self) -> Result<Vec<String>, AppError> {
        self.session().validate()?;
        if self.command.len() > 16 * 1024 || self.command.contains(['\r', '\n']) {
            return Err(AppError::InvalidInput);
        }
        let args =
            crate::redis::tokenize_command(&self.command).map_err(|_| AppError::InvalidInput)?;
        let name = args[0].to_ascii_uppercase();
        let subcommand = args
            .get(1)
            .map(|arg| arg.to_ascii_uppercase())
            .unwrap_or_default();
        if matches!(
            name.as_str(),
            "SUBSCRIBE"
                | "PSUBSCRIBE"
                | "SSUBSCRIBE"
                | "UNSUBSCRIBE"
                | "PUNSUBSCRIBE"
                | "SUNSUBSCRIBE"
                | "MONITOR"
                | "SYNC"
                | "PSYNC"
                | "REPLCONF"
        ) || (name == "CLIENT"
            && matches!(subcommand.as_str(), "REPLY" | "TRACKING" | "CACHING"))
            || (name == "SCRIPT" && subcommand == "DEBUG")
        {
            return Err(AppError::UnsupportedFeature);
        }
        Ok(args)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CliReply {
    pub result: Option<CommandResult>,
    pub error_code: Option<String>,
    pub session_closed: bool,
    pub truncated: bool,
}

impl CliReply {
    pub fn error(code: &str, session_closed: bool) -> Self {
        Self {
            result: None,
            error_code: Some(code.into()),
            session_closed,
            truncated: false,
        }
    }
}
