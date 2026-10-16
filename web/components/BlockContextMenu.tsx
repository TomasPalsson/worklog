"use client";

import { useEffect, useRef, useState, useTransition, type FormEvent, type KeyboardEvent, type MouseEvent } from "react";
import type { Block } from "@/lib/types";
import { formatRange } from "@/lib/format";
import { deleteBlock, mergeGroup, undoLastChange } from "@/app/actions";
import { redescribeBlock, splitBlock } from "@/app/actions-block-menu";
import type { NoteFields } from "@/lib/noteBlock";
import { toast } from "@/lib/toast";
import { InstructionBox } from "./InstructionBox";
import { menuKeys } from "./menuKeys";
import { useMenuClamp } from "./useMenuClamp";
import { useMenuDismiss } from "./useMenuDismiss";
import { overlaps, useIgnoreActions } from "./useIgnoreActions";

interface Props {
  block: Block & NoteFields;
  day: string;
  /** The day's blocks; Duplicate merges into the nearest same-ticket one. */
  dayBlocks: Block[];
  /** Pointer position the menu opens at. */
  x: number;
  y: number;
  close: () => void;
  onDescribe: () => void;
  onTogglePersonal: () => void;
  /** Internal: open the ticket picker instead of ignoring. */
  onInternal: () => void;
}

const DELETE_BLOCKED = "Synced or exported — can't delete";
const MERGE_LOCKED = "Synced or exported — can't merge";
const MERGE_OVERLAP = "Overlaps in time — merging would double-count";
// Mirror the daemon's REASON_* strings so the tooltip says what a refusal would.
const REDESCRIBE_PERSONAL = "personal blocks are not described";
const REDESCRIBE_NOTE_BLOCK = "note blocks are rewritten from their note";
const REDESCRIBE_NOT_DESCRIBED = "block has no description yet — describe it first";

/** Open/close state and the card's right-click and ContextMenu / Shift+F10 handlers; `enabled` is false outside the day page. */
export function useBlockContextMenu(enabled: boolean) {
  const cardRef = useRef<HTMLElement>(null);
  const [menuAt, setMenuAt] = useState<{ x: number; y: number } | null>(null);
  const closeMenu = () => setMenuAt(null);
  // Esc and item picks unmount the focused item; hand focus back to the card, but leave an outside click's own focus alone.
  const wasOpen = useRef(false);
  useEffect(() => {
    if (wasOpen.current && !menuAt && document.activeElement === document.body) cardRef.current?.focus();
    wasOpen.current = !!menuAt;
  }, [menuAt]);

  const cardProps = {
    tabIndex: enabled ? 0 : undefined,
    onContextMenu: (e: MouseEvent) => {
      // Editable fields keep the native menu (paste, spell-check).
      if (!enabled || (e.target as Element).closest(".block-context-menu, input, textarea, [contenteditable]")) return;
      e.preventDefault();
      setMenuAt({ x: e.clientX, y: e.clientY });
    },
    onKeyDown: (e: KeyboardEvent<HTMLElement>) => {
      if (!enabled || e.target !== e.currentTarget) return;
      if (e.key !== "ContextMenu" && !(e.key === "F10" && e.shiftKey)) return;
      e.preventDefault();
      const box = e.currentTarget.getBoundingClientRect();
      setMenuAt({ x: box.left, y: box.top });
    },
  };
  return { cardRef, cardProps, menuAt, closeMenu };
}

function redescribeBlockedReason(block: Block & NoteFields) {
  if (block.is_personal) return REDESCRIBE_PERSONAL;
  if (block.rough_note) return REDESCRIBE_NOTE_BLOCK;
  if (!block.description?.trim()) return REDESCRIBE_NOT_DESCRIBED;
  return undefined;
}

/** The live blocks directly before and after `block` on its day, by start time. */
function neighbours(block: Block, dayBlocks: Block[]) {
  // Ties on start time break by id so two same-start blocks see each other on opposite sides.
  const order = (a: Block, b: Block) => Date.parse(a.started_at) - Date.parse(b.started_at) || a.id - b.id;
  const others = dayBlocks.filter((o) => o.id !== block.id && o.day === block.day && !o.ignored_at).sort(order);
  return {
    prev: others.filter((o) => order(o, block) < 0).at(-1),
    next: others.find((o) => order(o, block) > 0),
  };
}

function mergeBlockedReason(block: Block, other: Block | undefined, none: string) {
  if (block.tempo_worklog_id || block.exported_at) return MERGE_LOCKED;
  if (!other) return none;
  if (other.tempo_worklog_id || other.exported_at) return MERGE_LOCKED;
  return overlaps(block, other) ? MERGE_OVERLAP : undefined;
}

