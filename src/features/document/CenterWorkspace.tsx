import { useRef, useState } from "react";
import { AlertTriangle, CloudUpload, FileUp, Info, X } from "lucide-react";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { CalculationsTab } from "@/features/verification/CalculationsTab";
import { useI18n } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { preparePdfRecognition } from "@/services/pdfRecognition";
import { normalizeApiError } from "@/services/tauriClient";
import { useAiWorkspaceStore } from "@/stores/useAiWorkspaceStore";
import { useDocumentStore } from "@/stores/useDocumentStore";
import { useDriveStore } from "@/stores/useDriveStore";
import { useLayoutStore, type CenterTab } from "@/stores/useLayoutStore";
import { useLibraryStore } from "@/stores/useLibraryStore";
import { useScienceStore } from "@/stores/useScienceStore";
import type { ApiError, RecognitionMode } from "@/types/ipc";
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
    totalPages,
    croppedPreviewUrl,
    croppedPage,
  } = useDocumentStore();
  const {
    problems,
    solutions,
    recognizing,
    error: aiError,
    recognize,
    clearError: clearAiError,
  } = useAiWorkspaceStore();
  const { toolExecutions, verifications } = useScienceStore();
  const {
    overview: driveOverview,
    documentStatusById,
    uploadingDocId,
    uploadDocument: uploadDocumentToDrive,
    openConflictModal,
  } = useDriveStore();
  const calcBadgeCount = verifications.length + toolExecutions.length;
  const activeDocId = detail?.document.id ?? null;
  const docDriveStatus = activeDocId ? documentStatusById[activeDocId] : undefined;
  const docConflictJob =
    docDriveStatus?.jobs.find((j) => j.status === "conflict") ?? null;
  const driveConnected = driveOverview?.connection?.status === "connected";

  const preparingRef = useRef(false);
  const [preparing, setPreparing] = useState(false);
  const [preparationError, setPreparationError] = useState<{ documentId: string; error: ApiError } | null>(null);

  const tabs: { id: CenterTab; label: string; count?: number; phaseBadge?: string }[] = [
    { id: "original", label: t.tabs.original },
    { id: "recognized", label: t.tabs.recognized, count: problems.length || undefined },
    { id: "solutionPdf", label: t.tabs.solutionPdf },
    { id: "solutionEditor", label: t.tabs.solutionEditor },
    { id: "calculations", label: t.tabs.calculations, count: calcBadgeCount || undefined },
    { id: "versions", label: t.tabs.versions, count: solutions.length || undefined },
  ];

  const primaryAttachment =
    detail?.attachments.find((a) => a.id === detail.document.primaryAttachmentId) ?? null;

  const activeError = libraryError ?? docError ?? aiError
    ?? (preparationError?.documentId === detail?.document.id ? preparationError?.error : null);

  const handleDismissError = () => {
    clearError();
    clearAiError();
    setPreparationError(null);
  };

  const handleRecognize = async (mode: RecognitionMode, useViewerRange = false) => {
    if (!detail || !primaryAttachment || loading || preparingRef.current || recognizing) return;
    const documentId = detail.document.id;
    const stillSelected = () =>
      useDocumentStore.getState().detail === detail &&
      useLibraryStore.getState().selectedDocumentId === documentId;
    if (!stillSelected()) return;

    preparingRef.current = true;
    setPreparing(true);
    setPreparationError(null);
    clearAiError();
    try {
      const isPdf = primaryAttachment.mimeType === "application/pdf";
      const crop = isPdf && mode === "page_range" && croppedPage !== null
        ? croppedPreviewUrl?.split(",")[1] ?? null
        : null;
      const range = isPdf && (mode === "page_range" || useViewerRange)
        ? crop && croppedPage !== null
          ? { start: croppedPage, end: croppedPage }
          : { start: pageRangeStart, end: pageRangeEnd }
        : null;
      if (isPdf && !primaryBase64) throw new Error("PDF data is unavailable");
      const pageImages = isPdf && primaryBase64 && !crop
        ? await preparePdfRecognition(primaryBase64, range)
        : [];
      if (!stillSelected()) return;
      setActiveTab("recognized");
      await recognize({
        documentId,
        recognitionMode: mode,
        pageRangeStart: range?.start ?? null,
        pageRangeEnd: range?.end ?? null,
        pageImages,
        croppedRegionPngBase64: crop,
      });
    } catch (err) {
      if (stillSelected()) {
        const error = normalizeApiError(err);
        setPreparationError({
          documentId,
          error: { ...error, message: t.viewer.recognitionPreparationFailed, detail: error.message },
        });
      }
    } finally {
      preparingRef.current = false;
      setPreparing(false);
    }
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

        {activeDocId && driveConnected && (
          <div className="ml-auto flex shrink-0 items-center gap-1.5 pb-1 pr-1">
            {docConflictJob ? (
              <Button
                size="xs"
                variant="primary"
                onClick={() => openConflictModal(docConflictJob)}
              >
                <AlertTriangle className="h-3 w-3" />
                <span>Drive 競合を解決</span>
              </Button>
            ) : (
              <Button
                size="xs"
                variant="outline"
                disabled={uploadingDocId === activeDocId}
                onClick={() =>
                  void uploadDocumentToDrive({
                    documentId: activeDocId,
                    scope: "default",
                  })
                }
              >
                <CloudUpload className="h-3 w-3" />
                <span>
                  {uploadingDocId === activeDocId
                    ? "投入中..."
                    : (docDriveStatus?.files.length ?? 0) > 0
                      ? "Drive 更新"
                      : "Drive へ保存"}
                </span>
              </Button>
            )}
          </div>
        )}
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
                onRecognize={() => void handleRecognize(
                  croppedPreviewUrl ? "page_range" : settings.defaultRecognitionMode,
                  pageRangeStart !== 1 || pageRangeEnd !== totalPages,
                )}
                recognizing={recognizing || preparing}
              />
            ) : (
              <ImageViewer
                attachment={primaryAttachment}
                base64Data={primaryBase64}
                onRecognizeImage={() => void handleRecognize(settings.defaultRecognitionMode)}
                recognizing={recognizing || preparing}
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
          <RecognizedProblemsTab
            key={detail?.document.id}
            onTriggerRecognize={handleRecognize}
            preparing={preparing}
          />
        ) : activeTab === "solutionPdf" ? (
          <SolutionPdfTab />
        ) : activeTab === "solutionEditor" ? (
          <SolutionEditorTab />
        ) : activeTab === "versions" ? (
          <SolutionVersionsTab />
        ) : (
          <CalculationsTab />
        )}
      </div>
    </main>
  );
}
