"use client";

// Journey 4 (spec 006): every automatic change reaches the Owner as one
// pop-up per batch, live or as a catch-up on return. Mounted once in
// app/layout.tsx.
//
// `fetchChanges`/`fetchUnseenChanges`/`markChangesSeen` are injectable
// (default to the real server actions), like BillingPins.tsx's
// DeildMover — @/app/actions is `mock.module`d whole-file by several
// other test files, and that replacement is process-global for the Bun
// test run, not scoped to one file.

// Design contract — extend mode: this sits inside an established UI, so
// nothing was rolled; the fields are written by hand in the roll's order.
// SEED KEY: none (extend mode, hand-written contract)
//   WORLD          system: worklog review UI
//   ARCHETYPE      timeline rail: right-aligned mono time | 22px source
//                  glyph on a 1px rail | text column (the block-detail page)
//   STRATEGY       restrained: paper neutrals; amber only for unread (the
//                  slip's tint and rule), source hues on glyphs and marks
//   PALETTE SEED   inherited tokens: --bg oklch(0.985 0.003 85), --amber,
//                  --sage, --terracotta, --violet, --slate
//   DISPLAY FONT   inherited Geist Sans (600 titles, 500 slip)
//   BODY FONT      inherited Geist Sans; Geist Mono for times only
//   MOTION MOMENT  the panel grows out of the slip's corner (scale .96 from
//                  bottom right), 220ms ease-out-quart; nothing else animates
//   GRAMMAR
//     - a 3px amber left rule marks something needing attention (as the
//       "Unassigned" section does)
//     - times are right-aligned tabular mono, as on block rows
//     - "what happened when" is a rail with source glyphs, as block detail
//     - sources keep their colours: Claude violet, you sage, rebuild slate
//     - actions are underlined text, as the "Review" link on the clue line
//   REFUSE
//     - notification cards stacked in a popover
//     - red/green highlighter diffs
//     - uppercase micro-labels ("DESCRIPTION", "BILLED TO")
//     - pulsing dots and pill badges as attention
//   SCENE          the Owner comes back to the day view at their desk and
//                  wants to know, in seconds, what the machine changed
//   FIRST VIEWPORT a slip pinned bottom-right names how many blocks changed
//                  with an underlined "Review"; opening it raises the panel
//                  in the same corner
//   THESIS         changes are an edit trail read like tracked changes on
//                  paper: the text as it reads now, thin strike and underline
//                  where words moved, one plain sentence saying who did it
//                  and what it was. Refuses the notification-card stack.

import Link from "next/link";
import { useEffect, useMemo, useRef, useState } from "react";
import { ListRestart, PenLine, Scale, Sparkles, Tag, X, type LucideIcon } from "lucide-react";
import * as actions from "@/app/actions";
import { formatClock, shiftDay, shortMonthDay, shortWeekday, todayISO } from "@/lib/format";
import { CHANGE_SOURCE_LABELS, LIVE_POLL_SECONDS, type BlockChange, type ChangeSource } from "@/lib/deildir";
import {
  countBlocks,
  overlap,
  summariseChanges,
  wordDiff,
  type BlockChangeGroup,
  type NetChange,
} from "@/lib/changeSummary";
import { toast } from "@/lib/toast";

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

const SOURCE_ICON: Record<ChangeSource, LucideIcon> = {
  claude: Sparkles,
  verdict: Scale,
  keyword: Tag,
  rebuild: ListRestart,
  user: PenLine,
};

function dayLabel(day: string): string {
  const today = todayISO();
  if (day === today) return "Today";
  if (day === shiftDay(today, -1)) return "Yesterday";
  return `${shortWeekday(day)}, ${shortMonthDay(day)}`;
}

/** "Claude" or "Claude, after a block rebuild": the last actor is the
 * one whose edit stands; an earlier rebuild is context, not a co-author. */
function who(sources: ChangeSource[]): string {
  const last = sources[sources.length - 1];
  const afterRebuild = last !== "rebuild" && sources.includes("rebuild");
  return `${CHANGE_SOURCE_LABELS[last]}${afterRebuild ? ", after a block rebuild," : ""}`;
}

/** The text as it reads now, with tracked-change marks, plus one sentence
 * saying who did what. The old value always sits in the sentence, never
 * in the headline. */
