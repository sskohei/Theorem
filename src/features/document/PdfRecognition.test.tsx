import { invoke } from "@tauri-apps/api/core";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CenterWorkspace } from "./CenterWorkspace";
import { base64ToUint8Array } from "@/lib/utils";
import { useAiWorkspaceStore } from "@/stores/useAiWorkspaceStore";
import { useDocumentStore } from "@/stores/useDocumentStore";
import { useLayoutStore } from "@/stores/useLayoutStore";
import { useLibraryStore } from "@/stores/useLibraryStore";
import type { DocumentDetail, RecognizeProblemsRequest } from "@/types/ipc";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), isTauri: () => true }));
vi.mock("pdfjs-dist", async () => {
  const legacy = await import("pdfjs-dist/legacy/build/pdf.mjs");
  // PDF.js uses its built-in Node worker loader under Vitest.
  return { ...legacy, GlobalWorkerOptions: { workerSrc: "" } };
});

interface RasterCanvas {
  width: number;
  height: number;
  getContext(kind: "2d"): CanvasRenderingContext2D;
  toDataURL(type: "image/png"): string;
}

interface NativeCanvas {
  createCanvas(width: number, height: number): RasterCanvas;
  loadImage(bytes: Uint8Array): Promise<CanvasImageSource>;
}

interface ModuleRequire {
  (id: "@napi-rs/canvas"): NativeCanvas;
  resolve(id: string): string;
}

const { createRequire } = await vi.importActual<{
  createRequire: (path: string) => ModuleRequire;
}>("node:module");
const require = createRequire(import.meta.url);
const resolvePdfDependency = createRequire(require.resolve("pdfjs-dist/package.json"));
let raster: NativeCanvas | undefined;
try {
  // Reuse the optional renderer PDF.js itself installs for Node.
  raster = resolvePdfDependency("@napi-rs/canvas");
} catch {
  // PDF.js does not install a native binary for every platform.
}

function scannedPdf(): string {
  const objects = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Count 3 /Kids [3 0 R 5 0 R 7 0 R] >>",
  ];
  for (const color of ["FF0000", "00FF00", "0000FF"]) {
    const contents = "q 16 0 0 16 0 0 cm BI /W 1 /H 1 /BPC 8 /CS /RGB /F /AHx ID " + color + "> EI Q\n";
    objects.push(
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 16 16] /Contents " + (objects.length + 2) + " 0 R >>",
      "<< /Length " + contents.length + " >>\nstream\n" + contents + "endstream",
    );
  }
  let data = "%PDF-1.4\n";
  const offsets = objects.map((object, index) => {
    const offset = data.length;
    data += (index + 1) + " 0 obj\n" + object + "\nendobj\n";
    return offset;
  });
  const xref = data.length;
  data += "xref\n0 8\n0000000000 65535 f \n";
  data += offsets.map((offset) => String(offset).padStart(10, "0") + " 00000 n \n").join("");
  data += "trailer\n<< /Size 8 /Root 1 0 R >>\nstartxref\n" + xref + "\n%%EOF\n";
  return btoa(data);
}

const date = "2026-11-26T00:00:00Z";
const scan: DocumentDetail = {
  document: {
    id: "scan", subjectId: "math", folderId: null, title: "Scan", sourceType: "pdf",
    primaryAttachmentId: "original", thumbnailAttachmentId: null, pageCount: 3, recognitionMode: "printed_only",
    studyStatus: "unstarted", isFavorite: false, lastOpenedAt: null, tagIds: [], createdAt: date, updatedAt: date,
  },
  attachments: [{
    id: "original", documentId: "scan", kind: "original_pdf", originalFilename: "scan.pdf",
    relativePath: "attachments/originals/scan.pdf", mimeType: "application/pdf",
    byteSize: 1000, sha256: "fixture", width: null, height: null, pageCount: 3, createdAt: date,
  }],
  tags: [],
};

