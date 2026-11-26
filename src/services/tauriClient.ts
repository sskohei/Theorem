import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type {
  AgentProfile,
  AgentStatus,
  ApiError,
  AppSettings,
  AttachmentBinaryResponse,
  AttachmentInfo,
  BackupInfo,
  BootstrapState,
  ChatMessage,
  Conversation,
  DocumentDetail,
  DocumentDriveStatus,
  DocumentFilter,
  DocumentSummary,
  DriveConnection,
  DriveOAuthConfigStatus,
  DriveOverview,
  DriveUploadJob,
  EnqueueDriveUploadRequest,
  EnqueueDriveUploadResult,
  Folder,
  GenerateSolutionRequest,
  ImportBatchResult,
  ImportedDocumentItem,
  PermissionDecision,
  Problem,
  RecognitionMode,
  RecognizeProblemsRequest,
  RecognizeProblemsResult,
  ResolveDriveConflictRequest,
  RunScienceToolRequest,
  RunScienceToolResult,
  SaveAgentProfileRequest,
  SaveDriveOAuthConfigRequest,
  SaveSolutionEditRequest,
  ScienceMcpStatus,
  SendChatMessageRequest,
  SendChatMessageResult,
  Solution,
  SolutionArtifactItem,
  SolutionOperationResult,
  StorageStats,
  Subject,
  SubjectCategory,
  Tag,
  ToolExecutionRecord,
  UpdateDriveSettingsRequest,
  UpsertProblemRequest,
  VerificationResultRecord,
} from "@/types/ipc";

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function normalizeApiError(err: unknown): ApiError {
  if (typeof err === "object" && err !== null && "code" in err && "message" in err) {
    const e = err as Record<string, unknown>;
    return {
      code: String(e.code),
      message: String(e.message),
      detail: typeof e.detail === "string" ? e.detail : null,
      recoverable: typeof e.recoverable === "boolean" ? e.recoverable : true,
    };
  }
  if (err instanceof Error) {
    return {
      code: "CLIENT_ERROR",
      message: err.message,
      detail: null,
      recoverable: true,
    };
  }
  return {
    code: "UNKNOWN_ERROR",
    message: String(err),
    detail: null,
    recoverable: true,
  };
}

async function callCommand<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (err) {
    throw normalizeApiError(err);
  }
}

