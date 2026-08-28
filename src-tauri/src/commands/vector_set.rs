use crate::{
    domain::{
        AddVectorSetElementsInput, CreateVectorSetInput, DeleteVectorSetElementsInput, KeyValue,
        SetVectorSetAttributesInput, VectorSetElement, VectorSetElementInput, VectorSetKeyInput,
        VectorSetPage, VectorSetSummary, VectorSimilarityQueryInput, VectorSimilarityResult,
    },
    error::AppError,
    redis::RedisOperations,
    AppState,
};

#[tauri::command]
pub async fn create_vector_set(
    state: tauri::State<'_, AppState>,
    input: CreateVectorSetInput,
) -> Result<KeyValue, AppError> {
    state.redis.create_vector_set(input).await
}

#[tauri::command]
pub async fn add_vector_set_elements(
    state: tauri::State<'_, AppState>,
    input: AddVectorSetElementsInput,
) -> Result<(), AppError> {
    state.redis.add_vector_set_elements(input).await
}

#[tauri::command]
pub async fn get_vector_set_summary(
    state: tauri::State<'_, AppState>,
    input: VectorSetKeyInput,
) -> Result<VectorSetSummary, AppError> {
    state.redis.get_vector_set_summary(input).await
}

#[tauri::command]
pub async fn list_vector_set_elements(
    state: tauri::State<'_, AppState>,
    input: crate::domain::ListVectorSetElementsInput,
) -> Result<VectorSetPage, AppError> {
    state.redis.list_vector_set_elements(input).await
}

#[tauri::command]
pub async fn get_vector_set_element(
    state: tauri::State<'_, AppState>,
    input: VectorSetElementInput,
) -> Result<VectorSetElement, AppError> {
    state.redis.get_vector_set_element(input).await
}

#[tauri::command]
pub async fn set_vector_set_attributes(
    state: tauri::State<'_, AppState>,
    input: SetVectorSetAttributesInput,
) -> Result<VectorSetElement, AppError> {
    state.redis.set_vector_set_attributes(input).await
}

#[tauri::command]
pub async fn delete_vector_set_attributes(
    state: tauri::State<'_, AppState>,
    input: VectorSetElementInput,
) -> Result<(), AppError> {
    state.redis.delete_vector_set_attributes(input).await
}

#[tauri::command]
pub async fn delete_vector_set_elements(
    state: tauri::State<'_, AppState>,
    input: DeleteVectorSetElementsInput,
) -> Result<u64, AppError> {
    state.redis.delete_vector_set_elements(input).await
}

#[tauri::command]
pub async fn search_vector_set(
    state: tauri::State<'_, AppState>,
    input: VectorSimilarityQueryInput,
) -> Result<VectorSimilarityResult, AppError> {
    state.redis.search_vector_set(input).await
}

#[tauri::command]
pub async fn download_vector_embedding(
    state: tauri::State<'_, AppState>,
    input: VectorSetElementInput,
) -> Result<String, AppError> {
    state.redis.download_vector_embedding(input).await
}
