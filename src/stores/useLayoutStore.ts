import { create } from "zustand";
import type { ThemePreference } from "@/types/ipc";

export type CenterTab =
  | "original"
  | "recognized"
  | "solutionPdf"
  | "solutionEditor"
  | "calculations"
  | "versions";

export type SettingsCategory =
  | "general"
  | "appearance"
  | "agents"
  | "models"
  | "prompts"
  | "pdf"
  | "drive"
  | "scienceMcp"
  | "storage"
  | "security"
  | "about";

interface LayoutState {
  leftSidebarOpen: boolean;
  rightSidebarOpen: boolean;
  leftSidebarWidth: number;
  rightSidebarWidth: number;
  activeTab: CenterTab;
  splitView: boolean;
  theme: ThemePreference;
  settingsOpen: boolean;
  settingsCategory: SettingsCategory;

  setLeftSidebarOpen: (open: boolean) => void;
  toggleLeftSidebar: () => void;
  setRightSidebarOpen: (open: boolean) => void;
  toggleRightSidebar: () => void;
  setLeftSidebarWidth: (width: number) => void;
  setRightSidebarWidth: (width: number) => void;
  resetSidebarWidths: () => void;
  setActiveTab: (tab: CenterTab) => void;
  setSplitView: (enabled: boolean) => void;
  toggleSplitView: () => void;
  setTheme: (theme: ThemePreference) => void;
  openSettings: (category?: SettingsCategory) => void;
  closeSettings: () => void;
  setSettingsCategory: (category: SettingsCategory) => void;
}

const MIN_LEFT = 220;
const MAX_LEFT = 460;
const MIN_RIGHT = 240;
const MAX_RIGHT = 520;

export const useLayoutStore = create<LayoutState>((set) => ({
  leftSidebarOpen: true,
  rightSidebarOpen: true,
  leftSidebarWidth: 280,
  rightSidebarWidth: 320,
  activeTab: "original",
  splitView: false,
  theme: "system",
  settingsOpen: false,
  settingsCategory: "general",

  setLeftSidebarOpen: (open) => set({ leftSidebarOpen: open }),
  toggleLeftSidebar: () => set((s) => ({ leftSidebarOpen: !s.leftSidebarOpen })),
  setRightSidebarOpen: (open) => set({ rightSidebarOpen: open }),
  toggleRightSidebar: () => set((s) => ({ rightSidebarOpen: !s.rightSidebarOpen })),
  setLeftSidebarWidth: (width) =>
    set({ leftSidebarWidth: Math.min(MAX_LEFT, Math.max(MIN_LEFT, Math.round(width))) }),
  setRightSidebarWidth: (width) =>
    set({ rightSidebarWidth: Math.min(MAX_RIGHT, Math.max(MIN_RIGHT, Math.round(width))) }),
  resetSidebarWidths: () => set({ leftSidebarWidth: 280, rightSidebarWidth: 320 }),
  setActiveTab: (tab) => set({ activeTab: tab, splitView: false }),
  setSplitView: (enabled) => set({ splitView: enabled }),
  toggleSplitView: () => set((s) => ({ splitView: !s.splitView })),
  setTheme: (theme) => set({ theme }),
  openSettings: (category) =>
    set((s) => ({
      settingsOpen: true,
      settingsCategory: category ?? s.settingsCategory,
    })),
  closeSettings: () => set({ settingsOpen: false }),
  setSettingsCategory: (category) => set({ settingsCategory: category }),
}));
