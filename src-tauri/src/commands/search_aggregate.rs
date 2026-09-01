use crate::{
    domain::search_aggregate::{SearchAggregateInput, SearchAggregateResult},
    error::AppError,
    AppState,
};

#[tauri::command]
pub async fn aggregate_search(
    state: tauri::State<'_, AppState>,
    input: SearchAggregateInput,
) -> Result<SearchAggregateResult, AppError> {
    state.redis.aggregate_search(input).await
}
