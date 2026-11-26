import { create } from "zustand";
import { tauriClient, normalizeApiError } from "@/services/tauriClient";
import type {
  ApiError,
  AppSettings,
  BootstrapState,
  DocumentFilter,
  DocumentSummary,
  DuplicateMatch,
  Folder,
  FullTextSearchHit,
  StudyStatus,
  Subject,
  SubjectCategory,
  Tag,
} from "@/types/ipc";
import { useLayoutStore } from "./useLayoutStore";

export type LibraryQuickView = "all" | "favorites" | "recent";

interface LibraryState {
  initialized: boolean;
  loading: boolean;
  importing: boolean;
  appVersion: string;
  dataRootDisplay: string;
  subjects: Subject[];
  folders: Folder[];
  tags: Tag[];
  documents: DocumentSummary[];
  settings: AppSettings;

  quickView: LibraryQuickView;
  selectedSubjectId: string | null;
  selectedFolderId: string | null;
  selectedTagId: string | null;
  selectedStudyStatus: StudyStatus | null;
  searchQuery: string;
  ftsHits: FullTextSearchHit[];
  ftsLoading: boolean;
  selectedDocumentId: string | null;

  duplicateNotice: DuplicateMatch[] | null;
  error: ApiError | null;

  applyBootstrap: (boot: BootstrapState) => void;
  bootstrap: () => Promise<void>;
  refreshDocuments: () => Promise<void>;
  setQuickView: (view: LibraryQuickView) => Promise<void>;
  selectSubjectFilter: (subjectId: string | null) => Promise<void>;
  selectFolderFilter: (subjectId: string, folderId: string | null) => Promise<void>;
  selectTagFilter: (tagId: string | null) => Promise<void>;
  setStudyStatusFilter: (status: StudyStatus | null) => Promise<void>;
  setSearchQuery: (query: string) => Promise<void>;
  runFullTextSearch: (query: string) => Promise<void>;
  clearFullTextSearch: () => void;
  selectDocument: (documentId: string | null) => void;

  createSubject: (name: string, category: SubjectCategory, description?: string) => Promise<void>;
  updateSubject: (
    id: string,
    name: string,
    category: SubjectCategory,
    description?: string,
  ) => Promise<void>;
  deleteSubject: (id: string) => Promise<void>;

  createFolder: (subjectId: string, parentId: string | null, name: string) => Promise<void>;
  updateFolder: (id: string, name: string, parentId: string | null) => Promise<void>;
  setFolderAutoUpload: (id: string, autoUploadDrive: boolean) => Promise<void>;
  deleteFolder: (id: string) => Promise<void>;

  createTag: (name: string, color?: string) => Promise<void>;
  deleteTag: (id: string) => Promise<void>;
  setDocumentTags: (documentId: string, tagIds: string[]) => Promise<void>;

  toggleFavorite: (doc: DocumentSummary) => Promise<void>;
  setDocumentStudyStatus: (id: string, studyStatus: StudyStatus) => Promise<void>;
  renameDocument: (id: string, title: string) => Promise<void>;
  moveDocument: (id: string, subjectId: string, folderId: string | null) => Promise<void>;
  deleteDocument: (id: string) => Promise<void>;

  importFromPaths: (paths: string[]) => Promise<void>;
  importFromBytes: (filename: string, base64Data: string) => Promise<void>;
  saveSettings: (next: AppSettings) => Promise<void>;

  dismissDuplicateNotice: () => void;
  clearError: () => void;
}

const DEFAULT_SETTINGS: AppSettings = {
  language: "ja",
  theme: "system",
  defaultSubjectId: null,
  defaultRecognitionMode: "printed_only",
  leftSidebarWidth: 280,
  rightSidebarWidth: 320,
  cloudAiConsentAccepted: false,
  pdf: {
    defaultTemplate: "auto",
    paperSize: "a4",
    showModelName: true,
    showGeneratedAt: true,
    showVersionNumber: true,
  },
  agentAssignments: {
    extractionProfileId: null,
    solutionProfileId: null,
    chatProfileId: null,
    verificationProfileId: null,
    revisionProfileId: null,
  },
  prompts: {
    mathematicsCustomInstructions: "",
    physicsCustomInstructions: "",
    modeCustomInstructions: "",
  },
  scienceMcp: {
    enabled: true,
    uvPath: null,
    serverDir: null,
    timeoutSeconds: 10,
    defaultPlotFormat: "svg",
  },
  drive: {
    oauthClientId: null,
  },
};

function buildFilter(s: LibraryState): DocumentFilter {
  return {
    subjectId: s.selectedSubjectId,
    folderId: s.selectedFolderId,
    tagId: s.selectedTagId,
    searchQuery: s.searchQuery.trim() ? s.searchQuery.trim() : null,
    studyStatus: s.selectedStudyStatus,
    favoritesOnly: s.quickView === "favorites",
    recentOnly: s.quickView === "recent",
  };
}

function resolveImportTarget(s: LibraryState): { subjectId: string; folderId: string | null } | null {
  const subjectId =
    s.selectedSubjectId ??
    s.settings.defaultSubjectId ??
    s.subjects[0]?.id ??
    null;
  if (!subjectId) return null;
  const folderId = s.selectedSubjectId === subjectId ? s.selectedFolderId : null;
  return { subjectId, folderId };
}

