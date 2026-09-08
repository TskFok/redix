use crate::{
    domain::{
        AcknowledgeStreamPendingEntriesInput, CreateKeyInput, CreateStreamConsumerGroupInput,
        DeleteKeyInput, DeleteKeysInput, DeleteStreamConsumerGroupInput, DeleteStreamConsumerInput,
        ExportKeysInput, ExportedKey, GetKeyInput, GetStreamConsumerGroupsInput,
        GetStreamConsumersInput, GetStreamPendingEntriesInput, ImportKeysInput, KeyInfo,
        KeyInfoInput, KeySummary, KeyValue, RenameKeyInput, ScanAllKeysInput, ScanKeysInput,
        ScanPage, SetKeyInput, SetKeyTtlInput, StreamConsumer, StreamConsumerGroup,
        StreamPendingEntry,
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
pub async fn scan_all_keys(
    state: tauri::State<'_, AppState>,
    input: ScanAllKeysInput,
) -> Result<Vec<KeySummary>, AppError> {
    state.redis.scan_all_keys(input).await
}

#[tauri::command]
pub async fn get_browser_key(
    state: tauri::State<'_, AppState>,
    input: GetKeyInput,
) -> Result<KeyValue, AppError> {
    state
        .redis
        .get_browser_key(&input.connection_id, &input.key)
        .await
}

#[tauri::command]
pub async fn rename_browser_key(
    state: tauri::State<'_, AppState>,
    input: RenameKeyInput,
) -> Result<KeyValue, AppError> {
    state.redis.rename_browser_key(input).await
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

#[tauri::command]
pub async fn get_stream_consumer_groups(
    state: tauri::State<'_, AppState>,
    input: GetStreamConsumerGroupsInput,
) -> Result<Vec<StreamConsumerGroup>, AppError> {
    state.redis.get_stream_consumer_groups(input).await
}

#[tauri::command]
pub async fn create_stream_consumer_group(
    state: tauri::State<'_, AppState>,
    input: CreateStreamConsumerGroupInput,
) -> Result<(), AppError> {
    state.redis.create_stream_consumer_group(input).await
}

#[tauri::command]
pub async fn delete_stream_consumer_group(
    state: tauri::State<'_, AppState>,
    input: DeleteStreamConsumerGroupInput,
) -> Result<u64, AppError> {
    state.redis.delete_stream_consumer_group(input).await
}

#[tauri::command]
pub async fn get_stream_consumers(
    state: tauri::State<'_, AppState>,
    input: GetStreamConsumersInput,
) -> Result<Vec<StreamConsumer>, AppError> {
    state.redis.get_stream_consumers(input).await
}

#[tauri::command]
pub async fn get_stream_pending_entries(
    state: tauri::State<'_, AppState>,
    input: GetStreamPendingEntriesInput,
) -> Result<Vec<StreamPendingEntry>, AppError> {
    state.redis.get_stream_pending_entries(input).await
}

#[tauri::command]
pub async fn acknowledge_stream_pending_entries(
    state: tauri::State<'_, AppState>,
    input: AcknowledgeStreamPendingEntriesInput,
) -> Result<u64, AppError> {
    state.redis.acknowledge_stream_pending_entries(input).await
}

#[tauri::command]
pub async fn delete_stream_consumer(
    state: tauri::State<'_, AppState>,
    input: DeleteStreamConsumerInput,
) -> Result<u64, AppError> {
    state.redis.delete_stream_consumer(input).await
}

#[tauri::command]
pub async fn export_keys(
    state: tauri::State<'_, AppState>,
    input: ExportKeysInput,
) -> Result<Vec<ExportedKey>, AppError> {
    state.redis.export_keys(input).await
}

#[tauri::command]
pub async fn import_keys(
    state: tauri::State<'_, AppState>,
    input: ImportKeysInput,
) -> Result<u64, AppError> {
    state.redis.import_keys(input).await
}

#[tauri::command]
pub async fn claim_stream_pending_entries(
    state: tauri::State<'_, AppState>,
    input: crate::domain::ClaimStreamPendingEntriesInput,
) -> Result<Vec<String>, AppError> {
    state.redis.claim_stream_pending_entries(input).await
}
