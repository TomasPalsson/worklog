"use client";

import { useState } from "react";
import {
  Braces,
  Check,
  ListRestart,
  RefreshCw,
  Send,
  Sparkles,
  X,
} from "lucide-react";
import {
  refreshJira,
  runEstimate,
  runInfer,
  runSync,
} from "@/app/actions";
import type { ActionResult } from "@/app/actions";
import { toast } from "@/lib/toast";

interface Props {
  day: string;
  cacheCount: number;
  cacheLast: string | null;
}

type ActionId = "infer" | "estimate" | "jira" | "dry-run" | "sync";

export function ActionBar({ day, cacheCount, cacheLast }: Props) {
  // Per-button pending set so one slow action doesn't freeze the rest.
  const [pending, setPending] = useState<Set<ActionId>>(new Set());
  const [confirmSync, setConfirmSync] = useState(false);

  const isPending = (id: ActionId) => pending.has(id);

  async function run<R>(
    id: ActionId,
    label: string,
    fn: () => Promise<ActionResult<R> | R>,
  ) {
    setPending((p) => new Set(p).add(id));
    try {
      const res = await fn();
      // Heuristic: if the returned value has `ok: boolean`, it's a
      // tagged ActionResult. Otherwise assume raw success.
      if (res && typeof res === "object" && "ok" in res) {
        const tagged = res as ActionResult<R>;
        if (tagged.ok) toast.ok(`${label}: ${summarise(tagged.data)}`);
        else toast.error(`${label} failed — ${tagged.error}`);
      } else {
        toast.ok(`${label}: ${summarise(res)}`);
      }
    } catch (e) {
      toast.error(`${label} failed — ${(e as Error).message}`);
    } finally {
      setPending((p) => {
        const n = new Set(p);
        n.delete(id);
        return n;
      });
    }
  }

  const onSyncClick = () => {
    if (!confirmSync) {
      setConfirmSync(true);
      // Give the user 4s to confirm before reverting.
      setTimeout(() => setConfirmSync(false), 4000);
      return;
    }
    setConfirmSync(false);
    void run("sync", "Synced to Tempo", () => runSync(day, false));
  };

  return (
    <div className="actions">
      <ActionButton
        pending={isPending("infer")}
        icon={<ListRestart />}
        label="Rebuild blocks"
        pendingLabel="Rebuilding…"
        title="Cluster today's events into blocks (idempotent)"
        onClick={() => run("infer", "Rebuilt blocks", () => runInfer(day))}
      />
      <ActionButton
        pending={isPending("estimate")}
        icon={<Sparkles />}
        label="Estimate with Claude"
        pendingLabel="Estimating…"
        title="Use claude -p to fill tickets/descriptions for un-estimated blocks"
        onClick={() => run("estimate", "Estimated", () => runEstimate(day))}
      />
      <ActionButton
        pending={isPending("jira")}
        icon={<RefreshCw />}
        label="Refresh Jira"
        pendingLabel="Refreshing…"
        title={
          cacheLast
            ? `${cacheCount} tickets cached · last ${new Date(cacheLast).toLocaleString()}`
            : "Fetch open tickets from Jira"
        }
        onClick={() => run("jira", "Refreshed Jira", () => refreshJira(day))}
      />
      <ActionButton
        pending={isPending("dry-run")}
        icon={<Braces />}
        label="Dry-run sync"
        pendingLabel="Checking…"
        title="Show what would be posted to Tempo — no network writes"
        onClick={() =>
          run("dry-run", "Dry-run", () => runSync(day, true))
        }
      />
      <button
        type="button"
        className="action-btn"
        disabled={isPending("sync")}
        data-confirm={confirmSync ? "true" : undefined}
        onClick={onSyncClick}
        title={
          confirmSync
            ? "Click again to confirm — this posts worklogs to Tempo"
            : "Post un-synced blocks to Tempo (click twice to confirm)"
        }
      >
        {confirmSync ? (
          <>
            <Check />
            Confirm sync?
          </>
        ) : isPending("sync") ? (
          <>
            <Send />
            Syncing…
          </>
        ) : (
          <>
            <Send />
            Sync to Tempo
          </>
        )}
      </button>
      {confirmSync && (
        <button
          type="button"
          className="action-btn"
          onClick={() => setConfirmSync(false)}
          title="Cancel"
          aria-label="cancel sync"
        >
          <X />
          Cancel
        </button>
      )}
    </div>
  );
}

function ActionButton(props: {
  pending: boolean;
  icon: React.ReactNode;
  label: string;
  pendingLabel: string;
  title: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className="action-btn"
      disabled={props.pending}
      title={props.title}
      aria-busy={props.pending || undefined}
      onClick={props.onClick}
    >
      {props.icon}
      {props.pending ? props.pendingLabel : props.label}
    </button>
  );
}

function summarise(r: unknown): string {
  if (r && typeof r === "object") {
    const o = r as Record<string, unknown>;
    if ("estimated" in o)
      return `${o.estimated} estimated · ${o.skipped ?? 0} skipped · ${o.failed ?? 0} failed`;
    if ("synced" in o) {
      const synced = Number(o.synced) || 0;
      const skipped = Number(o.skipped ?? 0);
      const errs = Array.isArray(o.errors) ? o.errors.length : 0;
      if (synced === 0 && skipped === 0 && errs === 0) {
        // Empty result = everything's already in Tempo (or there were
        // no candidates at all). "0 synced · 0 skipped · 0 errors" reads
        // like silent failure even though it's the happy case.
        return o.dry_run ? "nothing to preview" : "already up to date — nothing to sync";
      }
      return `${synced} synced · ${skipped} skipped · ${errs} error${errs === 1 ? "" : "s"}${o.dry_run ? " (dry-run)" : ""}`;
    }
    if ("blocks" in o) return `${o.blocks} blocks · ${o.minutes ?? 0} min`;
    if ("tickets_written" in o) return `${o.tickets_written} tickets`;
  }
  return "ok";
}
