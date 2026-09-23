"use client";

// The zero-touch default view's only affordance: a one-line summary under
// the DayStrip plus a "Review" toggle that opens an in-page panel (no
// modal library) listing what the daemon already filed and what it hid as
// noise. Rendered instead of the SortTray once there's nothing left for the
// owner to sort — see UnsortedList's branch.

import { useState, useTransition } from "react";
import { Globe, Slack } from "lucide-react";
import { labelEvent } from "@/app/actions";
import { toast } from "@/lib/toast";
import { formatEventTime } from "@/lib/format-event";
import { AutoFiledRow } from "./AutoFiled";
import { PalettePicker } from "./PalettePicker";
import type { RoutedEvent } from "@/lib/types";

interface Props {
  day: string;
  /** Already filed (folder set) — origin rule/link/context/fix/guess. */
  filed: RoutedEvent[];
  /** Hidden — origin noise or dismissed, from the same include_hidden fetch. */
  hidden: RoutedEvent[];
  folderOptions: string[];
}

export function ReviewDrawer({ day, filed, hidden: initialHidden, folderOptions }: Props) {
  const [open, setOpen] = useState(false);
  const [hidden, setHidden] = useState(initialHidden);

  function fileHidden(id: number, folder: string) {
    setHidden((prev) => prev.filter((e) => e.id !== id));
  }

  return (
    <div className="review-line">
      <span className="review-summary">
        {filed.length} Slack and web clue{filed.length === 1 ? "" : "s"} folded into your blocks
        {hidden.length > 0 && (
          <>
            {" "}
            · {hidden.length} hidden as noise
          </>
        )}
      </span>
      <button
        type="button"
        className="review-toggle"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        Review
      </button>

      {open && (
        <div className="review-drawer" role="region" aria-label="Review filed and hidden events">
          <section className="review-section">
            <h3 className="review-section-title">Filed · {filed.length}</h3>
            {filed.length === 0 ? (
              <p className="review-section-empty">Nothing filed yet.</p>
            ) : (
              <ul className="auto-filed-list" role="list">
                {filed.map((e) => (
                  <AutoFiledRow key={e.id} event={e} />
                ))}
              </ul>
            )}
          </section>
          <section className="review-section">
            <h3 className="review-section-title">Hidden · {hidden.length}</h3>
            {hidden.length === 0 ? (
              <p className="review-section-empty">Nothing hidden.</p>
            ) : (
              <ul className="review-hidden-list" role="list">
                {hidden.map((e) => (
                  <HiddenRow key={e.id} event={e} day={day} folderOptions={folderOptions} onFiled={fileHidden} />
                ))}
              </ul>
            )}
          </section>
        </div>
      )}
    </div>
  );
}

interface HiddenRowProps {
  event: RoutedEvent;
  day: string;
  folderOptions: string[];
  onFiled: (id: number, folder: string) => void;
}

/** A hidden (noise/dismissed) row: just enough to reverse the hide — no
 * "always" checkbox, no dismiss button. Picking a project files it. */
function HiddenRow({ event, day, folderOptions, onFiled }: HiddenRowProps) {
  const [pending, start] = useTransition();

  function pick(folder: string) {
    start(async () => {
      const r = await labelEvent(event.id, folder, null, day);
      if (!r.ok) {
        toast.error(`Couldn't file "${event.title}" — ${r.error}`);
        return;
      }
      toast.ok(`Filed to ${folder}`);
      onFiled(event.id, folder);
    });
  }

  return (
    <li className="review-hidden-row" data-source={event.source}>
      <span className="auto-filed-time">{formatEventTime(event.started_at)}</span>
      {event.source === "firefox" && <Globe width={13} height={13} strokeWidth={1.75} />}
      {event.source === "slack" && <Slack width={13} height={13} strokeWidth={1.75} />}
      <span className="auto-filed-title" title={event.title}>
        {event.title}
      </span>
      <PalettePicker
        value={null}
        options={folderOptions}
        allowFree
        placeholder="Project"
        searchPlaceholder="Search projects…"
        label={`Project for ${event.title}`}
        busy={pending}
        onPick={pick}
      />
    </li>
  );
}
