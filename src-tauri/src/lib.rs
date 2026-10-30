mod backup;
mod cash;
mod catalog;
mod commands;
mod config;
mod db;
mod domain;
mod error;
mod inventory;
mod purchases;
mod reports;
mod sales;
mod util;

#[cfg(test)]
mod phase2_tests;
#[cfg(test)]
mod phase3_tests;
#[cfg(test)]
mod phase4_tests;
#[cfg(test)]
mod phase5_tests;
#[cfg(test)]
mod phase6_tests;

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
            commands::list_suppliers,
            commands::save_supplier,
            commands::delete_supplier,
            commands::list_purchases,
            commands::get_purchase,
            commands::save_purchase,
            commands::confirm_purchase,
            commands::cancel_purchase_draft,
            commands::current_cash,
            commands::open_cash,
            commands::add_cash_movement,
            commands::close_cash,
            commands::list_cash_sessions,
            commands::get_cash_session,
            commands::dashboard,
            commands::sales_report,
            commands::top_products,
            commands::export_csv,
            commands::get_auto_backup_dir,
            commands::set_auto_backup_dir,
            commands::get_app_info,
            commands::export_backup,
            commands::export_sqlite_native,
            commands::inspect_backup,
            commands::restore_backup,
            commands::confirm_sale,
            commands::cancel_sale,
            commands::list_sales,
            commands::get_sale,
            commands::adjust_stock,
            commands::get_kardex,
            commands::check_stock_integrity,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
