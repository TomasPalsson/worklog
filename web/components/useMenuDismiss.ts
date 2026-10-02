import { useEffect, type RefObject } from "react";

/** Esc (capture + preventDefault, so the dialog stays open) and an outside click close a menu; Esc refocuses its trigger. */
export function useMenuDismiss(open: boolean, wrap: RefObject<HTMLElement | null>, close: () => void) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      close();
      wrap.current?.querySelector<HTMLElement>("[aria-haspopup]")?.focus();
    };
    const onDown = (e: Event) => {
      if (!wrap.current?.contains(e.target as Node)) close();
    };
    document.addEventListener("keydown", onKey, true);
    document.addEventListener("mousedown", onDown);
    return () => {
      document.removeEventListener("keydown", onKey, true);
      document.removeEventListener("mousedown", onDown);
    };
  }, [open, wrap, close]);
}
