import { beforeEach, describe, expect, it, vi } from "vitest";
import { tauriClient } from "@/services/tauriClient";
import { useDocumentStore } from "./useDocumentStore";
import { useLayoutStore } from "./useLayoutStore";
import { useLibraryStore } from "./useLibraryStore";

vi.mock("@/services/tauriClient", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/services/tauriClient")>();
  return {
    ...actual,
    tauriClient: {
      ...actual.tauriClient,
      listDocuments: vi.fn(),
      importFilesFromPaths: vi.fn(),
    },
  };
});

describe("Zustand stores (Layout, Document, Library)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useLayoutStore.setState({
      leftSidebarOpen: true,
      rightSidebarOpen: true,
      leftSidebarWidth: 280,
      rightSidebarWidth: 320,
      activeTab: "original",
      theme: "system",
      settingsOpen: false,
      settingsCategory: "general",
    });
    useDocumentStore.setState({
      detail: null,
      primaryBase64: null,
      loading: false,
      error: null,
      currentPage: 1,
      totalPages: 5,
      zoom: 1.15,
      rotation: 0,
      pageRangeStart: 1,
      pageRangeEnd: 5,
      extractedPageText: "",
      croppedPreviewUrl: null,
    });
    useLibraryStore.setState({
      subjects: [
        {
          id: "sub-1",
          name: "大学数学",
          category: "mathematics",
          description: null,
          sortOrder: 0,
          createdAt: "2026-11-26T00:00:00Z",
          updatedAt: "2026-11-26T00:00:00Z",
        },
      ],
      folders: [],
      tags: [],
      documents: [],
      selectedSubjectId: null,
      selectedFolderId: null,
      selectedTagId: null,
      searchQuery: "",
      selectedDocumentId: null,
      duplicateNotice: null,
      error: null,
    });
  });

  it("clamps sidebar widths and toggles visibility in useLayoutStore", () => {
    const layout = useLayoutStore.getState();
    layout.setLeftSidebarWidth(50);
    expect(useLayoutStore.getState().leftSidebarWidth).toBe(220);
    layout.setLeftSidebarWidth(999);
    expect(useLayoutStore.getState().leftSidebarWidth).toBe(460);

    layout.setRightSidebarWidth(100);
    expect(useLayoutStore.getState().rightSidebarWidth).toBe(240);
    layout.setRightSidebarWidth(999);
    expect(useLayoutStore.getState().rightSidebarWidth).toBe(520);

    layout.resetSidebarWidths();
    expect(useLayoutStore.getState().leftSidebarWidth).toBe(280);
    expect(useLayoutStore.getState().rightSidebarWidth).toBe(320);
  });

  it("clamps page navigation, page range, zoom, and rotation in useDocumentStore", () => {
    const doc = useDocumentStore.getState();
    doc.setCurrentPage(99);
    expect(useDocumentStore.getState().currentPage).toBe(5);
    doc.setCurrentPage(-3);
    expect(useDocumentStore.getState().currentPage).toBe(1);

    doc.setZoom(10);
    expect(useDocumentStore.getState().zoom).toBe(3);
    doc.setZoom(0.1);
    expect(useDocumentStore.getState().zoom).toBe(0.5);

    doc.rotateClockwise();
    doc.rotateClockwise();
    doc.rotateClockwise();
    doc.rotateClockwise();
    expect(useDocumentStore.getState().rotation).toBe(0);

    doc.setPageRange(4, 2);
    expect(useDocumentStore.getState().pageRangeStart).toBe(4);
    expect(useDocumentStore.getState().pageRangeEnd).toBe(4);
  });

  it("surfaces partial import failure and duplicate matches in useLibraryStore", async () => {
    vi.mocked(tauriClient.listDocuments).mockResolvedValue([]);
    vi.mocked(tauriClient.importFilesFromPaths).mockResolvedValue({
      imported: [
        {
          detail: {
            document: {
              id: "doc-new",
              subjectId: "sub-1",
              folderId: null,
              title: "exam",
              sourceType: "pdf",
              primaryAttachmentId: "att-new",
              thumbnailAttachmentId: null,
              pageCount: 2,
              recognitionMode: "printed_only",
              studyStatus: "unstarted",
              isFavorite: false,
              lastOpenedAt: null,
              tagIds: [],
              createdAt: "2026-11-26T00:00:00Z",
              updatedAt: "2026-11-26T00:00:00Z",
            },
            attachments: [],
            tags: [],
          },
          duplicateMatches: [
            {
              documentId: "doc-old",
              documentTitle: "既存試験",
              attachmentId: "att-old",
              originalFilename: "exam.pdf",
              sha256: "deadbeef",
            },
          ],
        },
      ],
      failures: [{ filename: "bad.txt", reason: "非対応形式" }],
    });

    await useLibraryStore.getState().importFromPaths(["C:\\exam.pdf", "C:\\bad.txt"]);
    const st = useLibraryStore.getState();
    expect(st.selectedDocumentId).toBe("doc-new");
    expect(st.duplicateNotice).toHaveLength(1);
    expect(st.error?.code).toBe("PARTIAL_IMPORT_FAILURE");
  });
});