export const tauriClient = {
  getBootstrapState(): Promise<BootstrapState> {
    return callCommand<BootstrapState>("get_bootstrap_state");
  },

  // Subjects
  listSubjects(): Promise<Subject[]> {
    return callCommand<Subject[]>("list_subjects");
  },
  createSubject(
    name: string,
    category: SubjectCategory,
    description?: string | null,
  ): Promise<Subject> {
    return callCommand<Subject>("create_subject", {
      name,
      category,
      description: description ?? null,
    });
  },
  updateSubject(
    id: string,
    name: string,
    category: SubjectCategory,
    description?: string | null,
  ): Promise<Subject> {
    return callCommand<Subject>("update_subject", {
      id,
      name,
      category,
      description: description ?? null,
    });
  },
  deleteSubject(id: string): Promise<void> {
    return callCommand<void>("delete_subject", { id });
  },

  // Folders
  listFolders(): Promise<Folder[]> {
    return callCommand<Folder[]>("list_folders");
  },
  createFolder(subjectId: string, parentId: string | null, name: string): Promise<Folder> {
    return callCommand<Folder>("create_folder", { subjectId, parentId, name });
  },
  updateFolder(id: string, name: string, parentId: string | null): Promise<Folder> {
    return callCommand<Folder>("update_folder", { id, name, parentId });
  },
  deleteFolder(id: string): Promise<void> {
    return callCommand<void>("delete_folder", { id });
  },

  // Tags
  listTags(): Promise<Tag[]> {
    return callCommand<Tag[]>("list_tags");
  },
  createTag(name: string, color?: string | null): Promise<Tag> {
    return callCommand<Tag>("create_tag", { name, color: color ?? null });
  },
  deleteTag(id: string): Promise<void> {
    return callCommand<void>("delete_tag", { id });
  },
  setDocumentTags(documentId: string, tagIds: string[]): Promise<Tag[]> {
    return callCommand<Tag[]>("set_document_tags", { documentId, tagIds });
  },

  // Documents & Attachments
  listDocuments(filter: DocumentFilter): Promise<DocumentSummary[]> {
    return callCommand<DocumentSummary[]>("list_documents", { filter });
  },
  getDocumentDetail(documentId: string): Promise<DocumentDetail> {
    return callCommand<DocumentDetail>("get_document_detail", { documentId });
  },
  updateDocument(params: {
    id: string;
    title?: string;
    subjectId?: string;
    folderId?: string | null;
    isFavorite?: boolean;
    recognitionMode?: RecognitionMode;
  }): Promise<DocumentDetail> {
    const clearFolder = "folderId" in params && params.folderId === null;
    return callCommand<DocumentDetail>("update_document", {
      input: {
        id: params.id,
        title: params.title ?? null,
        subjectId: params.subjectId ?? null,
        folderId: typeof params.folderId === "string" ? params.folderId : null,
        clearFolder,
        isFavorite: params.isFavorite ?? null,
        recognitionMode: params.recognitionMode ?? null,
      },
    });
  },
  touchDocumentOpened(documentId: string): Promise<string> {
    return callCommand<string>("touch_document_opened", { documentId });
  },
  deleteDocument(id: string): Promise<void> {
    return callCommand<void>("delete_document", { id });
  },
  importFilesFromPaths(
    subjectId: string,
    folderId: string | null,
    paths: string[],
  ): Promise<ImportBatchResult> {
    return callCommand<ImportBatchResult>("import_files_from_paths", {
      subjectId,
      folderId,
      paths,
    });
  },
  importFileBytes(
    subjectId: string,
    folderId: string | null,
    filename: string,
    base64Data: string,
  ): Promise<ImportedDocumentItem> {
    return callCommand<ImportedDocumentItem>("import_file_bytes", {
      subjectId,
      folderId,
      filename,
      base64Data,
    });
  },
  readAttachmentBase64(attachmentId: string): Promise<AttachmentBinaryResponse> {
    return callCommand<AttachmentBinaryResponse>("read_attachment_base64", { attachmentId });
  },
  saveDocumentThumbnail(documentId: string, pngBase64: string): Promise<AttachmentInfo> {
    return callCommand<AttachmentInfo>("save_document_thumbnail", { documentId, pngBase64 });
  },
  exportAttachment(attachmentId: string, destinationPath: string): Promise<void> {
    return callCommand<void>("export_attachment", { attachmentId, destinationPath });
  },

  // Settings, Storage & Backup
  getSettings(): Promise<AppSettings> {
    return callCommand<AppSettings>("get_settings");
  },
  updateSettings(settings: AppSettings): Promise<AppSettings> {
    return callCommand<AppSettings>("update_settings", { settings });
  },
  getStorageStats(): Promise<StorageStats> {
    return callCommand<StorageStats>("get_storage_stats");
  },
  cleanupTempFiles(): Promise<number> {
    return callCommand<number>("cleanup_temp_files");
  },
  createBackup(): Promise<BackupInfo> {
    return callCommand<BackupInfo>("create_backup");
  },
  listBackups(): Promise<BackupInfo[]> {
    return callCommand<BackupInfo[]>("list_backups");
  },
  restoreBackup(backupRelPath: string): Promise<BootstrapState> {
    return callCommand<BootstrapState>("restore_backup", { backupRelPath });
  },

  // Phase 2: Agents & OS Keychain
  listAgentProfiles(): Promise<AgentProfile[]> {
    return callCommand<AgentProfile[]>("list_agent_profiles");
  },
  saveAgentProfile(input: SaveAgentProfileRequest): Promise<AgentProfile> {
    return callCommand<AgentProfile>("save_agent_profile", { input });
  },
  deleteAgentProfile(id: string): Promise<void> {
    return callCommand<void>("delete_agent_profile", { id });
  },
  checkAgentStatus(profileId: string): Promise<AgentStatus> {
    return callCommand<AgentStatus>("check_agent_status", { profileId });
  },
  detectAllAgents(): Promise<AgentStatus[]> {
    return callCommand<AgentStatus[]>("detect_all_agents");
  },
  setAgentApiKey(profileId: string, apiKey: string): Promise<AgentProfile> {
    return callCommand<AgentProfile>("set_agent_api_key", { profileId, apiKey });
  },
  deleteAgentApiKey(profileId: string): Promise<AgentProfile> {
    return callCommand<AgentProfile>("delete_agent_api_key", { profileId });
  },
  cancelAgentRun(runId: string): Promise<void> {
    return callCommand<void>("cancel_agent_run", { runId });
  },
  respondAgentPermission(
    runId: string,
    requestId: string,
    decision: PermissionDecision,
  ): Promise<void> {
    return callCommand<void>("respond_agent_permission", { runId, requestId, decision });
  },

  // Phase 2: Problems & Recognition
  listProblems(documentId: string): Promise<Problem[]> {
    return callCommand<Problem[]>("list_problems", { documentId });
  },
  upsertProblem(input: UpsertProblemRequest): Promise<Problem> {
    return callCommand<Problem>("upsert_problem", { input });
  },
  deleteProblem(problemId: string): Promise<void> {
    return callCommand<void>("delete_problem", { problemId });
  },
  splitProblem(
    problemId: string,
    firstStatement: string,
    secondStatement: string,
    secondSubproblemNumber?: string | null,
  ): Promise<Problem[]> {
    return callCommand<Problem[]>("split_problem", {
      problemId,
      firstStatement,
      secondStatement,
      secondSubproblemNumber: secondSubproblemNumber ?? null,
    });
  },
  mergeProblems(primaryId: string, secondaryId: string): Promise<Problem[]> {
    return callCommand<Problem[]>("merge_problems", { primaryId, secondaryId });
  },
  recognizeProblems(input: RecognizeProblemsRequest): Promise<RecognizeProblemsResult> {
    return callCommand<RecognizeProblemsResult>("recognize_problems", { input });
  },

  // Phase 2: Solutions & Typst PDF
  listSolutions(documentId: string, problemId?: string | null): Promise<Solution[]> {
    return callCommand<Solution[]>("list_solutions", {
      documentId,
      problemId: problemId ?? null,
    });
  },
  generateSolution(input: GenerateSolutionRequest): Promise<SolutionOperationResult> {
    return callCommand<SolutionOperationResult>("generate_solution", { input });
  },
  saveSolutionEdit(input: SaveSolutionEditRequest): Promise<SolutionOperationResult> {
    return callCommand<SolutionOperationResult>("save_solution_edit", { input });
  },
  recompileSolutionPdf(solutionId: string): Promise<SolutionOperationResult> {
    return callCommand<SolutionOperationResult>("recompile_solution_pdf", { solutionId });
  },
  restoreSolutionVersion(solutionId: string): Promise<SolutionOperationResult> {
    return callCommand<SolutionOperationResult>("restore_solution_version", { solutionId });
  },

  // Phase 2: Conversations & Chat
  listConversations(documentId: string): Promise<Conversation[]> {
    return callCommand<Conversation[]>("list_conversations", { documentId });
  },
  createConversation(
    documentId: string,
    title: string,
    problemId?: string | null,
    solutionId?: string | null,
  ): Promise<Conversation> {
    return callCommand<Conversation>("create_conversation", {
      documentId,
      problemId: problemId ?? null,
      solutionId: solutionId ?? null,
      title,
    });
  },
  deleteConversation(conversationId: string): Promise<void> {
    return callCommand<void>("delete_conversation", { conversationId });
  },
  listMessages(conversationId: string): Promise<ChatMessage[]> {
    return callCommand<ChatMessage[]>("list_messages", { conversationId });
  },
  sendChatMessage(input: SendChatMessageRequest): Promise<SendChatMessageResult> {
    return callCommand<SendChatMessageResult>("send_chat_message", { input });
  },

  // Phase 3: Science MCP Server, Tool Executions, Verifications & Artifacts
  checkScienceMcpStatus(): Promise<ScienceMcpStatus> {
    return callCommand<ScienceMcpStatus>("check_science_mcp_status");
  },
  listDocumentToolExecutions(documentId: string): Promise<ToolExecutionRecord[]> {
    return callCommand<ToolExecutionRecord[]>("list_document_tool_executions", { documentId });
  },
  listSolutionVerifications(solutionId: string): Promise<VerificationResultRecord[]> {
    return callCommand<VerificationResultRecord[]>("list_solution_verifications", { solutionId });
  },
  listSolutionArtifacts(solutionId: string): Promise<SolutionArtifactItem[]> {
    return callCommand<SolutionArtifactItem[]>("list_solution_artifacts", { solutionId });
  },
  runScienceTool(input: RunScienceToolRequest): Promise<RunScienceToolResult> {
    return callCommand<RunScienceToolResult>("run_science_tool", { input });
  },
  attachArtifactToSolution(
    solutionId: string,
    attachmentId: string,
    caption?: string | null,
  ): Promise<SolutionOperationResult> {
    return callCommand<SolutionOperationResult>("attach_artifact_to_solution", {
      solutionId,
      attachmentId,
      caption: caption ?? null,
    });
  },

  // Phase 4: Google Drive Export, OAuth PKCE & Persistent Job Queue
  getDriveOverview(): Promise<DriveOverview> {
    return callCommand<DriveOverview>("get_drive_overview");
  },
  saveDriveOAuthConfig(input: SaveDriveOAuthConfigRequest): Promise<DriveOAuthConfigStatus> {
    return callCommand<DriveOAuthConfigStatus>("save_drive_oauth_config", { input });
  },
  connectGoogleDrive(): Promise<DriveConnection> {
    return callCommand<DriveConnection>("connect_google_drive");
  },
  disconnectGoogleDrive(): Promise<void> {
    return callCommand<void>("disconnect_google_drive");
  },
  updateDriveSettings(input: UpdateDriveSettingsRequest): Promise<DriveConnection> {
    return callCommand<DriveConnection>("update_drive_settings", { input });
  },
  setFolderAutoUpload(folderId: string, autoUploadDrive: boolean): Promise<Folder> {
    return callCommand<Folder>("set_folder_auto_upload", { folderId, autoUploadDrive });
  },
  uploadDocumentToDrive(input: EnqueueDriveUploadRequest): Promise<EnqueueDriveUploadResult> {
    return callCommand<EnqueueDriveUploadResult>("upload_document_to_drive", { input });
  },
  listDocumentDriveStatus(documentId: string): Promise<DocumentDriveStatus> {
    return callCommand<DocumentDriveStatus>("list_document_drive_status", { documentId });
  },
  retryDriveJob(jobId: string): Promise<DriveUploadJob> {
    return callCommand<DriveUploadJob>("retry_drive_job", { jobId });
  },
  cancelDriveJob(jobId: string): Promise<DriveUploadJob> {
    return callCommand<DriveUploadJob>("cancel_drive_job", { jobId });
  },
  resolveDriveConflict(input: ResolveDriveConflictRequest): Promise<DriveUploadJob> {
    return callCommand<DriveUploadJob>("resolve_drive_conflict", { input });
  },
  processDriveQueueNow(): Promise<number> {
    return callCommand<number>("process_drive_queue_now");
  },

  // Native Dialogs
  async pickProblemFiles(): Promise<string[]> {
    const selected = await open({
      multiple: true,
      title: "問題ファイルを選択（PDF / 画像）",
      filters: [
        {
          name: "問題ファイル (PDF / 画像)",
          extensions: ["pdf", "png", "jpg", "jpeg", "webp"],
        },
      ],
    });
    if (!selected) return [];
    return Array.isArray(selected) ? selected : [selected];
  },

  async pickSavePath(defaultName: string): Promise<string | null> {
    const chosen = await save({
      defaultPath: defaultName,
      title: "外部ファイルとして保存",
    });
    return chosen ?? null;
  },
};
