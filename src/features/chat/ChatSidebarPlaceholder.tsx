import { MessageSquare, Sparkles } from "lucide-react";
import { Badge } from "@/components/ui/Badge";
import { useI18n } from "@/lib/i18n";

export function ChatSidebarPlaceholder() {
  const t = useI18n();

  return (
    <aside
      aria-label={t.chat.title}
      className="flex h-full flex-col border-l border-slate-200 bg-slate-100/80 dark:border-slate-800 dark:bg-slate-900/70"
    >
      <div className="flex items-center justify-between border-b border-slate-200 px-3 py-2.5 dark:border-slate-800">
        <div className="flex items-center gap-1.5 text-xs font-semibold text-slate-800 dark:text-slate-100">
          <MessageSquare className="h-4 w-4 text-blue-600 dark:text-blue-400" />
          <span>{t.chat.title}</span>
        </div>
        <Badge variant="outline">{t.chat.phase2Badge}</Badge>
      </div>

      <div className="flex flex-1 flex-col items-center justify-center p-5 text-center">
        <div className="mb-3 flex h-10 w-10 items-center justify-center rounded-xl bg-blue-100 text-blue-600 dark:bg-blue-900/40 dark:text-blue-300">
          <Sparkles className="h-5 w-5" />
        </div>
        <p className="text-xs font-medium text-slate-700 dark:text-slate-200">
          {t.chat.description}
        </p>
        <p className="mt-2 text-[11px] leading-relaxed text-slate-500 dark:text-slate-400">
          {t.chat.readyNote}
        </p>
      </div>

      <div className="border-t border-slate-200 p-3 dark:border-slate-800">
        <textarea
          disabled
          rows={2}
          placeholder={t.chat.inputPlaceholder}
          className="w-full resize-none rounded-md border border-slate-300 bg-slate-200/60 p-2 text-xs text-slate-500 dark:border-slate-700 dark:bg-slate-800/60 dark:text-slate-400"
        />
      </div>
    </aside>
  );
}
