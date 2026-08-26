use crate::{
    domain::{
        AppendJsonArrayInput, DeleteJsonPathInput, GetJsonPathInput, JsonMutationResult,
        JsonPathValue, ModuleCapabilities, SetJsonPathInput,
    },
    error::AppError,
    redis::RedisOperations,
    AppState,
};

#[tauri::command(rename_all = "snake_case")]
pub async fn get_module_capabilities(
    state: tauri::State<'_, AppState>,
    connection_id: String,
) -> Result<ModuleCapabilities, AppError> {
    state.redis.get_module_capabilities(&connection_id).await
}

#[tauri::command]
pub async fn get_json_path(
    state: tauri::State<'_, AppState>,
    input: GetJsonPathInput,
) -> Result<JsonPathValue, AppError> {
    state.redis.get_json_path(input).await
}

#[tauri::command]
pub async fn set_json_path(
    state: tauri::State<'_, AppState>,
    input: SetJsonPathInput,
) -> Result<JsonMutationResult, AppError> {
    state.redis.set_json_path(input).await
}

#[tauri::command]
pub async fn append_json_array(
    state: tauri::State<'_, AppState>,
    input: AppendJsonArrayInput,
) -> Result<JsonMutationResult, AppError> {
    state.redis.append_json_array(input).await
}

#[tauri::command]
pub async fn delete_json_path(
    state: tauri::State<'_, AppState>,
    input: DeleteJsonPathInput,
) -> Result<JsonMutationResult, AppError> {
    state.redis.delete_json_path(input).await
}
