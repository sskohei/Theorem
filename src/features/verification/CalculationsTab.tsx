import { useEffect, useState } from "react";
import {
  AlertTriangle,
  Bot,
  Calculator,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Clock,
  FilePlus,
  HelpCircle,
  Image as ImageIcon,
  Play,
  RefreshCw,
  ShieldCheck,
  UserCheck,
  XCircle,
} from "lucide-react";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { useAiWorkspaceStore } from "@/stores/useAiWorkspaceStore";
import { useLayoutStore } from "@/stores/useLayoutStore";
import { useLibraryStore } from "@/stores/useLibraryStore";
import { useScienceStore } from "@/stores/useScienceStore";
import type {
  ToolExecutionStatus,
  VerificationSourceKind,
  VerificationStatus,
} from "@/types/ipc";

interface ToolPreset {
  name: string;
  label: string;
  sampleArgs: Record<string, unknown>;
}

const TOOL_PRESETS: ToolPreset[] = [
  {
    name: "verify_equation",
    label: "等式検証 (verify_equation)",
    sampleArgs: {
      lhs: "sin(x)**2 + cos(x)**2",
      rhs: "1",
      assumptions: ["x is real"],
    },
  },
  {
    name: "differentiate",
    label: "微分 (differentiate)",
    sampleArgs: { expression: "x**3 * sin(x)", variable: "x", order: 1 },
  },
  {
    name: "integrate",
    label: "積分 (integrate)",
    sampleArgs: { expression: "exp(-x**2)", variable: "x", lower: "0", upper: "oo" },
  },
  {
    name: "solve_equation",
    label: "方程式求解 (solve_equation)",
    sampleArgs: { equation: "x**2 - 5*x + 6 = 0", variable: "x", domain: "real" },
  },
  {
    name: "solve_ode",
    label: "常微分方程式 (solve_ode)",
    sampleArgs: {
      equation: "Derivative(y(t), t, 2) + 2*Derivative(y(t), t) + 5*y(t) = 0",
      func_name: "y",
      variable: "t",
      ics: { "y(0)": "1", "y'(0)": "0" },
    },
  },
  {
    name: "calculate_determinant",
    label: "行列式 (calculate_determinant)",
    sampleArgs: { matrix: [[2, 1], [5, 3]] },
  },
  {
    name: "calculate_eigenvalues",
    label: "固有値 (calculate_eigenvalues)",
    sampleArgs: { matrix: [[4, 1], [2, 3]] },
  },
  {
    name: "convert_units",
    label: "単位変換 (convert_units)",
    sampleArgs: { value: 72, from_unit: "km/h", to_unit: "m/s" },
  },
  {
    name: "check_dimensions",
    label: "次元解析 (check_dimensions)",
    sampleArgs: {
      lhs_expression: "F",
      rhs_expression: "m * a",
      unit_map: { F: "N", m: "kg", a: "m/s**2" },
    },
  },
  {
    name: "plot_function",
    label: "関数グラフ生成 (plot_function)",
    sampleArgs: {
      expressions: ["exp(-t) * (cos(2*t) + sin(2*t)/2)", "exp(-t)"],
      variable: "t",
      x_range: [0, 6],
      labels: ["x(t)", "envelope"],
      title: "Damped Oscillation",
      output_format: "svg",
    },
  },
  {
    name: "plot_vector_field",
    label: "2D ベクトル場 (plot_vector_field)",
    sampleArgs: {
      u_expr: "-y",
      v_expr: "x",
      x_range: [-3, 3],
      y_range: [-3, 3],
      grid_size: 14,
      title: "Rotational Field",
    },
  },
  {
    name: "plot_free_body_diagram",
    label: "自由物体図・力のベクトル図 (plot_free_body_diagram)",
    sampleArgs: {
      forces: [
        { label: "mg", fx: 0, fy: -9.8 },
        { label: "N", fx: 0, fy: 9.8 },
        { label: "T", fx: 4.0, fy: 0 },
        { label: "f", fx: -4.0, fy: 0 },
      ],
      body_label: "m",
      incline_deg: 0,
      show_components: false,
      title: "Free-Body Diagram",
    },
  },
  {
    name: "plot_circuit_diagram",
    label: "電気回路図 RLC (plot_circuit_diagram)",
    sampleArgs: {
      elements: [
        { kind: "voltage_source", label: "E", value: "12V" },
        { kind: "resistor", label: "R", value: "100Ω" },
        { kind: "inductor", label: "L", value: "10mH" },
        { kind: "capacitor", label: "C", value: "47μF" },
      ],
      topology: "series",
      title: "RLC Series Circuit",
    },
  },
  {
    name: "plot_field_lines",
    label: "電場・磁場の力線と等電位線 (plot_field_lines)",
    sampleArgs: {
      field_kind: "electric",
      sources: [
        { x: -1, y: 0, q: 1 },
        { x: 1, y: 0, q: -1 },
      ],
      show_equipotentials: true,
      title: "Electric Dipole Field",
    },
  },
  {
    name: "plot_wave",
    label: "波動・重ね合わせ・定常波 (plot_wave)",
    sampleArgs: {
      waves: [
        { amplitude: 1, wavenumber: 4, omega: 2, phase: 0, label: "y1" },
        { amplitude: 1, wavenumber: 4.5, omega: 2.1, phase: 0, label: "y2" },
      ],
      t: 0.5,
      show_superposition: true,
      show_envelope: true,
      title: "Wave Superposition & Beats",
    },
  },
  {
    name: "plot_potential_energy",
    label: "ポテンシャルエネルギー曲線・転回点 (plot_potential_energy)",
    sampleArgs: {
      expression: "x**2 / 2",
      variable: "x",
      x_range: [-4, 4],
      energy_levels: [2.0, 4.5],
      title: "Harmonic Potential Well",
    },
  },
  {
    name: "plot_trajectory",
    label: "運動の軌跡・速度ベクトル (plot_trajectory)",
    sampleArgs: {
      x_expression: "10 * t",
      y_expression: "10 * t - 4.9 * t**2",
      parameter: "t",
      t_range: [0, 2.04],
      show_velocity_vectors: true,
      num_vectors: 6,
      title: "Projectile Trajectory",
    },
  },
];

