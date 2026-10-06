# Spec: Verdict does more

**Created**: 2026-10-06 · **Route**: dispatch · **Design**: design.md

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: Verdict, the local model that files loose events into projects, only runs if the Owner starts it in a terminal, fails silently when off, picks from every project folder (worse accuracy), forgets every correction, and does nothing about tickets or Tempo text, which the Owner still fixes by hand.

**Solution**: Verdict starts with worklog and is switched on/off from Settings; the day page says when it is not checking. It picks from a short list, learns from past fixes as examples, offers one-tap choices when unsure, picks tickets when clearly sure, checks Tempo text before it is sent, and a nightly scorecard tunes how sure it must be. At 17:00 the lines it is sure about go to Tempo, and a Review section lists earlier days' sent lines so the Owner can confirm them or fix what went in.

**Who it's for**: the Owner, who sorts their own day and logs it to Tempo.

**MVP cut line**: everything tagged `MUST` in §4.1 ships. `SHOULD` is v1.1. `MAY` is backlog.

**Key decision**: Verdict acts alone only where it can be measured. Every guess and every correction goes into a permanent decision log, and the thresholds come from replaying that log, not from intuition.

## 1. Context

### 1.1 Problem statement

After a restart Verdict stays off, and Slack messages and browser tabs it would have filed are hidden as noise with no sign anywhere but Settings. When it runs, it picks from 24+ folders, where its own published test drops from 96% to 72% correct. When the Owner fixes a guess, the guess is overwritten and nothing is learned. Tickets are chosen by a cloud model on every tick and swapped by hand, and vague Tempo text is found only by reading every line.

**Current workaround**: start `worklog verdict serve` by hand; sort Unsorted with the full project picker; swap tickets and rewrite Tempo text by hand.

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Owner | runs worklog, sorts events, logs to Tempo | one person, one Mac, no GPU |

**Primary actor**: Owner.
**Hidden stakeholders**: Tempo (receives the text and tickets) and the Owner's customers, who read it.

## 2. Scope

### 2.1 In scope

- Verdict starts, stops and restarts with worklog, controlled by a switch in Settings
- The day page shows when Verdict is off or not answering, and how many loose events it did not check
- A permanent log of every Verdict decision and every Owner correction
- Project guesses from a short list, with past fixes shown to the model as examples
- One-tap shortlist when Verdict is unsure about a project
- Verdict picks a ticket when clearly sure; otherwise today's path decides
- Verdict checks each Tempo line's text before it is sent
- A nightly scorecard that measures Verdict against the log and tunes the two thresholds
- At 17:00, lines worklog is sure about are sent to Tempo; lines it is unsure about stay on their day page as today
- A Review section lists auto-sent lines from earlier days that the Owner has not confirmed, to confirm or edit

### 2.2 Non-goals

> Binding. A change here is an amendment, not an interpretation.

- No channel, website or other source is ever tied to a project automatically; every event is judged on its own text
- No sending of a line that failed a check or has an unsure ticket; those always wait for the Owner
- No removing a sent line from Tempo from the Review section; Review confirms or edits only
- No cloud or hosted model, and no training, fine-tuning or re-calibration of Verdict's model
- No department (deild) or customer suggestions, and no change to billing export
- No change to the Owner's "always" rules

## 3. Journeys

### Journey 1 — Verdict runs without a terminal (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | Verdict is switched on in Settings | the Mac restarts | Verdict is answering within 90 s of worklog starting, with no terminal opened |
| Error | the Python runner Verdict needs is not installed | the Owner switches Verdict on | the switch shows "Needs uv: brew install uv" and stays off |
| Edge | Verdict crashes 4 times in 10 minutes | the next health check runs | it stops retrying; Settings and the day page show "Verdict stopped: <last error>" with a Retry button |

### Journey 2 — The day page tells the truth (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | Verdict is running | the day page loads | no Verdict line is shown |
| Error | Verdict should be on but is not answering | the day page loads | an amber line: "Verdict isn't answering · N events not checked" with a Turn on button |
| Edge | the Owner switched Verdict off | the day page loads | a grey line: "Verdict is off · N events not checked" |
| Edge | Verdict was switched on less than 90 s ago | the day page loads | a grey line: "Verdict is starting" |

### Journey 3 — Filing a loose event (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | a #team message says "deploy vitinn tomorrow?" and the Owner worked in vitinn-infra this week | routing runs | it is filed under vitinn-infra and the guess is in the log |
| Error | Verdict gives different answers when the choices are reordered | routing runs | the event stays in Unsorted with its top choices as buttons |
| Edge | the next #team message says "lunch at 12?" | routing runs | it is not filed under vitinn-infra because of the channel; it is judged on its own words |

### Journey 4 — Tickets and text (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | a block's branch and events point clearly at one of its candidate tickets | estimation runs | the ticket is set by Verdict and marked as an automatic pick |
| Error | Verdict is unsure or off | estimation runs | the ticket is chosen exactly as today |
| Edge | a generated Tempo line reads "Worked on various tasks." | the line is generated | it is rewritten once; if it still fails, it is flagged "needs a look" on the day page |

