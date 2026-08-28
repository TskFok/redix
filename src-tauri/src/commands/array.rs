use crate::{
    domain::{
        AggregateArrayInput, AppendArrayInput, ArrayKeyInput, ArrayMultiGetInput,
        ArrayMutationResult, ArrayRange, ArrayRangeInput, ArrayScan, ArrayScanInput,
        ArraySearchResult, ArraySummary, CreateArrayInput, DeleteArrayElementsInput,
        DeleteArrayRangeInput, KeyValue, SearchArrayInput, SetArrayElementInput,
    },
    error::AppError,
    redis::RedisOperations,
    AppState,
};

#[tauri::command]
pub async fn create_array(
    state: tauri::State<'_, AppState>,
    input: CreateArrayInput,
) -> Result<KeyValue, AppError> {
    state.redis.create_array(input).await
}

#[tauri::command]
pub async fn get_array_summary(
    state: tauri::State<'_, AppState>,
    input: ArrayKeyInput,
) -> Result<ArraySummary, AppError> {
    state.redis.get_array_summary(input).await
}

#[tauri::command]
pub async fn get_array_range(
    state: tauri::State<'_, AppState>,
    input: ArrayRangeInput,
) -> Result<ArrayRange, AppError> {
    state.redis.get_array_range(input).await
}

#[tauri::command]
pub async fn scan_array(
    state: tauri::State<'_, AppState>,
    input: ArrayScanInput,
) -> Result<ArrayScan, AppError> {
    state.redis.scan_array(input).await
}

#[tauri::command]
pub async fn get_array_elements(
    state: tauri::State<'_, AppState>,
    input: ArrayMultiGetInput,
) -> Result<Vec<Option<String>>, AppError> {
    state.redis.get_array_elements(input).await
}

#[tauri::command]
pub async fn set_array_element(
    state: tauri::State<'_, AppState>,
    input: SetArrayElementInput,
) -> Result<ArrayMutationResult, AppError> {
    state.redis.set_array_element(input).await
}

#[tauri::command]
pub async fn append_array_elements(
    state: tauri::State<'_, AppState>,
    input: AppendArrayInput,
) -> Result<ArrayMutationResult, AppError> {
    state.redis.append_array_elements(input).await
}

#[tauri::command]
pub async fn delete_array_elements(
    state: tauri::State<'_, AppState>,
    input: DeleteArrayElementsInput,
) -> Result<ArrayMutationResult, AppError> {
    state.redis.delete_array_elements(input).await
}

#[tauri::command]
pub async fn delete_array_range(
    state: tauri::State<'_, AppState>,
    input: DeleteArrayRangeInput,
) -> Result<ArrayMutationResult, AppError> {
    state.redis.delete_array_range(input).await
}

#[tauri::command]
pub async fn search_array(
    state: tauri::State<'_, AppState>,
    input: SearchArrayInput,
) -> Result<ArraySearchResult, AppError> {
    state.redis.search_array(input).await
}

#[tauri::command]
pub async fn aggregate_array(
    state: tauri::State<'_, AppState>,
    input: AggregateArrayInput,
) -> Result<crate::domain::ArrayAggregateResult, AppError> {
    state.redis.aggregate_array(input).await
}
