mod commands;
mod config;
mod db;
mod domain;
mod error;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let state = tauri::async_runtime::block_on(commands::bootstrap(app.handle()))?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_boot_state,
            commands::test_db_connection,
            commands::set_db_config,
            commands::init_business,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
