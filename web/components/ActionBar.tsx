"use client";

import { useState, useTransition } from "react";
import {
  Braces,
  ListRestart,
  RefreshCw,
  Send,
  Sparkles,
} from "lucide-react";
import {
  refreshJira,
  runEstimate,
  runInfer,
  runSync,
} from "@/app/actions";
import { Toast } from "./Toast";

interface Props {
  day: string;
  cacheCount: number;
  cacheLast: string | null;
}

type Msg = { tone: "ok" | "error"; text: string } | null;

export function ActionBar({ day, cacheCount, cacheLast }: Props) {
  const [pending, start] = useTransition();
  const [toast, setToast] = useState<Msg>(null);

  const show = (m: Msg) => {
    setToast(m);
    if (m) setTimeout(() => setToast(null), 3500);
  };

  const guard =
    <R,>(fn: () => Promise<R>, label: string) =>
    () => {
      start(async () => {
        try {
          const r = await fn();
          show({ tone: "ok", text: `${label}: ${summarise(r)}` });
        } catch (e) {
          show({ tone: "error", text: `${label} failed — ${(e as Error).message}` });
        }
      });
    };

  return (
    <div className="actions">
      <button
        type="button"
        className="action-btn"
        disabled={pending}
        onClick={guard(() => runInfer(day), "Rebuilt blocks")}
        title="Cluster today's events into blocks (idempotent)"
      >
        <ListRestart />
        Rebuild blocks
      </button>

      <button
        type="button"
        className="action-btn"
        disabled={pending}
        onClick={guard(() => runEstimate(day), "Estimated")}
        title="Use claude -p to fill tickets/descriptions for un-estimated blocks"
      >
        <Sparkles />
        Estimate with Claude
      </button>

      <button
        type="button"
        className="action-btn"
        disabled={pending}
        onClick={guard(() => refreshJira(day), "Refreshed Jira")}
        title={
          cacheLast
            ? `${cacheCount} tickets cached · last ${new Date(cacheLast).toLocaleString()}`
            : "Fetch open tickets from Jira"
        }
      >
        <RefreshCw />
        Refresh Jira
      </button>

      <button
        type="button"
        className="action-btn"
        disabled={pending}
        onClick={guard(() => runSync(day, true), "Dry-run")}
        title="Show what would be posted to Tempo — no network writes"
      >
        <Braces />
        Dry-run sync
      </button>

      <button
        type="button"
        className="action-btn"
        disabled={pending}
        onClick={guard(() => runSync(day, false), "Synced to Tempo")}
        title="Post un-synced blocks to Tempo"
      >
        <Send />
        Sync to Tempo
      </button>

      {toast && <Toast tone={toast.tone}>{toast.text}</Toast>}
    </div>
  );
}

function summarise(r: unknown): string {
  if (r && typeof r === "object") {
    const o = r as Record<string, unknown>;
    if ("estimated" in o)
      return `${o.estimated} estimated · ${o.skipped ?? 0} skipped · ${o.failed ?? 0} failed`;
    if ("synced" in o) {
      const errs = Array.isArray(o.errors) ? o.errors.length : 0;
      return `${o.synced} synced · ${o.skipped ?? 0} skipped · ${errs} error${errs === 1 ? "" : "s"}${o.dry_run ? " (dry-run)" : ""}`;
    }
    if ("blocks" in o) return `${o.blocks} blocks · ${o.minutes ?? 0} min`;
    if ("tickets_written" in o) return `${o.tickets_written} tickets`;
  }
  return "ok";
}
