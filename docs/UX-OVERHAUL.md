# worklog — UX overhaul

A working document: where the friction is, what the ideal workflow looks
like, the user stories that define "done", and a roadmap. Phase 1 is
**shipped** (see the changelog at the bottom); the rest is proposed.

## The core complaint

> "I'm not a fan of how the CLI tool works, it adds so much overhead."

worklog's *pipeline* is good — signals are collected, clustered into
blocks, estimated, and synced with almost no effort. The friction is
everywhere **around** the pipeline:

1. **Two surfaces, arbitrarily split.** The CLI owns the pipeline
   (`collect`, `infer`, `estimate`, `sync`). Single-block edits — assign a
   ticket, fix a duration, delete — had **no CLI at all** and could only be
   done by hand-writing `curl http://127.0.0.1:9323/blocks/…`. Asking a
   simple question ("how many hours today?") meant `curl … | jq`.
2. **The web UI is the only review surface.** `worklog day` ends by
   spinning up a Docker container and a browser tab. For "what did I do
   today?" that is a sledgehammer — container start, page load, a GUI — to
   answer a question a paragraph of text could.
3. **No quick answers.** There is no `worklog today`. Every read is either
   the web UI or raw HTTP.
4. **Blocks fragment.** A single task often lands as 3–4 adjacent blocks
   (a commit here, three prompts there, a calendar event). The estimator
   auto-merges *same-ticket adjacent* blocks, but there was no way for a
   human to merge blocks the estimator left alone — e.g. two blocks that
   *should* share a ticket but don't yet.
5. **Discoverability.** 20-plus subcommands, flat namespace, `--help` is a
   wall. New users don't know where to start; returning users forget.

## Friction analysis — by the numbers

| Task | Before | Keystrokes / steps |
|------|--------|--------------------|
| "How many hours today, on what?" | `curl …/days/$(date +%F) \| jq` (and know the schema) | ~1 line of JSON spelunking |
| Reassign a block's ticket | `curl -X POST …/blocks/42/ticket -H … -d '{"jira_issue":"X"}'` | ~80 chars, exact JSON |
| Fix a duration | `curl -X POST …/blocks/42/duration -d '{"minutes":45}'` | same |
| Delete a block | `curl -X POST …/blocks/42/delete` | safe-ish, but no confirm, no Tempo feedback |
| Merge two blocks | **impossible** without raw SQL | — |
| See today's blocks with ids | `worklog day` → Docker → browser | container + GUI |

Every write demanded the user remember the daemon port, the route shape,
and the JSON body. That is the "overhead."

## Ideal workflow — the principle

> **The terminal answers, the CLI edits, the web UI is for deep review.**

A daily loop should never *require* the browser:

```
$ worklog today
2026-05-16 — 6h 25m tracked across 7 blocks

  ticket       time     what
  GENAI-1219   3h 10m   Review PR #38; auth fixes; deploy
  GOJ-1422     1h 45m   Estimator tuning
  unassigned   0h 40m   1 block — needs a ticket
  personal     0h 50m   2 blocks

  5h 35m work · 0h 50m personal
  ! 1 block needs a ticket — worklog block list
  ! 2 blocks edited since last sync — worklog sync

$ worklog block list                 # see ids + state
$ worklog block assign 281 GOJ-1422   # fix the unassigned one
$ worklog block merge 278 279 280     # collapse a fragmented task
$ worklog sync --dry-run              # preview
$ worklog sync                        # ship
```

No `curl`. No browser. No remembering JSON. The web UI is still there for
when you *want* the timeline view and drill-downs — but it is a choice,
not a toll booth.

## User stories

Format: **As a** … **I want** … **so that** …. Tagged `[shipped]`,
`[next]`, or `[later]`.

### Querying — "what did I do?"

- **US-1** `[shipped]` As a developer wrapping up my day, I want
  `worklog today` to print my total hours and a per-ticket breakdown in
  the terminal, so that I don't open a browser to answer a 5-second
  question.
- **US-2** `[shipped]` As a developer, I want `worklog summary --day
  2026-05-12` for any past day, so that I can reconstruct a timesheet
  retroactively.
- **US-3** `[shipped]` As a developer, I want the summary to *tell me what
  to do next* (unassigned blocks, unsynced edits, failed estimates), so
  that I don't have to know which command comes next.
- **US-4** `[shipped]` As a developer, I want `worklog week` to roll the
  per-ticket breakdown across 7 days, so that I can sanity-check a week
  before the Friday timesheet.
- **US-5** `[later]` As a developer, I want `worklog ask "how long on
  GENAI-1219 this month?"` to answer free-form questions, so that I never
  think about schemas. (Implementable as a thin local query DSL, or an
  estimator-provider call.)

### Editing blocks — "fix it fast"

