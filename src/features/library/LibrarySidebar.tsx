import { useState, type RefObject } from "react";
import {
  BookOpen,
  ChevronDown,
  ChevronRight,
  Clock,
  Cloud,
  CloudUpload,
  FileImage,
  FilePlus2,
  FileText,
  Folder as FolderIcon,
  FolderInput,
  FolderPlus,
  Pencil,
  Plus,
  Search,
  Settings,
  Star,
  Tag as TagIcon,
  Trash2,
} from "lucide-react";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { useI18n } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { useDriveStore } from "@/stores/useDriveStore";
import { useLayoutStore } from "@/stores/useLayoutStore";
import { useLibraryStore } from "@/stores/useLibraryStore";
import type {
  DocumentSummary,
  Folder,
  FullTextSearchHit,
  StudyStatus,
  Subject,
  SubjectCategory,
} from "@/types/ipc";

interface LibrarySidebarProps {
  searchInputRef: RefObject<HTMLInputElement | null>;
  onImportClick: () => void;
}

function collectDescendantFolderIds(folders: Folder[], rootId: string): Set<string> {
  const blocked = new Set<string>([rootId]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const f of folders) {
      if (f.parentId && blocked.has(f.parentId) && !blocked.has(f.id)) {
        blocked.add(f.id);
        changed = true;
      }
    }
  }
  return blocked;
}

