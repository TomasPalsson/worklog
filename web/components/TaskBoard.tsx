"use client";

import { useCallback, useEffect, useRef, useState, type DragEvent } from "react";

import {
  commentOnTicket,
  draftTicketUpdate,
  loadTicketBlocks,
  loadTicketDetail,
  logTicketTime,
  loadTransitions,
  transitionTicket,
} from "@/app/actions-hub";
import { runSync } from "@/app/actions";
import { saveTempoLineHours, saveTempoLineText } from "@/app/actions-tempo-lines";
import { COLUMNS, asTicket, columnOf, columnTitle, localToday, movesInto, weekMax, type Column } from "@/lib/taskBoard";
import type { TaskRow, Transition } from "@/lib/types";
import { TaskCard, type TaskActions } from "./TaskCard";
import type { Drafts } from "./TaskComposer";
import { TaskColumn, type Chooser } from "./TaskColumn";
import { TaskModal, type TaskModalProps } from "./TaskModal";
import { TaskToolbar } from "./TaskToolbar";
import { useTicketUrl } from "./useTicketUrl";

const realActions: TaskActions = {
  loadTransitions,
  loadTicketDetail,
  loadTicketBlocks,
  logTicketTime,
  saveTempoLineHours,
  saveTempoLineText,
  runSync,
  transitionTicket,
  commentOnTicket,
  draftTicketUpdate,
};

type StatusPatch = Parameters<TaskModalProps["onStatus"]>[0];
type Patch = (key: string, s: StatusPatch) => void;

// Browsers without `inert` fall back to hiding the board from assistive tech.
const HAS_INERT = typeof HTMLElement !== "undefined" && "inert" in HTMLElement.prototype;

const focusCard = (key: string) =>
  document.querySelector<HTMLElement>(`[data-task-key="${key}"] .task-card-btn`)?.focus();

type UndoMap = Record<string, { from: Column; to: Column }>;
type Timer = { handle?: ReturnType<typeof setTimeout>; left: number; since: number };
type MoveOpts = { back?: boolean; focus?: boolean; transitions?: Transition[] };
type MoveError ={ text: string; target: Column; back: boolean };

/** The "Moved to X · Undo" strip: one per card, gone after `ms` or on that card's next move. */
function useUndo(ms: number) {
  const [undoable, setUndoable] = useState<UndoMap>({});
  const timers = useRef<Record<string, Timer>>({});
  const clear = useCallback((key: string) => {
    clearTimeout(timers.current[key]?.handle);
    delete timers.current[key];
    setUndoable(({ [key]: _, ...rest }) => rest);
  }, []);
  const arm = (key: string, t: Timer) => {
    t.since = Date.now();
    t.handle = setTimeout(() => clear(key), t.left);
  };
  const offer = (key: string, from: Column, to: Column) => {
    if (from === to) return;
    clearTimeout(timers.current[key]?.handle);
    setUndoable((u) => ({ ...u, [key]: { from, to } }));
    arm(key, (timers.current[key] = { left: ms, since: 0 }));
  };
  /** Hover or focus inside the card freezes the countdown; leaving resumes with what was left. */
  const hold = (key: string, held: boolean) => {
    const t = timers.current[key];
    if (!t) return;
    if (held && t.handle) {
      clearTimeout(t.handle);
      t.handle = undefined;
      t.left -= Date.now() - t.since;
    } else if (!held && !t.handle) arm(key, t);
  };
  useEffect(() => () => Object.values(timers.current).forEach((t) => clearTimeout(t.handle)), []);
  return { undoable, clearUndo: clear, offerUndo: offer, holdUndo: hold };
}

