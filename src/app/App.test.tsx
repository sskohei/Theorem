import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { tauriClient } from "@/services/tauriClient";
import { useDocumentStore } from "@/stores/useDocumentStore";
import { useLayoutStore } from "@/stores/useLayoutStore";
import { useLibraryStore } from "@/stores/useLibraryStore";
import type { BootstrapState, DocumentDetail, DocumentSummary } from "@/types/ipc";

vi.mock("@/services/tauriClient", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/services/tauriClient")>();
  return {
    ...actual,
    isTauriRuntime: () => false,
    tauriClient: {
      ...actual.tauriClient,
      getBootstrapState: vi.fn(),
      listDocuments: vi.fn(),
      getDocumentDetail: vi.fn(),
      readAttachmentBase64: vi.fn(),
      touchDocumentOpened: vi.fn(),
      createSubject: vi.fn(),
      updateSubject: vi.fn(),
      createFolder: vi.fn(),
      updateFolder: vi.fn(),
      createTag: vi.fn(),
      updateDocument: vi.fn(),
      importFilesFromPaths: vi.fn(),
      importFileBytes: vi.fn(),
      updateSettings: vi.fn(),
      getStorageStats: vi.fn(),
      listBackups: vi.fn(),
      createBackup: vi.fn(),
      cleanupTempFiles: vi.fn(),
      listAgentProfiles: vi.fn(),
      checkAgentStatus: vi.fn(),
      listProblems: vi.fn(),
      listSolutions: vi.fn(),
      listConversations: vi.fn(),
      listMessages: vi.fn(),
      listDocumentToolExecutions: vi.fn(),
      listSolutionVerifications: vi.fn(),
      listSolutionArtifacts: vi.fn(),
      checkScienceMcpStatus: vi.fn(),
    },
  };
});

const SAMPLE_DOC: DocumentSummary = {
  id: "doc-1",
  subjectId: "sub-math",
  folderId: "fld-calc",
  title: "2026微積分中間試験",
  sourceType: "image",
  primaryAttachmentId: "att-1",
  thumbnailAttachmentId: null,
  pageCount: 1,
  recognitionMode: "printed_only",
  studyStatus: "unstarted",
  isFavorite: false,
  lastOpenedAt: "2026-11-25T10:00:00Z",
  tagIds: ["tag-1"],
  createdAt: "2026-11-25T10:00:00Z",
  updatedAt: "2026-11-25T10:00:00Z",
};

const SAMPLE_BOOT: BootstrapState = {
  appVersion: "0.1.0",
  dataRootDisplay: "C:\\Users\\test\\AppData\\Roaming\\Theorem",
  subjects: [
    {
      id: "sub-math",
      name: "大学数学",
      category: "mathematics",
      description: null,
      sortOrder: 0,
      createdAt: "2026-11-25T00:00:00Z",
      updatedAt: "2026-11-25T00:00:00Z",
    },
    {
      id: "sub-phys",
      name: "大学物理",
      category: "physics",
      description: null,
      sortOrder: 1,
      createdAt: "2026-11-25T00:00:00Z",
      updatedAt: "2026-11-25T00:00:00Z",
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
      createdAt: "2026-11-25T00:00:00Z",
      updatedAt: "2026-11-25T00:00:00Z",
    },
    {
      id: "fld-la",
      subjectId: "sub-math",
      parentId: null,
      name: "線形代数",
      autoUploadDrive: false,
      sortOrder: 1,
      createdAt: "2026-11-25T00:00:00Z",
      updatedAt: "2026-11-25T00:00:00Z",
    },
  ],
  tags: [
    {
      id: "tag-1",
      name: "重要",
      color: "#2563eb",
      createdAt: "2026-11-25T00:00:00Z",
    },
  ],
  documents: [SAMPLE_DOC],
  settings: {
    language: "ja",
    theme: "light",
    defaultSubjectId: null,
    defaultRecognitionMode: "printed_only",
    leftSidebarWidth: 280,
    rightSidebarWidth: 320,
    cloudAiConsentAccepted: false,
    pdf: {
      defaultTemplate: "auto",
      paperSize: "a4",
      showModelName: true,
      showGeneratedAt: true,
      showVersionNumber: true,
    },
    agentAssignments: {
      extractionProfileId: null,
      solutionProfileId: null,
      chatProfileId: null,
      verificationProfileId: null,
      revisionProfileId: null,
    },
    prompts: {
      mathematicsCustomInstructions: "",
      physicsCustomInstructions: "",
      modeCustomInstructions: "",
    },
    scienceMcp: {
      enabled: true,
      uvPath: null,
      serverDir: null,
      timeoutSeconds: 10,
      defaultPlotFormat: "svg",
    },
  },
};

