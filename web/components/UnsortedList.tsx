"use client";

// The day page's SortTray: unsorted browser/Slack events grouped by
// channel/DM/site (one row per group, not one row per message/visit — see
// lib/sortGroups.ts), plus the filed events tucked away in <AutoFiled>.

import { useState, useTransition } from "react";
import { Ban, ChevronRight, Globe, Slack } from "lucide-react";
import { labelEvent, dismissEvent } from "@/app/actions";
import { toast } from "@/lib/toast";
import { formatEventTime, previewDetails } from "@/lib/format-event";
import {
  countLabel,
  groupEvents,
  ruleKindFor,
  ruleKindOnFirst,
  type EventGroup,
} from "@/lib/sortGroups";
import type { LabelOrigin, RoutedEvent } from "@/lib/types";
import { PalettePicker } from "./PalettePicker";
import { AutoFiled } from "./AutoFiled";
import { ReviewDrawer } from "./ReviewDrawer";

interface Props {
  day: string;
  events: RoutedEvent[];
  folderOptions: string[];
}

/** Origins the daemon hides by default — never something to sort, only
 * something the review drawer can reverse. */
const HIDDEN_ORIGINS = new Set<LabelOrigin>(["noise", "dismissed"]);

function isHidden(e: RoutedEvent): boolean {
  return e.label_origin !== null && HIDDEN_ORIGINS.has(e.label_origin);
}

/** Split the day's routed events into the three buckets the tray/drawer
 * branch on. Pulled out of the component so its body stays short. */
function splitRoutedEvents(rows: RoutedEvent[]) {
  return {
    unsorted: rows.filter((e) => e.folder === null && !isHidden(e)),
    filed: rows.filter((e) => e.folder !== null),
    hidden: rows.filter((e) => e.folder === null && isHidden(e)),
  };
}

/** Not a hook (no hook calls inside) despite the shape — named this way
 * only to group the two setRows-closing setters used by GroupRow/DismissRest. */
function makeRowMutators(setRows: (fn: (prev: RoutedEvent[]) => RoutedEvent[]) => void) {
  function replaceEvents(updated: RoutedEvent[]) {
    const byId = new Map(updated.map((e) => [e.id, e]));
    setRows((prev) => prev.map((e) => byId.get(e.id) ?? e));
  }
  function removeEvents(ids: number[]) {
    const idSet = new Set(ids);
    setRows((prev) => prev.filter((e) => !idSet.has(e.id)));
  }
  return { replaceEvents, removeEvents };
}

export function UnsortedList({ day, events, folderOptions }: Props) {
  const [rows, setRows] = useState(events);
  // Snapshot once at mount — the tray shouldn't force itself back open (or
  // closed) just because filing a group shrank the unsorted count.
  const [initialOpen] = useState(() => events.some((e) => e.folder === null && !isHidden(e)));
  if (rows.length === 0) return null;

  const { unsorted, filed, hidden } = splitRoutedEvents(rows);

  // Zero-touch default: nothing left for the owner to sort. Replace the
  // tray + AutoFiled disclosure with the quiet summary line + Review drawer.
  if (unsorted.length === 0) {
    return <ReviewDrawer day={day} filed={filed} hidden={hidden} folderOptions={folderOptions} />;
  }

  const groups = groupEvents(unsorted);
  const { replaceEvents, removeEvents } = makeRowMutators(setRows);

  return (
    <>
      <details className="sort-tray" open={initialOpen}>
        <summary>
          <span className="sort-tray-title">To sort</span>
          <span className="sort-tray-count">
            {unsorted.length} event{unsorted.length === 1 ? "" : "s"} · {groups.length} group
            {groups.length === 1 ? "" : "s"}
          </span>
          <span className="sort-tray-hint">
            Pick a project once for the whole group. Tick “always” and future ones sort
            themselves.
          </span>
        </summary>
        {groups.length === 0 ? (
          <p className="sort-tray-empty">Nothing left to sort.</p>
        ) : (
          <>
            <ul className="sort-tray-body" role="list">
              {groups.map((g) => (
                <GroupRow
                  key={g.key}
                  group={g}
                  day={day}
                  folderOptions={folderOptions}
                  onFiled={replaceEvents}
                  onDismissed={removeEvents}
                />
              ))}
            </ul>
            <DismissRest groups={groups} day={day} onDismissed={removeEvents} />
          </>
        )}
      </details>
      <AutoFiled events={filed} />
    </>
  );
}

function SourceIcon({ source }: { source: string }) {
  if (source === "firefox") return <Globe width={13} height={13} strokeWidth={1.75} />;
  if (source === "slack") return <Slack width={13} height={13} strokeWidth={1.75} />;
  return null;
}

interface GroupRowProps {
  group: EventGroup;
  day: string;
  folderOptions: string[];
  onFiled: (updated: RoutedEvent[]) => void;
  onDismissed: (ids: number[]) => void;
}

/** All the async logic for one group row: filing (labelEvent per id) and
 * dismissing (dismissEvent per id), both sequential with the rule kind on
 * the first call only. Pulled out of `GroupRow` so the component itself
 * stays pure markup. */
