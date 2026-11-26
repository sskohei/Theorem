import { useMemo, useState } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  Combine,
  Plus,
  Scissors,
  Settings,
  Sparkles,
  Trash2,
} from "lucide-react";
import { MarkdownMath } from "@/components/MarkdownMath";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { cn } from "@/lib/utils";
import { useAgentStore } from "@/stores/useAgentStore";
import { useAiWorkspaceStore } from "@/stores/useAiWorkspaceStore";
import { useDocumentStore } from "@/stores/useDocumentStore";
import { useLayoutStore } from "@/stores/useLayoutStore";
import { useLibraryStore } from "@/stores/useLibraryStore";
import type { Problem, RecognitionMode, SolutionMode, SubjectCategory } from "@/types/ipc";

const MATH_FIELDS: [string, string][] = [
  ["calculus", "微分積分学"],
  ["linear_algebra", "線形代数学"],
  ["set_topology", "集合と位相"],
  ["algebra", "代数学（群・環・体）"],
  ["complex_analysis", "複素解析"],
  ["real_analysis", "実解析・測度論"],
  ["ode", "常微分方程式"],
  ["pde", "偏微分方程式"],
  ["vector_analysis", "ベクトル解析"],
  ["fourier_analysis", "フーリエ解析"],
  ["probability_statistics", "確率・統計"],
  ["discrete_math", "離散数学・組合せ論"],
  ["differential_geometry", "微分幾何学"],
  ["functional_analysis", "関数解析"],
];

const PHYSICS_FIELDS: [string, string][] = [
  ["classical_mechanics", "古典力学"],
  ["analytical_mechanics", "解析力学"],
  ["electromagnetism", "電磁気学"],
  ["thermodynamics", "熱力学"],
  ["statistical_mechanics", "統計力学"],
  ["quantum_mechanics", "量子力学"],
  ["waves_optics", "振動・波動・光学"],
  ["relativity", "相対性理論"],
  ["fluid_continuum", "流体力学・連続体力学"],
  ["solid_state", "物性物理学"],
];

interface RecognizedProblemsTabProps {
  onTriggerRecognize?: (mode: RecognitionMode) => void;
}

