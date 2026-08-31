use crate::{
    domain::{CliCommandInput, CliReply, CliSessionInput},
    error::AppError,
    AppState,
};

#[tauri::command]
pub async fn open_cli_session(
    state: tauri::State<'_, AppState>,
    input: CliSessionInput,
) -> Result<(), AppError> {
    state.cli.open(&state.redis, input).await
}

#[tauri::command]
pub async fn execute_cli_command(
    state: tauri::State<'_, AppState>,
    input: CliCommandInput,
) -> Result<CliReply, AppError> {
    state.cli.execute(input).await
}

#[tauri::command]
pub async fn close_cli_session(
    state: tauri::State<'_, AppState>,
    input: CliSessionInput,
) -> Result<(), AppError> {
    state.cli.close(input).await
}
