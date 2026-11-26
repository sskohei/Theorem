use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Subject {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: String,
    pub subject_id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub auto_upload_drive: bool,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentInfo {
    pub id: String,
    pub document_id: Option<String>,
    pub kind: String,
    pub original_filename: String,
    pub relative_path: String,
    pub mime_type: String,
    pub byte_size: i64,
    pub sha256: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub page_count: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSummary {
    pub id: String,
    pub subject_id: String,
    pub folder_id: Option<String>,
    pub title: String,
    pub source_type: String,
    pub primary_attachment_id: Option<String>,
    pub thumbnail_attachment_id: Option<String>,
    pub page_count: Option<i64>,
    pub recognition_mode: String,
    pub study_status: String,
    pub is_favorite: bool,
    pub last_opened_at: Option<String>,
    pub tag_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentDetail {
    pub document: DocumentSummary,
    pub attachments: Vec<AttachmentInfo>,
    pub tags: Vec<Tag>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DocumentFilter {
    pub subject_id: Option<String>,
    pub folder_id: Option<String>,
    pub tag_id: Option<String>,
    pub search_query: Option<String>,
    pub favorites_only: bool,
    pub recent_only: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDocumentRequest {
    pub id: String,
    pub title: Option<String>,
    pub subject_id: Option<String>,
    pub folder_id: Option<String>,
    #[serde(default)]
    pub clear_folder: bool,
    pub is_favorite: Option<bool>,
    pub recognition_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateMatch {
    pub document_id: String,
    pub document_title: String,
    pub attachment_id: String,
    pub original_filename: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedDocumentItem {
    pub detail: DocumentDetail,
    pub duplicate_matches: Vec<DuplicateMatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportFailure {
    pub filename: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBatchResult {
    pub imported: Vec<ImportedDocumentItem>,
    pub failures: Vec<ImportFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub language: String,
    pub theme: String,
    pub default_subject_id: Option<String>,
    pub default_recognition_mode: String,
    pub left_sidebar_width: u32,
    pub right_sidebar_width: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: "ja".to_string(),
            theme: "system".to_string(),
            default_subject_id: None,
            default_recognition_mode: "printed_only".to_string(),
            left_sidebar_width: 280,
            right_sidebar_width: 340,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageStats {
    pub data_root_display: String,
    pub database_bytes: u64,
    pub originals_bytes: u64,
    pub thumbnails_bytes: u64,
    pub backups_bytes: u64,
    pub logs_bytes: u64,
    pub temp_bytes: u64,
    pub total_bytes: u64,
    pub document_count: i64,
    pub attachment_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub filename: String,
    pub relative_path: String,
    pub byte_size: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapState {
    pub app_version: String,
    pub data_root_display: String,
    pub subjects: Vec<Subject>,
    pub folders: Vec<Folder>,
    pub tags: Vec<Tag>,
    pub documents: Vec<DocumentSummary>,
    pub settings: AppSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentBinaryResponse {
    pub attachment: AttachmentInfo,
    pub base64_data: String,
}