function useGroupRowActions(
  group: EventGroup,
  day: string,
  always: boolean,
  onFiled: (updated: RoutedEvent[]) => void,
  onDismissed: (ids: number[]) => void,
) {
  const [pending, start] = useTransition();
  const [error, setError] = useState<string | null>(null);
  const [exiting, setExiting] = useState(false);
  const ruleKind = ruleKindFor(group.source);
  const ids = group.events.map((ev) => ev.id);

  function pickProject(folder: string) {
    setError(null);
    const kinds = ruleKindOnFirst(ids.length, always ? ruleKind : null);
    start(async () => {
      const updated: RoutedEvent[] = [];
      for (let i = 0; i < ids.length; i++) {
        const r = await labelEvent(ids[i], folder, kinds[i], day);
        if (!r.ok) {
          setError(r.error);
          toast.error(`Couldn't file ${group.label} — ${r.error}`);
          return;
        }
        updated.push(r.data);
      }
      setExiting(true);
      toast.ok(`Filed ${ids.length} to ${folder}`);
      window.setTimeout(() => onFiled(updated), 150);
    });
  }

  function dismissGroup() {
    setError(null);
    const kinds = ruleKindOnFirst(ids.length, always ? ruleKind : null);
    start(async () => {
      for (let i = 0; i < ids.length; i++) {
        const r = await dismissEvent(ids[i], kinds[i], day);
        if (!r.ok) {
          setError(r.error);
          toast.error(`Couldn't dismiss ${group.label} — ${r.error}`);
          return;
        }
      }
      setExiting(true);
      toast.ok(
        always ? `Ignoring ${group.label} from now on` : `Dismissed ${ids.length} from ${group.label}`,
      );
      window.setTimeout(() => onDismissed(ids), 150);
    });
  }

  return { pending, error, exiting, ruleKind, pickProject, dismissGroup };
}

function GroupRow({ group, day, folderOptions, onFiled, onDismissed }: GroupRowProps) {
  const [always, setAlways] = useState(false);
  const { pending, error, exiting, ruleKind, pickProject, dismissGroup } = useGroupRowActions(
    group,
    day,
    always,
    onFiled,
    onDismissed,
  );
  const scope = ruleKind === "slack_channel" ? "channel" : "site";
  const { preview } = previewDetails(group.latestPreview, 120);

  return (
    <li className={`sort-row ${exiting ? "sort-row-exit" : ""}`} data-source={group.source}>
      <details className="sort-row-disclosure">
        <summary>
          <ChevronRight className="disclosure-chev" />
          <SourceIcon source={group.source} />
          <span className="sort-row-name">{group.label}</span>
          <span className="sort-row-count">{countLabel(group)}</span>
          <span className="sort-row-range">
            {formatEventTime(group.earliestAt)}–{formatEventTime(group.latestAt)}
          </span>
          <span className="sort-row-preview">{preview}</span>
        </summary>
        <ul className="sort-row-events" role="list">
          {group.events.map((ev) => (
            <li key={ev.id} className="sort-row-event">
              <span className="sort-row-event-time">{formatEventTime(ev.started_at)}</span>
              <span className="sort-row-event-preview">
                {previewDetails(ev.details, 160).preview}
              </span>
            </li>
          ))}
        </ul>
      </details>

      <GroupRowControls
        group={group}
        scope={scope}
        always={always}
        onAlwaysChange={setAlways}
        pending={pending}
        error={error}
        folderOptions={folderOptions}
        onPick={pickProject}
        onDismiss={dismissGroup}
      />
    </li>
  );
}

interface GroupRowControlsProps {
  group: EventGroup;
  scope: string;
  always: boolean;
  onAlwaysChange: (v: boolean) => void;
  pending: boolean;
  error: string | null;
  folderOptions: string[];
  onPick: (folder: string) => void;
  onDismiss: () => void;
}

function GroupRowControls({
  group,
  scope,
  always,
  onAlwaysChange,
  pending,
  error,
  folderOptions,
  onPick,
  onDismiss,
}: GroupRowControlsProps) {
  return (
    <div className="sort-row-controls">
      <label className="sort-row-always">
        <input
          type="checkbox"
          checked={always}
          aria-label={`Always for this ${scope} — ${group.label}`}
          onChange={(e) => onAlwaysChange(e.target.checked)}
        />
        always for this {scope}
      </label>
      <button
        type="button"
        className="sort-row-dismiss"
        aria-label={`not work — ${group.label}`}
        aria-busy={pending || undefined}
        disabled={pending}
        onClick={onDismiss}
      >
        <Ban width={13} height={13} strokeWidth={1.75} />
        Not work
      </button>
      <PalettePicker
        value={null}
        options={folderOptions}
        allowFree
        placeholder="Project"
        searchPlaceholder="Search projects…"
        label={`Project for ${group.label}`}
        busy={pending}
        onPick={onPick}
      />
      {error && (
        <p className="picker-error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}

function DismissRest({
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
