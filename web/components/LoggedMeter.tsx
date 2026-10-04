import type { CSSProperties } from "react";
import type { LoggedDay } from "@/lib/logged_contract";
import { hours } from "./LoggedEntries";

type Props = {
  logged: number;
  required: number | null;
  state: LoggedDay["state"];
  orientation?: "row" | "column";
};

/** A fill meter: logged / required, capped at 100%. Colour comes from data-state. */
export function LoggedMeter({ logged, required, state, orientation = "row" }: Props) {
  const max = required ?? 0;
  const pct = max > 0 ? Math.min(logged / max, 1) * 100 : 0;
  return (
    <span
      className="logged-meter"
      data-state={state}
      data-orientation={orientation}
      data-some={logged > 0 ? "" : undefined}
      role="meter"
      aria-valuemin={0}
      aria-valuemax={max}
      aria-valuenow={logged}
      aria-label={`${hours(logged)} of ${hours(max)} logged`}
      style={{ "--fill": `${pct}%` } as CSSProperties}
    />
  );
}
