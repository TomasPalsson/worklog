## Codebase patterns

- 2026-09-23: L1 (fish collector) done. Collector signature convention is
  `collect(conn, since: NaiveDate, until: NaiveDate)` + `collect_with`/
  `collect_from_*` test entrypoint (see github.rs/gcal.rs); date bounds
  convert via `since.and_time(NaiveTime::MIN).and_utc().timestamp()`.
  `billing::work_folder_for_path` only resolves `~/Desktop/Work` and
  returns a bare key, not a full root path — L1/L2 needed the repo
  **root path** under either `~/Desktop/Work` or `~/Desktop/Projects`,
  so `collectors/fish.rs` has its own small `repo_root_for` helper
  (worktree-collapse copied from billing.rs, both prefixes, returns
  `<prefix>/<key>`). Reuse that helper (or lift it to a shared spot) for
  L2's reflog collector instead of re-deriving it.
- 2026-09-23: L2 (git reflog collector) done. Didn't end up needing
  `fish::repo_root_for` — reflog only walks repos **directly** under
  each root (`read_dir(root)` + check for a dir), so the repo dir found
  during traversal already **is** the project root; no path-prefix
  resolution needed. Reflog line parsing: split on the first `\t` to
  separate `<old> <new> <name> <<email>> <epoch> <tz>` from the
  message, then take `tokens[1]` (new sha) and the last two
  whitespace-split tokens (epoch, tz) — author name/email can contain
  spaces so don't assume fixed field count from the front. Only
  `checkout`/`merge` need their target parsed out of the message
  (`moving from X to Y` / `merge <branch>: ...`); everything else
  collapses to a bare action word so no commit-message text ever lands
  in `title`.

- 2026-09-23: L3 (CLI wiring for shell/reflog) done. `fish::collect` and
  `reflog::collect` both take just `(conn, since, until)` with defaults
  baked in — no client/auth arg to plumb, unlike jira/github/gcal/slack.
  Replaced the repeated `matches!(target, CollectTarget::All | CollectTarget::X)`
  idiom with a `wants(target, source)` helper (needs `PartialEq` on
  `CollectTarget`) so the new Shell/Reflog branches and the CLI-name-parsing
  test (`collect_targets_include_shell_and_reflog`) share one place to
  check "does this target run that source" instead of re-deriving it.

- 2026-09-23: L4 (shell/reflog events join blocks) done — **no infer.rs
  production code changed**, only the two tests were added.
  `build_blocks`/`split_by_project` already key off `InferEvent.source`/
  `project_path` generically; since fish.rs and reflog.rs (L1/L2) already
  populate `project_path` on `shell`/`git_reflog` events the same way
  every other collector does, the existing gap-timeout + project-split
  logic clusters and splits them for free. `infer.rs` is already ~1200
  lines (pre-existing, flagged by size-guard) — out of scope for this
  item since the loop's file allowlist is `infer.rs` only and no
  production line changed.

- 2026-09-23: L5 (timeline.rs: block_confidence + day_gaps) done. New
  standalone module, no dependency on infer.rs or collectors — just
  `chrono::{DateTime, Duration, Utc}` like the rest of the crate.
  `day_gaps` merges overlapping/adjacent blocks first (sort by start,
  extend-or-push), then reports gaps only between the merged pairs —
  that's what makes overlapping input blocks produce zero gaps for
  free instead of needing a special case.

- 2026-09-23: L6 (daemon: confidence + gaps on /days/:day) done.
  `BlockSummary.confidence` = `timeline::block_confidence(sources.len())`
  where `sources` is the already-grouped-by-source `Vec<SourceCount>` built
  in `stitch_day_summary` — distinct source count is just its length, no
  new query. `DaySummary.gaps` comes from `timeline::day_gaps` fed the
  day's `(started_at, ended_at)` parsed via
  `chrono::DateTime::parse_from_rfc3339(..).to_utc()` (same pattern as
  `billing::block_interval`), min_gap 30 min, formatted back with
  `.to_rfc3339()`. Kept `stitch_day_summary` and the file's overall size
  as-is per the loop's explicit instruction not to split daemon.rs; the
  size-guard hook's file-length complaint is expected noise here, but it
  does still flag oversized *functions* — factoring the new test's block+
  event seeding into a small `seed_block` helper kept the test under the
  60-line function cap.

- 2026-09-23: L7 (web: confidence badge + day gaps) done. `Block` in
  `lib/types.ts` already mirrors the daemon's flattened `BlockSummary`
  (event_count/sources/project_path live there), so `confidence` was
  added to `Block` itself rather than inventing a separate
  `BlockSummary` type; `DaySummary` is a local interface in
  `lib/daemon.ts` (not exported from types.ts) — added `gaps: DayGap[]`
  there, with `DayGap` as a new exported type in types.ts. `page.tsx`
  is an async server component (uses `next/headers`), so it isn't
  unit-testable with bun:test/testing-library like client components
  are — pulled the gap row's text into a pure `formatGapRow` helper in
  `lib/format.ts` (reusing `formatRange`) and tested that in
  `lib/format.test.ts` instead of rendering the page. `BlockCard.tsx`
  statically imports `TicketCombobox` → `CreateTicketDialog`, which
  pull in `createTicket`/`fetchAccounts`/`fetchProjects`/
  `assignTicket`/`assignExternalTicket`/`searchJiraTickets` from
  `@/app/actions` regardless of the `hideTicketing` prop (ES imports
  are static) — `mock.module("@/app/actions", ...)` in a component test
  must stub every export reachable from the whole import graph, not
  just the ones the test's props path uses, or bun throws "Export
  named 'X' not found".
