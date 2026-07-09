/**
 * Shared step scaffolding + the props contract every wizard step consumes (W3).
 */

import type { ReactNode } from "react";
import type { ServerStatusResponse } from "@/types";

/** Props passed to every onboarding step body. */
export interface StepProps {
  accessToken: string | null;
  serverStatus: ServerStatusResponse | null;
  /** Advance to the next step (or finish on the last step). */
  onNext: () => void;
  /** Refresh the server status (Step A wires this to App's refresh). */
  onRefreshServer?: () => Promise<void>;
}

interface StepShellProps {
  title: string;
  description: string;
  children: ReactNode;
}

/** Consistent header + scrollable body for a step inside the Dialog. */
export function StepShell({ title, description, children }: StepShellProps) {
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="mb-3 shrink-0">
        <h2 className="text-lg font-semibold leading-tight">{title}</h2>
        <p className="mt-1 text-sm text-muted-foreground">{description}</p>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto pr-1">{children}</div>
    </div>
  );
}

/** Small inline status line used across steps (waiting / ok / warn / error). */
export function StatusLine({ tone, children }: { tone: "ok" | "warn" | "error" | "muted"; children: ReactNode }) {
  const cls = {
    ok: "border-emerald-500/40 bg-emerald-500/10 text-emerald-400",
    warn: "border-amber-500/40 bg-amber-500/10 text-amber-500",
    error: "border-destructive/40 bg-destructive/10 text-destructive",
    muted: "border-border/60 bg-secondary/30 text-muted-foreground",
  }[tone];
  return <p className={`rounded-md border px-3 py-2 text-sm ${cls}`}>{children}</p>;
}
