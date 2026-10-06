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
import { refreshMirresAction } from "@/app/actions-tempo-lines";
import { toast } from "@/lib/toast";
import { MirresIcon } from "./icons";

interface Props {
  day: string;
  cacheCount: number;
  cacheLast: string | null;
}

type ActionId = "infer" | "estimate" | "jira" | "mirres" | "dry-run" | "sync";

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
        pending={isPending("mirres")}
        icon={<MirresIcon />}
        label="Refresh Mirres"
        pendingLabel="Refreshing…"
        title="Fetch each line's billable status from Mirres"
        onClick={() =>
          run("mirres", "Mirres", async () => {
            const r = await refreshMirresAction(day);
            // Nothing matched is a problem to fix, not a success.
            if (r.ok && r.data.length > 0 && !r.data.some((l) => l.billing))
              return { ok: false as const, error: "no lines matched. Check that the tickets have an Account in Jira." };
            return r;
          })
        }
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

/** One `not_generated` entry from `runEstimate`'s `line_texts` field. */
interface NotGeneratedLine {
  folder: string;
  customer: string;
  reason: string;
}

/** Max lines named before the rest collapse into a trailing "…" (FR-35). */
const MAX_NAMED_LINE_TEXTS = 3;

function lineTextsSuffix(lineTexts: unknown): string {
  if (!lineTexts || typeof lineTexts !== "object") return "";
  const notGenerated = (lineTexts as { not_generated?: NotGeneratedLine[] }).not_generated ?? [];
  if (notGenerated.length === 0) return "";
  const names = notGenerated
    .slice(0, MAX_NAMED_LINE_TEXTS)
    .map((n) => `${n.folder} (${n.reason})`)
    .join(", ");
  const more = notGenerated.length > MAX_NAMED_LINE_TEXTS ? "…" : "";
  return ` · ${notGenerated.length} line texts not regenerated: ${names}${more}`;
}

export function summarise(r: unknown): string {
  // Refresh Mirres returns the day's ticket lines.
  if (Array.isArray(r)) {
    const missing = r.filter((l) => !l?.billing).map((l) => l?.jira_issue);
    const found = r.length - missing.length;
    return `${found} of ${r.length} lines matched` + (missing.length ? ` · no match: ${missing.join(", ")}` : "");
  }
  if (r && typeof r === "object") {
    const o = r as Record<string, unknown>;
    if ("estimated" in o)
      return (
        `${o.estimated} estimated · ${o.skipped ?? 0} skipped · ${o.failed ?? 0} failed` +
        lineTextsSuffix(o.line_texts)
      );
    if ("synced" in o) {
      // A dry run never counts as synced: it reports one `dry-run` / `dry-run-update` row per line it would send.
      const lines = Array.isArray(o.results)
        ? o.results.filter((r) => r?.status === "dry-run" || r?.status === "dry-run-update").length
        : 0;
      const synced = o.dry_run ? lines : Number(o.synced) || 0;
      const skipped = Number(o.skipped ?? 0);
      const errs = Array.isArray(o.errors) ? o.errors.length : 0;
      if (synced === 0 && skipped === 0 && errs === 0) {
        // Empty result = everything's already in Tempo (or there were
        // no candidates at all). "0 synced · 0 skipped · 0 errors" reads
        // like silent failure even though it's the happy case.
        return o.dry_run ? "nothing to preview" : "already up to date — nothing to sync";
      }
      return `${synced} ${o.dry_run ? "to sync" : "synced"} · ${skipped} skipped · ${errs} error${errs === 1 ? "" : "s"}${o.dry_run ? " (dry-run)" : ""}`;
    }
    if ("blocks" in o) return `${o.blocks} blocks · ${o.minutes ?? 0} min`;
    if ("tickets_written" in o) return `${o.tickets_written} tickets`;
  }
  return "ok";
}
