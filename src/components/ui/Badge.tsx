import type { HTMLAttributes } from "react";
import { cn } from "@/lib/utils";

type BadgeVariant = "default" | "info" | "warning" | "outline";

export interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  variant?: BadgeVariant;
}

const styles: Record<BadgeVariant, string> = {
  default: "bg-slate-200 text-slate-700 dark:bg-slate-800 dark:text-slate-300",
  info: "bg-blue-100 text-blue-800 dark:bg-blue-900/50 dark:text-blue-200",
  warning: "bg-amber-100 text-amber-900 dark:bg-amber-900/40 dark:text-amber-200",
  outline: "border border-slate-300 text-slate-700 dark:border-slate-700 dark:text-slate-300",
};

export function Badge({ className, variant = "default", ...props }: BadgeProps) {
  return (
    <span
      className={cn(
        "inline-flex items-center rounded px-1.5 py-0.5 text-[11px] font-medium",
        styles[variant],
        className,
      )}
      {...props}
    />
  );
}
