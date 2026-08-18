use crate::{
    domain::{
        CreateKeyInput, DeleteKeyInput, DeleteKeysInput, GetKeyInput, KeyInfo, KeyInfoInput,
        KeyValue, RenameKeyInput, ScanKeysInput, ScanPage, SetKeyInput, SetKeyTtlInput,
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
pub async fn create_key(
    state: tauri::State<'_, AppState>,
    input: CreateKeyInput,
) -> Result<KeyValue, AppError> {
    state.redis.create_key(input).await
}

#[tauri::command]
pub async fn rename_key(
    state: tauri::State<'_, AppState>,
    input: RenameKeyInput,
) -> Result<KeyValue, AppError> {
    state.redis.rename_key(input).await
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
pub async fn delete_keys(
    state: tauri::State<'_, AppState>,
    input: DeleteKeysInput,
) -> Result<u64, AppError> {
    state.redis.delete_keys(input).await
}

#[tauri::command]
pub async fn set_key_ttl(
    state: tauri::State<'_, AppState>,
    input: SetKeyTtlInput,
) -> Result<i64, AppError> {
    state.redis.set_key_ttl(input).await
}

#[tauri::command]
pub async fn get_key_info(
    state: tauri::State<'_, AppState>,
    input: KeyInfoInput,
) -> Result<KeyInfo, AppError> {
    state.redis.get_key_info(input).await
}