function renderVerificationStatusBadge(status: VerificationStatus) {
  switch (status) {
    case "passed":
      return (
        <span className="inline-flex items-center gap-1 rounded-full bg-emerald-100 px-2 py-0.5 text-[10px] font-semibold text-emerald-800 dark:bg-emerald-950 dark:text-emerald-200">
          <CheckCircle2 className="h-3 w-3" />
          合格 (passed)
        </span>
      );
    case "failed":
      return (
        <span className="inline-flex items-center gap-1 rounded-full bg-red-100 px-2 py-0.5 text-[10px] font-semibold text-red-800 dark:bg-red-950 dark:text-red-200">
          <XCircle className="h-3 w-3" />
          不一致・反例検出 (failed)
        </span>
      );
    case "inconclusive":
      return (
        <span className="inline-flex items-center gap-1 rounded-full bg-amber-100 px-2 py-0.5 text-[10px] font-semibold text-amber-900 dark:bg-amber-950 dark:text-amber-200">
          <HelpCircle className="h-3 w-3" />
          判定不能 (inconclusive)
        </span>
      );
    case "error":
      return (
        <span className="inline-flex items-center gap-1 rounded-full bg-orange-100 px-2 py-0.5 text-[10px] font-semibold text-orange-900 dark:bg-orange-950 dark:text-orange-200">
          <AlertTriangle className="h-3 w-3" />
          エラー (error)
        </span>
      );
    default:
      return (
        <span className="inline-flex items-center gap-1 rounded-full bg-slate-200 px-2 py-0.5 text-[10px] font-semibold text-slate-700 dark:bg-slate-800 dark:text-slate-300">
          <Clock className="h-3 w-3" />
          未実行 (not_run)
        </span>
      );
  }
}

