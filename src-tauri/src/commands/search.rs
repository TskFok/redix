use crate::{
    domain::{
        CreateSearchIndexInput, GetKeySearchIndexesInput, KeySearchIndexSummary,
        ListSearchIndexesResult, SearchIndexInfo, SearchIndexInput, SearchQueryInput,
        SearchQueryResult,
    },
    error::AppError,
    redis::RedisOperations,
    AppState,
};

#[tauri::command(rename_all = "snake_case")]
pub async fn list_search_indexes(
    state: tauri::State<'_, AppState>,
    connection_id: String,
) -> Result<ListSearchIndexesResult, AppError> {
    state.redis.list_search_indexes(&connection_id).await
}

#[tauri::command]
pub async fn create_search_index(
    state: tauri::State<'_, AppState>,
    input: CreateSearchIndexInput,
) -> Result<(), AppError> {
    state.redis.create_search_index(input).await
}

#[tauri::command]
pub async fn get_search_index(
    state: tauri::State<'_, AppState>,
    input: SearchIndexInput,
) -> Result<SearchIndexInfo, AppError> {
    state.redis.get_search_index(input).await
}

#[tauri::command]
pub async fn delete_search_index(
    state: tauri::State<'_, AppState>,
    input: SearchIndexInput,
) -> Result<(), AppError> {
    state.redis.delete_search_index(input).await
}

#[tauri::command]
pub async fn search_keys(
    state: tauri::State<'_, AppState>,
    input: SearchQueryInput,
) -> Result<SearchQueryResult, AppError> {
    state.redis.search_keys(input).await
}

#[tauri::command]
pub async fn get_key_search_indexes(
    state: tauri::State<'_, AppState>,
    input: GetKeySearchIndexesInput,
) -> Result<Vec<KeySearchIndexSummary>, AppError> {
    state.redis.get_key_search_indexes(input).await
}

#[tauri::command]
pub async fn search_vector_index(
    state: tauri::State<'_, AppState>,
    input: crate::domain::SearchVectorQueryInput,
) -> Result<crate::domain::SearchVectorQueryResult, AppError> {
    state.redis.search_vector_index(input).await
}
