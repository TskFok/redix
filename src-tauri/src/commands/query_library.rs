use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    domain::{
        normalize_query_library_item, QueryLibraryDocument, QueryLibraryItem, QueryLibraryItemInput,
    },
    error::AppError,
    persistence::JsonDocumentStore,
    AppState,
};

static QUERY_ID_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[tauri::command]
pub fn list_query_library(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<QueryLibraryItem>, AppError> {
    list_query_library_inner(state.inner())
}

#[tauri::command]
pub fn save_query_library_item(
    state: tauri::State<'_, AppState>,
    input: QueryLibraryItemInput,
) -> Result<QueryLibraryItem, AppError> {
    save_query_library_item_inner(state.inner(), input)
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_query_library_item(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), AppError> {
    delete_query_library_item_inner(state.inner(), id)
}

pub fn list_query_library_inner(state: &AppState) -> Result<Vec<QueryLibraryItem>, AppError> {
    let mut items = query_library_store(state)
        .load_or_default::<QueryLibraryDocument>()?
        .items;
    items.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(items)
}

pub fn save_query_library_item_inner(
    state: &AppState,
    input: QueryLibraryItemInput,
) -> Result<QueryLibraryItem, AppError> {
    input.validate()?;
    let store = query_library_store(state);
    let mut document = store.load_or_default::<QueryLibraryDocument>()?;
    let existing_index = input
        .id
        .as_deref()
        .and_then(|id| document.items.iter().position(|item| item.id == id));

    if existing_index.is_none() && document.items.len() >= 500 {
        return Err(AppError::InvalidConnection);
    }

    let id = existing_index
        .map(|index| document.items[index].id.clone())
        .unwrap_or_else(next_query_id);
    let item = normalize_query_library_item(QueryLibraryItem {
        id,
        name: input.name,
        command: input.command,
        tags: input.tags,
        updated_at: current_unix_millis(),
    })?;

    if let Some(index) = existing_index {
        document.items[index] = item.clone();
    } else {
        document.items.push(item.clone());
    }
    document.items.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    store.save(&document)?;
    Ok(item)
}

pub fn delete_query_library_item_inner(state: &AppState, id: String) -> Result<(), AppError> {
    if id.trim().is_empty() {
        return Err(AppError::InvalidConnection);
    }

    let store = query_library_store(state);
    let mut document = store.load_or_default::<QueryLibraryDocument>()?;
    let Some(index) = document.items.iter().position(|item| item.id == id) else {
        return Err(AppError::InvalidConnection);
    };
    document.items.remove(index);
    store.save(&document)
}

fn query_library_store(state: &AppState) -> JsonDocumentStore {
    JsonDocumentStore::new(state.data_dir.join("query-library.json"))
}

fn next_query_id() -> String {
    format!(
        "query-{}-{}",
        current_unix_millis(),
        QUERY_ID_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}
