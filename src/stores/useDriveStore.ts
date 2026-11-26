import { create } from "zustand";
import { normalizeApiError, tauriClient } from "@/services/tauriClient";
import type {
  ApiError,
  DocumentDriveStatus,
  DriveConflictResolution,
  DriveOverview,
  DriveUploadJob,
  EnqueueDriveUploadRequest,
  EnqueueDriveUploadResult,
  SaveDriveOAuthConfigRequest,
  UpdateDriveSettingsRequest,
} from "@/types/ipc";

interface DriveStoreState {
  overview: DriveOverview | null;
  documentStatusById: Record<string, DocumentDriveStatus>;
  loading: boolean;
  connecting: boolean;
  uploadingDocId: string | null;
  processingQueue: boolean;
  activeConflictJob: DriveUploadJob | null;
  lastNotice: string | null;
  error: ApiError | null;

  loadOverview: () => Promise<void>;
  loadDocumentStatus: (documentId: string | null) => Promise<void>;
  saveOAuthConfig: (req: SaveDriveOAuthConfigRequest) => Promise<void>;
  connect: () => Promise<void>;
  disconnect: () => Promise<void>;
  updateSettings: (req: UpdateDriveSettingsRequest) => Promise<void>;
  uploadDocument: (req: EnqueueDriveUploadRequest) => Promise<EnqueueDriveUploadResult | null>;
  retryJob: (jobId: string) => Promise<void>;
  cancelJob: (jobId: string) => Promise<void>;
  resolveConflict: (jobId: string, resolution: DriveConflictResolution) => Promise<void>;
  processQueueNow: () => Promise<void>;
  openConflictModal: (job: DriveUploadJob) => void;
  closeConflictModal: () => void;
  clearError: () => void;
  clearNotice: () => void;
}

export const useDriveStore = create<DriveStoreState>((set, get) => ({
  overview: null,
  documentStatusById: {},
  loading: false,
  connecting: false,
  uploadingDocId: null,
  processingQueue: false,
  activeConflictJob: null,
  lastNotice: null,
  error: null,

  loadOverview: async () => {
    set({ loading: true });
    try {
      const overview = await tauriClient.getDriveOverview();
      set({ overview, loading: false, error: null });
    } catch (err) {
      set({ loading: false, error: normalizeApiError(err) });
    }
  },

  loadDocumentStatus: async (documentId) => {
    if (!documentId) return;
    try {
      const status = await tauriClient.listDocumentDriveStatus(documentId);
      set((s) => ({
        documentStatusById: { ...s.documentStatusById, [documentId]: status },
      }));
    } catch {
      // Non-fatal: keep local workspace fully operable even when Drive status query fails
    }
  },

  saveOAuthConfig: async (req) => {
    set({ error: null, lastNotice: null });
    try {
      await tauriClient.saveDriveOAuthConfig(req);
      await get().loadOverview();
      set({ lastNotice: "Google OAuth クライアント設定を保存しました（シークレットは OS キーチェーン保管）。" });
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  connect: async () => {
    set({ connecting: true, error: null, lastNotice: null });
    try {
      const conn = await tauriClient.connectGoogleDrive();
      await get().loadOverview();
      set({
        connecting: false,
        lastNotice: `Google Drive (${conn.accountEmail}) に接続しました（最小スコープ drive.file）。`,
      });
    } catch (err) {
      set({ connecting: false, error: normalizeApiError(err) });
    }
  },

  disconnect: async () => {
    set({ error: null, lastNotice: null });
    try {
      await tauriClient.disconnectGoogleDrive();
      await get().loadOverview();
      set({
        lastNotice:
          "Google Drive の接続を解除し OS キーチェーンのトークンを削除しました（Drive 上のファイルは削除されません）。",
      });
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  updateSettings: async (req) => {
    set({ error: null, lastNotice: null });
    try {
      await tauriClient.updateDriveSettings(req);
      await get().loadOverview();
      set({ lastNotice: "Google Drive 連携設定を保存しました。" });
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  uploadDocument: async (req) => {
    set({ uploadingDocId: req.documentId, error: null, lastNotice: null });
    try {
      const res = await tauriClient.uploadDocumentToDrive(req);
      await Promise.all([get().loadOverview(), get().loadDocumentStatus(req.documentId)]);
      let notice: string;
      if (res.enqueuedJobs.length > 0) {
        notice = `${res.enqueuedJobs.length} 件のファイルをアップロードキューへ登録しました。`;
      } else if (res.skippedUpToDate.length > 0) {
        notice = "Google Drive 上のファイルはすでに最新状態です（重複アップロードをスキップ）。";
      } else {
        notice = res.notices[0] ?? "アップロード対象ファイルがありません。";
      }
      set({ uploadingDocId: null, lastNotice: notice });
      return res;
    } catch (err) {
      set({ uploadingDocId: null, error: normalizeApiError(err) });
      return null;
    }
  },

  retryJob: async (jobId) => {
    set({ error: null });
    try {
      const job = await tauriClient.retryDriveJob(jobId);
      await Promise.all([get().loadOverview(), get().loadDocumentStatus(job.documentId)]);
      set({ lastNotice: `ジョブ (${job.targetFilename}) を再試行キューへ投入しました。` });
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  cancelJob: async (jobId) => {
    set({ error: null });
    try {
      const job = await tauriClient.cancelDriveJob(jobId);
      await Promise.all([get().loadOverview(), get().loadDocumentStatus(job.documentId)]);
      set({ lastNotice: `ジョブ (${job.targetFilename}) をキャンセルしました。` });
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  resolveConflict: async (jobId, resolution) => {
    set({ error: null });
    try {
      const job = await tauriClient.resolveDriveConflict({ jobId, resolution });
      set({ activeConflictJob: null });
      await Promise.all([get().loadOverview(), get().loadDocumentStatus(job.documentId)]);
      const label =
        resolution === "overwrite"
          ? "ローカル版で上書き更新をキュー投入しました。"
          : resolution === "save_as_new"
            ? "別ファイルとして新規保存をキュー投入しました。"
            : resolution === "recreate_after_remote_delete"
              ? "新規ファイルとして再作成をキュー投入しました。"
              : "競合ジョブをキャンセルしました。";
      set({ lastNotice: label });
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  processQueueNow: async () => {
    set({ processingQueue: true, error: null });
    try {
      const count = await tauriClient.processDriveQueueNow();
      await get().loadOverview();
      set({
        processingQueue: false,
        lastNotice:
          count > 0
            ? `${count} 件のアップロードジョブを処理しました。`
            : "現在待機中のアップロードジョブはありません。",
      });
    } catch (err) {
      set({ processingQueue: false, error: normalizeApiError(err) });
    }
  },

  openConflictModal: (job) => set({ activeConflictJob: job }),
  closeConflictModal: () => set({ activeConflictJob: null }),
  clearError: () => set({ error: null }),
  clearNotice: () => set({ lastNotice: null }),
}));
