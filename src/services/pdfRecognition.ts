import * as pdfjs from "pdfjs-dist";
import pdfWorkerUrl from "pdfjs-dist/build/pdf.worker.mjs?url";
import { base64ToUint8Array } from "@/lib/utils";
import type { PageImagePayload } from "@/types/ipc";

pdfjs.GlobalWorkerOptions.workerSrc = pdfWorkerUrl;

export function loadPdf(base64: string) {
  return pdfjs.getDocument({ data: base64ToUint8Array(base64) });
}

export async function preparePdfRecognition(
  base64: string,
  range: { start: number; end: number } | null,
): Promise<PageImagePayload[]> {
  const task = loadPdf(base64);
  try {
    const pdf = await task.promise;
    const { start, end } = range ?? { start: 1, end: pdf.numPages };
    if (!Number.isInteger(start) || !Number.isInteger(end) || start < 1 || end < start || end > pdf.numPages) {
      throw new Error("Invalid PDF page range");
    }
    const pages: PageImagePayload[] = [];
    for (let number = start; number <= end; number++) {
      const page = await pdf.getPage(number);
      const viewport = page.getViewport({ scale: 1.5 });
      const canvas = document.createElement("canvas");
      canvas.width = Math.max(1, Math.floor(viewport.width));
      canvas.height = Math.max(1, Math.floor(viewport.height));
      const canvasContext = canvas.getContext("2d");
      if (!canvasContext) throw new Error("Could not prepare the PDF canvas");
      await page.render({ canvasContext, viewport, canvas }).promise;
      const pngBase64 = canvas.toDataURL("image/png").split(",")[1];
      if (!pngBase64) throw new Error("Could not encode the PDF page image");
      const content = await page.getTextContent();
      const extractedText = content.items.map((item) => "str" in item ? item.str : "").join(" ").trim();
      pages.push({ pageNumber: number, pngBase64, extractedText: extractedText || null });
    }
    return pages;
  } finally {
    await task.destroy();
  }
}
