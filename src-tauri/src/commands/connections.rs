use crate::{
    domain::{ConnectionInfo, ConnectionProfile, SaveConnectionInput, TestConnectionInput},
    error::AppError,
    redis::RedisOperations,
    AppState,
};

#[tauri::command]
pub async fn list_connections(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ConnectionProfile>, AppError> {
    state.profiles.load()
}

#[tauri::command]
pub async fn save_connection(
    state: tauri::State<'_, AppState>,
    input: SaveConnectionInput,
) -> Result<ConnectionProfile, AppError> {
    save_connection_inner(state.inner(), input).await
}

pub async fn save_connection_inner(
    state: &AppState,
    input: SaveConnectionInput,
) -> Result<ConnectionProfile, AppError> {
    let mut profile = input.profile;
    profile.validate()?;

    match input.password.as_deref() {
        Some(password) => {
            state.secrets.write(&profile.id, password)?;
            profile.has_password = true;
        }
        None if !profile.has_password => {
            state.secrets.delete(&profile.id)?;
        }
        None => {}
    }

    let mut profiles = state.profiles.load()?;
    if let Some(existing) = profiles.iter_mut().find(|item| item.id == profile.id) {
        *existing = profile.clone();
    } else {
        profiles.push(profile.clone());
    }
    state.profiles.save(&profiles)?;

    Ok(profile)
}

#[tauri::command]
pub async fn delete_connection(
    state: tauri::State<'_, AppState>,
    connection_id: String,
) -> Result<(), AppError> {
    state.redis.close_connection(&connection_id).await?;

    let mut profiles = state.profiles.load()?;
    let original_len = profiles.len();
    profiles.retain(|profile| profile.id != connection_id);
    if profiles.len() == original_len {
        return Err(AppError::InvalidConnection);
    }

    state.profiles.save(&profiles)?;
    state.secrets.delete(&connection_id)
}

#[tauri::command]
pub async fn test_connection(
    state: tauri::State<'_, AppState>,
    input: TestConnectionInput,
) -> Result<ConnectionInfo, AppError> {
    state
        .redis
        .test_connection(&input.profile, input.password.as_deref())
        .await
}

#[tauri::command]
pub async fn open_connection(
    state: tauri::State<'_, AppState>,
    connection_id: String,
) -> Result<ConnectionInfo, AppError> {
    state.redis.open_connection(&connection_id).await
}

#[tauri::command]
pub async fn close_connection(
    state: tauri::State<'_, AppState>,
    connection_id: String,
) -> Result<(), AppError> {
    state.redis.close_connection(&connection_id).await
}
