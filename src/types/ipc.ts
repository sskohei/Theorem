export type SubjectCategory = "mathematics" | "physics" | "other";
export type ThemePreference = "light" | "dark" | "system";
export type RecognitionMode =
  | "printed_only"
  | "include_handwriting"
  | "page_range"
  | "handwritten_answer";

export interface ApiError {
  code: string;
  message: string;
  detail: string | null;
  recoverable: boolean;
}

export interface Subject {
  id: string;
  name: string;
  category: SubjectCategory;
  description: string | null;
  sortOrder: number;
  createdAt: string;
  updatedAt: string;
}

export interface Folder {
  id: string;
  subjectId: string;
  parentId: string | null;
  name: string;
  autoUploadDrive: boolean;
  sortOrder: number;
  createdAt: string;
  updatedAt: string;
}

export interface Tag {
  id: string;
  name: string;
  color: string | null;
  createdAt: string;
}

export interface AttachmentInfo {
  id: string;
  documentId: string | null;
  kind: string;
  originalFilename: string;
  relativePath: string;
  mimeType: string;
  byteSize: number;
  sha256: string;
  width: number | null;
  height: number | null;
  pageCount: number | null;
  createdAt: string;
}

export interface DocumentSummary {
  id: string;
  subjectId: string;
  folderId: string | null;
  title: string;
  sourceType: "pdf" | "image" | "multi_image";
  primaryAttachmentId: string | null;
  thumbnailAttachmentId: string | null;
  pageCount: number | null;
  recognitionMode: RecognitionMode;
  studyStatus: string;
  isFavorite: boolean;
  lastOpenedAt: string | null;
  tagIds: string[];
  createdAt: string;
  updatedAt: string;
}

export interface DocumentDetail {
  document: DocumentSummary;
  attachments: AttachmentInfo[];
  tags: Tag[];
}

export interface DocumentFilter {
  subjectId: string | null;
  folderId: string | null;
  tagId: string | null;
  searchQuery: string | null;
  favoritesOnly: boolean;
  recentOnly: boolean;
}

export interface DuplicateMatch {
  documentId: string;
  documentTitle: string;
  attachmentId: string;
  originalFilename: string;
  sha256: string;
}

export interface ImportedDocumentItem {
  detail: DocumentDetail;
  duplicateMatches: DuplicateMatch[];
}

export interface ImportFailure {
  filename: string;
  reason: string;
}

export interface ImportBatchResult {
  imported: ImportedDocumentItem[];
  failures: ImportFailure[];
}

export interface PdfSettings {
  defaultTemplate: "auto" | "mathematics" | "physics" | "concise" | "detailed";
  paperSize: "a4" | "letter" | "b5";
  showModelName: boolean;
  showGeneratedAt: boolean;
  showVersionNumber: boolean;
}

export interface AgentPurposeAssignments {
  extractionProfileId: string | null;
  solutionProfileId: string | null;
  chatProfileId: string | null;
  verificationProfileId: string | null;
  revisionProfileId: string | null;
}

export interface PromptSettings {
  mathematicsCustomInstructions: string;
  physicsCustomInstructions: string;
  modeCustomInstructions: string;
}

export interface ScienceMcpSettings {
  enabled: boolean;
  uvPath: string | null;
  serverDir: string | null;
  timeoutSeconds: number;
  defaultPlotFormat: "svg" | "png";
}

export interface DriveAppSettings {
  oauthClientId: string | null;
}

export interface AppSettings {
  language: string;
  theme: ThemePreference;
  defaultSubjectId: string | null;
  defaultRecognitionMode: RecognitionMode;
  leftSidebarWidth: number;
  rightSidebarWidth: number;
  cloudAiConsentAccepted: boolean;
  pdf: PdfSettings;
  agentAssignments: AgentPurposeAssignments;
  prompts: PromptSettings;
  scienceMcp: ScienceMcpSettings;
  drive?: DriveAppSettings;
}

export interface StorageStats {
  dataRootDisplay: string;
  databaseBytes: number;
  originalsBytes: number;
  thumbnailsBytes: number;
  backupsBytes: number;
  logsBytes: number;
  tempBytes: number;
  totalBytes: number;
  documentCount: number;
  attachmentCount: number;
}

export interface BackupInfo {
  filename: string;
  relativePath: string;
  byteSize: number;
  createdAt: string;
}

export interface BootstrapState {
  appVersion: string;
  dataRootDisplay: string;
  subjects: Subject[];
  folders: Folder[];
  tags: Tag[];
  documents: DocumentSummary[];
  settings: AppSettings;
}

export interface AttachmentBinaryResponse {
  attachment: AttachmentInfo;
  base64Data: string;
}

