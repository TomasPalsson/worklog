import { useEffect, type KeyboardEvent, type RefObject } from "react";

const FOCUSABLE = 'a[href], button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), summary, [tabindex]:not([tabindex="-1"])';

/** Tabbable descendants, minus anything hidden inside a closed <details> (its summary stays). */
function focusables(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => {
    const closed = el.closest("details:not([open])");
    return !closed || (el.tagName === "SUMMARY" && el.parentElement === closed);
  });
}

/** Keeps Tab and Shift+Tab inside the dialog: the last control wraps to the first and back. */
function trapTab(e: KeyboardEvent<HTMLElement>) {
  if (e.key !== "Tab") return;
  const items = focusables(e.currentTarget);
  if (items.length === 0) return;
  const first = items[0];
  const last = items[items.length - 1];
  const at = document.activeElement;
  if (e.shiftKey && (at === first || at === e.currentTarget)) {
    e.preventDefault();
    last.focus();
  } else if (!e.shiftKey && at === last) {
    e.preventDefault();
    first.focus();
  }
}

/**
 * Modal behaviour for the ticket dialog: focus lands on it, the page behind cannot scroll, Tab stays inside,
 * and Esc closes it unless an inner layer (menu, preview, editor, form) already handled the key.
 */
export function useModalShell(dialog: RefObject<HTMLElement | null>, onClose: () => void) {
  useEffect(() => dialog.current?.focus(), [dialog]);
  useEffect(() => {
    const html = document.documentElement;
    const before = html.style.overflow;
    html.style.overflow = "hidden";
    return () => {
      html.style.overflow = before;
    };
  }, []);
  useEffect(() => {
    const onKey = (e: globalThis.KeyboardEvent) => e.key === "Escape" && !e.defaultPrevented && onClose();
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);
  return { onKeyDown: trapTab };
}
