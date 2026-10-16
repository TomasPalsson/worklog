"use client";

import { useEffect, useRef, useState, useTransition, type KeyboardEvent, type MouseEvent } from "react";
import type { Block } from "@/lib/types";
import { formatRange } from "@/lib/format";
import { deleteBlock, undoLastChange } from "@/app/actions";
import { toast } from "@/lib/toast";
import { menuKeys } from "./menuKeys";
import { useMenuDismiss } from "./useMenuDismiss";
import { useIgnoreActions } from "./useIgnoreActions";

interface Props {
  block: Block;
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

/** The right-click menu on a day-page block card: the card's own actions plus Delete. */
export function BlockContextMenu({ block, day, dayBlocks, x, y, close, onDescribe, onTogglePersonal, onInternal }: Props) {
  const menuRef = useRef<HTMLDivElement>(null);
  const [, start] = useTransition();
  const locked = !!block.tempo_worklog_id || !!block.exported_at;
  const ignoreReason = block.tempo_worklog_id
    ? "Already synced to Tempo — remove the worklog there first"
    : "Already exported for billing";
  const timeRangeLabel = formatRange(block.started_at, block.ended_at);
  const { onIgnore, onDuplicate } = useIgnoreActions(block, day, dayBlocks, close);

  useMenuDismiss(true, menuRef, close);
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
      {items.map(([label, run, disabledReason]) => (
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
      ))}
    </div>
  );
}
