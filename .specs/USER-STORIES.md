# User stories — the ideal day

The owner does nothing but look. Every story names the data it rests on and a check a
stranger can run. Data sources: see DATA-SOURCES.md. "New" = not collected yet.

## S1 — Everything is ready when I open the site
As the owner, when I open the day page at the end of the day, every source has already been
collected, blocks are built, routed and estimated — I press no buttons.
- Data: scheduled collection (schedule.rs, exists) for claude, github, slack, gcal, firefox, fish, git reflog.
- Check: at 17:00 local with no manual action, `GET /days/<today>` lists blocks covering ≥ 90% of the
  minutes between my first and last activity; "Last collected" is < 15 min old.

## S2 — Meetings are anchors
As the owner, my calendar meetings appear as fixed blocks with their real start/end, and other
activity during a meeting is folded into it, not billed twice.
- Data: gcal (collector exists, never run).
- Check: a 30-min meeting yields exactly one 30-min block; overlapping Slack/Firefox events inside it
  add 0 extra minutes.

## S3 — Terminal work is attributed to the right project
As the owner, time I spend running commands in a repo counts toward that repo's project, even when
no Claude session or commit happened.
- Data: New — fish history (`when` epoch + command; project from `cd`/paths), git reflog per repo.
- Check: 20 min of commands inside ~/Desktop/Work/vitinn-infra with no other events yields a
  vitinn-infra block of 15–25 min.

## S4 — Branch switches mark project changes
As the owner, when I switch repos or branches, the block changes project at that minute.
- Data: New — git reflog (`.git/logs/HEAD`) across ~/Desktop/Work and ~/Desktop/Projects.
- Check: a checkout in repo B at 10:14 ends repo A's block at ≤ 10:15 and starts B's.

## S5 — Browser time is sorted without me
As the owner, work tabs are filed to their project by rule or by Verdict; personal tabs never appear.
- Data: firefox add-on (exists), Firefox places.sqlite backfill (New) for gaps before install,
  Verdict (spec 004).
- Check: a github.com/<org>/<repo> tab is filed to <repo>; a tab in the Personal container or a
  non-work domain yields no event.

## S6 — Slack conversations land on the project they are about
As the owner, messages that link a repo, PR, ticket or path are filed to it; small talk stays out of
billed time.
- Data: slack (exists), Verdict, Jira keys (exists).
- Check: a message with a PR link is filed; "Skoða" alone stays unsorted and adds 0 billed minutes.

## S7 — Every minute has a reason
As the owner, each block shows which events produced it (e.g. "12 shell commands, 2 commits,
1 Claude session"), so I trust it without opening raw data.
- Data: all of the above, via block ↔ event links (infer.rs).
- Check: every block lists ≥ 1 source badge and a count per source.

## S8 — Gaps are called out, not invented
As the owner, stretches with no signal are shown as gaps with their length, never silently billed.
- Data: the union of all sources.
- Check: a 45-min window with no events renders as a 45-min gap and adds 0 billed minutes.

## S9 — Confidence per block
As the owner, each block says how sure the timeline is (e.g. "3 independent sources agree"),
so I review only the shaky ones.
- Data: count of distinct sources overlapping the block.
- Check: a block backed by claude + fish + git shows "high"; a block backed only by one Firefox tab
  shows "low".

## S10 — Invoice lines are ready
As the owner, the billing export for the day is filled from the blocks with no invented customer or
project and no double-billed overlap.
- Data: blocks + billing registry (exists).
- Check: the export's hours equal the union of the day's work-block intervals rounded to 0.5 h.

## S11 — Private stays private
As the owner, nothing from a personal tab, DM text or a private site leaves my Mac, and personal
blocks are excluded from billing.
- Data: personal.rs (exists), privacy filters (exists), Verdict local-only.
- Check: grep of every outbound request body in tests finds no DM text; personal blocks have 0
  billed minutes.

## S12 — App focus fills the rest (needs Full Disk Access)
As the owner, if I grant Full Disk Access once, per-app focus time (IDE, terminal, browser) closes the
remaining gaps to the minute.
- Data: New — knowledgeC.db. Opt-in only; never worked around.
- Check: with access granted, ≥ 95% of active minutes are covered; without it, the app says what it
  would add.
