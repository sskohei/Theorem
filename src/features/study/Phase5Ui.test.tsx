import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ChatSidebar } from "@/features/chat/ChatSidebar";
import { CenterWorkspace } from "@/features/document/CenterWorkspace";
import { RecognizedProblemsTab } from "@/features/document/RecognizedProblemsTab";
import { SolutionVersionsTab } from "@/features/document/SolutionVersionsTab";
import { LibrarySidebar } from "@/features/library/LibrarySidebar";
import { tauriClient } from "@/services/tauriClient";
import { useAgentStore } from "@/stores/useAgentStore";
import { useAiWorkspaceStore } from "@/stores/useAiWorkspaceStore";
import { useChatStore } from "@/stores/useChatStore";
import { useDocumentStore } from "@/stores/useDocumentStore";
import { useDriveStore } from "@/stores/useDriveStore";
import { useLayoutStore } from "@/stores/useLayoutStore";
import { useLibraryStore } from "@/stores/useLibraryStore";
import { useStudyStore } from "@/stores/useStudyStore";
import type { AgentProfile, ChatMessage, DocumentSummary, Problem, Solution } from "@/types/ipc";

vi.mock("@/services/tauriClient", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/services/tauriClient")>();
  return {
    ...actual,
    isTauriRuntime: () => false,
    tauriClient: {
      ...actual.tauriClient,
      proposeSolutionRevision: vi.fn(),
      decideSolutionRevision: vi.fn(),
      compareSolutionVersions: vi.fn(),
      compareAgentsGenerateSolutions: vi.fn(),
      listReferenceMaterials: vi.fn(),
      createReferenceMaterial: vi.fn(),
      deleteReferenceMaterial: vi.fn(),
      fullTextSearch: vi.fn(),
      generateSolution: vi.fn(),
      listSolutions: vi.fn(),
      listProblems: vi.fn(),
      listMessages: vi.fn(),
      listDocuments: vi.fn(),
      updateDocument: vi.fn(),
      readAttachmentBase64: vi.fn(),
    },
  };
});

const DOC_1: DocumentSummary = {
  id: "doc-1",
  subjectId: "sub-math",
  folderId: null,
  title: "ガウス積分演習",
  sourceType: "image",
  primaryAttachmentId: "att-orig",
  thumbnailAttachmentId: null,
  pageCount: 1,
  recognitionMode: "printed_only",
  studyStatus: "in_progress",
  isFavorite: false,
  lastOpenedAt: "2026-11-26T00:00:00Z",
  tagIds: [],
  createdAt: "2026-11-26T00:00:00Z",
  updatedAt: "2026-11-26T00:00:00Z",
};

const PROB_1: Problem = {
  id: "prob-1",
  documentId: "doc-1",
  problemNumber: "第1問",
  subproblemNumber: "(1)",
  sortOrder: 0,
  subjectCategory: "mathematics",
  field: "calculus",
  statementMarkdown: "ガウス積分を求めよ。",
  givenConditions: [],
  targetObjective: "厳密値",
  sourcePages: [1],
  figureRefs: [],
  uncertainties: [],
  isUserVerified: true,
  createdAt: "2026-11-26T00:00:00Z",
  updatedAt: "2026-11-26T00:00:00Z",
};

const SOL_V1: Solution = {
  id: "sol-1",
  documentId: "doc-1",
  problemId: "prob-1",
  versionNumber: 1,
  parentSolutionId: null,
  isCurrent: false,
  mode: "detailed",
  schemaVersion: 1,
  structuredJson: "{}",
  validationStatus: "valid",
  typstSourceAttachmentId: "att-typ-1",
  pdfAttachmentId: "att-pdf-1",
  agentRunId: "run-1",
  modelName: "mock-math-pro",
  changeSummary: null,
  createdAt: "2026-11-26T00:00:00Z",
  updatedAt: "2026-11-26T00:00:00Z",
};

const SOL_V2: Solution = {
  ...SOL_V1,
  id: "sol-2",
  versionNumber: 2,
  parentSolutionId: "sol-1",
  isCurrent: true,
  changeSummary: "ヤコビアン補足",
};

