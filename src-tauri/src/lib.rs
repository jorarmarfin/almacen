mod catalog;
mod commands;
mod config;
mod db;
mod domain;
mod error;
mod inventory;
mod util;

#[cfg(test)]
mod phase2_tests;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
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
            commands::list_categories,
            commands::save_category,
            commands::delete_category,
            commands::list_products,
            commands::save_product,
            commands::set_product_active,
            commands::import_products_csv,
            commands::get_products_template,
            commands::write_text_file,
            commands::adjust_stock,
            commands::get_kardex,
            commands::check_stock_integrity,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
