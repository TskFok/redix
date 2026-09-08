use crate::{domain::is_sensitive_command, error::AppError, persistence::VersionedJsonDocument};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct QueryLibraryItem {
    pub id: String,
    pub name: String,
    pub command: String,
    pub tags: Vec<String>,
    pub updated_at: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct QueryLibraryItemInput {
    pub id: Option<String>,
    pub name: String,
    pub command: String,
    pub tags: Vec<String>,
}

impl QueryLibraryItemInput {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.id.as_deref().is_some_and(|id| !is_safe_id(id))
            || self.name.trim().is_empty()
            || self.name.trim().chars().count() > 120
            || self.command.trim().is_empty()
            || self.command.trim().chars().count() > 10_000
            || is_sensitive_command(self.command.trim())
            || self.tags.len() > 20
            || self
                .tags
                .iter()
                .any(|tag| tag.trim().is_empty() || tag.trim().chars().count() > 40)
        {
            return Err(AppError::InvalidConnection);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct QueryLibraryDocument {
    pub version: u32,
    pub items: Vec<QueryLibraryItem>,
}

impl Default for QueryLibraryDocument {
    fn default() -> Self {
        Self {
            version: 1,
            items: Vec::new(),
        }
    }
}

impl VersionedJsonDocument for QueryLibraryDocument {
    fn version(&self) -> u32 {
        self.version
    }

    fn migrate(value: serde_json::Value) -> Result<Self, AppError> {
        let version = document_version(&value)?;
        if version > 1 {
            return Err(AppError::PersistenceFailed);
        }

        let object = value.as_object().ok_or(AppError::PersistenceFailed)?;
        let raw_items = object
            .get("items")
            .cloned()
            .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
        let raw_items = serde_json::from_value::<Vec<RawQueryLibraryItem>>(raw_items)
            .map_err(|_| AppError::PersistenceFailed)?;
        if raw_items.len() > 500 {
            return Err(AppError::PersistenceFailed);
        }

        let items = raw_items
            .into_iter()
            .enumerate()
            .map(|(index, item)| {
                normalize_query_library_item(QueryLibraryItem {
                    id: item
                        .id
                        .filter(|id| !id.trim().is_empty())
                        .unwrap_or_else(|| format!("legacy-{index}")),
                    name: item.name.unwrap_or_default(),
                    command: item.command.unwrap_or_default(),
                    tags: item.tags,
                    updated_at: item.updated_at,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self { version: 1, items })
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct AppSettings {
    pub version: u32,
    pub theme: String,
    pub result_format: String,
    pub scan_count: u32,
    pub continue_on_error: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            version: 1,
            theme: "system".into(),
            result_format: "raw".into(),
            scan_count: 100,
            continue_on_error: false,
        }
    }
}

impl AppSettings {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.version != 1
            || !matches!(self.theme.as_str(), "system" | "light" | "dark")
            || !matches!(self.result_format.as_str(), "raw" | "text" | "json")
            || !(10..=10_000).contains(&self.scan_count)
        {
            return Err(AppError::InvalidConnection);
        }

        Ok(())
    }
}

impl VersionedJsonDocument for AppSettings {
    fn version(&self) -> u32 {
        self.version
    }

    fn migrate(value: serde_json::Value) -> Result<Self, AppError> {
        let version = document_version(&value)?;
        if version > 1 {
            return Err(AppError::PersistenceFailed);
        }

        let raw = serde_json::from_value::<RawAppSettings>(value)
            .map_err(|_| AppError::PersistenceFailed)?;
        let defaults = Self::default();
        let settings = Self {
            version: 1,
            theme: raw.theme.unwrap_or(defaults.theme),
            result_format: raw.result_format.unwrap_or(defaults.result_format),
            scan_count: raw.scan_count.unwrap_or(defaults.scan_count),
            continue_on_error: raw.continue_on_error.unwrap_or(defaults.continue_on_error),
        };
        settings
            .validate()
            .map_err(|_| AppError::PersistenceFailed)?;
        Ok(settings)
    }
}

pub fn normalize_query_library_item(item: QueryLibraryItem) -> Result<QueryLibraryItem, AppError> {
    let normalized = QueryLibraryItem {
        id: item.id.trim().to_owned(),
        name: item.name.trim().to_owned(),
        command: item.command.trim().to_owned(),
        tags: item
            .tags
            .into_iter()
            .map(|tag| tag.trim().to_owned())
            .filter(|tag| !tag.is_empty())
            .collect(),
        updated_at: item.updated_at,
    };
    QueryLibraryItemInput {
        id: Some(normalized.id.clone()),
        name: normalized.name.clone(),
        command: normalized.command.clone(),
        tags: normalized.tags.clone(),
    }
    .validate()?;
    Ok(normalized)
}

#[derive(Debug, Default, serde::Deserialize)]
struct RawQueryLibraryItem {
    id: Option<String>,
    name: Option<String>,
    command: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    updated_at: u64,
}

#[derive(Debug, Default, serde::Deserialize)]
struct RawAppSettings {
    theme: Option<String>,
    result_format: Option<String>,
    scan_count: Option<u32>,
    continue_on_error: Option<bool>,
}

fn document_version(value: &serde_json::Value) -> Result<u32, AppError> {
    match value.get("version") {
        None | Some(serde_json::Value::Null) => Ok(0),
        Some(version) => version
            .as_u64()
            .and_then(|version| u32::try_from(version).ok())
            .ok_or(AppError::PersistenceFailed),
    }
}

fn is_safe_id(id: &str) -> bool {
    let id = id.trim();
    !id.is_empty()
        && id.chars().count() <= 128
        && id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}
