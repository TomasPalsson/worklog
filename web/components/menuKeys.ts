import type { KeyboardEvent } from "react";

/** Arrow Up/Down (wrapping), Home and End move focus between a menu's `role="menuitem"` children. */
export function menuKeys(e: KeyboardEvent<HTMLElement>) {
  const items = [...e.currentTarget.querySelectorAll<HTMLElement>('[role="menuitem"]')];
  const at = items.indexOf(document.activeElement as HTMLElement);
  const next = { ArrowDown: at + 1, ArrowUp: at - 1, Home: 0, End: items.length - 1 }[e.key];
  if (next === undefined || items.length === 0) return;
  e.preventDefault();
  items[(next + items.length) % items.length].focus();
}
