use crate::{
    error::AppError,
    redis::bulk_tasks::{BulkTask, StartBulkDeleteInput},
    AppState,
};
#[tauri::command]
pub async fn start_bulk_delete(
    state: tauri::State<'_, AppState>,
    input: StartBulkDeleteInput,
) -> Result<BulkTask, AppError> {
    state
        .redis
        .start_bulk_delete(&state.bulk_tasks, input)
        .await
}
#[tauri::command]
pub fn list_bulk_tasks(state: tauri::State<'_, AppState>) -> Result<Vec<BulkTask>, AppError> {
    state.bulk_tasks.list()
}
#[tauri::command]
pub fn cancel_bulk_task(
    state: tauri::State<'_, AppState>,
    task_id: String,
) -> Result<(), AppError> {
    state.bulk_tasks.cancel(&task_id)
}
