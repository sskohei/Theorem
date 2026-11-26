import { create } from "zustand";
import { normalizeApiError, tauriClient } from "@/services/tauriClient";
import type {
  ApiError,
  CompareAgentsRequest,
  CompareAgentsResult,
  CreateReferenceMaterialRequest,
  ReferenceMaterial,
  SolutionVersionDiff,
} from "@/types/ipc";

interface StudyStoreState {
  references: ReferenceMaterial[];
  selectedReferenceIds: string[];
  loadingReferences: boolean;

  versionDiff: SolutionVersionDiff | null;
  comparingVersions: boolean;

  agentComparison: CompareAgentsResult | null;
  comparingAgents: boolean;

  error: ApiError | null;

  loadReferences: (documentId: string | null, subjectId?: string | null) => Promise<void>;
  createReference: (req: CreateReferenceMaterialRequest) => Promise<ReferenceMaterial | null>;
  deleteReference: (id: string) => Promise<void>;
  toggleReferenceSelection: (id: string) => void;
  clearSelectedReferences: () => void;

  compareVersions: (baseSolutionId: string, targetSolutionId: string) => Promise<void>;
  clearVersionDiff: () => void;

  compareAgents: (req: CompareAgentsRequest) => Promise<CompareAgentsResult | null>;
  clearAgentComparison: () => void;
  clearError: () => void;
}

export const useStudyStore = create<StudyStoreState>((set, get) => ({
  references: [],
  selectedReferenceIds: [],
  loadingReferences: false,

  versionDiff: null,
  comparingVersions: false,

  agentComparison: null,
  comparingAgents: false,

  error: null,

  loadReferences: async (documentId, subjectId = null) => {
    if (!documentId && !subjectId) {
      set({ references: [], selectedReferenceIds: [] });
      return;
    }
    set({ loadingReferences: true, error: null });
    try {
      const refs = await tauriClient.listReferenceMaterials(documentId, subjectId);
      const validIds = new Set(refs.map((r) => r.id));
      set((s) => ({
        references: refs,
        selectedReferenceIds: s.selectedReferenceIds.filter((id) => validIds.has(id)),
        loadingReferences: false,
      }));
    } catch (err) {
      set({ loadingReferences: false, error: normalizeApiError(err) });
    }
  },

  createReference: async (req) => {
    set({ error: null });
    try {
      const created = await tauriClient.createReferenceMaterial(req);
      set((s) => ({
        references: [created, ...s.references],
        selectedReferenceIds: [...s.selectedReferenceIds, created.id],
      }));
      return created;
    } catch (err) {
      set({ error: normalizeApiError(err) });
      return null;
    }
  },

  deleteReference: async (id) => {
    set({ error: null });
    try {
      await tauriClient.deleteReferenceMaterial(id);
      set((s) => ({
        references: s.references.filter((r) => r.id !== id),
        selectedReferenceIds: s.selectedReferenceIds.filter((rid) => rid !== id),
      }));
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  toggleReferenceSelection: (id) =>
    set((s) => ({
      selectedReferenceIds: s.selectedReferenceIds.includes(id)
        ? s.selectedReferenceIds.filter((rid) => rid !== id)
        : [...s.selectedReferenceIds, id],
    })),

  clearSelectedReferences: () => set({ selectedReferenceIds: [] }),

  compareVersions: async (baseSolutionId, targetSolutionId) => {
    set({ comparingVersions: true, error: null });
    try {
      const diff = await tauriClient.compareSolutionVersions(baseSolutionId, targetSolutionId);
      set({ versionDiff: diff, comparingVersions: false });
    } catch (err) {
      set({ comparingVersions: false, error: normalizeApiError(err) });
    }
  },

  clearVersionDiff: () => set({ versionDiff: null }),

  compareAgents: async (req) => {
    set({ comparingAgents: true, error: null });
    try {
      const res = await tauriClient.compareAgentsGenerateSolutions({
        ...req,
        referenceMaterialIds: req.referenceMaterialIds ?? get().selectedReferenceIds,
      });
      set({ agentComparison: res, comparingAgents: false });
      return res;
    } catch (err) {
      set({ comparingAgents: false, error: normalizeApiError(err) });
      return null;
    }
  },

  clearAgentComparison: () => set({ agentComparison: null }),
  clearError: () => set({ error: null }),
}));