function openRecognitionTab(base64: string) {
  useLibraryStore.setState({ selectedDocumentId: scan.document.id, error: null });
  useAiWorkspaceStore.setState({ activeDocumentId: scan.document.id, problems: [], error: null, recognizing: false });
  useDocumentStore.setState({
    detail: scan, primaryBase64: base64, loading: false, error: null,
    currentPage: 1, totalPages: 3, pageRangeStart: 1, pageRangeEnd: 3,
    extractedPageText: "", croppedPreviewUrl: null, croppedPage: null,
  });
  useLayoutStore.setState({ activeTab: "recognized" });
  render(<CenterWorkspace onImportClick={() => {}} />);
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.mocked(invoke).mockReset();
});

describe("PDF recognition tab IPC", () => {
  it.skipIf(!raster)("sends rendered pages 2 and 3 for an image-only PDF without text extraction", async () => {
    const native = raster!;
    const canvases = new WeakMap<HTMLCanvasElement, RasterCanvas>();
    const backing = (element: HTMLCanvasElement) => {
      let canvas = canvases.get(element);
      if (!canvas) {
        canvas = native.createCanvas(element.width, element.height);
        canvases.set(element, canvas);
      }
      return canvas;
    };
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockImplementation(function (this: HTMLCanvasElement) {
      return backing(this).getContext("2d");
    });
    vi.spyOn(HTMLCanvasElement.prototype, "toDataURL").mockImplementation(function (this: HTMLCanvasElement) {
      return backing(this).toDataURL("image/png");
    });
    vi.mocked(invoke).mockResolvedValue({ problems: [], schemaIssues: [], rawOutput: null, runId: "recognition" });

    openRecognitionTab(scannedPdf());
    const user = userEvent.setup();
    await user.selectOptions(screen.getByLabelText("問題認識モード"), "page_range");
    const start = screen.getByRole("spinbutton", { name: "認識開始ページ" });
    await user.clear(start);
    await user.type(start, "2");
    await user.click(screen.getByRole("button", { name: "AI で問題認識" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledTimes(1));
    const [command, args] = vi.mocked(invoke).mock.calls[0]!;
    expect(command).toBe("recognize_problems");
    const input = (args as { input: RecognizeProblemsRequest }).input;
    expect(input).toMatchObject({
      documentId: "scan", recognitionMode: "page_range", pageRangeStart: 2, pageRangeEnd: 3,
      pageImages: [{ pageNumber: 2, extractedText: null }, { pageNumber: 3, extractedText: null }],
      croppedRegionPngBase64: null,
    });
    const pixels = await Promise.all(input.pageImages.map(async ({ pngBase64 }) => {
      const image = await native.loadImage(base64ToUint8Array(pngBase64));
      const ctx = native.createCanvas(24, 24).getContext("2d");
      ctx.drawImage(image, 0, 0);
      return Array.from(ctx.getImageData(12, 12, 1, 1).data);
    }));
    expect(pixels).toEqual([[0, 255, 0, 255], [0, 0, 255, 255]]);
    await waitFor(() => expect(useAiWorkspaceStore.getState().recognizing).toBe(false));
  });

  it("uses the recorded crop page even after the viewer moves to another page", async () => {
    const png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Y9Z6zcAAAAASUVORK5CYII=";
    vi.mocked(invoke).mockResolvedValue({ problems: [], schemaIssues: [], rawOutput: null, runId: "recognition" });
    openRecognitionTab(scannedPdf());
    act(() => useDocumentStore.setState({
      currentPage: 3, croppedPreviewUrl: "data:image/png;base64," + png, croppedPage: 2,
    }));

    const user = userEvent.setup();
    await user.selectOptions(screen.getByLabelText("問題認識モード"), "page_range");
    await user.click(screen.getByRole("button", { name: "AI で問題認識" }));
    expect(invoke).toHaveBeenCalledExactlyOnceWith("recognize_problems", {
      input: {
        documentId: "scan", recognitionMode: "page_range", pageRangeStart: 2, pageRangeEnd: 2,
        pageImages: [], croppedRegionPngBase64: png,
      },
    });
    await waitFor(() => expect(useAiWorkspaceStore.getState().recognizing).toBe(false));
  });

  it("displays a preparation error and sends no IPC when the PDF cannot be loaded", async () => {
    openRecognitionTab(btoa("not a PDF"));
    await userEvent.setup().click(screen.getByRole("button", { name: "AI で問題認識" }));
    expect(await screen.findByText(/Invalid PDF structure/)).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalled();
  });
});