- **US-6** `[shipped]` As a developer, I want `worklog block list` to show
  every block's id, time range, ticket and lifecycle state, so that I have
  the ids the edit commands need.
- **US-7** `[shipped]` As a developer, I want `worklog block assign <id>
  <ticket>` (and `--clear`), so that I fix a misrouted block in one line.
- **US-8** `[shipped]` As a developer, I want `worklog block duration <id>
  <minutes>` and `worklog block describe <id> <text>`, so that I correct
  an estimate without curl.
- **US-9** `[shipped]` As a developer, I want `worklog block delete <id>`
  to confirm first and report whether a Tempo entry was also removed, so
  that I never silently orphan a remote worklog.
- **US-10** `[shipped]` As a developer, I want every edit on an
  already-synced block to tell me "now dirty — run `worklog sync`", so
  that Tempo never silently drifts from local state.
- **US-11** `[shipped]` As a developer, I want `worklog block split <id>
  <first-minutes>` to break one block in two, so that I can separate two
  tasks the clusterer lumped together.
- **US-12** `[later]` As a developer, I want `worklog block edit <id>` to
  open an `$EDITOR` form (ticket / duration / description), so that I can
  fix several fields at once.

### Merging — "one task, one block"

- **US-13** `[shipped]` As a developer, I want `worklog block merge <a>
  <b> …` to fold blocks into the first one, summing logged time and
  keeping the primary's ticket/description, so that a task fragmented
  across commits + prompts + a meeting becomes one worklog line.