export function LibrarySidebar({ searchInputRef, onImportClick }: LibrarySidebarProps) {
  const t = useI18n();
  const openSettings = useLayoutStore((s) => s.openSettings);
  const setActiveTab = useLayoutStore((s) => s.setActiveTab);
  const {
    subjects,
    folders,
    tags,
    documents,
    quickView,
    selectedSubjectId,
    selectedFolderId,
    selectedTagId,
    selectedStudyStatus,
    searchQuery,
    ftsHits,
    ftsLoading,
    selectedDocumentId,
    importing,
    setQuickView,
    selectSubjectFilter,
    selectFolderFilter,
    selectTagFilter,
    setStudyStatusFilter,
    setSearchQuery,
    runFullTextSearch,
    clearFullTextSearch,
    selectDocument,
    createSubject,
    updateSubject,
    deleteSubject,
    createFolder,
    updateFolder,
    setFolderAutoUpload,
    deleteFolder,
    createTag,
    deleteTag,
    setDocumentTags,
    toggleFavorite,
    renameDocument,
    moveDocument,
    deleteDocument,
  } = useLibraryStore();
  const {
    overview: driveOverview,
    uploadingDocId,
    uploadDocument: uploadDocumentToDrive,
  } = useDriveStore();
  const driveConnected = driveOverview?.connection?.status === "connected";

  const [expandedSubjects, setExpandedSubjects] = useState<Record<string, boolean>>({});
  const [expandedFolders, setExpandedFolders] = useState<Record<string, boolean>>({});
  const [showAddSubject, setShowAddSubject] = useState(false);
  const [newSubjectName, setNewSubjectName] = useState("");
  const [newSubjectCategory, setNewSubjectCategory] = useState<SubjectCategory>("mathematics");
  const [editingSubjectId, setEditingSubjectId] = useState<string | null>(null);
  const [editSubjectName, setEditSubjectName] = useState("");
  const [editSubjectCategory, setEditSubjectCategory] = useState<SubjectCategory>("mathematics");

  const [addingFolderTarget, setAddingFolderTarget] = useState<{
    subjectId: string;
    parentId: string | null;
  } | null>(null);
  const [newFolderName, setNewFolderName] = useState("");
  const [editingFolderId, setEditingFolderId] = useState<string | null>(null);
  const [editFolderName, setEditFolderName] = useState("");
  const [movingFolderId, setMovingFolderId] = useState<string | null>(null);
  const [moveFolderParentId, setMoveFolderParentId] = useState<string>("");

  const [showAddTag, setShowAddTag] = useState(false);
  const [newTagName, setNewTagName] = useState("");

  const [editingDocId, setEditingDocId] = useState<string | null>(null);
  const [editDocTitle, setEditDocTitle] = useState("");
  const [taggingDocId, setTaggingDocId] = useState<string | null>(null);
  const [movingDocId, setMovingDocId] = useState<string | null>(null);
  const [moveDocSubjectId, setMoveDocSubjectId] = useState<string>("");
  const [moveDocFolderId, setMoveDocFolderId] = useState<string>("");

  const isSubjectExpanded = (id: string) => expandedSubjects[id] ?? true;
  const isFolderExpanded = (id: string) => expandedFolders[id] ?? true;

  const handleCreateSubject = async () => {
    if (!newSubjectName.trim()) return;
    await createSubject(newSubjectName.trim(), newSubjectCategory);
    setNewSubjectName("");
    setShowAddSubject(false);
  };

  const handleUpdateSubject = async (sub: Subject) => {
    if (!editSubjectName.trim()) return;
    await updateSubject(
      sub.id,
      editSubjectName.trim(),
      editSubjectCategory,
      sub.description ?? undefined,
    );
    setEditingSubjectId(null);
  };

  const handleCreateFolder = async () => {
    if (!addingFolderTarget || !newFolderName.trim()) return;
    await createFolder(
      addingFolderTarget.subjectId,
      addingFolderTarget.parentId,
      newFolderName.trim(),
    );
    setNewFolderName("");
    setAddingFolderTarget(null);
  };

  const handleRenameFolder = async (folder: Folder) => {
    if (!editFolderName.trim()) return;
    await updateFolder(folder.id, editFolderName.trim(), folder.parentId);
    setEditingFolderId(null);
  };

  const handleMoveFolder = async (folder: Folder) => {
    const nextParentId = moveFolderParentId === "" ? null : moveFolderParentId;
    await updateFolder(folder.id, folder.name, nextParentId);
    setMovingFolderId(null);
  };

  const handleMoveDocument = async (doc: DocumentSummary) => {
    if (!moveDocSubjectId) return;
    const nextFolderId = moveDocFolderId === "" ? null : moveDocFolderId;
    await moveDocument(doc.id, moveDocSubjectId, nextFolderId);
    setMovingDocId(null);
  };

  const handleCreateTag = async () => {
    if (!newTagName.trim()) return;
    await createTag(newTagName.trim());
    setNewTagName("");
    setShowAddTag(false);
  };

  const handleRenameDocument = async (doc: DocumentSummary) => {
    if (!editDocTitle.trim()) return;
    await renameDocument(doc.id, editDocTitle.trim());
    setEditingDocId(null);
  };

  const toggleDocTag = async (doc: DocumentSummary, tagId: string) => {
    const next = doc.tagIds.includes(tagId)
      ? doc.tagIds.filter((id) => id !== tagId)
      : [...doc.tagIds, tagId];
    await setDocumentTags(doc.id, next);
  };

  const renderFolderBranch = (subjectId: string, parentId: string | null, depth: number) => {
    const children = folders.filter(
      (f) => f.subjectId === subjectId && f.parentId === parentId,
    );
    if (children.length === 0) return null;

    return (
      <div className="space-y-0.5">
        {children.map((folder) => {
          const hasKids = folders.some((f) => f.parentId === folder.id);
          const expanded = isFolderExpanded(folder.id);
          const isSelected = selectedFolderId === folder.id;

          return (
            <div key={folder.id}>
              <div
                style={{ paddingLeft: `${depth * 14 + 12}px` }}
                className={cn(
                  "group flex items-center gap-1 rounded-md py-1 pr-1 text-xs",
                  isSelected
                    ? "bg-blue-100 text-blue-900 dark:bg-blue-900/50 dark:text-blue-100"
                    : "text-slate-700 hover:bg-slate-200/70 dark:text-slate-300 dark:hover:bg-slate-800",
                )}
              >
                <button
                  type="button"
                  aria-label="toggle folder"
                  onClick={() =>
                    setExpandedFolders((prev) => ({ ...prev, [folder.id]: !expanded }))
                  }
                  className="flex h-4 w-4 shrink-0 items-center justify-center text-slate-400"
                >
                  {hasKids ? (
                    expanded ? (
                      <ChevronDown className="h-3 w-3" />
                    ) : (
                      <ChevronRight className="h-3 w-3" />
                    )
                  ) : null}
                </button>

                {editingFolderId === folder.id ? (
                  <Input
                    value={editFolderName}
                    onChange={(e) => setEditFolderName(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") void handleRenameFolder(folder);
                      if (e.key === "Escape") setEditingFolderId(null);
                    }}
                    onBlur={() => setEditingFolderId(null)}
                    autoFocus
                    className="h-6 text-xs"
                  />
                ) : (
                  <button
                    type="button"
                    onClick={() => void selectFolderFilter(subjectId, folder.id)}
                    className="flex min-w-0 flex-1 items-center gap-1.5 text-left"
                  >
                    <FolderIcon className="h-3.5 w-3.5 shrink-0 text-amber-500" />
                    <span className="truncate">{folder.name}</span>
                    {folder.autoUploadDrive && (
                      <Cloud
                        aria-label="Drive 自動アップロード有効"
                        className="h-3 w-3 shrink-0 text-blue-500"
                      />
                    )}
                  </button>
                )}

                <div className="hidden items-center gap-0.5 group-hover:flex">
                  <button
                    type="button"
                    title={
                      folder.autoUploadDrive
                        ? "フォルダ自動アップロードを解除"
                        : "このフォルダを Drive 自動アップロード対象にする"
                    }
                    aria-label={`Drive 自動アップロード切替: ${folder.name}`}
                    onClick={() => void setFolderAutoUpload(folder.id, !folder.autoUploadDrive)}
                    className={cn(
                      "rounded p-0.5 hover:bg-slate-300/70 dark:hover:bg-slate-700",
                      folder.autoUploadDrive && "text-blue-600 dark:text-blue-400",
                    )}
                  >
                    <Cloud className="h-3 w-3" />
                  </button>
                  <button
                    type="button"
                    title={t.library.addFolder}
                    onClick={() => {
                      setAddingFolderTarget({ subjectId, parentId: folder.id });
                      setNewFolderName("");
                    }}
                    className="rounded p-0.5 hover:bg-slate-300/70 dark:hover:bg-slate-700"
                  >
                    <FolderPlus className="h-3 w-3" />
                  </button>
                  <button
                    type="button"
                    title={t.library.moveFolder}
                    aria-label={`${t.library.moveFolder}: ${folder.name}`}
                    onClick={() => {
                      setMovingFolderId((prev) => (prev === folder.id ? null : folder.id));
                      setMoveFolderParentId(folder.parentId ?? "");
                    }}
                    className="rounded p-0.5 hover:bg-slate-300/70 dark:hover:bg-slate-700"
                  >
                    <FolderInput className="h-3 w-3" />
                  </button>
                  <button
                    type="button"
                    title={t.common.rename}
                    onClick={() => {
                      setEditingFolderId(folder.id);
                      setEditFolderName(folder.name);
                    }}
                    className="rounded p-0.5 hover:bg-slate-300/70 dark:hover:bg-slate-700"
                  >
                    <Pencil className="h-3 w-3" />
                  </button>
                  <button
                    type="button"
                    title={t.common.delete}
                    onClick={() => {
                      if (window.confirm(t.library.deleteConfirmFolder)) {
                        void deleteFolder(folder.id);
                      }
                    }}
                    className="rounded p-0.5 text-red-600 hover:bg-red-100 dark:text-red-400 dark:hover:bg-red-950"
                  >
                    <Trash2 className="h-3 w-3" />
                  </button>
                </div>
              </div>

              {movingFolderId === folder.id &&
                (() => {
                  const blockedIds = collectDescendantFolderIds(folders, folder.id);
                  const candidateParents = folders.filter(
                    (f) => f.subjectId === folder.subjectId && !blockedIds.has(f.id),
                  );
                  return (
                    <div
                      style={{ paddingLeft: `${(depth + 1) * 14 + 12}px` }}
                      className="my-1 space-y-1 rounded border border-slate-200 bg-white p-1.5 pr-2 text-xs dark:border-slate-700 dark:bg-slate-900"
                    >
                      <div className="text-[10px] font-medium text-slate-500 dark:text-slate-400">
                        {t.library.moveFolderTarget}
                      </div>
                      <select
                        aria-label={t.library.moveFolderTarget}
                        value={moveFolderParentId}
                        onChange={(e) => setMoveFolderParentId(e.target.value)}
                        className="h-7 w-full rounded border border-slate-300 bg-white px-1.5 text-xs dark:border-slate-700 dark:bg-slate-800"
                      >
                        <option value="">{t.library.folderRootOption}</option>
                        {candidateParents.map((cand) => (
                          <option key={cand.id} value={cand.id}>
                            {cand.name}
                          </option>
                        ))}
                      </select>
                      <div className="flex justify-end gap-1">
                        <Button
                          size="xs"
                          variant="ghost"
                          onClick={() => setMovingFolderId(null)}
                        >
                          {t.common.cancel}
                        </Button>
                        <Button
                          size="xs"
                          variant="primary"
                          onClick={() => void handleMoveFolder(folder)}
                        >
                          {t.common.save}
                        </Button>
                      </div>
                    </div>
                  );
                })()}

              {addingFolderTarget?.parentId === folder.id && (
                <div
                  style={{ paddingLeft: `${(depth + 1) * 14 + 16}px` }}
                  className="my-1 flex items-center gap-1 pr-2"
                >
                  <Input
                    placeholder={t.library.folderNamePlaceholder}
                    value={newFolderName}
                    onChange={(e) => setNewFolderName(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") void handleCreateFolder();
                      if (e.key === "Escape") setAddingFolderTarget(null);
                    }}
                    autoFocus
                    className="h-7 text-xs"
                  />
                  <Button size="xs" variant="primary" onClick={() => void handleCreateFolder()}>
                    {t.common.create}
                  </Button>
                </div>
              )}

              {expanded && renderFolderBranch(subjectId, folder.id, depth + 1)}
            </div>
          );
        })}
      </div>
    );
  };

  return (
    <aside className="flex h-full flex-col border-r border-slate-200 bg-slate-100/80 dark:border-slate-800 dark:bg-slate-900/70">
      {/* Top Action + Search */}
      <div className="space-y-2 border-b border-slate-200 p-3 dark:border-slate-800">
        <Button
          variant="primary"
          size="md"
          className="w-full justify-center"
          disabled={importing}
          onClick={onImportClick}
        >
          <FilePlus2 className="h-4 w-4" />
          {importing ? t.common.loading : t.library.addProblem}
        </Button>

        <div className="relative flex gap-1">
          <Search className="pointer-events-none absolute top-2 left-2.5 h-3.5 w-3.5 text-slate-400" />
          <Input
            ref={searchInputRef}
            value={searchQuery}
            onChange={(e) => {
              const v = e.target.value;
              void setSearchQuery(v);
              if (!v.trim()) clearFullTextSearch();
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" && searchQuery.trim()) {
                void runFullTextSearch(searchQuery);
              }
            }}
            placeholder={t.header.searchPlaceholder}
            aria-label={t.header.searchPlaceholder}
            className="pl-8"
          />
          <Button
            size="xs"
            variant="outline"
            disabled={!searchQuery.trim() || ftsLoading}
            onClick={() => void runFullTextSearch(searchQuery)}
            title="問題文・解答・会話・資料を横断全文検索 (SQLite FTS5)"
          >
            {ftsLoading ? "..." : "全文"}
          </Button>
        </div>

        {/* Phase 5: SQLite FTS5 Cross-Entity Full-Text Search Results */}
        {ftsHits.length > 0 && (
          <div
            aria-label="全文検索結果"
            className="max-h-48 space-y-1 overflow-y-auto rounded-lg border border-blue-200 bg-white p-2 text-[11px] shadow-xs dark:border-blue-900 dark:bg-slate-900"
          >
            <div className="flex items-center justify-between text-[10px] font-semibold text-blue-700 dark:text-blue-300">
              <span>横断全文検索ヒット ({ftsHits.length} 件)</span>
              <button
                type="button"
                onClick={clearFullTextSearch}
                className="text-slate-400 hover:text-slate-700"
              >
                閉じる
              </button>
            </div>
            {ftsHits.map((hit: FullTextSearchHit) => {
              const scopeLabel: Record<string, string> = {
                document: "文書",
                problem: "問題文",
                solution: "解答",
                message: "会話",
                reference: "講義資料",
              };
              return (
                <button
                  key={`${hit.entityType}-${hit.entityId}`}
                  type="button"
                  onClick={() => {
                    if (hit.documentId) {
                      selectDocument(hit.documentId);
                      if (hit.entityType === "problem" || hit.entityType === "reference") {
                        setActiveTab("recognized");
                      } else if (hit.entityType === "solution") {
                        setActiveTab("solutionPdf");
                      } else {
                        setActiveTab("original");
                      }
                    }
                  }}
                  className="block w-full rounded border border-slate-100 p-1.5 text-left hover:bg-blue-50 dark:border-slate-800 dark:hover:bg-blue-950/50"
                >
                  <div className="flex items-center gap-1.5">
                    <Badge variant="info" className="px-1 py-0 text-[9px]">
                      {scopeLabel[hit.entityType] ?? hit.entityType}
                    </Badge>
                    <span className="truncate font-medium text-slate-900 dark:text-slate-100">
                      {hit.documentTitle}
                    </span>
                  </div>
                  <div className="mt-0.5 line-clamp-2 text-[10px] text-slate-600 dark:text-slate-400">
                    {hit.snippet}
                  </div>
                </button>
              );
            })}
          </div>
        )}

        <p className="text-[11px] text-slate-500 dark:text-slate-400">{t.library.pasteHint}</p>
      </div>

      {/* Scrollable Navigation + Document List */}
      <div className="flex-1 overflow-y-auto px-2 py-2 space-y-4">
        {/* Quick Views */}
        <nav className="space-y-0.5" aria-label="クイックフィルタ">
          <button
            type="button"
            onClick={() => void setQuickView("all")}
            className={cn(
              "flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-xs font-medium",
              quickView === "all" && !selectedSubjectId && !selectedTagId
                ? "bg-blue-100 text-blue-900 dark:bg-blue-900/50 dark:text-blue-100"
                : "text-slate-700 hover:bg-slate-200/70 dark:text-slate-300 dark:hover:bg-slate-800",
            )}
          >
            <BookOpen className="h-3.5 w-3.5" />
            <span>{t.library.allDocuments}</span>
          </button>
          <button
            type="button"
            onClick={() => void setQuickView("favorites")}
            className={cn(
              "flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-xs font-medium",
              quickView === "favorites"
                ? "bg-blue-100 text-blue-900 dark:bg-blue-900/50 dark:text-blue-100"
                : "text-slate-700 hover:bg-slate-200/70 dark:text-slate-300 dark:hover:bg-slate-800",
            )}
          >
            <Star className="h-3.5 w-3.5 text-amber-500" />
            <span>{t.library.favorites}</span>
          </button>
          <button
            type="button"
            onClick={() => void setQuickView("recent")}
            className={cn(
              "flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-xs font-medium",
              quickView === "recent"
                ? "bg-blue-100 text-blue-900 dark:bg-blue-900/50 dark:text-blue-100"
                : "text-slate-700 hover:bg-slate-200/70 dark:text-slate-300 dark:hover:bg-slate-800",
            )}
          >
            <Clock className="h-3.5 w-3.5 text-sky-500" />
            <span>{t.library.recent}</span>
          </button>

          {/* Phase 5: Study Status Filter Pills */}
          <div className="flex flex-wrap gap-1 px-1 pt-1.5" aria-label="学習ステータスフィルタ">
            {(
              [
                ["unstarted", "未着手"],
                ["in_progress", "学習中"],
                ["solved", "解決済"],
                ["review_needed", "要復習"],
              ] as [StudyStatus, string][]
            ).map(([st, lbl]) => {
              const active = selectedStudyStatus === st;
              return (
                <button
                  key={st}
                  type="button"
                  onClick={() => void setStudyStatusFilter(st)}
                  className={cn(
                    "rounded-full border px-2 py-0.5 text-[10px] font-medium transition-colors",
                    active
                      ? "border-blue-500 bg-blue-600 text-white"
                      : "border-slate-300 bg-white text-slate-600 hover:bg-slate-100 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-300",
                  )}
                >
                  {lbl}
                </button>
              );
            })}
          </div>
        </nav>

        {/* Subjects & Hierarchical Folders */}
        <section>
          <div className="mb-1 flex items-center justify-between px-2">
            <span className="text-[11px] font-semibold tracking-wider text-slate-500 uppercase dark:text-slate-400">
              {t.library.subjects}
            </span>
            <button
              type="button"
              title={t.library.addSubject}
              onClick={() => setShowAddSubject((v) => !v)}
              className="rounded p-0.5 text-slate-500 hover:bg-slate-200 dark:hover:bg-slate-800"
            >
              <Plus className="h-3.5 w-3.5" />
            </button>
          </div>

          {showAddSubject && (
            <div className="mb-2 space-y-1.5 rounded-md border border-slate-200 bg-white p-2 dark:border-slate-700 dark:bg-slate-900">
              <Input
                placeholder={t.library.subjectNamePlaceholder}
                value={newSubjectName}
                onChange={(e) => setNewSubjectName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void handleCreateSubject();
                }}
                autoFocus
              />
              <div className="flex items-center gap-1.5">
                <select
                  value={newSubjectCategory}
                  onChange={(e) => setNewSubjectCategory(e.target.value as SubjectCategory)}
                  className="h-7 flex-1 rounded border border-slate-300 bg-white px-1.5 text-xs dark:border-slate-700 dark:bg-slate-800"
                >
                  <option value="mathematics">{t.library.categoryMath}</option>
                  <option value="physics">{t.library.categoryPhysics}</option>
                  <option value="other">{t.library.categoryOther}</option>
                </select>
                <Button size="xs" variant="primary" onClick={() => void handleCreateSubject()}>
                  {t.common.create}
                </Button>
              </div>
            </div>
          )}

          <div className="space-y-1">
            {subjects.map((sub) => {
              const expanded = isSubjectExpanded(sub.id);
              const isSelected = selectedSubjectId === sub.id && selectedFolderId === null;

              return (
                <div key={sub.id}>
                  <div
                    className={cn(
                      "group flex items-center gap-1 rounded-md px-1.5 py-1 text-xs font-medium",
                      isSelected
                        ? "bg-blue-100 text-blue-900 dark:bg-blue-900/50 dark:text-blue-100"
                        : "text-slate-800 hover:bg-slate-200/70 dark:text-slate-200 dark:hover:bg-slate-800",
                    )}
                  >
                    <button
                      type="button"
                      aria-label="toggle subject"
                      onClick={() =>
                        setExpandedSubjects((prev) => ({ ...prev, [sub.id]: !expanded }))
                      }
                      className="flex h-4 w-4 shrink-0 items-center justify-center text-slate-400"
                    >
                      {expanded ? (
                        <ChevronDown className="h-3.5 w-3.5" />
                      ) : (
                        <ChevronRight className="h-3.5 w-3.5" />
                      )}
                    </button>

                    <button
                      type="button"
                      onClick={() => void selectSubjectFilter(sub.id)}
                      className="min-w-0 flex-1 truncate text-left"
                    >
                      {sub.name}
                    </button>

                    <div className="hidden items-center gap-0.5 group-hover:flex">
                      <button
                        type="button"
                        title={t.library.addFolder}
                        onClick={() => {
                          setExpandedSubjects((prev) => ({ ...prev, [sub.id]: true }));
                          setAddingFolderTarget({ subjectId: sub.id, parentId: null });
                          setNewFolderName("");
                        }}
                        className="rounded p-0.5 hover:bg-slate-300/70 dark:hover:bg-slate-700"
                      >
                        <FolderPlus className="h-3 w-3" />
                      </button>
                      <button
                        type="button"
                        title={t.library.editSubject}
                        aria-label={`${t.library.editSubject}: ${sub.name}`}
                        onClick={() => {
                          setEditingSubjectId((prev) => (prev === sub.id ? null : sub.id));
                          setEditSubjectName(sub.name);
                          setEditSubjectCategory(sub.category);
                        }}
                        className="rounded p-0.5 hover:bg-slate-300/70 dark:hover:bg-slate-700"
                      >
                        <Pencil className="h-3 w-3" />
                      </button>
                      <button
                        type="button"
                        title={t.common.delete}
                        onClick={() => {
                          if (window.confirm(t.library.deleteConfirmSubject)) {
                            void deleteSubject(sub.id);
                          }
                        }}
                        className="rounded p-0.5 text-red-600 hover:bg-red-100 dark:text-red-400 dark:hover:bg-red-950"
                      >
                        <Trash2 className="h-3 w-3" />
                      </button>
                    </div>
                  </div>

                  {editingSubjectId === sub.id && (
                    <div className="my-1 ml-5 space-y-1.5 rounded-md border border-slate-200 bg-white p-2 dark:border-slate-700 dark:bg-slate-900">
                      <Input
                        value={editSubjectName}
                        onChange={(e) => setEditSubjectName(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") void handleUpdateSubject(sub);
                          if (e.key === "Escape") setEditingSubjectId(null);
                        }}
                        autoFocus
                        className="h-7 text-xs"
                      />
                      <div className="flex items-center gap-1">
                        <select
                          aria-label="科目カテゴリ"
                          value={editSubjectCategory}
                          onChange={(e) =>
                            setEditSubjectCategory(e.target.value as SubjectCategory)
                          }
                          className="h-7 flex-1 rounded border border-slate-300 bg-white px-1.5 text-xs dark:border-slate-700 dark:bg-slate-800"
                        >
                          <option value="mathematics">{t.library.categoryMath}</option>
                          <option value="physics">{t.library.categoryPhysics}</option>
                          <option value="other">{t.library.categoryOther}</option>
                        </select>
                        <Button
                          size="xs"
                          variant="ghost"
                          onClick={() => setEditingSubjectId(null)}
                        >
                          {t.common.cancel}
                        </Button>
                        <Button
                          size="xs"
                          variant="primary"
                          onClick={() => void handleUpdateSubject(sub)}
                        >
                          {t.common.save}
                        </Button>
                      </div>
                    </div>
                  )}

                  {addingFolderTarget?.subjectId === sub.id &&
                    addingFolderTarget.parentId === null && (
                      <div className="my-1 flex items-center gap-1 pr-2 pl-6">
                        <Input
                          placeholder={t.library.folderNamePlaceholder}
                          value={newFolderName}
                          onChange={(e) => setNewFolderName(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === "Enter") void handleCreateFolder();
                            if (e.key === "Escape") setAddingFolderTarget(null);
                          }}
                          autoFocus
                          className="h-7 text-xs"
                        />
                        <Button size="xs" variant="primary" onClick={() => void handleCreateFolder()}>
                          {t.common.create}
                        </Button>
                      </div>
                    )}

                  {expanded && renderFolderBranch(sub.id, null, 1)}
                </div>
              );
            })}
          </div>
        </section>

        {/* Tags */}
        <section>
          <div className="mb-1.5 flex items-center justify-between px-2">
            <span className="text-[11px] font-semibold tracking-wider text-slate-500 uppercase dark:text-slate-400">
              {t.library.tags}
            </span>
            <button
              type="button"
              title={t.library.addTag}
              onClick={() => setShowAddTag((v) => !v)}
              className="rounded p-0.5 text-slate-500 hover:bg-slate-200 dark:hover:bg-slate-800"
            >
              <Plus className="h-3.5 w-3.5" />
            </button>
          </div>

          {showAddTag && (
            <div className="mb-2 flex items-center gap-1 px-1">
              <Input
                placeholder={t.library.tagNamePlaceholder}
                value={newTagName}
                onChange={(e) => setNewTagName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void handleCreateTag();
                }}
                autoFocus
                className="h-7 text-xs"
              />
              <Button size="xs" variant="primary" onClick={() => void handleCreateTag()}>
                {t.common.create}
              </Button>
            </div>
          )}

          <div className="flex flex-wrap gap-1 px-1.5">
            {tags.map((tag) => {
              const active = selectedTagId === tag.id;
              return (
                <div
                  key={tag.id}
                  className={cn(
                    "inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11px]",
                    active
                      ? "border-blue-500 bg-blue-100 text-blue-900 dark:bg-blue-900/50 dark:text-blue-100"
                      : "border-slate-300 bg-white text-slate-700 hover:bg-slate-50 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-300",
                  )}
                >
                  <button
                    type="button"
                    onClick={() => void selectTagFilter(tag.id)}
                    className="inline-flex items-center gap-1"
                  >
                    <TagIcon className="h-2.5 w-2.5" />
                    <span>{tag.name}</span>
                  </button>
                  <button
                    type="button"
                    aria-label={`delete tag ${tag.name}`}
                    onClick={() => void deleteTag(tag.id)}
                    className="text-slate-400 hover:text-red-500"
                  >
                    ×
                  </button>
                </div>
              );
            })}
          </div>
        </section>

        {/* Document List */}
        <section>
          <div className="mb-1.5 flex items-center justify-between px-2">
            <span className="text-[11px] font-semibold tracking-wider text-slate-500 uppercase dark:text-slate-400">
              {t.library.documentsHeading} ({documents.length})
            </span>
          </div>

          {documents.length === 0 ? (
            <p className="px-2 py-4 text-center text-xs text-slate-500 dark:text-slate-400">
              {t.library.emptyDocuments}
            </p>
          ) : (
            <ul className="space-y-1">
              {documents.map((doc) => {
                const isSelected = selectedDocumentId === doc.id;
                return (
                  <li key={doc.id}>
                    <div
                      className={cn(
                        "group rounded-md border px-2 py-1.5 transition-colors",
                        isSelected
                          ? "border-blue-500 bg-blue-50 dark:border-blue-500 dark:bg-blue-950/60"
                          : "border-transparent bg-white/70 hover:border-slate-300 dark:bg-slate-900/50 dark:hover:border-slate-700",
                      )}
                    >
                      <div className="flex items-start gap-2">
                        <button
                          type="button"
                          onClick={() => selectDocument(doc.id)}
                          className="flex min-w-0 flex-1 items-start gap-2 text-left"
                        >
                          {doc.sourceType === "pdf" ? (
                            <FileText className="mt-0.5 h-4 w-4 shrink-0 text-red-500" />
                          ) : (
                            <FileImage className="mt-0.5 h-4 w-4 shrink-0 text-emerald-500" />
                          )}
                          <div className="min-w-0 flex-1">
                            {editingDocId === doc.id ? (
                              <Input
                                value={editDocTitle}
                                onClick={(e) => e.stopPropagation()}
                                onChange={(e) => setEditDocTitle(e.target.value)}
                                onKeyDown={(e) => {
                                  if (e.key === "Enter") void handleRenameDocument(doc);
                                  if (e.key === "Escape") setEditingDocId(null);
                                }}
                                onBlur={() => setEditingDocId(null)}
                                autoFocus
                                className="h-6 text-xs"
                              />
                            ) : (
                              <div className="truncate text-xs font-medium text-slate-900 dark:text-slate-100">
                                {doc.title}
                              </div>
                            )}
                            <div className="mt-0.5 flex flex-wrap items-center gap-1.5 text-[10px] text-slate-500 dark:text-slate-400">
                              <span>{doc.sourceType.toUpperCase()}</span>
                              {(() => {
                                const stMap: Record<string, { label: string; variant: "default" | "info" | "warning" | "outline" }> = {
                                  unstarted: { label: "未着手", variant: "outline" },
                                  in_progress: { label: "学習中", variant: "info" },
                                  solved: { label: "解決済", variant: "default" },
                                  review_needed: { label: "要復習", variant: "warning" },
                                };
                                const item = stMap[doc.studyStatus];
                                return item ? (
                                  <Badge variant={item.variant} className="px-1 py-0 text-[9px]">
                                    {item.label}
                                  </Badge>
                                ) : null;
                              })()}
                              {doc.pageCount ? (
                                <span>
                                  {doc.pageCount} {t.library.pagesUnit}
                                </span>
                              ) : null}
                              {doc.tagIds.map((tid) => {
                                const tg = tags.find((x) => x.id === tid);
                                return tg ? (
                                  <Badge key={tid} variant="info" className="px-1 py-0 text-[10px]">
                                    {tg.name}
                                  </Badge>
                                ) : null;
                              })}
                              {(() => {
                                const summary = driveOverview?.documentSummaries.find(
                                  (s) => s.documentId === doc.id,
                                );
                                if (!summary || summary.aggregateStatus === "none") return null;
                                const labelMap: Record<string, string> = {
                                  uploaded: "Drive 済",
                                  uploading: "Drive 送信中",
                                  pending: "Drive 待機",
                                  update_required: "Drive 更新要",
                                  conflict: "Drive 競合",
                                  remote_deleted: "Drive 削除検出",
                                  failed: "Drive 失敗",
                                };
                                const variant =
                                  summary.aggregateStatus === "uploaded"
                                    ? "outline"
                                    : summary.aggregateStatus === "conflict" ||
                                        summary.aggregateStatus === "failed" ||
                                        summary.aggregateStatus === "remote_deleted"
                                      ? "warning"
                                      : "info";
                                return (
                                  <Badge variant={variant} className="px-1 py-0 text-[9px]">
                                    {labelMap[summary.aggregateStatus] ?? summary.aggregateStatus}
                                  </Badge>
                                );
                              })()}
                            </div>
                          </div>
                        </button>

                        <div className="flex shrink-0 items-center gap-0.5">
                          {driveConnected && (
                            <button
                              type="button"
                              title="Google Drive へアップロード / 更新"
                              aria-label={`Google Drive へアップロード: ${doc.title}`}
                              disabled={uploadingDocId === doc.id}
                              onClick={() =>
                                void uploadDocumentToDrive({
                                  documentId: doc.id,
                                  scope: "default",
                                })
                              }
                              className="hidden rounded p-0.5 text-blue-600 hover:bg-blue-100 group-hover:inline-flex dark:text-blue-400 dark:hover:bg-blue-950"
                            >
                              <CloudUpload className="h-3.5 w-3.5" />
                            </button>
                          )}
                          <button
                            type="button"
                            title={t.library.favorites}
                            onClick={() => void toggleFavorite(doc)}
                            className="rounded p-0.5 hover:bg-slate-200 dark:hover:bg-slate-800"
                          >
                            <Star
                              className={cn(
                                "h-3.5 w-3.5",
                                doc.isFavorite
                                  ? "fill-amber-400 text-amber-500"
                                  : "text-slate-400",
                              )}
                            />
                          </button>
                          <button
                            type="button"
                            title={t.library.moveDocument}
                            aria-label={`${t.library.moveDocument}: ${doc.title}`}
                            onClick={() => {
                              setMovingDocId((prev) => (prev === doc.id ? null : doc.id));
                              setMoveDocSubjectId(doc.subjectId);
                              setMoveDocFolderId(doc.folderId ?? "");
                            }}
                            className="hidden rounded p-0.5 text-slate-400 hover:bg-slate-200 group-hover:inline-flex dark:hover:bg-slate-800"
                          >
                            <FolderInput className="h-3.5 w-3.5" />
                          </button>
                          <button
                            type="button"
                            title={t.library.tags}
                            onClick={() =>
                              setTaggingDocId((prev) => (prev === doc.id ? null : doc.id))
                            }
                            className="hidden rounded p-0.5 text-slate-400 hover:bg-slate-200 group-hover:inline-flex dark:hover:bg-slate-800"
                          >
                            <TagIcon className="h-3.5 w-3.5" />
                          </button>
                          <button
                            type="button"
                            title={t.common.rename}
                            onClick={() => {
                              setEditingDocId(doc.id);
                              setEditDocTitle(doc.title);
                            }}
                            className="hidden rounded p-0.5 text-slate-400 hover:bg-slate-200 group-hover:inline-flex dark:hover:bg-slate-800"
                          >
                            <Pencil className="h-3.5 w-3.5" />
                          </button>
                          <button
                            type="button"
                            title={t.common.delete}
                            onClick={() => {
                              if (window.confirm(t.library.deleteConfirmDocument)) {
                                void deleteDocument(doc.id);
                              }
                            }}
                            className="hidden rounded p-0.5 text-red-500 hover:bg-red-100 group-hover:inline-flex dark:hover:bg-red-950"
                          >
                            <Trash2 className="h-3.5 w-3.5" />
                          </button>
                        </div>
                      </div>

                      {movingDocId === doc.id && (
                        <div className="mt-1.5 space-y-1.5 border-t border-slate-200 pt-1.5 text-xs dark:border-slate-700">
                          <div className="space-y-1">
                            <label className="block text-[10px] font-medium text-slate-500 dark:text-slate-400">
                              {t.library.selectSubjectLabel}
                            </label>
                            <select
                              aria-label={t.library.selectSubjectLabel}
                              value={moveDocSubjectId}
                              onChange={(e) => {
                                const nextSub = e.target.value;
                                setMoveDocSubjectId(nextSub);
                                setMoveDocFolderId("");
                              }}
                              className="h-7 w-full rounded border border-slate-300 bg-white px-1.5 text-xs dark:border-slate-700 dark:bg-slate-800"
                            >
                              {subjects.map((sub) => (
                                <option key={sub.id} value={sub.id}>
                                  {sub.name}
                                </option>
                              ))}
                            </select>
                          </div>
                          <div className="space-y-1">
                            <label className="block text-[10px] font-medium text-slate-500 dark:text-slate-400">
                              {t.library.selectFolderLabel}
                            </label>
                            <select
                              aria-label={t.library.selectFolderLabel}
                              value={moveDocFolderId}
                              onChange={(e) => setMoveDocFolderId(e.target.value)}
                              className="h-7 w-full rounded border border-slate-300 bg-white px-1.5 text-xs dark:border-slate-700 dark:bg-slate-800"
                            >
                              <option value="">{t.library.noFolderOption}</option>
                              {folders
                                .filter((f) => f.subjectId === moveDocSubjectId)
                                .map((f) => (
                                  <option key={f.id} value={f.id}>
                                    {f.name}
                                  </option>
                                ))}
                            </select>
                          </div>
                          <div className="flex justify-end gap-1">
                            <Button
                              size="xs"
                              variant="ghost"
                              onClick={() => setMovingDocId(null)}
                            >
                              {t.common.cancel}
                            </Button>
                            <Button
                              size="xs"
                              variant="primary"
                              onClick={() => void handleMoveDocument(doc)}
                            >
                              {t.common.save}
                            </Button>
                          </div>
                        </div>
                      )}

                      {taggingDocId === doc.id && tags.length > 0 && (
                        <div className="mt-1.5 flex flex-wrap gap-1 border-t border-slate-200 pt-1.5 dark:border-slate-700">
                          {tags.map((tg) => {
                            const attached = doc.tagIds.includes(tg.id);
                            return (
                              <button
                                key={tg.id}
                                type="button"
                                onClick={() => void toggleDocTag(doc, tg.id)}
                                className={cn(
                                  "rounded px-1.5 py-0.5 text-[10px]",
                                  attached
                                    ? "bg-blue-600 text-white"
                                    : "bg-slate-200 text-slate-700 dark:bg-slate-800 dark:text-slate-300",
                                )}
                              >
                                {attached ? "✓ " : "+ "}
                                {tg.name}
                              </button>
                            );
                          })}
                        </div>
                      )}
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
        </section>
      </div>

      {/* Footer Settings Button */}
      <div className="border-t border-slate-200 p-2 dark:border-slate-800">
        <Button
          variant="ghost"
          size="sm"
          className="w-full justify-start"
          onClick={() => openSettings("general")}
        >
          <Settings className="h-4 w-4" />
          <span>{t.header.settingsButton}</span>
          <span className="ml-auto text-[10px] text-slate-400">Ctrl+,</span>
        </Button>
      </div>
    </aside>
  );
}
