// Ultra-light toast bus. A component mounts one <ToastHost /> at the
// top of the tree; any client code can call `toast.error(...)` or
// `toast.ok(...)` to show a message. Used so Server Action errors that
// useTransition would otherwise swallow reach the user.

export type Tone = "ok" | "error";

export interface ToastMsg {
  id: number;
  tone: Tone;
  text: string;
}

type Listener = (msgs: ToastMsg[]) => void;

let nextId = 1;
let queue: ToastMsg[] = [];
const listeners: Set<Listener> = new Set();

function emit() {
  for (const l of listeners) l(queue);
}

function push(tone: Tone, text: string, ttlMs = 3500) {
  const msg = { id: nextId++, tone, text };
  queue = [...queue, msg];
  emit();
  // Auto-dismiss after ttl.
  setTimeout(() => {
    queue = queue.filter((m) => m.id !== msg.id);
    emit();
  }, ttlMs);
}

export const toast = {
  ok: (text: string) => push("ok", text),
  error: (text: string) => push("error", text, 6000),
};

export function subscribe(listener: Listener): () => void {
  listeners.add(listener);
  listener(queue);
  return () => {
    listeners.delete(listener);
  };
}