- **US-14** `[shipped]` As a developer, I want merge to **refuse** when an
  absorbed block is already synced (so its Tempo entry can't be orphaned)
  and to mark a synced *primary* dirty, so that the data-integrity
  invariants hold automatically.
- **US-15** `[shipped]` As a developer, I want `worklog block merge --auto`
  to merge every adjacent same-ticket block on a day in one shot, so that
  cleanup is one command.

### Daily flow — "least friction"

- **US-16** `[shipped]` As a developer, I want `worklog day` to end by
  printing the `worklog today` summary instead of silently finishing, so
  that the pipeline always leaves me with a readable result.
- **US-17** `[next]` As a developer, I want an interactive `worklog review`
  TUI that walks unassigned blocks one at a time with a ticket picker, so
  that triage is a guided flow, not a hunt for ids.
- **US-18** `[later]` As a developer, I want a single `worklog` (no
  subcommand) to run collect → infer → estimate → print summary, so that
  the default verb is the daily verb.

### Onboarding & discoverability

- **US-19** `[shipped]` As a new user, I want `worklog --help` grouped by
  *workflow* (daily / collection / review / setup), so that I can find the
  command I need.
- **US-20** `[shipped]` As a new user, I want `worklog status` — one
  screen: daemon up?, hook installed?, schedule running?, web up?,
  database, secrets, today's totals — so that I have a single
  health-and-state dashboard instead of five separate `*-status`
  commands.

## Other ideas worth considering

- ~~Collapse the `*-status` commands into one `worklog status`.~~
  **Shipped in Phase 3.** `hook status`, `schedule status`, `daemon
  status`, `web status` and `secret list` still exist, but `worklog
  status` is now the one screen that answers "is worklog healthy?".
- ~~`worklog day` should not *require* Docker.~~ **Shipped in Phase 3** —
  the terminal summary is the default tail; `--serve` is opt-in.
- **Friendlier daemon failures.** Every daemon-backed command should, on a
  connection refusal, say `the worklog daemon isn't running — start it
  with 'worklog daemon install'` rather than a raw reqwest error.
  (`worklog block` / `summary` already call `ensure_daemon_running`, which
  installs + starts it transparently.)
- **`worklog undo`.** A short edit journal so a fat-fingered delete or
  merge is recoverable without re-inference.
- **Shell completions.** `worklog completions zsh|bash|fish` — clap
  generates these for free; ticket-key completion could read the cached
  `jira_tickets` table.
- **Natural-language entry point.** The bundled Claude Code skill already
  lets Claude operate worklog. A `worklog ask` that pipes a question
  through the configured estimator provider would give the same affordance
  in a bare terminal.

## Architecture notes

The CLI ↔ daemon split is sound — the daemon owns the one writable SQLite
connection, which is what keeps the Docker'd web UI and the CLI consistent.
The fix was never to *remove* the daemon; it was to stop making the user
**be** the HTTP client. `worklog block` / `worklog summary` now speak to
the daemon through `worklog-cli::daemon_client`, and `ensure_daemon_running`
means the user never thinks about whether it's up.

All block mutations still funnel through `worklog-core::block_service`, so
the CLI, the daemon HTTP API, and the web UI share one implementation of
every invariant (the `tempo_worklog_id` canary, `estimated_by = 'manual'`
protection, the dirty flag). `merge_blocks` was added there, not in the
CLI, for exactly this reason.

## Changelog — Phase 1 (shipped)

- **`worklog summary` / `worklog today`** — terminal day summary: total
  hours, per-ticket breakdown, work/personal split, and next-action hints.
  `--json` for scripting.
- **`worklog block`** — `list`, `assign` (`--clear`), `duration`,
  `describe`, `delete`, `merge`. All talk to the daemon over HTTP via the
  new `daemon_client` module — no `curl` required. Destructive commands
  (`delete`, `merge`) confirm first; `--yes` skips for scripts.
- **Block merging** — `block_service::merge_blocks` + the daemon
  `POST /blocks/merge` endpoint. Sums logged time, keeps the primary's
  identity, re-links events, and enforces the Tempo-orphan and dirty-flag
  invariants. Covered by unit + endpoint tests.
- **Grouped `--help`** — root help is now organised by workflow area.

## Changelog — Phase 2 (shipped)

- **`worklog week`** — rolls the 7 days ending on `--day` into a per-day
  table (total / work / personal / blocks) plus a per-ticket weekly
  roll-up and unsynced/unassigned hints. `--json` for scripting.
- **`worklog day` ends with the summary** — the pipeline used to finish
  silently after estimation. It now prints the same per-ticket summary
  `worklog summary` shows, re-read post-estimate, before serving (or
  instead of serving under `--no-serve`). `--json --no-serve` keeps its
  existing machine-readable shape.
- **Shared aggregation** — `summary`, `day`'s tail and each `week` row run
  through one pure `aggregate_day` function, so the three surfaces can
  never disagree on a day's numbers.
- **`worklog block split <id> <first-minutes>`** — the inverse of `merge`:
  splits a block in two, re-buckets events by timestamp, keeps the
  original synced (marked dirty) and makes the tail a fresh unsynced
  block. `block_service::split_block` + `POST /blocks/:id/split`, with
  unit + endpoint tests.

## Changelog — Phase 3 (shipped)

- **`worklog block merge --auto`** — collapses every run of adjacent
  same-ticket blocks on a day, reusing the estimator's existing merge
  pass (which safe-skips synced and manually-edited blocks). New
  `POST /blocks/auto-merge` endpoint.
- **`worklog status`** — one health-and-state screen: daemon, hook,
  schedule, web UI, database and secrets, plus today's tracked time.
  Replaces hunting through five separate `*-status` commands. `--json`
  for scripting/monitoring.
- **`worklog day` no longer requires Docker** — the pipeline finishes in
  the terminal with the day summary by default; the dockerised web UI is
  opt-in via `--serve`. `--no-serve` is kept as a deprecated no-op so old
  scripts don't break, and `worklog day --json` now emits JSON without
  needing `--no-serve` (the old invariant-7 footgun is gone).

## Changelog — Phase 4 (in progress)

- **`worklog completions <shell>`** — emits a bash / zsh / fish / etc.
  completion script, derived from the clap grammar so it stays correct
  as commands change.

## Changelog — Phase 5 (correctness)

Bug fixes for two issues that made the per-ticket hours wrong:

- **Phantom tickets like `CRIT-1` are rejected.** The estimator accepted
  any `KEY-N`-shaped token found in event text as a Jira ticket — so
  severity tags (`CRIT-1`, `HIGH-2`), version strings (`UTF-8`, `GPT-4`,
  `SHA-256`) all became tickets. `validate_ticket` now trusts a literal
  only when its project prefix matches a real cached Jira project. A
  genuine but un-cached ticket (closed / freshly filed) still passes;
  invented prefixes never do.
- **Overlapping blocks no longer double-bill.** Blocks can overlap in
  wall-clock time (project-aware splitting puts concurrent work on two
  blocks; the estimator sets durations independently of `ended_at`). All
  time figures in `summary` / `week` / `day` / `status` are now the
  **union** of the underlying intervals — overlap counted once — so the
  hours reported are real hours. `summary` and `week` report how much
  overlap was de-duplicated; `worklog block list` names the overlapping
  block pairs so they can be merged / split / deleted.

## Roadmap — Phase 6 (remaining)

These are the remaining `[later]` user stories — none are blocking:

- `worklog block split <id> <at>` accepting a clock time, not just
  first-minutes (US-11 refinement).
- `worklog block edit <id>` — open an `$EDITOR` form (US-12).
- `worklog review` — interactive triage TUI (US-17).
- bare `worklog` runs the daily pipeline (US-18).
- `worklog ask "…"` — natural-language queries (US-5).
- `worklog undo` — an edit journal so a fat-fingered delete/merge/split
  is recoverable.
- auto-resolve overlaps — proportionally clip overlapping blocks so the
  data sent to Tempo (not just the display) is non-overlapping.
