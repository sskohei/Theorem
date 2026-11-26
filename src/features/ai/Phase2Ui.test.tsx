import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ChatSidebar } from "@/features/chat/ChatSidebar";
import { SolutionEditorTab } from "@/features/document/SolutionEditorTab";
import { SolutionVersionsTab } from "@/features/document/SolutionVersionsTab";
import { SettingsModal } from "@/features/settings/SettingsModal";
import { tauriClient } from "@/services/tauriClient";
import { useAgentStore } from "@/stores/useAgentStore";
import { useAiWorkspaceStore } from "@/stores/useAiWorkspaceStore";
import { useChatStore } from "@/stores/useChatStore";
import { useLayoutStore } from "@/stores/useLayoutStore";
import { useLibraryStore } from "@/stores/useLibraryStore";
import type { AgentEventPayload, AgentProfile, ChatMessage, Conversation, SendChatMessageResult, Solution } from "@/types/ipc";

vi.mock("@/services/tauriClient", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/services/tauriClient")>();
  return {
    ...actual,
    isTauriRuntime: () => false,
    tauriClient: {
      ...actual.tauriClient,
      listAgentProfiles: vi.fn(),
      checkAgentStatus: vi.fn(),
      saveAgentProfile: vi.fn(),
      updateSettings: vi.fn(),
      createConversation: vi.fn(),
      sendChatMessage: vi.fn(),
      listMessages: vi.fn(),
      listConversations: vi.fn(),
      cancelAgentRun: vi.fn(),
      respondAgentPermission: vi.fn(),
      saveSolutionEdit: vi.fn(),
      restoreSolutionVersion: vi.fn(),
      listSolutions: vi.fn(),
      readAttachmentBase64: vi.fn(),
    },
  };
});