export const useLibraryStore = create<LibraryState>((set, get) => ({
  initialized: false,
  loading: false,
  importing: false,
  appVersion: "0.1.0",
  dataRootDisplay: "",
  subjects: [],
  folders: [],
  tags: [],
  documents: [],
  settings: DEFAULT_SETTINGS,

  quickView: "all",
  selectedSubjectId: null,
  selectedFolderId: null,
  selectedTagId: null,
  selectedStudyStatus: null,
  searchQuery: "",
  ftsHits: [],
  ftsLoading: false,
  selectedDocumentId: null,

  duplicateNotice: null,
  error: null,

  applyBootstrap: (boot) => {
    const layout = useLayoutStore.getState();
    layout.setTheme(boot.settings.theme);
    layout.setLeftSidebarWidth(boot.settings.leftSidebarWidth);
    layout.setRightSidebarWidth(boot.settings.rightSidebarWidth);
    set((prev) => ({
      initialized: true,
      loading: false,
      appVersion: boot.appVersion,
      dataRootDisplay: boot.dataRootDisplay,
      subjects: boot.subjects,
      folders: boot.folders,
      tags: boot.tags,
      documents: boot.documents,
      settings: boot.settings,
      selectedDocumentId:
        prev.selectedDocumentId && boot.documents.some((d) => d.id === prev.selectedDocumentId)
          ? prev.selectedDocumentId
          : (boot.documents[0]?.id ?? null),
      error: null,
    }));
  },

  bootstrap: async () => {
    set({ loading: true, error: null });
    try {
      const boot = await tauriClient.getBootstrapState();
      get().applyBootstrap(boot);
    } catch (err) {
      set({ loading: false, initialized: true, error: normalizeApiError(err) });
    }
  },

  refreshDocuments: async () => {
    try {
      const docs = await tauriClient.listDocuments(buildFilter(get()));
      set({ documents: docs });
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  setQuickView: async (view) => {
    set({
      quickView: view,
      selectedSubjectId: null,
      selectedFolderId: null,
      selectedTagId: null,
    });
    await get().refreshDocuments();
  },

  selectSubjectFilter: async (subjectId) => {
    set({
      quickView: "all",
      selectedSubjectId: subjectId,
      selectedFolderId: null,
    });
    await get().refreshDocuments();
  },

  selectFolderFilter: async (subjectId, folderId) => {
    set({
      quickView: "all",
      selectedSubjectId: subjectId,
      selectedFolderId: folderId,
    });
    await get().refreshDocuments();
  },

  selectTagFilter: async (tagId) => {
    set((s) => ({
      selectedTagId: s.selectedTagId === tagId ? null : tagId,
    }));
    await get().refreshDocuments();
  },

  setStudyStatusFilter: async (status) => {
    set((s) => ({
      selectedStudyStatus: s.selectedStudyStatus === status ? null : status,
    }));
    await get().refreshDocuments();
  },

  setSearchQuery: async (query) => {
    set({ searchQuery: query });
    await get().refreshDocuments();
  },

  runFullTextSearch: async (query) => {
    const trimmed = query.trim();
    if (!trimmed) {
      set({ ftsHits: [], ftsLoading: false });
      return;
    }
    set({ ftsLoading: true, error: null });
    try {
      const s = get();
      const hits = await tauriClient.fullTextSearch({
        query: trimmed,
        subjectId: s.selectedSubjectId,
        studyStatus: s.selectedStudyStatus,
        favoritesOnly: s.quickView === "favorites",
        limit: 40,
      });
      set({ ftsHits: hits, ftsLoading: false });
    } catch (err) {
      set({ ftsLoading: false, error: normalizeApiError(err) });
    }
  },

  clearFullTextSearch: () => set({ ftsHits: [], ftsLoading: false }),

  selectDocument: (documentId) => {
    set({ selectedDocumentId: documentId });
    if (documentId) {
      void tauriClient.touchDocumentOpened(documentId).catch(() => {});
    }
  },

  createSubject: async (name, category, description) => {
    try {
      const created = await tauriClient.createSubject(name, category, description);
      set((s) => ({ subjects: [...s.subjects, created], error: null }));
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  updateSubject: async (id, name, category, description) => {
    try {
      const updated = await tauriClient.updateSubject(id, name, category, description);
      set((s) => ({
        subjects: s.subjects.map((sub) => (sub.id === id ? updated : sub)),
        error: null,
      }));
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  deleteSubject: async (id) => {
    try {
      await tauriClient.deleteSubject(id);
      set((s) => ({
        subjects: s.subjects.filter((sub) => sub.id !== id),
        folders: s.folders.filter((f) => f.subjectId !== id),
        selectedSubjectId: s.selectedSubjectId === id ? null : s.selectedSubjectId,
        selectedFolderId: s.selectedSubjectId === id ? null : s.selectedFolderId,
        error: null,
      }));
      await get().refreshDocuments();
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  createFolder: async (subjectId, parentId, name) => {
    try {
      const folder = await tauriClient.createFolder(subjectId, parentId, name);
      set((s) => ({ folders: [...s.folders, folder], error: null }));
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  updateFolder: async (id, name, parentId) => {
    try {
      const updated = await tauriClient.updateFolder(id, name, parentId);
      set((s) => ({
        folders: s.folders.map((f) => (f.id === id ? updated : f)),
        error: null,
      }));
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  setFolderAutoUpload: async (id, autoUploadDrive) => {
    try {
      const updated = await tauriClient.setFolderAutoUpload(id, autoUploadDrive);
      set((s) => ({
        folders: s.folders.map((f) => (f.id === id ? updated : f)),
        error: null,
      }));
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  deleteFolder: async (id) => {
    try {
      await tauriClient.deleteFolder(id);
      const folders = await tauriClient.listFolders();
      set((s) => ({
        folders,
        selectedFolderId: s.selectedFolderId === id ? null : s.selectedFolderId,
        error: null,
      }));
      await get().refreshDocuments();
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  createTag: async (name, color) => {
    try {
      const created = await tauriClient.createTag(name, color);
      set((s) => ({ tags: [...s.tags, created], error: null }));
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  deleteTag: async (id) => {
    try {
      await tauriClient.deleteTag(id);
      set((s) => ({
        tags: s.tags.filter((t) => t.id !== id),
        selectedTagId: s.selectedTagId === id ? null : s.selectedTagId,
        error: null,
      }));
      await get().refreshDocuments();
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  setDocumentTags: async (documentId, tagIds) => {
    try {
      await tauriClient.setDocumentTags(documentId, tagIds);
      await get().refreshDocuments();
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  toggleFavorite: async (doc) => {
    try {
      await tauriClient.updateDocument({ id: doc.id, isFavorite: !doc.isFavorite });
      await get().refreshDocuments();
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  setDocumentStudyStatus: async (id, studyStatus) => {
    try {
      await tauriClient.updateDocument({ id, studyStatus });
      await get().refreshDocuments();
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  renameDocument: async (id, title) => {
    try {
      await tauriClient.updateDocument({ id, title });
      await get().refreshDocuments();
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  moveDocument: async (id, subjectId, folderId) => {
    try {
      await tauriClient.updateDocument({ id, subjectId, folderId });
      await get().refreshDocuments();
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  deleteDocument: async (id) => {
    try {
      await tauriClient.deleteDocument(id);
      set((s) => ({
        selectedDocumentId: s.selectedDocumentId === id ? null : s.selectedDocumentId,
        error: null,
      }));
      await get().refreshDocuments();
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  importFromPaths: async (paths) => {
    if (paths.length === 0) return;
    const target = resolveImportTarget(get());
    if (!target) {
      set({
        error: {
          code: "NO_SUBJECT",
          message: "取り込み先の科目が存在しません。先に科目を作成してください。",
          detail: null,
          recoverable: true,
        },
      });
      return;
    }

    set({ importing: true, error: null });
    try {
      const batch = await tauriClient.importFilesFromPaths(
        target.subjectId,
        target.folderId,
        paths,
      );
      const duplicates = batch.imported.flatMap((item) => item.duplicateMatches);
      const firstImportedId = batch.imported[0]?.detail.document.id ?? null;
      await get().refreshDocuments();
      set((s) => ({
        importing: false,
        selectedDocumentId: firstImportedId ?? s.selectedDocumentId,
        duplicateNotice: duplicates.length > 0 ? duplicates : s.duplicateNotice,
        error:
          batch.failures.length > 0
            ? {
                code: "PARTIAL_IMPORT_FAILURE",
                message: `${batch.failures.length} 件のファイルを取り込めませんでした`,
                detail: batch.failures.map((f) => `${f.filename}: ${f.reason}`).join(" / "),
                recoverable: true,
              }
            : null,
      }));
    } catch (err) {
      set({ importing: false, error: normalizeApiError(err) });
    }
  },

  importFromBytes: async (filename, base64Data) => {
    const target = resolveImportTarget(get());
    if (!target) {
      set({
        error: {
          code: "NO_SUBJECT",
          message: "取り込み先の科目が存在しません。先に科目を作成してください。",
          detail: null,
          recoverable: true,
        },
      });
      return;
    }

    set({ importing: true, error: null });
    try {
      const item = await tauriClient.importFileBytes(
        target.subjectId,
        target.folderId,
        filename,
        base64Data,
      );
      await get().refreshDocuments();
      set((s) => ({
        importing: false,
        selectedDocumentId: item.detail.document.id,
        duplicateNotice:
          item.duplicateMatches.length > 0 ? item.duplicateMatches : s.duplicateNotice,
      }));
    } catch (err) {
      set({ importing: false, error: normalizeApiError(err) });
    }
  },

  saveSettings: async (next) => {
    try {
      const saved = await tauriClient.updateSettings(next);
      useLayoutStore.getState().setTheme(saved.theme);
      set({ settings: saved, error: null });
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  dismissDuplicateNotice: () => set({ duplicateNotice: null }),
  clearError: () => set({ error: null }),
}));
