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

export interface AppSettings {
  language: string;
  theme: ThemePreference;
  defaultSubjectId: string | null;
  defaultRecognitionMode: RecognitionMode;
  leftSidebarWidth: number;
  rightSidebarWidth: number;
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
