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

## Not done yet (next loop)
- Wire `timeline::block_confidence` / `day_gaps` into `GET /days/:day` and show them on the day
  page (S7–S9 in the UI). The helpers exist and are tested; the day page does not use them yet.