export function RecognizedProblemsTab({ onTriggerRecognize }: RecognizedProblemsTabProps) {
  const { setActiveTab, openSettings } = useLayoutStore();
  const { settings } = useLibraryStore();
  const { detail, pageRangeStart, pageRangeEnd, croppedPreviewUrl, extractedPageText, currentPage } =
    useDocumentStore();
  const { profiles, statusesByProfileId } = useAgentStore();
  const {
    problems,
    selectedProblemId,
    recognizing,
    generatingSolution,
    recognitionSchemaIssues,
    recognitionRawOutput,
    selectProblem,
    recognize,
    upsertProblem,
    deleteProblem,
    splitProblem,
    mergeProblems,
    generateSolution,
  } = useAiWorkspaceStore();

  const [recMode, setRecMode] = useState<RecognitionMode>(settings.defaultRecognitionMode);
  const [solutionMode, setSolutionMode] = useState<SolutionMode>("detailed");
  const [targetScope, setTargetScope] = useState<
    "single_problem" | "all_problems" | "page_range"
  >("single_problem");

  const [editingId, setEditingId] = useState<string | null>(null);
  const [draft, setDraft] = useState<{
    problemNumber: string;
    subproblemNumber: string;
    subjectCategory: SubjectCategory;
    field: string;
    statementMarkdown: string;
    givenConditionsText: string;
    targetObjective: string;
    sourcePagesText: string;
    figureRefsText: string;
    uncertaintiesText: string;
    isUserVerified: boolean;
  } | null>(null);

  const [splittingId, setSplittingId] = useState<string | null>(null);
  const [splitFirst, setSplitFirst] = useState("");
  const [splitSecond, setSplitSecond] = useState("");
  const [splitSubNum, setSplitSubNum] = useState("(2)");

  const docId = detail?.document.id ?? null;

  // Determine active extraction profile & check image-input capability
  const extractionProfile = useMemo(() => {
    const assigned = settings.agentAssignments.extractionProfileId;
    return (
      profiles.find((p) => p.id === assigned) ??
      profiles.find((p) => p.isDefault && p.isEnabled) ??
      profiles[0] ??
      null
    );
  }, [profiles, settings.agentAssignments.extractionProfileId]);

  const extractionStatus = extractionProfile
    ? statusesByProfileId[extractionProfile.id]
    : undefined;
  const imageUnsupported =
    extractionStatus !== undefined && !extractionStatus.capabilities.supportsImageInput;

  const selectedProblem = problems.find((p) => p.id === selectedProblemId) ?? problems[0] ?? null;

  const startEdit = (p: Problem) => {
    setEditingId(p.id);
    setDraft({
      problemNumber: p.problemNumber,
      subproblemNumber: p.subproblemNumber ?? "",
      subjectCategory: p.subjectCategory,
      field: p.field ?? "",
      statementMarkdown: p.statementMarkdown,
      givenConditionsText: p.givenConditions.join("\n"),
      targetObjective: p.targetObjective ?? "",
      sourcePagesText: p.sourcePages.join(", "),
      figureRefsText: p.figureRefs.join("\n"),
      uncertaintiesText: p.uncertainties.join("\n"),
      isUserVerified: p.isUserVerified,
    });
  };

  const handleSaveEdit = async () => {
    if (!docId || !draft) return;
    const pages = draft.sourcePagesText
      .split(/[,\s]+/)
      .map((s) => Number.parseInt(s, 10))
      .filter((n) => !Number.isNaN(n) && n >= 1);

    await upsertProblem({
      id: editingId === "__new__" ? null : editingId,
      documentId: docId,
      problemNumber: draft.problemNumber.trim() || "第1問",
      subproblemNumber: draft.subproblemNumber.trim() || null,
      subjectCategory: draft.subjectCategory,
      field: draft.field || null,
      statementMarkdown: draft.statementMarkdown,
      givenConditions: draft.givenConditionsText
        .split("\n")
        .map((s) => s.trim())
        .filter(Boolean),
      targetObjective: draft.targetObjective.trim() || null,
      sourcePages: pages,
      figureRefs: draft.figureRefsText
        .split("\n")
        .map((s) => s.trim())
        .filter(Boolean),
      uncertainties: draft.uncertaintiesText
        .split("\n")
        .map((s) => s.trim())
        .filter(Boolean),
      isUserVerified: draft.isUserVerified,
    });
    setEditingId(null);
    setDraft(null);
  };

  const handleRunRecognize = async () => {
    if (!docId) return;
    if (onTriggerRecognize) {
      onTriggerRecognize(recMode);
      return;
    }
    const cropB64 = croppedPreviewUrl?.includes(",") ? croppedPreviewUrl.split(",")[1] : null;
    await recognize({
      documentId: docId,
      recognitionMode: recMode,
      pageRangeStart: recMode === "page_range" ? pageRangeStart : null,
      pageRangeEnd: recMode === "page_range" ? pageRangeEnd : null,
      pageImages: extractedPageText
        ? [{ pageNumber: currentPage, pngBase64: "", extractedText: extractedPageText }]
        : [],
      croppedRegionPngBase64: cropB64 ?? null,
    });
  };

  const handleGenerateSolution = async () => {
    if (!docId) return;
    const res = await generateSolution({
      documentId: docId,
      problemId: targetScope === "single_problem" ? (selectedProblem?.id ?? null) : null,
      mode: solutionMode,
      targetScope,
      pageRangeStart: targetScope === "page_range" ? pageRangeStart : null,
      pageRangeEnd: targetScope === "page_range" ? pageRangeEnd : null,
    });
    if (res) {
      if (
        res.schemaIssues.length > 0 ||
        res.latexErrors.length > 0 ||
        res.pdfError
      ) {
        setActiveTab("solutionEditor");
      } else {
        setActiveTab("solutionPdf");
      }
    }
  };

  if (!docId) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-xs text-slate-500">
        左サイドバーから問題ドキュメントを選択してください。
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Top Action Bar: Recognize & Generate Solution */}
      <div className="flex flex-wrap items-center gap-2 border-b border-slate-200 bg-white px-3 py-2 dark:border-slate-800 dark:bg-slate-900">
        <div className="flex items-center gap-1.5">
          <select
            aria-label="問題認識モード"
            value={recMode}
            onChange={(e) => setRecMode(e.target.value as RecognitionMode)}
            className="h-7 rounded border border-slate-300 bg-white px-2 text-xs dark:border-slate-700 dark:bg-slate-800"
          >
            <option value="printed_only">印刷問題のみ（既定）</option>
            <option value="include_handwriting">手書きを含める</option>
            <option value="page_range">
              ページ範囲指定 ({pageRangeStart}〜{pageRangeEnd})
            </option>
          </select>
          <Button
            size="xs"
            variant="primary"
            disabled={recognizing || imageUnsupported}
            onClick={() => void handleRunRecognize()}
          >
            <Sparkles className="h-3.5 w-3.5" />
            <span>{recognizing ? "問題認識中..." : "AI で問題認識"}</span>
          </Button>
          <Button
            size="xs"
            variant="outline"
            onClick={() => {
              setEditingId("__new__");
              setDraft({
                problemNumber: `第${problems.length + 1}問`,
                subproblemNumber: "",
                subjectCategory: "mathematics",
                field: "calculus",
                statementMarkdown: "",
                givenConditionsText: "",
                targetObjective: "",
                sourcePagesText: String(currentPage),
                figureRefsText: "",
                uncertaintiesText: "",
                isUserVerified: true,
              });
            }}
          >
            <Plus className="h-3.5 w-3.5" />
            <span>手動追加</span>
          </Button>
        </div>

        <div className="ml-auto flex flex-wrap items-center gap-1.5">
          <select
            aria-label="解答生成対象"
            value={targetScope}
            onChange={(e) =>
              setTargetScope(e.target.value as "single_problem" | "all_problems" | "page_range")
            }
            className="h-7 rounded border border-slate-300 bg-white px-2 text-xs dark:border-slate-700 dark:bg-slate-800"
          >
            <option value="single_problem">選択中の問題のみ</option>
            <option value="all_problems">ドキュメント内の全問題</option>
            <option value="page_range">
              ページ範囲 ({pageRangeStart}〜{pageRangeEnd})
            </option>
          </select>

          <select
            aria-label="解答生成モード"
            value={solutionMode}
            onChange={(e) => setSolutionMode(e.target.value as SolutionMode)}
            className="h-7 rounded border border-slate-300 bg-white px-2 text-xs dark:border-slate-700 dark:bg-slate-800"
          >
            <option value="standard">標準 (standard)</option>
            <option value="detailed">詳細解説 (detailed)</option>
            <option value="rigorous">厳密証明 (rigorous)</option>
            <option value="hint">ヒント (hint)</option>
          </select>

          <Button
            size="xs"
            variant="primary"
            disabled={generatingSolution || problems.length === 0}
            onClick={() => void handleGenerateSolution()}
          >
            <Sparkles className="h-3.5 w-3.5" />
            <span>{generatingSolution ? "解答・PDF生成中..." : "解答を生成する"}</span>
          </Button>
        </div>
      </div>

      {/* Image Unsupported Warning */}
      {imageUnsupported && (
        <div
          role="alert"
          className="flex items-center justify-between gap-2 border-b border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-900 dark:border-amber-900 dark:bg-amber-950/60 dark:text-amber-100"
        >
          <div className="flex items-center gap-2">
            <AlertTriangle className="h-4 w-4 shrink-0 text-amber-600" />
            <span>
              現在選択されている認識用エージェント（{extractionProfile?.name}
              ）は画像入力に対応していません。画像入力対応エージェント（Codex / Antigravity 等）へ切り替えてください。
            </span>
          </div>
          <Button size="xs" variant="outline" onClick={() => openSettings("agents")}>
            <Settings className="h-3.5 w-3.5" />
            <span>エージェント設定</span>
          </Button>
        </div>
      )}

      {/* Recognition Schema Validation Errors */}
      {recognitionSchemaIssues.length > 0 && (
        <div className="border-b border-red-200 bg-red-50 px-3 py-2 text-xs text-red-800 dark:border-red-900 dark:bg-red-950/60 dark:text-red-200">
          <div className="font-semibold">問題認識出力の JSON Schema 検証エラー:</div>
          <ul className="mt-1 list-disc pl-4">
            {recognitionSchemaIssues.map((iss, i) => (
              <li key={i}>
                <code>{iss.path}</code>: {iss.message}
              </li>
            ))}
          </ul>
          {recognitionRawOutput && (
            <pre className="mt-1.5 max-h-28 overflow-auto rounded bg-white/80 p-2 font-mono text-[10px] dark:bg-slate-900">
              {recognitionRawOutput}
            </pre>
          )}
        </div>
      )}

      {/* Body: Problem Cards & Editor */}
      <div className="flex-1 overflow-y-auto p-4 space-y-3">
        {editingId === "__new__" && draft && (
          <ProblemEditorCard
            draft={draft}
            setDraft={setDraft}
            onSave={() => void handleSaveEdit()}
            onCancel={() => {
              setEditingId(null);
              setDraft(null);
            }}
          />
        )}

        {problems.length === 0 && editingId !== "__new__" ? (
          <div className="rounded-xl border border-dashed border-slate-300 bg-white p-8 text-center dark:border-slate-700 dark:bg-slate-900">
            <p className="text-sm font-semibold text-slate-800 dark:text-slate-100">
              まだ問題文が認識・登録されていません
            </p>
            <p className="mt-1.5 text-xs text-slate-500 dark:text-slate-400">
              「AI で問題認識」を実行するか、「手動追加」から問題文と LaTeX 数式を入力してください。
            </p>
          </div>
        ) : (
          problems.map((p, index) => {
            const isSelected = selectedProblem?.id === p.id;
            const nextProblem = problems[index + 1];

            if (editingId === p.id && draft) {
              return (
                <ProblemEditorCard
                  key={p.id}
                  draft={draft}
                  setDraft={setDraft}
                  onSave={() => void handleSaveEdit()}
                  onCancel={() => {
                    setEditingId(null);
                    setDraft(null);
                  }}
                />
              );
            }

            return (
              <div
                key={p.id}
                onClick={() => selectProblem(p.id)}
                className={cn(
                  "rounded-xl border bg-white p-4 shadow-2xs transition-colors dark:bg-slate-900",
                  isSelected
                    ? "border-blue-500 ring-1 ring-blue-500/30"
                    : "border-slate-200 hover:border-slate-300 dark:border-slate-800 dark:hover:border-slate-700",
                )}
              >
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div className="flex flex-wrap items-center gap-1.5">
                    <span className="text-xs font-bold text-slate-900 dark:text-slate-100">
                      {p.problemNumber} {p.subproblemNumber ?? ""}
                    </span>
                    <Badge variant="info">
                      {p.subjectCategory === "mathematics"
                        ? "大学数学"
                        : p.subjectCategory === "physics"
                          ? "大学物理"
                          : "その他"}
                    </Badge>
                    {p.field && <Badge variant="outline">{p.field}</Badge>}
                    {p.sourcePages.length > 0 && (
                      <Badge variant="default">p.{p.sourcePages.join(",")}</Badge>
                    )}
                    {p.isUserVerified && (
                      <Badge variant="info" className="gap-1 bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-200">
                        <CheckCircle2 className="h-3 w-3" />
                        確認済
                      </Badge>
                    )}
                  </div>

                  <div className="flex items-center gap-1">
                    <Button
                      size="xs"
                      variant="outline"
                      onClick={(e) => {
                        e.stopPropagation();
                        startEdit(p);
                      }}
                    >
                      編集
                    </Button>
                    <Button
                      size="xs"
                      variant="ghost"
                      title="この問題を 2 つに分割"
                      onClick={(e) => {
                        e.stopPropagation();
                        setSplittingId((prev) => (prev === p.id ? null : p.id));
                        const mid = Math.floor(p.statementMarkdown.length / 2);
                        setSplitFirst(p.statementMarkdown.slice(0, mid));
                        setSplitSecond(p.statementMarkdown.slice(mid));
                        setSplitSubNum("(2)");
                      }}
                    >
                      <Scissors className="h-3.5 w-3.5" />
                      <span>分割</span>
                    </Button>
                    {nextProblem && (
                      <Button
                        size="xs"
                        variant="ghost"
                        title="次の問題と結合"
                        onClick={(e) => {
                          e.stopPropagation();
                          void mergeProblems(p.id, nextProblem.id);
                        }}
                      >
                        <Combine className="h-3.5 w-3.5" />
                        <span>次と結合</span>
                      </Button>
                    )}
                    <Button
                      size="xs"
                      variant="ghost"
                      className="text-red-600 hover:bg-red-50 dark:text-red-400"
                      onClick={(e) => {
                        e.stopPropagation();
                        if (window.confirm("この問題を削除しますか？")) {
                          void deleteProblem(p.id);
                        }
                      }}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </Button>
                  </div>
                </div>

                {/* Uncertainties Warning Banner */}
                {p.uncertainties.length > 0 && (
                  <div className="mt-2 rounded-md border border-amber-300 bg-amber-50 p-2 text-xs text-amber-900 dark:border-amber-800 dark:bg-amber-950/50 dark:text-amber-100">
                    <div className="flex items-center gap-1.5 font-semibold">
                      <AlertTriangle className="h-3.5 w-3.5 text-amber-600" />
                      <span>要確認箇所（推測補完せず警告表示）</span>
                    </div>
                    <ul className="mt-1 list-disc pl-4">
                      {p.uncertainties.map((u, idx) => (
                        <li key={idx}>{u}</li>
                      ))}
                    </ul>
                  </div>
                )}

                <div className="mt-2.5">
                  <MarkdownMath content={p.statementMarkdown} />
                </div>

                {(p.givenConditions.length > 0 ||
                  p.targetObjective ||
                  p.figureRefs.length > 0) && (
                  <div className="mt-3 grid gap-2 border-t border-slate-100 pt-2.5 text-[11px] text-slate-600 dark:border-slate-800 dark:text-slate-400">
                    {p.givenConditions.length > 0 && (
                      <div>
                        <span className="font-semibold">与えられた条件: </span>
                        {p.givenConditions.join(" / ")}
                      </div>
                    )}
                    {p.targetObjective && (
                      <div>
                        <span className="font-semibold">求める対象: </span>
                        {p.targetObjective}
                      </div>
                    )}
                    {p.figureRefs.length > 0 && (
                      <div>
                        <span className="font-semibold">図参照: </span>
                        {p.figureRefs.join(" / ")}
                      </div>
                    )}
                  </div>
                )}

                {splittingId === p.id && (
                  <div
                    onClick={(e) => e.stopPropagation()}
                    className="mt-3 space-y-2 rounded-lg border border-blue-200 bg-blue-50/50 p-3 text-xs dark:border-blue-900 dark:bg-blue-950/30"
                  >
                    <div className="font-semibold text-blue-900 dark:text-blue-200">
                      問題を 2 つに分割
                    </div>
                    <textarea
                      rows={2}
                      value={splitFirst}
                      onChange={(e) => setSplitFirst(e.target.value)}
                      placeholder="前半の問題文"
                      className="w-full rounded border border-slate-300 bg-white p-2 font-mono text-xs dark:border-slate-700 dark:bg-slate-900"
                    />
                    <div className="flex items-center gap-2">
                      <span className="text-[11px]">後半の小問番号:</span>
                      <Input
                        value={splitSubNum}
                        onChange={(e) => setSplitSubNum(e.target.value)}
                        className="h-7 w-24"
                      />
                    </div>
                    <textarea
                      rows={2}
                      value={splitSecond}
                      onChange={(e) => setSplitSecond(e.target.value)}
                      placeholder="後半の問題文"
                      className="w-full rounded border border-slate-300 bg-white p-2 font-mono text-xs dark:border-slate-700 dark:bg-slate-900"
                    />
                    <div className="flex justify-end gap-1.5">
                      <Button size="xs" variant="ghost" onClick={() => setSplittingId(null)}>
                        キャンセル
                      </Button>
                      <Button
                        size="xs"
                        variant="primary"
                        onClick={() => {
                          void splitProblem(p.id, splitFirst, splitSecond, splitSubNum).then(() =>
                            setSplittingId(null),
                          );
                        }}
                      >
                        分割を実行
                      </Button>
                    </div>
                  </div>
                )}
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}

interface ProblemDraft {
  problemNumber: string;
  subproblemNumber: string;
  subjectCategory: SubjectCategory;
  field: string;
  statementMarkdown: string;
  givenConditionsText: string;
  targetObjective: string;
  sourcePagesText: string;
  figureRefsText: string;
  uncertaintiesText: string;
  isUserVerified: boolean;
}

function ProblemEditorCard({
  draft,
  setDraft,
  onSave,
  onCancel,
}: {
  draft: ProblemDraft;
  setDraft: (d: ProblemDraft) => void;
  onSave: () => void;
  onCancel: () => void;
}) {
  const fieldOptions =
    draft.subjectCategory === "physics" ? PHYSICS_FIELDS : MATH_FIELDS;

  return (
    <div className="space-y-3 rounded-xl border border-blue-400 bg-white p-4 text-xs shadow-xs dark:border-blue-700 dark:bg-slate-900">
      <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
        <div>
          <label className="mb-1 block text-[11px] font-medium text-slate-500">問題番号</label>
          <Input
            value={draft.problemNumber}
            onChange={(e) => setDraft({ ...draft, problemNumber: e.target.value })}
          />
        </div>
        <div>
          <label className="mb-1 block text-[11px] font-medium text-slate-500">小問番号</label>
          <Input
            value={draft.subproblemNumber}
            placeholder="(1)"
            onChange={(e) => setDraft({ ...draft, subproblemNumber: e.target.value })}
          />
        </div>
        <div>
          <label className="mb-1 block text-[11px] font-medium text-slate-500">科目カテゴリ</label>
          <select
            value={draft.subjectCategory}
            onChange={(e) =>
              setDraft({ ...draft, subjectCategory: e.target.value as SubjectCategory })
            }
            className="h-8 w-full rounded-md border border-slate-300 bg-white px-2 text-xs dark:border-slate-700 dark:bg-slate-900"
          >
            <option value="mathematics">大学数学</option>
            <option value="physics">大学物理</option>
            <option value="other">その他</option>
          </select>
        </div>
        <div>
          <label className="mb-1 block text-[11px] font-medium text-slate-500">分野</label>
          <select
            value={draft.field}
            onChange={(e) => setDraft({ ...draft, field: e.target.value })}
            className="h-8 w-full rounded-md border border-slate-300 bg-white px-2 text-xs dark:border-slate-700 dark:bg-slate-900"
          >
            <option value="">未選択</option>
            {fieldOptions.map(([key, label]) => (
              <option key={key} value={key}>
                {label}
              </option>
            ))}
          </select>
        </div>
      </div>

      <div className="grid gap-3 lg:grid-cols-2">
        <div>
          <label className="mb-1 block text-[11px] font-medium text-slate-500">
            問題文（Markdown + LaTeX）
          </label>
          <textarea
            rows={5}
            value={draft.statementMarkdown}
            onChange={(e) => setDraft({ ...draft, statementMarkdown: e.target.value })}
            className="w-full rounded-md border border-slate-300 bg-white p-2 font-mono text-xs dark:border-slate-700 dark:bg-slate-950"
          />
        </div>
        <div>
          <label className="mb-1 block text-[11px] font-medium text-slate-500">
            KaTeX ライブプレビュー
          </label>
          <div className="min-h-[112px] rounded-md border border-slate-200 bg-slate-50 p-2.5 dark:border-slate-800 dark:bg-slate-950/60">
            <MarkdownMath content={draft.statementMarkdown || "*数式・問題文プレビュー*"} />
          </div>
        </div>
      </div>

      <div className="grid gap-2 sm:grid-cols-3">
        <div>
          <label className="mb-1 block text-[11px] font-medium text-slate-500">
            与えられた条件（1行1項目）
          </label>
          <textarea
            rows={2}
            value={draft.givenConditionsText}
            onChange={(e) => setDraft({ ...draft, givenConditionsText: e.target.value })}
            className="w-full rounded border border-slate-300 bg-white p-1.5 text-xs dark:border-slate-700 dark:bg-slate-950"
          />
        </div>
        <div>
          <label className="mb-1 block text-[11px] font-medium text-slate-500">求める対象</label>
          <Input
            value={draft.targetObjective}
            onChange={(e) => setDraft({ ...draft, targetObjective: e.target.value })}
          />
          <label className="mt-1.5 mb-0.5 block text-[11px] font-medium text-slate-500">
            掲載ページ（カンマ区切り）
          </label>
          <Input
            value={draft.sourcePagesText}
            placeholder="1, 2"
            onChange={(e) => setDraft({ ...draft, sourcePagesText: e.target.value })}
          />
        </div>
        <div>
          <label className="mb-1 block text-[11px] font-medium text-slate-500">
            図参照（1行1項目）
          </label>
          <textarea
            rows={2}
            value={draft.figureRefsText}
            onChange={(e) => setDraft({ ...draft, figureRefsText: e.target.value })}
            className="w-full rounded border border-slate-300 bg-white p-1.5 text-xs dark:border-slate-700 dark:bg-slate-950"
          />
        </div>
      </div>

      <div className="flex items-center justify-between pt-1">
        <label className="inline-flex items-center gap-1.5 text-xs text-slate-700 dark:text-slate-300">
          <input
            type="checkbox"
            checked={draft.isUserVerified}
            onChange={(e) => setDraft({ ...draft, isUserVerified: e.target.checked })}
          />
          <span>ユーザー確認済みとしてマーク</span>
        </label>
        <div className="flex gap-2">
          <Button size="xs" variant="ghost" onClick={onCancel}>
            キャンセル
          </Button>
          <Button size="xs" variant="primary" onClick={onSave}>
            保存
          </Button>
        </div>
      </div>
    </div>
  );
}
