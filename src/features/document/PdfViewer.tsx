import { useEffect, useRef, useState } from "react";
import * as pdfjsLib from "pdfjs-dist";
import pdfWorkerUrl from "pdfjs-dist/build/pdf.worker.mjs?url";
import {
  Camera,
  ChevronLeft,
  ChevronRight,
  Crop,
  Download,
  FileText,
  PanelLeft,
  Sparkles,
  ZoomIn,
  ZoomOut,
} from "lucide-react";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { useI18n } from "@/lib/i18n";
import { base64ToUint8Array, cn } from "@/lib/utils";
import { tauriClient } from "@/services/tauriClient";
import { useDocumentStore } from "@/stores/useDocumentStore";
import type { AttachmentInfo, PageImagePayload } from "@/types/ipc";

pdfjsLib.GlobalWorkerOptions.workerSrc = pdfWorkerUrl;

interface PdfViewerProps {
  documentId: string;
  attachment: AttachmentInfo;
  base64Data: string;
  hasSavedThumbnail: boolean;
  onRecognizePages?: (pages: PageImagePayload[], croppedPngBase64: string | null) => void;
  recognizing?: boolean;
}

interface CropRect {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

export function PdfViewer({
  documentId,
  attachment,
  base64Data,
  hasSavedThumbnail,
  onRecognizePages,
  recognizing = false,
}: PdfViewerProps) {
  const t = useI18n();
  const {
    currentPage,
    totalPages,
    zoom,
    pageRangeStart,
    pageRangeEnd,
    extractedPageText,
    croppedPreviewUrl,
    setCurrentPage,
    setTotalPages,
    setZoom,
    setPageRange,
    setExtractedPageText,
    setCroppedPreviewUrl,
  } = useDocumentStore();

  const [pdfDoc, setPdfDoc] = useState<pdfjsLib.PDFDocumentProxy | null>(null);
  const [renderError, setRenderError] = useState<string | null>(null);
  const [showThumbs, setShowThumbs] = useState(true);
  const [showTextPanel, setShowTextPanel] = useState(false);
  const [showPreprocessBar, setShowPreprocessBar] = useState(false);
  const [cropMode, setCropMode] = useState(false);
  const [cropRect, setCropRect] = useState<CropRect | null>(null);
  const [draggingCrop, setDraggingCrop] = useState(false);
  const [pageInput, setPageInput] = useState("1");
  const [thumbnails, setThumbnails] = useState<Record<number, string>>({});

  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const textLayerRef = useRef<HTMLDivElement | null>(null);
  const thumbSavedRef = useRef(false);

  // 1. Load PDF document from Rust base64 payload
  useEffect(() => {
    let cancelled = false;
    let loadedDoc: pdfjsLib.PDFDocumentProxy | null = null;
    thumbSavedRef.current = hasSavedThumbnail;
    setRenderError(null);
    setThumbnails({});
    setPdfDoc(null);

    const load = async () => {
      try {
        const bytes = base64ToUint8Array(base64Data);
        const task = pdfjsLib.getDocument({ data: bytes });
        const doc = await task.promise;
        if (cancelled) {
          await doc.destroy();
          return;
        }
        loadedDoc = doc;
        setPdfDoc(doc);
        setTotalPages(doc.numPages);
        setPageRange(1, doc.numPages);
      } catch (err) {
        if (!cancelled) {
          setRenderError(err instanceof Error ? err.message : String(err));
        }
      }
    };
    void load();

    return () => {
      cancelled = true;
      if (loadedDoc) {
        void loadedDoc.destroy();
      }
    };
  }, [base64Data, hasSavedThumbnail, setPageRange, setTotalPages]);

  // 2. Render active page + selectable text layer + text extraction
  useEffect(() => {
    if (!pdfDoc) return;
    setPageInput(String(currentPage));
    let cancelled = false;
    let activeRenderTask: pdfjsLib.RenderTask | null = null;

    const render = async () => {
      try {
        const page = await pdfDoc.getPage(currentPage);
        if (cancelled) return;
        const viewport = page.getViewport({ scale: zoom });
        const canvas = canvasRef.current;
        if (!canvas) return;
        const ctx = canvas.getContext("2d");
        if (!ctx) return;

        canvas.width = Math.floor(viewport.width);
        canvas.height = Math.floor(viewport.height);
        activeRenderTask = page.render({ canvasContext: ctx, viewport, canvas });
        await activeRenderTask.promise;
        if (cancelled) return;

        // Text extraction & selectable overlay
        const textContent = await page.getTextContent();
        if (cancelled) return;
        const layer = textLayerRef.current;
        if (layer) {
          layer.innerHTML = "";
        }
        const pieces: string[] = [];
        for (const item of textContent.items) {
          if ("str" in item && item.str.trim()) {
            pieces.push(item.str);
            if (layer) {
              const span = document.createElement("span");
              span.textContent = item.str;
              const tx = pdfjsLib.Util.transform(viewport.transform, item.transform);
              const fontHeight = Math.hypot(tx[2] ?? 0, tx[3] ?? 0);
              span.style.left = `${tx[4] ?? 0}px`;
              span.style.top = `${(tx[5] ?? 0) - fontHeight}px`;
              span.style.fontSize = `${Math.max(8, fontHeight)}px`;
              layer.appendChild(span);
            }
          }
        }
        setExtractedPageText(pieces.join(" "));
      } catch (err) {
        const isCancelled =
          err instanceof Error && err.name === "RenderingCancelledException";
        if (!cancelled && !isCancelled) {
          setRenderError(err instanceof Error ? err.message : String(err));
        }
      }
    };
    void render();

    return () => {
      cancelled = true;
      activeRenderTask?.cancel();
    };
  }, [pdfDoc, currentPage, zoom, setExtractedPageText]);

  // 3. Render page thumbnails (first 30 pages eagerly) and persist page 1 thumbnail to Rust
  useEffect(() => {
    if (!pdfDoc || !showThumbs) return;
    let cancelled = false;

    const buildThumbs = async () => {
      const limit = Math.min(pdfDoc.numPages, 30);
      for (let p = 1; p <= limit; p++) {
        if (cancelled) return;
        try {
          const page = await pdfDoc.getPage(p);
          const vp = page.getViewport({ scale: 0.25 });
          const off = document.createElement("canvas");
          off.width = Math.max(1, Math.floor(vp.width));
          off.height = Math.max(1, Math.floor(vp.height));
          const ctx = off.getContext("2d");
          if (!ctx) continue;
          await page.render({ canvasContext: ctx, viewport: vp, canvas: off }).promise;
          if (cancelled) return;
          const dataUrl = off.toDataURL("image/png");
          setThumbnails((prev) => ({ ...prev, [p]: dataUrl }));

          if (p === 1 && !thumbSavedRef.current) {
            thumbSavedRef.current = true;
            const b64 = dataUrl.split(",")[1];
            if (b64) {
              void tauriClient.saveDocumentThumbnail(documentId, b64).catch(() => {});
            }
          }
        } catch {
          // Ignore individual thumbnail failure
        }
      }
    };
    void buildThumbs();

    return () => {
      cancelled = true;
    };
  }, [pdfDoc, showThumbs, documentId]);

  const handleExportExternal = async () => {
    try {
      const dest = await tauriClient.pickSavePath(attachment.originalFilename);
      if (!dest) return;
      await tauriClient.exportAttachment(attachment.id, dest);
    } catch (err) {
      setRenderError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleRenderPagePng = () => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    setCroppedPreviewUrl(canvas.toDataURL("image/png"));
  };

  const handleTriggerRecognize = async () => {
    if (!onRecognizePages) return;
    const cropB64 = croppedPreviewUrl?.includes(",")
      ? (croppedPreviewUrl.split(",")[1] ?? null)
      : null;
    if (cropB64) {
      onRecognizePages([], cropB64);
      return;
    }
    if (!pdfDoc) {
      onRecognizePages([], null);
      return;
    }
    const start = Math.max(1, pageRangeStart);
    const end = Math.min(pdfDoc.numPages, Math.max(start, pageRangeEnd), start + 4);
    const payloads: PageImagePayload[] = [];
    for (let p = start; p <= end; p++) {
      try {
        const page = await pdfDoc.getPage(p);
        const vp = page.getViewport({ scale: 1.5 });
        const off = document.createElement("canvas");
        off.width = Math.max(1, Math.floor(vp.width));
        off.height = Math.max(1, Math.floor(vp.height));
        const ctx = off.getContext("2d");
        let pngBase64 = "";
        if (ctx) {
          await page.render({ canvasContext: ctx, viewport: vp, canvas: off }).promise;
          pngBase64 = off.toDataURL("image/png").split(",")[1] ?? "";
        }
        const tc = await page.getTextContent();
        const text = tc.items
          .map((item) => ("str" in item ? item.str : ""))
          .filter(Boolean)
          .join(" ");
        payloads.push({ pageNumber: p, pngBase64, extractedText: text || null });
      } catch {
        // Continue remaining pages
      }
    }
    onRecognizePages(payloads, null);
  };

  const handleCanvasMouseDown = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!cropMode) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    setCropRect({ x0: x, y0: y, x1: x, y1: y });
    setDraggingCrop(true);
  };

