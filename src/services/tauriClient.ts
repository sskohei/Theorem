import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type {
  ApiError,
  AppSettings,
  AttachmentBinaryResponse,
  AttachmentInfo,
  BackupInfo,
  BootstrapState,
  DocumentDetail,
  DocumentFilter,
  DocumentSummary,
  Folder,
  ImportBatchResult,
  ImportedDocumentItem,
  RecognitionMode,
  StorageStats,
  Subject,
  SubjectCategory,
  Tag,
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