function renderSourceKindBadge(sourceKind: VerificationSourceKind) {
  if (sourceKind === "science_mcp") {
    return (
      <span className="inline-flex items-center gap-1 rounded border border-blue-300 bg-blue-50 px-2 py-0.5 text-[10px] font-semibold text-blue-800 dark:border-blue-800 dark:bg-blue-950/60 dark:text-blue-200">
        <ShieldCheck className="h-3 w-3" />
        実ツール検証 (Science MCP)
      </span>
    );
  }
  if (sourceKind === "user_check") {
    return (
      <span className="inline-flex items-center gap-1 rounded border border-teal-300 bg-teal-50 px-2 py-0.5 text-[10px] font-semibold text-teal-800 dark:border-teal-800 dark:bg-teal-950/60 dark:text-teal-200">
        <UserCheck className="h-3 w-3" />
        ユーザー確認
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-1 rounded border border-slate-300 bg-slate-100 px-2 py-0.5 text-[10px] font-medium text-slate-700 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-300">
      <Bot className="h-3 w-3" />
      AI 自己チェック（ツール未検証）
    </span>
  );
}

function renderToolStatusBadge(status: ToolExecutionStatus) {
  switch (status) {
    case "succeeded":
      return <Badge variant="info">成功 (succeeded)</Badge>;
    case "timeout":
      return <Badge variant="warning">タイムアウト (timeout)</Badge>;
    case "failed":
      return <Badge variant="warning">失敗 (failed)</Badge>;
    case "cancelled":
      return <Badge variant="default">中断 (cancelled)</Badge>;
    default:
      return <Badge variant="outline">実行中 (running)</Badge>;
  }
}

export function CalculationsTab() {
  const { setActiveTab, openSettings } = useLayoutStore();
  const { selectedDocumentId } = useLibraryStore();
  const { solutions, selectedSolutionId, loadForDocument: reloadAiForDocument } =
    useAiWorkspaceStore();
  const {
    toolExecutions,
    verifications,
    artifacts,
    artifactPreviews,
    loading,
    runningTool,
    attachingArtifactId,
    error,
    loadForDocument,
    loadForSolution,
    runTool,
    attachArtifactToSolution,
    clearError,
  } = useScienceStore();

  const activeSolution =
    solutions.find((s) => s.id === selectedSolutionId) ??
    solutions.find((s) => s.isCurrent) ??
    solutions[0] ??
    null;

  const [selectedTool, setSelectedTool] = useState<string>(TOOL_PRESETS[0]!.name);
  const [argsText, setArgsText] = useState<string>(
    JSON.stringify(TOOL_PRESETS[0]!.sampleArgs, null, 2),
  );
  const [attachToPdf, setAttachToPdf] = useState<boolean>(true);
  const [localJsonError, setLocalJsonError] = useState<string | null>(null);
  const [expandedExecIds, setExpandedExecIds] = useState<Record<string, boolean>>({});

  useEffect(() => {
    void loadForDocument(selectedDocumentId);
  }, [selectedDocumentId, loadForDocument]);

  useEffect(() => {
    void loadForSolution(activeSolution?.id ?? null);
  }, [activeSolution?.id, loadForSolution]);

  const handleSelectTool = (toolName: string) => {
    setSelectedTool(toolName);
    setLocalJsonError(null);
    const preset = TOOL_PRESETS.find((p) => p.name === toolName);
    if (preset) {
      setArgsText(JSON.stringify(preset.sampleArgs, null, 2));
    }
  };

  const handleRunDirectTool = async () => {
    if (!selectedDocumentId) return;
    let parsedArgs: Record<string, unknown>;
    try {
      const v: unknown = JSON.parse(argsText);
      if (typeof v !== "object" || v === null || Array.isArray(v)) {
        setLocalJsonError("引数は JSON オブジェクト形式で入力してください。");
        return;
      }
      parsedArgs = v as Record<string, unknown>;
      setLocalJsonError(null);
    } catch (err) {
      setLocalJsonError(
        `引数 JSON 構文エラー: ${err instanceof Error ? err.message : String(err)}`,
      );
      return;
    }

    const res = await runTool({
      documentId: selectedDocumentId,
      solutionId: activeSolution?.id ?? null,
      toolName: selectedTool,
      arguments: parsedArgs,
      attachToSolutionPdf: attachToPdf && activeSolution !== null,
    });
    if (res?.updatedSolution) {
      await reloadAiForDocument(selectedDocumentId);
    }
  };

  const handleAttachToPdf = async (attachmentId: string, caption: string | null) => {
    if (!activeSolution || !selectedDocumentId) return;
    const newSolId = await attachArtifactToSolution(activeSolution.id, attachmentId, caption);
    if (newSolId) {
      await reloadAiForDocument(selectedDocumentId);
      setActiveTab("solutionPdf");
    }
  };

  const toggleExpandExec = (id: string) => {
    setExpandedExecIds((prev) => ({ ...prev, [id]: !prev[id] }));
  };

  if (!selectedDocumentId) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-xs text-slate-500">
        左サイドバーから問題ドキュメントを選択すると、Science MCP の計算結果・検証記録・グラフを確認できます。
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Header Toolbar */}
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-slate-200 bg-white px-3 py-2 text-xs dark:border-slate-800 dark:bg-slate-900">
        <div className="flex items-center gap-2">
          <Calculator className="h-4 w-4 text-blue-600 dark:text-blue-400" />
          <span className="font-semibold text-slate-800 dark:text-slate-100">
            Science MCP 計算・検証・グラフ
          </span>
          {activeSolution && (
            <Badge variant="info">対象解答: v{activeSolution.versionNumber}</Badge>
          )}
        </div>
        <div className="flex items-center gap-1.5">
          <Button
            size="xs"
            variant="outline"
            disabled={loading}
            onClick={() => {
              void loadForDocument(selectedDocumentId);
              if (activeSolution) void loadForSolution(activeSolution.id);
            }}
          >
            <RefreshCw className={`h-3.5 w-3.5 ${loading ? "animate-spin" : ""}`} />
            <span>更新</span>
          </Button>
          <Button size="xs" variant="ghost" onClick={() => openSettings("scienceMcp")}>
            計算ツール設定
          </Button>
        </div>
      </div>

      {/* Error Banner */}
      {error && (
        <div
          role="alert"
          className="flex items-start justify-between gap-2 border-b border-red-200 bg-red-50 px-3 py-2 text-xs text-red-800 dark:border-red-900 dark:bg-red-950/60 dark:text-red-200"
        >
          <div>
            <strong>{error.code}:</strong> {error.message}
          </div>
          <Button size="xs" variant="ghost" onClick={clearError}>
            閉じる
          </Button>
        </div>
      )}

      {/* Main Scroll Area */}
      <div className="flex-1 overflow-y-auto p-4 space-y-5 text-xs">
        {/* 1. Verification Results Section */}
        <section aria-label="検証結果一覧" className="space-y-2.5">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <h3 className="text-xs font-bold text-slate-800 dark:text-slate-100">
              検証結果一覧 ({verifications.length})
            </h3>
            <span className="text-[10px] text-slate-500 dark:text-slate-400">
              実ツール検証 (Science MCP) と AI 自己チェックを厳格分離
            </span>
          </div>

          <div className="rounded-lg border border-blue-200/80 bg-blue-50/50 px-3 py-2 text-[11px] text-blue-900 dark:border-blue-900 dark:bg-blue-950/30 dark:text-blue-200">
            計算ツールによる <code>passed</code> は指定された式変形・数値・次元等の局所的整合性を示すものであり、
            <strong>数学的証明全体や物理モデル・近似の妥当性を保証するものではありません。</strong>
          </div>

          {verifications.length === 0 ? (
            <div className="rounded-lg border border-dashed border-slate-300 bg-white p-4 text-center text-slate-500 dark:border-slate-700 dark:bg-slate-900">
              {activeSolution
                ? "この解答バージョンにはまだ検証記録がありません。「verified」モードで解答生成するか、下の計算ツールから検算を実行してください。"
                : "解答を生成すると、計算ツール検証結果と AI 自己チェック結果がここに分類表示されます。"}
            </div>
          ) : (
            <div className="space-y-2">
              {verifications.map((v) => (
                <div
                  key={v.id}
                  className={`rounded-lg border p-3 ${
                    v.sourceKind === "science_mcp"
                      ? "border-blue-200 bg-white dark:border-blue-900/70 dark:bg-slate-900"
                      : "border-slate-200 bg-slate-50/70 dark:border-slate-800 dark:bg-slate-900/60"
                  }`}
                >
                  <div className="flex flex-wrap items-center gap-1.5">
                    {renderSourceKindBadge(v.sourceKind)}
                    {renderVerificationStatusBadge(v.status)}
                    <span className="font-semibold text-slate-900 dark:text-slate-100">
                      {v.targetDescription}
                    </span>
                  </div>
                  <div className="mt-1.5 grid gap-1 text-[11px] text-slate-600 dark:text-slate-300">
                    <div>
                      <span className="font-medium text-slate-500">手法: </span>
                      <code>{v.method}</code>
                    </div>
                    {v.assumptions.length > 0 && (
                      <div>
                        <span className="font-medium text-slate-500">仮定・条件: </span>
                        {v.assumptions.join(" / ")}
                      </div>
                    )}
                    {v.detailJson && (
                      <pre className="mt-1 overflow-x-auto rounded bg-slate-100 p-1.5 font-mono text-[10px] dark:bg-slate-950">
                        {v.detailJson}
                      </pre>
                    )}
                  </div>
                </div>
              ))}
            </div>
          )}
        </section>

        {/* 2. Generated Plots & Figures Section */}
        <section aria-label="生成グラフ一覧" className="space-y-2.5">
          <div className="flex items-center justify-between">
            <h3 className="flex items-center gap-1.5 text-xs font-bold text-slate-800 dark:text-slate-100">
              <ImageIcon className="h-3.5 w-3.5 text-blue-600" />
              <span>生成グラフ・図 ({artifacts.length})</span>
            </h3>
          </div>

          {artifacts.length === 0 ? (
            <div className="rounded-lg border border-dashed border-slate-300 bg-white p-4 text-center text-slate-500 dark:border-slate-700 dark:bg-slate-900">
              まだ生成されたグラフはありません。<code>plot_function</code> /{" "}
              <code>plot_vector_field</code> 等で SVG/PNG グラフを生成し解答 PDF へ埋め込めます。
            </div>
          ) : (
            <div className="grid gap-3 md:grid-cols-2">
              {artifacts.map((art) => {
                const preview = artifactPreviews[art.attachmentId];
                const dataSrc = preview
                  ? `data:${art.mimeType};base64,${preview.base64Data}`
                  : null;
                return (
                  <div
                    key={art.id}
                    className="rounded-lg border border-slate-200 bg-white p-3 space-y-2 dark:border-slate-800 dark:bg-slate-900"
                  >
                    <div className="flex items-center justify-between gap-2">
                      <span className="truncate font-semibold text-slate-800 dark:text-slate-100">
                        {art.caption ?? art.originalFilename}
                      </span>
                      <Badge variant="outline">
                        {art.mimeType === "image/svg+xml" ? "SVG ベクター" : "PNG"}
                      </Badge>
                    </div>

                    <div className="flex min-h-[140px] items-center justify-center overflow-hidden rounded border border-slate-100 bg-white p-2 dark:border-slate-800">
                      {dataSrc ? (
                        <img
                          src={dataSrc}
                          alt={art.caption ?? art.originalFilename}
                          className="max-h-56 w-auto object-contain"
                        />
                      ) : (
                        <span className="text-[11px] text-slate-400">プレビュー読込中...</span>
                      )}
                    </div>

                    <div className="flex items-center justify-between gap-2 text-[10px] text-slate-500">
                      <span className="truncate font-mono">{art.relativePath}</span>
                      {activeSolution && (
                        <Button
                          size="xs"
                          variant="outline"
                          disabled={attachingArtifactId === art.attachmentId}
                          onClick={() => void handleAttachToPdf(art.attachmentId, art.caption)}
                        >
                          <FilePlus className="h-3 w-3" />
                          <span>解答 PDF に埋め込む</span>
                        </Button>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </section>

        {/* 3. Direct Science MCP Calculation & Plot Runner */}
        <section
          aria-label="Science MCP 直接実行"
          className="rounded-xl border border-slate-200 bg-white p-3.5 space-y-2.5 dark:border-slate-800 dark:bg-slate-900"
        >
          <div className="flex flex-wrap items-center justify-between gap-2">
            <h3 className="font-bold text-slate-800 dark:text-slate-100">
              計算ツールを直接実行（ローカル Science MCP）
            </h3>
            <span className="text-[10px] text-slate-500">
              AST ホワイトリスト検証・隔離ワーカー実行・UUID グラフ出力
            </span>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <select
              aria-label="実行する Science MCP ツール"
              value={selectedTool}
              onChange={(e) => handleSelectTool(e.target.value)}
              className="h-8 flex-1 min-w-[220px] rounded-md border border-slate-300 bg-white px-2 text-xs dark:border-slate-700 dark:bg-slate-950"
            >
              {TOOL_PRESETS.map((p) => (
                <option key={p.name} value={p.name}>
                  {p.label}
                </option>
              ))}
            </select>

            <label className="inline-flex items-center gap-1.5 text-[11px] text-slate-700 dark:text-slate-300">
              <input
                type="checkbox"
                checked={attachToPdf}
                disabled={!activeSolution}
                onChange={(e) => setAttachToPdf(e.target.checked)}
              />
              <span>結果・グラフを解答 PDF へ反映</span>
            </label>

            <Button
              size="xs"
              variant="primary"
              disabled={runningTool}
              onClick={() => void handleRunDirectTool()}
            >
              {runningTool ? (
                <RefreshCw className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Play className="h-3.5 w-3.5" />
              )}
              <span>{runningTool ? "計算実行中..." : "計算を実行"}</span>
            </Button>
          </div>

          {localJsonError && (
            <div className="rounded bg-red-50 px-2 py-1 text-[11px] text-red-700 dark:bg-red-950/60 dark:text-red-300">
              {localJsonError}
            </div>
          )}

          <textarea
            aria-label="ツール引数 JSON"
            rows={5}
            value={argsText}
            onChange={(e) => setArgsText(e.target.value)}
            className="w-full rounded-md border border-slate-300 bg-slate-50 p-2 font-mono text-[11px] leading-relaxed dark:border-slate-700 dark:bg-slate-950"
          />
        </section>

        {/* 4. Tool Execution History */}
        <section aria-label="ツール実行履歴" className="space-y-2">
          <h3 className="text-xs font-bold text-slate-800 dark:text-slate-100">
            計算ツール実行履歴 ({toolExecutions.length})
          </h3>
          {toolExecutions.length === 0 ? (
            <div className="rounded-lg border border-dashed border-slate-300 bg-white p-4 text-center text-slate-500 dark:border-slate-700 dark:bg-slate-900">
              このドキュメントでのツール実行履歴はまだありません。
            </div>
          ) : (
            <div className="space-y-1.5">
              {toolExecutions.map((ex) => {
                const expanded = Boolean(expandedExecIds[ex.id]);
                return (
                  <div
                    key={ex.id}
                    className="rounded-lg border border-slate-200 bg-white dark:border-slate-800 dark:bg-slate-900"
                  >
                    <button
                      type="button"
                      onClick={() => toggleExpandExec(ex.id)}
                      className="flex w-full items-center justify-between gap-2 px-3 py-2 text-left hover:bg-slate-50 dark:hover:bg-slate-800/50"
                    >
                      <div className="flex items-center gap-2 min-w-0">
                        {expanded ? (
                          <ChevronDown className="h-3.5 w-3.5 shrink-0 text-slate-400" />
                        ) : (
                          <ChevronRight className="h-3.5 w-3.5 shrink-0 text-slate-400" />
                        )}
                        <code className="font-semibold text-blue-700 dark:text-blue-300">
                          {ex.toolName}
                        </code>
                        <span className="text-[10px] text-slate-400">({ex.serverName})</span>
                        {renderToolStatusBadge(ex.status)}
                      </div>
                      <div className="flex shrink-0 items-center gap-2 text-[10px] text-slate-500">
                        {ex.durationMs !== null && <span>{ex.durationMs} ms</span>}
                      </div>
                    </button>
                    {expanded && (
                      <div className="space-y-2 border-t border-slate-100 px-3 py-2 text-[11px] dark:border-slate-800">
                        <div>
                          <div className="font-semibold text-slate-500">入力引数</div>
                          <pre className="mt-0.5 max-h-40 overflow-auto rounded bg-slate-100 p-2 font-mono text-[10px] dark:bg-slate-950">
                            {formatPrettyJson(ex.argumentsJson)}
                          </pre>
                        </div>
                        {ex.resultJson && (
                          <div>
                            <div className="font-semibold text-slate-500">出力結果</div>
                            <pre className="mt-0.5 max-h-52 overflow-auto rounded bg-slate-100 p-2 font-mono text-[10px] dark:bg-slate-950">
                              {formatPrettyJson(ex.resultJson)}
                            </pre>
                          </div>
                        )}
                        {ex.errorMessage && (
                          <div className="rounded bg-red-50 p-2 text-red-700 dark:bg-red-950/60 dark:text-red-200">
                            エラー: {ex.errorMessage}
                          </div>
                        )}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </section>
      </div>
    </div>
  );
}

function formatPrettyJson(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}
