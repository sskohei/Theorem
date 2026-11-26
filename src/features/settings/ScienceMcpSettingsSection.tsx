import { useState } from "react";
import { CheckCircle2, RefreshCw, ShieldCheck, XCircle } from "lucide-react";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { useLibraryStore } from "@/stores/useLibraryStore";
import { useScienceStore } from "@/stores/useScienceStore";
import type { ScienceMcpSettings } from "@/types/ipc";

export function ScienceMcpSettingsPanel() {
  const { settings, saveSettings } = useLibraryStore();
  const { status, checkingStatus, checkStatus } = useScienceStore();
  const [savedNotice, setSavedNotice] = useState<string | null>(null);

  const cfg: ScienceMcpSettings = settings.scienceMcp ?? {
    enabled: true,
    uvPath: null,
    serverDir: null,
    timeoutSeconds: 10,
    defaultPlotFormat: "svg",
  };

  const updateScienceConfig = async (patch: Partial<ScienceMcpSettings>) => {
    const next: ScienceMcpSettings = { ...cfg, ...patch };
    await saveSettings({ ...settings, scienceMcp: next });
    setSavedNotice("計算ツール設定を保存しました。");
  };

  return (
    <div className="space-y-4 text-xs">
      <div className="rounded-lg border border-blue-200 bg-blue-50/60 p-3 dark:border-blue-900 dark:bg-blue-950/40">
        <div className="flex items-center gap-1.5 font-semibold text-blue-900 dark:text-blue-100">
          <ShieldCheck className="h-4 w-4 text-blue-600 dark:text-blue-400" />
          <span>Science MCP Server（ローカル Python / SymPy / Pint / Matplotlib）</span>
        </div>
        <p className="mt-1 leading-relaxed text-blue-800 dark:text-blue-200">
          汎用 Python コード実行ツールは一切提供せず、AST ホワイトリスト数式パーサー・独立ワーカープロセスによるハードタイムアウト・
          固定 <code>artifacts/</code> UUID 出力で保護されています。ユーザーの既存グローバル MCP 設定ファイル（<code>~/.codex/config.toml</code> 等）は変更しません。
        </p>
      </div>

      {savedNotice && (
        <div className="rounded-md border border-emerald-200 bg-emerald-50 px-3 py-2 text-emerald-900 dark:border-emerald-900 dark:bg-emerald-950/50 dark:text-emerald-100">
          {savedNotice}
        </div>
      )}

      {/* Enable / Disable Toggle */}
      <div className="space-y-2">
        <label className="flex items-center gap-2 font-medium text-slate-800 dark:text-slate-100">
          <input
            type="checkbox"
            checked={cfg.enabled}
            onChange={(e) => void updateScienceConfig({ enabled: e.target.checked })}
          />
          <span>Science MCP Server を有効にする（AI セッションへ自動スコープ登録）</span>
        </label>
      </div>

      {/* uv Path & Timeout & Plot Format */}
      <div className="grid gap-3 sm:grid-cols-2">
        <div>
          <label className="mb-1 block font-medium text-slate-700 dark:text-slate-200">
            Python (`uv`) 実行パス（空欄で自動検出）
          </label>
          <Input
            aria-label="uv 実行パス"
            value={cfg.uvPath ?? ""}
            placeholder="例: C:\\Users\\...\\uv.exe（未指定で自動検出）"
            onChange={(e) =>
              void updateScienceConfig({
                uvPath: e.target.value.trim() ? e.target.value.trim() : null,
              })
            }
          />
        </div>

        <div>
          <label className="mb-1 block font-medium text-slate-700 dark:text-slate-200">
            science-mcp ディレクトリ（空欄で自動検出）
          </label>
          <Input
            aria-label="science-mcp ディレクトリ"
            value={cfg.serverDir ?? ""}
            placeholder="未指定でリポジトリ/アプリ標準位置を自動解決"
            onChange={(e) =>
              void updateScienceConfig({
                serverDir: e.target.value.trim() ? e.target.value.trim() : null,
              })
            }
          />
        </div>

        <div>
          <label className="mb-1 block font-medium text-slate-700 dark:text-slate-200">
            実行タイムアウト秒数（2〜60 秒）
          </label>
          <Input
            aria-label="Science MCP 実行タイムアウト秒数"
            type="number"
            min={2}
            max={60}
            value={cfg.timeoutSeconds}
            onChange={(e) => {
              const n = Number.parseInt(e.target.value, 10);
              if (!Number.isNaN(n)) {
                void updateScienceConfig({ timeoutSeconds: Math.min(60, Math.max(2, n)) });
              }
            }}
          />
        </div>

        <div>
          <label className="mb-1 block font-medium text-slate-700 dark:text-slate-200">
            グラフ出力形式
          </label>
          <select
            aria-label="グラフ出力形式"
            value={cfg.defaultPlotFormat}
            onChange={(e) =>
              void updateScienceConfig({
                defaultPlotFormat: e.target.value === "png" ? "png" : "svg",
              })
            }
            className="h-8 w-full rounded-md border border-slate-300 bg-white px-2.5 text-xs dark:border-slate-700 dark:bg-slate-800"
          >
            <option value="svg">SVG 優先（ベクター・PDF 高精細推奨）</option>
            <option value="png">PNG（300 DPI 高解像度ラスター）</option>
          </select>
        </div>
      </div>

      {/* Live Probe */}
      <div className="space-y-2.5 border-t border-slate-200 pt-3 dark:border-slate-800">
        <div className="flex items-center justify-between">
          <span className="font-semibold text-slate-800 dark:text-slate-100">
            ローカル接続・ツール疎通確認
          </span>
          <Button
            size="xs"
            variant="primary"
            disabled={checkingStatus}
            onClick={() => void checkStatus()}
          >
            <RefreshCw className={`h-3.5 w-3.5 ${checkingStatus ? "animate-spin" : ""}`} />
            <span>{checkingStatus ? "プローブ中..." : "Science MCP の状態を確認"}</span>
          </Button>
        </div>

        {status && (
          <div className="rounded-lg border border-slate-200 bg-slate-50 p-3 space-y-2 dark:border-slate-800 dark:bg-slate-950/60">
            <div className="flex flex-wrap items-center gap-2">
              {status.serverReady ? (
                <span className="inline-flex items-center gap-1 font-semibold text-emerald-700 dark:text-emerald-300">
                  <CheckCircle2 className="h-4 w-4" />
                  稼働可能（全 {status.toolCount} ツール検出）
                </span>
              ) : (
                <span className="inline-flex items-center gap-1 font-semibold text-red-700 dark:text-red-300">
                  <XCircle className="h-4 w-4" />
                  未接続 / 利用不可
                </span>
              )}
              {status.uvVersion && <Badge variant="outline">{status.uvVersion}</Badge>}
            </div>

            {status.serverDir && (
              <div className="text-[11px] text-slate-600 dark:text-slate-400">
                パッケージ: <code className="break-all">{status.serverDir}</code>
              </div>
            )}
            {status.detailNote && (
              <div className="text-[11px] text-amber-800 dark:text-amber-300">
                {status.detailNote}
              </div>
            )}

            {status.tools.length > 0 && (
              <div className="pt-1">
                <div className="mb-1 text-[11px] font-medium text-slate-600 dark:text-slate-300">
                  利用可能な計算ツール一覧:
                </div>
                <div className="flex flex-wrap gap-1">
                  {status.tools.map((tool) => (
                    <Badge key={tool.name} variant="info" className="font-mono text-[10px]">
                      {tool.name}
                    </Badge>
                  ))}
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
