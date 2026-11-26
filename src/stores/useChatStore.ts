import { create } from "zustand";
import { normalizeApiError, tauriClient } from "@/services/tauriClient";
import type {
  AgentEventEnvelope,
  ApiError,
  ChatImageUpload,
  ChatMessage,
  Conversation,
  PermissionDecision,
  SendChatMessageResult,
} from "@/types/ipc";

export interface PendingPermissionRequest {
  runId: string;
  requestId: string;
  action: string;
  target: string | null;
  detail: string;
}

export interface LiveToolCallItem {
  callId: string;
  server: string | null;
  toolName: string;
  arguments: unknown;
  status: "running" | "succeeded" | "failed";
  progress: string | null;
  output: unknown;
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
  sendingHere: boolean;
  streamingText: string;
  liveToolCalls: LiveToolCallItem[];
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
    referenceMaterialIds?: string[];
  }) => Promise<void>;
  proposeRevision: (params: {
    documentId: string;
    baseSolutionId: string;
    instruction: string;
    referenceMaterialIds?: string[];
  }) => Promise<void>;
  decideRevision: (params: {
    messageId: string;
    decision: "approve" | "reject";
    approvedHunkIds?: string[] | null;
  }) => Promise<boolean>;
  cancelActiveRun: () => Promise<void>;
  respondPermission: (
    runId: string,
    requestId: string,
    decision: PermissionDecision,
  ) => Promise<void>;
  handleAgentEvent: (envelope: AgentEventEnvelope) => void;
  clearError: () => void;
}

interface ChatRun {
  id: string;
  documentId: string;
  conversationId: string | null;
  streamingText: string;
  liveToolCalls: LiveToolCallItem[];
  permissions: PendingPermissionRequest[];
  error: ApiError | null;
}

