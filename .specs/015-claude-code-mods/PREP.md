# Prep — Claude Code terminal mod for worklog
Gathered: 2026-10-04 · Questions: 9 of 12 · Route: dispatch · Status: ready for spec

## Decisions
- D-01 Build ideas 1–4 from research.md: status line, branch→ticket, commit/PR ticket trailer, /worklog command + review pane (user: "I like the first 4", before Q1). — user, Q1
- D-02 Terminal surface only; desktop/vscode/mobile out of scope (stated before Q1). — user, Q1
- D-03 The mod lives in this repo and `worklog hook install` enables it, so it ships and upgrades with worklog. — user, Q1
- D-04 Ticket key precedence in hook-run: prompt, then cwd path, then git branch (branch only as fallback). — user, Q2
- D-05 Status line shows only today's hours (e.g. `worklog 2h30`); no ticket, no review count — user's existing `flow statusline` is already big. — user, Q3
- D-06 Once-a-day toast nudge, e.g. "yesterday isn't synced" or "yesterday doesn't add up to 8h"; shown only in aproorg repos or under ~/Desktop/Work/. — user, Q4
- D-07 The nudge checks the last workday (Mon checks Fri; weekends skipped). — user, Q5
- D-08 Commit/PR: add a `Ticket: KEY` trailer via attribution.text only; never block or deny a commit. — user, Q6
- D-09 `/worklog review` pane has exactly 4 actions: personal, ignore, set ticket, edit description. No delete, split, duration, estimate or Tempo send (those stay in the web UI). — user, Q7
## Not this
- Desktop-only surfaces (Svg, attach family).
- Re-sending `worklog hook-run` from the mod, or replacing the settings.json hooks with mod events.
- Auto-opening panes from timers or session.start.
- Never sends anything to Tempo or Jira on its own. — user, Q8
- Shows nothing in non-work folders (not aproorg, not under ~/Desktop/Work/) except the hours status line. — user, Q8
- Never slows the prompt: daemon polled at most once per 60 s, never awaited in a prompt/turn path. — user, Q8
## Discretion
- Exact wording of status line, toasts and pane rows.
## Assumptions
- A-01 `hook_run::handle` takes the Jira key only from prompt + cwd, never the branch — evidence: rust/crates/worklog-core/src/hook_run.rs:131 — confidence: high — unconfirmed
- A-02 `git::current_branch(dir)` already exists and is used for customer pins — evidence: rust/crates/worklog-core/src/git.rs:131, rust/crates/worklog-core/src/session_pins.rs:156 — confidence: high — unconfirmed
- A-03 Daemon has GET /days/:day, /blocks/:day and POST /blocks/:id/{ticket,description,personal,ignore}, no auth on TCP 127.0.0.1:9323 — evidence: rust/crates/worklog-core/src/daemon.rs:147-165 — confidence: high — unconfirmed
- A-04 Mod API has attribution.text (kinds commit|pr), prompt.context, turn.complete, command.run, $.http.fetch, $.clock.every, $.ui.status/toast/open — evidence: plugin-authoring/types/claude-code.d.ts:680,4043,4310,1710,3381,3360 — confidence: high — unconfirmed
- A-05 Claude Code hooks are installed today by `worklog hook install` editing ~/.claude/settings.json — evidence: rust/crates/worklog-core/src/hook.rs:109 — confidence: high — confirmed Q1
- A-07 The release ships only the binary, so the mod files must be embedded in it and written out at install time; Claude Code finds them via `CLAUDE_CODE_PLUGIN_DIRS` or a plugin entry — evidence: install.sh, plugin-authoring/reference.md (--plugin-dir, CLAUDE_CODE_PLUGIN_DIRS) — confidence: medium — unconfirmed
- A-06 (ticket part dropped by D-05) Day boundaries follow $WORKLOG_TZ — evidence: .specs/015-claude-code-mods/research.md (Top 5 #1), CLAUDE.md conventions — confidence: medium — unconfirmed
- A-08 "aproorg repo" = the cwd's git `origin` remote URL contains `github.com/aproorg` (or `github.com:aproorg`) — evidence: none — confidence: medium — unconfirmed
## Verify
- Run `worklog hook install`, open Claude Code on a `GENAI-…` branch under ~/Desktop/Work/: the hours status line and a ticket toast appear; `/worklog review` opens the pane; `cargo test --manifest-path rust/Cargo.toml` and `claude plugin test <mod dir>` are green. — user, Q9
## Open
- (none yet)
