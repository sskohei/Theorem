import { create } from "zustand";
import { tauriClient, normalizeApiError } from "@/services/tauriClient";
import type { ApiError, DocumentDetail } from "@/types/ipc";

interface DocumentViewerState {
  detail: DocumentDetail | null;
  primaryBase64: string | null;
  loading: boolean;
  error: ApiError | null;

  currentPage: number;
  totalPages: number;
  zoom: number;
  rotation: number;
  pageRangeStart: number;
  pageRangeEnd: number;
  extractedPageText: string;
  croppedPreviewUrl: string | null;

  loadDocument: (documentId: string | null) => Promise<void>;
  setCurrentPage: (page: number) => void;
  setTotalPages: (total: number) => void;
  setZoom: (zoom: number) => void;
  rotateClockwise: () => void;
  setPageRange: (start: number, end: number) => void;
  setExtractedPageText: (text: string) => void;
  setCroppedPreviewUrl: (url: string | null) => void;
}

export const useDocumentStore = create<DocumentViewerState>((set) => ({
  detail: null,
  primaryBase64: null,
  loading: false,
  error: null,

  currentPage: 1,
  totalPages: 1,
  zoom: 1.15,
  rotation: 0,
  pageRangeStart: 1,
  pageRangeEnd: 1,
  extractedPageText: "",
  croppedPreviewUrl: null,

  loadDocument: async (documentId) => {
    if (!documentId) {
      set({
        detail: null,
        primaryBase64: null,
        loading: false,
        error: null,
        currentPage: 1,
        totalPages: 1,
        croppedPreviewUrl: null,
        extractedPageText: "",
      });
      return;
    }

    set({ loading: true, error: null, croppedPreviewUrl: null, extractedPageText: "" });
    try {
      const detail = await tauriClient.getDocumentDetail(documentId);
      const primaryId = detail.document.primaryAttachmentId;
      let primaryBase64: string | null = null;
      if (primaryId) {
        const bin = await tauriClient.readAttachmentBase64(primaryId);
        primaryBase64 = bin.base64Data;
      }
      const pages = detail.document.pageCount ?? 1;
      set({
        detail,
        primaryBase64,
        loading: false,
        currentPage: 1,
        totalPages: pages,
        pageRangeStart: 1,
        pageRangeEnd: pages,
        rotation: 0,
      });
    } catch (err) {
      set({ loading: false, error: normalizeApiError(err) });
    }
  },

  setCurrentPage: (page) =>
    set((s) => ({ currentPage: Math.min(Math.max(1, page), Math.max(1, s.totalPages)) })),
  setTotalPages: (total) =>
    set((s) => ({
      totalPages: Math.max(1, total),
      pageRangeEnd: Math.min(Math.max(1, s.pageRangeEnd), Math.max(1, total)),
    })),
  setZoom: (zoom) => set({ zoom: Math.min(3.0, Math.max(0.5, Number(zoom.toFixed(2)))) }),
  rotateClockwise: () => set((s) => ({ rotation: (s.rotation + 90) % 360 })),
  setPageRange: (start, end) =>
    set((s) => {
      const clampedStart = Math.min(Math.max(1, start), s.totalPages);
      const clampedEnd = Math.min(Math.max(clampedStart, end), s.totalPages);
      return { pageRangeStart: clampedStart, pageRangeEnd: clampedEnd };
    }),
  setExtractedPageText: (text) => set({ extractedPageText: text }),
  setCroppedPreviewUrl: (url) => set({ croppedPreviewUrl: url }),
}));
