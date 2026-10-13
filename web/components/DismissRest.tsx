"use client";

// The SortTray footer: dismiss every remaining group at once, behind an
// in-place confirm.

import { useState, useTransition } from "react";
import { dismissEvent } from "@/app/actions";
import { toast } from "@/lib/toast";
import type { EventGroup } from "@/lib/sortGroups";

export function DismissRest({
  groups,
  day,
  onDismissed,
}: {
  groups: EventGroup[];
  day: string;
  onDismissed: (ids: number[]) => void;
}) {
  const [confirming, setConfirming] = useState(false);
  const [pending, start] = useTransition();
  if (groups.length === 0) return null;
  const allIds = groups.flatMap((g) => g.events.map((ev) => ev.id));

  function run() {
    start(async () => {
      for (const id of allIds) {
        const r = await dismissEvent(id, null, day);
        if (!r.ok) {
          toast.error(`Dismiss failed — ${r.error}`);
          return;
        }
      }
      toast.ok(`Dismissed ${allIds.length} events`);
      onDismissed(allIds);
    });
  }

  return (
    <div className="sort-tray-footer">
      {confirming ? (
        <>
          <span className="sort-tray-confirm-text">
            Dismiss {groups.length} group{groups.length === 1 ? "" : "s"}?
          </span>
          <button
            type="button"
            className="sort-tray-confirm-yes"
            disabled={pending}
            aria-busy={pending || undefined}
            onClick={run}
          >
            Yes
          </button>
          <button
            type="button"
            className="sort-tray-confirm-no"
            disabled={pending}
            onClick={() => setConfirming(false)}
          >
            Cancel
          </button>
        </>
      ) : (
        <button type="button" className="sort-tray-dismiss-rest" onClick={() => setConfirming(true)}>
          Dismiss the rest
        </button>
      )}
    </div>
  );
}
