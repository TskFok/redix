use crate::{
    domain::{
        GetSlowLogsInput, ProfilerSession, PubSubSession, PublishPubSubInput, SlowLogConfig,
        SlowLogEntry, StartProfilerInput, StartPubSubInput, StopProfilerInput, StopPubSubInput,
        UpdateSlowLogConfigInput,
    },
    error::AppError,
    redis::RedisOperations,
    AppState,
};

#[tauri::command]
pub async fn get_slow_logs(
    state: tauri::State<'_, AppState>,
    input: GetSlowLogsInput,
) -> Result<Vec<SlowLogEntry>, AppError> {
    state.redis.get_slow_logs(input).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn clear_slow_logs(
    state: tauri::State<'_, AppState>,
    connection_id: String,
) -> Result<(), AppError> {
    state.redis.clear_slow_logs(&connection_id).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_slow_log_config(
    state: tauri::State<'_, AppState>,
    connection_id: String,
) -> Result<SlowLogConfig, AppError> {
    state.redis.get_slow_log_config(&connection_id).await
}

#[tauri::command]
pub async fn update_slow_log_config(
    state: tauri::State<'_, AppState>,
    input: UpdateSlowLogConfigInput,
) -> Result<SlowLogConfig, AppError> {
    state.redis.update_slow_log_config(input).await
}

#[tauri::command]
pub async fn start_pub_sub(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: StartPubSubInput,
) -> Result<PubSubSession, AppError> {
    state.redis.start_pub_sub(app, input).await
}

#[tauri::command]
pub async fn stop_pub_sub(
    state: tauri::State<'_, AppState>,
    input: StopPubSubInput,
) -> Result<(), AppError> {
    state.redis.stop_pub_sub(input).await
}

#[tauri::command]
pub async fn publish_pub_sub(
    state: tauri::State<'_, AppState>,
    input: PublishPubSubInput,
) -> Result<u64, AppError> {
    state.redis.publish_pub_sub(input).await
}

#[tauri::command]
pub async fn start_profiler(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: StartProfilerInput,
) -> Result<ProfilerSession, AppError> {
    state.redis.start_profiler(app, input).await
}

#[tauri::command]
pub async fn stop_profiler(
    state: tauri::State<'_, AppState>,
    input: StopProfilerInput,
) -> Result<(), AppError> {
    state.redis.stop_profiler(input).await
}
