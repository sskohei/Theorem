pub mod application;
pub mod commands;
pub mod database;
pub mod document;
pub mod domain;
pub mod error;
pub mod security;
pub mod storage;

use tauri::Manager;

use crate::application::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_root = app
                .path()
                .app_data_dir()
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            let state = tauri::async_runtime::block_on(AppState::initialize(data_root))
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_bootstrap_state,
            commands::list_subjects,
            commands::create_subject,
            commands::update_subject,
            commands::delete_subject,
            commands::list_folders,
            commands::create_folder,
            commands::update_folder,
            commands::delete_folder,
            commands::list_tags,
            commands::create_tag,
            commands::delete_tag,
            commands::set_document_tags,
            commands::list_documents,
            commands::get_document_detail,
            commands::update_document,
            commands::touch_document_opened,
            commands::delete_document,
            commands::import_files_from_paths,
            commands::import_file_bytes,
            commands::read_attachment_base64,
            commands::save_document_thumbnail,
            commands::export_attachment,
            commands::get_settings,
            commands::update_settings,
            commands::get_storage_stats,
            commands::cleanup_temp_files,
            commands::create_backup,
            commands::list_backups,
            commands::restore_backup,
        ]);

    if let Err(err) = builder.run(tauri::generate_context!()) {
        eprintln!("FormulaDesk failed to start: {err}");
        std::process::exit(1);
    }
}
