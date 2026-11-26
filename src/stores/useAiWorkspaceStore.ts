import { create } from "zustand";
import { normalizeApiError, tauriClient } from "@/services/tauriClient";
import type {
  ApiError,
  AttachmentInfo,
  GenerateSolutionRequest,
  LatexConversionError,
  Problem,
  RecognizeProblemsRequest,
  SaveSolutionEditRequest,
  SchemaValidationIssue,
  Solution,
  SolutionOperationResult,
  UpsertProblemRequest,
} from "@/types/ipc";

interface AiWorkspaceState {
  activeDocumentId: string | null;
  problems: Problem[];
  selectedProblemId: string | null;
  solutions: Solution[];
  selectedSolutionId: string | null;
  solutionPdfAttachment: AttachmentInfo | null;
  solutionPdfBase64: string | null;
  loadingPdf: boolean;

  recognizing: boolean;
  generatingSolution: boolean;
  savingSolution: boolean;

  recognitionSchemaIssues: SchemaValidationIssue[];
  recognitionRawOutput: string | null;
  solutionSchemaIssues: SchemaValidationIssue[];
  solutionLatexErrors: LatexConversionError[];
  solutionPdfError: string | null;
  solutionRawOutput: string | null;
  error: ApiError | null;

  loadForDocument: (documentId: string | null) => Promise<void>;
  selectProblem: (problemId: string | null) => void;
  selectSolution: (solutionId: string | null) => Promise<void>;

  recognize: (req: RecognizeProblemsRequest) => Promise<boolean>;
  upsertProblem: (input: UpsertProblemRequest) => Promise<Problem | null>;
  deleteProblem: (problemId: string) => Promise<void>;
  splitProblem: (
    problemId: string,
    firstStatement: string,
    secondStatement: string,
    secondSubproblemNumber?: string | null,
  ) => Promise<void>;
  mergeProblems: (primaryId: string, secondaryId: string) => Promise<void>;

  generateSolution: (req: GenerateSolutionRequest) => Promise<SolutionOperationResult | null>;
  saveSolutionEdit: (req: SaveSolutionEditRequest) => Promise<SolutionOperationResult | null>;
  recompileSolutionPdf: (solutionId: string) => Promise<SolutionOperationResult | null>;
  restoreSolutionVersion: (solutionId: string) => Promise<SolutionOperationResult | null>;
  clearError: () => void;
}

