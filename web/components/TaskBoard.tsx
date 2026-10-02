"use client";

import { useCallback, useEffect, useRef, useState, type DragEvent } from "react";

import {
  commentOnTicket,
  draftTicketUpdate,
  loadTicketDetail,
  loadTransitions,
  transitionTicket,
} from "@/app/actions-hub";
import { COLUMNS, columnOf, columnTitle, movesInto, type Column } from "@/lib/taskBoard";
import type { TaskRow, Transition } from "@/lib/types";
import { TaskCard, type TaskActions } from "./TaskCard";
import type { Drafts } from "./TaskComposer";
import { TaskColumn, type Chooser } from "./TaskColumn";
import { TaskPanel, type TaskPanelProps } from "./TaskPanel";
import { TaskToolbar } from "./TaskToolbar";

const realActions: TaskActions = {
  loadTransitions,
  loadTicketDetail,
  transitionTicket,
  commentOnTicket,
  draftTicketUpdate,
};

type StatusPatch = Parameters<TaskPanelProps["onStatus"]>[0];
type Patch = (key: string, s: StatusPatch) => void;

type UndoMap = Record<string, { from: Column; to: Column }>;

/** The "Moved to X · Undo" strip: one per card, gone after `ms` or on that card's next move. */
function useUndo(ms: number) {
  const [undoable, setUndoable] = useState<UndoMap>({});
  const timers = useRef<Record<string, ReturnType<typeof setTimeout>>>({});
  const clear = useCallback((key: string) => {
    clearTimeout(timers.current[key]);
    delete timers.current[key];
    setUndoable(({ [key]: _, ...rest }) => rest);
  }, []);
  const offer = (key: string, from: Column, to: Column) => {
    if (from === to) return;
    clearTimeout(timers.current[key]);
    setUndoable((u) => ({ ...u, [key]: { from, to } }));
    timers.current[key] = setTimeout(() => clear(key), ms);
  };
  useEffect(() => () => Object.values(timers.current).forEach(clearTimeout), []);
  return { undoable, clearUndo: clear, offerUndo: offer };
}

/** Optimistic move: placement, pending flags, per-card errors, chooser, spoken outcome, landing highlight. */
function useMoves(patch: Patch, actions: TaskActions, undoMs: number) {
  const { undoable, clearUndo, offerUndo } = useUndo(undoMs);
  const [placed, setPlaced] = useState<Record<string, Column>>({});
  const [pending, setPending] = useState<Set<string>>(new Set());
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [chooser, setChooser] = useState<Chooser | null>(null);
  const [announce, setAnnounce] = useState("");
  const [landed, setLanded] = useState<string | null>(null);

  const dismiss = (key: string) => setErrors(({ [key]: _, ...rest }) => rest);
  const begin = (key: string, target: Column) => {
    setChooser(null);
    clearUndo(key);
    setPlaced((p) => ({ ...p, [key]: target }));
    setPending((p) => new Set(p).add(key));
    dismiss(key);
  };
  const settle = (key: string, target: Column, reason?: string) => {
    setPlaced(({ [key]: _, ...rest }) => rest);
    setPending((p) => new Set([...p].filter((k) => k !== key)));
    if (!reason) return;
    setErrors((e) => ({ ...e, [key]: `Couldn't move to ${columnTitle(target)} — ${reason}` }));
    setAnnounce(`Couldn't move ${key} to ${columnTitle(target)}.`);
  };
  const succeed = (key: string, s: StatusPatch, from: Column) => {
    patch(key, s);
    const to = columnOf(s.status_category);
    setAnnounce(`Moved ${key} to ${columnTitle(to)}.`);
    setLanded(key);
    offerUndo(key, from, to);
  };
  useEffect(() => {
    if (!landed) return;
    const t = setTimeout(() => setLanded(null), 1200);
    return () => clearTimeout(t);
  }, [landed]);

  async function run(key: string, target: Column, t: Transition, from: Column) {
    begin(key, target);
    const res = await actions.transitionTicket(key, t.id);
    if (res.ok) succeed(key, { status: res.data.status, status_category: res.data.status_category }, from);
    settle(key, target, res.ok ? undefined : res.error);
  }

  async function move(row: TaskRow, target: Column, from: Column, back = false) {
    begin(row.key, target);
    const res = await actions.loadTransitions(row.key);
    if (!res.ok) return settle(row.key, target, res.error);
    const moves = movesInto(res.data, target);
    if (moves.length === 1) return run(row.key, target, moves[0], from);
    if (moves.length > 1) {
      settle(row.key, target);
      return setChooser({ key: row.key, column: target, transitions: moves, from });
    }
    const status = row.status ?? "its status";
    settle(
      row.key,
      target,
      back
        ? `Jira has no way back to ${columnTitle(target)} from ${status}.`
        : `Jira has no move from ${status} to ${columnTitle(target)}.`,
    );
  }

  return { placed, pending, errors, chooser, announce, landed, undoable, offerUndo, dismiss, cancel: () => setChooser(null), move, run };
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
    // Capture phase + preventDefault: the panel's Esc handler sees defaultPrevented and stays open.
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      cancel();
    };
    document.addEventListener("keydown", onKey, true);
    return () => document.removeEventListener("keydown", onKey, true);
  }, [active, cancel]);
}

