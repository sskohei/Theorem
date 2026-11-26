import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ChatSidebar } from "@/features/chat/ChatSidebar";
import { SettingsModal } from "@/features/settings/SettingsModal";
import { CalculationsTab } from "@/features/verification/CalculationsTab";
import { tauriClient } from "@/services/tauriClient";
import { useAgentStore } from "@/stores/useAgentStore";
import { useAiWorkspaceStore } from "@/stores/useAiWorkspaceStore";
import { useChatStore } from "@/stores/useChatStore";
import { useLayoutStore } from "@/stores/useLayoutStore";
import { useLibraryStore } from "@/stores/useLibraryStore";
import { useScienceStore } from "@/stores/useScienceStore";
import type {
  Solution,
  SolutionArtifactItem,
  ToolExecutionRecord,
  VerificationResultRecord,
} from "@/types/ipc";

vi.mock("@/services/tauriClient", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/services/tauriClient")>();
  return {
    ...actual,
    isTauriRuntime: () => false,
    tauriClient: {
      ...actual.tauriClient,
      checkScienceMcpStatus: vi.fn(),
      listDocumentToolExecutions: vi.fn(),
      listSolutionVerifications: vi.fn(),
      listSolutionArtifacts: vi.fn(),
      runScienceTool: vi.fn(),
      attachArtifactToSolution: vi.fn(),
      readAttachmentBase64: vi.fn(),
      listProblems: vi.fn(),
      listSolutions: vi.fn(),
      updateSettings: vi.fn(),
    },
  };
});

const SAMPLE_SOL: Solution = {
  id: "sol-phys-1",
  documentId: "doc-phys",
  problemId: "prob-phys-1",
  versionNumber: 1,
  parentSolutionId: null,
  isCurrent: true,
  mode: "verified",
  schemaVersion: 1,
  structuredJson: JSON.stringify({
    schema_version: 1,
    subject: "physics",
    title: "減衰振動",
    mode: "verified",
    steps: [{ type: "equation", latex: "x(t) = e^{-t} \\cos(2t)" }],
    answer: { latex: "x(t) = e^{-t} \\cos(2t)" },
  }),
  validationStatus: "valid",
  typstSourceAttachmentId: "att-typ",
  pdfAttachmentId: "att-pdf",
  agentRunId: "run-1",
  modelName: "mock-math-pro",
  changeSummary: null,
  createdAt: "2026-11-26T00:00:00Z",
  updatedAt: "2026-11-26T00:00:00Z",
};

const SAMPLE_VERIFICATIONS: VerificationResultRecord[] = [
  {
    id: "ver-1",
    solutionId: "sol-phys-1",
    toolExecutionId: "exec-1",
    sourceKind: "science_mcp",
    targetDescription: "運動方程式の次元整合性",
    assumptions: ["[m] = kg", "[a] = m/s**2"],
    method: "pint.dimensionality 一致検証",
    status: "passed",
    detailJson: JSON.stringify({ lhs_dimension: "[M][L][T]^-2" }),
    createdAt: "2026-11-26T00:00:01Z",
  },
  {
    id: "ver-2",
    solutionId: "sol-phys-1",
    toolExecutionId: "exec-2",
    sourceKind: "science_mcp",
    targetDescription: "エネルギー式比較",
    assumptions: [],
    method: "pint.dimensionality 一致検証",
    status: "failed",
    detailJson: null,
    createdAt: "2026-11-26T00:00:02Z",
  },
  {
    id: "ver-3",
    solutionId: "sol-phys-1",
    toolExecutionId: null,
    sourceKind: "ai_self_check",
    targetDescription: "AI 自己チェック初期条件",
    assumptions: [],
    method: "代入確認",
    status: "inconclusive",
    detailJson: null,
    createdAt: "2026-11-26T00:00:03Z",
  },
];

const SAMPLE_EXECUTIONS: ToolExecutionRecord[] = [
  {
    id: "exec-1",
    agentRunId: "run-1",
    documentId: "doc-phys",
    solutionId: "sol-phys-1",
    serverName: "science_mcp",
    toolName: "check_dimensions",
    argumentsJson: JSON.stringify({ lhs_expression: "F", rhs_expression: "m * a" }),
    resultJson: JSON.stringify({ ok: true, result: { consistent: true } }),
    status: "succeeded",
    durationMs: 42,
    errorMessage: null,
    createdAt: "2026-11-26T00:00:01Z",
    finishedAt: "2026-11-26T00:00:01Z",
  },
];

