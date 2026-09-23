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
