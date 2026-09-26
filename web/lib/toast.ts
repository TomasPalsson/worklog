// Ultra-light toast bus. A component mounts one <ToastHost /> at the
// top of the tree; any client code can call `toast.error(...)` or
// `toast.ok(...)` to show a message. Used so Server Action errors that
// useTransition would otherwise swallow reach the user.

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

export const toast = {
  ok: (text: string, action?: ToastAction) => push("ok", text, 3500, action),
  error: (text: string, action?: ToastAction) => push("error", text, 6000, action),
  // Change-batch pop-ups (FR-10): longer-lived than a plain ok toast so
  // there's time to click "Show" before it auto-dismisses.
  notice: (text: string, action?: ToastAction) => push("ok", text, 10000, action),
};

export function subscribe(listener: Listener): () => void {
  listeners.add(listener);
  listener(queue);
  return () => {
    listeners.delete(listener);
  };
}