  const handleCanvasMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!cropMode || !draggingCrop || !cropRect) return;
    const rect = e.currentTarget.getBoundingClientRect();
    setCropRect({
      ...cropRect,
      x1: Math.min(Math.max(0, e.clientX - rect.left), rect.width),
      y1: Math.min(Math.max(0, e.clientY - rect.top), rect.height),
    });
  };

  const handleCanvasMouseUp = () => {
    if (!cropMode || !draggingCrop || !cropRect || !canvasRef.current) {
      setDraggingCrop(false);
      return;
    }
    setDraggingCrop(false);
    const sx = Math.min(cropRect.x0, cropRect.x1);
    const sy = Math.min(cropRect.y0, cropRect.y1);
    const sw = Math.abs(cropRect.x1 - cropRect.x0);
    const sh = Math.abs(cropRect.y1 - cropRect.y0);
    if (sw < 12 || sh < 12) return;

    const out = document.createElement("canvas");
    out.width = Math.floor(sw);
    out.height = Math.floor(sh);
    const ctx = out.getContext("2d");
    if (ctx) {
      ctx.drawImage(canvasRef.current, sx, sy, sw, sh, 0, 0, sw, sh);
      setCroppedPreviewUrl(out.toDataURL("image/png"));
    }
  };

  const normalizedCropStyle = cropRect
    ? {
        left: `${Math.min(cropRect.x0, cropRect.x1)}px`,
        top: `${Math.min(cropRect.y0, cropRect.y1)}px`,
        width: `${Math.abs(cropRect.x1 - cropRect.x0)}px`,
        height: `${Math.abs(cropRect.y1 - cropRect.y0)}px`,
      }
    : undefined;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* PDF Toolbar */}
      <div className="flex flex-wrap items-center gap-1.5 border-b border-slate-200 bg-white px-3 py-1.5 dark:border-slate-800 dark:bg-slate-900">
        <Button
          variant={showThumbs ? "secondary" : "ghost"}
          size="xs"
          onClick={() => setShowThumbs((v) => !v)}
          title={t.viewer.thumbnails}
        >
          <PanelLeft className="h-3.5 w-3.5" />
          <span>{t.viewer.thumbnails}</span>
        </Button>

        <div className="mx-1 h-4 w-px bg-slate-200 dark:bg-slate-700" />

        <Button
          size="xs"
          disabled={currentPage <= 1}
          onClick={() => setCurrentPage(currentPage - 1)}
          aria-label={t.viewer.prevPage}
        >
          <ChevronLeft className="h-3.5 w-3.5" />
        </Button>
        <div className="flex items-center gap-1 text-xs">
          <Input
            value={pageInput}
            onChange={(e) => setPageInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                const n = Number.parseInt(pageInput, 10);
                if (!Number.isNaN(n)) setCurrentPage(n);
              }
            }}
            onBlur={() => {
              const n = Number.parseInt(pageInput, 10);
              if (!Number.isNaN(n)) setCurrentPage(n);
              else setPageInput(String(currentPage));
            }}
            className="h-6 w-12 text-center"
            aria-label="ページ番号"
          />
          <span className="text-slate-500">/ {totalPages}</span>
        </div>
        <Button
          size="xs"
          disabled={currentPage >= totalPages}
          onClick={() => setCurrentPage(currentPage + 1)}
          aria-label={t.viewer.nextPage}
        >
          <ChevronRight className="h-3.5 w-3.5" />
        </Button>

        <div className="mx-1 h-4 w-px bg-slate-200 dark:bg-slate-700" />

        <Button size="xs" onClick={() => setZoom(zoom - 0.15)} aria-label={t.viewer.zoomOut}>
          <ZoomOut className="h-3.5 w-3.5" />
        </Button>
        <span className="w-11 text-center text-xs text-slate-600 dark:text-slate-300">
          {Math.round(zoom * 100)}%
        </span>
        <Button size="xs" onClick={() => setZoom(zoom + 0.15)} aria-label={t.viewer.zoomIn}>
          <ZoomIn className="h-3.5 w-3.5" />
        </Button>

        <div className="mx-1 h-4 w-px bg-slate-200 dark:bg-slate-700" />

        <Button
          variant={showTextPanel ? "secondary" : "ghost"}
          size="xs"
          onClick={() => setShowTextPanel((v) => !v)}
        >
          <FileText className="h-3.5 w-3.5" />
          <span>{t.viewer.extractText}</span>
        </Button>

        <Button
          variant={showPreprocessBar || cropMode ? "secondary" : "ghost"}
          size="xs"
          onClick={() => setShowPreprocessBar((v) => !v)}
        >
          <Crop className="h-3.5 w-3.5" />
          <span>{t.viewer.preprocess}</span>
        </Button>

        {onRecognizePages && (
          <Button
            variant="primary"
            size="xs"
            disabled={recognizing}
            onClick={() => void handleTriggerRecognize()}
          >
            <Sparkles className="h-3.5 w-3.5" />
            <span>{recognizing ? "認識中..." : "問題認識"}</span>
          </Button>
        )}

        <Button
          variant="outline"
          size="xs"
          className="ml-auto"
          onClick={() => void handleExportExternal()}
        >
          <Download className="h-3.5 w-3.5" />
          <span>{t.viewer.exportExternal}</span>
        </Button>
      </div>

      {/* Preprocessing Bar (Page Range + Page-to-PNG + Region Crop) */}
      {showPreprocessBar && (
        <div className="flex flex-wrap items-center gap-3 border-b border-slate-200 bg-slate-50 px-3 py-1.5 text-xs dark:border-slate-800 dark:bg-slate-900/60">
          <div className="flex items-center gap-1.5">
            <span className="text-slate-600 dark:text-slate-300">{t.viewer.pageRangeLabel}:</span>
            <Input
              type="number"
              min={1}
              max={totalPages}
              value={pageRangeStart}
              onChange={(e) => setPageRange(Number(e.target.value), pageRangeEnd)}
              className="h-6 w-14 text-center"
            />
            <span>〜</span>
            <Input
              type="number"
              min={pageRangeStart}
              max={totalPages}
              value={pageRangeEnd}
              onChange={(e) => setPageRange(pageRangeStart, Number(e.target.value))}
              className="h-6 w-14 text-center"
            />
          </div>

          <Button
            size="xs"
            variant={cropMode ? "primary" : "outline"}
            onClick={() => {
              setCropMode((v) => !v);
              setCropRect(null);
            }}
          >
            <Crop className="h-3.5 w-3.5" />
            <span>{cropMode ? t.viewer.cropModeOn : t.viewer.cropModeStart}</span>
          </Button>

          <Button size="xs" variant="outline" onClick={handleRenderPagePng}>
            <Camera className="h-3.5 w-3.5" />
            <span>{t.viewer.renderPagePng}</span>
          </Button>
        </div>
      )}

      {renderError && (
        <div className="bg-red-50 px-3 py-2 text-xs text-red-700 dark:bg-red-950/60 dark:text-red-300">
          PDF 描画エラー: {renderError}
        </div>
      )}

      {/* Main PDF Body */}
      <div className="flex min-h-0 flex-1 overflow-hidden">
        {showThumbs && (
          <div className="w-32 shrink-0 overflow-y-auto border-r border-slate-200 bg-slate-100/80 p-2 space-y-2 dark:border-slate-800 dark:bg-slate-900/50">
            {Array.from({ length: totalPages }, (_, idx) => idx + 1).map((p) => (
              <button
                key={p}
                type="button"
                onClick={() => setCurrentPage(p)}
                className={cn(
                  "flex w-full flex-col items-center rounded border p-1 transition-colors",
                  currentPage === p
                    ? "border-blue-500 bg-blue-50 dark:bg-blue-950/60"
                    : "border-slate-200 bg-white hover:border-slate-400 dark:border-slate-700 dark:bg-slate-900",
                )}
              >
                {thumbnails[p] ? (
                  <img
                    src={thumbnails[p]}
                    alt={`Page ${p}`}
                    className="w-full rounded-xs object-contain"
                  />
                ) : (
                  <div className="flex h-24 w-full items-center justify-center text-[10px] text-slate-400">
                    {p}
                  </div>
                )}
                <span className="mt-1 text-[10px] text-slate-600 dark:text-slate-400">{p}</span>
              </button>
            ))}
          </div>
        )}

        <div className="flex-1 overflow-auto bg-slate-200/70 p-4 dark:bg-slate-950">
          <div className="mx-auto flex flex-col items-center gap-4">
            <div
              onMouseDown={handleCanvasMouseDown}
              onMouseMove={handleCanvasMouseMove}
              onMouseUp={handleCanvasMouseUp}
              className={cn(
                "relative inline-block bg-white shadow-md",
                cropMode && "cursor-crosshair select-none",
              )}
            >
              <canvas ref={canvasRef} className="block" />
              {!cropMode && <div ref={textLayerRef} className="pdf-text-layer" />}
              {cropMode && normalizedCropStyle && (
                <div
                  style={normalizedCropStyle}
                  className="pointer-events-none absolute border-2 border-blue-500 bg-blue-500/15"
                />
              )}
            </div>

            {croppedPreviewUrl && (
              <div className="w-full max-w-2xl rounded-lg border border-blue-300 bg-white p-3 shadow-xs dark:border-blue-800 dark:bg-slate-900">
                <div className="mb-2 flex items-center justify-between">
                  <span className="text-xs font-semibold text-blue-700 dark:text-blue-300">
                    {t.viewer.cropPreviewTitle}
                  </span>
                  <Button size="xs" variant="ghost" onClick={() => setCroppedPreviewUrl(null)}>
                    {t.common.close}
                  </Button>
                </div>
                <img
                  src={croppedPreviewUrl}
                  alt="Cropped region preview"
                  className="max-h-72 rounded border border-slate-200 object-contain dark:border-slate-700"
                />
              </div>
            )}

            {showTextPanel && (
              <div className="w-full max-w-2xl rounded-lg border border-slate-300 bg-white p-3 dark:border-slate-700 dark:bg-slate-900">
                <div className="mb-1.5 text-xs font-semibold text-slate-700 dark:text-slate-200">
                  {t.viewer.extractedTextTitle}
                </div>
                <pre className="max-h-48 overflow-auto whitespace-pre-wrap text-xs text-slate-700 dark:text-slate-300">
                  {extractedPageText || t.viewer.noTextFound}
                </pre>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
