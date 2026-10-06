import type { PreflightRow } from "@/lib/daily_helpers_contract";

export function ReadBackRow({ row }: { row: PreflightRow }) {
  return (
    <p role="status" className={row.ok ? "task-check-ok" : "task-check-red"}>
      <span role="img" aria-label={row.ok ? "Matches Tempo" : "Differs from Tempo"}>{row.ok ? "✓" : "✗"}</span>{" "}
      <span>{row.detail}</span>
    </p>
  );
}
