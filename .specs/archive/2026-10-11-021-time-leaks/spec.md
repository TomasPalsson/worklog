# Spec: Time leaks

**Created**: 2026-10-11 · **Route**: dispatch

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: Worklog catches the Owner's time, then loses it quietly. Ignored blocks show as "personal". A failed 17:00 send surfaces only the next day. A broken collector or a dead daemon looks like "nothing happened". About 9.5 h of ticketed work since 22 Sep never reached Tempo or an invoice.

**Solution**: Every number adds up. Every failure says what failed, the same day. Ignore says why. One list shows everything not yet sent or exported. Small daily chores (deild picking, install, focus check, disk cleanup) get shorter.

**Who it's for**: The Owner (sole user, logs to Tempo and invoices customers).

**MVP cut line**: everything tagged `MUST` in §4.1 ships. `SHOULD` is v1.1. `MAY` is backlog.

**Key decision**: Ignore keeps setting the hidden personal flag (10+ readers rely on it); the readers that show totals learn to count ignored time as its own bucket instead.

## 1. Context

### 1.1 Problem statement

Source: brainstorm 2026-10-10 (`~/.brainstorm/brainstorm-worklog-2026-10-10.html`), fact-checked against code at 7b9cbb7 and the Owner's database. Week 4–10 Oct: 29 of 86 blocks ignored (≈868 min); `worklog week` shows them as 13h 36m "personal". The auto-send stops at the first failing ticket and shows "not sent" only after the day ends. Lines Tempo skips are recorded as sent. Several collectors and status probes turn errors into silence.

**Current workaround**: The Owner notices gaps by eye, types "daemon crashed fix it please" into Claude, re-picks the same deild by hand (73 edits in 4 days), and types "merge and install".

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Owner | Reviews blocks, sends to Tempo, exports invoice lines | Sole user; works in ~5 project folders a day |
| Scheduler | The 15-minute collect job and the daemon's 17:00 auto-send | Runs unattended |

**Primary actor**: Owner.
**Hidden stakeholders**: Customers on invoices (must never see invented data); teammates reading the Slack standup (must not see personal telemetry).

## 2. Scope

### 2.1 In scope

- Ignored time as its own bucket in the terminal day/week tables
- A reason on ignore (noise, duplicate) and shortcuts to merge or assign instead
- Same-day, specific send failures; skipped lines no longer counted as sent
- A loose-ends list for the current billing cycle
- Day page and `worklog status` say what failed instead of showing empty or "not installed"
- Per-source collect health in `worklog status`
- GitHub collect fetches every result page
- Daemon/schedule install and `web down` report real failures
- Deild suggestion chip from the Owner's own past picks
- `/wl install` in the Claude Code mod
- A focus line in the local standup draft
- Hide `serve` and `eval`; fix the README command table
- Archive finished spec folders; remove clean merged worktrees

### 2.2 Non-goals

- No Friday auto-close of the week (Owner, 2026-10-11).
- No change to half-hour rounding or the 15-minute floor (it changes invoices); tiny-block folding already happens.
- No automatic rules learned from ignore reasons in v1.
- No new `--fix` flag on `worklog status`; no change to `collect`'s exit code or JSON shape.
- No rewrite of existing rows (no backfill of `is_personal` on ignored blocks).
- The focus line never goes into the Slack post.
- `/wl install` never merges a pull request.
- Worktrees with uncommitted changes, locked worktrees, and worktrees changed in the last 48 h are never removed.
- `doctor` stays; spec 020 and its folder are untouched.

## 3. Journeys

### Journey 1 — Honest week (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | A week with 2h work, 1h ignored, 0.5h personal, no overlap | Owner runs `worklog week` | Columns show work 2h, personal 30m, ignored 1h, total 3h 30m |
| Error | A work block overlaps an ignored block by 1h | Owner runs `worklog week` | Totals still add up; the overlap shows as its own line |
| Edge | A day with only ignored blocks | Owner runs `worklog day` summary | Work 0m, personal 0m, ignored shown |

### Journey 2 — Ignore with a reason (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | An unsynced block | Owner picks "Noise" | Block is ignored and its reason reads noise |
| Error | A block already in Tempo | Owner opens the ignore menu | Menu is disabled with the existing "already synced" reason |
| Edge | "Duplicate" on a block with no same-day neighbour | Owner picks it | Block is ignored with reason duplicate (no merge) |

