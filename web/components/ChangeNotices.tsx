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

import Link from "next/link";
import { useEffect, useMemo, useRef, useState } from "react";
import * as actions from "@/app/actions";
import { formatClock, shiftDay, shortMonthDay, shortWeekday, todayISO } from "@/lib/format";
import { CHANGE_SOURCE_LABELS, LIVE_POLL_SECONDS, type BlockChange } from "@/lib/deildir";
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

function dayLabel(day: string): string {
  const today = todayISO();
  if (day === today) return "Today";
  if (day === shiftDay(today, -1)) return "Yesterday";
  return `${shortWeekday(day)}, ${shortMonthDay(day)}`;
}

function DescriptionDiff({ old, next, lastSource }: { old: string | null; next: string | null; lastSource: string }) {
  if (next === null) {
    return (
      <>
        <p className="change-text">
          <del>{old}</del>
        </p>
        <span className="change-pending">
          {lastSource === "rebuild" ? "Cleared — a new description comes with the next estimate" : "Cleared"}
        </span>
      </>
    );
  }
  if (old === null) {
    return (
      <p className="change-text">
        <ins>{next}</ins>
      </p>
    );
  }
  const parts = wordDiff(old, next);
  // A near-total rewrite reads as confetti inline; show it as before/after.
  if (overlap(parts) < 0.4) {
    return (
      <>
        <p className="change-text change-text-before">
          <del>{old}</del>
        </p>
        <p className="change-text">
          <ins>{next}</ins>
        </p>
      </>
    );
  }
  return (
    <p className="change-text">
      {parts.map((p, i) =>
        p.kind === "same" ? p.text : p.kind === "add" ? <ins key={i}>{p.text}</ins> : <del key={i}>{p.text}</del>,
      )}
    </p>
  );
}

function ChangeLine({ change }: { change: NetChange }) {
  return (
    <div className="change-line">
      <span className="change-kind">{change.kind === "description" ? "Description" : "Billed to"}</span>
      {change.kind === "description" ? (
        <DescriptionDiff old={change.old} next={change.new} lastSource={change.sources[change.sources.length - 1]} />
      ) : (
        <>
          <p className="change-text change-text-before">
            <del>{change.old ?? "Unresolved"}</del>
          </p>
          <p className="change-text">
            <span className="change-arrow" aria-label="changed to">
              →{" "}
            </span>
            <ins>{change.new ?? "Unresolved"}</ins>
          </p>
        </>
      )}
    </div>
  );
}

function BlockEntry({ block, onNavigate }: { block: BlockChangeGroup; onNavigate: () => void }) {
  const sources = [...new Set(block.changes.flatMap((c) => c.sources))];
  return (
    <li className="change-block">
      <div className="change-block-head">
        <Link href={`/${block.day}`} className="change-block-time" onClick={onNavigate}>
          {formatClock(block.started_at)}
        </Link>
        <span className="change-block-by">by {sources.map((s) => CHANGE_SOURCE_LABELS[s]).join(" + ")}</span>
      </div>
      {block.changes.map((c) => (
        <ChangeLine key={c.kind} change={c} />
      ))}
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
          onClick: () => setListChanges(pollChanges),
        });
      })();
    }, LIVE_POLL_SECONDS * 1000);
    return () => clearInterval(id);
  }, [fetchChanges]);

  // Esc closes the list; focus lands on its close button so the
  // keyboard is already inside the dialog.
  const listOpen = listChanges !== null;
  useEffect(() => {
    if (!listOpen) return;
    closeRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setListChanges(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [listOpen]);

  // Opening the catch-up chip is what marks its changes seen — the live
  // toast's own "Show" (above) never touches seen state or this chip.
  function openCatchUp() {
    if (!catchUp) return;
    setListChanges(catchUp);
    setCatchUp(null);
    const upTo = Math.max(...catchUp.map((c) => c.id));
    void markChangesSeen(upTo);
  }

  const catchUpBlocks = countBlocks(catchUpDays);
  const close = () => setListChanges(null);

  return (
    <>
      {catchUpBlocks > 0 && (
        <button type="button" className="change-chip" onClick={openCatchUp}>
          <span className="change-chip-dot" aria-hidden="true" />
          {plural(catchUpBlocks, "block")} changed while you were away
        </button>
      )}
      {listOpen && (
        <div className="change-list-anchor">
          <div className="change-list" role="dialog" aria-label="Recent changes">
            <div className="change-list-head">
              <div>
                <h2 className="change-list-title">Recent changes</h2>
                <p className="change-list-sub">
                  {plural(countBlocks(listDays), "block")} · <del>removed</del> <ins>added</ins>
                </p>
              </div>
              <button ref={closeRef} type="button" className="change-list-close" aria-label="Close" onClick={close}>
                ×
              </button>
            </div>
            {listDays.length === 0 ? (
              <p className="change-list-empty">Nothing changed in the end — every edit was undone.</p>
            ) : (
              listDays.map((d) => (
                <section key={d.day} className="change-day">
                  <h3 className="change-day-label">{dayLabel(d.day)}</h3>
                  <ul className="change-blocks">
                    {d.blocks.map((b) => (
                      <BlockEntry key={b.started_at} block={b} onNavigate={close} />
                    ))}
                  </ul>
                </section>
              ))
            )}
          </div>
        </div>
      )}
    </>
  );
}
