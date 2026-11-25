import * as pdfjsLib from "pdfjs-dist";
import pdfWorkerUrl from "pdfjs-dist/build/pdf.worker.mjs?url";

pdfjsLib.GlobalWorkerOptions.workerSrc = pdfWorkerUrl;

let currentDoc: pdfjsLib.PDFDocumentProxy | null = null;
let currentPage = 1;
let scale = 1.25;

const fileInput = document.getElementById("file-input") as HTMLInputElement;
const prevBtn = document.getElementById("prev-btn") as HTMLButtonElement;
const nextBtn = document.getElementById("next-btn") as HTMLButtonElement;
const zoomInBtn = document.getElementById("zoom-in") as HTMLButtonElement;
const zoomOutBtn = document.getElementById("zoom-out") as HTMLButtonElement;
const exportPngBtn = document.getElementById("export-png") as HTMLButtonElement;
const pageInfo = document.getElementById("page-info") as HTMLSpanElement;
const zoomInfo = document.getElementById("zoom-info") as HTMLSpanElement;
const thumbsEl = document.getElementById("thumbs") as HTMLElement;
const canvas = document.getElementById("pdf-canvas") as HTMLCanvasElement;
const textLayerEl = document.getElementById("text-layer") as HTMLDivElement;
const extractedTextEl = document.getElementById("extracted-text") as HTMLPreElement;

async function renderPage(num: number): Promise<void> {
  if (!currentDoc) return;
  const page = await currentDoc.getPage(num);
  const viewport = page.getViewport({ scale });
  const ctx = canvas.getContext("2d");
  if (!ctx) return;

  canvas.width = Math.floor(viewport.width);
  canvas.height = Math.floor(viewport.height);
  await page.render({ canvasContext: ctx, viewport, canvas }).promise;

  // Text extraction + selectable text layer PoC
  const textContent = await page.getTextContent();
  textLayerEl.innerHTML = "";
  const lines: string[] = [];
  for (const item of textContent.items) {
    if ("str" in item && item.str.trim()) {
      lines.push(item.str);
      const span = document.createElement("span");
      span.textContent = item.str;
      const tx = pdfjsLib.Util.transform(viewport.transform, item.transform);
      span.style.left = `${tx[4]}px`;
      span.style.top = `${tx[5] - Math.hypot(tx[2], tx[3])}px`;
      span.style.fontSize = `${Math.hypot(tx[2], tx[3])}px`;
      textLayerEl.appendChild(span);
    }
  }
  extractedTextEl.textContent = lines.join(" ");
  pageInfo.textContent = `${currentPage} / ${currentDoc.numPages}`;
  zoomInfo.textContent = `${Math.round(scale * 100)}%`;
}

async function renderThumbnails(doc: pdfjsLib.PDFDocumentProxy): Promise<void> {
  thumbsEl.innerHTML = "";
  for (let p = 1; p <= doc.numPages; p++) {
    const page = await doc.getPage(p);
    const vp = page.getViewport({ scale: 0.25 });
    const c = document.createElement("canvas");
    c.width = Math.floor(vp.width);
    c.height = Math.floor(vp.height);
    const ctx = c.getContext("2d");
    if (ctx) {
      await page.render({ canvasContext: ctx, viewport: vp, canvas: c }).promise;
    }
    c.addEventListener("click", () => {
      currentPage = p;
      void renderPage(currentPage);
    });
    thumbsEl.appendChild(c);
  }
}

fileInput.addEventListener("change", async () => {
  const file = fileInput.files?.[0];
  if (!file) return;
  const data = new Uint8Array(await file.arrayBuffer());
  currentDoc = await pdfjsLib.getDocument({ data }).promise;
  currentPage = 1;
  await renderThumbnails(currentDoc);
  await renderPage(currentPage);
});

prevBtn.addEventListener("click", () => {
  if (currentDoc && currentPage > 1) {
    currentPage -= 1;
    void renderPage(currentPage);
  }
});

nextBtn.addEventListener("click", () => {
  if (currentDoc && currentPage < currentDoc.numPages) {
    currentPage += 1;
    void renderPage(currentPage);
  }
});

zoomInBtn.addEventListener("click", () => {
  scale = Math.min(3.0, scale + 0.25);
  void renderPage(currentPage);
});

zoomOutBtn.addEventListener("click", () => {
  scale = Math.max(0.5, scale - 0.25);
  void renderPage(currentPage);
});

exportPngBtn.addEventListener("click", () => {
  const url = canvas.toDataURL("image/png");
  const a = document.createElement("a");
  a.href = url;
  a.download = `page-${currentPage}.png`;
  a.click();
});
