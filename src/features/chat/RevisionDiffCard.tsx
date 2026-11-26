import { useState } from "react";
import { Check, CheckCircle2, FileDiff, XCircle } from "lucide-react";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { useAiWorkspaceStore } from "@/stores/useAiWorkspaceStore";
import { useChatStore } from "@/stores/useChatStore";
import { useLayoutStore } from "@/stores/useLayoutStore";
import type { ChatMessage } from "@/types/ipc";

interface RevisionDiffCardProps {
  message: ChatMessage;
}

export function RevisionDiffCard({ message }: RevisionDiffCardProps) {
  const proposal = message.revisionPatch;
  const status = message.revisionStatus;
  const { sending, decideRevision } = useChatStore();
  const { activeDocumentId, loadForDocument } = useAiWorkspaceStore();
  const { setActiveTab } = useLayoutStore();

  const [selectedIds, setSelectedIds] = useState<string[]>(() =>
    proposal ? proposal.hunks.map((h) => h.hunkId) : [],
  );

  if (!proposal) return null;

  const isPending = status === "pending";

  const toggleHunk = (hunkId: string) => {
    setSelectedIds((prev) =>
      prev.includes(hunkId) ? prev.filter((id) => id !== hunkId) : [...prev, hunkId],
    );
  };

  const handleApprove = async () => {
    const created = await decideRevision({
      messageId: message.id,
      decision: "approve",
      approvedHunkIds: selectedIds.length === proposal.hunks.length ? null : selectedIds,
    });
    if (created && activeDocumentId) {
      await loadForDocument(activeDocumentId);
      setActiveTab("solutionPdf");
    }
  };

  const handleReject = async () => {
    await decideRevision({
      messageId: message.id,
      decision: "reject",
    });
  };

  return (
    <div
      aria-label="解答修正差分カード"
      className="mt-2 rounded-lg border border-indigo-200 bg-indigo-50/50 p-2.5 text-[11px] dark:border-indigo-900 dark:bg-indigo-950/35"
    >
      <div className="flex flex-wrap items-center justify-between gap-1.5">
        <div className="flex items-center gap-1.5 font-semibold text-indigo-950 dark:text-indigo-100">
          <FileDiff className="h-3.5 w-3.5 text-indigo-600 dark:text-indigo-400" />
          <span>解答部分修正の差分提案 (v{proposal.baseVersionNumber})</span>
        </div>
        {status === "pending" && <Badge variant="warning">承認待ち</Badge>}
        {status === "approved" && (
          <Badge variant="info">
            承認済{proposal.appliedVersionNumber ? ` (→ v${proposal.appliedVersionNumber})` : ""}
          </Badge>
        )}
        {status === "rejected" && <Badge variant="default">拒否済（変更なし）</Badge>}
      </div>

      <p className="mt-1 text-[10px] text-slate-600 dark:text-slate-300">{proposal.summary}</p>

      {proposal.hunks.length === 0 ? (
        <div className="mt-1.5 rounded bg-white/80 p-2 text-[10px] text-slate-500 dark:bg-slate-900">
          現行バージョンとの差分はありませんでした。
        </div>
      ) : (
        <div className="mt-2 space-y-1.5 max-h-64 overflow-y-auto pr-0.5">
          {proposal.hunks.map((hunk) => {
            const checked = selectedIds.includes(hunk.hunkId);
            const kindLabel =
              hunk.changeKind === "added"
                ? "追加"
                : hunk.changeKind === "removed"
                  ? "削除"
                  : "変更";
            return (
              <div
                key={hunk.hunkId}
                className="rounded border border-slate-200 bg-white p-2 dark:border-slate-800 dark:bg-slate-900"
              >
                <label className="flex items-center justify-between gap-2 font-medium text-slate-800 dark:text-slate-100">
                  <span className="inline-flex items-center gap-1.5">
                    {isPending && (
                      <input
                        type="checkbox"
                        aria-label={`差分を選択: ${hunk.label}`}
                        checked={checked}
                        onChange={() => toggleHunk(hunk.hunkId)}
                      />
                    )}
                    <span>{hunk.label}</span>
                    <code className="text-[9px] text-slate-400">{hunk.fieldPath}</code>
                  </span>
                  <Badge
                    variant={hunk.changeKind === "removed" ? "warning" : "info"}
                    className="text-[9px]"
                  >
                    {kindLabel}
                  </Badge>
                </label>

                {hunk.changeKind !== "added" && (
                  <div className="mt-1 rounded bg-red-50 px-2 py-1 text-[10px] text-red-900 dark:bg-red-950/40 dark:text-red-200">
                    <span className="font-semibold">変更前: </span>
                    {hunk.beforeSummary}
                  </div>
                )}
                {hunk.changeKind !== "removed" && (
                  <div className="mt-1 rounded bg-emerald-50 px-2 py-1 text-[10px] text-emerald-900 dark:bg-emerald-950/40 dark:text-emerald-200">
                    <span className="font-semibold">変更後: </span>
                    {hunk.afterSummary}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {isPending && (
        <div className="mt-2.5 flex flex-wrap items-center justify-end gap-1.5">
          <Button
            size="xs"
            variant="ghost"
            disabled={sending}
            onClick={() => void handleReject()}
          >
            <XCircle className="h-3.5 w-3.5" />
            <span>拒否</span>
          </Button>
          <Button
            size="xs"
            variant="primary"
            disabled={sending || selectedIds.length === 0}
            onClick={() => void handleApprove()}
          >
            <Check className="h-3.5 w-3.5" />
            <span>
              承認して新バージョン作成 ({selectedIds.length}/{proposal.hunks.length})
            </span>
          </Button>
        </div>
      )}

      {status === "approved" && (
        <div className="mt-1.5 flex items-center gap-1 text-[10px] text-emerald-700 dark:text-emerald-300">
          <CheckCircle2 className="h-3 w-3" />
          <span>過去バージョンを保持したまま新バージョンを作成し、PDF を再生成しました。</span>
        </div>
      )}
    </div>
  );
}
