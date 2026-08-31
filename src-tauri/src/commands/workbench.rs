use crate::{
    domain::{
        filter_history_entry, ClearCommandHistoryInput, CommandDefinition, CommandExecutionItem,
        CommandHistoryDocument, CommandHistoryEntry, CommandResult, DeleteCommandHistoryInput,
        ExecuteCommandInput, ExecuteCommandsInput, SaveCommandHistoryInput,
    },
    error::AppError,
    persistence::JsonDocumentStore,
    redis::RedisOperations,
    AppState,
};

static HISTORY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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

#[tauri::command]
pub async fn execute_commands(
    state: tauri::State<'_, AppState>,
    input: ExecuteCommandsInput,
) -> Result<Vec<CommandExecutionItem>, AppError> {
    state.redis.execute_commands(input).await
}

#[tauri::command]
pub fn get_command_catalog(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<CommandDefinition>, AppError> {
    Ok(state.redis.command_catalog())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_command_history(
    state: tauri::State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<CommandHistoryEntry>, AppError> {
    if connection_id.trim().is_empty() {
        return Err(AppError::InvalidConnection);
    }

    let store = history_store(state.inner());
    let _guard = HISTORY_LOCK
        .lock()
        .map_err(|_| AppError::PersistenceFailed)?;
    let document = store.load_or_default::<CommandHistoryDocument>()?;
    let mut entries = document
        .entries
        .iter()
        .filter(|entry| entry.connection_id == connection_id)
        .filter_map(filter_history_entry)
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    entries.truncate(100);
    Ok(entries)
}

#[tauri::command]
pub fn save_command_history(
    state: tauri::State<'_, AppState>,
    input: SaveCommandHistoryInput,
) -> Result<(), AppError> {
    input.validate()?;
    let _guard = HISTORY_LOCK
        .lock()
        .map_err(|_| AppError::PersistenceFailed)?;
    let store = history_store(state.inner());
    let mut document = store.load_or_default::<CommandHistoryDocument>()?;
    document.entries.retain(|entry| {
        entry.connection_id != input.connection_id && filter_history_entry(entry).is_some()
    });
    document
        .entries
        .extend(input.entries.iter().filter_map(filter_history_entry));
    document.entries.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then_with(|| right.command.cmp(&left.command))
    });
    store.save(&document)
}

#[tauri::command]
pub fn delete_command_history(
    state: tauri::State<'_, AppState>,
    input: DeleteCommandHistoryInput,
) -> Result<(), AppError> {
    let _guard = HISTORY_LOCK
        .lock()
        .map_err(|_| AppError::PersistenceFailed)?;
    let store = history_store(state.inner());
    let mut document = store.load_or_default::<CommandHistoryDocument>()?;
    document.delete_entry(&input)?;
    store.save(&document)
}

#[tauri::command]
pub fn clear_command_history(
    state: tauri::State<'_, AppState>,
    input: ClearCommandHistoryInput,
) -> Result<(), AppError> {
    let _guard = HISTORY_LOCK
        .lock()
        .map_err(|_| AppError::PersistenceFailed)?;
    let store = history_store(state.inner());
    let mut document = store.load_or_default::<CommandHistoryDocument>()?;
    document.clear_connection(&input)?;
    store.save(&document)
}

fn history_store(state: &AppState) -> JsonDocumentStore {
    JsonDocumentStore::new(state.data_dir.join("workbench-history.json"))
}
