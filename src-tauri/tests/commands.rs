use redix_lib::commands::{browser, connections, workbench};

#[test]
fn exposes_all_tauri_command_adapters() {
    let _ = browser::scan_keys;
    let _ = browser::get_key;
    let _ = browser::set_key;
    let _ = browser::delete_key;
    let _ = browser::set_key_ttl;
    let _ = browser::create_key;
    let _ = browser::rename_key;
    let _ = browser::delete_keys;
    let _ = browser::get_key_info;
    let _ = connections::list_connections;
    let _ = connections::save_connection;
    let _ = connections::delete_connection;
    let _ = connections::test_connection;
    let _ = connections::open_connection;
    let _ = connections::close_connection;
    let _ = workbench::execute_command;
}
