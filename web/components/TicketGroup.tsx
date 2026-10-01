"use client";

import { ReactNode, useEffect, useRef, useState, useTransition } from "react";
import { Check, GitMerge, Pencil, RefreshCw, Sparkles } from "lucide-react";
import type { BlockGroup } from "@/app/[day]/page";
import { formatBilledHours, formatTotalHours } from "@/lib/format";
import { canMergeGroup } from "@/lib/group-actions";
import { mergeGroup } from "@/app/actions";
import {
  regenerateTempoLineText,
  saveTempoLineHours,
  saveTempoLineText,
} from "@/app/actions-tempo-lines";
import type { TempoLine } from "@/lib/tempo_line_contract";
import { toast } from "@/lib/toast";
import { OriginIcon, originLabel } from "./BillingGroup";
import { LineHours } from "./LineHours";

interface Props {
  group: BlockGroup;
  day: string;
  /** The day's Tempo line for this ticket; absent for the unassigned group. */
  line?: TempoLine;
  /** Test-only overrides for the Tempo line server actions, as BillingGroup does. */
  saveText?: typeof saveTempoLineText;
  saveHours?: typeof saveTempoLineHours;
  regenerate?: typeof regenerateTempoLineText;
  children: ReactNode;
}

/**
 * Collapsible row that mirrors how Tempo will see the day after sync:
 * one entry per (day, ticket). Click the summary to expand and
 * see/edit the individual blocks underneath.
 *
 * Uses native <details> for keyboard + a11y for free — same pattern
 * as the personal-section in app/[day]/page.tsx.
 *
 * Assigned multi-block groups also get a "Merge all" button that
 * folds the rest of the group into the earliest-start block.
 */
