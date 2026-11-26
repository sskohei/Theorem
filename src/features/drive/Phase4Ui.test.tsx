import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SolutionVersionsTab } from "@/features/document/SolutionVersionsTab";
import { DriveConflictModal } from "@/features/drive/DriveConflictModal";
import { LibrarySidebar } from "@/features/library/LibrarySidebar";
import { SettingsModal } from "@/features/settings/SettingsModal";
import { tauriClient } from "@/services/tauriClient";
import { useAiWorkspaceStore } from "@/stores/useAiWorkspaceStore";
import { useDriveStore } from "@/stores/useDriveStore";
import { useLayoutStore } from "@/stores/useLayoutStore";
import { useLibraryStore } from "@/stores/useLibraryStore";
import type {
  DriveConnection,
  DriveOverview,
  DriveUploadJob,
  Solution,
} from "@/types/ipc";

vi.mock("@/services/tauriClient", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/services/tauriClient")>();
  return {
    ...actual,
    isTauriRuntime: () => false,
    tauriClient: {
      ...actual.tauriClient,
      getDriveOverview: vi.fn(),
      listDocumentDriveStatus: vi.fn(),
      saveDriveOAuthConfig: vi.fn(),
      connectGoogleDrive: vi.fn(),
      disconnectGoogleDrive: vi.fn(),
      updateDriveSettings: vi.fn(),
      uploadDocumentToDrive: vi.fn(),
      retryDriveJob: vi.fn(),
      cancelDriveJob: vi.fn(),
      resolveDriveConflict: vi.fn(),
      processDriveQueueNow: vi.fn(),
      setFolderAutoUpload: vi.fn(),
      listDocuments: vi.fn(),
    },
  };
});

const SAMPLE_CONNECTION: DriveConnection = {
  id: "conn-1",
  accountEmail: "student@example.com",
  accountDisplayName: "理系学生",
  scopes: ["https://www.googleapis.com/auth/drive.file"],
  rootFolderDriveId: "drv-root-1",
  rootFolderName: "Theorem",
  uploadMode: "manual",
  versionPolicy: "update_latest",
  includePastVersions: false,
  includeSolutionSource: false,
  includeArtifacts: false,
  status: "connected",
  createdAt: "2026-11-26T00:00:00Z",
  updatedAt: "2026-11-26T00:00:00Z",
};

const SAMPLE_CONFLICT_JOB: DriveUploadJob = {
  id: "job-conflict-1",
  connectionId: "conn-1",
  documentId: "doc-1",
  logicalRole: "latest_solution_pdf",
  attachmentId: "att-pdf-2",
  driveFileId: "drv-file-sol-1",
  targetFilename: "2026微積分中間試験_解答.pdf",
  idempotencyKey: "idem-1",
  forceOverwrite: false,
  saveAsNew: false,
  status: "conflict",
  attemptCount: 1,
  nextRetryAt: null,
  lastErrorCode: "EXTERNAL_CONFLICT",
  lastErrorMessage: "Google Drive 上で外部変更が検出されたため上書きを停止しました。",
  conflictRemoteMd5: "deadbeef12345678",
  conflictRemoteModifiedTime: "2026-11-26T12:00:00Z",
  conflictRemoteVersion: "7",
  createdAt: "2026-11-26T11:00:00Z",
  updatedAt: "2026-11-26T12:01:00Z",
};

const SAMPLE_FAILED_JOB: DriveUploadJob = {
  id: "job-failed-1",
  connectionId: "conn-1",
  documentId: "doc-1",
  logicalRole: "original_problem",
  attachmentId: "att-orig-1",
  driveFileId: null,
  targetFilename: "2026微積分中間試験_問題.pdf",
  idempotencyKey: "idem-2",
  forceOverwrite: false,
  saveAsNew: false,
  status: "failed",
  attemptCount: 3,
  nextRetryAt: "2026-11-26T12:10:00Z",
  lastErrorCode: "NETWORK_ERROR",
  lastErrorMessage: "ネットワーク接続に失敗しました。",
  conflictRemoteMd5: null,
  conflictRemoteModifiedTime: null,
  conflictRemoteVersion: null,
  createdAt: "2026-11-26T11:00:00Z",
  updatedAt: "2026-11-26T12:00:00Z",
};

const SAMPLE_OVERVIEW: DriveOverview = {
  oauthConfig: {
    clientId: "123456.apps.googleusercontent.com",
    clientIdSource: "settings",
    hasClientSecret: true,
    requiredScope: "https://www.googleapis.com/auth/drive.file",
  },
  connection: SAMPLE_CONNECTION,
  jobs: [SAMPLE_CONFLICT_JOB, SAMPLE_FAILED_JOB],
  documentSummaries: [
    {
      documentId: "doc-1",
      aggregateStatus: "conflict",
      uploadedCount: 1,
      pendingCount: 0,
      conflictCount: 1,
      failedCount: 1,
    },
  ],
  pendingCount: 0,
  uploadingCount: 0,
  conflictCount: 1,
  failedCount: 1,
};