/** Replaces the item list: whole minutes the first half keeps for "Split…". */
function SplitBox({ block, day, close }: { block: Block; day: string; close: () => void }) {
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const minutes = Number(value);
  const max = Math.ceil(block.duration_seconds / 60) - 1;
  const valid = value.trim() !== "" && Number.isInteger(minutes) && minutes >= 1 && minutes <= max;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(undefined);
    let r: Awaited<ReturnType<typeof splitBlock>>;
    try {
      r = await splitBlock(block.id, day, minutes);
    } catch (err) {
      r = { ok: false, error: err instanceof Error ? err.message : String(err) };
    } finally {
      setBusy(false);
    }
    if (!r.ok) return setError(r.error);
    toast.ok(`Split ${formatRange(block.started_at, block.ended_at)} at ${minutes}m`);
    close();
  };

  return (
    <form className="block-context-menu-instruction" onSubmit={submit}>
      <input
        autoFocus
        type="number"
        aria-label="Minutes"
        min={1}
        max={max}
        step={1}
        value={value}
        readOnly={busy}
        onChange={(e) => setValue(e.target.value)}
      />
      <button type="submit" disabled={busy || !valid}>
        Confirm
      </button>
      {error && <div role="alert">{error}</div>}
    </form>
  );
}

/** The right-click menu on a day-page block card: the card actions, Delete, Regenerate, Split and Merge. */
export function BlockContextMenu({ block, day, dayBlocks, x, y, close, onDescribe, onTogglePersonal, onInternal }: Props) {
  const menuRef = useRef<HTMLDivElement>(null);
  const [, start] = useTransition();
  const [boxOpen, setBoxOpen] = useState(false);
  const [splitOpen, setSplitOpen] = useState(false);
  const [writing, setWriting] = useState(false);
  const locked = !!block.tempo_worklog_id || !!block.exported_at;
  const ignoreReason = block.tempo_worklog_id
    ? "Already synced to Tempo — remove the worklog there first"
    : "Already exported for billing";
  const timeRangeLabel = formatRange(block.started_at, block.ended_at);
  const { onIgnore, onDuplicate } = useIgnoreActions(block, day, dayBlocks, close);

  // While the AI writes, an outside click must not lose the request; the box handles Esc itself.
  useMenuDismiss(!writing, menuRef, close);
  const pos = useMenuClamp(menuRef, x, y, `${boxOpen}-${splitOpen}`);
  useEffect(() => {
    menuRef.current?.querySelector<HTMLElement>('[role="menuitem"]:not(:disabled)')?.focus();
  }, []);

  const onDelete = () => {
    close();
    start(async () => {
      const r = await deleteBlock(block.id, day);
      if (!r.ok) toast.error(`Delete failed — ${r.error}`);
      else toast.undoable(`Deleted ${timeRangeLabel}`, () => undoLastChange(day));
    });
  };

  const onMerge = (other: Block) => {
    close();
    start(async () => {
      const r = await mergeGroup(block.id, [other.id], day);
      if (!r.ok) toast.error(`Merge failed — ${r.error}`);
      else toast.ok(`Merged ${formatRange(other.started_at, other.ended_at)} into ${timeRangeLabel}`);
    });
  };
  const { prev, next } = neighbours(block, dayBlocks);

  const items: [label: string, run: () => void, disabledReason?: string][] = [
    ["Describe with Claude", () => (close(), onDescribe())],
    [block.is_personal ? "Mark as work" : "Mark personal", () => (close(), onTogglePersonal())],
    ["Regenerate with prompt…", () => setBoxOpen(true), redescribeBlockedReason(block)],
    ["Split…", () => setSplitOpen(true)],
    ["Merge with previous", () => prev && onMerge(prev), mergeBlockedReason(block, prev, "No previous block")],
    ["Merge with next", () => next && onMerge(next), mergeBlockedReason(block, next, "No next block")],
    ["Noise", () => onIgnore("noise"), locked ? ignoreReason : undefined],
    ["Duplicate", onDuplicate, locked ? ignoreReason : undefined],
    ["Internal", () => (close(), onInternal()), locked ? ignoreReason : undefined],
    ["Delete", onDelete, locked ? DELETE_BLOCKED : undefined],
  ];

  return (
    <div
      ref={menuRef}
      role="menu"
      className="block-context-menu"
      style={pos}
      onKeyDown={menuKeys}
    >
      {splitOpen ? (
        <SplitBox block={block} day={day} close={close} />
      ) : boxOpen ? (
        <InstructionBox
          onSubmit={(instruction) => redescribeBlock(block.id, day, instruction)}
          close={close}
          onWriting={setWriting}
          successMessage="Description rewritten"
          failurePrefix="Rewrite failed"
        />
      ) : (
        items.map(([label, run, disabledReason]) => (
          <button
            key={label}
            type="button"
            role="menuitem"
            disabled={!!disabledReason}
            title={disabledReason}
            onClick={run}
          >
            {label}
          </button>
        ))
      )}
    </div>
  );
}
