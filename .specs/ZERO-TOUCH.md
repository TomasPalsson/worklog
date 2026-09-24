# Zero-touch day — design (owner goal, 2026-09-23)

> "The entire point is that I don't have to go through my messages or history."

Slack messages and Firefox tabs are **evidence**, not items to sort. The owner should never see
them unless they ask. The day page answers "what did I do, for whom, how long" with zero clicks.

## Pipeline (every 15 min, unattended)
1. **Collect** everything (claude hook, github, slack, firefox add-on, shell, reflog).
2. **Route**, cheapest proof first: owner rule → names-the-repo link → Slack time context →
   Verdict (strict ratios).
3. **Absorb (new):** any still-unlabelled Slack/Firefox event that falls *inside* an interval of
   other-source work activity (claude/shell/git_reflog/github within ±2 min) joins that project,
   origin `context`. Billing cannot change: the time is already inside a block.
4. **Noise (new):** whatever is left is auto-labelled `noise` — hidden, never billed, never a
   to-do. (After hours DMs, lunch news, a legal site.)
5. **Blocks** are rebuilt (manual edits, descriptions, tempo ids are carried over as today).
6. **Estimate (now automatic):** `worklog day` runs the existing `claude -p` estimator on
   un-estimated blocks. It already reads every event linked to a block, so Slack/Firefox/shell
   clues shape the ticket + description ("Review vitinn-infra PR #802 and #811").

Schedule change: the launchd/systemd job runs `worklog day` instead of `worklog collect all`.

## Day page
- Day strip (blocks by project, gaps hatched: Lunch / Away) at the top.
- Blocks with their auto description, ticket, minutes, confidence, and a clue line
  ("12 shell · 3 Slack · 2 web") that expands on demand.
- Nothing to sort by default. A single quiet line: "31 clues used · 6 ignored as noise — review".
- Only real exceptions get attention: a block with no ticket, a low-confidence block.
- The grouped tray with "Not work" / "always" survives only as the review drawer for overrides.

## Status (2026-09-23, loop branch = PR #46)
- Built + live on http://localhost:3333 (installed ~/.local/bin/worklog, schedule runs `worklog day`):
  absorb/noise (84abefd), day collects all 6 sources (83e4f4f), zero-ask page (fd9e317),
  strip colours/labels/legend (31b381d, b170a92, 546f96c), no-ticket banner removed (665f8cb),
  review drawer shows message/URL gist + reason (203fe14).
- Real day: 0 to sort (4 link, 6 context, 21 noise); 15/18 blocks described by Claude.
- In flight: interactive strip (lanes expand, legend focus, tooltip w/ description) — developer.
- Next: rebuild web (`worklog web down; WORKLOG_WEB_DIR=<loop>/web worklog web up`), screenshot
  bar + lanes, run gates.sh, push.

## Status 2 (2026-09-24)
- Missing vitinn-infra time fixed: claude_transcripts collector (typed prompts = claude_turn,
  Claude busy = claude_work 1/min, no text), reflog reads worktrees+submodules, day summary
  `project` folds worktrees, submodule→repo map, infer_lanes: one owner per minute (focus follows
  latest human action ≤15 min, work before personal, background fills idle) — b40dc92.
- In flight: owner-adjustable overlap split (overlaps on /days/:day, overlap_allocations table,
  POST /days/:day/allocations, lanes-view amber bands + popover slider) — developer.
- Verify after: gates.sh, then ~/.claude/jobs/aa6acbea/tmp/vitinn_e2e.sh (install + worklog day +
  per-project minutes), screenshot lanes view.

## Guardrails
- Nothing leaves the machine except the estimator's existing `claude -p` call (already in use).
- Noise is reversible: the review drawer can un-hide and file any event.
- Owner overrides (`fix`, manual block edits) always win.