// ============================================================================
// Phase 2: Problems, Solutions, Typst PDF, Chat & AI Agents
// ============================================================================

export type ProviderKind = "codex" | "claude_code" | "antigravity" | "custom_acp" | "mock";
export type AuthMode = "official_harness" | "api_key_keyring";
export type SolutionMode =
  | "standard"
  | "concise"
  | "detailed"
  | "rigorous"
  | "beginner"
  | "hint"
  | "verified"
  | "review";
export type PermissionDecision = "allow_once" | "deny";

export interface Problem {
  id: string;
  documentId: string;
  problemNumber: string;
  subproblemNumber: string | null;
  sortOrder: number;
  subjectCategory: SubjectCategory;
  field: string | null;
  statementMarkdown: string;
  givenConditions: string[];
  targetObjective: string | null;
  sourcePages: number[];
  figureRefs: string[];
  uncertainties: string[];
  isUserVerified: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface UpsertProblemRequest {
  id?: string | null;
  documentId: string;
  problemNumber: string;
  subproblemNumber?: string | null;
  subjectCategory: SubjectCategory;
  field?: string | null;
  statementMarkdown: string;
  givenConditions: string[];
  targetObjective?: string | null;
  sourcePages: number[];
  figureRefs: string[];
  uncertainties: string[];
  isUserVerified: boolean;
}

export interface Solution {
  id: string;
  documentId: string;
  problemId: string | null;
  versionNumber: number;
  parentSolutionId: string | null;
  isCurrent: boolean;
  mode: SolutionMode;
  schemaVersion: number;
  structuredJson: string;
  validationStatus: "valid" | "invalid" | "user_edited";
  typstSourceAttachmentId: string | null;
  pdfAttachmentId: string | null;
  agentRunId: string | null;
  modelName: string | null;
  changeSummary: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface SchemaValidationIssue {
  path: string;
  message: string;
}

export interface LatexConversionError {
  location: string;
  stepIndex: number | null;
  latex: string;
  reason: string;
}

export interface SolutionOperationResult {
  solution: Solution;
  schemaIssues: SchemaValidationIssue[];
  latexErrors: LatexConversionError[];
  pdfError: string | null;
  rawOutput: string | null;
}

export interface PageImagePayload {
  pageNumber: number;
  pngBase64: string;
  extractedText?: string | null;
}

export interface RecognizeProblemsRequest {
  documentId: string;
  agentProfileId?: string | null;
  modelName?: string | null;
  recognitionMode: RecognitionMode;
  pageRangeStart?: number | null;
  pageRangeEnd?: number | null;
  pageImages: PageImagePayload[];
  croppedRegionPngBase64?: string | null;
}

export interface RecognizeProblemsResult {
  problems: Problem[];
  schemaIssues: SchemaValidationIssue[];
  rawOutput: string | null;
  runId: string;
}

export interface GenerateSolutionRequest {
  documentId: string;
  problemId?: string | null;
  agentProfileId?: string | null;
  modelName?: string | null;
  mode: SolutionMode;
  targetScope: "all_problems" | "single_problem" | "page_range";
  pageRangeStart?: number | null;
  pageRangeEnd?: number | null;
  repairFeedback?: string | null;
  parentSolutionId?: string | null;
}

export interface SaveSolutionEditRequest {
  documentId: string;
  problemId?: string | null;
  parentSolutionId?: string | null;
  mode: SolutionMode;
  structuredJson: string;
  changeSummary?: string | null;
}

export interface Conversation {
  id: string;
  documentId: string;
  problemId: string | null;
  solutionId: string | null;
  title: string;
  createdAt: string;
  updatedAt: string;
}

export interface ChatMessage {
  id: string;
  conversationId: string;
  role: "user" | "assistant" | "system";
  contentMarkdown: string;
  status: "streaming" | "completed" | "cancelled" | "failed";
  agentRunId: string | null;
  referencedProblemId: string | null;
  attachmentIds: string[];
  createdAt: string;
  updatedAt: string;
}

export interface ChatImageUpload {
  filename: string;
  mimeType: string;
  base64Data: string;
}

export interface SendChatMessageRequest {
  runId: string;
  conversationId: string;
  documentId: string;
  agentProfileId?: string | null;
  modelName?: string | null;
  contentMarkdown: string;
  referencedProblemId?: string | null;
  includeProblemContext: boolean;
  includeSolutionContext: boolean;
  attachedImages: ChatImageUpload[];
}

export interface SendChatMessageResult {
  runId: string;
  userMessage: ChatMessage;
  assistantMessage: ChatMessage;
  resumedExternalSession: boolean;
}

export interface AgentCapabilities {
  supportsImageInput: boolean;
  supportsPdfInput: boolean;
  supportsStreaming: boolean;
  supportsSessionResume: boolean;
  supportsModelListing: boolean;
  supportsAuthStatusCheck: boolean;
  supportsMcpScopedConfig: boolean;
  supportsPermissionHooks: boolean;
  supportsSandboxRestriction: boolean;
  supportsCancel: boolean;
}

export interface AgentModelInfo {
  id: string;
  label: string;
}

export interface AgentStatus {
  providerKind: ProviderKind;
  installed: boolean;
  resolvedPath: string | null;
  version: string | null;
  authStatus: "authenticated" | "unauthenticated" | "unknown" | "terms_restricted";
  authMethod: string | null;
  verificationStatus: "verified_live" | "verified_mock" | "unverified";
  availableModels: AgentModelInfo[] | null;
  capabilities: AgentCapabilities;
  authGuide: string;
  complianceNote: string;
  detailNote: string | null;
}

export interface AgentProfile {
  id: string;
  name: string;
  providerKind: ProviderKind;
  executablePath: string | null;
  args: string[];
  defaultModel: string | null;
  authMode: AuthMode;
  hasKeyringSecret: boolean;
  isEnabled: boolean;
  isDefault: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface SaveAgentProfileRequest {
  id?: string | null;
  name: string;
  providerKind: ProviderKind;
  executablePath?: string | null;
  args: string[];
  defaultModel?: string | null;
  authMode: AuthMode;
  isEnabled: boolean;
  isDefault: boolean;
}

export type AgentEventPayload =
  | { type: "sessionStarted"; externalSessionId: string | null; resumed: boolean }
  | { type: "messageStarted"; role: string }
  | { type: "textDelta"; text: string }
  | {
      type: "toolCallStarted";
      callId: string;
      server: string | null;
      toolName: string;
      arguments: unknown;
    }
  | { type: "toolCallUpdated"; callId: string; status: string; progress: string | null }
  | { type: "toolCallCompleted"; callId: string; succeeded: boolean; output: unknown }
  | {
      type: "permissionRequested";
      requestId: string;
      action: string;
      target: string | null;
      detail: string;
    }
  | {
      type: "artifactCreated";
      relativePath: string;
      mimeType: string;
      caption: string | null;
    }
  | {
      type: "usageUpdated";
      inputTokens: number | null;
      outputTokens: number | null;
      rawLabel: string | null;
    }
  | { type: "messageCompleted"; finalText: string | null }
  | { type: "cancelled"; reason: string | null }
  | {
      type: "failed";
      code: string;
      message: string;
      rateLimited: boolean;
      authRequired: boolean;
    };

export interface AgentEventEnvelope {
  eventId: string;
  runId: string;
  sessionId: string;
  messageId: string | null;
  timestamp: string;
  event: AgentEventPayload;
}

// ============================================================================
// Phase 3: Science MCP Server, Tool Executions, Verifications & Artifacts
// ============================================================================

export type VerificationStatus = "passed" | "failed" | "inconclusive" | "not_run" | "error";
export type VerificationSourceKind = "science_mcp" | "ai_self_check" | "user_check";
export type ToolExecutionStatus = "running" | "succeeded" | "failed" | "timeout" | "cancelled";

export interface ScienceMcpToolInfo {
  name: string;
  description: string;
}

export interface ScienceMcpStatus {
  enabled: boolean;
  uvAvailable: boolean;
  uvPath: string | null;
  uvVersion: string | null;
  serverDir: string | null;
  serverReady: boolean;
  toolCount: number;
  tools: ScienceMcpToolInfo[];
  securityNote: string;
  detailNote: string | null;
}

export interface ToolExecutionRecord {
  id: string;
  agentRunId: string | null;
  documentId: string | null;
  solutionId: string | null;
  serverName: string;
  toolName: string;
  argumentsJson: string;
  resultJson: string | null;
  status: ToolExecutionStatus;
  durationMs: number | null;
  errorMessage: string | null;
  createdAt: string;
  finishedAt: string | null;
}

export interface VerificationResultRecord {
  id: string;
  solutionId: string;
  toolExecutionId: string | null;
  sourceKind: VerificationSourceKind;
  targetDescription: string;
  assumptions: string[];
  method: string;
  status: VerificationStatus;
  detailJson: string | null;
  createdAt: string;
}

export interface SolutionArtifactItem {
  id: string;
  solutionId: string;
  attachmentId: string;
  artifactType: "plot" | "diagram" | "crop" | "table";
  caption: string | null;
  sourceToolExecutionId: string | null;
  sortOrder: number;
  relativePath: string;
  mimeType: string;
  originalFilename: string;
  byteSize: number;
  createdAt: string;
}

export interface RunScienceToolRequest {
  documentId: string;
  solutionId?: string | null;
  toolName: string;
  arguments: Record<string, unknown>;
  attachToSolutionPdf: boolean;
}

export interface RunScienceToolResult {
  execution: ToolExecutionRecord;
  verification: VerificationResultRecord | null;
  createdArtifacts: AttachmentInfo[];
  updatedSolution: SolutionOperationResult | null;
}

// ============================================================================
// Phase 4: Google Drive Export, OAuth PKCE & Persistent Job Queue
// ============================================================================

export type DriveUploadMode = "manual" | "auto" | "folder_auto";
export type DriveVersionPolicy = "update_latest" | "create_new" | "manual_version";
export type DriveConnectionStatus = "connected" | "reauth_required" | "disconnected";
export type DriveFileSyncStatus =
  | "uploaded"
  | "update_required"
  | "conflict"
  | "remote_deleted"
  | "error";
export type DriveJobStatus =
  | "pending"
  | "uploading"
  | "uploaded"
  | "update_required"
  | "failed"
  | "conflict"
  | "cancelled";
export type DriveAggregateStatus =
  | "none"
  | "uploaded"
  | "uploading"
  | "pending"
  | "update_required"
  | "conflict"
  | "remote_deleted"
  | "failed";
export type DriveConflictResolution =
  | "overwrite"
  | "save_as_new"
  | "recreate_after_remote_delete"
  | "cancel";

export interface DriveOAuthConfigStatus {
  clientId: string | null;
  clientIdSource: "settings" | "env" | "none" | string;
  hasClientSecret: boolean;
  requiredScope: string;
}

export interface SaveDriveOAuthConfigRequest {
  clientId?: string | null;
  clientSecret?: string | null;
  clearClientSecret?: boolean;
}

export interface DriveConnection {
  id: string;
  accountEmail: string;
  accountDisplayName: string | null;
  scopes: string[];
  rootFolderDriveId: string | null;
  rootFolderName: string;
  uploadMode: DriveUploadMode;
  versionPolicy: DriveVersionPolicy;
  includePastVersions: boolean;
  includeSolutionSource: boolean;
  includeArtifacts: boolean;
  status: DriveConnectionStatus;
  createdAt: string;
  updatedAt: string;
}

export interface UpdateDriveSettingsRequest {
  rootFolderName?: string | null;
  uploadMode?: DriveUploadMode | null;
  versionPolicy?: DriveVersionPolicy | null;
  includePastVersions?: boolean | null;
  includeSolutionSource?: boolean | null;
  includeArtifacts?: boolean | null;
}

export interface DriveFileMapping {
  id: string;
  connectionId: string;
  documentId: string | null;
  logicalRole: string;
  driveFileId: string;
  driveFolderId: string;
  driveFilename: string;
  webViewLink: string | null;
  lastUploadedAttachmentId: string | null;
  lastUploadedSha256: string | null;
  remoteMd5Checksum: string | null;
  remoteModifiedTime: string | null;
  remoteVersion: string | null;
  syncStatus: DriveFileSyncStatus;
  createdAt: string;
  updatedAt: string;
}

export interface DriveUploadJob {
  id: string;
  connectionId: string;
  documentId: string;
  logicalRole: string;
  attachmentId: string | null;
  driveFileId: string | null;
  targetFilename: string;
  idempotencyKey: string;
  forceOverwrite: boolean;
  saveAsNew: boolean;
  status: DriveJobStatus;
  attemptCount: number;
  nextRetryAt: string | null;
  lastErrorCode: string | null;
  lastErrorMessage: string | null;
  conflictRemoteMd5: string | null;
  conflictRemoteModifiedTime: string | null;
  conflictRemoteVersion: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface DriveDocumentSyncSummary {
  documentId: string;
  aggregateStatus: DriveAggregateStatus;
  uploadedCount: number;
  pendingCount: number;
  conflictCount: number;
  failedCount: number;
}

export interface DriveOverview {
  oauthConfig: DriveOAuthConfigStatus;
  connection: DriveConnection | null;
  jobs: DriveUploadJob[];
  documentSummaries: DriveDocumentSyncSummary[];
  pendingCount: number;
  uploadingCount: number;
  conflictCount: number;
  failedCount: number;
}

export interface DocumentDriveStatus {
  documentId: string;
  files: DriveFileMapping[];
  jobs: DriveUploadJob[];
}

export interface EnqueueDriveUploadRequest {
  documentId: string;
  solutionId?: string | null;
  scope?: "default" | "original_only" | "latest_solution_only" | "all_configured";
}

export interface EnqueueDriveUploadResult {
  enqueuedJobs: DriveUploadJob[];
  skippedUpToDate: string[];
  notices: string[];
}

export interface ResolveDriveConflictRequest {
  jobId: string;
  resolution: DriveConflictResolution;
}
