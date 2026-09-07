use crate::{
    error::AppError,
    redis::analysis_tasks::{
        AnalysisTask, AnalysisTaskKey, AnalysisTaskResult, AnalysisTaskScope,
        StartAnalysisTaskInput,
    },
    AppState,
};
#[tauri::command]
pub async fn start_analysis_task(
    state: tauri::State<'_, AppState>,
    input: StartAnalysisTaskInput,
) -> Result<AnalysisTask, AppError> {
    state
        .redis
        .start_analysis_task(&state.analysis_tasks, input)
        .await
}
#[tauri::command]
pub fn list_analysis_tasks(
    state: tauri::State<'_, AppState>,
    input: AnalysisTaskScope,
) -> Result<Vec<AnalysisTask>, AppError> {
    state.analysis_tasks.list(&input)
}
#[tauri::command]
pub fn get_analysis_task(
    state: tauri::State<'_, AppState>,
    input: AnalysisTaskKey,
) -> Result<AnalysisTaskResult, AppError> {
    state.analysis_tasks.get(&input)
}
#[tauri::command]
pub fn cancel_analysis_task(
    state: tauri::State<'_, AppState>,
    input: AnalysisTaskKey,
) -> Result<(), AppError> {
    state.analysis_tasks.cancel(&input)
}
