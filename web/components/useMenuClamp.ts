import { useLayoutEffect, useState, type RefObject } from "react";

/** Keeps a menu on screen; `deps` re-measure it when its content swaps. */
export function useMenuClamp(ref: RefObject<HTMLElement | null>, x: number, y: number, deps: unknown[]) {
  const [pos, setPos] = useState({ left: x, top: y });
  useLayoutEffect(() => {
    const box = ref.current?.getBoundingClientRect();
    if (!box) return;
    setPos({
      left: Math.max(8, Math.min(x, window.innerWidth - box.width - 8)),
      top: Math.max(8, Math.min(y, window.innerHeight - box.height - 8)),
    });
  }, [ref, x, y, ...deps]);
  return pos;
}
