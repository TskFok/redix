use crate::{
    error::AppError,
    persistence::analysis_history::{AnalysisHistorySummary, SaveAnalysisInput, SavedAnalysis},
    AppState,
};

#[derive(serde::Deserialize)]
pub struct AnalysisHistoryScope {
    pub connection_id: String,
    pub database: u8,
}

#[derive(serde::Deserialize)]
pub struct AnalysisHistoryItemInput {
    pub connection_id: String,
    pub database: u8,
    pub id: String,
}

#[tauri::command]
pub fn list_analysis_history(
    state: tauri::State<'_, AppState>,
    input: AnalysisHistoryScope,
) -> Result<Vec<AnalysisHistorySummary>, AppError> {
    state
        .analysis_history
        .list(&input.connection_id, input.database)
}

#[tauri::command]
pub fn save_analysis_history(
    state: tauri::State<'_, AppState>,
    input: SaveAnalysisInput,
) -> Result<AnalysisHistorySummary, AppError> {
    state.analysis_history.save(input)
}

#[tauri::command]
pub fn get_analysis_history(
    state: tauri::State<'_, AppState>,
    input: AnalysisHistoryItemInput,
) -> Result<SavedAnalysis, AppError> {
    state
        .analysis_history
        .get(&input.connection_id, input.database, &input.id)
}

#[tauri::command]
pub fn delete_analysis_history(
    state: tauri::State<'_, AppState>,
    input: AnalysisHistoryItemInput,
) -> Result<(), AppError> {
    state
        .analysis_history
        .delete(&input.connection_id, input.database, &input.id)
}
