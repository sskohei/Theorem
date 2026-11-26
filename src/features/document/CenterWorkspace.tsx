import { AlertTriangle, FileUp, Info, Layers, X } from "lucide-react";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { useI18n } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { useAiWorkspaceStore } from "@/stores/useAiWorkspaceStore";
import { useDocumentStore } from "@/stores/useDocumentStore";
import { useLayoutStore, type CenterTab } from "@/stores/useLayoutStore";
import { useLibraryStore } from "@/stores/useLibraryStore";
import type { PageImagePayload } from "@/types/ipc";
import { ImageViewer } from "./ImageViewer";
import { PdfViewer } from "./PdfViewer";
import { RecognizedProblemsTab } from "./RecognizedProblemsTab";
import { SolutionEditorTab } from "./SolutionEditorTab";
import { SolutionPdfTab } from "./SolutionPdfTab";
import { SolutionVersionsTab } from "./SolutionVersionsTab";

interface CenterWorkspaceProps {
  onImportClick: () => void;
}

export function CenterWorkspace({ onImportClick }: CenterWorkspaceProps) {
  const t = useI18n();
  const { activeTab, setActiveTab } = useLayoutStore();
  const {
    settings,
    duplicateNotice,
    error: libraryError,
    dismissDuplicateNotice,
    clearError,
    selectDocument,
  } = useLibraryStore();
  const {
    detail,
    primaryBase64,
    loading,
    error: docError,
    pageRangeStart,
    pageRangeEnd,
  } = useDocumentStore();
  const {
    problems,
    solutions,
    recognizing,
    error: aiError,
    recognize,
    clearError: clearAiError,
  } = useAiWorkspaceStore();

  const tabs: { id: CenterTab; label: string; count?: number; phaseBadge?: string }[] = [
    { id: "original", label: t.tabs.original },
    { id: "recognized", label: t.tabs.recognized, count: problems.length || undefined },
    { id: "solutionPdf", label: t.tabs.solutionPdf },
    { id: "solutionEditor", label: t.tabs.solutionEditor },
    { id: "calculations", label: t.tabs.calculations, phaseBadge: "Phase 3" },
    { id: "versions", label: t.tabs.versions, count: solutions.length || undefined },
  ];

  const primaryAttachment =
    detail?.attachments.find((a) => a.id === detail.document.primaryAttachmentId) ?? null;

  const activeError = libraryError ?? docError ?? aiError;

  const handleDismissError = () => {
    clearError();
    clearAiError();
  };

  const handleRecognizeFromPdf = async (
    pages: PageImagePayload[],
    croppedPngBase64: string | null,
  ) => {
    if (!detail) return;
    setActiveTab("recognized");
    await recognize({
      documentId: detail.document.id,
      recognitionMode: croppedPngBase64 ? "page_range" : settings.defaultRecognitionMode,
      pageRangeStart,
      pageRangeEnd,
      pageImages: pages,
      croppedRegionPngBase64: croppedPngBase64,
    });
  };

  const handleRecognizeFromImage = async () => {
    if (!detail) return;
    setActiveTab("recognized");
    await recognize({
      documentId: detail.document.id,
      recognitionMode: settings.defaultRecognitionMode,
      pageRangeStart: null,
      pageRangeEnd: null,
      pageImages: [],
      croppedRegionPngBase64: null,
    });
  };

  return (
    <main className="flex min-w-0 flex-1 flex-col overflow-hidden bg-slate-50 dark:bg-slate-950">
      {/* 6-Tab Navigation Header */}
      <div
        role="tablist"
        aria-label="ワークスペースタブ"
        className="flex items-center gap-1 overflow-x-auto border-b border-slate-200 bg-white px-2 pt-1.5 dark:border-slate-800 dark:bg-slate-900"
      >
        {tabs.map((tab) => {
          const active = activeTab === tab.id;
          return (
            <button
              key={tab.id}
              type="button"
              role="tab"
              aria-selected={active}
              onClick={() => setActiveTab(tab.id)}
              className={cn(
                "flex shrink-0 items-center gap-1.5 rounded-t-md border-b-2 px-3 py-1.5 text-xs font-medium transition-colors cursor-pointer",
                active
                  ? "border-blue-600 text-blue-700 dark:border-blue-400 dark:text-blue-300"
                  : "border-transparent text-slate-600 hover:text-slate-900 dark:text-slate-400 dark:hover:text-slate-100",
              )}
            >
              <span>{tab.label}</span>
              {tab.count !== undefined && (
                <Badge variant="info" className="px-1 py-0 text-[9px]">
                  {tab.count}
                </Badge>
              )}
              {tab.phaseBadge && (
                <Badge variant="outline" className="px-1 py-0 text-[9px] opacity-75">
                  {tab.phaseBadge}
                </Badge>
              )}
            </button>
          );
        })}
      </div>

      {/* Error Banner */}
      {activeError && (
        <div
          role="alert"
          className="flex items-start gap-2 border-b border-red-200 bg-red-50 px-3 py-2 text-xs text-red-800 dark:border-red-900 dark:bg-red-950/70 dark:text-red-200"
        >
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <div className="min-w-0 flex-1">
            <div className="font-semibold">
              {t.common.errorPrefix} ({activeError.code}): {activeError.message}
            </div>
            {activeError.detail && <div className="mt-0.5 opacity-85">{activeError.detail}</div>}
          </div>
          <Button
            size="icon"
            variant="ghost"
            onClick={handleDismissError}
            aria-label={t.common.dismiss}
          >
            <X className="h-3.5 w-3.5" />
          </Button>
        </div>
      )}

      {/* Duplicate Detection Notice Banner */}
      {duplicateNotice && duplicateNotice.length > 0 && (
        <div
          role="status"
          className="flex items-start gap-2 border-b border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-900 dark:border-amber-900 dark:bg-amber-950/60 dark:text-amber-100"
        >
          <Info className="mt-0.5 h-4 w-4 shrink-0 text-amber-600 dark:text-amber-400" />
          <div className="min-w-0 flex-1">
            <div className="font-semibold">{t.library.duplicateNoticeTitle}</div>
            <p className="mt-0.5">{t.library.duplicateNoticeBody}</p>
            <div className="mt-1 flex flex-wrap gap-1.5">
              {duplicateNotice.map((dup) => (
                <Button
                  key={dup.attachmentId}
                  size="xs"
                  variant="outline"
                  onClick={() => {
                    selectDocument(dup.documentId);
                    dismissDuplicateNotice();
                  }}
                >
                  {t.library.openExisting}: {dup.documentTitle}
                </Button>
              ))}
            </div>
          </div>
          <Button
            size="icon"
            variant="ghost"
            onClick={dismissDuplicateNotice}
            aria-label={t.common.dismiss}
          >
            <X className="h-3.5 w-3.5" />
          </Button>
        </div>
      )}

      {/* Tab Body */}
      <div className="min-h-0 flex-1 overflow-hidden">
        {activeTab === "original" ? (
          loading ? (
            <div className="flex h-full items-center justify-center text-sm text-slate-500">
              {t.common.loading}
            </div>
          ) : detail && primaryAttachment && primaryBase64 ? (
            primaryAttachment.mimeType === "application/pdf" ? (
              <PdfViewer
                documentId={detail.document.id}
                attachment={primaryAttachment}
                base64Data={primaryBase64}
                hasSavedThumbnail={detail.document.thumbnailAttachmentId !== null}
                onRecognizePages={(pages, crop) => void handleRecognizeFromPdf(pages, crop)}
                recognizing={recognizing}
              />
            ) : (
              <ImageViewer
                attachment={primaryAttachment}
                base64Data={primaryBase64}
                onRecognizeImage={() => void handleRecognizeFromImage()}
                recognizing={recognizing}
              />
            )
          ) : (
            <div className="flex h-full flex-col items-center justify-center p-8 text-center">
              <div className="mb-3 flex h-12 w-12 items-center justify-center rounded-xl bg-blue-100 text-blue-600 dark:bg-blue-900/40 dark:text-blue-300">
                <FileUp className="h-6 w-6" />
              </div>
              <h2 className="text-sm font-semibold text-slate-800 dark:text-slate-100">
                {t.viewer.emptyTitle}
              </h2>
              <p className="mt-1.5 max-w-md text-xs leading-relaxed text-slate-500 dark:text-slate-400">
                {t.viewer.emptyDescription}
              </p>
              <Button variant="primary" size="md" className="mt-4" onClick={onImportClick}>
                <FileUp className="h-4 w-4" />
                <span>{t.viewer.importNow}</span>
              </Button>
            </div>
          )
        ) : activeTab === "recognized" ? (
          <RecognizedProblemsTab />
        ) : activeTab === "solutionPdf" ? (
          <SolutionPdfTab />
        ) : activeTab === "solutionEditor" ? (
          <SolutionEditorTab />
        ) : activeTab === "versions" ? (
          <SolutionVersionsTab />
        ) : (
          <div className="flex h-full flex-col items-center justify-center p-8 text-center">
            <div className="mb-3 flex h-12 w-12 items-center justify-center rounded-xl bg-slate-200 text-slate-600 dark:bg-slate-800 dark:text-slate-300">
              <Layers className="h-6 w-6" />
            </div>
            <h2 className="text-sm font-semibold text-slate-800 dark:text-slate-100">
              {t.viewer.phase3PlannedTitle}
            </h2>
            <p className="mt-1.5 max-w-md text-xs leading-relaxed text-slate-500 dark:text-slate-400">
              {t.viewer.phase3PlannedDesc}
            </p>
          </div>
        )}
      </div>
    </main>
  );
}
