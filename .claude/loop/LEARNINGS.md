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

