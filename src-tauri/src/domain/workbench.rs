use crate::{error::AppError, persistence::VersionedJsonDocument};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CommandResult {
    pub kind: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CommandArgument {
    pub name: String,
    pub required: bool,
    pub hint: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CommandDefinition {
    pub name: String,
    pub summary: String,
    pub arguments: Vec<CommandArgument>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ExecuteCommandsInput {
    pub connection_id: String,
    pub commands: Vec<String>,
    pub continue_on_error: bool,
}

impl ExecuteCommandsInput {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.connection_id.trim().is_empty()
            || self.commands.is_empty()
            || self.commands.len() > 100
            || self
                .commands
                .iter()
                .any(|command| command.trim().is_empty())
        {
            return Err(AppError::InvalidConnection);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CommandExecutionItem {
    pub command: String,
    pub result: Option<CommandResult>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SaveCommandHistoryInput {
    pub connection_id: String,
    pub entries: Vec<CommandHistoryEntry>,
}

impl SaveCommandHistoryInput {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.connection_id.trim().is_empty()
            || self.entries.len() > 100
            || self.entries.iter().any(|entry| {
                entry.connection_id != self.connection_id || entry.command.trim().is_empty()
            })
        {
            return Err(AppError::InvalidConnection);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CommandHistoryEntry {
    pub connection_id: String,
    pub command: String,
    pub result: Option<CommandResult>,
    pub error_code: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CommandHistoryDocument {
    pub version: u32,
    pub entries: Vec<CommandHistoryEntry>,
}

impl Default for CommandHistoryDocument {
    fn default() -> Self {
        Self {
            version: 1,
            entries: Vec::new(),
        }
    }
}

impl VersionedJsonDocument for CommandHistoryDocument {
    fn version(&self) -> u32 {
        self.version
    }

    fn migrate(value: serde_json::Value) -> Result<Self, AppError> {
        let document: Self =
            serde_json::from_value(value).map_err(|_| AppError::PersistenceFailed)?;
        if document.version == 1 {
            Ok(document)
        } else {
            Err(AppError::PersistenceFailed)
        }
    }
}

pub fn command_catalog() -> Vec<CommandDefinition> {
    [
        ("PING", "检查 Redis 连接", vec![]),
        ("GET", "读取字符串键", vec![("key", true, "键名")]),
        (
            "SET",
            "写入字符串键",
            vec![("key", true, "键名"), ("value", true, "字符串值")],
        ),
        ("DEL", "删除一个或多个键", vec![("key", true, "键名")]),
        ("EXISTS", "检查键是否存在", vec![("key", true, "键名")]),
        ("TYPE", "查看键类型", vec![("key", true, "键名")]),
        ("TTL", "查看键的秒级 TTL", vec![("key", true, "键名")]),
        ("PTTL", "查看键的毫秒级 TTL", vec![("key", true, "键名")]),
        ("DBSIZE", "查看数据库键数量", vec![]),
        (
            "INFO",
            "查看 Redis 服务信息",
            vec![("section", false, "信息分区")],
        ),
        (
            "SCAN",
            "增量扫描键名",
            vec![("cursor", true, "游标"), ("pattern", false, "匹配模式")],
        ),
    ]
    .into_iter()
    .map(|(name, summary, arguments)| CommandDefinition {
        name: name.into(),
        summary: summary.into(),
        arguments: arguments
            .into_iter()
            .map(|(name, required, hint)| CommandArgument {
                name: name.into(),
                required,
                hint: hint.into(),
            })
            .collect(),
    })
    .collect()
}

pub fn is_sensitive_command(command: &str) -> bool {
    matches!(
        command
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase()
            .as_str(),
        "AUTH" | "HELLO" | "ACL" | "CONFIG"
    )
}

pub fn filter_history_entry(entry: &CommandHistoryEntry) -> Option<CommandHistoryEntry> {
    (!is_sensitive_command(&entry.command)).then(|| entry.clone())
}
