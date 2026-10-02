"use client";

import type { DragEvent, ReactNode } from "react";

import { COLUMNS, transitionLabel, type Column } from "@/lib/taskBoard";
import type { Transition } from "@/lib/types";

export interface Chooser {
  key: string;
  column: Column;
  transitions: Transition[];
  /** Column the card sat in, so a successful pick can offer Undo. */
  from: Column;
}

interface Props {
  id: Column;
  count: number;
  over: boolean;
  note: string | null;
  hint: boolean;
  chooser: Chooser | null;
  onPick: (t: Transition) => void;
  onCancel: () => void;
  onDragOver: (e: DragEvent) => void;
  onDragLeave: (e: DragEvent) => void;
  onDrop: (e: DragEvent) => void;
  children: ReactNode;
}

function ChooserBox({ chooser, title, onPick, onCancel }: {
  chooser: Chooser;
  title: string;
  onPick: (t: Transition) => void;
  onCancel: () => void;
}) {
  return (
    <div role="group" aria-label={`Move ${chooser.key} to ${title}?`} className="task-chooser">
      <p>{`Move ${chooser.key} to ${title}?`}</p>
      {chooser.transitions.map((t) => (
        <button key={t.id} type="button" onClick={() => onPick(t)}>
          {transitionLabel(t)}
        </button>
      ))}
      <button type="button" className="task-chooser-cancel" onClick={onCancel}>
        Cancel
      </button>
    </div>
  );
}

export function TaskColumn(p: Props) {
  const title = COLUMNS.find((c) => c.id === p.id)?.title ?? p.id;
  return (
    <section
      className="task-col"
      data-col={p.id}
      data-over={p.over || undefined}
      data-testid={`column-${p.id}`}
      aria-label={title}
      onDragOver={p.onDragOver}
      onDragLeave={p.onDragLeave}
      onDrop={p.onDrop}
    >
      <h2 className="task-col-head">
        <span>{p.over ? `${title} · release to move` : title}</span>
        <span className="task-col-count" data-testid={`count-${p.id}`}>
          {p.count}
        </span>
      </h2>
      {p.chooser && <ChooserBox chooser={p.chooser} title={title} onPick={p.onPick} onCancel={p.onCancel} />}
      {p.note && <p className="task-col-note">{p.note}</p>}
      {p.hint && <p className="task-col-hint">Drop a ticket here</p>}
      <ul className="task-list">{p.children}</ul>
    </section>
  );
}
