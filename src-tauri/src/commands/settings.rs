use crate::{domain::AppSettings, error::AppError, persistence::JsonDocumentStore, AppState};

#[tauri::command]
pub fn get_app_settings(state: tauri::State<'_, AppState>) -> Result<AppSettings, AppError> {
    get_app_settings_inner(state.inner())
}

#[tauri::command]
pub fn save_app_settings(
    state: tauri::State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, AppError> {
    save_app_settings_inner(state.inner(), settings)
}

pub fn get_app_settings_inner(state: &AppState) -> Result<AppSettings, AppError> {
    settings_store(state).load_or_default::<AppSettings>()
}

pub fn save_app_settings_inner(
    state: &AppState,
    settings: AppSettings,
) -> Result<AppSettings, AppError> {
    settings.validate()?;
    settings_store(state).save(&settings)?;
    Ok(settings)
}

fn settings_store(state: &AppState) -> JsonDocumentStore {
    JsonDocumentStore::new(state.data_dir.join("settings.json"))
}
