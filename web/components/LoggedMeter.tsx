import type { CSSProperties } from "react";
import type { LoggedDay } from "@/lib/logged_contract";
import { hours } from "./LoggedEntries";

type Props = {
  logged: number;
  required: number | null;
  state: LoggedDay["state"];
  orientation?: "row" | "column";
  /** Seconds worked; when above `logged`, a ghost layer shows the not-yet-synced part. */
  worked?: number;
};

/** A fill meter: logged / required, capped at 100%. Colour comes from data-state. */
export function LoggedMeter({ logged, required, state, orientation = "row", worked }: Props) {
  const max = required ?? 0;
  const pct = max > 0 ? Math.min(logged / max, 1) * 100 : 0;
  const ghost = worked !== undefined && worked > logged && max > 0;
  const ghostPct = ghost ? Math.min(worked / max, 1) * 100 : 0;
  return (
    <span
      className="logged-meter"
      data-state={state}
      data-orientation={orientation}
      data-ghost={ghost ? "" : undefined}
      data-some={logged > 0 ? "" : undefined}
      role="meter"
      aria-valuemin={0}
      aria-valuemax={max}
      aria-valuenow={logged}
      aria-label={`${hours(logged)} of ${hours(max)} logged${worked !== undefined ? `, ${hours(worked)} worked` : ""}`}
      style={{ "--fill": `${pct}%`, ...(ghost ? { "--ghost": `${ghostPct}%` } : {}) } as CSSProperties}
    />
  );
}
