use crate::{domain::value_codec::*, error::AppError, AppState};

#[tauri::command]
pub async fn get_string_value(
    state: tauri::State<'_, AppState>,
    input: GetStringValueInput,
) -> Result<StringValue, AppError> {
    state.redis.get_string_value(input).await
}

#[tauri::command]
pub async fn set_string_value(
    state: tauri::State<'_, AppState>,
    input: SetStringValueInput,
) -> Result<StringValueSaved, AppError> {
    state.redis.set_string_value(input).await
}

#[tauri::command]
pub async fn decode_string_value(
    input: DecodeStringValueInput,
) -> Result<DecodedStringValue, AppError> {
    tokio::task::spawn_blocking(move || decode_value(input))
        .await
        .map_err(|_| AppError::CommandFailed)?
}

#[tauri::command]
pub async fn encode_string_value(input: EncodeStringValueInput) -> Result<String, AppError> {
    tokio::task::spawn_blocking(move || encode_value(input))
        .await
        .map_err(|_| AppError::CommandFailed)?
}
