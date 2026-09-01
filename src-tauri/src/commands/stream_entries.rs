use crate::{domain::stream_entries::*, error::AppError, AppState};

#[tauri::command]
pub async fn get_stream_entries(
    state: tauri::State<'_, AppState>,
    input: GetStreamEntriesInput,
) -> Result<StreamEntriesPage, AppError> {
    state.redis.get_stream_entries(input).await
}

#[tauri::command]
pub async fn add_stream_entry(
    state: tauri::State<'_, AppState>,
    input: AddStreamEntryInput,
) -> Result<String, AppError> {
    state.redis.add_stream_entry(input).await
}

#[tauri::command]
pub async fn delete_stream_entries(
    state: tauri::State<'_, AppState>,
    input: DeleteStreamEntriesInput,
) -> Result<u64, AppError> {
    state.redis.delete_stream_entries(input).await
}
