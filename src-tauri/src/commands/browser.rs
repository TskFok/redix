use crate::{
    domain::{
        DeleteKeyInput, GetKeyInput, KeyValue, ScanKeysInput, ScanPage, SetKeyInput, SetKeyTtlInput,
    },
    error::AppError,
    redis::RedisOperations,
    AppState,
};

#[tauri::command]
pub async fn scan_keys(
    state: tauri::State<'_, AppState>,
    input: ScanKeysInput,
) -> Result<ScanPage, AppError> {
    state.redis.scan_keys(input).await
}

#[tauri::command]
pub async fn get_key(
    state: tauri::State<'_, AppState>,
    input: GetKeyInput,
) -> Result<KeyValue, AppError> {
    state.redis.get_key(&input.connection_id, &input.key).await
}

#[tauri::command]
pub async fn set_key(
    state: tauri::State<'_, AppState>,
    input: SetKeyInput,
) -> Result<KeyValue, AppError> {
    state.redis.set_key(input).await
}

#[tauri::command]
pub async fn delete_key(
    state: tauri::State<'_, AppState>,
    input: DeleteKeyInput,
) -> Result<(), AppError> {
    state
        .redis
        .delete_key(&input.connection_id, &input.key)
        .await
}

#[tauri::command]
pub async fn set_key_ttl(
    state: tauri::State<'_, AppState>,
    input: SetKeyTtlInput,
) -> Result<i64, AppError> {
    state.redis.set_key_ttl(input).await
}
