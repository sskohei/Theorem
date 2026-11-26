import { useEffect, useState } from "react";
import { AlertTriangle, Code2, Eye, RefreshCw, Save, Sparkles } from "lucide-react";
import { MarkdownMath } from "@/components/MarkdownMath";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { useAiWorkspaceStore } from "@/stores/useAiWorkspaceStore";
import { useLayoutStore } from "@/stores/useLayoutStore";

interface StructuredStep {
  type: string;
  heading?: string | null;
  text?: string | null;
  latex?: string | null;
  display?: boolean;
}

interface ParsedSolutionDoc {
  title?: string;
  subject?: string;
  mode?: string;
  steps?: StructuredStep[];
  answer?: { text?: string | null; latex?: string | null; unit?: string | null };
  [key: string]: unknown;
}

export function SolutionEditorTab() {
  const { setActiveTab } = useLayoutStore();
  const {
    activeDocumentId,
    solutions,
    selectedSolutionId,
    savingSolution,
    generatingSolution,
    solutionSchemaIssues,
    solutionLatexErrors,
    solutionPdfError,
    saveSolutionEdit,
    generateSolution,
  } = useAiWorkspaceStore();

  const activeSolution =
    solutions.find((s) => s.id === selectedSolutionId) ??
    solutions.find((s) => s.isCurrent) ??
    solutions[0] ??
    null;

  const [editorMode, setEditorMode] = useState<"structured" | "json">("structured");
  const [jsonText, setJsonText] = useState("");
  const [changeSummary, setChangeSummary] = useState("");
  const [localParseError, setLocalParseError] = useState<string | null>(null);

  useEffect(() => {
    setJsonText(activeSolution?.structuredJson ?? "");
    setChangeSummary("");
    setLocalParseError(null);
  }, [activeSolution?.id, activeSolution?.structuredJson]);

  const parsedDoc: ParsedSolutionDoc | null = (() => {
    if (!jsonText.trim()) return null;
    try {
      const v = JSON.parse(jsonText) as ParsedSolutionDoc;
      return typeof v === "object" && v !== null ? v : null;
    } catch {
      return null;
    }
  })();

  const updateParsed = (updater: (doc: ParsedSolutionDoc) => ParsedSolutionDoc) => {
    if (!parsedDoc) return;
    const next = updater({ ...parsedDoc });
    setJsonText(JSON.stringify(next, null, 2));
    setLocalParseError(null);
  };

  const handleSaveNewVersion = async () => {
    if (!activeDocumentId || !activeSolution) return;
    try {
      JSON.parse(jsonText);
      setLocalParseError(null);
    } catch (err) {
      setLocalParseError(
        `JSON 構文エラー: ${err instanceof Error ? err.message : String(err)}`,
      );
      return;
    }

    const res = await saveSolutionEdit({
      documentId: activeDocumentId,
      problemId: activeSolution.problemId,
      parentSolutionId: activeSolution.id,
      mode: activeSolution.mode,
      structuredJson: jsonText,
      changeSummary: changeSummary.trim() || "解答エディタから手動修正",
    });
    if (res && res.schemaIssues.length === 0 && res.latexErrors.length === 0 && !res.pdfError) {
      setActiveTab("solutionPdf");
    }
  };

  const handleBoundedRegenerate = async () => {
    if (!activeDocumentId || !activeSolution) return;
    const feedbackLines: string[] = [];
    for (const le of solutionLatexErrors) {
      feedbackLines.push(`- ${le.location}: \`${le.latex}\` (${le.reason})`);
    }
    for (const si of solutionSchemaIssues) {
      feedbackLines.push(`- Schema ${si.path}: ${si.message}`);
    }
    if (solutionPdfError) {
      feedbackLines.push(`- Typst compile: ${solutionPdfError}`);
    }
    const res = await generateSolution({
      documentId: activeDocumentId,
      problemId: activeSolution.problemId,
      mode: activeSolution.mode,
      targetScope: activeSolution.problemId ? "single_problem" : "all_problems",
      parentSolutionId: activeSolution.id,
      repairFeedback:
        feedbackLines.join("\n") || "標準的な Schema v1 JSON と標準 LaTeX のみで再生成してください。",
    });
    if (res && res.schemaIssues.length === 0 && res.latexErrors.length === 0 && !res.pdfError) {
      setActiveTab("solutionPdf");
    }
  };

  if (!activeSolution) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-xs text-slate-500">
        編集対象の解答バージョンがありません。先に「認識した問題文」タブから解答を生成してください。
      </div>
    );
  }

  const hasErrors =
    solutionSchemaIssues.length > 0 ||
    solutionLatexErrors.length > 0 ||
    Boolean(solutionPdfError) ||
    Boolean(localParseError);

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Toolbar */}
      <div className="flex flex-wrap items-center gap-2 border-b border-slate-200 bg-white px-3 py-1.5 text-xs dark:border-slate-800 dark:bg-slate-900">
        <Badge variant="info">基準: v{activeSolution.versionNumber}</Badge>
        <div className="inline-flex rounded-md border border-slate-300 p-0.5 dark:border-slate-700">
          <button
            type="button"
            onClick={() => setEditorMode("structured")}
            className={`inline-flex items-center gap-1 rounded px-2 py-0.5 text-[11px] ${
              editorMode === "structured"
                ? "bg-blue-600 text-white"
                : "text-slate-600 dark:text-slate-300"
            }`}
          >
            <Eye className="h-3 w-3" />
            構造化フォーム
          </button>
          <button
            type="button"
            onClick={() => setEditorMode("json")}
            className={`inline-flex items-center gap-1 rounded px-2 py-0.5 text-[11px] ${
              editorMode === "json"
                ? "bg-blue-600 text-white"
                : "text-slate-600 dark:text-slate-300"
            }`}
          >
            <Code2 className="h-3 w-3" />
            JSON 直接編集
          </button>
        </div>

        <Input
          value={changeSummary}
          onChange={(e) => setChangeSummary(e.target.value)}
          placeholder="変更概要メモ（任意）"
          className="h-7 w-48"
        />

        <div className="ml-auto flex items-center gap-1.5">
          {hasErrors && (
            <Button
              size="xs"
              variant="outline"
              disabled={generatingSolution}
              onClick={() => void handleBoundedRegenerate()}
            >
              <Sparkles className="h-3.5 w-3.5" />
              <span>{generatingSolution ? "再生成中..." : "エラー箇所を修正して再生成"}</span>
            </Button>
          )}
          <Button
            size="xs"
            variant="primary"
            disabled={savingSolution}
            onClick={() => void handleSaveNewVersion()}
          >
            {savingSolution ? (
              <RefreshCw className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Save className="h-3.5 w-3.5" />
            )}
            <span>新バージョンとして保存・PDF 再生成</span>
          </Button>
        </div>
      </div>

      {/* Error Highlight Panel */}
      {hasErrors && (
        <div className="space-y-1 border-b border-red-200 bg-red-50 px-3 py-2 text-xs text-red-900 dark:border-red-900 dark:bg-red-950/60 dark:text-red-100">
          <div className="flex items-center gap-1.5 font-semibold">
            <AlertTriangle className="h-4 w-4 text-red-600" />
            <span>修正が必要なエラー箇所</span>
          </div>
          {localParseError && <div>{localParseError}</div>}
          {solutionLatexErrors.map((le, i) => (
            <div key={i} className="rounded bg-white/80 px-2 py-1 font-mono text-[11px] dark:bg-slate-900">
              <strong>{le.location}</strong>: <code>{le.latex}</code> — {le.reason}
            </div>
          ))}
          {solutionSchemaIssues.map((si, i) => (
            <div key={i} className="font-mono text-[11px]">
              <strong>{si.path}</strong>: {si.message}
            </div>
          ))}
          {solutionPdfError && <div className="font-mono text-[11px]">{solutionPdfError}</div>}
        </div>
      )}

      {/* Editor Body */}
      <div className="flex-1 overflow-y-auto p-4">
        {editorMode === "json" || !parsedDoc ? (
          <div className="flex h-full flex-col gap-2">
            <p className="text-[11px] text-slate-500">
              Schema v1 準拠の JSON を直接修正できます。保存すると過去バージョンを上書きせず新バージョン（vN+1）を作成し Typst で PDF を再コンパイルします。
            </p>
            <textarea
              aria-label="構造化解答 JSON エディタ"
              value={jsonText}
              onChange={(e) => setJsonText(e.target.value)}
              className="min-h-[380px] flex-1 rounded-lg border border-slate-300 bg-white p-3 font-mono text-xs leading-relaxed dark:border-slate-700 dark:bg-slate-900"
            />
          </div>
        ) : (
          <div className="space-y-4 text-xs">
            <div className="rounded-lg border border-slate-200 bg-white p-3 dark:border-slate-800 dark:bg-slate-900">
              <label className="mb-1 block text-[11px] font-semibold text-slate-600 dark:text-slate-300">
                解答タイトル
              </label>
              <Input
                value={parsedDoc.title ?? ""}
                onChange={(e) => updateParsed((d) => ({ ...d, title: e.target.value }))}
              />
            </div>

            {/* Steps Editor with live KaTeX preview + LaTeX error highlight */}
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <span className="font-semibold text-slate-700 dark:text-slate-200">
                  解法ステップ ({parsedDoc.steps?.length ?? 0})
                </span>
                <Button
                  size="xs"
                  variant="outline"
                  onClick={() =>
                    updateParsed((d) => ({
                      ...d,
                      steps: [
                        ...(d.steps ?? []),
                        { type: "equation", text: "", latex: "", display: true },
                      ],
                    }))
                  }
                >
                  + ステップを追加
                </Button>
              </div>

              {(parsedDoc.steps ?? []).map((step, idx) => {
                const stepLatexErr = solutionLatexErrors.find((e) => e.stepIndex === idx);
                return (
                  <div
                    key={idx}
                    className={`rounded-lg border p-3 space-y-2 ${
                      stepLatexErr
                        ? "border-red-400 bg-red-50/40 dark:border-red-800 dark:bg-red-950/30"
                        : "border-slate-200 bg-white dark:border-slate-800 dark:bg-slate-900"
                    }`}
                  >
                    <div className="flex items-center justify-between">
                      <span className="font-semibold text-slate-700 dark:text-slate-200">
                        Step {idx + 1} ({step.type})
                      </span>
                      {stepLatexErr && (
                        <Badge variant="warning">LaTeX 変換エラー: {stepLatexErr.reason}</Badge>
                      )}
                    </div>
                    <textarea
                      rows={2}
                      placeholder="解説テキスト"
                      value={step.text ?? ""}
                      onChange={(e) =>
                        updateParsed((d) => {
                          const steps = [...(d.steps ?? [])];
                          steps[idx] = { ...steps[idx]!, text: e.target.value };
                          return { ...d, steps };
                        })
                      }
                      className="w-full rounded border border-slate-300 bg-white p-2 text-xs dark:border-slate-700 dark:bg-slate-950"
                    />
                    <div className="grid gap-2 lg:grid-cols-2">
                      <div>
                        <label className="mb-0.5 block text-[10px] text-slate-500">
                          LaTeX 数式（$ 記号不要）
                        </label>
                        <Input
                          value={step.latex ?? ""}
                          placeholder="\frac{1}{2} m v^2"
                          onChange={(e) =>
                            updateParsed((d) => {
                              const steps = [...(d.steps ?? [])];
                              steps[idx] = {
                                ...steps[idx]!,
                                latex: e.target.value || null,
                              };
                              return { ...d, steps };
                            })
                          }
                          className="font-mono"
                        />
                      </div>
                      <div className="rounded border border-slate-200 bg-slate-50 p-2 dark:border-slate-800 dark:bg-slate-950">
                        {step.latex ? (
                          <MarkdownMath content={`$$${step.latex}$$`} />
                        ) : (
                          <span className="text-[11px] text-slate-400">数式プレビュー</span>
                        )}
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>

            {/* Final Answer Box */}
            <div className="rounded-lg border border-blue-200 bg-white p-3 space-y-2 dark:border-blue-900 dark:bg-slate-900">
              <div className="font-semibold text-blue-800 dark:text-blue-200">最終解答</div>
              <Input
                value={parsedDoc.answer?.text ?? ""}
                placeholder="解答テキスト"
                onChange={(e) =>
                  updateParsed((d) => ({
                    ...d,
                    answer: { ...(d.answer ?? {}), text: e.target.value || null },
                  }))
                }
              />
              <div className="grid gap-2 lg:grid-cols-2">
                <Input
                  value={parsedDoc.answer?.latex ?? ""}
                  placeholder="最終解答 LaTeX"
                  onChange={(e) =>
                    updateParsed((d) => ({
                      ...d,
                      answer: { ...(d.answer ?? {}), latex: e.target.value || null },
                    }))
                  }
                  className="font-mono"
                />
                <div className="rounded border border-slate-200 bg-slate-50 p-2 dark:border-slate-800 dark:bg-slate-950">
                  {parsedDoc.answer?.latex ? (
                    <MarkdownMath content={`$$${parsedDoc.answer.latex}$$`} />
                  ) : (
                    <span className="text-[11px] text-slate-400">最終解答数式プレビュー</span>
                  )}
                </div>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
