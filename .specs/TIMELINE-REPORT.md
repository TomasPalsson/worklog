# Timeline report — 2026-09-23 (loop branch)

Sources collected into the real DB tonight: claude (hook), slack, firefox (add-on, from 15:44),
github_pr, **shell** (new, fish history), **git_reflog** (new). Routing by spec 004 (rule for exact
repo/path mentions, Verdict for the rest).

## Result
- Before (claude/slack/github only): 14 blocks, 312 min.
- After shell + git_reflog: **16 blocks, 393 min**, 08:59–20:07 span 668 min → 59% covered.
- Confidence (distinct sources per block, timeline.rs thresholds): **8 high, 5 medium, 3 low**.
- Gaps ≥ 30 min: 11:23–11:59, 14:06–14:41, 16:23–17:13, 17:36–19:49.

| block | min | conf | project | sources |
|---|---|---|---|---|
| 08:59–10:08 | 68 | medium | lyfjastofnun (worktree) | claude, shell |
| 10:45–11:23 | 38 | high | vitinn-infra | claude, git_reflog, slack |
| 13:35–14:06 | 31 | high | vitinn-infra | claude, git_reflog, shell |
| 15:11–15:28 | 16 | high | vitinn-infra | claude, git_reflog, shell, slack |
| 15:49–16:08 | 19 | high | genai-infra | claude, git_reflog, shell |
| … 11 more | | | | (full list: `python3 day_report.py`) |

## Found and fixed on real data
- fish collector stored command text in `project_path` (`…/vitinn-infra\ngit switch feat`,
  `…/LibreChat && claude --resume <id>`). Fixed in cee2052 (segment split, root-only path);
  the 64 leaky rows were deleted and re-collected clean (0 rows with command text).

## What the gaps need (owner action — never worked around)
1. **Google Calendar** — meetings are the likeliest gap filler; collector exists, needs
   `~/.config/worklog/google_credentials.json` + `worklog collect gcal --auth`.
2. **Full Disk Access** for the terminal → knowledgeC.db per-app focus intervals.
3. Firefox add-on now covers browsing from 15:44 on; future days fill without action.

## Loop 2 (L6–L7) — day page
- `GET /days/:day` now returns per-block `confidence` and the day's `gaps` (252c1d5); the day page
  shows a confidence badge per block and "No activity HH:MM–HH:MM (N min)" rows (2969486).
- Checked on the real day: API gaps 35/34/49/133 min and 8 high / 5 medium / 3 low — identical to an
  independent script. Screenshot: `timeline-day-page.jpeg`.

## Branch review (fb3a1a9..43e4c4d, 15 agents, 7 dropped)
- Kept (95/100): `AWS_SECRET_ACCESS_KEY=… aws …` stored the secret as the shell title. Fixed in
  7612edf (env assignments and wrappers skipped, basename only, else `shell`). Real DB re-collected:
  85 rows, 0 unsafe titles, 0 details, 0 bad paths. No secret had been stored (the 2 odd titles were
  script paths).

## Gates at 7612edf
fmt 0 · clippy 0 · cargo test 636 passed / 0 failed · web 117 pass · typecheck 0 · build 0 ·
verdict self-test OK · extension tests 0.

## Open
- fish.rs is 517 lines and daemon.rs ~4300 vs the 400-line size guard — split in their own change.
- Gap rows render as a list above the blocks, not interleaved between them (cosmetic).
