import { create } from "zustand";
import { normalizeApiError, tauriClient } from "@/services/tauriClient";
import type {
  AgentEventEnvelope,
  ApiError,
  ChatImageUpload,
  ChatMessage,
  Conversation,
  PermissionDecision,
} from "@/types/ipc";

export interface PendingPermissionRequest {
  runId: string;
  requestId: string;
  action: string;
  target: string | null;
  detail: string;
}

interface ChatStoreState {
  activeDocumentId: string | null;
  conversations: Conversation[];
  activeConversationId: string | null;
  messages: ChatMessage[];

  selectedProfileId: string | null;
  selectedModel: string | null;
  includeProblemContext: boolean;
  includeSolutionContext: boolean;

  sending: boolean;
  activeRunId: string | null;
  streamingText: string;
  pendingPermissions: PendingPermissionRequest[];
  error: ApiError | null;

  loadForDocument: (documentId: string | null) => Promise<void>;
  selectConversation: (conversationId: string) => Promise<void>;
  createNewConversation: (
    documentId: string,
    title?: string,
    problemId?: string | null,
    solutionId?: string | null,
  ) => Promise<Conversation | null>;
  deleteConversation: (conversationId: string) => Promise<void>;

  setSelectedProfileId: (profileId: string | null) => void;
  setSelectedModel: (model: string | null) => void;
  setIncludeProblemContext: (include: boolean) => void;
  setIncludeSolutionContext: (include: boolean) => void;

  sendMessage: (params: {
    documentId: string;
    contentMarkdown: string;
    referencedProblemId?: string | null;
    attachedImages?: ChatImageUpload[];
  }) => Promise<void>;
  cancelActiveRun: () => Promise<void>;
  respondPermission: (
    runId: string,
    requestId: string,
    decision: PermissionDecision,
  ) => Promise<void>;
  handleAgentEvent: (envelope: AgentEventEnvelope) => void;
  clearError: () => void;
}

