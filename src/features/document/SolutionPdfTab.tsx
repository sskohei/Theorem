import { AlertTriangle, FileEdit, RefreshCw, Sparkles } from "lucide-react";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { formatShortDate } from "@/lib/utils";
import { useAiWorkspaceStore } from "@/stores/useAiWorkspaceStore";
import { useLayoutStore } from "@/stores/useLayoutStore";
import { PdfViewer } from "./PdfViewer";

export function SolutionPdfTab() {
  const { setActiveTab } = useLayoutStore();
  const {
    solutions,
    selectedSolutionId,
    solutionPdfAttachment,
    solutionPdfBase64,
    loadingPdf,
    savingSolution,
    solutionSchemaIssues,
    solutionLatexErrors,
    solutionPdfError,
    recompileSolutionPdf,
  } = useAiWorkspaceStore();

  const activeSolution =
    solutions.find((s) => s.id === selectedSolutionId) ??
    solutions.find((s) => s.isCurrent) ??
    solutions[0] ??
    null;

  if (!activeSolution) {
    return (
      <div className="flex h-full flex-col items-center justify-center p-8 text-center">
        <p className="text-sm font-semibold text-slate-800 dark:text-slate-100">
          まだ解答 PDF が生成されていません
        </p>
        <p className="mt-1.5 max-w-md text-xs text-slate-500 dark:text-slate-400">
          「認識した問題文」タブから「解答を生成する」を実行すると、Typst で日本語・数式入り A4 解答 PDF が自動生成されます。
        </p>
        <Button
          size="sm"
          variant="primary"
          className="mt-4"
          onClick={() => setActiveTab("recognized")}
        >
          <Sparkles className="h-4 w-4" />
          <span>「認識した問題文」へ移動</span>
        </Button>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Header Metadata & Actions */}
      <div className="flex flex-wrap items-center gap-2 border-b border-slate-200 bg-white px-3 py-1.5 text-xs dark:border-slate-800 dark:bg-slate-900">
        <Badge variant="info">v{activeSolution.versionNumber}</Badge>
        {activeSolution.isCurrent && <Badge variant="default">最新版</Badge>}
        <Badge variant="outline">{activeSolution.mode}</Badge>
        <span className="text-slate-500 dark:text-slate-400">
          モデル: {activeSolution.modelName ?? "未取得"}
        </span>
        <span className="text-slate-400">· {formatShortDate(activeSolution.createdAt)}</span>

        <div className="ml-auto flex items-center gap-1.5">
          <Button
            size="xs"
            variant="outline"
            disabled={savingSolution}
            onClick={() => void recompileSolutionPdf(activeSolution.id)}
          >
            <RefreshCw className="h-3.5 w-3.5" />
            <span>{savingSolution ? "再コンパイル中..." : "PDF 再生成"}</span>
          </Button>
          <Button size="xs" variant="outline" onClick={() => setActiveTab("solutionEditor")}>
            <FileEdit className="h-3.5 w-3.5" />
            <span>解答エディタで編集</span>
          </Button>
        </div>
      </div>

      {/* Explicit Schema / LaTeX / Typst Errors (never silently dropped!) */}
      {(solutionSchemaIssues.length > 0 ||
        solutionLatexErrors.length > 0 ||
        solutionPdfError) && (
        <div className="space-y-1.5 border-b border-red-200 bg-red-50 px-3 py-2 text-xs text-red-900 dark:border-red-900 dark:bg-red-950/60 dark:text-red-100">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-1.5 font-semibold">
              <AlertTriangle className="h-4 w-4 text-red-600" />
              <span>PDF 生成エラーまたは未対応 LaTeX 記法が検出されました</span>
            </div>
            <Button size="xs" variant="primary" onClick={() => setActiveTab("solutionEditor")}>
              解答エディタで修正する
            </Button>
          </div>
          {solutionLatexErrors.map((le, idx) => (
            <div key={idx} className="rounded bg-white/80 p-1.5 font-mono text-[11px] dark:bg-slate-900">
              [{le.location}] {le.latex} — {le.reason}
            </div>
          ))}
          {solutionSchemaIssues.map((si, idx) => (
            <div key={idx} className="font-mono text-[11px]">
              [Schema {si.path}] {si.message}
            </div>
          ))}
          {solutionPdfError && <div className="font-mono text-[11px]">{solutionPdfError}</div>}
        </div>
      )}

      {/* Embedded PDF.js Viewer */}
      <div className="min-h-0 flex-1 overflow-hidden">
        {loadingPdf ? (
          <div className="flex h-full items-center justify-center text-xs text-slate-500">
            解答 PDF を読み込み中...
          </div>
        ) : solutionPdfAttachment && solutionPdfBase64 ? (
          <PdfViewer
            documentId={activeSolution.documentId}
            attachment={solutionPdfAttachment}
            base64Data={solutionPdfBase64}
            hasSavedThumbnail
          />
        ) : (
          <div className="flex h-full flex-col items-center justify-center p-8 text-center text-xs text-slate-500">
            <p>このバージョンにはコンパイル済み PDF がありません。</p>
            <Button
              size="xs"
              variant="primary"
              className="mt-3"
              onClick={() => setActiveTab("solutionEditor")}
            >
              解答エディタで修正して PDF を生成する
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}
