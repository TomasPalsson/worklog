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
- In flight: T007 (exact repo/path rule) and T006 (web) — parallel developers.
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