### Journey 3 — Failed send (Scheduler, Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | Tempo accepts all lines at 17:00 | Auto-send runs | Lines are recorded as sent |
| Error | Ticket A's send errors, ticket B's would succeed | Auto-send runs | A shows "Not sent: <error>" in Review today; B is still sent |
| Edge | Tempo skips a line (no ticket, rounds to 0) | Auto-send runs | Line shows "Not sent: <reason>", not sent |

### Journey 4 — Loose ends (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | 3 ticketed blocks this cycle lack a Tempo id and an export date | Owner opens Loose ends | 3 rows, each naming what is missing |
| Error | The daemon is down | Owner opens Loose ends | A notice says the list could not load |
| Edge | A block is ignored or personal | Owner opens Loose ends | It is not listed |

## 4. Requirements

### 4.1 Functional requirements

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | The terminal day and week tables MUST show ignored time as its own column, never inside personal | Unit test on the aggregate: ignored block lands in ignored, not personal |
| FR-02 | MUST | For every day, total = the union of all block intervals, and overlap = (work + personal + ignored) − total; the tables MUST print overlap as its own line `overlap <Hh MMm>` when it is above 0 | Unit test with overlapping work and ignored blocks |
| FR-03 | MUST | Reclassify MUST leave ignored blocks unchanged | Unit test: reclassify on an ignored work-path block keeps it ignored and personal-flagged |
| FR-04 | MUST | The Owner MUST be able to ignore a block with reason noise or duplicate | Daemon test stores and returns each reason |
| FR-05 | MUST | A rebuild of the day MUST keep an ignored block's reason | Infer test: carried block keeps the reason |
| FR-06a | MUST | The ignore menu MUST offer Noise, which ignores with reason noise | Web component test |
| FR-06b | MUST | The ignore menu MUST offer Duplicate, which merges into the nearest same-day block on the same ticket, or ignores with reason duplicate when there is none | Web component test, both branches |
| FR-06c | MUST | The ignore menu MUST offer Internal, which opens the ticket picker; assigning a ticket does not ignore the block | Web component test |
| FR-07 | MUST | An existing caller that sends no reason MUST keep working, with the reason left empty | Daemon test with the old body |
| FR-08 | MUST | When one ticket's auto-send fails, the Scheduler MUST store that ticket's error text and continue with the next ticket | Auto-send test: first send errors, second is sent |
| FR-09 | MUST | A line the Tempo sync skips because it has no ticket, or because it rounds to 0 h, MUST be stored as not sent with the sync result's reason text; a line skipped as already in Tempo stays sent | Auto-send test for each of the three skip kinds |
| FR-10 | MUST | The Review list (the day page's list of sent and not-sent Tempo lines) MUST include today's not-sent lines the same day | Test: a failed line for today appears in review lines |
| FR-11 | MUST | The loose-ends list MUST show ticketed, non-personal, non-ignored blocks in the current billing cycle that lack a Tempo id or an export date, naming which is missing | Core test with each combination |
| FR-12 | SHOULD | Each loose-ends row SHOULD link to its day page | Web test |
| FR-13 | MUST | When any of these fails to load — unsorted events, folder list, elsewhere items, Tempo lines, Verdict status, review list, recap — the day page MUST name it in one notice and still render the others | Web test: one failing call shows its name; others render |
| FR-14 | MUST | `worklog status` MUST show "check failed (<reason>)" when a probe errors, not "not installed" | Unit test on the status mapping |
| FR-15 | MUST | When the daemon, schedule or web server is down, `worklog status` MUST print `worklog daemon install`, `worklog schedule install` or `worklog web up` respectively | Unit test on the hint text |
| FR-16 | MUST | Each collector run MUST record its last success time (meta key `collect.<source>.last_ok`) or last error time and text (meta key `collect.<source>.last_err`) | Test: an Ok and an Err run each leave their record |
| FR-17 | MUST | `worklog status` MUST list each source with last success, last error and event count in the last 24 h | Unit test on the rows |
| FR-18 | MUST | GitHub collect MUST fetch every result page, stopping at the search cap | Mock test with 2 pages |
| FR-19a | MUST | Daemon and schedule install MUST fail with the service tool's error text when its load or enable step exits non-zero | Inline tests on the result mapping |
| FR-19b | MUST | Daemon install MUST fail when the daemon does not answer its health check within 5 s | Inline test |
| FR-20 | MUST | `worklog web down` MUST remove its pid file only when the process no longer exists 2 s after the forced kill; otherwise it fails and keeps the file | Inline test |
| FR-21 | SHOULD | A block with a customer and no deild SHOULD show, as a chip, the deild the Owner saved most often for that customer across all saved split rows (ties: the most recent); clicking saves it through the existing split-row path, and nothing is saved without the click | Core test on the query incl. a tie; web test on the chip |
| FR-22 | SHOULD | `/wl install` SHOULD run the local install of origin/main from inside the worklog repo and show its result; outside the repo it refuses | Mod test with a stubbed process |
| FR-23 | SHOULD | The local standup draft SHOULD show "yesterday: N switches, longest focus <duration> on <ticket> (<start>–<end>)" as a separate field, never in the posted text. Over yesterday's non-personal, non-ignored blocks sorted by start: a switch is a block whose ticket differs from the previous block's (no ticket is its own value); a focus run is consecutive blocks on one ticket with gaps of 15 min or less, its duration the sum of their seconds; blocks with no ticket never count as focus | Core test on the summary incl. a gap of 16 min; test the post body excludes it |
| FR-24 | MUST | `worklog --help` MUST NOT list `serve` or `eval`; both still run | CLI test parses both |
| FR-25 | MUST | The README command table MUST match `worklog --help` and state `upgrade` as the alias of `self-update` | Reviewer diff against `--help` |
| FR-26 | SHOULD | Finished spec folders 003, 004, 007, 008 SHOULD move to `.specs/archive/` with one LEDGER row each | `ls .specs` |
| FR-27 | SHOULD | A worktree under `.claude/worktrees` SHOULD be removed only when it has no uncommitted changes, is not locked, was not modified in the last 48 h, and every commit on it is on origin/main by ancestry or by an identical squash-merge subject; at spec time these are billed-hours-header, settings-z-index, task-board-5-columns, task-board-wide, ticket-create-assign-me | `git worktree list` before and after; each removal re-checks the rule |

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| Loose-ends query | < 200 ms on the Owner's database (≈300 blocks/month) | Timed in the core test on a 2,000-block fixture, release build |
| GitHub paging | ≤ 10 pages per search (the 1,000-result search cap) | Mock test asserts the stop |
| Status probes | `worklog status` returns in < 3 s with the daemon down | Timed run with the daemon stopped |
| Failure visibility | 0 collector, send or day-page failures end with no stored or shown message | Every error path in §3 has a test asserting the message |
| Privacy | 0 focus or personal-time data in any Slack post | Test asserts the post body |

## 6. Launch criteria

- [ ] Every MUST in §4.1 has a passing test
- [ ] `cargo test`, `cargo clippy -D warnings`, `cargo fmt --check`, `bun test`, `bun run typecheck`, `bun run build` pass
- [ ] The error path of each journey is exercised by a test
- [ ] Owner checks the day page: ignore menu, load notice, loose-ends list, deild chip
- [ ] `worklog week` for 4–10 Oct adds up (work + personal + ignored − overlap = total)

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | Billing cycle uses the code's configured start/close days (default start 20) | Med | Loose-ends window is off by days; config-only fix |
| A2 | No reader depends on ignored blocks being inside the CLI personal total | High | A script parsing `worklog week --json` sees a new key |
| A3 | Same-day neighbour for Duplicate = the nearest same-day block on the same ticket | Med | Merge picks a surprising block; Owner can undo |
| A4 | The 5 clean worktrees have no unpushed value (squash-merged subjects on main) | High | Lost local branch tip; branches stay, only the folders go |
| A5 | Line references in this spec are at 7b9cbb7; Base is 5d765ac; builders locate by symbol | High | Builder searches a few lines away |

## 8. Open questions

None.

## Appendix A — Glossary

| Term | Means |
|------|-------|
| block | A chunk of the Owner's time grouped from events, with an optional ticket |
| ignore | Hiding a block from timesheet and invoice |
| loose end | A ticketed work block in the current billing cycle with no Tempo id or no export date |
| deild | A customer's department on an invoice line |
| billing cycle | The period between the configured start day and the next one |
