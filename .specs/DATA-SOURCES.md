# Timeline data sources — inventory 2026-09-23

Read-only survey for the "whole day reconstructed" goal (after 004 ships). Counts only; no content.

## Already collected (worklog.db, 2026-09-23)

| source | today | all-time | note |
|---|---|---|---|
| claude | 81 | 274,643 | Claude Code hook events, per repo dir |
| github_pr | 7 | 186 | |
| github_commit | 0 | 150 | last 2026-09-22 |
| slack | 13 | 64 | sent messages only |
| firefox | 14 | 14 | add-on heartbeats since 15:44 |
| gcal | 0 | 0 | collector exists, never run — meetings missing |

## Not collected yet

| source | path | readable | timestamps | today | value |
|---|---|---|---|---|---|
| fish shell history | ~/.local/share/fish/fish_history | yes | `when:` epoch per command | yes (4,641 total) | high |
| git reflog + commits | `.git/logs/HEAD` in 58 repos under ~/Desktop/Work, ~/Desktop/Projects | yes | epoch per line | 7 repos touched | high |
| Firefox full history | Firefox profile places.sqlite | yes | `visit_date` µs | 695 visits | high (backfill before add-on, but sensitive) |
| Claude transcripts | ~/.claude/projects/*/*.jsonl | yes | ISO `timestamp` | 35 files | high (already via hook) |
| knowledgeC / Screen Time | ~/Library/Application Support/Knowledge/knowledgeC.db | no — needs Full Disk Access | per-app focus intervals | ? | very high |
| JetBrains / VS Code | app support dirs | yes | varies | ~0 today | low-med |
| Safari | History.db | no (TCC) | | ? | med |
| Chrome | Default/History | yes | | 0 (stale since 09-02) | low |

## Picks, in order

1. Run the existing gcal collector (meeting boundaries, zero new code).
2. fish history collector (per-command time + cwd → project).
3. git reflog collector over the Work/Projects roots (branch checkouts = which project, when).
4. Firefox places.sqlite backfill with the same privacy filters as the add-on.
5. knowledgeC.db only if the owner grants Full Disk Access — never worked around.
