import { useState } from "react";
import { Download, RotateCw, Sparkles, ZoomIn, ZoomOut } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { useI18n } from "@/lib/i18n";
import { formatBytes } from "@/lib/utils";
import { normalizeApiError, tauriClient } from "@/services/tauriClient";
import { useDocumentStore } from "@/stores/useDocumentStore";
import type { AttachmentInfo } from "@/types/ipc";

interface ImageViewerProps {
  attachment: AttachmentInfo;
  base64Data: string;
  onRecognizeImage?: () => void;
  recognizing?: boolean;
}

export function ImageViewer({
  attachment,
  base64Data,
  onRecognizeImage,
  recognizing = false,
}: ImageViewerProps) {
  const t = useI18n();
  const { zoom, rotation, setZoom, rotateClockwise } = useDocumentStore();
  const [exportError, setExportError] = useState<string | null>(null);
  const dataUrl = `data:${attachment.mimeType};base64,${base64Data}`;

  const handleExport = async () => {
    setExportError(null);
    try {
      const dest = await tauriClient.pickSavePath(attachment.originalFilename);
      if (!dest) return;
      await tauriClient.exportAttachment(attachment.id, dest);
    } catch (err) {
      setExportError(normalizeApiError(err).message);
    }
  };

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <div className="flex flex-wrap items-center gap-1.5 border-b border-slate-200 bg-white px-3 py-1.5 dark:border-slate-800 dark:bg-slate-900">
        <Button size="xs" onClick={() => setZoom(zoom - 0.2)} aria-label={t.viewer.zoomOut}>
          <ZoomOut className="h-3.5 w-3.5" />
        </Button>
        <span className="w-12 text-center text-xs text-slate-600 dark:text-slate-300">
          {Math.round(zoom * 100)}%
        </span>
        <Button size="xs" onClick={() => setZoom(zoom + 0.2)} aria-label={t.viewer.zoomIn}>
          <ZoomIn className="h-3.5 w-3.5" />
        </Button>

        <Button size="xs" onClick={rotateClockwise} title={t.viewer.rotate}>
          <RotateCw className="h-3.5 w-3.5" />
          <span>{t.viewer.rotate}</span>
        </Button>

        <span className="ml-2 text-xs text-slate-500 dark:text-slate-400">
          {attachment.width && attachment.height
            ? `${attachment.width} × ${attachment.height}px`
            : ""}
          {" · "}
          {formatBytes(attachment.byteSize)}
        </span>

        {onRecognizeImage && (
          <Button
            variant="primary"
            size="xs"
            disabled={recognizing}
            onClick={onRecognizeImage}
          >
            <Sparkles className="h-3.5 w-3.5" />
            <span>{recognizing ? "認識中..." : "問題認識"}</span>
          </Button>
        )}

        <Button
          variant="outline"
          size="xs"
          className="ml-auto"
          onClick={() => void handleExport()}
        >
          <Download className="h-3.5 w-3.5" />
          <span>{t.viewer.exportExternal}</span>
        </Button>
      </div>

      {exportError && (
        <div
          role="alert"
          className="bg-red-50 px-3 py-1.5 text-xs text-red-700 dark:bg-red-950/60 dark:text-red-300"
        >
          {exportError}
        </div>
      )}

      <div className="flex flex-1 items-center justify-center overflow-auto bg-slate-200/70 p-6 dark:bg-slate-950">
        <img
          src={dataUrl}
          alt={attachment.originalFilename}
          style={{
            transform: `scale(${zoom}) rotate(${rotation}deg)`,
            transformOrigin: "center center",
          }}
          className="max-h-full max-w-full rounded bg-white shadow-md transition-transform"
        />
      </div>
    </div>
  );
}
