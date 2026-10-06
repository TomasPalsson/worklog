// Ultra-light toast bus. A component mounts one <ToastHost /> at the
// top of the tree; any client code can call `toast.error(...)` or
// `toast.ok(...)` to show a message. Used so Server Action errors that
// useTransition would otherwise swallow reach the user.

import type { UndoOutcome } from "./daily_helpers_contract";

export type Tone = "ok" | "error";

/** A clickable button rendered alongside the toast text. */
export interface ToastAction {
  label: string;
  onClick: () => void;
}

export interface ToastMsg {
  id: number;
  tone: Tone;
  text: string;
  action?: ToastAction;
}

type Listener = (msgs: ToastMsg[]) => void;

let nextId = 1;
let queue: ToastMsg[] = [];
const listeners: Set<Listener> = new Set();

function emit() {
  for (const l of listeners) l(queue);
}

function push(tone: Tone, text: string, ttlMs = 3500, action?: ToastAction) {
  const msg = { id: nextId++, tone, text, action };
  queue = [...queue, msg];
  emit();
  // Auto-dismiss after ttl.
  setTimeout(() => {
    queue = queue.filter((m) => m.id !== msg.id);
    emit();
  }, ttlMs);
}

type UndoResult = { ok: true; data: UndoOutcome } | { ok: false; error: string };

async function runUndo(undo: () => Promise<UndoResult>) {
  const r = await undo();
  if (!r.ok) return toast.error(`Undo failed — ${r.error}`);
  switch (r.data.outcome) {
    case "restored":
      return toast.ok(`Undid ${r.data.change}`);
    case "nothing_to_undo":
      return toast.ok("Nothing left to undo");
    case "refused_synced":
      return toast.error(`Block ${r.data.block_id} was sent to Tempo; undo would desync it`);
  }
}

export const toast = {
  ok: (text: string, action?: ToastAction) => push("ok", text, 3500, action),
  error: (text: string, action?: ToastAction) => push("error", text, 6000, action),
  // Change-batch pop-ups (FR-10): longer-lived than a plain ok toast so
  // there's time to click "Show" before it auto-dismisses.
  // Confirmation after a block change, with Undo wired to the daemon's journal.
  undoable: (text: string, undo: () => Promise<UndoResult>) =>
    push("ok", text, 10000, { label: "Undo", onClick: () => void runUndo(undo) }),
  notice: (text: string, action?: ToastAction) => push("ok", text, 10000, action),
};

/** Removes a toast now — used when its action button is clicked. */
export function dismiss(id: number) {
  queue = queue.filter((m) => m.id !== id);
  emit();
}

export function subscribe(listener: Listener): () => void {
  listeners.add(listener);
  listener(queue);
  return () => {
    listeners.delete(listener);
  };
}
