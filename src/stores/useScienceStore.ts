import { create } from "zustand";
import { normalizeApiError, tauriClient } from "@/services/tauriClient";
import type {
  ApiError,
  AttachmentBinaryResponse,
  RunScienceToolRequest,
  RunScienceToolResult,
  ScienceMcpStatus,
  SolutionArtifactItem,
  ToolExecutionRecord,
  VerificationResultRecord,
} from "@/types/ipc";

interface ScienceStoreState {
  status: ScienceMcpStatus | null;
  checkingStatus: boolean;

  activeDocumentId: string | null;
  activeSolutionId: string | null;
  toolExecutions: ToolExecutionRecord[];
  verifications: VerificationResultRecord[];
  artifacts: SolutionArtifactItem[];
  artifactPreviews: Record<string, AttachmentBinaryResponse>;

  loading: boolean;
  runningTool: boolean;
  attachingArtifactId: string | null;
  error: ApiError | null;

  checkStatus: () => Promise<ScienceMcpStatus | null>;
  loadForDocument: (documentId: string | null) => Promise<void>;
  loadForSolution: (solutionId: string | null) => Promise<void>;
  loadArtifactPreview: (attachmentId: string) => Promise<void>;
  runTool: (req: RunScienceToolRequest) => Promise<RunScienceToolResult | null>;
  attachArtifactToSolution: (
    solutionId: string,
    attachmentId: string,
    caption?: string | null,
  ) => Promise<string | null>;
  clearError: () => void;
}

export const useScienceStore = create<ScienceStoreState>((set, get) => ({
  status: null,
  checkingStatus: false,

  activeDocumentId: null,
  activeSolutionId: null,
  toolExecutions: [],
  verifications: [],
  artifacts: [],
  artifactPreviews: {},

  loading: false,
  runningTool: false,
  attachingArtifactId: null,
  error: null,

  checkStatus: async () => {
    set({ checkingStatus: true, error: null });
    try {
      const status = await tauriClient.checkScienceMcpStatus();
      set({ status, checkingStatus: false });
      return status;
    } catch (err) {
      set({ checkingStatus: false, error: normalizeApiError(err) });
      return null;
    }
  },

  loadForDocument: async (documentId) => {
    if (!documentId) {
      set({
        activeDocumentId: null,
        toolExecutions: [],
        error: null,
      });
      return;
    }
    set({ activeDocumentId: documentId, loading: true, error: null });
    try {
      const toolExecutions = await tauriClient.listDocumentToolExecutions(documentId);
      set({ toolExecutions, loading: false });
    } catch (err) {
      set({ loading: false, error: normalizeApiError(err) });
    }
  },

  loadForSolution: async (solutionId) => {
    if (!solutionId) {
      set({
        activeSolutionId: null,
        verifications: [],
        artifacts: [],
      });
      return;
    }
    set({ activeSolutionId: solutionId, loading: true, error: null });
    try {
      const [verifications, artifacts] = await Promise.all([
        tauriClient.listSolutionVerifications(solutionId),
        tauriClient.listSolutionArtifacts(solutionId),
      ]);
      set({ verifications, artifacts, loading: false });

      // Preload preview binaries for generated plot/diagram artifacts
      for (const art of artifacts) {
        if (!get().artifactPreviews[art.attachmentId]) {
          void get().loadArtifactPreview(art.attachmentId);
        }
      }
    } catch (err) {
      set({ loading: false, error: normalizeApiError(err) });
    }
  },

  loadArtifactPreview: async (attachmentId) => {
    try {
      const bin = await tauriClient.readAttachmentBase64(attachmentId);
      set((s) => ({
        artifactPreviews: { ...s.artifactPreviews, [attachmentId]: bin },
      }));
    } catch {
      // Non-fatal preview error; card still shows metadata
    }
  },

  runTool: async (req) => {
    set({ runningTool: true, error: null });
    try {
      const res = await tauriClient.runScienceTool(req);
      const toolExecutions = await tauriClient.listDocumentToolExecutions(req.documentId);
      const targetSolId = res.updatedSolution?.solution.id ?? req.solutionId ?? get().activeSolutionId;
      set({ runningTool: false, toolExecutions });
      if (targetSolId) {
        await get().loadForSolution(targetSolId);
      }
      for (const att of res.createdArtifacts) {
        void get().loadArtifactPreview(att.id);
      }
      return res;
    } catch (err) {
      set({ runningTool: false, error: normalizeApiError(err) });
      return null;
    }
  },

  attachArtifactToSolution: async (solutionId, attachmentId, caption) => {
    set({ attachingArtifactId: attachmentId, error: null });
    try {
      const op = await tauriClient.attachArtifactToSolution(solutionId, attachmentId, caption);
      set({ attachingArtifactId: null });
      await get().loadForSolution(op.solution.id);
      return op.solution.id;
    } catch (err) {
      set({ attachingArtifactId: null, error: normalizeApiError(err) });
      return null;
    }
  },

  clearError: () => set({ error: null }),
}));
