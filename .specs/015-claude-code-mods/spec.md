# Spec: Claude Code terminal mod for worklog

**Created**: 2026-10-04 · **Route**: dispatch

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: The Owner only sees worklog's numbers by opening the web UI, so missed Tempo days and wrong tickets are found late. Claude Code sessions on a ticket branch (`GENAI-9129-foo`) are not tagged with that ticket, because hook-run never reads the branch.

**Solution**: A worklog mod that runs inside the Claude Code terminal. It shows today's hours, gives one reminder a day about the last workday, tells the Owner and Claude which ticket the branch is, puts a `Ticket:` trailer on commits and PRs, and adds a `/wl` command with a small review pane. hook-run uses the branch as a last-resort ticket source.

**Who it's for**: The Owner, a consultant who codes in Claude Code and logs time to Tempo.

**MVP cut line**: everything tagged `MUST` in §4.1 ships. `SHOULD` is v1.1. `MAY` is backlog.

**Key decision**: The mod ships inside the worklog binary, and `worklog hook install` turns it on (D-03). Nothing is installed by hand.

## 1. Context

### 1.1 Problem statement

The Owner works in Claude Code all day. worklog records those sessions through settings.json hooks, but the ticket comes only from the prompt text or the folder path (A-01). When the Owner works on a ticket branch without typing the key, the block lands unticketed and has to be fixed in the web UI. A day that is short of 8h, or never sent to Tempo, is noticed days later.

**Current workaround**: Open the review UI on :3333 and fix blocks by hand. Check the Logged page for missing days.

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Owner | Codes in the Claude Code terminal and logs time to Tempo | Never uses the desktop app (D-02); the status line is already crowded (D-05) |
| Claude | The model in the session | Writes commits and PRs; reads injected context |

**Primary actor**: Owner.
**Hidden stakeholders**: The Tempo and billing pipelines, which consume tickets set here.

## 2. Scope

### 2.1 In scope

- Branch name as the third and last ticket source in hook-run (D-04).
- An hours status line (D-05).
- A once-a-day reminder about the last workday, in work folders only (D-06, D-07).
- Ticket toast and context for Claude on a ticket branch, in work folders only.
- A `Ticket: KEY` trailer on commits and PRs, never a block (D-08).
- `/wl today|week|review`, with a review pane of 4 actions (D-09).
- `worklog hook install` writes the mod out and enables it. `uninstall` removes it (D-03).

### 2.2 Non-goals

> Binding. A change here is an amendment, not an interpretation.

- Desktop-only surfaces (Svg, attach family).
- Re-sending `worklog hook-run` from the mod, or replacing the settings.json hooks with mod events.
- Auto-opening panes from timers or session.start.
- Never sends anything to Tempo or Jira on its own.
- Shows nothing in non-work folders (not aproorg, not under ~/Desktop/Work/) except the hours status line.
- Never slows the prompt: daemon polled at most once per 60 s, never awaited in a prompt/turn path.
- No delete, split, duration, estimate or Tempo send in the pane (those stay in the web UI).

## 3. Journeys

### Journey 1 — Start a session on a ticket branch (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | cwd under ~/Desktop/Work/acme on branch `GENAI-9129-foo`, daemon up | Owner starts Claude Code | Status line reads `worklog 2h30`; a toast names `GENAI-9129`; Claude's context names the ticket; new hook events carry `GENAI-9129` |
| Error | Daemon not running | Owner starts Claude Code | No status line, no error text; hook events still carry the branch ticket |
| Edge | Same branch, but the prompt says `PROJ-42` | Owner submits the prompt | That event carries `PROJ-42` (prompt beats branch) |

### Journey 2 — The daily reminder (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | Today is Monday, Friday has unsynced lines, work folder | First session of the day starts | One toast says Friday isn't synced; no second toast that day in any session |
| Error | Daemon down, or week never pulled | Session starts | No toast, and the day is not marked as nudged |
| Edge | cwd is ~/code/hobby (not work) | Session starts | No toast; hours line still shows |

