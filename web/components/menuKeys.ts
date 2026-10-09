import type { KeyboardEvent } from "react";

/** Arrow Up/Down (wrapping), Home and End move focus between a menu's enabled `role="menuitem"` children. */
export function menuKeys(e: KeyboardEvent<HTMLElement>) {
  // focus() on a disabled button is a silent no-op, so a disabled item would trap the arrows.
  const items = [...e.currentTarget.querySelectorAll<HTMLElement>('[role="menuitem"]:not(:disabled)')];
  const at = items.indexOf(document.activeElement as HTMLElement);
  const next = { ArrowDown: at + 1, ArrowUp: at - 1, Home: 0, End: items.length - 1 }[e.key];
  if (next === undefined || items.length === 0) return;
  e.preventDefault();
  items[(next + items.length) % items.length].focus();
}
