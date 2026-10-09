import Link from "next/link";
import { shiftDay } from "@/lib/format";

export type Range = "7d" | "30d" | "90d" | "all";

const CHIPS: [Range, string][] = [
  ["7d", "7 days"],
  ["30d", "30 days"],
  ["90d", "90 days"],
  ["all", "All time"],
];

export function parseRange(raw: string | undefined): Range {
  return CHIPS.some(([r]) => r === raw) ? (raw as Range) : "all";
}

/** from/to for GET /stats. "all" sends neither: the daemon starts at the first day with data. */
export function rangeDates(range: Range, today: string): { from?: string; to?: string } {
  if (range === "all") return {};
  const n = Number.parseInt(range, 10);
  return { from: shiftDay(today, -(n - 1)), to: today };
}

export function RangeChips({ current }: { current: Range }) {
  return (
    <nav className="stats-chips" aria-label="Date range">
      {CHIPS.map(([r, label]) => (
        <Link
          key={r}
          href={r === "all" ? "/stats" : `/stats?range=${r}`}
          className="stats-chip"
          aria-current={r === current ? "page" : undefined}
        >
          {label}
        </Link>
      ))}
    </nav>
  );
}