export const useChatStore = create<ChatStoreState>((set, get) => {
  let documentRequest = 0;
  let selection = 0;
  let messageRequest = 0;
  let run: ChatRun | null = null;

  const isVisible = (target: ChatRun) =>
    get().activeDocumentId === target.documentId
    && get().activeConversationId === target.conversationId;

  const visibleRun = () => {
    const visible = run && isVisible(run) ? run : null;
    return {
      sendingHere: visible !== null,
      streamingText: visible?.streamingText ?? "",
      liveToolCalls: visible?.liveToolCalls ?? [],
      pendingPermissions: visible?.permissions ?? [],
      error: visible?.error ?? null,
    };
  };

  const refreshMessages = async (conversationId: string, currentSelection: number) => {
    const request = ++messageRequest;
    const current = () => currentSelection === selection && request === messageRequest;
    try {
      const messages = await tauriClient.listMessages(conversationId);
      if (current()) set({ messages });
    } catch (err) {
      if (current()) set({ error: normalizeApiError(err) });
    }
  };

  return {
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
    sendingHere: false,
    streamingText: "",    liveToolCalls: [],
    pendingPermissions: [],
    error: null,

    loadForDocument: async (documentId) => {
      const request = ++documentRequest;
      const currentSelection = ++selection;
      set({
        activeDocumentId: documentId,
        conversations: [],
        activeConversationId: null,
        messages: [],
        sendingHere: false,
        streamingText: "",        liveToolCalls: [],
        pendingPermissions: [],
        error: null,
      });
      if (!documentId) return;

      try {
        const conversations = await tauriClient.listConversations(documentId);
        if (request !== documentRequest) return;
        const existing = get().conversations;
        set({ conversations: [...existing, ...conversations.filter(
          (conversation) => !existing.some((item) => item.id === conversation.id),
        )] });
        const first = conversations[0];
        if (first && currentSelection === selection) await get().selectConversation(first.id);
      } catch (err) {
        if (currentSelection === selection) set({ error: normalizeApiError(err) });
      }
    },

    selectConversation: async (conversationId) => {
      if (!get().conversations.some((conv) => conv.id === conversationId)) return;
      const currentSelection = ++selection;
      set({ activeConversationId: conversationId, messages: [] });
      set(visibleRun());
      await refreshMessages(conversationId, currentSelection);
    },

    createNewConversation: async (documentId, title = "新しいチャット", problemId, solutionId) => {
      if (get().activeDocumentId !== documentId) return null;
      const currentSelection = selection;
      try {
        const conv = await tauriClient.createConversation(documentId, title, problemId, solutionId);
        if (get().activeDocumentId === documentId) {
          set((state) => ({
            conversations: [conv, ...state.conversations.filter((item) => item.id !== conv.id)],
          }));
          if (currentSelection === selection) {
            ++selection;
            set({ activeConversationId: conv.id, messages: [] });
            set(visibleRun());
          }
        }
        return conv;
      } catch (err) {
        if (currentSelection === selection) set({ error: normalizeApiError(err) });
        return null;
      }
    },

    deleteConversation: async (conversationId) => {
      const documentId = get().activeDocumentId;
      const currentSelection = selection;
      try {
        await tauriClient.deleteConversation(conversationId);
        if (get().activeDocumentId !== documentId) return;
        const remaining = get().conversations.filter((conv) => conv.id !== conversationId);
        set({ conversations: remaining });
        if (get().activeConversationId !== conversationId) return;
        if (remaining[0]) {
          await get().selectConversation(remaining[0].id);
        } else {
          ++selection;
          set({ activeConversationId: null, messages: [] });
          set(visibleRun());
        }
      } catch (err) {
        if (currentSelection === selection) set({ error: normalizeApiError(err) });
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
      referenceMaterialIds = [],
    }) => {
      const state = get();
      if (run || state.activeDocumentId !== documentId) return;

      const target: ChatRun = {
        id: crypto.randomUUID(),
        documentId,
        conversationId: state.activeConversationId,
        streamingText: "",
        liveToolCalls: [],
        permissions: [],
        error: null,
      };
      run = target;
      set({ sending: true, activeRunId: target.id, ...visibleRun() });

      try {
        let conversationId = target.conversationId;
        if (!conversationId) {
          const title = contentMarkdown.trim().slice(0, 28) || "新しいチャット";
          const created = await get().createNewConversation(documentId, title, referencedProblemId ?? null, null);
          if (!created) return;
          conversationId = created.id;
          target.conversationId = conversationId;
        }

        const optimisticUser: ChatMessage = {
          id: target.id,
          conversationId,
          role: "user",
          contentMarkdown: contentMarkdown.trim(),
          status: "completed",
          agentRunId: null,
          referencedProblemId: referencedProblemId ?? null,
          attachmentIds: [],
          revisionPatch: null,
          revisionStatus: null,
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
        };

        if (isVisible(target)) {
          ++messageRequest;
          set((current) => ({ ...visibleRun(), messages: [...current.messages, optimisticUser] }));
        }

        let result: SendChatMessageResult | undefined;
        try {
          result = await tauriClient.sendChatMessage({
            runId: target.id,
            conversationId,
            documentId,
            agentProfileId: state.selectedProfileId,
            modelName: state.selectedModel,
            contentMarkdown,
            referencedProblemId: referencedProblemId ?? null,
            includeProblemContext: state.includeProblemContext,
            includeSolutionContext: state.includeSolutionContext,
            attachedImages,
            referenceMaterialIds,
          });
        } catch (err) {
          target.error ??= normalizeApiError(err);
        }
        if (isVisible(target)) {
          set((current) => {
            const messages = current.messages.filter((item) =>
              item.id !== optimisticUser.id && item.id !== result?.userMessage.id
              && item.id !== result?.assistantMessage.id);
            if (result) messages.push(result.userMessage, result.assistantMessage);
            return { ...visibleRun(), messages };
          });
          await refreshMessages(conversationId, selection);
        }
      } finally {
        const visible = isVisible(target);
        run = null;
        set({ sending: false, activeRunId: null });
        if (visible) {
          set({ ...visibleRun(), liveToolCalls: target.liveToolCalls, error: target.error ?? get().error });
        }
      }
    },

    proposeRevision: async ({
      documentId,
      baseSolutionId,
      instruction,
      referenceMaterialIds = [],
    }) => {
      const state = get();
      if (run || state.activeDocumentId !== documentId) return;

      const target: ChatRun = {
        id: crypto.randomUUID(),
        documentId,
        conversationId: state.activeConversationId,
        streamingText: "",
        liveToolCalls: [],
        permissions: [],
        error: null,
      };
      run = target;
      set({ sending: true, activeRunId: target.id, ...visibleRun() });

      try {
        let conversationId = target.conversationId;
        if (!conversationId) {
          const created = await get().createNewConversation(
            documentId,
            `修正提案: ${instruction.trim().slice(0, 20)}`,
            null,
            baseSolutionId,
          );
          if (!created) return;
          conversationId = created.id;
          target.conversationId = conversationId;
        }

        try {
          await tauriClient.proposeSolutionRevision({
            conversationId,
            documentId,
            baseSolutionId,
            instruction,
            agentProfileId: state.selectedProfileId,
            modelName: state.selectedModel,
            referenceMaterialIds,
          });
        } catch (err) {
          target.error ??= normalizeApiError(err);
        }

        if (isVisible(target)) {
          await refreshMessages(conversationId, selection);
        }
      } finally {
        const visible = isVisible(target);
        run = null;
        set({ sending: false, activeRunId: null });
        if (visible) {
          set({ ...visibleRun(), liveToolCalls: target.liveToolCalls, error: target.error ?? get().error });
        }
      }
    },

    decideRevision: async ({ messageId, decision, approvedHunkIds }) => {
      if (run) return false;
      const currentSelection = selection;
      set({ sending: true, error: null });
      try {
        const res = await tauriClient.decideSolutionRevision({
          messageId,
          decision,
          approvedHunkIds: approvedHunkIds ?? null,
        });
        if (currentSelection === selection) {
          set((s) => ({
            sending: false,
            messages: s.messages.map((m) => (m.id === messageId ? res.message : m)),
          }));
        } else {
          set({ sending: false });
        }
        return res.newSolution !== null;
      } catch (err) {
        if (currentSelection === selection) {
          set({ sending: false, error: normalizeApiError(err) });
        } else {
          set({ sending: false });
        }
        return false;
      }
    },

    cancelActiveRun: async () => {
      const target = run;
      if (!target || !isVisible(target)) return;
      try {
        await tauriClient.cancelAgentRun(target.id);
      } catch (err) {
        if (run === target && isVisible(target)) set({ error: normalizeApiError(err) });
      }
    },

    respondPermission: async (runId, requestId, decision) => {
      const target = run;
      if (!target || target.id !== runId || !isVisible(target)) return;
      target.permissions = target.permissions.filter((p) => p.requestId !== requestId);
      set({ pendingPermissions: target.permissions });
      try {
        await tauriClient.respondAgentPermission(runId, requestId, decision);
      } catch (err) {
        if (run === target && isVisible(target)) set({ error: normalizeApiError(err) });
      }
    },

    handleAgentEvent: ({ runId, event }) => {
      const target = run;
      if (!target || target.id !== runId) return;
      if (event.type === "textDelta") {
        target.streamingText += event.text;
      } else if (event.type === "toolCallStarted") {
        target.liveToolCalls = [
          ...target.liveToolCalls.filter((call) => call.callId !== event.callId),
          {
            callId: event.callId,
            server: event.server,
            toolName: event.toolName,
            arguments: event.arguments,
            status: "running",
            progress: null,
            output: null,
          },
        ];
      } else if (event.type === "toolCallUpdated") {
        target.liveToolCalls = target.liveToolCalls.map((call) =>
          call.callId === event.callId ? { ...call, progress: event.progress } : call);
      } else if (event.type === "toolCallCompleted") {
        target.liveToolCalls = target.liveToolCalls.map((call) =>
          call.callId === event.callId
            ? { ...call, status: event.succeeded ? "succeeded" : "failed", output: event.output }
            : call);
      } else if (event.type === "permissionRequested") {
        target.permissions = [...target.permissions, {
          runId,
          requestId: event.requestId,
          action: event.action,
          target: event.target,
          detail: event.detail,
        }];
      } else if (event.type === "failed") {
        target.error = {
          code: event.code,
          message: event.message,
          detail: event.rateLimited
            ? "利用上限に達しました。自動リトライは行いません。"
            : event.authRequired
              ? "設定画面の「AIエージェント」から公式認証手順を確認してください。"
              : null,
          recoverable: true,
        };
      }
      if (isVisible(target)) set(visibleRun());
    },

    clearError: () => {
      if (run && isVisible(run)) run.error = null;
      set({ error: null });
    },
  };
});