### Journey 5 — The day sends itself, and the Owner reviews (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | auto-send was on yesterday; 4 lines were ready and 1 had no ticket | the Owner opens today's day page | Review lists yesterday's 4 sent lines; the 5th is still unsent on yesterday's page |
| Error | Tempo rejected one line at 17:00 | the Owner opens Review | that line shows "Not sent: <Tempo's message>" with a Send again button; the others show as sent |
| Edge | the Owner edits the hours of a sent line in Review | they save | the same Tempo worklog is updated, not duplicated, and the line leaves Review |

## 4. Requirements

### 4.1 Functional requirements

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | Owner MUST be able to switch Verdict on and off in Settings | switch state survives a daemon restart |
| FR-02 | MUST | worklog MUST start Verdict when it is switched on, and stop it when switched off | process present/absent within 5 s of the switch |
| FR-03 | MUST | worklog MUST restart Verdict after a crash, at most 3 times per 10 minutes | 4th crash leaves it stopped with the error shown |
| FR-04 | MUST | Settings MUST show Verdict's state: off, starting, running, not answering, needs uv, stopped with error | each state rendered in a component test |
| FR-05 | MUST | the day page MUST show a line when Verdict is not running, with the count of that day's loose events Verdict did not check | test with Verdict off and 3 unchecked events shows "3" |
| FR-06 | MUST | Verdict's state MUST default to on for an install with no setting | fresh env file → on |
| FR-07 | MUST | every Verdict project guess MUST be logged with its top 3 choices and scores, whether filed or not | log row per routed event |
| FR-08 | MUST | every Owner correction of a project or ticket MUST be logged next to what was there before | fix test shows before and after |
| FR-09 | MUST | log rows MUST never be removed by purge or compression | purge test leaves the log intact |
| FR-09a | MUST | an event or ticket MUST be filed by Verdict only if, in the answer with the original order, the top score ≥ "not enough evidence" score × the abstain margin setting (default 1.20) AND the top score ≥ the second score × the runner-up setting (default 1.10) AND the reversed-order answer has the same top choice. Scores are Verdict's probabilities | table test over 8 score triples |
| FR-10 | MUST | Verdict MUST be offered at most 6 projects per event, in this order until 6: the pinned folder; folders whose name appears as a whole word in the event's title, details or link ("link near-hits"); then folders with the most worked minutes in the last 14 days | shortlist test with 30 folders yields the expected 6 in order |
| FR-11 | MUST | the shortlist MUST never be empty when the event has a pinned folder | pinned-only test |
| FR-12 | MUST | the titles of the 5 most recent Owner corrections TO a project MUST be shown to Verdict as examples of that project, each cut to 60 characters, all examples together capped at 600 characters | request body test |
| FR-13 | MUST | an example MUST never by itself file an event; the event's own text is always judged | test: identical channel, unrelated text → not filed |
| FR-14 | MUST | an event MUST be filed only if Verdict gives the same answer with the choices in two different orders | disagreement test → unfiled |
| FR-15 | MUST | an unfiled event MUST show Verdict's top 2–3 projects as one-tap buttons in Unsorted, with the full picker still available | component test |
| FR-16 | MUST | Verdict MUST pick a block's ticket under FR-09a from at most 6 candidates, in this order until 6: the ticket linked to the block's coding session; open ticket keys written in the block's events; tickets logged on blocks of the same folder in the last 30 days, most recent first | fixture: a block on branch `VIT-212-fix` with session ticket VIT-212 and Verdict answer (0.6, 0.1, 0.1, agreed) gets VIT-212 |
| FR-17 | MUST | a Verdict ticket pick MUST never replace a ticket the Owner set or a ticket key found in the block's events | origin precedence test |
| FR-18 | MUST | when Verdict does not pick, the ticket MUST be chosen exactly as before | regression test unchanged |
| FR-19 | MUST | each generated Tempo line MUST pass two yes/no Verdict questions to count as passed: (a) "is this about <ticket summary>?" versus "something other than <ticket summary>"; (b) "is this a specific piece of work such as a feature, fix, file or meeting topic?" versus "something other than that". A line passes only if both answers are yes | test with recorded Verdict answers; the scorecard also runs a fixture of 5 good and 5 vague lines against the real model and prints how many it got right |
| FR-20 | MUST | a line that fails a check MUST be regenerated once, then flagged "needs a look" if it still fails | flag test |
| FR-21 | MUST | a line the Owner wrote by hand MUST never be checked or flagged | manual-line test |
| FR-22 | MUST | a nightly scorecard MUST replay every project and ticket decision of the last 30 days through Verdict and report right (Verdict would file it, and it equals the final value), wrong (would file it, and it differs) and unsure (would not file it), where the final value is the Owner's latest correction, or the applied value when there was none | CLI output on a fixture log |
| FR-23 | MUST | the scorecard MUST save the pair of settings, from abstain margin 1.0–2.0 and runner-up 1.0–2.0 in steps of 0.05, with zero wrong in the replay and the most right; if no pair has zero wrong, it keeps the current settings and says so | apply test writes both settings |
| FR-24 | SHOULD | Settings SHOULD show the latest scorecard in one line | component test |
| FR-25 | MAY | the Owner MAY run the scorecard on demand from the command line | `worklog eval --replay` |
| FR-26 | MUST | Owner MUST be able to switch auto-send to Tempo on and off in Settings | switch state survives a restart |
| FR-27 | MUST | at 17:00 Owner-local time on a day with auto-send on, worklog MUST send every ready line of that day to Tempo once | scheduler test with a fake clock |
| FR-27a | MUST | a line that becomes ready after the 17:00 run MUST NOT be auto-sent; it waits on its day page for a manual sync | late-line test |
| FR-27b | MUST | if Tempo cannot be reached at 17:00, the run MUST retry at each 15-minute tick until 23:59 that day, then mark the unsent lines Not sent | retry test |
| FR-28 | MUST | a line MUST count as ready only if its ticket was set by the Owner, found in its events, or picked by Verdict past the order check, and its text passed both checks or was written by the Owner | readiness table test |
| FR-29 | MUST | today's day page MUST show a Review section listing every auto-sent line from earlier days that the Owner has not confirmed, grouped by day, newest first | component test |
| FR-30 | MUST | Owner MUST be able to confirm a line in Review, which removes it from Review and changes nothing in Tempo | confirm test |
| FR-31 | MUST | Owner MUST be able to edit a line's ticket, hours or text in Review, which updates the same Tempo worklog and counts as confirmed | edit test asserts update, not create |
| FR-32 | MUST | a line whose auto-send failed MUST appear in Review with Tempo's message and a Send again button | failure test |
| FR-33 | SHOULD | Owner SHOULD be able to confirm all of one day's lines in one action | component test |
| FR-34 | MUST | a line already sent MUST never be sent a second time as a new worklog | double-run test |
| FR-35 | MUST | the Review section MUST be hidden when nothing is waiting for review | component test |

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| Start time | Verdict answering ≤ 90 s after start, model already downloaded | timed in the CHK |
| Health check | every 60 s; ≤ 3 restarts per 10 min | supervisor unit test with a fake clock |
| Per-event decision | ≤ 300 ms p95 on the Owner's Mac, both orders included | the scorecard prints p95 over every decision it replays |
| Per-call timeout | 10 s; on timeout that event stays unfiled and counts as not checked | client test |
| Exposure | Verdict listens on 127.0.0.1 only; the decision log stores no tokens or secrets | server test; schema has no secret columns |
| Shortlist | ≤ 6 projects, ≤ 6 tickets, ≤ 5 examples per project | unit tests |
| Event text room | event text keeps ≥ 200 of Verdict's 512 input tokens | server self-test |
| Text check cost | ≤ 1 s added per Tempo line | timed over one real day |
| Auto-send timing | sent within 15 min after 17:00, or at the first tick after waking that day | scheduler test |
| Success after 7 days | Slack and browser events labelled noise per day down ≥ 50% against the 7 days before switch-on; Verdict picks the Owner corrects ≤ 1/day | noise count from the events table per day; corrections from the decision log |