function ChangeText({ change }: { change: NetChange }) {
  if (change.kind === "description") return <DescriptionText change={change} />;
  return (
    <>
      <p className="chg-text">
        Billed to <ins>{change.new ?? "nobody yet"}</ins>
      </p>
      <p className="chg-note">
        {who(change.sources)} changed the billing · was <del>{change.old ?? "unresolved"}</del>
      </p>
    </>
  );
}

function DescriptionText({ change }: { change: NetChange }) {
  const by = who(change.sources);
  const { old, new: next } = change;
  if (next === null) {
    const refills = change.sources[change.sources.length - 1] === "rebuild";
    return (
      <>
        <p className="chg-text chg-text-blank">
          {refills ? "No description yet — the next estimate writes one" : "No description"}
        </p>
        <p className="chg-note">
          {by} cleared it · was <del>{old}</del>
        </p>
      </>
    );
  }
  if (old === null) {
    return (
      <>
        <p className="chg-text">{next}</p>
        <p className="chg-note">{by} wrote the description</p>
      </>
    );
  }
  const parts = wordDiff(old, next);
  // A near-total rewrite reads as confetti inline; show the new text clean
  // and the old one in the sentence.
  if (overlap(parts) < 0.4) {
    return (
      <>
        <p className="chg-text">{next}</p>
        <p className="chg-note">
          {by} rewrote the description · was <del>{old}</del>
        </p>
      </>
    );
  }
  return (
    <>
      <p className="chg-text">
        {parts.map((p, i) =>
          p.kind === "same" ? p.text : p.kind === "add" ? <ins key={i}>{p.text}</ins> : <del key={i}>{p.text}</del>,
        )}
      </p>
      <p className="chg-note">{by} reworded the description</p>
    </>
  );
}

function BlockRow({ block, onNavigate }: { block: BlockChangeGroup; onNavigate: () => void }) {
  const last = block.changes.flatMap((c) => c.sources).at(-1) ?? "user";
  const Icon = SOURCE_ICON[last];
  const time = formatClock(block.started_at);
  return (
    <li className="chg-row" data-source={last}>
      <Link
        href={`/${block.day}`}
        className="chg-time"
        onClick={onNavigate}
        aria-label={`Open ${dayLabel(block.day)}, block at ${time}`}
      >
        {time}
      </Link>
      <span className="chg-glyph" aria-hidden="true">
        <Icon width={12} height={12} strokeWidth={2} />
      </span>
      <div className="chg-body">
        {block.changes.map((c) => (
          <ChangeText key={c.kind} change={c} />
        ))}
      </div>
    </li>
  );
}

