pub mod analysis_history;
pub mod array;
pub mod browser;
pub mod cli;
pub mod connections;
pub mod database;
pub mod json;
pub mod observability;
pub mod query_library;
pub mod search;
pub mod settings;
pub mod vector_set;
pub mod workbench;

pub use array::{
    aggregate_array, append_array_elements, create_array, delete_array_elements,
    delete_array_range, get_array_elements, get_array_range, get_array_summary, scan_array,
    search_array, set_array_element,
};

pub use browser::{
    create_key, delete_key, delete_keys, export_keys, get_key, get_key_info, import_keys,
    rename_key, scan_keys, set_key, set_key_ttl,
};
pub use connections::{
    close_connection, delete_connection, export_connections, import_connections, list_connections,
    open_connection, save_connection, test_connection,
};
pub use database::{
    analyze_database, get_database_overview, get_instance_details, get_instance_overview,
    select_database,
};
pub use json::{
    append_json_array, delete_json_path, get_json_path, get_module_capabilities, set_json_path,
};
pub use query_library::{delete_query_library_item, list_query_library, save_query_library_item};
pub use search::{
    create_search_index, delete_search_index, get_key_search_indexes, get_search_index,
    list_search_indexes, search_keys,
};
pub use settings::{get_app_settings, save_app_settings};
pub use vector_set::{
    add_vector_set_elements, create_vector_set, delete_vector_set_attributes,
    delete_vector_set_elements, download_vector_embedding, get_vector_set_element,
    get_vector_set_summary, list_vector_set_elements, search_vector_set, set_vector_set_attributes,
};
pub use workbench::{
    execute_command, execute_commands, get_command_catalog, list_command_history,
    save_command_history,
};
pub mod collection;
pub mod search_aggregate;
pub mod stream_entries;
