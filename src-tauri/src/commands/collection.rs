use crate::{
    domain::collection::{
        CollectionEntry, CollectionMutationInput, CollectionPage, CollectionPageInput,
        ListIndexInput,
    },
    error::AppError,
    AppState,
};

#[tauri::command]
pub async fn get_collection_page(
    state: tauri::State<'_, AppState>,
    input: CollectionPageInput,
) -> Result<CollectionPage, AppError> {
    state.redis.get_collection_page(input).await
}

#[tauri::command]
pub async fn get_list_entry(
    state: tauri::State<'_, AppState>,
    input: ListIndexInput,
) -> Result<Option<CollectionEntry>, AppError> {
    state.redis.get_list_entry(input).await
}

#[tauri::command]
pub async fn mutate_collection(
    state: tauri::State<'_, AppState>,
    input: CollectionMutationInput,
) -> Result<(), AppError> {
    state.redis.mutate_collection(input).await
}