export const useChatStore = create<ChatStoreState>((set, get) => ({
  activeDocumentId: null,
  conversations: [],
  activeConversationId: null,
  messages: [],

  selectedProfileId: null,
  selectedModel: null,
  includeProblemContext: true,
  includeSolutionContext: true,

  sending: false,
  activeRunId: null,
  streamingText: "",
  pendingPermissions: [],
  error: null,

  loadForDocument: async (documentId) => {
    if (!documentId) {
      set({
        activeDocumentId: null,
        conversations: [],
        activeConversationId: null,
        messages: [],
        streamingText: "",
        pendingPermissions: [],
        error: null,
      });
      return;
    }

    set({ activeDocumentId: documentId, error: null });
    try {
      const conversations = await tauriClient.listConversations(documentId);
      const firstId = conversations[0]?.id ?? null;
      const messages = firstId ? await tauriClient.listMessages(firstId) : [];
      set({
        conversations,
        activeConversationId: firstId,
        messages,
      });
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  selectConversation: async (conversationId) => {
    set({ activeConversationId: conversationId, error: null });
    try {
      const messages = await tauriClient.listMessages(conversationId);
      set({ messages });
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  createNewConversation: async (documentId, title = "新しいチャット", problemId, solutionId) => {
    try {
      const conv = await tauriClient.createConversation(documentId, title, problemId, solutionId);
      set((s) => ({
        conversations: [conv, ...s.conversations],
        activeConversationId: conv.id,
        messages: [],
        error: null,
      }));
      return conv;
    } catch (err) {
      set({ error: normalizeApiError(err) });
      return null;
    }
  },

  deleteConversation: async (conversationId) => {
    try {
      await tauriClient.deleteConversation(conversationId);
      const remaining = get().conversations.filter((c) => c.id !== conversationId);
      const nextId =
        get().activeConversationId === conversationId
          ? (remaining[0]?.id ?? null)
          : get().activeConversationId;
      const messages = nextId ? await tauriClient.listMessages(nextId) : [];
      set({ conversations: remaining, activeConversationId: nextId, messages });
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  setSelectedProfileId: (profileId) => set({ selectedProfileId: profileId, selectedModel: null }),
  setSelectedModel: (model) => set({ selectedModel: model }),
  setIncludeProblemContext: (include) => set({ includeProblemContext: include }),
  setIncludeSolutionContext: (include) => set({ includeSolutionContext: include }),

  sendMessage: async ({
    documentId,
    contentMarkdown,
    referencedProblemId,
    attachedImages = [],
  }) => {
    let convId = get().activeConversationId;
    if (!convId) {
      const title = contentMarkdown.trim().slice(0, 28) || "新しいチャット";
      const created = await get().createNewConversation(
        documentId,
        title,
        referencedProblemId ?? null,
        null,
      );
      if (!created) return;
      convId = created.id;
    }

    const optimisticUser: ChatMessage = {
      id: `pending-user-${Date.now()}`,
      conversationId: convId,
      role: "user",
      contentMarkdown: contentMarkdown.trim(),
      status: "completed",
      agentRunId: null,
      referencedProblemId: referencedProblemId ?? null,
      attachmentIds: [],
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    };

    set((s) => ({
      sending: true,
      streamingText: "",
      activeRunId: null,
      pendingPermissions: [],
      messages: [...s.messages, optimisticUser],
      error: null,
    }));

    try {
      await tauriClient.sendChatMessage({
        conversationId: convId,
        documentId,
        agentProfileId: get().selectedProfileId,
        modelName: get().selectedModel,
        contentMarkdown,
        referencedProblemId: referencedProblemId ?? null,
        includeProblemContext: get().includeProblemContext,
        includeSolutionContext: get().includeSolutionContext,
        attachedImages,
      });
      const latest = await tauriClient.listMessages(convId);
      set({
        sending: false,
        activeRunId: null,
        streamingText: "",
        pendingPermissions: [],
        messages: latest,
      });
    } catch (err) {
      const latest = await tauriClient.listMessages(convId).catch(() => get().messages);
      set({
        sending: false,
        activeRunId: null,
        streamingText: "",
        pendingPermissions: [],
        messages: latest,
        error: normalizeApiError(err),
      });
    }
  },

  cancelActiveRun: async () => {
    const runId = get().activeRunId;
    if (!runId) return;
    try {
      await tauriClient.cancelAgentRun(runId);
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  respondPermission: async (runId, requestId, decision) => {
    set((s) => ({
      pendingPermissions: s.pendingPermissions.filter((p) => p.requestId !== requestId),
    }));
    try {
      await tauriClient.respondAgentPermission(runId, requestId, decision);
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  handleAgentEvent: (envelope) => {
    const ev = envelope.event;
    if (!get().sending) return;

    if (ev.type === "messageStarted") {
      set({ activeRunId: envelope.runId });
    } else if (ev.type === "textDelta") {
      set((s) => ({
        activeRunId: s.activeRunId ?? envelope.runId,
        streamingText: s.streamingText + ev.text,
      }));
    } else if (ev.type === "permissionRequested") {
      set((s) => ({
        activeRunId: s.activeRunId ?? envelope.runId,
        pendingPermissions: [
          ...s.pendingPermissions,
          {
            runId: envelope.runId,
            requestId: ev.requestId,
            action: ev.action,
            target: ev.target,
            detail: ev.detail,
          },
        ],
      }));
    } else if (ev.type === "failed") {
      set({
        error: {
          code: ev.code,
          message: ev.message,
          detail: ev.rateLimited
            ? "利用上限に達しました。自動リトライは行いません。"
            : ev.authRequired
              ? "設定画面の「AIエージェント」から公式認証手順を確認してください。"
              : null,
          recoverable: true,
        },
      });
    }
  },

  clearError: () => set({ error: null }),
}));
