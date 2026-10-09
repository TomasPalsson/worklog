// Progress strip for the billing-export wizard: one segment per line, width
// proportional to its hours. Replaces the equal-size dots.

import type { BillingRow } from "@/lib/types";

export type SegState = "done" | "todo" | "needs";

type StripRow = Pick<BillingRow, "hours" | "customer" | "verkefni" | "needs_description" | "folder">;

/** Done wins; otherwise anything the Owner still has to supply is "needs". */
export function segState(row: StripRow, isDone: boolean): SegState {
  if (isDone) return "done";
  if (row.customer === null || row.verkefni === null || row.needs_description) return "needs";
  return "todo";
}

const STATE_WORD: Record<SegState, string> = { done: "done", todo: "to do", needs: "needs you" };

export function segLabel(row: StripRow, i: number, n: number, state: SegState): string {
  return `Line ${i + 1} of ${n}: ${row.customer ?? row.folder}, ${row.hours}h, ${STATE_WORD[state]}`;
}

export function ExportStrip({
  rows,
  index,
  done,
  onSelect,
}: {
  rows: StripRow[];
  index: number;
  done: Set<number>;
  onSelect: (i: number) => void;
}) {
  const missing = rows.filter((r) => r.customer === null || r.verkefni === null).length;
  return (
    <div className="art-export-strip-wrap">
      <ol className="art-export-strip" aria-label="line progress">
        {rows.map((r, i) => {
          const state = segState(r, done.has(i));
          return (
            <li key={i} className="art-export-seg-li" style={{ flexGrow: Math.max(r.hours, 0) }}>
              <button
                type="button"
                className={`art-export-seg is-${state}${i === index ? " is-current" : ""}`}
                aria-label={segLabel(r, i, rows.length, state)}
                aria-current={i === index || undefined}
                onClick={() => onSelect(i)}
              />
            </li>
          );
        })}
      </ol>
      <p className="art-export-legend">
        <span><i className="art-export-sw is-done" aria-hidden="true" />done</span>
        <span><i className="art-export-sw is-todo" aria-hidden="true" />to do</span>
        <span><i className="art-export-sw is-needs" aria-hidden="true" />needs you</span>
        {missing > 0 && <span className="art-export-legend-n">{missing} need a pick</span>}
      </p>
    </div>
  );
}