const SAMPLE_DETAIL: DocumentDetail = {
  document: SAMPLE_DOC,
  attachments: [
    {
      id: "att-1",
      documentId: "doc-1",
      kind: "original_image",
      originalFilename: "exam.png",
      relativePath: "attachments/originals/att-1.png",
      mimeType: "image/png",
      byteSize: 128,
      sha256: "abc123",
      width: 64,
      height: 48,
      pageCount: 1,
      createdAt: "2026-11-25T10:00:00Z",
    },
  ],
  tags: SAMPLE_BOOT.tags,
};

describe("Theorem App Shell & Stores", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useLayoutStore.setState({
      leftSidebarOpen: true,
      rightSidebarOpen: true,
      leftSidebarWidth: 280,
      rightSidebarWidth: 320,
      activeTab: "original",
      theme: "light",
      settingsOpen: false,
      settingsCategory: "general",
    });
    useLibraryStore.setState({
      initialized: false,
      loading: false,
      importing: false,
      subjects: [],
      folders: [],
      tags: [],
      documents: [],
      selectedSubjectId: null,
      selectedFolderId: null,
      selectedTagId: null,
      searchQuery: "",
      selectedDocumentId: null,
      duplicateNotice: null,
      error: null,
    });
    useDocumentStore.setState({
      detail: null,
      primaryBase64: null,
      loading: false,
      error: null,
    });

    vi.mocked(tauriClient.getBootstrapState).mockResolvedValue(SAMPLE_BOOT);
    vi.mocked(tauriClient.listDocuments).mockResolvedValue([SAMPLE_DOC]);
    vi.mocked(tauriClient.getDocumentDetail).mockResolvedValue(SAMPLE_DETAIL);
    vi.mocked(tauriClient.readAttachmentBase64).mockResolvedValue({
      attachment: SAMPLE_DETAIL.attachments[0]!,
      base64Data:
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
    });
    vi.mocked(tauriClient.touchDocumentOpened).mockResolvedValue("2026-11-25T10:05:00Z");
    vi.mocked(tauriClient.updateSettings).mockImplementation(async (s) => s);
    vi.mocked(tauriClient.getStorageStats).mockResolvedValue({
      dataRootDisplay: SAMPLE_BOOT.dataRootDisplay,
      databaseBytes: 4096,
      originalsBytes: 128,
      thumbnailsBytes: 64,
      backupsBytes: 0,
      logsBytes: 100,
      tempBytes: 0,
      totalBytes: 4388,
      documentCount: 1,
      attachmentCount: 1,
    });
    vi.mocked(tauriClient.listBackups).mockResolvedValue([]);
    vi.mocked(tauriClient.listAgentProfiles).mockResolvedValue([
      {
        id: "prof-codex",
        name: "OpenAI Codex",
        providerKind: "codex",
        executablePath: null,
        args: [],
        defaultModel: null,
        authMode: "official_harness",
        hasKeyringSecret: false,
        isEnabled: true,
        isDefault: true,
        createdAt: "2026-11-26T00:00:00Z",
        updatedAt: "2026-11-26T00:00:00Z",
      },
    ]);
    vi.mocked(tauriClient.listProblems).mockResolvedValue([]);
    vi.mocked(tauriClient.listSolutions).mockResolvedValue([]);
    vi.mocked(tauriClient.listConversations).mockResolvedValue([]);
    vi.mocked(tauriClient.listMessages).mockResolvedValue([]);
    vi.mocked(tauriClient.listDocumentToolExecutions).mockResolvedValue([]);
    vi.mocked(tauriClient.listSolutionVerifications).mockResolvedValue([]);
    vi.mocked(tauriClient.listSolutionArtifacts).mockResolvedValue([]);
  });

  it("renders 3-column workspace, subjects, folders, document list, and image viewer", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("大学数学")).toBeInTheDocument();
      expect(screen.getByText("大学物理")).toBeInTheDocument();
      expect(screen.getByText("微積分")).toBeInTheDocument();
      expect(screen.getByText("2026微積分中間試験")).toBeInTheDocument();
    });

    // Right sidebar Phase 2 placeholder is present
    expect(screen.getByText("AI チャット")).toBeInTheDocument();

    // Image viewer renders loaded original image
    await waitFor(() => {
      expect(screen.getByAltText("exam.png")).toBeInTheDocument();
    });
  });

  it("toggles left and right sidebars and switches center tabs", async () => {
    const user = userEvent.setup();
    render(<App />);
    await waitFor(() => expect(screen.getByText("大学数学")).toBeInTheDocument());

    await user.click(screen.getByLabelText("左サイドバーを開閉 (Ctrl+B)"));
    expect(screen.queryByText("問題ファイルを追加")).not.toBeInTheDocument();

    await user.click(screen.getByLabelText("右サイドバーを開閉 (Ctrl+J)"));
    expect(screen.queryByLabelText("AI チャット")).not.toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: /計算結果/ }));
    expect(screen.getByText("Science MCP 計算・検証・グラフ")).toBeInTheDocument();
  });

  it("opens settings modal and switches theme and storage category", async () => {
    const user = userEvent.setup();
    render(<App />);
    await waitFor(() => expect(screen.getByText("大学数学")).toBeInTheDocument());

    await user.click(screen.getAllByLabelText("設定")[0]!);
    expect(screen.getByRole("dialog", { name: "設定" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "ストレージ" }));
    await waitFor(() => {
      expect(screen.getByText("今すぐバックアップ作成")).toBeInTheDocument();
    });

    await user.click(screen.getByRole("button", { name: "外観" }));
    await user.click(screen.getByRole("button", { name: "ダーク" }));
    await waitFor(() => {
      expect(document.documentElement.classList.contains("dark")).toBe(true);
    });
  });

  it("displays duplicate notice and error banner clearly", async () => {
    render(<App />);
    await waitFor(() => expect(screen.getByText("大学数学")).toBeInTheDocument());

    act(() => {
      useLibraryStore.setState({
        duplicateNotice: [
          {
            documentId: "doc-1",
            documentTitle: "2026微積分中間試験",
            attachmentId: "att-1",
            originalFilename: "exam.png",
            sha256: "abc123",
          },
        ],
        error: {
          code: "UNSUPPORTED_FORMAT",
          message: "PDF / JPG / PNG / WebP 以外の形式は取り込めません",
          detail: "bad.txt",
          recoverable: true,
        },
      });
    });

    await waitFor(() => {
      expect(screen.getByText("重複ファイルを検出しました")).toBeInTheDocument();
      expect(screen.getByRole("alert")).toHaveTextContent("UNSUPPORTED_FORMAT");
    });
  });

  it("moves a folder to another parent folder and moves a document across subjects", async () => {
    const user = userEvent.setup();
    vi.mocked(tauriClient.updateFolder).mockResolvedValue({
      ...SAMPLE_BOOT.folders[0]!,
      parentId: "fld-la",
    });
    vi.mocked(tauriClient.updateDocument).mockResolvedValue({
      ...SAMPLE_DETAIL,
      document: { ...SAMPLE_DOC, subjectId: "sub-phys", folderId: null },
    });

    render(<App />);
    await waitFor(() => expect(screen.getByText("微積分")).toBeInTheDocument());

    // Move folder 微積分 under 線形代数
    await user.click(screen.getByLabelText("フォルダを移動: 微積分"));
    await user.selectOptions(screen.getByLabelText("移動先の親フォルダ"), "fld-la");
    await user.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() => {
      expect(tauriClient.updateFolder).toHaveBeenCalledWith("fld-calc", "微積分", "fld-la");
    });

    // Move document to 大学物理 with no folder
    await user.click(screen.getByLabelText("科目・フォルダを変更: 2026微積分中間試験"));
    await user.selectOptions(screen.getByLabelText("科目"), "sub-phys");
    await user.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() => {
      expect(tauriClient.updateDocument).toHaveBeenCalledWith({
        id: "doc-1",
        subjectId: "sub-phys",
        folderId: null,
      });
    });
  });

  it("filters documents via search input and handles clipboard image paste import", async () => {
    const user = userEvent.setup();
    vi.mocked(tauriClient.importFileBytes).mockResolvedValue({
      detail: {
        ...SAMPLE_DETAIL,
        document: { ...SAMPLE_DOC, id: "doc-clip", title: "clipboard" },
      },
      duplicateMatches: [],
    });

    render(<App />);
    await waitFor(() => expect(screen.getByText("2026微積分中間試験")).toBeInTheDocument());

    const search = screen.getByLabelText("問題名・ファイル名・タグで検索 (Ctrl+K)");
    await user.type(search, "微積分");

    await waitFor(() => {
      expect(tauriClient.listDocuments).toHaveBeenLastCalledWith(
        expect.objectContaining({ searchQuery: "微積分" }),
      );
    });

    // Trigger import via store (representing paste / file drop)
    await act(async () => {
      await useLibraryStore.getState().importFromBytes("paste.png", "AAAA");
    });
    await waitFor(() => {
      expect(tauriClient.importFileBytes).toHaveBeenCalledWith(
        "sub-math",
        null,
        "paste.png",
        "AAAA",
      );
      expect(useLibraryStore.getState().selectedDocumentId).toBe("doc-clip");
    });
  });
});