/** Which card's panel is open; drafts outlive the panel so closing or switching never loses a comment. */
function usePanel() {
  const [openKey, setOpenKey] = useState<string | null>(null);
  const drafts = useRef<Drafts>({});
  const closePanel = useCallback(() => {
    if (openKey) document.querySelector<HTMLElement>(`[data-task-key="${openKey}"] button`)?.focus();
    setOpenKey(null);
  }, [openKey]);
  return { openKey, setOpenKey, drafts, closePanel };
}

export function TaskBoard({ tasks, actions = realActions, undoMs = 8000 }: {
  tasks: TaskRow[];
  actions?: TaskActions;
  undoMs?: number;
}) {
  const [rows, setRows] = useState(tasks);
  const [text, setText] = useState("");
  const [onlyWorked, setOnlyWorked] = useState(false);
  const { openKey, setOpenKey, drafts, closePanel } = usePanel();

  const patch = useCallback<Patch>(
    (key, s) => setRows((rs) => rs.map((r) => (r.key === key ? { ...r, ...s } : r))),
    [],
  );
  const m = useMoves(patch, actions, undoMs);
  const colOf = (r: TaskRow) => m.placed[r.key] ?? columnOf(r.status_category);
  // One flow for a drop and for the Move menu; dropping on the card's own column is a no-op.
  const moveTo = (key: string, target: Column, back = false) => {
    const row = rows.find((r) => r.key === key);
    if (row && colOf(row) !== target) m.move(row, target, colOf(row), back);
  };
  const drag = useDrag(moveTo);
  useChooserEscape(m.chooser !== null, m.cancel);

  if (rows.length === 0) return EMPTY;

  const q = text.trim();
  const note = q ? `No tickets match “${q}”.` : onlyWorked ? "No tickets match the filter." : null;
  const visible = rows.filter((r) => matches(r, text, onlyWorked));
  const open = rows.find((r) => r.key === openKey);
  const undoFor = (key: string) => {
    const u = m.undoable[key];
    return u && { to: columnTitle(u.to), run: () => moveTo(key, u.from, true) };
  };

  return (
    <>
      <TaskToolbar text={text} onlyWorked={onlyWorked} setText={setText} setOnlyWorked={setOnlyWorked} />
      <p className="task-sr" role="status" aria-live="polite">
        {m.announce}
      </p>
      <div className="task-board-scroll">
      <div className={open ? "task-board has-panel" : "task-board"}>
        {COLUMNS.map(({ id }) => {
          const cards = visible.filter((r) => colOf(r) === id);
          return (
            <TaskColumn
              key={id}
              id={id}
              count={cards.length}
              over={drag.over === id}
              note={cards.length === 0 ? note : null}
              hint={cards.length === 0 && !note}
              chooser={m.chooser?.column === id ? m.chooser : null}
              onPick={(t) => m.chooser && m.run(m.chooser.key, id, t, m.chooser.from)}
              onCancel={m.cancel}
              {...drag.column(id)}
            >
              {cards.map((r) => (
                <TaskCard
                  key={r.key}
                  task={r}
                  column={id}
                  selected={openKey === r.key}
                  pending={m.pending.has(r.key)}
                  dragging={drag.dragKey === r.key}
                  error={m.errors[r.key]}
                  landed={m.landed === r.key}
                  undo={undoFor(r.key)}
                  onMove={(to) => moveTo(r.key, to)}
                  onDismissError={() => m.dismiss(r.key)}
                  onOpen={() => setOpenKey(r.key)}
                  onDragStart={(dt) => drag.start(r.key, dt)}
                  onDragEnd={drag.end}
                />
              ))}
            </TaskColumn>
          );
        })}
      </div>
      </div>
      {open && (
        <TaskPanel key={open.key} drafts={drafts} task={open} actions={actions} onClose={closePanel} onStatus={(s) => {
          m.offerUndo(open.key, colOf(open), columnOf(s.status_category));
          patch(open.key, s);
        }}
        />
      )}
    </>
  );
}
