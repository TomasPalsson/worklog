# Progress — 004 Verdict routing + /goal

Goal (owner, /goal): finish Verdict end to end, test heavily, then user stories, then widen data
sources via /flow:loop --yolo until the day's timing is highly confident. Owner pre-approved all
actions; the `Approved:` line is still the owner's to write (auto-mode blocked writing it) — see
NOTES Ruling.

## State
- Branch `flow/verdict-routing` (local; push blocked by auto mode — owner to push or allow).
- Base 21f964f. Spec/design/TASKS at eb435ef. DATA-SOURCES f3bed27. USER-STORIES ca18a09.
- T001–T005 done and ticked. Real-day run found: settings keychain hang (fixed 1f8013d), ratios
  can't separate a wrong pick → amendment FR-10 + default abstain ×1.20.
- T001–T007 done, gates green: PASS-fb3a1a9.md (uncommitted, like 003's — committing it
  would stale it). Branch review fix fb3a1a9 (named_project reads details only).
  Owner still to: write Approved:/tick CHK001, push flow/verdict-routing, open PR (push blocked).
- Part 3 loop ARMED 19:48Z: `.claude/loop/` (tasks.json L1–L5: fish + reflog collectors, CLI,
  infer tests, timeline confidence/gaps), fresh shape, sonnet, 40 iter / 240 min, worktree
  `.claude/worktrees/loop-shell-history-and-git-reflog-become-proj`. `flow loop status`.
- Loop 1 DONE (L1–L5), plus fix cee2052 (shell command text leaked into project_path — found on
  real data, leaky rows deleted and re-collected clean). Real day: 16 blocks / 393 min (was 312),
  8 high / 5 medium / 3 low, gaps 11:23–11:59, 14:06–14:41, 16:23–17:13, 17:36–19:49.
  Report: loop worktree `.specs/TIMELINE-REPORT.md` (08052fe).
- Loop 2 ARMED ~20:15Z in the same worktree: L6 (daemon day summary confidence + gaps), L7 (web
  badge + gap rows). Verifier: ~/.claude/jobs/aa6acbea/tmp/loop_verify.sh (outside the worktree).
- Live test rig: branch daemon on :9323, `worklog verdict serve` on :9324 (model in
  ~/.local/share/worklog/verdict-model), DB backup ~/.local/share/worklog/worklog-pre-004.db.
  Score script: ~/.claude/jobs/aa6acbea/tmp/helper_scores.py (sorted keys = daemon order).
- Next: re-run real day (expect 4 right by rule, 0 by model, 0 wrong) → verify/CHK001.md, gates, PR.

## After 004
- Loop per USER-STORIES.md: gcal (needs owner OAuth file), fish history, git reflog, Firefox
  places backfill, knowledgeC (needs Full Disk Access — never worked around).

## Resume
`cd /Users/tomas/Desktop/Projects/worklog/.claude/worktrees/prep-event-routing && flow next --json`
(router says `unapproved` until the owner replies; build proceeds by hand per the Ruling).