### Journey 3 — Review today from the terminal (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | Today has 5 blocks | Owner runs `/wl review`, picks a block, presses `t`, types `GENAI-1` | Pane opens at any width; the block's ticket is `GENAI-1` in the web UI |
| Error | The block is already synced to Tempo | Owner presses `i` | The pane shows the daemon's refusal text; the block is unchanged |
| Edge | Today has no blocks | Owner runs `/wl review` | Pane says there is nothing to review today |

## 4. Requirements

### 4.1 Functional requirements

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | hook-run MUST take the ticket from the git branch of the event's cwd when neither prompt nor cwd path names one | Rust test: temp repo on `PROJ-7-x`, no key in prompt or path, event has `PROJ-7` |
| FR-02 | MUST | hook-run MUST prefer a prompt key, then a cwd-path key, over the branch key | Rust test: prompt `PROJ-42` on branch `PROJ-7-x` gives `PROJ-42` |
| FR-03 | MUST | When a session starts and every 60 s after, the mod MUST show `worklog <H>h<MM>` (e.g. `worklog 2h30`) as a dim label in the prompt footer's mode labels (`ui.render` on `SessionMode`), not via `$.ui.status` (user ruling 2026-10-04) | Mod test with stubbed daemon and fake clock |
| FR-03b | MUST | The hours in FR-03 MUST count only today's blocks that are not personal and not ignored | Unit test on `workSeconds` |
| FR-04 | MUST | When a daemon call fails or takes over 2 s, the mod MUST drop the footer label and show no error text | Mod test: fetch fails, no label drawn |
| FR-05 | MUST | In a work folder, the mod MUST show one reminder toast when the last workday has unsynced lines or is short of required hours; if both apply, one toast names both | Mod test on each case and on both |
| FR-06 | MUST | Required hours MUST be the day's Tempo required seconds, or 8h when never pulled; a day whose required seconds is 0 gets no reminder | Mod test on null, 0 and 28800 |
| FR-06b | MUST | After a reminder toast is shown, the mod MUST store `nudge:<today>`, and MUST NOT toast again that day in any session | Mod test with `mock.store`: second session, no toast |
| FR-06c | MUST | When the daemon is down or the week is unreadable, the mod MUST show no reminder and MUST NOT store the marker | Mod test: fetch fails, store untouched |
| FR-07 | MUST | The last workday MUST be the latest Mon–Fri before today; public holidays are not special-cased here (FR-06's 0 covers them) | Unit test: Monday maps to Friday, Sunday to Friday |
| FR-08 | MUST | In a work folder on a branch with a ticket, the mod MUST toast the ticket at session start | Mod test |
| FR-09 | MUST | In a work folder on a branch with a ticket, the mod MUST add a context block naming the ticket | Mod test on `prompt.context` |
| FR-10 | MUST | In a work folder on a branch with a ticket, the mod MUST append `Ticket: KEY` to the commit and PR attribution text | Mod test on `attribution.text` |
| FR-10b | MUST | The branch ticket MUST be the first match of `[A-Z][A-Z0-9]{1,9}-\d+` in `git branch --show-current`; detached HEAD, no repo or no match means no ticket and no toast, context or trailer | Unit test on `ticketFromBranch` |
| FR-11 | MUST | The mod MUST never deny a tool call, permission or commit | `claude plugin validate` lists no `tool.call` or permission hook; attribution test asserts only `{text}` is returned |
| FR-12 | MUST | `/wl today` MUST print `worklog today: <H>h<MM>`; `/wl week` MUST print `worklog week: <H>h<MM>` (sum of the week's logged seconds); any other argument prints the usage `today|week|review` | Mod test on each |
| FR-13 | MUST | `/wl review` MUST open a pane listing today's non-ignored blocks with exactly the 4 actions in §4.3 | Pane test: mount, press each hotkey, assert the request sent |
| FR-14 | MUST | The pane MUST show the daemon's error text when an action is refused | Pane test with a stubbed 400 |
| FR-15 | MUST | `worklog hook install` MUST write the mod files to `<data dir>/claude-mod` (data dir = `$WORKLOG_HOME` or `~/.local/share/worklog`) and add that path to `env.CLAUDE_CODE_PLUGIN_DIRS` in ~/.claude/settings.json, appending with `:` to any existing value; a second run changes nothing | Rust test: files written, key appended once, existing value kept |
| FR-16 | MUST | `worklog hook uninstall` MUST remove only that path from the key (dropping the key when empty) and leave other plugin dirs intact | Rust test |
| FR-17 | SHOULD | `worklog hook status --json` SHOULD report whether the mod is installed | Rust test |

### 4.2 What the mod reads

| Need | Call | Field (unit) |
|------|------|--------------|
| Local today | `GET /logged?from=<utc day>&to=<utc day>` | `today` (`YYYY-MM-DD`) |
| Today's hours, pane rows | `GET /days/<today>` | `blocks[]`: `duration_seconds` (s), `is_personal`, `ignored_at`, `jira_issue`, `description`, `id` |
| Last workday status | `GET /weeks/<monday of last workday>/closeout` | that day's `pending_lines` (>0 = unsynced), `max(logged_seconds, tempo_seconds)` (s), `required_seconds` (s or null) |
| Week hours | `GET /weeks/<monday of today>/closeout` | sum of `logged_seconds` (s) |
| Work folder | path under `~/Desktop/Work/`, or `git remote get-url origin` contains `github.com/aproorg` or `github.com:aproorg` | — |

### 4.3 Review pane actions

| Key | Action | Call | Body |
|-----|--------|------|------|
| `p` | toggle personal | `POST /blocks/<id>/personal` | `{"is_personal": !current}` |
| `i` | toggle ignore | `POST /blocks/<id>/ignore` | `{"ignored": !current}` |
| `t` | set ticket | `POST /blocks/<id>/ticket` | `{"jira_issue": "<typed>"}`, empty text sends `null` |
| `d` | edit description | `POST /blocks/<id>/description` | `{"description": "<typed>"}` |

A list control picks the block. `t` and `d` open a text input prefilled with the current value; Enter sends it, Esc cancels it with no request. After any successful action the list reloads. The pane opens whatever the terminal width (it is opened by the Owner's command).

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| Daemon polling | ≤ 1 request batch per 60 s per session | Fake-clock test counts requests |
| Session-start cost | ≤ 2 git subprocesses (`branch --show-current`, `remote get-url origin`) and 0 daemon calls awaited before the first prompt | Mod test counts `process.run` stubs |
| Daemon call timeout | each call gives up after 2000 ms | Mod test with a stub that never resolves and a fake clock |
| Reminder frequency | ≤ 1 toast per local day across all sessions | `mock.store` test on key `nudge:<today>` |
| hook-run extra cost | ≤ 1 `git` subprocess per event, only when no key was found | Code review of hook_run.rs |

## 6. Launch criteria

- [ ] Every MUST in §4.1 has a passing test.
- [ ] The error path of each journey is exercised.
- [ ] Run `worklog hook install`, open Claude Code on a `GENAI-…` branch under ~/Desktop/Work/: the hours status line and a ticket toast appear; `/wl review` opens the pane; `cargo test --manifest-path rust/Cargo.toml` and `claude plugin test mods/worklog` are green.

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | `CLAUDE_CODE_PLUGIN_DIRS` in the `env` block of ~/.claude/settings.json loads the folder in every session (documented in the mods reference; proven by the first CHK) | Med | Mod never loads; then amend to `claude plugin marketplace add` + `install` |
| A2 | "aproorg repo" means the `origin` remote URL contains `github.com/aproorg` or `github.com:aproorg` | Med | Reminder and ticket toast miss or over-fire in some folders |
| A3 | "Unsynced" = closeout `pending_lines > 0`; "hours" = `max(logged_seconds, tempo_seconds)` | Med | Wrong reminder text |
| A4 | Claude follows the attribution text, so the trailer is model-written, not guaranteed | High | Some commits lack the trailer; nothing breaks |
| A5 | The daemon's `today` from `GET /logged` is the source of truth for the local day | High | Off-by-one day near midnight |

## 8. Open questions

None.

## Appendix A — Glossary

| Term | Means |
|------|-------|
| Work folder | cwd under ~/Desktop/Work/, or a git repo whose origin is in the aproorg org |
| Last workday | The latest Mon–Fri day before today (local) |
| Mod | A Claude Code plugin of function hooks; source lives in `mods/worklog/` |
