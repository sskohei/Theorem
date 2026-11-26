import { useEffect, useState } from "react";
import {
  Archive,
  Bot,
  Calculator,
  Cloud,
  Cpu,
  FileText,
  HardDrive,
  Info,
  MessageSquareCode,
  Palette,
  RotateCcw,
  ShieldCheck,
  Sliders,
  Trash2,
  X,
} from "lucide-react";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { useI18n } from "@/lib/i18n";
import { cn, formatBytes, formatShortDate } from "@/lib/utils";
import { normalizeApiError, tauriClient } from "@/services/tauriClient";
import { useLayoutStore, type SettingsCategory } from "@/stores/useLayoutStore";
import { useLibraryStore } from "@/stores/useLibraryStore";
import type { BackupInfo, PdfSettings, RecognitionMode, StorageStats, ThemePreference } from "@/types/ipc";
import { AgentSettingsPanel, ModelSettingsPanel } from "./AgentSettingsSection";

export function SettingsModal() {
  const t = useI18n();
  const {
    settingsOpen,
    settingsCategory,
    theme,
    closeSettings,
    setSettingsCategory,
    resetSidebarWidths,
  } = useLayoutStore();
  const {
    appVersion,
    dataRootDisplay,
    subjects,
    settings,
    saveSettings,
    applyBootstrap,
  } = useLibraryStore();

  const [storageStats, setStorageStats] = useState<StorageStats | null>(null);
  const [backups, setBackups] = useState<BackupInfo[]>([]);
  const [statusMessage, setStatusMessage] = useState<string | null>(null);
  const [busyAction, setBusyAction] = useState(false);

  useEffect(() => {
    if (!settingsOpen || settingsCategory !== "storage") return;
    let cancelled = false;
    const load = async () => {
      try {
        const [stats, list] = await Promise.all([
          tauriClient.getStorageStats(),
          tauriClient.listBackups(),
        ]);
        if (!cancelled) {
          setStorageStats(stats);
          setBackups(list);
        }
      } catch (err) {
        if (!cancelled) setStatusMessage(normalizeApiError(err).message);
      }
    };
    void load();
    return () => {
      cancelled = true;
    };
  }, [settingsOpen, settingsCategory]);

  if (!settingsOpen) return null;

  const categories: {
    id: SettingsCategory;
    label: string;
    icon: React.ComponentType<{ className?: string }>;
    phase?: string;
  }[] = [
    { id: "general", label: t.settings.categories.general, icon: Sliders },
    { id: "appearance", label: t.settings.categories.appearance, icon: Palette },
    { id: "agents", label: t.settings.categories.agents, icon: Bot },
    { id: "models", label: t.settings.categories.models, icon: Cpu },
    { id: "prompts", label: t.settings.categories.prompts, icon: MessageSquareCode },
    { id: "pdf", label: t.settings.categories.pdf, icon: FileText },
    { id: "drive", label: t.settings.categories.drive, icon: Cloud, phase: "Phase 4" },
    { id: "scienceMcp", label: t.settings.categories.scienceMcp, icon: Calculator, phase: "Phase 3" },
    { id: "storage", label: t.settings.categories.storage, icon: HardDrive },
    { id: "security", label: t.settings.categories.security, icon: ShieldCheck },
    { id: "about", label: t.settings.categories.about, icon: Info },
  ];

  const handleThemeChange = async (nextTheme: ThemePreference) => {
    await saveSettings({ ...settings, theme: nextTheme });
    setStatusMessage(t.settings.general.saveSuccess);
  };

  const handleCleanupTemp = async () => {
    setBusyAction(true);
    try {
      const freed = await tauriClient.cleanupTempFiles();
      const stats = await tauriClient.getStorageStats();
      setStorageStats(stats);
      setStatusMessage(`${t.settings.storage.cleanedNotice} (${formatBytes(freed)})`);
    } catch (err) {
      setStatusMessage(normalizeApiError(err).message);
    } finally {
      setBusyAction(false);
    }
  };

  const handleCreateBackup = async () => {
    setBusyAction(true);
    try {
      const info = await tauriClient.createBackup();
      const [stats, list] = await Promise.all([
        tauriClient.getStorageStats(),
        tauriClient.listBackups(),
      ]);
      setStorageStats(stats);
      setBackups(list);
      setStatusMessage(`バックアップを作成しました: ${info.filename}`);
    } catch (err) {
      setStatusMessage(normalizeApiError(err).message);
    } finally {
      setBusyAction(false);
    }
  };

  const handleRestoreBackup = async (relPath: string) => {
    if (!window.confirm(t.settings.storage.restoreConfirm)) return;
    setBusyAction(true);
    try {
      const boot = await tauriClient.restoreBackup(relPath);
      applyBootstrap(boot);
      const stats = await tauriClient.getStorageStats();
      setStorageStats(stats);
      setStatusMessage("バックアップから復元しました。");
    } catch (err) {
      setStatusMessage(normalizeApiError(err).message);
    } finally {
      setBusyAction(false);
    }
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={t.settings.title}
      className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/50 p-4 backdrop-blur-xs"
    >
      <div className="flex h-[580px] w-full max-w-4xl overflow-hidden rounded-xl border border-slate-200 bg-white shadow-2xl dark:border-slate-800 dark:bg-slate-900">
        {/* Category List */}
        <nav className="w-52 shrink-0 overflow-y-auto border-r border-slate-200 bg-slate-50 p-2 dark:border-slate-800 dark:bg-slate-950/60">
          <div className="px-2.5 py-2 text-xs font-semibold text-slate-500 uppercase dark:text-slate-400">
            {t.settings.title}
          </div>
          <div className="space-y-0.5">
            {categories.map((cat) => {
              const Icon = cat.icon;
              const active = settingsCategory === cat.id;
              return (
                <button
                  key={cat.id}
                  type="button"
                  onClick={() => {
                    setStatusMessage(null);
                    setSettingsCategory(cat.id);
                  }}
                  className={cn(
                    "flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-xs font-medium cursor-pointer",
                    active
                      ? "bg-blue-100 text-blue-900 dark:bg-blue-900/50 dark:text-blue-100"
                      : "text-slate-700 hover:bg-slate-200/70 dark:text-slate-300 dark:hover:bg-slate-800",
                  )}
                >
                  <Icon className="h-3.5 w-3.5 shrink-0" />
                  <span className="truncate">{cat.label}</span>
                  {cat.phase && (
                    <Badge variant="outline" className="ml-auto px-1 py-0 text-[9px]">
                      {cat.phase}
                    </Badge>
                  )}
                </button>
              );
            })}
          </div>
        </nav>

        {/* Right Panel */}
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="flex items-center justify-between border-b border-slate-200 px-5 py-3 dark:border-slate-800">
            <h2 className="text-sm font-semibold text-slate-900 dark:text-slate-100">
              {t.settings.categories[settingsCategory]}
            </h2>
            <Button size="icon" variant="ghost" onClick={closeSettings} aria-label={t.settings.close}>
              <X className="h-4 w-4" />
            </Button>
          </div>

          <div className="flex-1 overflow-y-auto p-5 space-y-4 text-xs">
            {statusMessage && (
              <div className="rounded-md border border-blue-200 bg-blue-50 px-3 py-2 text-blue-900 dark:border-blue-900 dark:bg-blue-950/50 dark:text-blue-100">
                {statusMessage}
              </div>
            )}

            {settingsCategory === "general" && (
              <div className="space-y-4">
                <div>
                  <label className="mb-1 block font-medium text-slate-700 dark:text-slate-200">
                    {t.settings.general.language}
                  </label>
                  <div className="flex items-center gap-2">
                    <select
                      value={settings.language}
                      disabled
                      className="h-8 rounded-md border border-slate-300 bg-slate-100 px-2.5 text-xs dark:border-slate-700 dark:bg-slate-800"
                    >
                      <option value="ja">{t.settings.general.languageJa}</option>
                    </select>
                    <span className="text-[11px] text-slate-500">
                      {t.settings.general.languageEnHint}
                    </span>
                  </div>
                </div>

                <div>
                  <label className="mb-1 block font-medium text-slate-700 dark:text-slate-200">
                    {t.settings.general.defaultSubject}
                  </label>
                  <select
                    value={settings.defaultSubjectId ?? ""}
                    onChange={(e) => {
                      const val = e.target.value || null;
                      void saveSettings({ ...settings, defaultSubjectId: val }).then(() =>
                        setStatusMessage(t.settings.general.saveSuccess),
                      );
                    }}
                    className="h-8 w-64 rounded-md border border-slate-300 bg-white px-2.5 text-xs dark:border-slate-700 dark:bg-slate-800"
                  >
                    <option value="">自動（選択中または先頭の科目）</option>
                    {subjects.map((sub) => (
                      <option key={sub.id} value={sub.id}>
                        {sub.name}
                      </option>
                    ))}
                  </select>
                </div>

                <div>
                  <label className="mb-1 block font-medium text-slate-700 dark:text-slate-200">
                    {t.settings.general.defaultRecognitionMode}
                  </label>
                  <select
                    value={settings.defaultRecognitionMode}
                    onChange={(e) => {
                      void saveSettings({
                        ...settings,
                        defaultRecognitionMode: e.target.value as RecognitionMode,
                      }).then(() => setStatusMessage(t.settings.general.saveSuccess));
                    }}
                    className="h-8 w-64 rounded-md border border-slate-300 bg-white px-2.5 text-xs dark:border-slate-700 dark:bg-slate-800"
                  >
                    <option value="printed_only">印刷された問題文のみ（推奨・既定）</option>
                    <option value="include_handwriting">手書きを含めて認識</option>
                    <option value="page_range">指定したページ・範囲のみ</option>
                  </select>
                </div>
              </div>
            )}

            {settingsCategory === "appearance" && (
              <div className="space-y-4">
                <div>
                  <div className="mb-2 font-medium text-slate-700 dark:text-slate-200">
                    {t.settings.appearance.theme}
                  </div>
                  <div className="flex gap-2">
                    {(["light", "dark", "system"] as ThemePreference[]).map((opt) => (
                      <Button
                        key={opt}
                        variant={theme === opt ? "primary" : "outline"}
                        size="sm"
                        onClick={() => void handleThemeChange(opt)}
                      >
                        {opt === "light"
                          ? t.header.themeLight
                          : opt === "dark"
                            ? t.header.themeDark
                            : t.header.themeSystem}
                      </Button>
                    ))}
                  </div>
                </div>

                <div className="pt-2">
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => {
                      resetSidebarWidths();
                      void saveSettings({
                        ...settings,
                        leftSidebarWidth: 280,
                        rightSidebarWidth: 320,
                      });
                    }}
                  >
                    <RotateCcw className="h-3.5 w-3.5" />
                    <span>{t.settings.appearance.sidebarReset}</span>
                  </Button>
                </div>
              </div>
            )}

            {settingsCategory === "storage" && (
              <div className="space-y-4">
                <div>
                  <div className="font-medium text-slate-700 dark:text-slate-200">
                    {t.settings.storage.dataRoot}
                  </div>
                  <code className="mt-1 block rounded bg-slate-100 p-2 font-mono text-[11px] break-all dark:bg-slate-800">
                    {storageStats?.dataRootDisplay ?? dataRootDisplay}
                  </code>
                </div>

                {storageStats && (
                  <div className="grid grid-cols-2 gap-2 rounded-lg border border-slate-200 p-3 dark:border-slate-800">
                    <div>
                      <span className="text-slate-500">{t.settings.storage.documentsCount}: </span>
                      <strong>{storageStats.documentCount}</strong>
                    </div>
                    <div>
                      <span className="text-slate-500">{t.settings.storage.attachmentsCount}: </span>
                      <strong>{storageStats.attachmentCount}</strong>
                    </div>
                    <div>DB: {formatBytes(storageStats.databaseBytes)}</div>
                    <div>問題原本: {formatBytes(storageStats.originalsBytes)}</div>
                    <div>サムネイル: {formatBytes(storageStats.thumbnailsBytes)}</div>
                    <div>バックアップ: {formatBytes(storageStats.backupsBytes)}</div>
                    <div>一時領域: {formatBytes(storageStats.tempBytes)}</div>
                    <div className="font-semibold">合計: {formatBytes(storageStats.totalBytes)}</div>
                  </div>
                )}

                <div className="flex flex-wrap gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={busyAction}
                    onClick={() => void handleCleanupTemp()}
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                    <span>{t.settings.storage.cleanupTemp}</span>
                  </Button>
                  <Button
                    variant="primary"
                    size="sm"
                    disabled={busyAction}
                    onClick={() => void handleCreateBackup()}
                  >
                    <Archive className="h-3.5 w-3.5" />
                    <span>{t.settings.storage.createBackup}</span>
                  </Button>
                </div>

                <div className="space-y-2 border-t border-slate-200 pt-3 dark:border-slate-800">
                  <div className="font-semibold text-slate-800 dark:text-slate-100">
                    {t.settings.storage.backupTitle}
                  </div>
                  <p className="text-slate-500 dark:text-slate-400">
                    {t.settings.storage.backupDescription}
                  </p>
                  {backups.length === 0 ? (
                    <p className="text-slate-400">{t.settings.storage.noBackups}</p>
                  ) : (
                    <ul className="space-y-1.5">
                      {backups.map((b) => (
                        <li
                          key={b.relativePath}
                          className="flex items-center justify-between rounded border border-slate-200 px-3 py-2 dark:border-slate-800"
                        >
                          <div>
                            <div className="font-mono font-medium">{b.filename}</div>
                            <div className="text-[11px] text-slate-500">
                              {formatShortDate(b.createdAt)} · {formatBytes(b.byteSize)}
                            </div>
                          </div>
                          <Button
                            size="xs"
                            variant="outline"
                            disabled={busyAction}
                            onClick={() => void handleRestoreBackup(b.relativePath)}
                          >
                            {t.settings.storage.restoreBackup}
                          </Button>
                        </li>
                      ))}
                    </ul>
                  )}
                </div>
              </div>
            )}

            {settingsCategory === "security" && (
              <div className="space-y-4">
                <div className="rounded-lg border border-blue-200 bg-blue-50/70 p-3 dark:border-blue-900 dark:bg-blue-950/40">
                  <div className="font-semibold text-blue-900 dark:text-blue-100">
                    {t.settings.security.localFirstTitle}
                  </div>
                  <p className="mt-1 leading-relaxed text-blue-800 dark:text-blue-200">
                    {t.settings.security.localFirstBody}
                  </p>
                </div>
                <div>
                  <div className="mb-1.5 font-semibold text-slate-800 dark:text-slate-100">
                    {t.settings.security.boundariesTitle}
                  </div>
                  <ul className="list-disc space-y-1.5 pl-4 text-slate-600 dark:text-slate-300">
                    {t.settings.security.items.map((item) => (
                      <li key={item}>{item}</li>
                    ))}
                  </ul>
                </div>
              </div>
            )}

            {settingsCategory === "about" && (
              <div className="space-y-2">
                <div className="text-base font-bold text-slate-900 dark:text-slate-100">
                  {t.appName}
                </div>
                <p className="text-slate-500">{t.appSubtitle}</p>
                <dl className="mt-3 grid grid-cols-[120px_1fr] gap-y-2">
                  <dt className="text-slate-500">{t.settings.about.version}</dt>
                  <dd className="font-mono">{appVersion}</dd>
                  <dt className="text-slate-500">{t.settings.about.license}</dt>
                  <dd>MIT License</dd>
                  <dt className="text-slate-500">{t.settings.about.status}</dt>
                  <dd>{t.settings.about.statusValue}</dd>
                </dl>
              </div>
            )}

            {settingsCategory === "agents" && <AgentSettingsPanel />}

            {settingsCategory === "models" && <ModelSettingsPanel />}

            {settingsCategory === "prompts" && (
              <div className="space-y-4">
                <p className="text-[11px] text-slate-500 dark:text-slate-400">
                  大学数学・大学物理・解答モードごとの追加プロンプト指示を設定できます（任意シェル禁止や出力スキーマ検証などのシステム安全制約は上書きできません）。
                </p>
                <div>
                  <label className="mb-1 block font-medium text-slate-700 dark:text-slate-200">
                    大学数学向け追加指示
                  </label>
                  <textarea
                    rows={3}
                    value={settings.prompts.mathematicsCustomInstructions}
                    onChange={(e) =>
                      void saveSettings({
                        ...settings,
                        prompts: {
                          ...settings.prompts,
                          mathematicsCustomInstructions: e.target.value,
                        },
                      })
                    }
                    placeholder="例: 定義域と定理の仮定を必ず明示すること"
                    className="w-full rounded-md border border-slate-300 bg-white p-2 text-xs dark:border-slate-700 dark:bg-slate-800"
                  />
                </div>
                <div>
                  <label className="mb-1 block font-medium text-slate-700 dark:text-slate-200">
                    大学物理向け追加指示
                  </label>
                  <textarea
                    rows={3}
                    value={settings.prompts.physicsCustomInstructions}
                    onChange={(e) =>
                      void saveSettings({
                        ...settings,
                        prompts: {
                          ...settings.prompts,
                          physicsCustomInstructions: e.target.value,
                        },
                      })
                    }
                    placeholder="例: 座標系・SI 単位・極限ケースの考察を必ず含めること"
                    className="w-full rounded-md border border-slate-300 bg-white p-2 text-xs dark:border-slate-700 dark:bg-slate-800"
                  />
                </div>
                <div>
                  <label className="mb-1 block font-medium text-slate-700 dark:text-slate-200">
                    モード共通追加指示
                  </label>
                  <textarea
                    rows={2}
                    value={settings.prompts.modeCustomInstructions}
                    onChange={(e) =>
                      void saveSettings({
                        ...settings,
                        prompts: {
                          ...settings.prompts,
                          modeCustomInstructions: e.target.value,
                        },
                      })
                    }
                    placeholder="例: 途中式の変形理由を簡潔に添える"
                    className="w-full rounded-md border border-slate-300 bg-white p-2 text-xs dark:border-slate-700 dark:bg-slate-800"
                  />
                </div>
              </div>
            )}

            {settingsCategory === "pdf" && (
              <div className="space-y-4">
                <div>
                  <label className="mb-1 block font-medium text-slate-700 dark:text-slate-200">
                    既定テンプレート
                  </label>
                  <select
                    aria-label="既定PDFテンプレート"
                    value={settings.pdf.defaultTemplate}
                    onChange={(e) =>
                      void saveSettings({
                        ...settings,
                        pdf: {
                          ...settings.pdf,
                          defaultTemplate: e.target.value as PdfSettings["defaultTemplate"],
                        },
                      }).then(() => setStatusMessage(t.settings.general.saveSuccess))
                    }
                    className="h-8 w-64 rounded-md border border-slate-300 bg-white px-2.5 text-xs dark:border-slate-700 dark:bg-slate-800"
                  >
                    <option value="auto">自動（解答モードに合わせる）</option>
                    <option value="detailed">詳細解説 (detailed.typ)</option>
                    <option value="concise">簡潔模範解答 (concise.typ)</option>
                    <option value="mathematics">大学数学向け (mathematics.typ)</option>
                    <option value="physics">大学物理向け (physics.typ)</option>
                  </select>
                </div>

                <div>
                  <label className="mb-1 block font-medium text-slate-700 dark:text-slate-200">
                    用紙サイズ
                  </label>
                  <select
                    aria-label="PDF用紙サイズ"
                    value={settings.pdf.paperSize}
                    onChange={(e) =>
                      void saveSettings({
                        ...settings,
                        pdf: {
                          ...settings.pdf,
                          paperSize: e.target.value as PdfSettings["paperSize"],
                        },
                      }).then(() => setStatusMessage(t.settings.general.saveSuccess))
                    }
                    className="h-8 w-64 rounded-md border border-slate-300 bg-white px-2.5 text-xs dark:border-slate-700 dark:bg-slate-800"
                  >
                    <option value="a4">A4（既定）</option>
                    <option value="b5">B5</option>
                    <option value="letter">US Letter</option>
                  </select>
                </div>

                <div className="space-y-2 border-t border-slate-200 pt-3 dark:border-slate-800">
                  <div className="font-medium text-slate-700 dark:text-slate-200">
                    PDF ヘッダー・メタデータ表示設定
                  </div>
                  <label className="flex items-center gap-2">
                    <input
                      type="checkbox"
                      checked={settings.pdf.showModelName}
                      onChange={(e) =>
                        void saveSettings({
                          ...settings,
                          pdf: { ...settings.pdf, showModelName: e.target.checked },
                        })
                      }
                    />
                    <span>使用した AI モデル名を PDF に表示する</span>
                  </label>
                  <label className="flex items-center gap-2">
                    <input
                      type="checkbox"
                      checked={settings.pdf.showGeneratedAt}
                      onChange={(e) =>
                        void saveSettings({
                          ...settings,
                          pdf: { ...settings.pdf, showGeneratedAt: e.target.checked },
                        })
                      }
                    />
                    <span>生成日時を PDF に表示する</span>
                  </label>
                  <label className="flex items-center gap-2">
                    <input
                      type="checkbox"
                      checked={settings.pdf.showVersionNumber}
                      onChange={(e) =>
                        void saveSettings({
                          ...settings,
                          pdf: { ...settings.pdf, showVersionNumber: e.target.checked },
                        })
                      }
                    />
                    <span>解答バージョン番号を PDF に表示する</span>
                  </label>
                </div>
              </div>
            )}

            {!["general", "appearance", "agents", "models", "prompts", "pdf", "storage", "security", "about"].includes(
              settingsCategory,
            ) && (
              <div className="rounded-lg border border-dashed border-slate-300 p-6 text-center text-slate-500 dark:border-slate-700 dark:text-slate-400">
                {t.settings.upcomingNotice}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
