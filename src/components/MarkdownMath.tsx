import { useState } from "react";
import Markdown from "react-markdown";
import rehypeKatex from "rehype-katex";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import { Check, Copy } from "lucide-react";
import "katex/dist/katex.min.css";
import { cn } from "@/lib/utils";

interface MarkdownMathProps {
  content: string;
  className?: string;
}

/**
 * Renders Markdown + KaTeX math safely.
 * Raw HTML is NEVER enabled (`rehype-raw` is intentionally excluded per security policy).
 */
export function MarkdownMath({ content, className }: MarkdownMathProps) {
  return (
    <div
      className={cn(
        "prose-sm max-w-none break-words text-xs leading-relaxed text-slate-800 dark:text-slate-200",
        "[&_p]:my-1.5 [&_ul]:my-1.5 [&_ul]:list-disc [&_ul]:pl-5 [&_ol]:my-1.5 [&_ol]:list-decimal [&_ol]:pl-5",
        "[&_h1]:mt-3 [&_h1]:mb-1.5 [&_h1]:text-sm [&_h1]:font-bold [&_h2]:mt-2.5 [&_h2]:mb-1 [&_h2]:text-xs [&_h2]:font-bold",
        "[&_blockquote]:border-l-2 [&_blockquote]:border-slate-300 [&_blockquote]:pl-3 [&_blockquote]:text-slate-600 dark:[&_blockquote]:border-slate-700 dark:[&_blockquote]:text-slate-400",
        "[&_table]:my-2 [&_table]:w-full [&_table]:border-collapse [&_th]:border [&_th]:border-slate-300 [&_th]:bg-slate-100 [&_th]:px-2 [&_th]:py-1 dark:[&_th]:border-slate-700 dark:[&_th]:bg-slate-800",
        "[&_td]:border [&_td]:border-slate-300 [&_td]:px-2 [&_td]:py-1 dark:[&_td]:border-slate-700",
        "[&_.katex-display]:my-2 [&_.katex-display]:overflow-x-auto [&_.katex-display]:py-1",
        className,
      )}
    >
      <Markdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[rehypeKatex]}
        components={{
          pre({ children }) {
            return <CodeBlockWrapper>{children}</CodeBlockWrapper>;
          },
          code({ className: codeClass, children, ...props }) {
            const isBlock = Boolean(codeClass && codeClass.startsWith("language-"));
            if (!isBlock) {
              return (
                <code
                  className="rounded bg-slate-200/80 px-1 py-0.5 font-mono text-[11px] text-slate-900 dark:bg-slate-800 dark:text-slate-100"
                  {...props}
                >
                  {children}
                </code>
              );
            }
            return (
              <code className={cn("font-mono text-[11px]", codeClass)} {...props}>
                {children}
              </code>
            );
          },
        }}
      >
        {content}
      </Markdown>
    </div>
  );
}

function CodeBlockWrapper({ children }: { children: React.ReactNode }) {
  const [copied, setCopied] = useState(false);

  const extractText = (node: React.ReactNode): string => {
    if (typeof node === "string") return node;
    if (Array.isArray(node)) return node.map(extractText).join("");
    if (node && typeof node === "object" && "props" in node) {
      const el = node as { props?: { children?: React.ReactNode } };
      return extractText(el.props?.children);
    }
    return "";
  };

  const handleCopy = () => {
    const text = extractText(children);
    if (navigator.clipboard) {
      void navigator.clipboard.writeText(text).then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      });
    }
  };

  return (
    <div className="group relative my-2 overflow-hidden rounded-md border border-slate-200 bg-slate-900 text-slate-100 dark:border-slate-700">
      <button
        type="button"
        onClick={handleCopy}
        className="absolute top-1.5 right-1.5 inline-flex items-center gap-1 rounded bg-slate-800/90 px-1.5 py-0.5 text-[10px] text-slate-300 opacity-0 transition-opacity hover:bg-slate-700 group-hover:opacity-100"
        title="コードをコピー"
      >
        {copied ? <Check className="h-3 w-3 text-emerald-400" /> : <Copy className="h-3 w-3" />}
        <span>{copied ? "コピー済" : "コピー"}</span>
      </button>
      <pre className="overflow-x-auto p-3 text-[11px] leading-relaxed">{children}</pre>
    </div>
  );
}
