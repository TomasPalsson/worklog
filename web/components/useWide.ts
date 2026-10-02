import { useEffect, useState } from "react";

const QUERY = "(min-width: 760px)";
const matches = () => typeof window.matchMedia !== "function" || window.matchMedia(QUERY).matches;

/** True at the widths where the dialog has a sidebar; follows the viewport while the dialog is open. */
export function useWide(): boolean {
  const [wide, setWide] = useState(matches);
  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const q = window.matchMedia(QUERY);
    const on = () => setWide(q.matches);
    q.addEventListener?.("change", on);
    return () => q.removeEventListener?.("change", on);
  }, []);
  return wide;
}
