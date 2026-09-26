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

import { useEffect, useRef, useState } from "react";
import * as actions from "@/app/actions";
import { formatClock } from "@/lib/format";
import { CHANGE_SOURCE_LABELS, LIVE_POLL_SECONDS, type BlockChange } from "@/lib/deildir";
import { toast } from "@/lib/toast";

function ChangeRow({ change }: { change: BlockChange }) {
  return (
    <li className="change-list-row">
      <span className="change-list-field">
        {change.field}: {change.old ?? "—"} → {change.new ?? "—"}
      </span>
      <span className="change-list-meta">
        {CHANGE_SOURCE_LABELS[change.source]} · {change.day} {formatClock(change.started_at)}
      </span>
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

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const r = await fetchUnseenChanges();
      if (cancelled || !r.ok) return;
      cursorRef.current = r.data.cursor;
      if (r.data.changes.length > 0) setCatchUp(r.data.changes);
    })();
    return () => {
      cancelled = true;
    };
  }, [fetchUnseenChanges]);

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
        for (const batch of r.data.batches) {
          if (batch.source === "user") continue;
          const batchChanges = r.data.changes.filter((c) => c.batch === batch.batch);
          toast.notice(`${CHANGE_SOURCE_LABELS[batch.source]} changed ${batch.count} blocks`, {
            label: "Show",
            onClick: () => openList(batchChanges),
          });
        }
      })();
    }, LIVE_POLL_SECONDS * 1000);
    return () => clearInterval(id);
  }, [fetchChanges]);

  function openList(changes: BlockChange[]) {
    setListChanges(changes);
    setCatchUp(null);
    const upTo = Math.max(...changes.map((c) => c.id));
    void markChangesSeen(upTo);
  }

  return (
    <>
      {catchUp && catchUp.length > 0 && (
        <button type="button" className="change-chip" onClick={() => openList(catchUp)}>
          {catchUp.length} changes since your last visit
        </button>
      )}
      {listChanges && (
        <div className="change-list-anchor">
          <div className="change-list" role="dialog" aria-label="Recent changes">
            <div className="change-list-head">
              <span>Recent changes</span>
              <button
                type="button"
                className="change-list-close"
                aria-label="Close"
                onClick={() => setListChanges(null)}
              >
                ×
              </button>
            </div>
            <ul className="change-list-rows">
              {[...listChanges]
                .sort((a, b) => a.id - b.id)
                .map((c) => (
                  <ChangeRow key={c.id} change={c} />
                ))}
            </ul>
          </div>
        </div>
      )}
    </>
  );
}
