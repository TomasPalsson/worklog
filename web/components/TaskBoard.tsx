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
import { toast } from "@/lib/toast";
import type { TaskRow, Transition } from "@/lib/types";
import { TaskCard, type TaskActions } from "./TaskCard";
import type { Drafts } from "./TaskComposer";
import { TaskColumn, type Chooser } from "./TaskColumn";
import { TaskPanel, type TaskPanelProps } from "./TaskPanel";

const realActions: TaskActions = {
  loadTransitions,
  loadTicketDetail,
  transitionTicket,
  commentOnTicket,
  draftTicketUpdate,
};

type StatusPatch = Parameters<TaskPanelProps["onStatus"]>[0];
type Patch = (key: string, s: StatusPatch) => void;

/** Optimistic drag-to-move: placement, pending flags, per-card errors, chooser. */
function useMoves(patch: Patch, actions: TaskActions) {
  const [placed, setPlaced] = useState<Record<string, Column>>({});
  const [pending, setPending] = useState<Set<string>>(new Set());
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [chooser, setChooser] = useState<Chooser | null>(null);

  const begin = (key: string, target: Column) => {
    setChooser(null);
    setPlaced((p) => ({ ...p, [key]: target }));
    setPending((p) => new Set(p).add(key));
    setErrors(({ [key]: _, ...rest }) => rest);
  };
  const settle = (key: string, error?: string) => {
    setPlaced(({ [key]: _, ...rest }) => rest);
    setPending((p) => new Set([...p].filter((k) => k !== key)));
    if (error) {
      setErrors((e) => ({ ...e, [key]: error }));
      toast.error(error);
    }
  };

  async function run(key: string, target: Column, t: Transition) {
    begin(key, target);
    const res = await actions.transitionTicket(key, t.id);
    if (res.ok) patch(key, { status: res.data.status, status_category: res.data.status_category });
    settle(key, res.ok ? undefined : res.error);
  }

  async function move(row: TaskRow, target: Column) {
    begin(row.key, target);
    const res = await actions.loadTransitions(row.key);
    if (!res.ok) return settle(row.key, res.error);
    const moves = movesInto(res.data, target);
    if (moves.length === 1) return run(row.key, target, moves[0]);
    if (moves.length > 1) {
      settle(row.key);
      return setChooser({ key: row.key, column: target, transitions: moves });
    }
    settle(row.key, `Jira has no move from ${row.status ?? "its status"} to ${columnTitle(target)} for ${row.key}.`);
  }

  return { placed, pending, errors, chooser, cancel: () => setChooser(null), move, run };
}

/** Native HTML5 DnD state. The key lives in React state; dataTransfer is a fallback. */
function useDrag(rows: TaskRow[], colOf: (r: TaskRow) => Column, move: (r: TaskRow, c: Column) => void) {
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
        const row = rows.find((r) => r.key === key);
        end();
        if (row && colOf(row) !== id) move(row, id);
      },
    }),
  };
}

function matches(t: TaskRow, text: string, onlyWorked: boolean) {
  const q = text.trim().toLowerCase();
  if (onlyWorked && t.week_seconds <= 0) return false;
  return !q || t.key.toLowerCase().includes(q) || t.summary.toLowerCase().includes(q);
}

function Toolbar(p: {
  text: string;
  onlyWorked: boolean;
  setText: (v: string) => void;
  setOnlyWorked: (v: boolean) => void;
}) {
  return (
    <div className="task-toolbar">
      <label className="task-filter">
        <span className="task-label">Filter</span>
        <input type="search" value={p.text} onChange={(e) => p.setText(e.target.value)} />
      </label>
      <label className="task-check">
        <input type="checkbox" checked={p.onlyWorked} onChange={(e) => p.setOnlyWorked(e.target.checked)} />
        Only tickets I worked this week
      </label>
    </div>
  );
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

export function TaskBoard({ tasks, actions = realActions }: { tasks: TaskRow[]; actions?: TaskActions }) {
  const [rows, setRows] = useState(tasks);
  const [text, setText] = useState("");
  const [onlyWorked, setOnlyWorked] = useState(false);
  const { openKey, setOpenKey, drafts, closePanel } = usePanel();

  const patch = useCallback<Patch>(
    (key, s) => setRows((rs) => rs.map((r) => (r.key === key ? { ...r, ...s } : r))),
    [],
  );
  const m = useMoves(patch, actions);
  const colOf = (r: TaskRow) => m.placed[r.key] ?? columnOf(r.status_category);
  const drag = useDrag(rows, colOf, m.move);
  useChooserEscape(m.chooser !== null, m.cancel);

  if (rows.length === 0) return EMPTY;

  const q = text.trim();
  const note = q ? `No tickets match “${q}”.` : onlyWorked ? "No tickets match the filter." : null;
  const visible = rows.filter((r) => matches(r, text, onlyWorked));
  const open = rows.find((r) => r.key === openKey);

  return (
    <>
      <Toolbar text={text} onlyWorked={onlyWorked} setText={setText} setOnlyWorked={setOnlyWorked} />
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
              chooser={m.chooser?.column === id ? m.chooser : null}
              onPick={(t) => m.chooser && m.run(m.chooser.key, id, t)}
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
                  onOpen={() => setOpenKey(r.key)}
                  onDragStart={(dt) => drag.start(r.key, dt)}
                  onDragEnd={drag.end}
                />
              ))}
            </TaskColumn>
          );
        })}
      </div>
      {open && (
        <TaskPanel key={open.key} drafts={drafts} task={open} actions={actions} onClose={closePanel} onStatus={(s) => patch(open.key, s)} />
      )}
    </>
  );
}
