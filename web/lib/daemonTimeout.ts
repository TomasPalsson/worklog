// Per-route request timeouts for the daemon transport (lib/daemon.ts).

/**
 * Per-request timeout. `worklog estimate` shells out to `claude -p`
 * which can take 30+ seconds per block on larger days, so we use a
 * generous 60s cap for estimate-like routes and 10s for the rest.
 * Without this, a wedged daemon leaves the UI spinning forever.
 */
export function timeoutMs(path: string): number {
  // `includes` not `startsWith`: the per-block route is
  // POST /blocks/:id/estimate, so a startsWith check would fall through to
  // the 10s default and time out every Sparkles click.
  //
  // 10 minutes, not 60s: estimation is one `claude -p` per un-estimated
  // block, run sequentially. Measured on a real 16-block day: 216s (~13s a
  // block), so 60s failed every multi-block day even when the estimate
  // itself succeeded — the daemon kept working and the UI reported a
  // timeout. 10 minutes covers roughly 45 blocks; past that, run
  // `worklog estimate` in a terminal, which has no HTTP timeout.
  if (path.includes("/estimate")) return 600_000;
  if (path.startsWith("/sync")) return 30_000;
  // Statistics scans every event in the range.
  if (path.startsWith("/stats")) return 30_000;
  if (path.startsWith("/jira/refresh")) return 30_000;
  if (path.startsWith("/infer")) return 30_000;
  // Jira/Tempo round-trips: project + account listing and issue creation.
  if (path.startsWith("/tickets/create")) return 30_000;
  // Tempo hub: the AI draft shells out to `claude -p`; the pull pages Tempo.
  if (path.endsWith("/draft")) return 90_000;
  // A ticket line's Generate/Regenerate is one thinking `claude -p` call — well past 10s.
  if (path === "/tempo/lines/regenerate") return 120_000;
  if (path.startsWith("/tempo/pull")) return 60_000;
  if (/^\/tickets\/[^/]+\/(transitions?|comment|detail|blocks|log)$/.test(path)) return 20_000;
  if (path.startsWith("/projects")) return 20_000;
  if (path.startsWith("/accounts")) return 20_000;
  return 10_000;
}
