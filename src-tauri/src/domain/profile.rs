use crate::error::AppError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ConnectionProfile {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub database: u8,
    pub has_password: bool,
}

impl ConnectionProfile {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.id.trim().is_empty()
            || self.name.trim().is_empty()
            || self.host.trim().is_empty()
            || self.port == 0
            || self.database > 15
        {
            return Err(AppError::InvalidConnection);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SaveConnectionInput {
    pub profile: ConnectionProfile,
    pub password: Option<String>,
}

pub type TestConnectionInput = SaveConnectionInput;