/** Per-card move bookkeeping: optimistic placement, pending flags, error strips, focus return. */
function useMoveState(setAnnounce: (s: string) => void) {
  const [placed, setPlaced] = useState<Record<string, Column>>({});
  const [pending, setPending] = useState<Set<string>>(new Set());
  const [errors, setErrors] = useState<Record<string, MoveError>>({});
  const [refocus, setRefocus] = useState<{ key: string } | null>(null);
  const wantFocus = useRef(new Set<string>());

  const dismiss = (key: string) => setErrors(({ [key]: _, ...rest }) => rest);
  const place = (key: string, target: Column) => {
    setPlaced((p) => ({ ...p, [key]: target }));
    setPending((p) => new Set(p).add(key));
    dismiss(key);
  };
  const settle = (key: string, target: Column, reason?: string, back = false) => {
    setPlaced(({ [key]: _, ...rest }) => rest);
    setPending((p) => new Set([...p].filter((k) => k !== key)));
    if (wantFocus.current.delete(key)) setRefocus({ key });
    if (!reason) return;
    setErrors((e) => ({ ...e, [key]: { text: `Couldn't move to ${columnTitle(target)} — ${reason}`, target, back } }));
    setAnnounce(`Couldn't move ${key} to ${columnTitle(target)}.`);
  };
  // After a menu/undo/chooser move the card remounts elsewhere; put focus back on it once state settles.
  useEffect(() => {
    if (refocus) focusCard(refocus.key);
  }, [refocus]);
  return { placed, pending, errors, wantFocus, dismiss, place, settle };
}

/** Spoken outcome, landing highlight, and the patch + undo offer on a successful move. */
function useOutcome(patch: Patch, offerUndo: (key: string, from: Column, to: Column) => void) {
  const [announce, setAnnounce] = useState("");
  const [landed, setLanded] = useState<string | null>(null);
  const succeed = (key: string, s: StatusPatch, from: Column) => {
    patch(key, s);
    const to = columnOf(s.status_category);
    setAnnounce(`Moved ${key} to ${columnTitle(to)}.${from === to ? "" : " Undo available."}`);
    setLanded(key);
    offerUndo(key, from, to);
  };
  useEffect(() => {
    if (!landed) return;
    const t = setTimeout(() => setLanded(null), 1200);
    return () => clearTimeout(t);
  }, [landed]);
  return { announce, setAnnounce, landed, succeed };
}

/** Optimistic move: composes undo, per-card state, outcome and the transition chooser. */
function useMoves(patch: Patch, actions: TaskActions, undoMs: number) {
  const { undoable, clearUndo, offerUndo, holdUndo } = useUndo(undoMs);
  const { announce, setAnnounce, landed, succeed } = useOutcome(patch, offerUndo);
  const { placed, pending, errors, wantFocus, dismiss, place, settle } = useMoveState(setAnnounce);
  const [chooser, setChooser] = useState<Chooser | null>(null);

  const begin = (key: string, target: Column) => {
    setChooser(null);
    clearUndo(key);
    place(key, target);
  };

  async function run(key: string, target: Column, t: Transition, from: Column, back = false, focus = true) {
    if (focus) wantFocus.current.add(key);
    begin(key, target);
    const res = await actions.transitionTicket(key, t.id);
    if (res.ok) succeed(key, { status: res.data.status, status_category: res.data.status_category }, from);
    settle(key, target, res.ok ? undefined : res.error, back);
  }

  async function move(row: TaskRow, target: Column, from: Column, { back = false, focus = false, transitions }: MoveOpts = {}) {
    if (focus) wantFocus.current.add(row.key);
    begin(row.key, target);
    // The Move menu already asked Jira; reuse its answer rather than asking twice.
    const res = transitions ? { ok: true as const, data: transitions } : await actions.loadTransitions(row.key);
    if (!res.ok) return settle(row.key, target, res.error, back);
    const moves = movesInto(res.data, target);
    if (moves.length === 1) return run(row.key, target, moves[0], from, back, false); // focus already queued by `move`
    if (moves.length > 1) {
      wantFocus.current.delete(row.key); // the chooser takes focus instead
      settle(row.key, target);
      setAnnounce(`Choose how to move ${row.key} to ${columnTitle(target)}.`);
      return setChooser({ key: row.key, column: target, transitions: moves, from, back });
    }
    const status = row.status ?? "its status";
    settle(
      row.key,
      target,
      back
        ? `Jira has no way back to ${columnTitle(target)} from ${status}.`
        : `Jira has no move from ${status} to ${columnTitle(target)}.`,
      back,
    );
  }

  const cancel = () => {
    setChooser(null);
    if (chooser) focusCard(chooser.key);
  };
  return { placed, pending, errors, chooser, announce, landed, undoable, offerUndo, holdUndo, dismiss, cancel, move, run };
}

