pub mod agents;
pub mod application;
pub mod commands;
pub mod database;
pub mod document;
pub mod domain;
pub mod error;
pub mod gdrive;
pub mod pdf;
pub mod prompts;
pub mod science_mcp;
pub mod security;
pub mod storage;

use std::sync::Arc;
use tauri::{Emitter, Manager};

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
            let handle = app.handle().clone();
            let drive_handle = app.handle().clone();
            tauri::async_runtime::block_on(async {
                state
                    .set_agent_event_emitter(Arc::new(move |env| {
                        let _ = handle.emit("agent://event", env);
                    }))
                    .await;
                state
                    .set_drive_event_emitter(Arc::new(move || {
                        let _ = drive_handle.emit("drive://updated", ());
                    }))
                    .await;
                state.spawn_drive_background_worker();
            });
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
            commands::list_agent_profiles,
            commands::save_agent_profile,
            commands::delete_agent_profile,
            commands::check_agent_status,
            commands::detect_all_agents,
            commands::set_agent_api_key,
            commands::delete_agent_api_key,
            commands::cancel_agent_run,
            commands::respond_agent_permission,
            commands::list_problems,
            commands::upsert_problem,
            commands::delete_problem,
            commands::split_problem,
            commands::merge_problems,
            commands::recognize_problems,
            commands::list_solutions,
            commands::generate_solution,
            commands::save_solution_edit,
            commands::recompile_solution_pdf,
            commands::restore_solution_version,
            commands::list_conversations,
            commands::create_conversation,
            commands::delete_conversation,
            commands::list_messages,
            commands::send_chat_message,
            commands::check_science_mcp_status,
            commands::list_document_tool_executions,
            commands::list_solution_verifications,
            commands::list_solution_artifacts,
            commands::run_science_tool,
            commands::attach_artifact_to_solution,
            commands::get_drive_overview,
            commands::save_drive_oauth_config,
            commands::connect_google_drive,
            commands::disconnect_google_drive,
            commands::update_drive_settings,
            commands::set_folder_auto_upload,
            commands::upload_document_to_drive,
            commands::list_document_drive_status,
            commands::retry_drive_job,
            commands::cancel_drive_job,
            commands::resolve_drive_conflict,
            commands::process_drive_queue_now,
            commands::propose_solution_revision,
            commands::decide_solution_revision,
            commands::compare_solution_versions,
            commands::compare_agents_generate_solutions,
            commands::list_reference_materials,
            commands::create_reference_material,
            commands::delete_reference_material,
            commands::full_text_search,
            commands::rebuild_search_index,
        ]);

    if let Err(err) = builder.run(tauri::generate_context!()) {
        eprintln!("Theorem failed to start: {err}");
        std::process::exit(1);
    }
}
