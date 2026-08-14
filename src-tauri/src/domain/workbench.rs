#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CommandResult {
    pub kind: String,
    pub value: serde_json::Value,
}
