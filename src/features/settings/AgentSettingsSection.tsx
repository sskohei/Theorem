import { useEffect, useState } from "react";
import {
  CheckCircle2,
  KeyRound,
  Plus,
  RefreshCw,
  ShieldAlert,
  Trash2,
} from "lucide-react";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { useAgentStore } from "@/stores/useAgentStore";
import { useLibraryStore } from "@/stores/useLibraryStore";
import type { AuthMode } from "@/types/ipc";

export function AgentSettingsPanel() {
  const { settings, saveSettings } = useLibraryStore();
  const {
    profiles,
    statusesByProfileId,
    detecting,
    error,
    loadProfiles,
    detectAll,
    checkProfileStatus,
    saveProfile,
    deleteProfile,
  } = useAgentStore();

  const [manualPaths, setManualPaths] = useState<Record<string, string>>({});
  const [showAddCustom, setShowAddCustom] = useState(false);
  const [customName, setCustomName] = useState("");
  const [customExe, setCustomExe] = useState("");
  const [customArgs, setCustomArgs] = useState("");

  useEffect(() => {
    void loadProfiles();
  }, [loadProfiles]);

  const handleSaveManualPath = async (profileId: string) => {
    const p = profiles.find((x) => x.id === profileId);
    if (!p) return;
    await saveProfile({
      id: p.id,
      name: p.name,
      providerKind: p.providerKind,
      executablePath: manualPaths[profileId] ?? p.executablePath,
      args: p.args,
      defaultModel: p.defaultModel,
      authMode: p.authMode,
      isEnabled: p.isEnabled,
      isDefault: p.isDefault,
    });
    await checkProfileStatus(profileId);
  };

  const handleSetDefault = async (profileId: string) => {
    const p = profiles.find((x) => x.id === profileId);
    if (!p) return;
    await saveProfile({
      id: p.id,
      name: p.name,
      providerKind: p.providerKind,
      executablePath: p.executablePath,
      args: p.args,
      defaultModel: p.defaultModel,
      authMode: p.authMode,
      isEnabled: true,
      isDefault: true,
    });
  };

  const handleAddCustomAcp = async () => {
    if (!customName.trim() || !customExe.trim()) return;
    const args = customArgs
      .split(/\s+/)
      .map((s) => s.trim())
      .filter(Boolean);
    await saveProfile({
      name: customName.trim(),
      providerKind: "custom_acp",
      executablePath: customExe.trim(),
      args,
      defaultModel: null,
      authMode: "official_harness",
      isEnabled: true,
      isDefault: false,
    });
    setCustomName("");
    setCustomExe("");
    setCustomArgs("");
    setShowAddCustom(false);
  };

  const assignments = settings.agentAssignments;

  return (
    <div className="space-y-4 text-xs">
      {/* Privacy Notice for Cloud AI Transmission */}
      <div className="rounded-lg border border-amber-300 bg-amber-50/80 p-3 text-amber-950 dark:border-amber-800 dark:bg-amber-950/40 dark:text-amber-100">
        <div className="flex items-center gap-1.5 font-semibold">
          <ShieldAlert className="h-4 w-4 text-amber-600" />
          <span>外部 AI サービスへのデータ送信とプライバシー明示</span>
        </div>
        <p className="mt-1 leading-relaxed text-[11px]">
          Theorem はローカルファーストですが、クラウド AI ハーネス（Codex / Antigravity / Claude Code 等）を利用する場合、選択した問題画像・問題文・コンテキストが外部 AI サービスへ送信されます。SQLite DB や Google Drive 認証情報は一切送信されません。
        </p>
        <label className="mt-2 inline-flex items-center gap-1.5 font-medium">
          <input
            type="checkbox"
            checked={settings.cloudAiConsentAccepted}
            onChange={(e) =>
              void saveSettings({ ...settings, cloudAiConsentAccepted: e.target.checked })
            }
          />
          <span>外部 AI への問題画像・コンテキスト送信方針を確認しました</span>
        </label>
      </div>

      {error && (
        <div className="rounded border border-red-200 bg-red-50 p-2 text-red-700 dark:border-red-900 dark:bg-red-950/60 dark:text-red-200">
          {error.message}
        </div>
      )}

      {/* Auto-detect & Add Custom ACP */}
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="font-semibold text-slate-800 dark:text-slate-100">
          登録済み AI ハーネス ({profiles.length})
        </div>
        <div className="flex items-center gap-1.5">
          <Button
            size="xs"
            variant="outline"
            onClick={() => setShowAddCustom((v) => !v)}
          >
            <Plus className="h-3.5 w-3.5" />
            <span>カスタム ACP を追加</span>
          </Button>
          <Button
            size="xs"
            variant="primary"
            disabled={detecting}
            onClick={() => void detectAll()}
          >
            <RefreshCw className={`h-3.5 w-3.5 ${detecting ? "animate-spin" : ""}`} />
            <span>{detecting ? "検出中..." : "CLI を自動検出・状態確認"}</span>
          </Button>
        </div>
      </div>

      {showAddCustom && (
        <div className="space-y-2 rounded-lg border border-blue-200 bg-blue-50/40 p-3 dark:border-blue-900 dark:bg-blue-950/30">
          <div className="font-semibold">カスタム ACP エージェントを追加</div>
          <div className="grid gap-2 sm:grid-cols-3">
            <Input
              placeholder="表示名（例: Local ACP）"
              value={customName}
              onChange={(e) => setCustomName(e.target.value)}
            />
            <Input
              placeholder="実行ファイル絶対パス"
              value={customExe}
              onChange={(e) => setCustomExe(e.target.value)}
            />
            <Input
              placeholder="起動引数（例: acp --stdio --sandbox）"
              value={customArgs}
              onChange={(e) => setCustomArgs(e.target.value)}
            />
          </div>
          <div className="flex justify-end gap-1.5">
            <Button size="xs" variant="ghost" onClick={() => setShowAddCustom(false)}>
              キャンセル
            </Button>
            <Button size="xs" variant="primary" onClick={() => void handleAddCustomAcp()}>
              追加
            </Button>
          </div>
        </div>
      )}

      {/* Profile Status Cards */}
      <div className="space-y-2.5">
        {profiles.map((p) => {
          const st = statusesByProfileId[p.id];
          const pathVal = manualPaths[p.id] ?? p.executablePath ?? "";
          return (
            <div
              key={p.id}
              className="rounded-lg border border-slate-200 bg-white p-3 space-y-2 dark:border-slate-800 dark:bg-slate-900"
            >
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex flex-wrap items-center gap-1.5">
                  <span className="font-semibold text-slate-900 dark:text-slate-100">{p.name}</span>
                  <Badge variant="outline">{p.providerKind}</Badge>
                  {p.isDefault && <Badge variant="info">既定</Badge>}
                  {st ? (
                    <>
                      <Badge variant={st.installed ? "info" : "warning"}>
                        {st.installed ? `検出済 ${st.version ?? ""}` : "未検出"}
                      </Badge>
                      <Badge variant="outline">
                        認証:{" "}
                        {st.authStatus === "authenticated"
                          ? `確認済 (${st.authMethod ?? "公式"})`
                          : st.authStatus === "unauthenticated"
                            ? "未認証"
                            : st.authStatus === "terms_restricted"
                              ? "規約制限（OAuth無効）"
                              : "未取得"}
                      </Badge>
                      <Badge
                        variant={st.verificationStatus === "verified_live" ? "info" : "warning"}
                      >
                        {st.verificationStatus === "verified_live"
                          ? "実機確認済"
                          : st.verificationStatus === "verified_mock"
                            ? "モック検証用"
                            : "未検証"}
                      </Badge>
                    </>
                  ) : (
                    <Badge variant="default">未検出（ボタンで確認）</Badge>
                  )}
                </div>

                <div className="flex items-center gap-1">
                  {!p.isDefault && (
                    <Button
                      size="xs"
                      variant="outline"
                      onClick={() => void handleSetDefault(p.id)}
                    >
                      既定にする
                    </Button>
                  )}
                  <Button
                    size="xs"
                    variant="ghost"
                    disabled={detecting}
                    onClick={() => void checkProfileStatus(p.id)}
                  >
                    状態確認
                  </Button>
                  {p.providerKind === "custom_acp" && (
                    <Button
                      size="xs"
                      variant="ghost"
                      className="text-red-600"
                      onClick={() => void deleteProfile(p.id)}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </Button>
                  )}
                </div>
              </div>

              {st && (
                <div className="space-y-1 text-[11px] text-slate-600 dark:text-slate-400">
                  <div>
                    <span className="font-medium">公式認証ガイド: </span>
                    {st.authGuide}
                  </div>
                  <div>
                    <span className="font-medium">利用規約・安全制約: </span>
                    {st.complianceNote}
                  </div>
                  {st.detailNote && <div className="text-amber-700 dark:text-amber-300">{st.detailNote}</div>}
                </div>
              )}

              {p.providerKind !== "mock" && (
                <div className="flex items-center gap-1.5 pt-1">
                  <Input
                    value={pathVal}
                    placeholder="実行ファイルパス（空欄時は自動検出）"
                    onChange={(e) =>
                      setManualPaths((prev) => ({ ...prev, [p.id]: e.target.value }))
                    }
                    className="h-7 flex-1 font-mono text-[11px]"
                  />
                  <Button
                    size="xs"
                    variant="outline"
                    onClick={() => void handleSaveManualPath(p.id)}
                  >
                    パス保存
                  </Button>
                </div>
              )}
            </div>
          );
        })}
      </div>

      {/* Purpose-Based Agent Assignment (5 Purposes; Default Single Agent) */}
      <div className="space-y-2 border-t border-slate-200 pt-3 dark:border-slate-800">
        <div className="font-semibold text-slate-800 dark:text-slate-100">
          用途別エージェント割り当て（未指定時は既定エージェントを使用）
        </div>
        <div className="grid gap-2 sm:grid-cols-2">
          {(
            [
              ["extractionProfileId", "1. 問題文の認識（画像入力必須）"],
              ["solutionProfileId", "2. 解答の生成"],
              ["chatProfileId", "3. AI チャット"],
              ["verificationProfileId", "4. 解答の検証（Phase 3 連携）"],
              ["revisionProfileId", "5. 解答の修正（Phase 5）"],
            ] as const
          ).map(([fieldKey, label]) => (
            <div key={fieldKey}>
              <label className="mb-1 block text-[11px] text-slate-600 dark:text-slate-400">
                {label}
              </label>
              <select
                value={assignments[fieldKey] ?? ""}
                onChange={(e) => {
                  const val = e.target.value || null;
                  void saveSettings({
                    ...settings,
                    agentAssignments: { ...assignments, [fieldKey]: val },
                  });
                }}
                className="h-7 w-full rounded border border-slate-300 bg-white px-2 text-xs dark:border-slate-700 dark:bg-slate-800"
              >
                <option value="">既定エージェントを使用</option>
                {profiles
                  .filter((p) => p.isEnabled)
                  .map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
              </select>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

export function ModelSettingsPanel() {
  const {
    profiles,
    statusesByProfileId,
    loadProfiles,
    saveProfile,
    setApiKey,
    deleteApiKey,
  } = useAgentStore();

  const [apiKeyInputs, setApiKeyInputs] = useState<Record<string, string>>({});
  const [explicitConsent, setExplicitConsent] = useState<Record<string, boolean>>({});
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    void loadProfiles();
  }, [loadProfiles]);

  const handleSaveModelAndAuth = async (
    profileId: string,
    defaultModel: string | null,
    authMode: AuthMode,
  ) => {
    const p = profiles.find((x) => x.id === profileId);
    if (!p) return;
    await saveProfile({
      id: p.id,
      name: p.name,
      providerKind: p.providerKind,
      executablePath: p.executablePath,
      args: p.args,
      defaultModel,
      authMode,
      isEnabled: p.isEnabled,
      isDefault: p.isDefault,
    });
    setNotice(`${p.name} のモデル・認証方式を更新しました。`);
  };

  return (
    <div className="space-y-4 text-xs">
      <div className="rounded-lg border border-blue-200 bg-blue-50/70 p-3 text-blue-900 dark:border-blue-900 dark:bg-blue-950/40 dark:text-blue-100">
        <div className="font-semibold">従量課金（API キー）の無断切替禁止ポリシー</div>
        <p className="mt-1 text-[11px] leading-relaxed">
          公式ハーネス（サブスクリプション）利用時は環境変数の API キーを子プロセスへ引き継ぎません。API キーによる従量課金はユーザーの明示的な同意と切替操作時のみ有効化され、キーは OS キーチェーン（SQLite 平文保存禁止）へ保管されます。
        </p>
      </div>

      {notice && (
        <div className="flex items-center gap-1.5 rounded bg-emerald-50 px-3 py-2 text-emerald-800 dark:bg-emerald-950/50 dark:text-emerald-200">
          <CheckCircle2 className="h-4 w-4" />
          <span>{notice}</span>
        </div>
      )}

      {profiles.map((p) => {
        const st = statusesByProfileId[p.id];
        const models = st?.availableModels ?? null;
        return (
          <div
            key={p.id}
            className="space-y-2.5 rounded-lg border border-slate-200 bg-white p-3 dark:border-slate-800 dark:bg-slate-900"
          >
            <div className="flex items-center justify-between">
              <span className="font-semibold text-slate-900 dark:text-slate-100">{p.name}</span>
              <Badge variant="outline">{p.authMode}</Badge>
            </div>

            <div className="grid gap-2 sm:grid-cols-2">
              <div>
                <label className="mb-1 block text-[11px] text-slate-500">
                  デフォルトモデル（公式取得できた場合のみ選択可）
                </label>
                {models && models.length > 0 ? (
                  <select
                    value={p.defaultModel ?? ""}
                    onChange={(e) =>
                      void handleSaveModelAndAuth(p.id, e.target.value || null, p.authMode)
                    }
                    className="h-8 w-full rounded border border-slate-300 bg-white px-2 text-xs dark:border-slate-700 dark:bg-slate-800"
                  >
                    <option value="">ハーネス既定モデル</option>
                    {models.map((m) => (
                      <option key={m.id} value={m.id}>
                        {m.label}
                      </option>
                    ))}
                  </select>
                ) : (
                  <div className="flex h-8 items-center rounded border border-dashed border-slate-300 px-2 text-[11px] text-slate-500 dark:border-slate-700">
                    モデル一覧: 未取得（ハーネス既定モデルを使用）
                  </div>
                )}
              </div>

              {p.providerKind !== "mock" && (
                <div>
                  <label className="mb-1 block text-[11px] text-slate-500">認証モード</label>
                  <select
                    value={p.authMode}
                    onChange={(e) =>
                      void handleSaveModelAndAuth(
                        p.id,
                        p.defaultModel,
                        e.target.value as AuthMode,
                      )
                    }
                    className="h-8 w-full rounded border border-slate-300 bg-white px-2 text-xs dark:border-slate-700 dark:bg-slate-800"
                  >
                    <option value="official_harness">公式ハーネス認証（既定）</option>
                    <option value="api_key_keyring">
                      API キー代替（明示同意・OS キーチェーン）
                    </option>
                  </select>
                </div>
              )}
            </div>

            {p.authMode === "api_key_keyring" && p.providerKind !== "mock" && (
              <div className="space-y-2 rounded border border-amber-200 bg-amber-50/60 p-2.5 dark:border-amber-900 dark:bg-amber-950/30">
                <div className="flex items-center justify-between text-[11px]">
                  <span className="inline-flex items-center gap-1 font-semibold">
                    <KeyRound className="h-3.5 w-3.5" />
                    OS キーチェーン保管状態: {p.hasKeyringSecret ? "登録済み" : "未登録"}
                  </span>
                  {p.hasKeyringSecret && (
                    <Button
                      size="xs"
                      variant="ghost"
                      className="text-red-600"
                      onClick={() => void deleteApiKey(p.id)}
                    >
                      キー削除
                    </Button>
                  )}
                </div>
                <label className="inline-flex items-center gap-1.5 text-[11px]">
                  <input
                    type="checkbox"
                    checked={Boolean(explicitConsent[p.id])}
                    onChange={(e) =>
                      setExplicitConsent((prev) => ({ ...prev, [p.id]: e.target.checked }))
                    }
                  />
                  <span>従量課金 API キーの利用に明示的に同意します</span>
                </label>
                <div className="flex gap-1.5">
                  <Input
                    type="password"
                    placeholder="API キーを入力（ログ・SQLite には保存されません）"
                    value={apiKeyInputs[p.id] ?? ""}
                    onChange={(e) =>
                      setApiKeyInputs((prev) => ({ ...prev, [p.id]: e.target.value }))
                    }
                    className="h-7 flex-1"
                  />
                  <Button
                    size="xs"
                    variant="primary"
                    disabled={!explicitConsent[p.id] || !(apiKeyInputs[p.id] ?? "").trim()}
                    onClick={() => {
                      const val = apiKeyInputs[p.id] ?? "";
                      void setApiKey(p.id, val).then(() => {
                        setApiKeyInputs((prev) => ({ ...prev, [p.id]: "" }));
                        setNotice(`${p.name} の API キーを OS キーチェーンへ安全に保存しました。`);
                      });
                    }}
                  >
                    キーチェーン保存
                  </Button>
                </div>
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