export function ChangeNotices({
  fetchChanges = actions.fetchChanges,
  fetchUnseenChanges = actions.fetchUnseenChanges,
  markChangesSeen = actions.markChangesSeen,
}: {
  fetchChanges?: typeof actions.fetchChanges;
  fetchUnseenChanges?: typeof actions.fetchUnseenChanges;
  markChangesSeen?: typeof actions.markChangesSeen;
}) {
  const [catchUp, setCatchUp] = useState<BlockChange[] | null>(null);
  // Once opened, the catch-up is seen but stays re-openable for this visit.
  const [catchUpSeen, setCatchUpSeen] = useState(false);
  const [listFromCatchUp, setListFromCatchUp] = useState(false);
  const slipRef = useRef<HTMLButtonElement>(null);
  const [listChanges, setListChanges] = useState<BlockChange[] | null>(null);
  // The live poll's high-water mark. Starts at the unseen feed's cursor so
  // the first live poll never re-announces what the catch-up already
  // covers.
  const cursorRef = useRef(0);
  const closeRef = useRef<HTMLButtonElement>(null);
  const catchUpDays = useMemo(() => summariseChanges(catchUp ?? []), [catchUp]);
  const listDays = useMemo(() => summariseChanges(listChanges ?? []), [listChanges]);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const r = await fetchUnseenChanges();
      if (cancelled || !r.ok) return;
      cursorRef.current = r.data.cursor;
      if (r.data.changes.length === 0) return;
      // Changes that cancel out (a rebuild clearing a description Claude
      // then rewrites verbatim) are nothing to announce — just mark seen.
      if (countBlocks(summariseChanges(r.data.changes)) === 0) {
        void markChangesSeen(Math.max(...r.data.changes.map((c) => c.id)));
        return;
      }
      setCatchUp(r.data.changes);
    })();
    return () => {
      cancelled = true;
    };
  }, [fetchUnseenChanges, markChangesSeen]);

  useEffect(() => {
    const id = setInterval(() => {
      void (async () => {
        const startCursor = cursorRef.current;
        let r;
        try {
          r = await fetchChanges(startCursor);
        } catch {
          return;
        }
        if (!r.ok) return;
        cursorRef.current = r.data.cursor;
        // A live poll starting from 0 means nothing was unseen on mount —
        // `after=0` would otherwise dump up to 30 days of history as
        // "new" batches. Just record the cursor and wait for the next tick.
        if (startCursor === 0) return;
        // One toast per poll, not per batch: a rebuild and the Claude
        // rewrite right after it are one edit to the Owner.
        const batches = r.data.batches.filter((b) => b.source !== "user");
        const ids = new Set(batches.map((b) => b.batch));
        const pollChanges = r.data.changes.filter((c) => ids.has(c.batch));
        const blocks = countBlocks(summariseChanges(pollChanges));
        if (blocks === 0) return;
        const who = [...new Set(batches.map((b) => CHANGE_SOURCE_LABELS[b.source]))].join(" + ");
        toast.notice(`${who} changed ${plural(blocks, "block")}`, {
          label: "Show",
          // The toast's Show only opens the list — it must not mark the
          // catch-up's older unseen changes seen or clear its chip.
          onClick: () => {
            setListFromCatchUp(false);
            setListChanges(pollChanges);
          },
        });
      })();
    }, LIVE_POLL_SECONDS * 1000);
    return () => clearInterval(id);
  }, [fetchChanges]);

  // Esc closes the list; focus lands on its close button so the
  // keyboard is already inside the dialog, and returns to the slip after.
  const listOpen = listChanges !== null;
  const wasOpen = useRef(false);
  useEffect(() => {
    if (!listOpen) {
      if (wasOpen.current) slipRef.current?.focus();
      wasOpen.current = false;
      return;
    }
    wasOpen.current = true;
    closeRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setListChanges(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [listOpen]);

  // Opening the catch-up slip is what marks its changes seen — the live
  // toast's own "Show" (above) never touches seen state or this slip.
  function openCatchUp() {
    if (!catchUp) return;
    setListFromCatchUp(true);
    setListChanges(catchUp);
    if (catchUpSeen) return;
    setCatchUpSeen(true);
    void markChangesSeen(Math.max(...catchUp.map((c) => c.id)));
  }

  const catchUpBlocks = countBlocks(catchUpDays);
  const listBlocks = countBlocks(listDays);
  const close = () => setListChanges(null);

  return (
    <>
      {catchUpBlocks > 0 && !(listOpen && listFromCatchUp) && (
        <button
          ref={slipRef}
          type="button"
          className="chg-slip"
          data-seen={catchUpSeen || undefined}
          onClick={openCatchUp}
        >
          {catchUpSeen ? (
            <span className="chg-slip-msg">{plural(catchUpBlocks, "block")} changed</span>
          ) : (
            <span className="chg-slip-msg">{plural(catchUpBlocks, "block")} changed while you were away</span>
          )}
          <span className="chg-slip-action">{catchUpSeen ? "Reopen" : "Review"}</span>
        </button>
      )}
      {listOpen && (
        <div className="chg-panel" role="dialog" aria-modal="false" aria-labelledby="chg-title">
          <div className="chg-head">
            <h2 id="chg-title" className="chg-title">
              {listBlocks === 0 ? "No net changes" : `${plural(listBlocks, "block")} changed`}
            </h2>
            <button ref={closeRef} type="button" className="chg-close" aria-label="Close changes" onClick={close}>
              <X width={16} height={16} strokeWidth={1.75} />
            </button>
          </div>
          <div className="chg-scroll">
            {listDays.length === 0 ? (
              <p className="chg-empty">Every edit in this batch was undone again, so nothing on your blocks moved.</p>
            ) : (
              listDays.map((d) => (
                <section key={d.day} className="chg-day" aria-label={dayLabel(d.day)}>
                  <h3 className="chg-day-label">{dayLabel(d.day)}</h3>
                  <ol className="chg-rail">
                    {d.blocks.map((b) => (
                      <BlockRow key={b.started_at} block={b} onNavigate={close} />
                    ))}
                  </ol>
                </section>
              ))
            )}
          </div>
        </div>
      )}
    </>
  );
}