## 6. Launch criteria

- [ ] Every MUST in §4.1 has a passing test
- [ ] The error path of each journey is exercised by a test or a CHK
- [ ] The Owner switches Verdict on, kills it, and sees it come back (CHK)
- [ ] The Owner switches Verdict off and sees the grey line on the day page (CHK)
- [ ] `cargo test`, clippy, fmt, `bun test`, typecheck and build are green

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | `uv` lives in one of `~/.local/bin`, `/opt/homebrew/bin`, `/usr/local/bin`, or on PATH | High | switch shows "needs uv" for an unusual install |
| A2 | showing past fixes as option examples helps more than it hurts | Med | scorecard shows it; examples can be turned to 0 |
| A3 | Verdict's two-way check separates vague from concrete Tempo text | Med | lines flagged wrongly; check can be tuned from the log |
| A4 | the same two thresholds work for projects and tickets | Med | scorecard reports tickets separately |
| A5 | 14 days is long enough to include every active project | Med | a dormant project is missed; pinned folders still included |
| A6 | default on is wanted for every install | High | a user without uv sees "needs uv" once |
| A7 | auto-send defaults to off until the Owner switches it on | Med | Owner flips one switch once |
| A8 | if the Mac is asleep at 17:00, auto-send runs at the first tick after waking that same day | Med | lines sent a little late; never the next day |

## 8. Open questions

None. Removing a sent line from Tempo is out of scope, so the "already sent" marker is never cleared.

## Appendix A — Glossary

| Term | Means |
|------|-------|
| Verdict | the local model that picks one option from a list, or says "not enough evidence" |
| loose event | a Slack message or browser tab with no project of its own |
| decision log | the permanent record of Verdict's guesses and the Owner's corrections |
| shortlist | the at most 6 options Verdict chooses from |
| order check | asking Verdict twice with the options reversed and acting only if both agree |
| filed | Verdict put the event in a project (or set the ticket); "unfiled" means it did not |
| not enough evidence | Verdict's built-in option for "none of these"; its score is the abstain score |
| scorecard | the nightly replay of the decision log through Verdict |
