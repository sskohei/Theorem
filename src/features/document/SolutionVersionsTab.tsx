import { Eye, History, RotateCcw } from "lucide-react";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { cn, formatShortDate } from "@/lib/utils";
import { useAiWorkspaceStore } from "@/stores/useAiWorkspaceStore";
import { useLayoutStore } from "@/stores/useLayoutStore";

export function SolutionVersionsTab() {
  const { setActiveTab } = useLayoutStore();
  const {
    solutions,
    selectedSolutionId,
    savingSolution,
    selectSolution,
    restoreSolutionVersion,
  } = useAiWorkspaceStore();

  if (solutions.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center p-8 text-center text-xs text-slate-500">
        <History className="mb-2 h-6 w-6 text-slate-400" />
        <p>まだ解答バージョン履歴がありません。</p>
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto p-4 space-y-2.5">
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
                <Badge
                  variant={sol.validationStatus === "invalid" ? "warning" : "default"}
                >
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
  );
}