const SAMPLE_PROFILES: AgentProfile[] = [
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
  {
    id: "prof-claude",
    name: "Anthropic Claude Code",
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

const SAMPLE_SOL_V1: Solution = {
  id: "sol-1",
  documentId: "doc-1",
  problemId: "prob-1",
  versionNumber: 1,
  parentSolutionId: null,
  isCurrent: true,
  mode: "detailed",
  schemaVersion: 1,
  structuredJson: JSON.stringify(
    {
      schema_version: 1,
      subject: "mathematics",
      title: "ガウス積分の解答",
      mode: "detailed",
      steps: [{ type: "equation", text: "極座標変換", latex: "\\int_0^\\infty e^{-x^2}\\,dx" }],
      answer: { text: "結果", latex: "\\frac{\\sqrt{\\pi}}{2}" },
    },
    null,
    2,
  ),
  validationStatus: "valid",
  typstSourceAttachmentId: "att-typ-1",
  pdfAttachmentId: "att-pdf-1",
  agentRunId: "run-1",
  modelName: "mock-math-pro",
  changeSummary: null,
  createdAt: "2026-11-26T00:00:00Z",
  updatedAt: "2026-11-26T00:00:00Z",
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((ok, fail) => { resolve = ok; reject = fail; });
  return { promise, resolve, reject };
}

function message(conversationId: string, contentMarkdown: string): ChatMessage {
  return {
    id: contentMarkdown, conversationId, contentMarkdown, role: "user", status: "completed",
    agentRunId: null, referencedProblemId: null, attachmentIds: [],
    createdAt: "2026-11-26T00:00:00Z", updatedAt: "2026-11-26T00:00:00Z",
  };
}

function chatResult(runId: string, conversationId: string): SendChatMessageResult {
  return {
    runId, resumedExternalSession: false,
    userMessage: message(conversationId, "sent"),
    assistantMessage: { ...message(conversationId, "reply"), role: "assistant", agentRunId: runId },
  };
}

function emit(runId: string, event: AgentEventPayload) {
  useChatStore.getState().handleAgentEvent({
    eventId: crypto.randomUUID(), runId, sessionId: "session", messageId: null,
    timestamp: "2026-11-26T00:00:00Z", event,
  });
}

describe("Theorem Phase 2 AI UI (Chat, Agent Settings, Solution Editor & Versions)", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    useLibraryStore.setState({ selectedDocumentId: "doc-1" });
    useAgentStore.setState({
      profiles: SAMPLE_PROFILES,
      statusesByProfileId: {},
      loading: false,
      detecting: false,
      error: null,
    });
    useChatStore.setState({
      activeDocumentId: "doc-1",
      conversations: [
        {
          id: "conv-1",
          documentId: "doc-1",
          problemId: "prob-1",
          solutionId: "sol-1",
          title: "チャット 1",
          createdAt: "2026-11-26T00:00:00Z",
          updatedAt: "2026-11-26T00:00:00Z",
        },
      ],
      activeConversationId: "conv-1",
      messages: [],
      selectedProfileId: "prof-codex",
      selectedModel: null,
      includeProblemContext: true,
      includeSolutionContext: true,
      sending: false,
      sendingHere: false,
      activeRunId: null,
      streamingText: "",
      pendingPermissions: [],
      error: null,
    });
    useAiWorkspaceStore.setState({
      activeDocumentId: "doc-1",
      problems: [],
      selectedProblemId: null,
      solutions: [SAMPLE_SOL_V1],
      selectedSolutionId: "sol-1",
      solutionPdfAttachment: null,
      solutionPdfBase64: null,
      solutionSchemaIssues: [],
      solutionLatexErrors: [],
      solutionPdfError: null,
      error: null,
    });

    vi.mocked(tauriClient.listAgentProfiles).mockResolvedValue(SAMPLE_PROFILES);
    vi.mocked(tauriClient.updateSettings).mockImplementation(async (s) => s);
  });

  it("routes interleaved starts, deltas, permissions, failures and cancel to the submitted run", async () => {
    const user = userEvent.setup();
    const turn = deferred<SendChatMessageResult>();
    vi.mocked(tauriClient.sendChatMessage).mockReturnValue(turn.promise);
    vi.mocked(tauriClient.listMessages).mockResolvedValue([]);
    vi.mocked(tauriClient.respondAgentPermission).mockResolvedValue(undefined);
    vi.mocked(tauriClient.cancelAgentRun).mockResolvedValue(undefined);

    render(<ChatSidebar />);
    expect(screen.getByText("モデル一覧: 未取得（既定使用）")).toBeInTheDocument();
    await user.type(screen.getByRole("textbox"), "積分を説明してください");
    await user.click(screen.getByRole("button", { name: "送信" }));
    const input = vi.mocked(tauriClient.sendChatMessage).mock.calls[0]?.[0];
    expect(input).toMatchObject({ documentId: "doc-1", conversationId: "conv-1", runId: expect.any(String) });
    const id = input!.runId;
    const permission = {
      type: "permissionRequested", requestId: "req-42", action: "shell_command", target: null,
      detail: "任意シェル実行要求",
    } as const;
    act(() => {
      emit("recognition", { type: "messageStarted", role: "assistant" });
      emit(id, { type: "textDelta", text: "導出を" });
      emit("generation", { type: "textDelta", text: "別の生成" });
      emit(id, { type: "messageStarted", role: "assistant" });
      emit("recognition", permission);
      emit(id, permission);
      emit("generation", { type: "failed", code: "FAIL", message: "別の失敗", rateLimited: false, authRequired: false });
      emit("generation", { type: "messageStarted", role: "assistant" });
      emit(id, { type: "textDelta", text: "開始します。" });
    });

    expect(screen.getByText("導出を開始します。")).toBeInTheDocument();
    expect(screen.queryByText("別の生成")).not.toBeInTheDocument();
    expect(screen.queryByText("別の失敗")).not.toBeInTheDocument();
    expect(screen.getAllByRole("alertdialog", { name: "権限要求の確認" })).toHaveLength(1);
    await user.click(screen.getByRole("button", { name: "拒否（推奨）" }));
    expect(tauriClient.respondAgentPermission).toHaveBeenCalledWith(id, "req-42", "deny");
    await user.click(screen.getByRole("button", { name: "中断" }));
    expect(tauriClient.cancelAgentRun).toHaveBeenCalledWith(id);
    await act(async () => turn.resolve(chatResult(id, "conv-1")));
    expect(useChatStore.getState().sending).toBe(false);
  });

  it.each(["success", "failure"])("keeps the latest conversation and document when older loads finish with %s", async (outcome) => {
    const chat = useChatStore.getState();
    const first = chat.conversations[0]!;
    const second = { ...first, id: "conv-2" };
    useChatStore.setState({ conversations: [first, second] });
    const a = deferred<ChatMessage[]>();
    const b = deferred<ChatMessage[]>();
    vi.mocked(tauriClient.listMessages).mockReturnValueOnce(a.promise).mockReturnValueOnce(b.promise);
    const selectA = chat.selectConversation(first.id);
    const selectB = chat.selectConversation(second.id);
    b.resolve([message(second.id, "B")]);
    await selectB;
    if (outcome === "success") a.resolve([message(first.id, "A")]);
    else a.reject(new Error("A failed"));
    await selectA;
    expect(useChatStore.getState()).toMatchObject({
      activeConversationId: second.id, messages: [message(second.id, "B")], error: null,
    });

    const docA = deferred<Conversation[]>();
    const docB = deferred<Conversation[]>();
    const firstInB = { ...second, documentId: "doc-2" };
    vi.mocked(tauriClient.listConversations).mockReturnValueOnce(docA.promise).mockReturnValueOnce(docB.promise);
    vi.mocked(tauriClient.listMessages).mockResolvedValue([message(second.id, "Doc B")]);
    const loadA = chat.loadForDocument("doc-1");
    const loadB = chat.loadForDocument("doc-2");
    docB.resolve([firstInB]);
    await loadB;
    if (outcome === "success") docA.resolve([first]);
    else docA.reject(new Error("Doc A failed"));
    await loadA;
    expect(useChatStore.getState()).toMatchObject({
      activeDocumentId: "doc-2", activeConversationId: second.id,
      conversations: [firstInB], messages: [message(second.id, "Doc B")], error: null,
    });
  });

  it.each([
    ["success", "during send"], ["failure", "during send"],
    ["success", "during refresh"], ["failure", "during refresh"],
  ])("keeps B visible when A finishes with %s after switching %s", async (outcome, when) => {
    const chat = useChatStore.getState();
    const a = chat.conversations[0]!;
    const b = { ...a, id: "conv-2" };
    useChatStore.setState({ conversations: [a, b] });
    const turn = deferred<SendChatMessageResult>();
    const aReload = deferred<ChatMessage[]>();
    vi.mocked(tauriClient.sendChatMessage).mockReturnValue(turn.promise);
    vi.mocked(tauriClient.listMessages).mockImplementation((id) =>
      id === a.id ? aReload.promise : Promise.resolve([message(b.id, "B")]));
    const send = chat.sendMessage({ documentId: a.documentId, contentMarkdown: "送信 A" });
    const id = vi.mocked(tauriClient.sendChatMessage).mock.calls[0]![0].runId;
    expect(tauriClient.sendChatMessage).toHaveBeenCalledWith(expect.objectContaining({ conversationId: a.id }));

    const finishTurn = () => {
      if (outcome === "success") turn.resolve(chatResult(id, a.id));
      else turn.reject(new Error("send A failed"));
    };
    if (when === "during refresh") {
      finishTurn();
      await waitFor(() => expect(tauriClient.listMessages).toHaveBeenCalledWith(a.id));
    }
    await chat.selectConversation(b.id);
    emit(id, { type: "textDelta", text: "A のストリーム" });
    if (when === "during send") finishTurn();
    aReload.resolve([message(a.id, "A")]);
    await send;
    expect(useChatStore.getState()).toMatchObject({
      activeConversationId: b.id, messages: [message(b.id, "B")],
      sending: false, sendingHere: false, streamingText: "", pendingPermissions: [], error: null,
    });
  });

  it("shows honest verification/compliance badges in Agent Settings and PDF metadata toggles", async () => {
    const user = userEvent.setup();
    vi.mocked(tauriClient.checkAgentStatus).mockImplementation(async (id) => {
      if (id === "prof-claude") {
        return {
          providerKind: "claude_code",
          installed: false,
          resolvedPath: null,
          version: null,
          authStatus: "terms_restricted",
          authMethod: null,
          verificationStatus: "unverified",
          availableModels: null,
          capabilities: {
            supportsImageInput: false,
            supportsPdfInput: false,
            supportsStreaming: false,
            supportsSessionResume: false,
            supportsModelListing: false,
            supportsAuthStatusCheck: false,
            supportsMcpScopedConfig: false,
            supportsPermissionHooks: false,
            supportsSandboxRestriction: false,
            supportsCancel: false,
          },
          authGuide: "API キー代替設定ガイド",
          complianceNote: "OAuth 流用禁止",
          detailNote: "claude CLI 未導入のため実機確認未実施",
        };
      }
      return {
        providerKind: "codex",
        installed: true,
        resolvedPath: "C:\\codex.exe",
        version: "codex-cli 0.172.0",
        authStatus: "authenticated",
        authMethod: "chatgpt",
        verificationStatus: "verified_live",
        availableModels: [{ id: "gpt-5.4", label: "GPT-5.4" }],
        capabilities: {
          supportsImageInput: true,
          supportsPdfInput: false,
          supportsStreaming: true,
          supportsSessionResume: true,
          supportsModelListing: true,
          supportsAuthStatusCheck: true,
          supportsMcpScopedConfig: true,
          supportsPermissionHooks: true,
          supportsSandboxRestriction: true,
          supportsCancel: true,
        },
        authGuide: "codex login",
        complianceNote: "OPENAI_API_KEY 非継承",
        detailNote: null,
      };
    });

    useLayoutStore.setState({ settingsOpen: true, settingsCategory: "agents" });
    render(<SettingsModal />);

    await user.click(screen.getByRole("button", { name: "CLI を自動検出・状態確認" }));
    await waitFor(() => {
      expect(screen.getByText("実機確認済")).toBeInTheDocument();
      expect(screen.getByText("未検証")).toBeInTheDocument();
      expect(screen.getByText(/規約制限（OAuth無効）/)).toBeInTheDocument();
    });

    // Switch to PDF settings and toggle model name display
    await user.click(screen.getByRole("button", { name: "PDF出力" }));
    expect(screen.getByText("使用した AI モデル名を PDF に表示する")).toBeInTheDocument();
  });

  it("highlights LaTeX conversion errors in Solution Editor and supports version restore", async () => {
    const user = userEvent.setup();
    useAiWorkspaceStore.setState({
      solutionLatexErrors: [
        {
          location: "steps[0].latex",
          stepIndex: 0,
          latex: "\\unknownMacro{x}",
          reason: "未対応マクロ",
        },
      ],
    });

    render(<SolutionEditorTab />);
    expect(screen.getByText(/修正が必要なエラー箇所/)).toBeInTheDocument();
    expect(screen.getByText(/LaTeX 変換エラー: 未対応マクロ/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "エラー箇所を修正して再生成" })).toBeInTheDocument();

    const v2: Solution = { ...SAMPLE_SOL_V1, id: "sol-2", versionNumber: 2, isCurrent: true };
    const v1Old: Solution = { ...SAMPLE_SOL_V1, isCurrent: false };
    act(() => {
      useAiWorkspaceStore.setState({ solutions: [v2, v1Old], selectedSolutionId: "sol-2" });
    });
    vi.mocked(tauriClient.restoreSolutionVersion).mockResolvedValue({
      solution: { ...SAMPLE_SOL_V1, id: "sol-3", versionNumber: 3, isCurrent: true },
      schemaIssues: [],
      latexErrors: [],
      pdfError: null,
      rawOutput: null,
    });
    vi.mocked(tauriClient.listSolutions).mockResolvedValue([v2, v1Old]);

    render(<SolutionVersionsTab />);
    await user.click(screen.getByRole("button", { name: "最新版として復元" }));
    await waitFor(() => {
      expect(tauriClient.restoreSolutionVersion).toHaveBeenCalledWith("sol-1");
    });
  });
});
