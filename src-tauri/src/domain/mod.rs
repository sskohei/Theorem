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
pub struct PdfSettings {
    #[serde(default = "default_pdf_template")]
    pub default_template: String,
    #[serde(default = "default_paper_size")]
    pub paper_size: String,
    #[serde(default = "default_true")]
    pub show_model_name: bool,
    #[serde(default = "default_true")]
    pub show_generated_at: bool,
    #[serde(default = "default_true")]
    pub show_version_number: bool,
}

fn default_pdf_template() -> String {
    "auto".to_string()
}

fn default_paper_size() -> String {
    "a4".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for PdfSettings {
    fn default() -> Self {
        Self {
            default_template: default_pdf_template(),
            paper_size: default_paper_size(),
            show_model_name: true,
            show_generated_at: true,
            show_version_number: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentPurposeAssignments {
    pub extraction_profile_id: Option<String>,
    pub solution_profile_id: Option<String>,
    pub chat_profile_id: Option<String>,
    pub verification_profile_id: Option<String>,
    pub revision_profile_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PromptSettings {
    #[serde(default)]
    pub mathematics_custom_instructions: String,
    #[serde(default)]
    pub physics_custom_instructions: String,
    #[serde(default)]
    pub mode_custom_instructions: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScienceMcpSettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub uv_path: Option<String>,
    #[serde(default)]
    pub server_dir: Option<String>,
    #[serde(default = "default_science_timeout")]
    pub timeout_seconds: u32,
    #[serde(default = "default_plot_format")]
    pub default_plot_format: String,
}

fn default_science_timeout() -> u32 {
    10
}

fn default_plot_format() -> String {
    "svg".to_string()
}

impl Default for ScienceMcpSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            uv_path: None,
            server_dir: None,
            timeout_seconds: default_science_timeout(),
            default_plot_format: default_plot_format(),
        }
    }
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
    #[serde(default)]
    pub cloud_ai_consent_accepted: bool,
    #[serde(default)]
    pub pdf: PdfSettings,
    #[serde(default)]
    pub agent_assignments: AgentPurposeAssignments,
    #[serde(default)]
    pub prompts: PromptSettings,
    #[serde(default)]
    pub science_mcp: ScienceMcpSettings,
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
            cloud_ai_consent_accepted: false,
            pdf: PdfSettings::default(),
            agent_assignments: AgentPurposeAssignments::default(),
            prompts: PromptSettings::default(),
            science_mcp: ScienceMcpSettings::default(),
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

// ============================================================================
// Phase 2: Problems, Solutions, PDF Compilation, Chat, and AI Agents
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub id: String,
    pub document_id: String,
    pub problem_number: String,
    pub subproblem_number: Option<String>,
    pub sort_order: i64,
    pub subject_category: String,
    pub field: Option<String>,
    pub statement_markdown: String,
    pub given_conditions: Vec<String>,
    pub target_objective: Option<String>,
    pub source_pages: Vec<i64>,
    pub figure_refs: Vec<String>,
    pub uncertainties: Vec<String>,
    pub is_user_verified: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertProblemRequest {
    pub id: Option<String>,
    pub document_id: String,
    pub problem_number: String,
    pub subproblem_number: Option<String>,
    pub subject_category: String,
    pub field: Option<String>,
    pub statement_markdown: String,
    #[serde(default)]
    pub given_conditions: Vec<String>,
    pub target_objective: Option<String>,
    #[serde(default)]
    pub source_pages: Vec<i64>,
    #[serde(default)]
    pub figure_refs: Vec<String>,
    #[serde(default)]
    pub uncertainties: Vec<String>,
    #[serde(default)]
    pub is_user_verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Solution {
    pub id: String,
    pub document_id: String,
    pub problem_id: Option<String>,
    pub version_number: i64,
    pub parent_solution_id: Option<String>,
    pub is_current: bool,
    pub mode: String,
    pub schema_version: i64,
    pub structured_json: String,
    pub validation_status: String,
    pub typst_source_attachment_id: Option<String>,
    pub pdf_attachment_id: Option<String>,
    pub agent_run_id: Option<String>,
    pub model_name: Option<String>,
    pub change_summary: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaValidationIssue {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LatexConversionError {
    pub location: String,
    pub step_index: Option<usize>,
    pub latex: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolutionOperationResult {
    pub solution: Solution,
    pub schema_issues: Vec<SchemaValidationIssue>,
    pub latex_errors: Vec<LatexConversionError>,
    pub pdf_error: Option<String>,
    pub raw_output: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageImagePayload {
    pub page_number: i64,
    pub png_base64: String,
    pub extracted_text: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecognizeProblemsRequest {
    pub document_id: String,
    pub agent_profile_id: Option<String>,
    pub model_name: Option<String>,
    pub recognition_mode: String,
    pub page_range_start: Option<i64>,
    pub page_range_end: Option<i64>,
    #[serde(default)]
    pub page_images: Vec<PageImagePayload>,
    pub cropped_region_png_base64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecognizeProblemsResult {
    pub problems: Vec<Problem>,
    pub schema_issues: Vec<SchemaValidationIssue>,
    pub raw_output: Option<String>,
    pub run_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateSolutionRequest {
    pub document_id: String,
    pub problem_id: Option<String>,
    pub agent_profile_id: Option<String>,
    pub model_name: Option<String>,
    pub mode: String,
    pub target_scope: String,
    pub page_range_start: Option<i64>,
    pub page_range_end: Option<i64>,
    pub repair_feedback: Option<String>,
    pub parent_solution_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSolutionEditRequest {
    pub document_id: String,
    pub problem_id: Option<String>,
    pub parent_solution_id: Option<String>,
    pub mode: String,
    pub structured_json: String,
    pub change_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub document_id: String,
    pub problem_id: Option<String>,
    pub solution_id: Option<String>,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content_markdown: String,
    pub status: String,
    pub agent_run_id: Option<String>,
    pub referenced_problem_id: Option<String>,
    pub attachment_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendChatMessageRequest {
    pub conversation_id: String,
    pub document_id: String,
    pub agent_profile_id: Option<String>,
    pub model_name: Option<String>,
    pub content_markdown: String,
    pub referenced_problem_id: Option<String>,
    #[serde(default)]
    pub include_problem_context: bool,
    #[serde(default)]
    pub include_solution_context: bool,
    #[serde(default)]
    pub attached_images: Vec<ChatImageUpload>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatImageUpload {
    pub filename: String,
    pub mime_type: String,
    pub base64_data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendChatMessageResult {
    pub run_id: String,
    pub user_message: ChatMessage,
    pub assistant_message: ChatMessage,
    pub resumed_external_session: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    pub supports_image_input: bool,
    pub supports_pdf_input: bool,
    pub supports_streaming: bool,
    pub supports_session_resume: bool,
    pub supports_model_listing: bool,
    pub supports_auth_status_check: bool,
    pub supports_mcp_scoped_config: bool,
    pub supports_permission_hooks: bool,
    pub supports_sandbox_restriction: bool,
    pub supports_cancel: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentModelInfo {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus {
    pub provider_kind: String,
    pub installed: bool,
    pub resolved_path: Option<String>,
    pub version: Option<String>,
    /// `"authenticated"` | `"unauthenticated"` | `"unknown"` | `"terms_restricted"`
    pub auth_status: String,
    pub auth_method: Option<String>,
    /// `"verified_live"` | `"verified_mock"` | `"unverified"`
    pub verification_status: String,
    pub available_models: Option<Vec<AgentModelInfo>>,
    pub capabilities: AgentCapabilities,
    pub auth_guide: String,
    pub compliance_note: String,
    pub detail_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfile {
    pub id: String,
    pub name: String,
    pub provider_kind: String,
    pub executable_path: Option<String>,
    pub args: Vec<String>,
    pub default_model: Option<String>,
    pub auth_mode: String,
    pub has_keyring_secret: bool,
    pub is_enabled: bool,
    pub is_default: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAgentProfileRequest {
    pub id: Option<String>,
    pub name: String,
    pub provider_kind: String,
    pub executable_path: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    pub default_model: Option<String>,
    pub auth_mode: String,
    #[serde(default = "default_true")]
    pub is_enabled: bool,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEventEnvelope {
    pub event_id: String,
    pub run_id: String,
    pub session_id: String,
    pub message_id: Option<String>,
    pub timestamp: String,
    pub event: AgentEventPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentEventPayload {
    SessionStarted {
        #[serde(rename = "externalSessionId")]
        external_session_id: Option<String>,
        resumed: bool,
    },
    MessageStarted {
        role: String,
    },
    TextDelta {
        text: String,
    },
    ToolCallStarted {
        #[serde(rename = "callId")]
        call_id: String,
        server: Option<String>,
        #[serde(rename = "toolName")]
        tool_name: String,
        arguments: serde_json::Value,
    },
    ToolCallUpdated {
        #[serde(rename = "callId")]
        call_id: String,
        status: String,
        progress: Option<String>,
    },
    ToolCallCompleted {
        #[serde(rename = "callId")]
        call_id: String,
        succeeded: bool,
        output: serde_json::Value,
    },
    PermissionRequested {
        #[serde(rename = "requestId")]
        request_id: String,
        action: String,
        target: Option<String>,
        detail: String,
    },
    ArtifactCreated {
        #[serde(rename = "relativePath")]
        relative_path: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
        caption: Option<String>,
    },
    UsageUpdated {
        #[serde(rename = "inputTokens")]
        input_tokens: Option<u64>,
        #[serde(rename = "outputTokens")]
        output_tokens: Option<u64>,
        #[serde(rename = "rawLabel")]
        raw_label: Option<String>,
    },
    MessageCompleted {
        #[serde(rename = "finalText")]
        final_text: Option<String>,
    },
    Cancelled {
        reason: Option<String>,
    },
    Failed {
        code: String,
        message: String,
        #[serde(rename = "rateLimited")]
        rate_limited: bool,
        #[serde(rename = "authRequired")]
        auth_required: bool,
    },
}

// ============================================================================
// Phase 3: Science MCP Server, Tool Executions, Verifications & Artifacts
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScienceMcpToolInfo {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScienceMcpStatus {
    pub enabled: bool,
    pub uv_available: bool,
    pub uv_path: Option<String>,
    pub uv_version: Option<String>,
    pub server_dir: Option<String>,
    pub server_ready: bool,
    pub tool_count: usize,
    pub tools: Vec<ScienceMcpToolInfo>,
    pub security_note: String,
    pub detail_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecutionRecord {
    pub id: String,
    pub agent_run_id: Option<String>,
    pub document_id: Option<String>,
    pub solution_id: Option<String>,
    pub server_name: String,
    pub tool_name: String,
    pub arguments_json: String,
    pub result_json: Option<String>,
    /// `"running"` | `"succeeded"` | `"failed"` | `"timeout"` | `"cancelled"`
    pub status: String,
    pub duration_ms: Option<i64>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VerificationResultRecord {
    pub id: String,
    pub solution_id: String,
    pub tool_execution_id: Option<String>,
    /// `"science_mcp"` | `"ai_self_check"` | `"user_check"`
    pub source_kind: String,
    pub target_description: String,
    pub assumptions: Vec<String>,
    pub method: String,
    /// `"passed"` | `"failed"` | `"inconclusive"` | `"not_run"` | `"error"`
    pub status: String,
    pub detail_json: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SolutionArtifactItem {
    pub id: String,
    pub solution_id: String,
    pub attachment_id: String,
    /// `"plot"` | `"diagram"` | `"crop"` | `"table"`
    pub artifact_type: String,
    pub caption: Option<String>,
    pub source_tool_execution_id: Option<String>,
    pub sort_order: i64,
    pub relative_path: String,
    pub mime_type: String,
    pub original_filename: String,
    pub byte_size: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunScienceToolRequest {
    pub document_id: String,
    pub solution_id: Option<String>,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    #[serde(default)]
    pub attach_to_solution_pdf: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunScienceToolResult {
    pub execution: ToolExecutionRecord,
    pub verification: Option<VerificationResultRecord>,
    pub created_artifacts: Vec<AttachmentInfo>,
    pub updated_solution: Option<SolutionOperationResult>,
}