const PROFILES: AgentProfile[] = [
  {
    id: "prof-a",
    name: "Mock A",
    providerKind: "mock",
    executablePath: null,
    args: [],
    defaultModel: "mock-math-pro",
    authMode: "official_harness",
    hasKeyringSecret: false,
    isEnabled: true,
    isDefault: true,
    createdAt: "2026-11-26T00:00:00Z",
    updatedAt: "2026-11-26T00:00:00Z",
  },
  {
    id: "prof-b",
    name: "Mock B",
    providerKind: "mock",
    executablePath: null,
    args: [],
    defaultModel: "mock-fast",
    authMode: "official_harness",
    hasKeyringSecret: false,
    isEnabled: true,
    isDefault: false,
    createdAt: "2026-11-26T00:00:00Z",
    updatedAt: "2026-11-26T00:00:00Z",
  },
  {
    id: "prof-claude",
    name: "Claude Code",
    providerKind: "claude_code",
    executablePath: null,
    args: [],
    defaultModel: null,
    authMode: "official_harness",
    hasKeyringSecret: false,
    isEnabled: true,
    isDefault: false,
    createdAt: "2026-11-26T00:00:00Z",
    updatedAt: "2026-11-26T00:00:00Z",
  },
];

const REVISION_MSG: ChatMessage = {
  id: "msg-rev-1",
  conversationId: "conv-1",
  role: "assistant",
  contentMarkdown: "v2 に対する 2 件の差分提案を作成しました。",
  status: "completed",
  agentRunId: "run-rev",
  referencedProblemId: "prob-1",
  attachmentIds: [],
  revisionStatus: "pending",
  revisionPatch: {
    baseSolutionId: "sol-2",
    baseVersionNumber: 2,
    targetProblemId: "prob-1",
    summary: "ヤコビアン補足",
    proposedJson: "{}",
    hunks: [
      {
        hunkId: "steps:2",
        fieldPath: "steps[2]",
        label: "解法ステップ 3",
        changeKind: "added",
        beforeJson: null,
        afterJson: {},
        beforeSummary: "(なし)",
        afterSummary: "極座標変換のヤコビアン r を明記",
      },
      {
        hunkId: "answer",
        fieldPath: "answer",
        label: "最終解答",
        changeKind: "modified",
        beforeJson: {},
        afterJson: {},
        beforeSummary: "旧要約",
        afterSummary: "新要約",
      },
    ],
  },
  createdAt: "2026-11-26T00:00:00Z",
  updatedAt: "2026-11-26T00:00:00Z",
};