/** Native HTML5 DnD state. The key lives in React state; dataTransfer is a fallback. */
function useDrag(moveTo: (key: string, c: Column) => void) {
  const [dragKey, setDragKey] = useState<string | null>(null);
  const [over, setOver] = useState<Column | null>(null);

  const end = () => {
    setDragKey(null);
    setOver(null);
  };
  return {
    dragKey,
    over,
    start: (key: string, dt: DataTransfer | null) => {
      setDragKey(key);
      dt?.setData("text/plain", key);
    },
    end,
    column: (id: Column) => ({
      onDragOver: (e: DragEvent) => {
        if (!dragKey) return;
        e.preventDefault();
        setOver(id);
      },
      onDragLeave: (e: DragEvent) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setOver(null);
      },
      onDrop: (e: DragEvent) => {
        e.preventDefault();
        const key = dragKey ?? e.dataTransfer?.getData("text/plain");
        end();
        if (key) moveTo(key, id);
      },
    }),
  };
}

function matches(t: TaskRow, text: string, onlyWorked: boolean) {
  const q = text.trim().toLowerCase();
  if (onlyWorked && t.week_seconds <= 0) return false;
  return !q || t.key.toLowerCase().includes(q) || t.summary.toLowerCase().includes(q);
}

const EMPTY = (
  <p className="reg-lede">
    No tickets cached yet. Run <code>worklog collect jira</code> or open the day view — tickets appear here once Jira has
    been read.
  </p>
);

function useChooserEscape(active: boolean, cancel: () => void) {
  useEffect(() => {
    if (!active) return;
    // Capture phase + preventDefault: the dialog's Esc handler sees defaultPrevented and stays open.
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      cancel();
    };
    document.addEventListener("keydown", onKey, true);
    return () => document.removeEventListener("keydown", onKey, true);
  }, [active, cancel]);
}

/**
 * Which card's dialog is open (mirrored in `?ticket=`); drafts outlive the dialog so closing or switching never
 * loses a comment. Closing hands focus back to the card once the board is no longer inert.
 */
function usePanel(known: (key: string) => boolean) {
  const [openKey, setOpenKey] = useState<string | null>(null);
  const drafts = useRef<Drafts>({});
  const current = useRef<string | null>(null);
  const returnTo = useRef<string | null>(null);
  current.current = openKey;
  const url = useTicketUrl(known, (key) => {
    if (key === null) returnTo.current = current.current;
    setOpenKey(key);
  });
  const openCard = (key: string) => {
    url.opened(key, openKey !== null);
    setOpenKey(key);
  };
  const closePanel = () => {
    returnTo.current = openKey;
    url.closed();
    setOpenKey(null);
  };
  useEffect(() => {
    if (openKey === null && returnTo.current) focusCard(returnTo.current);
    returnTo.current = null;
  }, [openKey]);
  return { openKey, openCard, drafts, closePanel };
}

