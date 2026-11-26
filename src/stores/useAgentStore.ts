import { create } from "zustand";
import { normalizeApiError, tauriClient } from "@/services/tauriClient";
import type {
  AgentProfile,
  AgentStatus,
  ApiError,
  SaveAgentProfileRequest,
} from "@/types/ipc";

interface AgentStoreState {
  profiles: AgentProfile[];
  statusesByProfileId: Record<string, AgentStatus>;
  loading: boolean;
  detecting: boolean;
  error: ApiError | null;

  loadProfiles: () => Promise<void>;
  checkProfileStatus: (profileId: string) => Promise<AgentStatus | null>;
  detectAll: () => Promise<void>;
  saveProfile: (req: SaveAgentProfileRequest) => Promise<AgentProfile | null>;
  deleteProfile: (id: string) => Promise<void>;
  setApiKey: (profileId: string, apiKey: string) => Promise<void>;
  deleteApiKey: (profileId: string) => Promise<void>;
  clearError: () => void;
}

export const useAgentStore = create<AgentStoreState>((set, get) => ({
  profiles: [],
  statusesByProfileId: {},
  loading: false,
  detecting: false,
  error: null,

  loadProfiles: async () => {
    set({ loading: true, error: null });
    try {
      const profiles = await tauriClient.listAgentProfiles();
      set({ profiles, loading: false });
    } catch (err) {
      set({ loading: false, error: normalizeApiError(err) });
    }
  },

  checkProfileStatus: async (profileId) => {
    set({ detecting: true, error: null });
    try {
      const status = await tauriClient.checkAgentStatus(profileId);
      set((s) => ({
        detecting: false,
        statusesByProfileId: { ...s.statusesByProfileId, [profileId]: status },
      }));
      return status;
    } catch (err) {
      set({ detecting: false, error: normalizeApiError(err) });
      return null;
    }
  },

  detectAll: async () => {
    set({ detecting: true, error: null });
    try {
      const profiles =
        get().profiles.length > 0 ? get().profiles : await tauriClient.listAgentProfiles();
      const map: Record<string, AgentStatus> = { ...get().statusesByProfileId };
      for (const p of profiles) {
        try {
          map[p.id] = await tauriClient.checkAgentStatus(p.id);
        } catch {
          // Keep per-profile failure isolated
        }
      }
      set({ profiles, statusesByProfileId: map, detecting: false });
    } catch (err) {
      set({ detecting: false, error: normalizeApiError(err) });
    }
  },

  saveProfile: async (req) => {
    try {
      const saved = await tauriClient.saveAgentProfile(req);
      const profiles = await tauriClient.listAgentProfiles();
      set({ profiles, error: null });
      return saved;
    } catch (err) {
      set({ error: normalizeApiError(err) });
      return null;
    }
  },

  deleteProfile: async (id) => {
    try {
      await tauriClient.deleteAgentProfile(id);
      const profiles = await tauriClient.listAgentProfiles();
      set((s) => {
        const nextMap = { ...s.statusesByProfileId };
        delete nextMap[id];
        return { profiles, statusesByProfileId: nextMap, error: null };
      });
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  setApiKey: async (profileId, apiKey) => {
    try {
      const updated = await tauriClient.setAgentApiKey(profileId, apiKey);
      set((s) => ({
        profiles: s.profiles.map((p) => (p.id === profileId ? updated : p)),
        error: null,
      }));
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  deleteApiKey: async (profileId) => {
    try {
      const updated = await tauriClient.deleteAgentApiKey(profileId);
      set((s) => ({
        profiles: s.profiles.map((p) => (p.id === profileId ? updated : p)),
        error: null,
      }));
    } catch (err) {
      set({ error: normalizeApiError(err) });
    }
  },

  clearError: () => set({ error: null }),
}));
