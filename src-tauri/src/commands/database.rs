use crate::{
    domain::{
        AnalyzeDatabaseInput, DatabaseAnalysisReport, DatabaseOverview, InstanceDetails,
        InstanceOverview, SelectDatabaseInput,
    },
    error::AppError,
    redis::RedisOperations,
    AppState,
};

#[tauri::command(rename_all = "snake_case")]
pub async fn get_instance_overview(
    state: tauri::State<'_, AppState>,
    connection_id: String,
) -> Result<InstanceOverview, AppError> {
    state.redis.get_instance_overview(&connection_id).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_instance_details(
    state: tauri::State<'_, AppState>,
    connection_id: String,
) -> Result<InstanceDetails, AppError> {
    state.redis.get_instance_details(&connection_id).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn analyze_database(
    state: tauri::State<'_, AppState>,
    input: AnalyzeDatabaseInput,
) -> Result<DatabaseAnalysisReport, AppError> {
    state.redis.analyze_database(input).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_database_overview(
    state: tauri::State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<DatabaseOverview>, AppError> {
    state.redis.get_database_overview(&connection_id).await
}

#[tauri::command]
pub async fn select_database(
    state: tauri::State<'_, AppState>,
    input: SelectDatabaseInput,
) -> Result<crate::domain::ConnectionProfile, AppError> {
    let connection_id = input.connection_id.clone();
    let profile = state.redis.select_database(input).await?;
    state.cli.close_connection(&connection_id).await;
    Ok(profile)
}
