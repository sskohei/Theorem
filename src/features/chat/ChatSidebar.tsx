import { useMemo, useRef, useState } from "react";
import {
  Calculator,
  Check,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Copy,
  FileDiff,
  ImagePlus,
  Loader2,
  MessageSquare,
  Plus,
  RefreshCw,
  Send,
  ShieldAlert,
  Square,
  Trash2,
  X,
  XCircle,
} from "lucide-react";
import { MarkdownMath } from "@/components/MarkdownMath";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { cn, uint8ArrayToBase64 } from "@/lib/utils";
import { useAgentStore } from "@/stores/useAgentStore";
import { useAiWorkspaceStore } from "@/stores/useAiWorkspaceStore";
import { useChatStore } from "@/stores/useChatStore";
import { useLibraryStore } from "@/stores/useLibraryStore";
import { useStudyStore } from "@/stores/useStudyStore";
import type { ChatImageUpload } from "@/types/ipc";
import { RevisionDiffCard } from "./RevisionDiffCard";

export function ChatSidebar() {
  const { selectedDocumentId, settings } = useLibraryStore();
  const { profiles, statusesByProfileId } = useAgentStore();
  const { problems, selectedProblemId, solutions } = useAiWorkspaceStore();
  const {
    conversations,
    activeConversationId,
    messages,
    selectedProfileId,
    selectedModel,
    includeProblemContext,
    includeSolutionContext,
    sending,
    sendingHere,
    streamingText,
    liveToolCalls,
    pendingPermissions,
    error,
    selectConversation,
    createNewConversation,
    deleteConversation,
    setSelectedProfileId,
    setSelectedModel,
    setIncludeProblemContext,
    setIncludeSolutionContext,
    sendMessage,
    proposeRevision,
    cancelActiveRun,
    respondPermission,
    clearError,
  } = useChatStore();
  const { references, selectedReferenceIds, toggleReferenceSelection } = useStudyStore();

  const [inputText, setInputText] = useState("");
  const [revisionMode, setRevisionMode] = useState(false);
  const [attachedImages, setAttachedImages] = useState<ChatImageUpload[]>([]);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [expandedToolIds, setExpandedToolIds] = useState<Record<string, boolean>>({});
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const activeProfile = useMemo(() => {
    const id =
      selectedProfileId ??
      settings.agentAssignments.chatProfileId ??
      profiles.find((p) => p.isDefault && p.isEnabled)?.id;
    return profiles.find((p) => p.id === id) ?? profiles[0] ?? null;
  }, [profiles, selectedProfileId, settings.agentAssignments.chatProfileId]);

  const activeStatus = activeProfile ? statusesByProfileId[activeProfile.id] : undefined;
  const availableModels = activeStatus?.availableModels ?? null;

  const activeProblem = problems.find((p) => p.id === selectedProblemId) ?? null;
  const currentSolution = solutions.find((s) => s.isCurrent) ?? null;

  const handleSend = async () => {
    if (!selectedDocumentId || sending) return;
    if (!inputText.trim() && attachedImages.length === 0) return;

    const textToSend = inputText;
    const imgsToSend = attachedImages;
    setInputText("");
    setAttachedImages([]);

    if (revisionMode && currentSolution) {
      await proposeRevision({
        documentId: selectedDocumentId,
        baseSolutionId: currentSolution.id,
        instruction: textToSend,
        referenceMaterialIds: selectedReferenceIds,
      });
      setRevisionMode(false);
      return;
    }

    await sendMessage({
      documentId: selectedDocumentId,
      contentMarkdown: textToSend,
      referencedProblemId: activeProblem?.id ?? null,
      attachedImages: imgsToSend,
      referenceMaterialIds: selectedReferenceIds,
    });
  };

  const handleRegenerate = async () => {
    if (!selectedDocumentId || sending) return;
    const lastUser = [...messages].reverse().find((m) => m.role === "user");
    if (!lastUser) return;
    await sendMessage({
      documentId: selectedDocumentId,
      contentMarkdown: lastUser.contentMarkdown,
      referencedProblemId: lastUser.referencedProblemId,
    });
  };

  const handleAttachFiles = async (files: FileList | null) => {
    if (!files) return;
    const next: ChatImageUpload[] = [];
    for (const f of Array.from(files)) {
      if (!f.type.startsWith("image/")) continue;
      const bytes = new Uint8Array(await f.arrayBuffer());
      next.push({
        filename: f.name,
        mimeType: f.type,
        base64Data: uint8ArrayToBase64(bytes),
      });
    }
    if (next.length > 0) {
      setAttachedImages((prev) => [...prev, ...next]);
    }
  };

  const handlePasteInChat = (e: React.ClipboardEvent<HTMLTextAreaElement>) => {
    const items = e.clipboardData?.items;
    if (!items) return;
    for (const item of Array.from(items)) {
      if (item.type.startsWith("image/")) {
        const blob = item.getAsFile();
        if (!blob) continue;
        e.preventDefault();
        e.stopPropagation();
        const ext = item.type.split("/")[1] || "png";
        void blob.arrayBuffer().then((buf) => {
          setAttachedImages((prev) => [
            ...prev,
            {
              filename: `chat-paste-${Date.now()}.${ext}`,
              mimeType: item.type,
              base64Data: uint8ArrayToBase64(new Uint8Array(buf)),
            },
          ]);
        });
        break;
      }
    }
  };

  const handleCopy = (id: string, text: string) => {
    if (navigator.clipboard) {
      void navigator.clipboard.writeText(text).then(() => {
        setCopiedId(id);
        setTimeout(() => setCopiedId(null), 1500);
      });
    }
  };

  return (
    <aside
      aria-label="AI チャット"
      className="flex h-full flex-col border-l border-slate-200 bg-slate-100/80 dark:border-slate-800 dark:bg-slate-900/70"
    >
      {/* Header: Agent + Model + New Chat */}
      <div className="space-y-2 border-b border-slate-200 bg-white/90 px-3 py-2 dark:border-slate-800 dark:bg-slate-900">
        <div className="flex items-center justify-between gap-1">
          <div className="flex items-center gap-1.5 text-xs font-semibold text-slate-800 dark:text-slate-100">
            <MessageSquare className="h-4 w-4 text-blue-600 dark:text-blue-400" />
            <span>AI チャット</span>
          </div>
          <div className="flex items-center gap-1">
            {activeProfile?.providerKind === "mock" && (
              <Badge variant="warning" className="text-[10px]">
                Mock
              </Badge>
            )}
            <Button
              size="xs"
              variant="outline"
              disabled={!selectedDocumentId || sending}
              onClick={() => {
                if (selectedDocumentId) {
                  void createNewConversation(
                    selectedDocumentId,
                    `チャット ${conversations.length + 1}`,
                    activeProblem?.id ?? null,
                    currentSolution?.id ?? null,
                  );
                }
              }}
              title="新規チャット"
            >
              <Plus className="h-3.5 w-3.5" />
              <span>新規</span>
            </Button>
          </div>
        </div>

        {/* Agent & Model Selectors */}
        <div className="grid grid-cols-2 gap-1.5">
          <select
            aria-label="チャット用AIエージェント"
            value={activeProfile?.id ?? ""}
            onChange={(e) => setSelectedProfileId(e.target.value || null)}
            className="h-7 rounded border border-slate-300 bg-white px-1.5 text-[11px] dark:border-slate-700 dark:bg-slate-800"
          >
            {profiles
              .filter((p) => p.isEnabled)
              .map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
          </select>

          <select
            aria-label="チャット用モデル"
            value={selectedModel ?? activeProfile?.defaultModel ?? ""}
            disabled={!availableModels || availableModels.length === 0}
            onChange={(e) => setSelectedModel(e.target.value || null)}
            className="h-7 rounded border border-slate-300 bg-white px-1.5 text-[11px] disabled:opacity-60 dark:border-slate-700 dark:bg-slate-800"
          >
            {availableModels && availableModels.length > 0 ? (
              <>
                <option value="">既定モデル</option>
                {availableModels.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.label}
                  </option>
                ))}
              </>
            ) : (
              <option value="">モデル一覧: 未取得（既定使用）</option>
            )}
          </select>
        </div>

        {/* Thread Switcher */}
        {conversations.length > 0 && (
          <div className="flex items-center gap-1">
            <select
              aria-label="会話スレッド切替"
              value={activeConversationId ?? ""}
              onChange={(e) => void selectConversation(e.target.value)}
              className="h-6 flex-1 rounded border border-slate-200 bg-slate-50 px-1.5 text-[11px] dark:border-slate-700 dark:bg-slate-800"
            >
              {conversations.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.title}
                </option>
              ))}
            </select>
            {activeConversationId && (
              <button
                type="button"
                aria-label="会話を削除"
                onClick={() => {
                  if (window.confirm("この会話履歴を削除しますか？")) {
                    void deleteConversation(activeConversationId);
                  }
                }}
                className="rounded p-1 text-slate-400 hover:bg-red-100 hover:text-red-600 dark:hover:bg-red-950"
              >
                <Trash2 className="h-3 w-3" />
              </button>
            )}
          </div>
        )}
      </div>

      {/* Selective Context Bar */}
      <div className="flex flex-wrap items-center gap-2 border-b border-slate-200 bg-slate-50 px-3 py-1.5 text-[11px] dark:border-slate-800 dark:bg-slate-900/50">
        <span className="text-slate-500">送信コンテキスト:</span>
        <label className="inline-flex items-center gap-1 text-slate-700 dark:text-slate-300">
          <input
            type="checkbox"
            checked={includeProblemContext}
            onChange={(e) => setIncludeProblemContext(e.target.checked)}
          />
          <span>
            問題{activeProblem ? `(${activeProblem.problemNumber})` : ""}
          </span>
        </label>
        <label className="inline-flex items-center gap-1 text-slate-700 dark:text-slate-300">
          <input
            type="checkbox"
            checked={includeSolutionContext}
            onChange={(e) => setIncludeSolutionContext(e.target.checked)}
          />
          <span>解答{currentSolution ? `(v${currentSolution.versionNumber})` : ""}</span>
        </label>
        {references.length > 0 && (
          <div className="flex flex-wrap items-center gap-1 pt-0.5">
            {references.map((r) => {
              const active = selectedReferenceIds.includes(r.id);
              return (
                <button
                  key={r.id}
                  type="button"
                  onClick={() => toggleReferenceSelection(r.id)}
                  className={cn(
                    "rounded px-1.5 py-0.5 text-[10px] border transition-colors",
                    active
                      ? "border-blue-500 bg-blue-100 text-blue-900 dark:bg-blue-900/60 dark:text-blue-100"
                      : "border-slate-300 bg-white text-slate-600 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-300",
                  )}
                >
                  資料: {r.title}
                </button>
              );
            })}
          </div>
        )}
      </div>

      {/* Chat Error Banner */}
      {error && (
        <div
          role="alert"
          className="flex items-start gap-2 border-b border-red-200 bg-red-50 px-3 py-2 text-[11px] text-red-800 dark:border-red-900 dark:bg-red-950/60 dark:text-red-200"
        >
          <div className="min-w-0 flex-1">
            <div className="font-semibold">{error.message}</div>
            {error.detail && <div className="mt-0.5 opacity-85">{error.detail}</div>}
          </div>
          <button type="button" onClick={clearError} aria-label="エラーを閉じる">
            <X className="h-3.5 w-3.5" />
          </button>
        </div>
      )}

      {/* Messages List */}
      <div className="flex-1 overflow-y-auto p-3 space-y-3">
        {!selectedDocumentId ? (
          <div className="py-10 text-center text-xs text-slate-500">
            左サイドバーから問題を選択すると、その問題に紐づく AI チャットを開始できます。
          </div>
        ) : messages.length === 0 && !sendingHere ? (
          <div className="py-10 text-center text-xs text-slate-500 dark:text-slate-400">
            <p className="font-medium text-slate-700 dark:text-slate-300">
              問題や解答について質問できます
            </p>
            <p className="mt-1 text-[11px]">
              会話履歴はローカル SQLite に保存され、再起動後も引き継がれます。
            </p>
          </div>
        ) : (
          messages.map((msg) => {
            const isUser = msg.role === "user";
            return (
              <div
                key={msg.id}
                className={cn(
                  "rounded-lg border p-2.5 text-xs",
                  isUser
                    ? "ml-4 border-blue-200 bg-blue-50/80 dark:border-blue-900 dark:bg-blue-950/50"
                    : "mr-2 border-slate-200 bg-white dark:border-slate-800 dark:bg-slate-900",
                )}
              >
                <div className="mb-1 flex items-center justify-between text-[10px] text-slate-500">
                  <span className="font-semibold">{isUser ? "あなた" : "AI"}</span>
                  <div className="flex items-center gap-1">
                    {msg.status === "cancelled" && <Badge variant="warning">中断済み</Badge>}
                    {msg.status === "failed" && <Badge variant="warning">エラー</Badge>}
                    <button
                      type="button"
                      title="コピー"
                      onClick={() => handleCopy(msg.id, msg.contentMarkdown)}
                      className="rounded p-0.5 hover:bg-slate-200 dark:hover:bg-slate-800"
                    >
                      {copiedId === msg.id ? (
                        <Check className="h-3 w-3 text-emerald-600" />
                      ) : (
                        <Copy className="h-3 w-3" />
                      )}
                    </button>
                    {!isUser && (
                      <button
                        type="button"
                        title="最後の質問を再生成"
                        disabled={sending}
                        onClick={() => void handleRegenerate()}
                        className="rounded p-0.5 hover:bg-slate-200 disabled:opacity-50 dark:hover:bg-slate-800"
                      >
                        <RefreshCw className="h-3 w-3" />
                      </button>
                    )}
                  </div>
                </div>
                <MarkdownMath content={msg.contentMarkdown} />
                {msg.revisionPatch && <RevisionDiffCard message={msg} />}
              </div>
            );
          })
        )}

        {/* Live / Recent Tool Execution Status Cards (Science MCP) */}
        {liveToolCalls.length > 0 && (
          <div
            aria-label="ツール実行状況"
            className="space-y-1.5 rounded-lg border border-blue-200 bg-blue-50/50 p-2 text-[11px] dark:border-blue-900/70 dark:bg-blue-950/30"
          >
            <div className="flex items-center gap-1 font-semibold text-blue-900 dark:text-blue-200">
              <Calculator className="h-3.5 w-3.5" />
              <span>計算ツール実行状況 ({liveToolCalls.length})</span>
            </div>
            {liveToolCalls.map((tc) => {
              const open = Boolean(expandedToolIds[tc.callId]);
              return (
                <div
                  key={tc.callId}
                  className="rounded border border-slate-200 bg-white dark:border-slate-800 dark:bg-slate-900"
                >
                  <button
                    type="button"
                    onClick={() =>
                      setExpandedToolIds((prev) => ({ ...prev, [tc.callId]: !prev[tc.callId] }))
                    }
                    className="flex w-full items-center justify-between gap-1.5 px-2 py-1.5 text-left"
                  >
                    <div className="flex items-center gap-1.5 min-w-0">
                      {open ? (
                        <ChevronDown className="h-3 w-3 shrink-0 text-slate-400" />
                      ) : (
                        <ChevronRight className="h-3 w-3 shrink-0 text-slate-400" />
                      )}
                      {tc.status === "running" ? (
                        <Loader2 className="h-3 w-3 shrink-0 animate-spin text-blue-600" />
                      ) : tc.status === "succeeded" ? (
                        <CheckCircle2 className="h-3 w-3 shrink-0 text-emerald-600" />
                      ) : (
                        <XCircle className="h-3 w-3 shrink-0 text-red-600" />
                      )}
                      <code className="truncate font-mono font-semibold text-slate-800 dark:text-slate-100">
                        {tc.toolName}
                      </code>
                    </div>
                    <Badge variant={tc.status === "failed" ? "warning" : "info"} className="text-[9px]">
                      {tc.status === "running"
                        ? "実行中"
                        : tc.status === "succeeded"
                          ? "完了"
                          : "失敗"}
                    </Badge>
                  </button>
                  {tc.progress && tc.status === "running" && (
                    <div className="px-2 pb-1 text-[10px] text-slate-500">{tc.progress}</div>
                  )}
                  {open && (
                    <div className="space-y-1 border-t border-slate-100 p-2 text-[10px] dark:border-slate-800">
                      <div className="font-semibold text-slate-500">引数</div>
                      <pre className="max-h-24 overflow-auto rounded bg-slate-100 p-1 font-mono dark:bg-slate-950">
                        {JSON.stringify(tc.arguments, null, 2)}
                      </pre>
                      {tc.output !== null && (
                        <>
                          <div className="font-semibold text-slate-500">出力</div>
                          <pre className="max-h-32 overflow-auto rounded bg-slate-100 p-1 font-mono dark:bg-slate-950">
                            {JSON.stringify(tc.output, null, 2)}
                          </pre>
                        </>
                      )}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}

        {/* Active Streaming Message Bubble */}
        {sendingHere && (
          <div className="mr-2 rounded-lg border border-blue-300 bg-white p-2.5 text-xs dark:border-blue-800 dark:bg-slate-900">
            <div className="mb-1 flex items-center justify-between text-[10px] text-blue-600 dark:text-blue-400">
              <span className="font-semibold">AI（ストリーミング応答中...）</span>
            </div>
            <MarkdownMath content={streamingText || "思考・応答を生成しています..."} />
          </div>
        )}

        {/* Interactive Permission Request Cards */}
        {pendingPermissions.map((perm) => (
          <div
            key={perm.requestId}
            role="alertdialog"
            aria-label="権限要求の確認"
            className="rounded-lg border-2 border-amber-400 bg-amber-50 p-3 text-xs text-amber-950 dark:border-amber-700 dark:bg-amber-950/70 dark:text-amber-100"
          >
            <div className="flex items-center gap-1.5 font-semibold">
              <ShieldAlert className="h-4 w-4 text-amber-600" />
              <span>AI エージェントからの権限要求</span>
            </div>
            <div className="mt-1 text-[11px]">
              <div>操作: {perm.action}</div>
              {perm.target && <div>対象: {perm.target}</div>}
              <div className="mt-0.5 font-mono text-[10px] opacity-90">{perm.detail}</div>
            </div>
            <div className="mt-2 flex justify-end gap-1.5">
              <Button
                size="xs"
                variant="danger"
                onClick={() => void respondPermission(perm.runId, perm.requestId, "deny")}
              >
                拒否（推奨）
              </Button>
              <Button
                size="xs"
                variant="outline"
                onClick={() => void respondPermission(perm.runId, perm.requestId, "allow_once")}
              >
                この1回のみ許可
              </Button>
            </div>
          </div>
        ))}
      </div>

      {/* Input Area */}
      <div className="border-t border-slate-200 bg-white p-2.5 dark:border-slate-800 dark:bg-slate-900">
        {attachedImages.length > 0 && (
          <div className="mb-2 flex flex-wrap gap-1.5">
            {attachedImages.map((img, idx) => (
              <span
                key={idx}
                className="inline-flex items-center gap-1 rounded bg-slate-100 px-2 py-0.5 text-[10px] dark:bg-slate-800"
              >
                <span>{img.filename}</span>
                <button
                  type="button"
                  onClick={() =>
                    setAttachedImages((prev) => prev.filter((_, i) => i !== idx))
                  }
                >
                  ×
                </button>
              </span>
            ))}
          </div>
        )}

        <textarea
          aria-label="AI チャット質問入力"
          rows={3}
          disabled={!selectedDocumentId || sending}
          value={inputText}
          onChange={(e) => setInputText(e.target.value)}
          onPaste={handlePasteInChat}
          onKeyDown={(e) => {
            if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
              e.preventDefault();
              void handleSend();
            }
          }}
          placeholder={
            selectedDocumentId
              ? "質問を入力（Ctrl+Enter で送信 / 画像の Ctrl+V 貼付可）"
              : "問題を選択するとチャットできます"
          }
          className="w-full resize-none rounded-md border border-slate-300 bg-white p-2 text-xs text-slate-900 focus:border-blue-500 focus:outline-none disabled:opacity-60 dark:border-slate-700 dark:bg-slate-950 dark:text-slate-100"
        />

        <div className="mt-1.5 flex items-center justify-between">
          <div className="flex items-center gap-1">
            <input
              ref={fileInputRef}
              type="file"
              accept="image/png,image/jpeg,image/webp"
              multiple
              className="hidden"
              onChange={(e) => void handleAttachFiles(e.target.files)}
            />
            <Button
              size="xs"
              variant="ghost"
              disabled={!selectedDocumentId || sending}
              onClick={() => fileInputRef.current?.click()}
              title="画像を添付"
            >
              <ImagePlus className="h-3.5 w-3.5" />
              <span>画像添付</span>
            </Button>
            {currentSolution && (
              <Button
                size="xs"
                variant={revisionMode ? "primary" : "outline"}
                disabled={!selectedDocumentId || sending}
                onClick={() => setRevisionMode((v) => !v)}
                title="現行解答に対する部分修正の差分提案を作成（承認するまで適用されません）"
              >
                <FileDiff className="h-3.5 w-3.5" />
                <span>{revisionMode ? "部分修正モード ON" : "解答の部分修正"}</span>
              </Button>
            )}
          </div>

          {sendingHere ? (
            <Button size="xs" variant="danger" onClick={() => void cancelActiveRun()}>
              <Square className="h-3 w-3" />
              <span>中断</span>
            </Button>
          ) : (
            <Button
              size="xs"
              variant="primary"
              disabled={
                !selectedDocumentId ||
                sending ||
                (!inputText.trim() && (revisionMode || attachedImages.length === 0))
              }
              onClick={() => void handleSend()}
            >
              <Send className="h-3.5 w-3.5" />
              <span>{revisionMode ? "修正案を作成" : "送信"}</span>
            </Button>
          )}
        </div>
      </div>
    </aside>
  );
}
