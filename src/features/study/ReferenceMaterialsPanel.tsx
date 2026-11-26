import { useState } from "react";
import { BookOpen, ChevronDown, ChevronRight, Plus, Shield, Trash2 } from "lucide-react";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { useStudyStore } from "@/stores/useStudyStore";
import type { ReferenceMaterialKind } from "@/types/ipc";

interface ReferenceMaterialsPanelProps {
  documentId: string;
  subjectId: string | null;
}

const KIND_LABELS: Record<ReferenceMaterialKind, string> = {
  lecture_note: "講義ノート",
  textbook_excerpt: "教科書抜粋",
  formula_sheet: "公式集",
  handwritten_attempt: "手書き答案メモ",
};

export function ReferenceMaterialsPanel({
  documentId,
  subjectId,
}: ReferenceMaterialsPanelProps) {
  const {
    references,
    selectedReferenceIds,
    createReference,
    deleteReference,
    toggleReferenceSelection,
  } = useStudyStore();

  const [expanded, setExpanded] = useState(false);
  const [adding, setAdding] = useState(false);
  const [title, setTitle] = useState("");
  const [kind, setKind] = useState<ReferenceMaterialKind>("lecture_note");
  const [content, setContent] = useState("");

  const handleAdd = async () => {
    if (!title.trim() || !content.trim()) return;
    const created = await createReference({
      documentId,
      subjectId,
      title: title.trim(),
      kind,
      contentMarkdown: content.trim(),
    });
    if (created) {
      setTitle("");
      setContent("");
      setAdding(false);
    }
  };

  return (
    <div className="border-b border-slate-200 bg-slate-50/80 px-3 py-1.5 text-xs dark:border-slate-800 dark:bg-slate-900/50">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <button
          type="button"
          onClick={() => setExpanded((v) => !v)}
          className="inline-flex items-center gap-1.5 font-medium text-slate-700 hover:text-slate-950 dark:text-slate-200"
        >
          {expanded ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
          <BookOpen className="h-3.5 w-3.5 text-blue-600 dark:text-blue-400" />
          <span>講義資料・参考資料の選択参照</span>
          <Badge variant="outline" className="text-[10px]">
            選択中 {selectedReferenceIds.length} / {references.length}
          </Badge>
        </button>

        <Button
          size="xs"
          variant="outline"
          onClick={() => {
            setExpanded(true);
            setAdding((v) => !v);
          }}
        >
          <Plus className="h-3 w-3" />
          <span>講義資料を追加</span>
        </Button>
      </div>

      {expanded && (
        <div className="mt-2 space-y-2">
          <div className="flex items-center gap-1 text-[10px] text-slate-500 dark:text-slate-400">
            <Shield className="h-3 w-3 text-emerald-600" />
            <span>
              チェックした資料のみ AI へ選択的に渡され、非信頼データ境界で隔離して扱われます。
            </span>
          </div>

          {adding && (
            <div className="space-y-2 rounded-lg border border-blue-200 bg-white p-2.5 dark:border-blue-900 dark:bg-slate-900">
              <div className="grid gap-2 sm:grid-cols-[1fr_140px]">
                <Input
                  aria-label="参考資料タイトル"
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                  placeholder="例: 第4回講義ノート（フーリエ変換の性質）"
                />
                <select
                  aria-label="参考資料種別"
                  value={kind}
                  onChange={(e) => setKind(e.target.value as ReferenceMaterialKind)}
                  className="h-8 rounded-md border border-slate-300 bg-white px-2 text-xs dark:border-slate-700 dark:bg-slate-800"
                >
                  <option value="lecture_note">講義ノート</option>
                  <option value="textbook_excerpt">教科書抜粋</option>
                  <option value="formula_sheet">公式集</option>
                  <option value="handwritten_attempt">手書き答案メモ</option>
                </select>
              </div>
              <textarea
                aria-label="参考資料内容"
                rows={3}
                value={content}
                onChange={(e) => setContent(e.target.value)}
                placeholder="参照させたい定理・公式・講義ノート抜粋（Markdown / LaTeX 可）"
                className="w-full rounded-md border border-slate-300 bg-white p-2 text-xs dark:border-slate-700 dark:bg-slate-950"
              />
              <div className="flex justify-end gap-1.5">
                <Button size="xs" variant="ghost" onClick={() => setAdding(false)}>
                  キャンセル
                </Button>
                <Button
                  size="xs"
                  variant="primary"
                  disabled={!title.trim() || !content.trim()}
                  onClick={() => void handleAdd()}
                >
                  資料を保存
                </Button>
              </div>
            </div>
          )}

          {references.length === 0 ? (
            <p className="text-[11px] text-slate-500">
              登録済みの講義資料・参考資料はまだありません。
            </p>
          ) : (
            <ul className="space-y-1 max-h-36 overflow-y-auto">
              {references.map((r) => {
                const checked = selectedReferenceIds.includes(r.id);
                return (
                  <li
                    key={r.id}
                    className="flex items-center justify-between gap-2 rounded border border-slate-200 bg-white px-2 py-1 dark:border-slate-800 dark:bg-slate-900"
                  >
                    <label className="flex min-w-0 flex-1 items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={checked}
                        onChange={() => toggleReferenceSelection(r.id)}
                      />
                      <Badge variant="outline" className="shrink-0 text-[9px]">
                        {KIND_LABELS[r.kind] ?? r.kind}
                      </Badge>
                      <span className="truncate font-medium text-slate-800 dark:text-slate-100">
                        {r.title}
                      </span>
                    </label>
                    <button
                      type="button"
                      aria-label={`参考資料を削除: ${r.title}`}
                      onClick={() => void deleteReference(r.id)}
                      className="rounded p-0.5 text-slate-400 hover:bg-red-100 hover:text-red-600 dark:hover:bg-red-950"
                    >
                      <Trash2 className="h-3 w-3" />
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      )}
    </div>
  );
}
