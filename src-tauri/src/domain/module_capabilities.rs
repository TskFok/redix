use crate::{domain::ModuleSummary, error::AppError};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct GetModuleCapabilitiesInput {
    pub connection_id: String,
}

impl GetModuleCapabilitiesInput {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.connection_id.trim().is_empty() {
            return Err(AppError::InvalidInput);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ModuleCapabilities {
    pub modules: Vec<ModuleSummary>,
    pub json_supported: bool,
    pub json_version: Option<String>,
}

impl ModuleCapabilities {
    pub fn from_modules(modules: Vec<ModuleSummary>) -> Self {
        let mut json_supported = false;
        let mut json_version = None;

        for module in &modules {
            let normalized = module.name.trim().to_ascii_lowercase();
            if normalized == "rejson" || normalized == "redisjson" {
                json_supported = true;
                if json_version.is_none() {
                    if let Some(version) = module.version.as_ref().map(|value| value.trim()) {
                        if !version.is_empty() {
                            json_version = Some(version.to_string());
                        }
                    }
                }
            }
        }

        Self {
            modules,
            json_supported,
            json_version,
        }
    }
}
