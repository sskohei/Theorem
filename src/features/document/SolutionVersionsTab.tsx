import { useState } from "react";
import {
  CloudUpload,
  Eye,
  FileDiff,
  GitCompare,
  History,
  RotateCcw,
  ShieldAlert,
} from "lucide-react";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { cn, formatShortDate } from "@/lib/utils";
import { useAgentStore } from "@/stores/useAgentStore";
import { useAiWorkspaceStore } from "@/stores/useAiWorkspaceStore";
import { useDriveStore } from "@/stores/useDriveStore";
import { useLayoutStore } from "@/stores/useLayoutStore";
import { useStudyStore } from "@/stores/useStudyStore";
import type { SolutionMode } from "@/types/ipc";

export function SolutionVersionsTab() {
  const { setActiveTab } = useLayoutStore();
  const { profiles } = useAgentStore();
  const {
    activeDocumentId,
    selectedProblemId,
    solutions,
    selectedSolutionId,
    savingSolution,
    loadForDocument,
    selectSolution,
    restoreSolutionVersion,
  } = useAiWorkspaceStore();
  const {
    overview: driveOverview,
    uploadingDocId,
    uploadDocument: uploadDocumentToDrive,
  } = useDriveStore();
  const {
    versionDiff,
    comparingVersions,
    agentComparison,
    comparingAgents,
    selectedReferenceIds,
    compareVersions,
    clearVersionDiff,
    compareAgents,
    clearAgentComparison,
  } = useStudyStore();

  const driveConnected = driveOverview?.connection?.status === "connected";

  const [baseCompareId, setBaseCompareId] = useState<string>("");
  const [targetCompareId, setTargetCompareId] = useState<string>("");
  const [selectedAgentIds, setSelectedAgentIds] = useState<string[]>([]);
  const [compareMode, setCompareMode] = useState<SolutionMode>("detailed");

  const effectiveBaseId =
    baseCompareId || solutions[1]?.id || solutions[0]?.id || "";
  const effectiveTargetId = targetCompareId || solutions[0]?.id || "";

  const enabledProfiles = profiles.filter((p) => p.isEnabled);

  const toggleCompareAgent = (id: string) => {
    setSelectedAgentIds((prev) =>
      prev.includes(id) ? prev.filter((x) => x !== id) : prev.length < 4 ? [...prev, id] : prev,
    );
  };

  const handleRunMultiAgentCompare = async () => {
    if (!activeDocumentId || selectedAgentIds.length < 2) return;
    const res = await compareAgents({
      documentId: activeDocumentId,
      problemId: selectedProblemId,
      mode: compareMode,
      profileIds: selectedAgentIds,
      referenceMaterialIds: selectedReferenceIds,
    });
    if (res) {
      await loadForDocument(activeDocumentId);
    }
  };

  return (
    <div className="h-full overflow-y-auto p-4 space-y-4 text-xs">
      {/* Phase 5: Version-to-Version Diff Viewer */}
      {solutions.length >= 2 && (
        <section
          aria-label="バージョン間差分比較"
          className="rounded-xl border border-slate-200 bg-white p-3.5 space-y-2.5 dark:border-slate-800 dark:bg-slate-900"
        >
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="flex items-center gap-1.5 font-semibold text-slate-900 dark:text-slate-100">
              <FileDiff className="h-4 w-4 text-indigo-600 dark:text-indigo-400" />
              <span>バージョン間差分表示</span>
            </div>
            <div className="flex flex-wrap items-center gap-1.5">
              <select
                aria-label="比較元バージョン"
                value={effectiveBaseId}
                onChange={(e) => setBaseCompareId(e.target.value)}
                className="h-7 rounded border border-slate-300 bg-white px-2 text-xs dark:border-slate-700 dark:bg-slate-800"
              >
                {solutions.map((s) => (
                  <option key={s.id} value={s.id}>
                    比較元: v{s.versionNumber} ({s.mode})
                  </option>
                ))}
              </select>
              <span className="text-slate-400">→</span>
              <select
                aria-label="比較先バージョン"
                value={effectiveTargetId}
                onChange={(e) => setTargetCompareId(e.target.value)}
                className="h-7 rounded border border-slate-300 bg-white px-2 text-xs dark:border-slate-700 dark:bg-slate-800"
              >
                {solutions.map((s) => (
                  <option key={s.id} value={s.id}>
                    比較先: v{s.versionNumber} ({s.mode})
                  </option>
                ))}
              </select>
              <Button
                size="xs"
                variant="primary"
                disabled={
                  comparingVersions ||
                  !effectiveBaseId ||
                  !effectiveTargetId ||
                  effectiveBaseId === effectiveTargetId
                }
                onClick={() => void compareVersions(effectiveBaseId, effectiveTargetId)}
              >
                {comparingVersions ? "比較中..." : "差分を表示"}
              </Button>
              {versionDiff && (
                <Button size="xs" variant="ghost" onClick={clearVersionDiff}>
                  閉じる
                </Button>
              )}
            </div>
          </div>

          {versionDiff && (
            <div className="space-y-2 border-t border-slate-200 pt-2.5 dark:border-slate-800">
              <div className="flex flex-wrap items-center gap-2 text-[11px]">
                <span className="font-semibold">
                  v{versionDiff.baseVersionNumber} → v{versionDiff.targetVersionNumber}
                </span>
                <Badge variant="info">追加 {versionDiff.addedCount}</Badge>
                <Badge variant="outline">変更 {versionDiff.modifiedCount}</Badge>
                <Badge variant="warning">削除 {versionDiff.removedCount}</Badge>
              </div>
              {versionDiff.hunks.length === 0 ? (
                <p className="text-slate-500">構造化解答に差分はありません。</p>
              ) : (
                <div className="space-y-1.5">
                  {versionDiff.hunks.map((h) => (
                    <div
                      key={h.hunkId}
                      className="rounded-lg border border-slate-200 bg-slate-50/70 p-2 dark:border-slate-800 dark:bg-slate-950/60"
                    >
                      <div className="flex items-center justify-between gap-2 font-semibold">
                        <span>
                          {h.label} <code className="text-[10px] text-slate-400">{h.fieldPath}</code>
                        </span>
                        <Badge variant={h.changeKind === "removed" ? "warning" : "info"}>
                          {h.changeKind === "added"
                            ? "追加"
                            : h.changeKind === "removed"
                              ? "削除"
                              : "変更"}
                        </Badge>
                      </div>
                      {h.changeKind !== "added" && (
                        <div className="mt-1 rounded bg-red-50 px-2 py-1 text-[11px] text-red-900 dark:bg-red-950/40 dark:text-red-200">
                          - {h.beforeSummary}
                        </div>
                      )}
                      {h.changeKind !== "removed" && (
                        <div className="mt-1 rounded bg-emerald-50 px-2 py-1 text-[11px] text-emerald-900 dark:bg-emerald-950/40 dark:text-emerald-200">
                          + {h.afterSummary}
                        </div>
                      )}
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </section>
      )}

      {/* Phase 5: User-Triggered Multi-Agent Solution Comparison */}
      {activeDocumentId && enabledProfiles.length >= 2 && (
        <section
          aria-label="複数 AI 解答比較"
          className="rounded-xl border border-slate-200 bg-white p-3.5 space-y-2.5 dark:border-slate-800 dark:bg-slate-900"
        >
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="flex items-center gap-1.5 font-semibold text-slate-900 dark:text-slate-100">
              <GitCompare className="h-4 w-4 text-blue-600 dark:text-blue-400" />
              <span>複数 AI エージェントの解答並列比較</span>
            </div>
            <div className="flex items-center gap-1.5">
              <select
                aria-label="複数AI比較モード"
                value={compareMode}
                onChange={(e) => setCompareMode(e.target.value as SolutionMode)}
                className="h-7 rounded border border-slate-300 bg-white px-2 text-xs dark:border-slate-700 dark:bg-slate-800"
              >
                <option value="standard">標準</option>
                <option value="concise">簡潔</option>
                <option value="detailed">詳細解説</option>
                <option value="rigorous">厳密証明</option>
                <option value="beginner">初学者向け</option>
              </select>
              <Button
                size="xs"
                variant="primary"
                disabled={comparingAgents || selectedAgentIds.length < 2}
                onClick={() => void handleRunMultiAgentCompare()}
              >
                {comparingAgents
                  ? "並列比較を実行中..."
                  : `選択した ${selectedAgentIds.length} AI で比較生成`}
              </Button>
              {agentComparison && (
                <Button size="xs" variant="ghost" onClick={clearAgentComparison}>
                  結果を閉じる
                </Button>
              )}
            </div>
          </div>

          <p className="text-[11px] text-slate-500 dark:text-slate-400">
            ユーザーの明示操作時のみ実行します。利用規約上サードパーティ OAuth が制限されているエージェント（例: Claude Code の公式ハーネスモード）は規約保護のため自動ブロックされます。
          </p>

          <div className="flex flex-wrap gap-2">
            {enabledProfiles.map((p) => {
              const blocked =
                p.providerKind === "claude_code" && p.authMode === "official_harness";
              const checked = selectedAgentIds.includes(p.id);
              return (
                <label
                  key={p.id}
                  className={cn(
                    "inline-flex items-center gap-1.5 rounded-md border px-2 py-1 text-[11px]",
                    blocked
                      ? "border-amber-200 bg-amber-50/70 text-amber-800 opacity-75 dark:border-amber-900 dark:bg-amber-950/30 dark:text-amber-200"
                      : checked
                        ? "border-blue-500 bg-blue-50 text-blue-900 dark:bg-blue-950/50 dark:text-blue-100 cursor-pointer"
                        : "border-slate-200 bg-slate-50 text-slate-700 dark:border-slate-800 dark:bg-slate-950 dark:text-slate-300 cursor-pointer",
                  )}
                >
                  <input
                    type="checkbox"
                    disabled={blocked}
                    checked={checked}
                    onChange={() => toggleCompareAgent(p.id)}
                  />
                  <span>{p.name}</span>
                  {blocked && (
                    <span className="inline-flex items-center gap-0.5 text-[10px]">
                      <ShieldAlert className="h-3 w-3" />
                      規約制限
                    </span>
                  )}
                </label>
              );
            })}
          </div>

          {agentComparison && (
            <div className="space-y-2 border-t border-slate-200 pt-2.5 dark:border-slate-800">
              <div className="grid gap-2.5 md:grid-cols-2">
                {agentComparison.entries.map((entry) => (
                  <div
                    key={entry.profileId}
                    className="rounded-lg border border-slate-200 bg-slate-50/60 p-2.5 dark:border-slate-800 dark:bg-slate-950/50"
                  >
                    <div className="flex items-center justify-between gap-2 font-semibold">
                      <span>{entry.profileName}</span>
                      <Badge variant={entry.error ? "warning" : "info"}>
                        {entry.solutionResult
                          ? `v${entry.solutionResult.solution.versionNumber}`
                          : "エラー / 制限"}
                      </Badge>
                    </div>
                    {entry.error ? (
                      <p className="mt-1 text-[11px] text-amber-800 dark:text-amber-200">
                        {entry.error}
                      </p>
                    ) : (
                      entry.solutionResult && (
                        <div className="mt-1.5 space-y-1 text-[11px]">
                          <div className="text-slate-500">
                            モデル: {entry.modelName ?? "未取得"} · 検証状態:{" "}
                            {entry.solutionResult.solution.validationStatus}
                          </div>
                          <Button
                            size="xs"
                            variant="outline"
                            onClick={() => {
                              const sid = entry.solutionResult?.solution.id;
                              if (sid) {
                                void selectSolution(sid).then(() => setActiveTab("solutionPdf"));
                              }
                            }}
                          >
                            <Eye className="h-3 w-3" />
                            <span>この解答 PDF を開く</span>
                          </Button>
                        </div>
                      )
                    )}
                  </div>
                ))}
              </div>
              {agentComparison.diffBetweenFirstTwo && (
                <div className="rounded-md bg-blue-50/70 px-2.5 py-1.5 text-[11px] text-blue-900 dark:bg-blue-950/40 dark:text-blue-200">
                  先頭 2 エージェント間の構造化差分: 追加{" "}
                  {agentComparison.diffBetweenFirstTwo.addedCount} / 変更{" "}
                  {agentComparison.diffBetweenFirstTwo.modifiedCount} / 削除{" "}
                  {agentComparison.diffBetweenFirstTwo.removedCount} 件
                </div>
              )}
            </div>
          )}
        </section>
      )}

      {/* Version History List */}
      {solutions.length === 0 ? (
        <div className="flex flex-col items-center justify-center p-8 text-center text-xs text-slate-500">
          <History className="mb-2 h-6 w-6 text-slate-400" />
          <p>まだ解答バージョン履歴がありません。</p>
        </div>
      ) : (
        <div className="space-y-2.5">
          <div className="text-xs text-slate-500 dark:text-slate-400">
            すべての解答改訂は非破壊で保存されます。過去バージョンの復元は既存履歴を上書きせず、新しいバージョン（vN+1）を作成します。
          </div>

          {solutions.map((sol) => {
            const isSelected = selectedSolutionId === sol.id;
            return (
              <div
                key={sol.id}
                className={cn(
                  "flex flex-wrap items-center justify-between gap-3 rounded-xl border bg-white p-3.5 text-xs dark:bg-slate-900",
                  isSelected
                    ? "border-blue-500 ring-1 ring-blue-500/30"
                    : "border-slate-200 dark:border-slate-800",
                )}
              >
                <div className="space-y-1">
                  <div className="flex flex-wrap items-center gap-1.5">
                    <span className="font-bold text-slate-900 dark:text-slate-100">
                      v{sol.versionNumber}
                    </span>
                    {sol.isCurrent && <Badge variant="info">最新版</Badge>}
                    <Badge variant="outline">{sol.mode}</Badge>
                    <Badge variant={sol.validationStatus === "invalid" ? "warning" : "default"}>
                      {sol.validationStatus === "valid"
                        ? "検証済"
                        : sol.validationStatus === "user_edited"
                          ? "手動編集"
                          : "要修正"}
                    </Badge>
                    {sol.pdfAttachmentId && <Badge variant="default">PDF 生成済</Badge>}
                  </div>
                  <div className="text-[11px] text-slate-500 dark:text-slate-400">
                    作成日時: {formatShortDate(sol.createdAt)} · モデル: {sol.modelName ?? "未取得"}
                  </div>
                  {sol.changeSummary && (
                    <div className="text-[11px] text-slate-700 dark:text-slate-300">
                      変更概要: {sol.changeSummary}
                    </div>
                  )}
                </div>

                <div className="flex items-center gap-1.5">
                  <Button
                    size="xs"
                    variant="outline"
                    onClick={() => {
                      void selectSolution(sol.id).then(() => setActiveTab("solutionPdf"));
                    }}
                  >
                    <Eye className="h-3.5 w-3.5" />
                    <span>PDF 閲覧</span>
                  </Button>
                  {driveConnected && sol.pdfAttachmentId && (
                    <Button
                      size="xs"
                      variant="outline"
                      disabled={uploadingDocId === sol.documentId}
                      onClick={() =>
                        void uploadDocumentToDrive({
                          documentId: sol.documentId,
                          solutionId: sol.id,
                          scope: "default",
                        })
                      }
                    >
                      <CloudUpload className="h-3.5 w-3.5" />
                      <span>v{sol.versionNumber} を Drive へ保存</span>
                    </Button>
                  )}
                  {!sol.isCurrent && (
                    <Button
                      size="xs"
                      variant="primary"
                      disabled={savingSolution}
                      onClick={() => {
                        void restoreSolutionVersion(sol.id).then(() => setActiveTab("solutionPdf"));
                      }}
                    >
                      <RotateCcw className="h-3.5 w-3.5" />
                      <span>最新版として復元</span>
                    </Button>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