export const useAiWorkspaceStore = create<AiWorkspaceState>((set, get) => ({
  activeDocumentId: null,
  problems: [],
  selectedProblemId: null,
  solutions: [],
  selectedSolutionId: null,
  solutionPdfAttachment: null,
  solutionPdfBase64: null,
  loadingPdf: false,

  recognizing: false,
  generatingSolution: false,
  savingSolution: false,

  recognitionSchemaIssues: [],
  recognitionRawOutput: null,
  solutionSchemaIssues: [],
  solutionLatexErrors: [],
  solutionPdfError: null,
  solutionRawOutput: null,
  error: null,

  loadForDocument: async (documentId) => {
    if (!documentId) {
      set({
        activeDocumentId: null,
        problems: [],
        selectedProblemId: null,
        solutions: [],
        selectedSolutionId: null,
        solutionPdfAttachment: null,
        solutionPdfBase64: null,
        recognitionSchemaIssues: [],
        recognitionRawOutput: null,
        solutionSchemaIssues: [],
        solutionLatexErrors: [],
        solutionPdfError: null,
        solutionRawOutput: null,
        error: null,
      });
      return;
    }

    set({
      activeDocumentId: documentId,
      recognitionSchemaIssues: [],
      recognitionRawOutput: null,
      solutionSchemaIssues: [],
      solutionLatexErrors: [],
      solutionPdfError: null,
      solutionRawOutput: null,
      error: null,
    });

    try {
      const [problems, solutions] = await Promise.all([
        tauriClient.listProblems(documentId),
        tauriClient.listSolutions(documentId, null),
      ]);
      const currentSol = solutions.find((s) => s.isCurrent) ?? solutions[0] ?? null;
      set({
        problems,
        selectedProblemId: problems[0]?.id ?? null,
        solutions,
        selectedSolutionId: currentSol?.id ?? null,
      });
      if (currentSol) {
        await get().selectSolution(currentSol.id);
      } else {
        set({ solutionPdfAttachment: null, solutionPdfBase64: null });
      }
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  selectProblem: (problemId) => set({ selectedProblemId: problemId }),

  selectSolution: async (solutionId) => {
    set({ selectedSolutionId: solutionId, solutionPdfAttachment: null, solutionPdfBase64: null });
    if (!solutionId) return;
    const sol = get().solutions.find((s) => s.id === solutionId);
    if (!sol?.pdfAttachmentId) return;

    set({ loadingPdf: true });
    try {
      const bin = await tauriClient.readAttachmentBase64(sol.pdfAttachmentId);
      set({
        solutionPdfAttachment: bin.attachment,
        solutionPdfBase64: bin.base64Data,
        loadingPdf: false,
      });
    } catch (err) {
      set({ loadingPdf: false, error: normalizeApiError(err) });
    }
  },

  recognize: async (req) => {
    set({
      recognizing: true,
      recognitionSchemaIssues: [],
      recognitionRawOutput: null,
      error: null,
    });
    try {
      const res = await tauriClient.recognizeProblems(req);
      if (get().activeDocumentId !== req.documentId) {
        set({ recognizing: false });
        return res.schemaIssues.length === 0;
      }
      set({
        recognizing: false,
        problems: res.problems,
        selectedProblemId: res.problems[0]?.id ?? null,
        recognitionSchemaIssues: res.schemaIssues,
        recognitionRawOutput: res.rawOutput,
      });
      return res.schemaIssues.length === 0;
    } catch (err) {
      set(get().activeDocumentId === req.documentId
        ? { recognizing: false, error: normalizeApiError(err) }
        : { recognizing: false });
      return false;
    }
  },

  upsertProblem: async (input) => {
    try {
      const saved = await tauriClient.upsertProblem(input);
      const problems = await tauriClient.listProblems(input.documentId);
      set({ problems, selectedProblemId: saved.id, error: null });
      return saved;
    } catch (err) {
      set({ error: normalizeApiError(err) });
      return null;
    }
  },

  deleteProblem: async (problemId) => {
    const docId = get().activeDocumentId;
    if (!docId) return;
    try {
      await tauriClient.deleteProblem(problemId);
      const problems = await tauriClient.listProblems(docId);
      set((s) => ({
        problems,
        selectedProblemId:
          s.selectedProblemId === problemId ? (problems[0]?.id ?? null) : s.selectedProblemId,
        error: null,
      }));
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  splitProblem: async (problemId, firstStatement, secondStatement, secondSubproblemNumber) => {
    try {
      const problems = await tauriClient.splitProblem(
        problemId,
        firstStatement,
        secondStatement,
        secondSubproblemNumber,
      );
      set({ problems, error: null });
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  mergeProblems: async (primaryId, secondaryId) => {
    try {
      const problems = await tauriClient.mergeProblems(primaryId, secondaryId);
      set({ problems, selectedProblemId: primaryId, error: null });
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  generateSolution: async (req) => {
    set({
      generatingSolution: true,
      solutionSchemaIssues: [],
      solutionLatexErrors: [],
      solutionPdfError: null,
      solutionRawOutput: null,
      error: null,
    });
    try {
      const res = await tauriClient.generateSolution(req);
      const solutions = await tauriClient.listSolutions(req.documentId, null);
      set({
        generatingSolution: false,
        solutions,
        selectedSolutionId: res.solution.id,
        solutionSchemaIssues: res.schemaIssues,
        solutionLatexErrors: res.latexErrors,
        solutionPdfError: res.pdfError,
        solutionRawOutput: res.rawOutput,
      });
      await get().selectSolution(res.solution.id);
      return res;
    } catch (err) {
      set({ generatingSolution: false, error: normalizeApiError(err) });
      return null;
    }
  },

  saveSolutionEdit: async (req) => {
    set({
      savingSolution: true,
      solutionSchemaIssues: [],
      solutionLatexErrors: [],
      solutionPdfError: null,
      solutionRawOutput: null,
      error: null,
    });
    try {
      const res = await tauriClient.saveSolutionEdit(req);
      const solutions = await tauriClient.listSolutions(req.documentId, null);
      set({
        savingSolution: false,
        solutions,
        selectedSolutionId: res.solution.id,
        solutionSchemaIssues: res.schemaIssues,
        solutionLatexErrors: res.latexErrors,
        solutionPdfError: res.pdfError,
        solutionRawOutput: res.rawOutput,
      });
      await get().selectSolution(res.solution.id);
      return res;
    } catch (err) {
      set({ savingSolution: false, error: normalizeApiError(err) });
      return null;
    }
  },

  recompileSolutionPdf: async (solutionId) => {
    const docId = get().activeDocumentId;
    if (!docId) return null;
    set({
      savingSolution: true,
      solutionSchemaIssues: [],
      solutionLatexErrors: [],
      solutionPdfError: null,
      error: null,
    });
    try {
      const res = await tauriClient.recompileSolutionPdf(solutionId);
      const solutions = await tauriClient.listSolutions(docId, null);
      set({
        savingSolution: false,
        solutions,
        selectedSolutionId: res.solution.id,
        solutionSchemaIssues: res.schemaIssues,
        solutionLatexErrors: res.latexErrors,
        solutionPdfError: res.pdfError,
      });
      await get().selectSolution(res.solution.id);
      return res;
    } catch (err) {
      set({ savingSolution: false, error: normalizeApiError(err) });
      return null;
    }
  },

  restoreSolutionVersion: async (solutionId) => {
    const docId = get().activeDocumentId;
    if (!docId) return null;
    set({ savingSolution: true, error: null });
    try {
      const res = await tauriClient.restoreSolutionVersion(solutionId);
      const solutions = await tauriClient.listSolutions(docId, null);
      set({
        savingSolution: false,
        solutions,
        selectedSolutionId: res.solution.id,
        solutionSchemaIssues: res.schemaIssues,
        solutionLatexErrors: res.latexErrors,
        solutionPdfError: res.pdfError,
      });
      await get().selectSolution(res.solution.id);
      return res;
    } catch (err) {
      set({ savingSolution: false, error: normalizeApiError(err) });
      return null;
    }
  },

  clearError: () => set({ error: null }),
}));
