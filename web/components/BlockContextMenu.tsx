"use client";

import { useEffect, useRef, useState, useTransition, type FormEvent, type KeyboardEvent, type MouseEvent } from "react";
import type { Block } from "@/lib/types";
import { formatRange } from "@/lib/format";
import { deleteBlock, undoLastChange } from "@/app/actions";
import { redescribeBlock } from "@/app/actions-block-menu";
import type { NoteFields } from "@/lib/noteBlock";
import { MAX_INSTRUCTION_CHARS } from "@/lib/redescribe";
import { toast } from "@/lib/toast";
import { menuKeys } from "./menuKeys";
import { useMenuDismiss } from "./useMenuDismiss";
import { useIgnoreActions } from "./useIgnoreActions";

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
      if (!enabled || (e.target as Element).closest(".block-context-menu")) return;
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

/** Replaces the item list: the one-time instruction for "Regenerate with prompt…". */
function InstructionBox({ block, day, close, onWriting }: {
  block: Block;
  day: string;
  close: () => void;
  onWriting: (writing: boolean) => void;
}) {
  const [instruction, setInstruction] = useState("");
  const [writing, setWriting] = useState(false);
  const trimmed = instruction.trim();

  // The menu's own dismiss is off while writing, so Esc is handled here.
  useEffect(() => {
    if (!writing) return;
    const onKey = (e: globalThis.KeyboardEvent) => e.key === "Escape" && close();
    document.addEventListener("keydown", onKey, true);
    return () => document.removeEventListener("keydown", onKey, true);
  }, [writing, close]);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setWriting(true);
    onWriting(true);
    const r = await redescribeBlock(block.id, day, trimmed);
    setWriting(false);
    onWriting(false);
    if (!r.ok) return toast.error(`Rewrite failed — ${r.error}`);
    toast.ok("Description rewritten");
    close();
  };

  return (
    <form className="block-context-menu-instruction" onSubmit={submit}>
      <input
        autoFocus
        type="text"
        aria-label="Instruction"
        maxLength={MAX_INSTRUCTION_CHARS}
        value={instruction}
        readOnly={writing}
        onChange={(e) => setInstruction(e.target.value)}
      />
      <button type="submit" disabled={writing || !trimmed || [...trimmed].length > MAX_INSTRUCTION_CHARS}>
        {writing ? "Writing…" : "Regenerate"}
      </button>
    </form>
  );
}

/** The right-click menu on a day-page block card: the card's own actions plus Delete. */
export function BlockContextMenu({ block, day, dayBlocks, x, y, close, onDescribe, onTogglePersonal, onInternal }: Props) {
  const menuRef = useRef<HTMLDivElement>(null);
  const [, start] = useTransition();
  const [boxOpen, setBoxOpen] = useState(false);
  const [writing, setWriting] = useState(false);
  const locked = !!block.tempo_worklog_id || !!block.exported_at;
  const ignoreReason = block.tempo_worklog_id
    ? "Already synced to Tempo — remove the worklog there first"
    : "Already exported for billing";
  const timeRangeLabel = formatRange(block.started_at, block.ended_at);
  const { onIgnore, onDuplicate } = useIgnoreActions(block, day, dayBlocks, close);

  // While the AI writes, an outside click must not lose the request; the box handles Esc itself.
  useMenuDismiss(!writing, menuRef, close);
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

  const items: [label: string, run: () => void, disabledReason?: string][] = [
    ["Describe with Claude", () => (close(), onDescribe())],
    [block.is_personal ? "Mark as work" : "Mark personal", () => (close(), onTogglePersonal())],
    ["Regenerate with prompt…", () => setBoxOpen(true), redescribeBlockedReason(block)],
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
      style={{ left: x, top: y }}
      onKeyDown={menuKeys}
    >
      {boxOpen ? (
        <InstructionBox block={block} day={day} close={close} onWriting={setWriting} />
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
