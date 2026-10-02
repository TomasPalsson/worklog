import { useEffect, useState, type RefObject } from "react";

/** True while a clamped element really overflows (measured, not guessed). While `open` the last measurement is kept so "less" stays. */
export function useOverflow(ref: RefObject<HTMLElement | null>, dep: unknown, open: boolean): boolean {
  const [overflows, setOverflows] = useState(false);
  useEffect(() => {
    const el = ref.current;
    if (!el || open) return;
    const measure = () => setOverflows(el.scrollHeight > el.clientHeight + 1);
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [ref, dep, open]);
  return overflows;
}