const SAMPLE_ARTIFACTS: SolutionArtifactItem[] = [
  {
    id: "sart-1",
    solutionId: "sol-phys-1",
    attachmentId: "att-plot-1",
    artifactType: "plot",
    caption: "減衰振動 x(t) グラフ",
    sourceToolExecutionId: "exec-1",
    sortOrder: 0,
    relativePath: "attachments/plots/plot-1.svg",
    mimeType: "image/svg+xml",
    originalFilename: "plot-1.svg",
    byteSize: 256,
    createdAt: "2026-11-26T00:00:02Z",
  },
];

describe("Theorem Phase 3 UI (CalculationsTab, Science MCP Settings & Chat Tool Status)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useLibraryStore.setState({ selectedDocumentId: "doc-phys" });
    useAiWorkspaceStore.setState({
      activeDocumentId: "doc-phys",
      solutions: [SAMPLE_SOL],
      selectedSolutionId: "sol-phys-1",
    });
    useScienceStore.setState({
      status: null,
      checkingStatus: false,
      activeDocumentId: "doc-phys",
      activeSolutionId: "sol-phys-1",
      toolExecutions: [],
      verifications: [],
      artifacts: [],
      artifactPreviews: {},
      loading: false,
      runningTool: false,
      attachingArtifactId: null,
      error: null,
    });
    useAgentStore.setState({
      profiles: [
        {
          id: "prof-mock",
          name: "Mock Agent",
          providerKind: "mock",
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
      ],
      statusesByProfileId: {},
    });
    useChatStore.setState({
      activeDocumentId: "doc-phys",
      conversations: [],
      activeConversationId: null,
      messages: [],
      sending: false,
      activeRunId: null,
      streamingText: "",
      liveToolCalls: [],
      pendingPermissions: [],
      error: null,
    });

    vi.mocked(tauriClient.listDocumentToolExecutions).mockResolvedValue(SAMPLE_EXECUTIONS);
    vi.mocked(tauriClient.listSolutionVerifications).mockResolvedValue(SAMPLE_VERIFICATIONS);
    vi.mocked(tauriClient.listSolutionArtifacts).mockResolvedValue(SAMPLE_ARTIFACTS);
    vi.mocked(tauriClient.readAttachmentBase64).mockResolvedValue({
      attachment: {
        id: "att-plot-1",
        documentId: "doc-phys",
        kind: "plot",
        originalFilename: "plot-1.svg",
        relativePath: "attachments/plots/plot-1.svg",
        mimeType: "image/svg+xml",
        byteSize: 256,
        sha256: "abc",
        width: null,
        height: null,
        pageCount: null,
        createdAt: "2026-11-26T00:00:02Z",
      },
      base64Data: btoa('<svg xmlns="http://www.w3.org/2000/svg"></svg>'),
    });
    vi.mocked(tauriClient.listProblems).mockResolvedValue([]);
    vi.mocked(tauriClient.listSolutions).mockResolvedValue([SAMPLE_SOL]);
    vi.mocked(tauriClient.updateSettings).mockImplementation(async (s) => s);
  });

  it("renders verification sources distinctly, shows SVG plot preview, and triggers embed/run", async () => {
    const user = userEvent.setup();
    vi.mocked(tauriClient.attachArtifactToSolution).mockResolvedValue({
      solution: { ...SAMPLE_SOL, id: "sol-phys-2", versionNumber: 2 },
      schemaIssues: [],
      latexErrors: [],
      pdfError: null,
      rawOutput: null,
    });
    vi.mocked(tauriClient.runScienceTool).mockResolvedValue({
      execution: SAMPLE_EXECUTIONS[0]!,
      verification: SAMPLE_VERIFICATIONS[0]!,
      createdArtifacts: [],
      updatedSolution: null,
    });

    render(<CalculationsTab />);

    await waitFor(() => {
      expect(screen.getAllByText("実ツール検証 (Science MCP)")).toHaveLength(2);
      expect(screen.getByText("AI 自己チェック（ツール未検証）")).toBeInTheDocument();
      expect(screen.getByText("合格 (passed)")).toBeInTheDocument();
      expect(screen.getByText("不一致・反例検出 (failed)")).toBeInTheDocument();
      expect(screen.getByText("判定不能 (inconclusive)")).toBeInTheDocument();
      expect(screen.getByAltText("減衰振動 x(t) グラフ")).toBeInTheDocument();
    });

    // Limitation disclaimer is visible
    expect(
      screen.getByText(/数学的証明全体や物理モデル・近似の妥当性を保証するものではありません/),
    ).toBeInTheDocument();

    // Expand tool execution history
    await user.click(screen.getByRole("button", { name: /check_dimensions/ }));
    expect(screen.getByText("入力引数")).toBeInTheDocument();

    // Embed plot artifact into PDF
    await user.click(screen.getByRole("button", { name: "解答 PDF に埋め込む" }));
    await waitFor(() => {
      expect(tauriClient.attachArtifactToSolution).toHaveBeenCalledWith(
        "sol-phys-1",
        "att-plot-1",
        "減衰振動 x(t) グラフ",
      );
    });

    // Direct tool execution button
    await user.click(screen.getByRole("button", { name: "計算を実行" }));
    await waitFor(() => {
      expect(tauriClient.runScienceTool).toHaveBeenCalledWith(
        expect.objectContaining({
          documentId: "doc-phys",
          solutionId: "sol-phys-1",
          toolName: "verify_equation",
        }),
      );
    });
  });

  it("probes Science MCP server status and updates settings in SettingsModal", async () => {
    const user = userEvent.setup();
    vi.mocked(tauriClient.checkScienceMcpStatus).mockResolvedValue({
      enabled: true,
      uvAvailable: true,
      uvPath: "C:\\uv.exe",
      uvVersion: "uv 0.13.10",
      serverDir: "C:\\Theorem\\science-mcp",
      serverReady: true,
      toolCount: 21,
      tools: [
        { name: "differentiate", description: "Derivative" },
        { name: "check_dimensions", description: "Dimensional analysis" },
        { name: "plot_function", description: "Plot" },
      ],
      securityNote: "AST whitelist",
      detailNote: null,
    });

    useLayoutStore.setState({ settingsOpen: true, settingsCategory: "scienceMcp" });
    render(<SettingsModal />);

    await user.click(screen.getByRole("button", { name: "Science MCP の状態を確認" }));
    await waitFor(() => {
      expect(screen.getByText(/稼働可能（全 21 ツール検出）/)).toBeInTheDocument();
      expect(screen.getByText("check_dimensions")).toBeInTheDocument();
    });

    await user.selectOptions(screen.getByLabelText("グラフ出力形式"), "png");
    await waitFor(() => {
      expect(tauriClient.updateSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          scienceMcp: expect.objectContaining({ defaultPlotFormat: "png" }),
        }),
      );
    });
  });

  it("displays live Science MCP tool calls inside ChatSidebar during streaming", async () => {
    const user = userEvent.setup();
    render(<ChatSidebar />);

    act(() => {
      useChatStore.setState({ sending: true });
      useChatStore.getState().handleAgentEvent({
        eventId: "ev-t1",
        runId: "run-chat-1",
        sessionId: "sess-1",
        messageId: "msg-1",
        timestamp: "2026-11-26T00:00:00Z",
        event: {
          type: "toolCallStarted",
          callId: "call-1",
          server: "science_mcp",
          toolName: "solve_ode",
          arguments: { equation: "Derivative(y(t), t) + y(t) = 0" },
        },
      });
      useChatStore.getState().handleAgentEvent({
        eventId: "ev-t2",
        runId: "run-chat-1",
        sessionId: "sess-1",
        messageId: "msg-1",
        timestamp: "2026-11-26T00:00:01Z",
        event: {
          type: "toolCallCompleted",
          callId: "call-1",
          succeeded: true,
          output: { ok: true, result: { primary_expression: "C1*exp(-t)" } },
        },
      });
    });

    expect(screen.getByText("計算ツール実行状況 (1)")).toBeInTheDocument();
    expect(screen.getByText("solve_ode")).toBeInTheDocument();
    expect(screen.getByText("完了")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /solve_ode/ }));
    expect(screen.getByText(/C1\*exp\(-t\)/)).toBeInTheDocument();
  });
});
