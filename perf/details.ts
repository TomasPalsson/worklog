// T6b: /blocks/:id/details for 2026-09-22's heaviest-session block. T6's own block is
// light on claude_tool/claude_helper/claude_message rows, so it doesn't exercise the
// Details SQL day pre-filter under real load. Registered into perf/bench.ts's daemon
// scenario table; run via `bun perf/bench.ts --only T6,T6b`.
import { PERF_DIR, readonlyDb, daemonFetch } from "./lib";

const DAY = "2026-09-22";
const HELPER_SOURCES = ["claude_tool", "claude_helper", "claude_message"];

/**
 * The block on `DAY` whose linked session(s) carry the most claude_tool/claude_helper/
 * claude_message rows — the count `helper_activity_for_sessions` (block_details.rs) has
 * to fetch and filter before the day pre-filter narrows it down.
 */
export function computeT6bBlockId(): number {
  const db = readonlyDb(`${PERF_DIR}/data/worklog.db`);
  try {
    const row = db
      .query(
        `WITH block_sessions AS (
           SELECT DISTINCT b.id AS block_id, e.session_id AS session_id
             FROM blocks b
             JOIN block_events be ON be.block_id = b.id
             JOIN events e ON e.id = be.event_id
            WHERE b.day = ? AND e.session_id IS NOT NULL
         )
         SELECT bs.block_id AS id, COUNT(*) AS n
           FROM block_sessions bs
           JOIN events act ON act.session_id = bs.session_id AND act.source IN (?, ?, ?)
          GROUP BY bs.block_id
          ORDER BY n DESC, bs.block_id
          LIMIT 1`,
      )
      .get(DAY, ...HELPER_SOURCES) as { id: number } | null;
    if (!row) throw new Error(`no block with helper/tool/message activity found for day ${DAY}`);
    return row.id;
  } finally {
    db.close();
  }
}

type DaemonResp = Record<string, { status: number; body: string }>;

export function buildT6bScenario(blockId: number) {
  return {
    id: "T6b", kind: "daemon" as const, defaultRuns: 20,
    run: async (port: number): Promise<DaemonResp> => {
      const details = await daemonFetch(port, `/blocks/${blockId}/details`);
      return { [`GET /blocks/${blockId}/details`]: details };
    },
  };
}
