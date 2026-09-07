use tauri::State;

use crate::{domain::ClusterTopology, error::AppError, redis::RedisOperations, AppState};

#[tauri::command]
pub async fn get_cluster_topology(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<ClusterTopology, AppError> {
    state.redis.get_cluster_topology(&connection_id).await
}

#[tauri::command]
pub async fn refresh_cluster_topology(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<ClusterTopology, AppError> {
    state.redis.refresh_cluster_topology(&connection_id).await
}
