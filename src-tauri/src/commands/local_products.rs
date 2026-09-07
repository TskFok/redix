use std::{
    collections::HashSet,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    domain::{
        local_products::*, normalize_query_library_item, QueryLibraryDocument, QueryLibraryItem,
    },
    error::AppError,
    persistence::JsonDocumentStore,
    AppState,
};

// Shared with the existing individual query CRUD commands: a package append and
// an editor save must never race between loading and replacing the same file.
pub(crate) static QUERY_LIBRARY_LOCK: Mutex<()> = Mutex::new(());
static CONNECTION_TAGS_LOCK: Mutex<()> = Mutex::new(());

#[tauri::command]
pub fn list_connection_tags(state: tauri::State<'_, AppState>) -> Result<ConnectionTags, AppError> {
    list_connection_tags_inner(state.inner())
}

#[tauri::command]
pub fn save_connection_tags(
    state: tauri::State<'_, AppState>,
    input: SaveConnectionTagsInput,
) -> Result<Vec<ConnectionTag>, AppError> {
    save_connection_tags_inner(state.inner(), input)
}

#[tauri::command]
pub fn export_query_package(state: tauri::State<'_, AppState>) -> Result<QueryPackage, AppError> {
    export_query_package_inner(state.inner())
}

#[tauri::command]
pub fn import_query_package(
    state: tauri::State<'_, AppState>,
    content: String,
) -> Result<Vec<QueryLibraryItem>, AppError> {
    import_query_package_inner(state.inner(), content)
}

pub fn list_connection_tags_inner(state: &AppState) -> Result<ConnectionTags, AppError> {
    let _lock = CONNECTION_TAGS_LOCK
        .lock()
        .map_err(|_| AppError::PersistenceFailed)?;
    let profiles = state.profiles.load()?;
    let ids: HashSet<_> = profiles.iter().map(|profile| profile.id.as_str()).collect();
    let mut document = tags_store(state).load::<ConnectionTagsDocument>()?;
    document
        .connections
        .retain(|id, _| ids.contains(id.as_str()));
    Ok(document.connections)
}

pub fn save_connection_tags_inner(
    state: &AppState,
    input: SaveConnectionTagsInput,
) -> Result<Vec<ConnectionTag>, AppError> {
    let tags = normalize_connection_tags(input.tags)?;
    let _lock = CONNECTION_TAGS_LOCK
        .lock()
        .map_err(|_| AppError::PersistenceFailed)?;
    let profiles = state.profiles.load()?;
    let ids: HashSet<_> = profiles.iter().map(|profile| profile.id.as_str()).collect();
    if !ids.contains(input.connection_id.as_str()) {
        return Err(AppError::InvalidInput);
    }
    let store = tags_store(state);
    let mut document = store.load::<ConnectionTagsDocument>()?;
    document
        .connections
        .retain(|id, _| ids.contains(id.as_str()));
    if tags.is_empty() {
        document.connections.remove(&input.connection_id);
    } else {
        document
            .connections
            .insert(input.connection_id, tags.clone());
    }
    if document.connections.len() > 10_000 {
        return Err(AppError::InvalidInput);
    }
    store.save(&document)?;
    Ok(tags)
}

pub fn export_query_package_inner(state: &AppState) -> Result<QueryPackage, AppError> {
    let _lock = QUERY_LIBRARY_LOCK
        .lock()
        .map_err(|_| AppError::PersistenceFailed)?;
    let document = query_store(state).load::<QueryLibraryDocument>()?;
    let package = QueryPackage {
        format: "redix-query-library".into(),
        version: 1,
        items: document
            .items
            .into_iter()
            .map(|item| QueryPackageItem {
                name: item.name,
                command: item.command,
                tags: item.tags,
            })
            .collect(),
    };
    package.validate()?;
    if serde_json::to_vec_pretty(&package)
        .map_err(|_| AppError::PersistenceFailed)?
        .len()
        > MAX_QUERY_PACKAGE_BYTES
    {
        return Err(AppError::InvalidInput);
    }
    Ok(package)
}

pub fn import_query_package_inner(
    state: &AppState,
    content: String,
) -> Result<Vec<QueryLibraryItem>, AppError> {
    let package = QueryPackage::parse(&content)?;
    let _lock = QUERY_LIBRARY_LOCK
        .lock()
        .map_err(|_| AppError::PersistenceFailed)?;
    let store = query_store(state);
    let mut document = store.load::<QueryLibraryDocument>()?;
    if document.items.len().saturating_add(package.items.len()) > MAX_QUERY_LIBRARY_ITEMS {
        return Err(AppError::InvalidInput);
    }
    let updated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default();
    let items = package
        .items
        .into_iter()
        .map(|item| {
            normalize_query_library_item(QueryLibraryItem {
                id: format!("query-{}", uuid::Uuid::new_v4()),
                name: item.name,
                command: item.command,
                tags: item.tags,
                updated_at,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if items.is_empty() {
        return Ok(items);
    }
    document.items.extend(items.iter().cloned());
    document.items.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    // No writes occur until the complete package and total capacity are valid.
    store.save(&document)?;
    Ok(items)
}

fn tags_store(state: &AppState) -> JsonDocumentStore {
    JsonDocumentStore::new(state.data_dir.join("connection-tags.json"))
}
fn query_store(state: &AppState) -> JsonDocumentStore {
    JsonDocumentStore::new(state.data_dir.join("query-library.json"))
}