describe("Theorem Phase 5 UI (Revision Diff Approval, Review/References, Multi-AI, FTS5, Split View)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useLayoutStore.setState({
      activeTab: "original",
      splitView: false,
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
      folders: [],
      tags: [],
      documents: [DOC_1],
      selectedDocumentId: "doc-1",
      selectedStudyStatus: null,
      searchQuery: "",
      ftsHits: [],
      ftsLoading: false,
    });
    useDocumentStore.setState({
      detail: {
        document: DOC_1,
        attachments: [
          {
            id: "att-orig",
            documentId: "doc-1",
            kind: "original_image",
            originalFilename: "prob.png",
            relativePath: "attachments/originals/att-orig.png",
            mimeType: "image/png",
            byteSize: 64,
            sha256: "abc",
            width: 64,
            height: 64,
            pageCount: 1,
            createdAt: "2026-11-26T00:00:00Z",
          },
        ],
        tags: [],
      },
      primaryBase64: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
      loading: false,
      error: null,
    });
    useAgentStore.setState({ profiles: PROFILES, statusesByProfileId: {} });
    useAiWorkspaceStore.setState({
      activeDocumentId: "doc-1",
      problems: [PROB_1],
      selectedProblemId: "prob-1",
      solutions: [SOL_V2, SOL_V1],
      selectedSolutionId: "sol-2",
      solutionPdfAttachment: null,
      solutionPdfBase64: null,
      loadingPdf: false,
      savingSolution: false,
      generatingSolution: false,
    });
    useChatStore.setState({
      activeDocumentId: "doc-1",
      conversations: [
        {
          id: "conv-1",
          documentId: "doc-1",
          problemId: "prob-1",
          solutionId: "sol-2",
          title: "チャット 1",
          createdAt: "2026-11-26T00:00:00Z",
          updatedAt: "2026-11-26T00:00:00Z",
        },
      ],
      activeConversationId: "conv-1",
      messages: [REVISION_MSG],
      sending: false,
      streamingText: "",
      liveToolCalls: [],
      pendingPermissions: [],
      error: null,
    });
    useDriveStore.setState({ overview: null, documentStatusById: {} });
    useStudyStore.setState({
      references: [],
      selectedReferenceIds: [],
      versionDiff: null,
      comparingVersions: false,
      agentComparison: null,
      comparingAgents: false,
      error: null,
    });

    vi.mocked(tauriClient.listProblems).mockResolvedValue([PROB_1]);
    vi.mocked(tauriClient.listSolutions).mockResolvedValue([SOL_V2, SOL_V1]);
    vi.mocked(tauriClient.listDocuments).mockResolvedValue([DOC_1]);
    vi.mocked(tauriClient.listMessages).mockResolvedValue([REVISION_MSG]);
  });

  it("renders RevisionDiffCard in ChatSidebar and supports selective hunk approval", async () => {
    const user = userEvent.setup();
    vi.mocked(tauriClient.decideSolutionRevision).mockResolvedValue({
      message: {
        ...REVISION_MSG,
        revisionStatus: "approved",
        revisionPatch: {
          ...REVISION_MSG.revisionPatch!,
          appliedSolutionId: "sol-3",
          appliedVersionNumber: 3,
        },
      },
      newSolution: {
        solution: { ...SOL_V2, id: "sol-3", versionNumber: 3 },
        schemaIssues: [],
        latexErrors: [],
        pdfError: null,
        rawOutput: null,
      },
    });

    render(<ChatSidebar />);

    expect(screen.getByLabelText("解答修正差分カード")).toBeInTheDocument();
    expect(screen.getByText("極座標変換のヤコビアン r を明記")).toBeInTheDocument();

    // Uncheck the `answer` hunk so ONLY `steps:2` is approved
    await user.click(screen.getByLabelText("差分を選択: 最終解答"));
    await user.click(screen.getByRole("button", { name: /承認して新バージョン作成 \(1\/2\)/ }));

    await waitFor(() => {
      expect(tauriClient.decideSolutionRevision).toHaveBeenCalledWith({
        messageId: "msg-rev-1",
        decision: "approve",
        approvedHunkIds: ["steps:2"],
      });
    });
  });

  it("supports handwritten_answer mode, review attempt input, and untrusted reference materials", async () => {
    const user = userEvent.setup();
    vi.mocked(tauriClient.createReferenceMaterial).mockResolvedValue({
      id: "ref-1",
      subjectId: "sub-math",
      documentId: "doc-1",
      title: "講義ノート第3回",
      kind: "lecture_note",
      contentMarkdown: "トネリ・フビニの定理",
      attachmentId: null,
      createdAt: "2026-11-26T00:00:00Z",
      updatedAt: "2026-11-26T00:00:00Z",
    });
    vi.mocked(tauriClient.generateSolution).mockResolvedValue({
      solution: { ...SOL_V2, mode: "review", versionNumber: 3 },
      schemaIssues: [],
      latexErrors: [],
      pdfError: null,
      rawOutput: null,
    });

    render(<RecognizedProblemsTab onTriggerRecognize={vi.fn().mockResolvedValue(undefined)} />);

    // Select handwritten_answer recognition mode & review solution mode
    await user.selectOptions(screen.getByLabelText("問題認識モード"), "handwritten_answer");
    await user.selectOptions(screen.getByLabelText("解答生成モード"), "review");
    await user.type(screen.getByLabelText("添削対象のユーザー答案"), "x^2 の積分を x^2/2 とした");

    // Add reference material
    await user.click(screen.getByRole("button", { name: "講義資料を追加" }));
    await user.type(screen.getByLabelText("参考資料タイトル"), "講義ノート第3回");
    await user.type(screen.getByLabelText("参考資料内容"), "トネリ・フビニの定理");
    await user.click(screen.getByRole("button", { name: "資料を保存" }));

    await waitFor(() => {
      expect(tauriClient.createReferenceMaterial).toHaveBeenCalledWith(
        expect.objectContaining({ title: "講義ノート第3回", kind: "lecture_note" }),
      );
    });

    await user.click(screen.getByRole("button", { name: "解答を生成する" }));
    await waitFor(() => {
      expect(tauriClient.generateSolution).toHaveBeenCalledWith(
        expect.objectContaining({
          mode: "review",
          userAttemptMarkdown: "x^2 の積分を x^2/2 とした",
          referenceMaterialIds: ["ref-1"],
        }),
      );
    });
  });

  it("compares solution versions and runs compliant multi-agent comparison in SolutionVersionsTab", async () => {
    const user = userEvent.setup();
    vi.mocked(tauriClient.compareSolutionVersions).mockResolvedValue({
      baseSolutionId: "sol-1",
      baseVersionNumber: 1,
      targetSolutionId: "sol-2",
      targetVersionNumber: 2,
      addedCount: 1,
      modifiedCount: 0,
      removedCount: 0,
      hunks: [
        {
          hunkId: "steps:2",
          fieldPath: "steps[2]",
          label: "解法ステップ 3",
          changeKind: "added",
          beforeJson: null,
          afterJson: {},
          beforeSummary: "(なし)",
          afterSummary: "ヤコビアン追記",
        },
      ],
    });
    vi.mocked(tauriClient.compareAgentsGenerateSolutions).mockResolvedValue({
      entries: [
        {
          profileId: "prof-a",
          profileName: "Mock A",
          providerKind: "mock",
          modelName: "mock-math-pro",
          solutionResult: {
            solution: { ...SOL_V2, versionNumber: 3 },
            schemaIssues: [],
            latexErrors: [],
            pdfError: null,
            rawOutput: null,
          },
          error: null,
        },
        {
          profileId: "prof-b",
          profileName: "Mock B",
          providerKind: "mock",
          modelName: "mock-fast",
          solutionResult: {
            solution: { ...SOL_V2, id: "sol-4", versionNumber: 4 },
            schemaIssues: [],
            latexErrors: [],
            pdfError: null,
            rawOutput: null,
          },
          error: null,
        },
      ],
      diffBetweenFirstTwo: {
        baseSolutionId: "sol-3",
        baseVersionNumber: 3,
        targetSolutionId: "sol-4",
        targetVersionNumber: 4,
        addedCount: 0,
        modifiedCount: 1,
        removedCount: 0,
        hunks: [],
      },
    });

    render(<SolutionVersionsTab />);

    // Version diff comparison
    await user.click(screen.getByRole("button", { name: "差分を表示" }));
    await waitFor(() => {
      expect(tauriClient.compareSolutionVersions).toHaveBeenCalledWith("sol-1", "sol-2");
      expect(screen.getByText("+ ヤコビアン追記")).toBeInTheDocument();
    });

    // Multi-AI comparison: Claude official_harness is disabled with 規約制限
    expect(screen.getByText("規約制限")).toBeInTheDocument();
    await user.click(screen.getByLabelText(/Mock A/));
    await user.click(screen.getByLabelText(/Mock B/));
    await user.click(screen.getByRole("button", { name: /選択した 2 AI で比較生成/ }));

    await waitFor(() => {
      expect(tauriClient.compareAgentsGenerateSolutions).toHaveBeenCalledWith(
        expect.objectContaining({ profileIds: ["prof-a", "prof-b"] }),
      );
      expect(screen.getByText(/先頭 2 エージェント間の構造化差分/)).toBeInTheDocument();
    });
  });

  it("supports study status switching, side-by-side split view, and FTS5 full-text search", async () => {
    const user = userEvent.setup();
    vi.mocked(tauriClient.updateDocument).mockResolvedValue({
      document: { ...DOC_1, studyStatus: "solved" },
      attachments: [],
      tags: [],
    });
    vi.mocked(tauriClient.fullTextSearch).mockResolvedValue([
      {
        entityType: "solution",
        entityId: "sol-2",
        documentId: "doc-1",
        documentTitle: "ガウス積分演習",
        title: "v2 (detailed)",
        snippet: "…極座標変換の【ヤコビアン】…",
        studyStatus: "in_progress",
        isFavorite: false,
      },
    ]);

    render(
      <>
        <LibrarySidebar onImportClick={vi.fn()} searchInputRef={{ current: null }} />
        <CenterWorkspace onImportClick={vi.fn()} />
      </>,
    );

    // Study status filter pill + switcher
    await user.selectOptions(screen.getByLabelText("学習ステータス"), "solved");
    await waitFor(() => {
      expect(tauriClient.updateDocument).toHaveBeenCalledWith({
        id: "doc-1",
        studyStatus: "solved",
      });
    });

    // Toggle side-by-side Split View (Original Problem + Solution PDF)
    await user.click(screen.getByRole("button", { name: "並置表示" }));
    expect(screen.getByLabelText("問題・解答並置ビュー")).toBeInTheDocument();

    // FTS5 Cross-Entity Full-Text Search
    const search = screen.getByLabelText("問題名・ファイル名・タグで検索 (Ctrl+K)");
    await user.type(search, "ヤコビアン");
    await user.click(screen.getByRole("button", { name: "全文" }));
    await waitFor(() => {
      expect(tauriClient.fullTextSearch).toHaveBeenCalledWith(
        expect.objectContaining({ query: "ヤコビアン" }),
      );
      expect(screen.getByLabelText("全文検索結果")).toHaveTextContent("…極座標変換の【ヤコビアン】…");
    });
  });
});
