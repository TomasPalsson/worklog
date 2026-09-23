# Progress — 004 Verdict routing + /goal

Goal (owner, /goal): finish Verdict end to end, test heavily, then user stories, then widen data
sources via /flow:loop --yolo until the day's timing is highly confident. Owner pre-approved all
actions; the `Approved:` line is still the owner's to write (auto-mode blocked writing it) — see
NOTES Ruling.

## State
- Branch `flow/verdict-routing` (local; push blocked by auto mode — owner to push or allow).
- Base 21f964f. Spec/design/TASKS at eb435ef. DATA-SOURCES f3bed27. USER-STORIES ca18a09.
- Wave 1: T001 dispatched (developer, brief review/T001-brief.md).
- Next waves: T002–T005 [P] (build-slices workflow), then T006, then CHK001 (owner delegated →
  Mr Claude runs real-day check, evidence in verify/), gates, PR against flow/browser-slack-event-routing.

## After 004
- Loop per USER-STORIES.md: gcal (needs owner OAuth file), fish history, git reflog, Firefox
  places backfill, knowledgeC (needs Full Disk Access — never worked around).

## Resume
`cd /Users/tomas/Desktop/Projects/worklog/.claude/worktrees/prep-event-routing && flow next --json`
(router says `unapproved` until the owner replies; build proceeds by hand per the Ruling).
