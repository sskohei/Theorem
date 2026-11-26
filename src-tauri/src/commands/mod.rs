use tauri::State;

use crate::agents::adapter::PermissionDecision;
use crate::application::AppState;
use crate::domain::{
    AgentProfile, AgentStatus, AppSettings, AttachmentBinaryResponse, AttachmentInfo, BackupInfo,
    BootstrapState, ChatMessage, Conversation, DocumentDetail, DocumentFilter, DocumentSummary,
    Folder, GenerateSolutionRequest, ImportBatchResult, ImportedDocumentItem, Problem,
    RecognizeProblemsRequest, RecognizeProblemsResult, RunScienceToolRequest, RunScienceToolResult,
    SaveAgentProfileRequest, SaveSolutionEditRequest, ScienceMcpStatus, SendChatMessageRequest,
    SendChatMessageResult, Solution, SolutionArtifactItem, SolutionOperationResult, StorageStats,
    Subject, Tag, ToolExecutionRecord, UpdateDocumentRequest, UpsertProblemRequest,
    VerificationResultRecord,
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

// ============================================================================
// Phase 2: Agents, Problems, Solutions, Typst PDF, Chat
// ============================================================================

#[tauri::command]
pub async fn list_agent_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<AgentProfile>, ApiError> {
    state.list_agent_profiles().await.map_err(ApiError::from)
}

#[tauri::command]
pub async fn save_agent_profile(
    state: State<'_, AppState>,
    input: SaveAgentProfileRequest,
) -> Result<AgentProfile, ApiError> {
    state
        .save_agent_profile(&input)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn delete_agent_profile(state: State<'_, AppState>, id: String) -> Result<(), ApiError> {
    state
        .delete_agent_profile(&id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn check_agent_status(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<AgentStatus, ApiError> {
    state
        .check_agent_status(&profile_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn detect_all_agents(state: State<'_, AppState>) -> Result<Vec<AgentStatus>, ApiError> {
    state.detect_all_agents().await.map_err(ApiError::from)
}

#[tauri::command]
pub async fn set_agent_api_key(
    state: State<'_, AppState>,
    profile_id: String,
    api_key: String,
) -> Result<AgentProfile, ApiError> {
    state
        .set_agent_api_key(&profile_id, &api_key)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn delete_agent_api_key(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<AgentProfile, ApiError> {
    state
        .delete_agent_api_key(&profile_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn cancel_agent_run(state: State<'_, AppState>, run_id: String) -> Result<(), ApiError> {
    state
        .cancel_agent_run(&run_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn respond_agent_permission(
    state: State<'_, AppState>,
    run_id: String,
    request_id: String,
    decision: PermissionDecision,
) -> Result<(), ApiError> {
    state
        .respond_agent_permission(&run_id, &request_id, decision)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_problems(
    state: State<'_, AppState>,
    document_id: String,
) -> Result<Vec<Problem>, ApiError> {
    state
        .list_problems(&document_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn upsert_problem(
    state: State<'_, AppState>,
    input: UpsertProblemRequest,
) -> Result<Problem, ApiError> {
    state.upsert_problem(&input).await.map_err(ApiError::from)
}

#[tauri::command]
pub async fn delete_problem(
    state: State<'_, AppState>,
    problem_id: String,
) -> Result<(), ApiError> {
    state
        .delete_problem(&problem_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn split_problem(
    state: State<'_, AppState>,
    problem_id: String,
    first_statement: String,
    second_statement: String,
    second_subproblem_number: Option<String>,
) -> Result<Vec<Problem>, ApiError> {
    state
        .split_problem(
            &problem_id,
            &first_statement,
            &second_statement,
            second_subproblem_number.as_deref(),
        )
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn merge_problems(
    state: State<'_, AppState>,
    primary_id: String,
    secondary_id: String,
) -> Result<Vec<Problem>, ApiError> {
    state
        .merge_problems(&primary_id, &secondary_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn recognize_problems(
    state: State<'_, AppState>,
    input: RecognizeProblemsRequest,
) -> Result<RecognizeProblemsResult, ApiError> {
    state
        .recognize_problems(&input)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_solutions(
    state: State<'_, AppState>,
    document_id: String,
    problem_id: Option<String>,
) -> Result<Vec<Solution>, ApiError> {
    state
        .list_solutions(&document_id, problem_id.as_deref())
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn generate_solution(
    state: State<'_, AppState>,
    input: GenerateSolutionRequest,
) -> Result<SolutionOperationResult, ApiError> {
    state
        .generate_solution(&input)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn save_solution_edit(
    state: State<'_, AppState>,
    input: SaveSolutionEditRequest,
) -> Result<SolutionOperationResult, ApiError> {
    state
        .save_solution_edit(&input)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn recompile_solution_pdf(
    state: State<'_, AppState>,
    solution_id: String,
) -> Result<SolutionOperationResult, ApiError> {
    state
        .recompile_solution_pdf(&solution_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn restore_solution_version(
    state: State<'_, AppState>,
    solution_id: String,
) -> Result<SolutionOperationResult, ApiError> {
    state
        .restore_solution_version(&solution_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_conversations(
    state: State<'_, AppState>,
    document_id: String,
) -> Result<Vec<Conversation>, ApiError> {
    state
        .list_conversations(&document_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_conversation(
    state: State<'_, AppState>,
    document_id: String,
    problem_id: Option<String>,
    solution_id: Option<String>,
    title: String,
) -> Result<Conversation, ApiError> {
    state
        .create_conversation(
            &document_id,
            problem_id.as_deref(),
            solution_id.as_deref(),
            &title,
        )
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn delete_conversation(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<(), ApiError> {
    state
        .delete_conversation(&conversation_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_messages(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<Vec<ChatMessage>, ApiError> {
    state
        .list_messages(&conversation_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn send_chat_message(
    state: State<'_, AppState>,
    input: SendChatMessageRequest,
) -> Result<SendChatMessageResult, ApiError> {
    state
        .send_chat_message(&input)
        .await
        .map_err(ApiError::from)
}

// ============================================================================
// Phase 3: Science MCP Server, Tool Executions, Verifications & Artifacts
// ============================================================================

#[tauri::command]
pub async fn check_science_mcp_status(
    state: State<'_, AppState>,
) -> Result<ScienceMcpStatus, ApiError> {
    state
        .check_science_mcp_status()
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_document_tool_executions(
    state: State<'_, AppState>,
    document_id: String,
) -> Result<Vec<ToolExecutionRecord>, ApiError> {
    state
        .list_document_tool_executions(&document_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_solution_verifications(
    state: State<'_, AppState>,
    solution_id: String,
) -> Result<Vec<VerificationResultRecord>, ApiError> {
    state
        .list_solution_verifications(&solution_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_solution_artifacts(
    state: State<'_, AppState>,
    solution_id: String,
) -> Result<Vec<SolutionArtifactItem>, ApiError> {
    state
        .list_solution_artifacts(&solution_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn run_science_tool(
    state: State<'_, AppState>,
    input: RunScienceToolRequest,
) -> Result<RunScienceToolResult, ApiError> {
    state.run_science_tool(&input).await.map_err(ApiError::from)
}

#[tauri::command]
pub async fn attach_artifact_to_solution(
    state: State<'_, AppState>,
    solution_id: String,
    attachment_id: String,
    caption: Option<String>,
) -> Result<SolutionOperationResult, ApiError> {
    state
        .attach_artifact_to_solution(&solution_id, &attachment_id, caption.as_deref())
        .await
        .map_err(ApiError::from)
}
