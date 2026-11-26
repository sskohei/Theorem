import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import {
  CloudOff,
  FilePlus2,
  Moon,
  PanelLeft,
  PanelRight,
  Settings,
  Sun,
  Upload,
} from "lucide-react";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { ChatSidebarPlaceholder } from "@/features/chat/ChatSidebarPlaceholder";
import { CenterWorkspace } from "@/features/document/CenterWorkspace";
import { LibrarySidebar } from "@/features/library/LibrarySidebar";
import { SettingsModal } from "@/features/settings/SettingsModal";
import { useI18n } from "@/lib/i18n";
import { uint8ArrayToBase64 } from "@/lib/utils";
import { isTauriRuntime, tauriClient } from "@/services/tauriClient";
import { useDocumentStore } from "@/stores/useDocumentStore";
import { useLayoutStore } from "@/stores/useLayoutStore";
import { useLibraryStore } from "@/stores/useLibraryStore";

export function App() {
  const t = useI18n();
  const {
    leftSidebarOpen,
    rightSidebarOpen,
    leftSidebarWidth,
    rightSidebarWidth,
    theme,
    toggleLeftSidebar,
    toggleRightSidebar,
    setLeftSidebarWidth,
    setRightSidebarWidth,
    openSettings,
  } = useLayoutStore();

  const {
    selectedDocumentId,
    settings,
    bootstrap,
    importFromPaths,
    importFromBytes,
    saveSettings,
  } = useLibraryStore();

  const { loadDocument, currentPage, setCurrentPage } = useDocumentStore();

  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const [draggingOverlay, setDraggingOverlay] = useState(false);

  // 1. Initial bootstrap from Rust SQLite core
  useEffect(() => {
    void bootstrap();
  }, [bootstrap]);

  // 2. Load selected document detail & binary payload from Rust
  useEffect(() => {
    void loadDocument(selectedDocumentId);
  }, [selectedDocumentId, loadDocument]);

  // 3. Synchronize light/dark/system theme class on <html>
  useEffect(() => {
    const root = document.documentElement;
    const apply = () => {
      const prefersDark =
        typeof window.matchMedia === "function" &&
        window.matchMedia("(prefers-color-scheme: dark)").matches;
      const isDark = theme === "dark" || (theme === "system" && prefersDark);
      root.classList.toggle("dark", isDark);
    };
    apply();
    if (theme !== "system" || typeof window.matchMedia !== "function") return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, [theme]);

  // 4. Native file dialog import
  const handleImportClick = useCallback(async () => {
    if (isTauriRuntime()) {
      const paths = await tauriClient.pickProblemFiles();
      if (paths.length > 0) {
        await importFromPaths(paths);
      }
      return;
    }
    // Browser/test fallback input
    const input = document.createElement("input");
    input.type = "file";
    input.multiple = true;
    input.accept = ".pdf,.png,.jpg,.jpeg,.webp";
    input.onchange = async () => {
      const files = Array.from(input.files ?? []);
      for (const file of files) {
        const bytes = new Uint8Array(await file.arrayBuffer());
        await importFromBytes(file.name, uint8ArrayToBase64(bytes));
      }
    };
    input.click();
  }, [importFromPaths, importFromBytes]);

  // 5. Native Tauri drag-and-drop file import listener
  useEffect(() => {
    if (!isTauriRuntime()) return;
    let unlisten: (() => void) | null = null;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "over" || event.payload.type === "enter") {
          setDraggingOverlay(true);
        } else if (event.payload.type === "leave") {
          setDraggingOverlay(false);
        } else if (event.payload.type === "drop") {
          setDraggingOverlay(false);
          if (event.payload.paths.length > 0) {
            void importFromPaths(event.payload.paths);
          }
        }
      })
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => {});

    return () => {
      if (unlisten) unlisten();
    };
  }, [importFromPaths]);

  // 6. Global clipboard image paste (Ctrl+V)
  useEffect(() => {
    const onPaste = (e: ClipboardEvent) => {
      const items = e.clipboardData?.items;
      if (!items) return;
      for (const item of Array.from(items)) {
        if (item.type.startsWith("image/")) {
          const blob = item.getAsFile();
          if (!blob) continue;
          e.preventDefault();
          const ext = item.type.split("/")[1] || "png";
          const filename = `clipboard-${Date.now()}.${ext}`;
          void blob.arrayBuffer().then((buf) => {
            const b64 = uint8ArrayToBase64(new Uint8Array(buf));
            void importFromBytes(filename, b64);
          });
          break;
        }
      }
    };
    window.addEventListener("paste", onPaste);
    return () => window.removeEventListener("paste", onPaste);
  }, [importFromBytes]);

  // 7. Global keyboard shortcuts
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const ctrl = e.ctrlKey || e.metaKey;
      const target = e.target as HTMLElement | null;
      const inInput =
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        target instanceof HTMLSelectElement;

      if (ctrl && e.key.toLowerCase() === "k") {
        e.preventDefault();
        searchInputRef.current?.focus();
      } else if (ctrl && e.key.toLowerCase() === "b") {
        e.preventDefault();
        toggleLeftSidebar();
      } else if (ctrl && e.key.toLowerCase() === "j") {
        e.preventDefault();
        toggleRightSidebar();
      } else if (ctrl && e.key === ",") {
        e.preventDefault();
        openSettings();
      } else if (ctrl && e.key.toLowerCase() === "o") {
        e.preventDefault();
        void handleImportClick();
      } else if (!inInput && e.key === "ArrowLeft") {
        setCurrentPage(currentPage - 1);
      } else if (!inInput && e.key === "ArrowRight") {
        setCurrentPage(currentPage + 1);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [
    toggleLeftSidebar,
    toggleRightSidebar,
    openSettings,
    handleImportClick,
    currentPage,
    setCurrentPage,
  ]);

  // 8. Resizable sidebar pointer handlers
  const startResize = (side: "left" | "right") => (startEvent: React.MouseEvent) => {
    startEvent.preventDefault();
    const startX = startEvent.clientX;
    const initialWidth = side === "left" ? leftSidebarWidth : rightSidebarWidth;

    const onMove = (moveEvent: MouseEvent) => {
      const delta = moveEvent.clientX - startX;
      if (side === "left") {
        setLeftSidebarWidth(initialWidth + delta);
      } else {
        setRightSidebarWidth(initialWidth - delta);
      }
    };

    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      const layout = useLayoutStore.getState();
      void saveSettings({
        ...settings,
        leftSidebarWidth: layout.leftSidebarWidth,
        rightSidebarWidth: layout.rightSidebarWidth,
      });
    };

    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  // HTML5 drag-and-drop fallback (for dev browser mode / tests)
  const handleHtmlDrop = async (e: React.DragEvent) => {
    e.preventDefault();
    setDraggingOverlay(false);
    if (isTauriRuntime()) return;
    const files = Array.from(e.dataTransfer.files);
    for (const file of files) {
      const bytes = new Uint8Array(await file.arrayBuffer());
      await importFromBytes(file.name, uint8ArrayToBase64(bytes));
    }
  };

  const handleToggleQuickTheme = () => {
    const next = theme === "dark" ? "light" : "dark";
    void saveSettings({ ...settings, theme: next });
  };

  return (
    <div
      onDragOver={(e) => {
        e.preventDefault();
        if (!isTauriRuntime()) setDraggingOverlay(true);
      }}
      onDragLeave={() => {
        if (!isTauriRuntime()) setDraggingOverlay(false);
      }}
      onDrop={(e) => void handleHtmlDrop(e)}
      className="relative flex h-screen w-screen flex-col overflow-hidden bg-slate-50 text-slate-900 dark:bg-slate-950 dark:text-slate-100"
    >
      {/* Top Header Bar */}
      <header className="flex h-11 shrink-0 items-center justify-between border-b border-slate-200 bg-white px-3 dark:border-slate-800 dark:bg-slate-900">
        <div className="flex items-center gap-2">
          <Button
            size="icon"
            variant="ghost"
            onClick={toggleLeftSidebar}
            title={t.header.toggleLeftSidebar}
            aria-label={t.header.toggleLeftSidebar}
          >
            <PanelLeft className="h-4 w-4" />
          </Button>
          <span className="text-sm font-bold tracking-tight text-slate-900 dark:text-slate-100">
            {t.appName}
          </span>
          <Badge variant="info" className="hidden sm:inline-flex">
            Phase 1
          </Badge>
        </div>

        <div className="flex items-center gap-1.5">
          <Badge variant="outline" className="hidden md:inline-flex gap-1 text-slate-500">
            <CloudOff className="h-3 w-3" />
            <span>{t.header.driveNotConnected}</span>
          </Badge>

          <Button size="xs" variant="outline" onClick={() => void handleImportClick()}>
            <FilePlus2 className="h-3.5 w-3.5" />
            <span className="hidden sm:inline">{t.header.importButton}</span>
          </Button>

          <Button
            size="icon"
            variant="ghost"
            onClick={handleToggleQuickTheme}
            title="ライト / ダーク切替"
            aria-label="テーマ切替"
          >
            {theme === "dark" ? <Sun className="h-4 w-4" /> : <Moon className="h-4 w-4" />}
          </Button>

          <Button
            size="icon"
            variant="ghost"
            onClick={() => openSettings("general")}
            title={t.header.settingsButton}
            aria-label={t.header.settingsButton}
          >
            <Settings className="h-4 w-4" />
          </Button>

          <Button
            size="icon"
            variant="ghost"
            onClick={toggleRightSidebar}
            title={t.header.toggleRightSidebar}
            aria-label={t.header.toggleRightSidebar}
          >
            <PanelRight className="h-4 w-4" />
          </Button>
        </div>
      </header>

      {/* 3-Column Main Shell */}
      <div className="flex min-h-0 flex-1 overflow-hidden">
        {leftSidebarOpen && (
          <>
            <div style={{ width: `${leftSidebarWidth}px` }} className="h-full shrink-0">
              <LibrarySidebar
                searchInputRef={searchInputRef}
                onImportClick={() => void handleImportClick()}
              />
            </div>
            <div
              role="separator"
              aria-orientation="vertical"
              aria-label="左サイドバー幅調整"
              onMouseDown={startResize("left")}
              className="w-1 shrink-0 cursor-col-resize bg-transparent transition-colors hover:bg-blue-400/60"
            />
          </>
        )}

        <CenterWorkspace onImportClick={() => void handleImportClick()} />

        {rightSidebarOpen && (
          <>
            <div
              role="separator"
              aria-orientation="vertical"
              aria-label="右サイドバー幅調整"
              onMouseDown={startResize("right")}
              className="w-1 shrink-0 cursor-col-resize bg-transparent transition-colors hover:bg-blue-400/60"
            />
            <div style={{ width: `${rightSidebarWidth}px` }} className="h-full shrink-0">
              <ChatSidebarPlaceholder />
            </div>
          </>
        )}
      </div>

      {/* Drag & Drop Overlay */}
      {draggingOverlay && (
        <div className="pointer-events-none fixed inset-0 z-40 flex items-center justify-center bg-blue-950/35 backdrop-blur-xs">
          <div className="flex flex-col items-center gap-2 rounded-2xl border-2 border-dashed border-blue-400 bg-white/95 px-10 py-8 shadow-xl dark:bg-slate-900/95">
            <Upload className="h-8 w-8 text-blue-600 dark:text-blue-400" />
            <p className="text-sm font-semibold text-slate-900 dark:text-slate-100">
              ここに PDF / 画像をドロップして取り込む
            </p>
            <p className="text-xs text-slate-500">対応形式: PDF, JPG, PNG, WebP（複数可）</p>
          </div>
        </div>
      )}

      <SettingsModal />
    </div>
  );
}