export function TicketGroup({
  group,
  day,
  line,
  saveText = saveTempoLineText,
  saveHours = saveTempoLineHours,
  regenerate = regenerateTempoLineText,
  children,
}: Props) {
  const blockNoun = group.blocks.length === 1 ? "block" : "blocks";
  const showMerge = canMergeGroup(group);
  // Separate transitions so a merge never shows "Writing…" on the text controls.
  const [pending, startTransition] = useTransition();
  const [merging, startMerge] = useTransition();
  const summaryRef = useRef<HTMLElement>(null);
  const editButtonRef = useRef<HTMLButtonElement>(null);
  const refocusEdit = useRef(false);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const lineKey = line && { day: line.day, jira_issue: line.jira_issue };
  const lineText = line ? (line.text ?? line.fallback_text) : "";
  const verb = line?.text_origin ? "regenerate" : "generate";
  // Regenerating over the Owner's own text takes a second click, like Sync.
  const [confirmReplace, setConfirmReplace] = useState(false);
  const confirmTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const disarmReplace = () => {
    if (confirmTimer.current) clearTimeout(confirmTimer.current);
    confirmTimer.current = null;
    setConfirmReplace(false);
  };
  useEffect(() => () => {
    if (confirmTimer.current) clearTimeout(confirmTimer.current);
  }, []);

  // Closing the text editor puts focus back on its "Edit text" button.
  useEffect(() => {
    if (!editing && refocusEdit.current) {
      refocusEdit.current = false;
      editButtonRef.current?.focus();
    }
  }, [editing]);
  const endEdit = () => {
    refocusEdit.current = true;
    setEditing(false);
  };

  const beginEdit = () => {
    disarmReplace();
    setDraft(lineText);
    setEditing(true);
  };

  const saveLineText = () => {
    if (!lineKey) return;
    const text = draft;
    startTransition(async () => {
      const r = await saveText(lineKey, text);
      if (!r.ok) {
        toast.error(`Couldn't save — ${r.error}`);
        return;
      }
      endEdit();
      toast.ok(text.trim() === "" ? "Reset to generated text" : "Saved");
    });
  };

  const regenerateLine = () => {
    if (!lineKey) return;
    if (line?.text_origin === "manual" && !confirmReplace) {
      setConfirmReplace(true);
      confirmTimer.current = setTimeout(disarmReplace, 4000);
      return;
    }
    disarmReplace();
    startTransition(async () => {
      const r = await regenerate(lineKey);
      if (!r.ok) {
        toast.error(`Couldn't ${verb} — ${r.error}`);
        return;
      }
      toast.ok(verb === "generate" ? "Generated" : "Regenerated");
    });
  };

  const runMerge = () => {
    if (merging) return;
    const sorted = [...group.blocks].sort((a, b) =>
      a.started_at < b.started_at ? -1 : a.started_at > b.started_at ? 1 : 0,
    );
    const primary = sorted[0]?.id;
    if (primary === undefined) return;
    const absorb = sorted.slice(1).map((b) => b.id);
    startMerge(async () => {
      const r = await mergeGroup(primary, absorb, day);
      if (!r.ok) {
        toast.error(`Merge failed — ${r.error}`);
      } else {
        toast.ok(`Merged ${absorb.length + 1} blocks on ${group.label}`);
        // After a successful merge the button itself unmounts (the
        // group now has 1 block, so canMergeGroup → false). Park focus
        // back on the group's summary row so keyboard users don't fall
        // to <body>.
        summaryRef.current?.focus();
      }
    });
  };

  // Two-pronged interception. Pointer/click: stopPropagation +
  // preventDefault keep the <details> from toggling. Keyboard: handle
  // Enter/Space on the button BEFORE the synthetic click reaches
  // <summary>, since some browsers route keyboard activation through
  // the summary's default action even when a descendant has focus.
  const onMergeClick: React.MouseEventHandler<HTMLButtonElement> = (e) => {
    e.stopPropagation();
    e.preventDefault();
    runMerge();
  };
  const onMergeKeyDown: React.KeyboardEventHandler<HTMLButtonElement> = (e) => {
    if (e.key === "Enter" || e.key === " ") {
      e.stopPropagation();
      e.preventDefault();
      runMerge();
    }
  };

  const mergeButton = showMerge && (
    <button
      type="button"
      className="merge-btn"
      disabled={merging}
      aria-busy={merging || undefined}
      onClick={onMergeClick}
      onKeyDown={onMergeKeyDown}
      title={`Merge all ${group.blocks.length} blocks on ${group.label} into one`}
      aria-label={`merge all blocks on ${group.label}`}
    >
      <GitMerge aria-hidden="true" />
      {/* Text is wrapped in a polite live region so screen readers
          announce the "Merging…" → "Merge all" flip — aria-busy
          alone doesn't trigger an announcement in most ATs. */}
      <span aria-live="polite">{merging ? "Merging…" : "Merge all"}</span>
    </button>
  );

  return (
    <details
      className={`ticket-group ${group.unassigned ? "unassigned" : "assigned"} sync-${group.syncState}`}
      open={group.defaultOpen}
    >
      <summary ref={summaryRef} tabIndex={0} className={line && lineKey ? "ticket-line-summary" : undefined}>
        {line && lineKey ? (
          // A Tempo line reads like the worklog it becomes: ticket and
          // state on top, the worklog text under it, the billed hours —
          // editable in place — on the right.
          <>
            <span className="ticket-line-main">
              <span className="ticket-line-head">
                <span className="ticket-group-label">{group.label}</span>
                <span className="ticket-group-meta">
                  {group.blocks.length} {blockNoun}
                </span>
                <SyncChip state={group.syncState} />
                {mergeButton}
              </span>
              {/* Clicks here must not toggle the <details>. */}
              <span className="billing-text-wrap ticket-line-text" onClick={(e) => e.stopPropagation()}>
                {editing ? (
              <span className="billing-text-edit">
                <textarea
                  aria-label={`Edit line text for ${group.label}`}
                  value={draft}
                  disabled={pending}
                  autoFocus
                  onChange={(e) => setDraft(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Escape") endEdit();
                    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) saveLineText();
                  }}
                />
                <span className="billing-text-edit-actions">
                  <button type="button" className="billing-text-primary" onClick={saveLineText} disabled={pending}>
                    <Check width={12} height={12} aria-hidden="true" />
                    Save
                  </button>
                  <button type="button" onClick={endEdit} disabled={pending}>
                    Cancel
                  </button>
                  <span className="billing-text-hint">⌘↵ to save · Esc to cancel · empty resets to generated</span>
                </span>
              </span>
            ) : (
              <>
                <span className="billing-text">{lineText}</span>
                <span className="billing-text-controls">
                  <span className={`billing-text-origin billing-text-origin-${line.text_origin ?? "none"}`}>
                    <OriginIcon origin={line.text_origin} />
                    {originLabel(line.text_origin)}
                  </span>
                  <button type="button" ref={editButtonRef} onClick={beginEdit} disabled={pending}>
                    <Pencil width={12} height={12} aria-hidden="true" />
                    Edit text
                  </button>
                  <button
                    type="button"
                    onClick={regenerateLine}
                    disabled={pending}
                    data-confirm={confirmReplace ? "true" : undefined}
                    aria-live="polite"
                    title={confirmReplace ? "Click again — this replaces the text you wrote" : undefined}
                  >
                    {verb === "generate" && !pending ? (
                      <Sparkles width={12} height={12} aria-hidden="true" />
                    ) : (
                      <RefreshCw
                        width={12}
                        height={12}
                        aria-hidden="true"
                        className={pending ? "billing-spin" : undefined}
                      />
                    )}
                    {pending
                      ? "Writing…"
                      : confirmReplace
                        ? "Replace your text?"
                        : verb === "generate"
                          ? "Generate"
                          : "Regenerate"}
                  </button>
                </span>
              </>
            )}
              </span>
            </span>
            <LineHours label={group.label} line={line} lineKey={lineKey} saveHours={saveHours} />
          </>
        ) : (
          <>
            <span className="ticket-group-label">{group.label}</span>
            <span className="ticket-group-meta">
              {group.blocks.length} {blockNoun}
            </span>
            {group.unassigned ? (
              <span className="ticket-group-meta">
                {formatTotalHours(group.totalSeconds)}
              </span>
            ) : (
              // Assigned groups sync as one Tempo worklog, rounded to the
              // nearest half hour — show what will actually be billed, with
              // the raw tracked time in the tooltip. "0h" flags a group
              // under 15 min that won't sync.
              <span
                className="ticket-group-meta"
                title={`${formatTotalHours(group.totalSeconds)} tracked`}
              >
                {formatBilledHours(group.totalSeconds)} billed
              </span>
            )}
            <SyncChip state={group.syncState} />
            <span className="ticket-group-description" title={group.previewDescription}>
              {group.previewDescription}
            </span>
          </>
        )}
        {!(line && lineKey) && mergeButton}
        <span className="ticket-group-hint" aria-hidden="true" />
      </summary>
      <div className="ticket-group-body">{children}</div>
    </details>
  );
}

function SyncChip({ state }: { state: BlockGroup["syncState"] }) {
  // Labels intentionally terse — they appear inline in a dense summary
  // row and the colour carries most of the meaning.
  const label =
    state === "synced"
      ? "synced"
      : state === "dirty"
        ? "edited"
        : state === "mixed"
          ? "partial"
          : "unsynced";
  return <span className={`sync-chip sync-chip-${state}`}>{label}</span>;
}
