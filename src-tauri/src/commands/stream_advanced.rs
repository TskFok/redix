use crate::{domain::stream_advanced::*, error::AppError, AppState};

#[tauri::command]
pub async fn update_stream_group_id(
    state: tauri::State<'_, AppState>,
    input: UpdateStreamGroupIdInput,
) -> Result<(), AppError> {
    state.redis.update_stream_group_id(input).await
}

#[tauri::command]
pub async fn get_stream_pending_page(
    state: tauri::State<'_, AppState>,
    input: GetStreamPendingPageInput,
) -> Result<StreamPendingPage, AppError> {
    state.redis.get_stream_pending_page(input).await
}

#[tauri::command]
pub async fn claim_stream_pending_advanced(
    state: tauri::State<'_, AppState>,
    input: ClaimStreamPendingAdvancedInput,
) -> Result<Vec<String>, AppError> {
    state.redis.claim_stream_pending_advanced(input).await
}