export function TaskBoard({ tasks, actions = realActions, undoMs = 8000 }: {
  tasks: TaskRow[];
  actions?: TaskActions;
  undoMs?: number;
}) {
  const [rows, setRows] = useState(tasks);
  const [text, setText] = useState("");
  const [onlyWorked, setOnlyWorked] = useState(false);
  const patch = useCallback<Patch>(
    (key, s) => setRows((rs) => rs.map((r) => (r.key === key ? { ...r, ...s } : r))),
    [],
  );
  const rowsRef = useRef(rows);
  rowsRef.current = rows;
  const { openKey, openCard, drafts, closePanel } = usePanel((key) => rowsRef.current.some((r) => r.key === key));
  const m = useMoves(patch, actions, undoMs);
  const colOf = (r: TaskRow) => m.placed[r.key] ?? columnOf(r.status_category);
  // One flow for a drop and for the Move menu; dropping on the card's own column is a no-op.
  const moveTo = (key: string, target: Column, opts?: MoveOpts) => {
    const row = rows.find((r) => r.key === key);
    if (row && colOf(row) !== target) m.move(row, target, colOf(row), opts);
  };
  const drag = useDrag(moveTo);
  useChooserEscape(m.chooser !== null, m.cancel);

  if (rows.length === 0) return EMPTY;
  const today = localToday();
  const maxSeconds = weekMax(rows);

  const q = text.trim();
  const visible = rows.filter((r) => matches(r, text, onlyWorked));
  const note = visible.length > 0 ? null : q ? `No tickets match “${q}”.` : "No tickets match the filter.";
  const open = rows.find((r) => r.key === openKey);
  const undoFor = (key: string) => {
    const u = m.undoable[key];
    return u && { to: columnTitle(u.to), run: () => moveTo(key, u.from, { back: true, focus: true }), hold: (h: boolean) => m.holdUndo(key, h) };
  };

  return (
    <>
      <p className="task-sr" role="status" aria-live="polite">
        {m.announce}
      </p>
      {/* The dialog is a sibling, not a child: everything behind it is inert while it is open. */}
      <div className="task-board-root" inert={open ? true : undefined} aria-hidden={open && !HAS_INERT ? true : undefined}>
      <TaskToolbar text={text} onlyWorked={onlyWorked} setText={setText} setOnlyWorked={setOnlyWorked} />
      {note && <p className="task-board-note">{note}</p>}
      <div className="task-board-scroll">
      <div className="task-board">
        {COLUMNS.map(({ id }) => {
          const cards = visible.filter((r) => colOf(r) === id);
          return (
            <TaskColumn
              key={id}
              id={id}
              count={cards.length}
              over={drag.over === id}
              hint={cards.length === 0 && !note}
              chooser={m.chooser?.column === id ? m.chooser : null}
              onPick={(t) => m.chooser && m.run(m.chooser.key, id, t, m.chooser.from, m.chooser.back)}
              onCancel={m.cancel}
              {...drag.column(id)}
            >
              {cards.map((r) => (
                <TaskCard
                  key={r.key}
                  task={r}
                  column={id}
                  today={today}
                  maxSeconds={maxSeconds}
                  selected={openKey === r.key}
                  pending={m.pending.has(r.key)}
                  dragging={drag.dragKey === r.key}
                  error={m.errors[r.key]?.text}
                  onRetry={() => {
                    const e = m.errors[r.key];
                    if (e) moveTo(r.key, e.target, { back: e.back, focus: true });
                  }}
                  landed={m.landed === r.key}
                  undo={undoFor(r.key)}
                  loadTransitions={() => actions.loadTransitions(r.key)}
                  onMove={(to, transitions) => moveTo(r.key, to, { focus: true, transitions })}
                  onDismissError={() => m.dismiss(r.key)}
                  onOpen={() => openCard(r.key)}
                  onDragStart={(dt) => drag.start(r.key, dt)}
                  onDragEnd={drag.end}
                />
              ))}
            </TaskColumn>
          );
        })}
      </div>
      </div>
      </div>
      {open && (
        <TaskModal key={open.key} drafts={drafts} task={open} actions={actions} onClose={closePanel}
          knownKeys={new Set(rows.map((r) => r.key))} tickets={rows.map(asTicket)} onOpenTicket={openCard} onStatus={(s) => {
          m.offerUndo(open.key, colOf(open), columnOf(s.status_category));
          patch(open.key, s);
        }}
        />
      )}
    </>
  );
}
