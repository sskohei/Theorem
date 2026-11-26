import { useEffect, useState } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  Cloud,
  CloudOff,
  ExternalLink,
  KeyRound,
  RefreshCw,
  RotateCcw,
  ShieldCheck,
  X,
  XCircle,
} from "lucide-react";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { formatShortDate } from "@/lib/utils";
import { useDriveStore } from "@/stores/useDriveStore";
import type { DriveJobStatus, DriveUploadMode, DriveVersionPolicy } from "@/types/ipc";

function renderJobStatusBadge(status: DriveJobStatus) {
  switch (status) {
    case "uploaded":
      return <Badge variant="info">アップロード済</Badge>;
    case "uploading":
      return <Badge variant="info">送信中...</Badge>;
    case "pending":
      return <Badge variant="outline">待機中</Badge>;
    case "update_required":
      return <Badge variant="warning">更新待ち</Badge>;
    case "conflict":
      return <Badge variant="warning">競合・確認待ち</Badge>;
    case "failed":
      return <Badge variant="warning">失敗</Badge>;
    case "cancelled":
      return <Badge variant="default">キャンセル済</Badge>;
  }
}

export function DriveSettingsPanel() {
  const {
    overview,
    loading,
    connecting,
    processingQueue,
    lastNotice,
    error,
    loadOverview,
    saveOAuthConfig,
    connect,
    disconnect,
    updateSettings,
    retryJob,
    cancelJob,
    openConflictModal,
    processQueueNow,
    clearError,
    clearNotice,
  } = useDriveStore();

  const [clientIdInput, setClientIdInput] = useState("");
  const [clientSecretInput, setClientSecretInput] = useState("");
  const [rootFolderInput, setRootFolderInput] = useState("Theorem");

  useEffect(() => {
    void loadOverview();
  }, [loadOverview]);

  useEffect(() => {
    if (overview?.oauthConfig.clientId !== undefined) {
      setClientIdInput(overview.oauthConfig.clientId ?? "");
    }
    if (overview?.connection?.rootFolderName) {
      setRootFolderInput(overview.connection.rootFolderName);
    }
  }, [overview?.oauthConfig.clientId, overview?.connection?.rootFolderName]);

  const conn = overview?.connection ?? null;
  const oauth = overview?.oauthConfig ?? null;
  const jobs = overview?.jobs ?? [];

  const handleSaveOAuth = async () => {
    await saveOAuthConfig({
      clientId: clientIdInput.trim() || null,
      clientSecret: clientSecretInput.trim() ? clientSecretInput.trim() : null,
      clearClientSecret: false,
    });
    setClientSecretInput("");
  };

  return (
    <div className="space-y-4 text-xs">
      {/* Scope & Privacy Guarantee Banner */}
      <div className="rounded-lg border border-blue-200 bg-blue-50/70 p-3 dark:border-blue-900 dark:bg-blue-950/40">
        <div className="flex items-center gap-1.5 font-semibold text-blue-900 dark:text-blue-100">
          <ShieldCheck className="h-4 w-4 text-blue-600 dark:text-blue-400" />
          <span>Google Drive エクスポート（最小権限 drive.file・AI 完全分離）</span>
        </div>
        <ul className="mt-1.5 list-disc space-y-1 pl-4 leading-relaxed text-blue-800 dark:text-blue-200">
          <li>
            要求スコープは <code>https://www.googleapis.com/auth/drive.file</code>{" "}
            のみです（Google Drive 全体へのアクセス権限は要求しません）。
          </li>
          <li>
            アップロード対象は問題ファイル・解答 PDF（および設定した任意ファイル）のみです。
            <strong>SQLite DB・AI 会話履歴・認証情報・ログは一切送信されません</strong>。
          </li>
          <li>
            ローカルで問題や解答を削除しても、Google Drive 上のファイルは自動削除されません。
            また AI エージェントへ Google Drive の操作権限やトークンは一切渡されません。
          </li>
        </ul>
      </div>

      {error && (
        <div
          role="alert"
          className="flex items-start justify-between gap-2 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-red-800 dark:border-red-900 dark:bg-red-950/50 dark:text-red-200"
        >
          <div className="min-w-0 flex-1">
            <div className="font-semibold">{error.message}</div>
            {error.detail && <div className="mt-0.5 text-[11px] opacity-90">{error.detail}</div>}
          </div>
          <button type="button" onClick={clearError} aria-label="エラーを閉じる">
            <X className="h-3.5 w-3.5" />
          </button>
        </div>
      )}

      {lastNotice && (
        <div
          role="status"
          className="flex items-center justify-between gap-2 rounded-md border border-emerald-200 bg-emerald-50 px-3 py-2 text-emerald-900 dark:border-emerald-900 dark:bg-emerald-950/50 dark:text-emerald-100"
        >
          <span>{lastNotice}</span>
          <button type="button" onClick={clearNotice} aria-label="通知を閉じる">
            <X className="h-3.5 w-3.5" />
          </button>
        </div>
      )}

      {/* Connection Status Card */}
      <div className="rounded-lg border border-slate-200 bg-slate-50/70 p-3.5 space-y-3 dark:border-slate-800 dark:bg-slate-950/60">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="space-y-0.5">
            <div className="flex items-center gap-2 font-semibold text-slate-900 dark:text-slate-100">
              {conn?.status === "connected" ? (
                <>
                  <CheckCircle2 className="h-4 w-4 text-emerald-600 dark:text-emerald-400" />
                  <span>Google Drive 接続済み</span>
                </>
              ) : conn?.status === "reauth_required" ? (
                <>
                  <AlertTriangle className="h-4 w-4 text-amber-600 dark:text-amber-400" />
                  <span>再認証が必要です（トークン期限切れ / 失効）</span>
                </>
              ) : (
                <>
                  <CloudOff className="h-4 w-4 text-slate-400" />
                  <span>Google Drive 未接続（任意接続）</span>
                </>
              )}
            </div>
            {conn && conn.status !== "disconnected" && (
              <div className="text-[11px] text-slate-600 dark:text-slate-400">
                アカウント:{" "}
                <span className="font-medium text-slate-800 dark:text-slate-200">
                  {conn.accountDisplayName
                    ? `${conn.accountDisplayName} (${conn.accountEmail})`
                    : conn.accountEmail}
                </span>
                {" · "}スコープ: <code>{conn.scopes.join(", ")}</code>
              </div>
            )}
          </div>

          <div className="flex items-center gap-1.5">
            {conn?.status === "connected" ? (
              <Button size="xs" variant="outline" onClick={() => void disconnect()}>
                接続を解除
              </Button>
            ) : (
              <Button
                size="xs"
                variant="primary"
                disabled={connecting}
                onClick={() => void connect()}
              >
                <Cloud className="h-3.5 w-3.5" />
                <span>
                  {connecting
                    ? "ブラウザで認証待機中..."
                    : conn?.status === "reauth_required"
                      ? "Google Drive を再認証"
                      : "Google Drive に接続"}
                </span>
              </Button>
            )}
          </div>
        </div>

        <p className="text-[11px] text-slate-500 dark:text-slate-400">
          認証は OS 既定のシステムブラウザ（Authorization Code + PKCE S256 + ローカルループバック）で行い、
          アクセストークン・リフレッシュトークンは OS キーチェーン（Windows Credential Manager 等）へ保存されます。
          Google Drive 未接続でもローカル機能・AI 機能・計算ツールはすべて利用できます。
        </p>
      </div>

      {/* Desktop OAuth Client Configuration */}
      <div className="rounded-lg border border-slate-200 p-3.5 space-y-2.5 dark:border-slate-800">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-1.5 font-semibold text-slate-800 dark:text-slate-100">
            <KeyRound className="h-3.5 w-3.5 text-slate-500" />
            <span>Desktop OAuth クライアント設定（Google Cloud）</span>
          </div>
          {oauth?.clientIdSource === "env" && (
            <Badge variant="outline">環境変数から検出済</Badge>
          )}
        </div>
        <p className="text-[11px] text-slate-500 dark:text-slate-400">
          ご自身の Google Cloud プロジェクトで発行した「デスクトップ アプリ」用 OAuth クライアント ID
          を設定してください（環境変数 <code>GOOGLE_OAUTH_CLIENT_ID</code> での指定にも対応）。
        </p>
        <div className="grid gap-2.5 sm:grid-cols-2">
          <div>
            <label className="mb-1 block font-medium text-slate-700 dark:text-slate-200">
              OAuth クライアント ID
            </label>
            <Input
              aria-label="Google OAuth クライアント ID"
              value={clientIdInput}
              placeholder="xxxx.apps.googleusercontent.com"
              onChange={(e) => setClientIdInput(e.target.value)}
            />
          </div>
          <div>
            <label className="mb-1 block font-medium text-slate-700 dark:text-slate-200">
              クライアントシークレット（任意・OS キーチェーン保管）
            </label>
            <Input
              aria-label="Google OAuth クライアントシークレット"
              type="password"
              value={clientSecretInput}
              placeholder={
                oauth?.hasClientSecret ? "設定済み（変更時のみ入力）" : "未設定（必要時のみ入力）"
              }
              onChange={(e) => setClientSecretInput(e.target.value)}
            />
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Button size="xs" variant="outline" onClick={() => void handleSaveOAuth()}>
            OAuth 設定を保存
          </Button>
          {oauth?.hasClientSecret && (
            <Button
              size="xs"
              variant="ghost"
              onClick={() => void saveOAuthConfig({ clearClientSecret: true })}
            >
              キーチェーンのシークレットを削除
            </Button>
          )}
        </div>
      </div>

      {/* Sync Policy & Export Options (active when a connection record exists) */}
      {conn && (
        <div className="rounded-lg border border-slate-200 p-3.5 space-y-3 dark:border-slate-800">
          <div className="font-semibold text-slate-800 dark:text-slate-100">
            保存先フォルダ・アップロード動作・バージョン管理ポリシー
          </div>

          <div className="grid gap-3 sm:grid-cols-2">
            <div>
              <label className="mb-1 block font-medium text-slate-700 dark:text-slate-200">
                My Drive 配下の保存先ルートフォルダ名
              </label>
              <div className="flex gap-1.5">
                <Input
                  aria-label="保存先ルートフォルダ名"
                  value={rootFolderInput}
                  onChange={(e) => setRootFolderInput(e.target.value)}
                />
                <Button
                  size="xs"
                  variant="outline"
                  onClick={() =>
                    void updateSettings({ rootFolderName: rootFolderInput.trim() || "Theorem" })
                  }
                >
                  変更
                </Button>
              </div>
              <p className="mt-1 text-[10px] text-slate-500">
                ローカルの「科目 / フォルダ / 問題名」階層をそのまま反映します。
              </p>
            </div>

            <div>
              <label className="mb-1 block font-medium text-slate-700 dark:text-slate-200">
                アップロードモード
              </label>
              <select
                aria-label="Google Drive アップロードモード"
                value={conn.uploadMode}
                onChange={(e) =>
                  void updateSettings({ uploadMode: e.target.value as DriveUploadMode })
                }
                className="h-8 w-full rounded-md border border-slate-300 bg-white px-2.5 text-xs dark:border-slate-700 dark:bg-slate-800"
              >
                <option value="manual">手動アップロード（デフォルト・明示操作時のみ）</option>
                <option value="auto">自動アップロード（取り込み・PDF 生成後に自動投入）</option>
                <option value="folder_auto">
                  フォルダ単位自動アップロード（指定フォルダ配下のみ）
                </option>
              </select>
            </div>

            <div className="sm:col-span-2">
              <label className="mb-1 block font-medium text-slate-700 dark:text-slate-200">
                Drive 側バージョン管理ポリシー
              </label>
              <select
                aria-label="Drive バージョン管理ポリシー"
                value={conn.versionPolicy}
                onChange={(e) =>
                  void updateSettings({ versionPolicy: e.target.value as DriveVersionPolicy })
                }
                className="h-8 w-full rounded-md border border-slate-300 bg-white px-2.5 text-xs dark:border-slate-700 dark:bg-slate-800"
              >
                <option value="update_latest">
                  最新版のファイルを同一 ID で更新（デフォルト・共有リンクを維持）
                </option>
                <option value="create_new">
                  新しいファイルとして保存（解答_v2.pdf 等で別ファイル作成）
                </option>
                <option value="manual_version">
                  指定した解答バージョンをバージョン履歴画面から手動アップロード
                </option>
              </select>
            </div>
          </div>

          <div className="space-y-1.5 border-t border-slate-200 pt-2.5 dark:border-slate-800">
            <div className="font-medium text-slate-700 dark:text-slate-200">
              任意アップロード対象（デフォルトは「元問題ファイル」と「最新解答 PDF」）
            </div>
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={conn.includePastVersions}
                onChange={(e) => void updateSettings({ includePastVersions: e.target.checked })}
              />
              <span>過去バージョンの解答 PDF も含める</span>
            </label>
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={conn.includeSolutionSource}
                onChange={(e) => void updateSettings({ includeSolutionSource: e.target.checked })}
              />
              <span>解答ソースファイル（.typ）も含める</span>
            </label>
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={conn.includeArtifacts}
                onChange={(e) => void updateSettings({ includeArtifacts: e.target.checked })}
              />
              <span>生成した図・グラフ（SVG / PNG）単体ファイルも含める</span>
            </label>
          </div>
        </div>
      )}

      {/* Persistent Upload Job Queue */}
      <div className="space-y-2 border-t border-slate-200 pt-3 dark:border-slate-800">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="font-semibold text-slate-800 dark:text-slate-100">
            永続アップロードジョブキュー（最新 {jobs.length} 件）
          </div>
          <div className="flex items-center gap-1.5">
            <Button
              size="xs"
              variant="outline"
              disabled={loading}
              onClick={() => void loadOverview()}
            >
              <RefreshCw className={`h-3 w-3 ${loading ? "animate-spin" : ""}`} />
              <span>再読込</span>
            </Button>
            {conn?.status === "connected" && (
              <Button
                size="xs"
                variant="primary"
                disabled={processingQueue}
                onClick={() => void processQueueNow()}
              >
                <ExternalLink className="h-3 w-3" />
                <span>{processingQueue ? "処理中..." : "待機キューを今すぐ実行"}</span>
              </Button>
            )}
          </div>
        </div>

        {jobs.length === 0 ? (
          <p className="rounded-md border border-dashed border-slate-300 p-4 text-center text-slate-500 dark:border-slate-700">
            アップロードジョブはまだありません。左サイドバーまたはワークスペース上部から問題・解答をアップロードできます。
          </p>
        ) : (
          <ul className="space-y-1.5 max-h-60 overflow-y-auto pr-1">
            {jobs.map((job) => (
              <li
                key={job.id}
                className="rounded-md border border-slate-200 bg-white p-2.5 dark:border-slate-800 dark:bg-slate-900"
              >
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div className="flex flex-wrap items-center gap-1.5">
                    {renderJobStatusBadge(job.status)}
                    <span className="font-medium text-slate-900 dark:text-slate-100">
                      {job.targetFilename}
                    </span>
                    <span className="font-mono text-[10px] text-slate-500">
                      ({job.logicalRole})
                    </span>
                  </div>
                  <div className="flex items-center gap-1">
                    {job.status === "conflict" && (
                      <Button
                        size="xs"
                        variant="primary"
                        onClick={() => openConflictModal(job)}
                      >
                        競合を解決
                      </Button>
                    )}
                    {(job.status === "failed" || job.status === "cancelled") && (
                      <Button size="xs" variant="outline" onClick={() => void retryJob(job.id)}>
                        <RotateCcw className="h-3 w-3" />
                        <span>再試行</span>
                      </Button>
                    )}
                    {(job.status === "pending" || job.status === "conflict") && (
                      <Button size="xs" variant="ghost" onClick={() => void cancelJob(job.id)}>
                        <XCircle className="h-3 w-3" />
                        <span>中止</span>
                      </Button>
                    )}
                  </div>
                </div>
                <div className="mt-1 flex flex-wrap items-center gap-2 text-[10px] text-slate-500">
                  <span>更新: {formatShortDate(job.updatedAt)}</span>
                  <span>試行: {job.attemptCount} 回</span>
                  {job.nextRetryAt && <span>次回再試行: {formatShortDate(job.nextRetryAt)}</span>}
                </div>
                {job.lastErrorMessage && (
                  <div className="mt-1 rounded bg-amber-50 px-2 py-1 text-[11px] text-amber-900 dark:bg-amber-950/50 dark:text-amber-200">
                    {job.lastErrorCode ? `[${job.lastErrorCode}] ` : ""}
                    {job.lastErrorMessage}
                  </div>
                )}
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
