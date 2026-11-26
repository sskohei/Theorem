use tauri::State;

use crate::application::AppState;
use crate::domain::{
    AppSettings, AttachmentBinaryResponse, AttachmentInfo, BackupInfo, BootstrapState,
    DocumentDetail, DocumentFilter, DocumentSummary, Folder, ImportBatchResult,
    ImportedDocumentItem, StorageStats, Subject, Tag, UpdateDocumentRequest,
};
use crate::error::ApiError;

#[tauri::command]
pub async fn get_bootstrap_state(state: State<'_, AppState>) -> Result<BootstrapState, ApiError> {
    state.bootstrap().await.map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_subjects(state: State<'_, AppState>) -> Result<Vec<Subject>, ApiError> {
    state.list_subjects().await.map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_subject(
    state: State<'_, AppState>,
    name: String,
    category: String,
    description: Option<String>,
) -> Result<Subject, ApiError> {
    state
        .create_subject(&name, &category, description.as_deref())
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn update_subject(
    state: State<'_, AppState>,
    id: String,
    name: String,
    category: String,
    description: Option<String>,
) -> Result<Subject, ApiError> {
    state
        .update_subject(&id, &name, &category, description.as_deref())
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn delete_subject(state: State<'_, AppState>, id: String) -> Result<(), ApiError> {
    state.delete_subject(&id).await.map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_folders(state: State<'_, AppState>) -> Result<Vec<Folder>, ApiError> {
    state.list_folders().await.map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_folder(
    state: State<'_, AppState>,
    subject_id: String,
    parent_id: Option<String>,
    name: String,
) -> Result<Folder, ApiError> {
    state
        .create_folder(&subject_id, parent_id.as_deref(), &name)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn update_folder(
    state: State<'_, AppState>,
    id: String,
    name: String,
    parent_id: Option<String>,
) -> Result<Folder, ApiError> {
    state
        .update_folder(&id, &name, parent_id.as_deref())
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn delete_folder(state: State<'_, AppState>, id: String) -> Result<(), ApiError> {
    state.delete_folder(&id).await.map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_tags(state: State<'_, AppState>) -> Result<Vec<Tag>, ApiError> {
    state.list_tags().await.map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_tag(
    state: State<'_, AppState>,
    name: String,
    color: Option<String>,
) -> Result<Tag, ApiError> {
    state
        .create_tag(&name, color.as_deref())
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn delete_tag(state: State<'_, AppState>, id: String) -> Result<(), ApiError> {
    state.delete_tag(&id).await.map_err(ApiError::from)
}

#[tauri::command]
pub async fn set_document_tags(
    state: State<'_, AppState>,
    document_id: String,
    tag_ids: Vec<String>,
) -> Result<Vec<Tag>, ApiError> {
    state
        .set_document_tags(&document_id, &tag_ids)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_documents(
    state: State<'_, AppState>,
    filter: DocumentFilter,
) -> Result<Vec<DocumentSummary>, ApiError> {
    state.list_documents(&filter).await.map_err(ApiError::from)
}

#[tauri::command]
pub async fn get_document_detail(
    state: State<'_, AppState>,
    document_id: String,
) -> Result<DocumentDetail, ApiError> {
    state
        .get_document_detail(&document_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn update_document(
    state: State<'_, AppState>,
    input: UpdateDocumentRequest,
) -> Result<DocumentDetail, ApiError> {
    let folder_ref = if input.clear_folder {
        Some(None)
    } else {
        input.folder_id.as_deref().map(Some)
    };
    state
        .update_document(
            &input.id,
            input.title.as_deref(),
            input.subject_id.as_deref(),
            folder_ref,
            input.is_favorite,
            input.recognition_mode.as_deref(),
        )
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn touch_document_opened(
    state: State<'_, AppState>,
    document_id: String,
) -> Result<String, ApiError> {
    state
        .touch_document_opened(&document_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn delete_document(state: State<'_, AppState>, id: String) -> Result<(), ApiError> {
    state.delete_document(&id).await.map_err(ApiError::from)
}

#[tauri::command]
pub async fn import_files_from_paths(
    state: State<'_, AppState>,
    subject_id: String,
    folder_id: Option<String>,
    paths: Vec<String>,
) -> Result<ImportBatchResult, ApiError> {
    state
        .import_files_from_paths(&subject_id, folder_id.as_deref(), &paths)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn import_file_bytes(
    state: State<'_, AppState>,
    subject_id: String,
    folder_id: Option<String>,
    filename: String,
    base64_data: String,
) -> Result<ImportedDocumentItem, ApiError> {
    state
        .import_file_bytes(&subject_id, folder_id.as_deref(), &filename, &base64_data)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn read_attachment_base64(
    state: State<'_, AppState>,
    attachment_id: String,
) -> Result<AttachmentBinaryResponse, ApiError> {
    state
        .read_attachment_base64(&attachment_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn save_document_thumbnail(
    state: State<'_, AppState>,
    document_id: String,
    png_base64: String,
) -> Result<AttachmentInfo, ApiError> {
    state
        .save_document_thumbnail(&document_id, &png_base64)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn export_attachment(
    state: State<'_, AppState>,
    attachment_id: String,
    destination_path: String,
) -> Result<(), ApiError> {
    state
        .export_attachment(&attachment_id, &destination_path)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, ApiError> {
    state.get_settings().map_err(ApiError::from)
}

#[tauri::command]
pub async fn update_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, ApiError> {
    state.update_settings(&settings).map_err(ApiError::from)
}

#[tauri::command]
pub async fn get_storage_stats(state: State<'_, AppState>) -> Result<StorageStats, ApiError> {
    state.get_storage_stats().await.map_err(ApiError::from)
}

#[tauri::command]
pub async fn cleanup_temp_files(state: State<'_, AppState>) -> Result<u64, ApiError> {
    state.cleanup_temp_files().map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_backup(state: State<'_, AppState>) -> Result<BackupInfo, ApiError> {
    state.create_backup().await.map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_backups(state: State<'_, AppState>) -> Result<Vec<BackupInfo>, ApiError> {
    state.list_backups().map_err(ApiError::from)
}

#[tauri::command]
pub async fn restore_backup(
    state: State<'_, AppState>,
    backup_rel_path: String,
) -> Result<BootstrapState, ApiError> {
    state
        .restore_backup(&backup_rel_path)
        .await
        .map_err(ApiError::from)
}
