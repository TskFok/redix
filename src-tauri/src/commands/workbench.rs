use crate::{
    domain::{CommandResult, ExecuteCommandInput},
    error::AppError,
    redis::RedisOperations,
    AppState,
};

#[tauri::command]
pub async fn execute_command(
    state: tauri::State<'_, AppState>,
    input: ExecuteCommandInput,
) -> Result<CommandResult, AppError> {
    state
        .redis
        .execute_command(&input.connection_id, &input.command)
        .await
}