const SAMPLE_SOLUTION: Solution = {
  id: "sol-1",
  documentId: "doc-1",
  problemId: "prob-1",
  versionNumber: 1,
  parentSolutionId: null,
  isCurrent: true,
  mode: "detailed",
  schemaVersion: 1,
  structuredJson: "{}",
  validationStatus: "valid",
  typstSourceAttachmentId: "att-typ-1",
  pdfAttachmentId: "att-pdf-1",
  agentRunId: "run-1",
  modelName: "mock-math",
  changeSummary: null,
  createdAt: "2026-11-26T00:00:00Z",
  updatedAt: "2026-11-26T00:00:00Z",
};

describe("Theorem Phase 4 UI (Google Drive Settings, Conflict Dialog, Library & Version Export)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useLayoutStore.setState({
      settingsOpen: true,
      settingsCategory: "drive",
      activeTab: "versions",
    });
    useDriveStore.setState({
      overview: SAMPLE_OVERVIEW,
      documentStatusById: {},
      loading: false,
      connecting: false,
      uploadingDocId: null,
      processingQueue: false,
      activeConflictJob: null,
      lastNotice: null,
      error: null,
    });
    useLibraryStore.setState({
      subjects: [
        {
          id: "sub-math",
          name: "大学数学",
          category: "mathematics",
          description: null,
          sortOrder: 0,
          createdAt: "2026-11-26T00:00:00Z",
          updatedAt: "2026-11-26T00:00:00Z",
        },
      ],
      folders: [
        {
          id: "fld-calc",
          subjectId: "sub-math",
          parentId: null,
          name: "微積分",
          autoUploadDrive: false,
          sortOrder: 0,
          createdAt: "2026-11-26T00:00:00Z",
          updatedAt: "2026-11-26T00:00:00Z",
        },
      ],
      tags: [],
      documents: [
        {
          id: "doc-1",
          subjectId: "sub-math",
          folderId: "fld-calc",
          title: "2026微積分中間試験",
          sourceType: "pdf",
          primaryAttachmentId: "att-orig-1",
          thumbnailAttachmentId: null,
          pageCount: 2,
          recognitionMode: "printed_only",
          studyStatus: "in_progress",
          isFavorite: false,
          lastOpenedAt: null,
          tagIds: [],
          createdAt: "2026-11-26T00:00:00Z",
          updatedAt: "2026-11-26T00:00:00Z",
        },
      ],
      selectedSubjectId: null,
      selectedFolderId: null,
      selectedTagId: null,
      searchQuery: "",
      selectedDocumentId: "doc-1",
    });
    useAiWorkspaceStore.setState({
      activeDocumentId: "doc-1",
      solutions: [SAMPLE_SOLUTION],
      selectedSolutionId: "sol-1",
      savingSolution: false,
    });

    vi.mocked(tauriClient.getDriveOverview).mockResolvedValue(SAMPLE_OVERVIEW);
    vi.mocked(tauriClient.listDocumentDriveStatus).mockResolvedValue({
      documentId: "doc-1",
      files: [],
      jobs: [SAMPLE_CONFLICT_JOB, SAMPLE_FAILED_JOB],
    });
    vi.mocked(tauriClient.saveDriveOAuthConfig).mockResolvedValue(SAMPLE_OVERVIEW.oauthConfig);
    vi.mocked(tauriClient.updateDriveSettings).mockResolvedValue(SAMPLE_CONNECTION);
    vi.mocked(tauriClient.retryDriveJob).mockResolvedValue({
      ...SAMPLE_FAILED_JOB,
      status: "pending",
    });
    vi.mocked(tauriClient.cancelDriveJob).mockResolvedValue({
      ...SAMPLE_CONFLICT_JOB,
      status: "cancelled",
    });
    vi.mocked(tauriClient.resolveDriveConflict).mockResolvedValue({
      ...SAMPLE_CONFLICT_JOB,
      status: "pending",
      forceOverwrite: true,
    });
    vi.mocked(tauriClient.processDriveQueueNow).mockResolvedValue(1);
    vi.mocked(tauriClient.uploadDocumentToDrive).mockResolvedValue({
      enqueuedJobs: [SAMPLE_FAILED_JOB],
      skippedUpToDate: [],
      notices: [],
    });
    vi.mocked(tauriClient.setFolderAutoUpload).mockResolvedValue({
      id: "fld-calc",
      subjectId: "sub-math",
      parentId: null,
      name: "微積分",
      autoUploadDrive: true,
      sortOrder: 0,
      createdAt: "2026-11-26T00:00:00Z",
      updatedAt: "2026-11-26T00:01:00Z",
    });
  });

  it("renders Drive settings panel with drive.file scope, saves OAuth config, updates policies, and operates job queue", async () => {
    const user = userEvent.setup();
    render(
      <>
        <SettingsModal />
        <DriveConflictModal />
      </>,
    );

    // Scope & privacy invariants visible
    await waitFor(() => {
      expect(screen.getAllByText("https://www.googleapis.com/auth/drive.file").length).toBeGreaterThanOrEqual(1);
      expect(
        screen.getByText(/SQLite DB・AI 会話履歴・認証情報・ログは一切送信されません/),
      ).toBeInTheDocument();
      expect(screen.getByText("Google Drive 接続済み")).toBeInTheDocument();
      expect(screen.getByText(/理系学生 \(student@example\.com\)/)).toBeInTheDocument();
    });

    // Save OAuth client ID & secret
    const clientIdInput = screen.getByLabelText("Google OAuth クライアント ID");
    await user.clear(clientIdInput);
    await user.type(clientIdInput, "new-client.apps.googleusercontent.com");
    await user.type(screen.getByLabelText("Google OAuth クライアントシークレット"), "GOCSPX-secret");
    await user.click(screen.getByRole("button", { name: "OAuth 設定を保存" }));

    await waitFor(() => {
      expect(tauriClient.saveDriveOAuthConfig).toHaveBeenCalledWith({
        clientId: "new-client.apps.googleusercontent.com",
        clientSecret: "GOCSPX-secret",
        clearClientSecret: false,
      });
    });

    // Change upload mode & version policy
    await user.selectOptions(
      screen.getByLabelText("Google Drive アップロードモード"),
      "folder_auto",
    );
    await waitFor(() => {
      expect(tauriClient.updateDriveSettings).toHaveBeenCalledWith({
        uploadMode: "folder_auto",
      });
    });

    await user.selectOptions(
      screen.getByLabelText("Drive バージョン管理ポリシー"),
      "create_new",
    );
    await waitFor(() => {
      expect(tauriClient.updateDriveSettings).toHaveBeenCalledWith({
        versionPolicy: "create_new",
      });
    });

    // Retry failed job & open conflict modal from job queue
    await user.click(screen.getByRole("button", { name: "再試行" }));
    await waitFor(() => {
      expect(tauriClient.retryDriveJob).toHaveBeenCalledWith("job-failed-1");
    });

    await user.click(screen.getByRole("button", { name: "競合を解決" }));
    expect(screen.getByRole("dialog", { name: "Google Drive 競合の確認" })).toBeInTheDocument();
    expect(screen.getByText("deadbeef12345678")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "ローカルで上書き" }));
    await waitFor(() => {
      expect(tauriClient.resolveDriveConflict).toHaveBeenCalledWith({
        jobId: "job-conflict-1",
        resolution: "overwrite",
      });
    });
  });

  it("renders REMOTE_DELETED recovery action inside DriveConflictModal", async () => {
    const user = userEvent.setup();
    useDriveStore.setState({
      activeConflictJob: {
        ...SAMPLE_CONFLICT_JOB,
        id: "job-deleted-1",
        lastErrorCode: "REMOTE_DELETED",
        lastErrorMessage: "Google Drive 上で対象ファイルが削除されています。",
      },
    });

    render(<DriveConflictModal />);
    expect(screen.getByText("Google Drive 側でファイルが削除されています")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "新規ファイルとして再作成" }));
    await waitFor(() => {
      expect(tauriClient.resolveDriveConflict).toHaveBeenCalledWith({
        jobId: "job-deleted-1",
        resolution: "recreate_after_remote_delete",
      });
    });
  });

  it("supports folder auto-upload toggle, library Drive badges, and explicit version upload", async () => {
    const user = userEvent.setup();
    render(
      <>
        <LibrarySidebar onImportClick={vi.fn()} searchInputRef={{ current: null }} />
        <SolutionVersionsTab />
      </>,
    );

    // Library badge reflects document aggregate Drive status
    expect(screen.getByText("Drive 競合")).toBeInTheDocument();

    // Toggle per-folder Drive auto upload
    await user.click(screen.getByLabelText("Drive 自動アップロード切替: 微積分"));
    await waitFor(() => {
      expect(tauriClient.setFolderAutoUpload).toHaveBeenCalledWith("fld-calc", true);
    });

    // Quick upload from document list
    await user.click(screen.getByLabelText("Google Drive へアップロード: 2026微積分中間試験"));
    await waitFor(() => {
      expect(tauriClient.uploadDocumentToDrive).toHaveBeenCalledWith({
        documentId: "doc-1",
        scope: "default",
      });
    });

    // Version history explicit version upload
    await user.click(screen.getByRole("button", { name: "v1 を Drive へ保存" }));
    await waitFor(() => {
      expect(tauriClient.uploadDocumentToDrive).toHaveBeenCalledWith({
        documentId: "doc-1",
        solutionId: "sol-1",
        scope: "default",
      });
    });
  });
});
