# Claude Code terminal mods and worklog

## Top 5

**1. Live status line (day total, ticket, end-of-day nudge)**
- **What you get:** One line under the prompt shows e.g. "2h30 today | GENAI-9129 | 3 to review". After a set hour it adds a once-a-day "blocks to review / yesterday not in Tempo" toast.
- **How it works:** `session.start` sets `$.clock.every(60000)`. Each tick calls `$.http.fetch('http://127.0.0.1:9323/days/<today>')` and `$.ui.status(text)`. The fetch is wrapped in try/catch, and `status(undefined)` clears the line when the daemon is down. The nudge is deduped with `$.store` key `nudge:<date>`, and the hour is computed from `WORKLOG_TZ`. Current ticket = the newest block of today.
- **Effort:** S
- **Worklog needs:** Nothing. Do not promise "hours left", because there is no daily target field; `/weeks/:monday/closeout` has only `tempo_required_days`. Do not use `worklog summary` every tick, since it calls `ensure_daemon_running`.

**2. Branch becomes the ticket (Rust fix plus mod toast and context)**
- **What you get:** A session on branch `GENAI-9129-foo` is tracked under that ticket. A toast and a context block tell you and Claude which ticket it is.
- **How it works:** In Rust, `hook_run::handle` falls back to `git::current_branch(cwd)` when the prompt and cwd give no key. This is the fix for the gap where `hook_run.rs:15-38` never reads the branch. The mod adds `prompt.context` (`{blocks:[...(await next(e)).blocks, mine]}`, byte-stable per key) from `worklog ticket get KEY --json`. It also adds a toast on `session.start`, rechecking the branch on `turn.complete` or a 60 s timer.
- **Effort:** S (Rust) plus S (mod)
- **Worklog needs:** A small change in `hook_run.rs` (branch as a third key source). The mod must not re-send `hook-run`, because that would duplicate every event. `CwdChanged` does not fire on `git checkout` in the same directory. Do not call `$.ui.ask` from `session.start`, because it blocks the first prompt.

**3. Commit and PR ticket trailer**
- **What you get:** Commits and PRs on a ticket branch carry the Jira key, so the GitHub and Jira collectors attribute them correctly later.
- **How it works:** The `attribution.text` hook (kinds `commit` and `pr`) adds a `Ticket: GENAI-9129` trailer. Optionally, `tool.call` with matcher `{tool:'Bash'}` returns `{deny}` once per commit attempt when `git commit -m` lacks the key. `github.rs:100,169` pulls the key into `jira_issue`, and `infer.rs:697-701` then sets `ticket_origin='event'`. Skip `main` and `master`. Pass through `-F` and commits without `-m`.
- **Effort:** S
- **Worklog needs:** Nothing. The trailer alone gives most of the value; the deny is optional.

**4. `/worklog` command with a review pane**
- **What you get:** `/worklog today|week|review` prints numbers into the transcript, and `review` opens a docked pane of today's blocks with single-key actions.
- **How it works:** Register one command `worklog` last in `session.start` and dispatch on args. `today` and `week` run `worklog summary --json` or `worklog week --json`, so `WORKLOG_TZ` is honoured. `review` calls `$.ui.open`; because a user command opened it, it seats at any width. `ui.render` draws rows from GET `/days/:day` and `/blocks/:day` over `127.0.0.1:9323`. Buttons act on a selected row (hotkeys are one key per button, not per row): `p` for `/blocks/:id/personal`, `i` for `/ignore`, `t` for a ticket Select that POSTs `/blocks/:id/ticket`, and `d` for `/description`. Never expose `/estimate`, `/delete`, or anything touching `tempo_worklog_id`.
- **Effort:** M
- **Worklog needs:** Nothing. Take the day from `worklog summary --json`, not from a UTC date computed in the mod.

**5. Send to Tempo: dry run first**
- **What you get:** A two-step Tempo send in the terminal. The real "send" button does not exist until a fresh dry-run preview has been shown.
- **How it works:** `/worklog sync` runs `worklog sync --dry-run` and reads GET `/tempo/lines/:day`. It stores `{day, hash, ts}` in `$.state`. The send button (`s`) is rendered only if the state is under 10 minutes old and the hash is unchanged. It then POSTs `/sync` with an explicit `dry_run:false` for the same day. Optionally, a `tool.call` Bash guard denies `worklog sync` without `--dry-run` when no fresh preview exists. That guard only covers Claude's shell calls, not your own terminal.
- **Effort:** M
- **Worklog needs:** Nothing. Hours and rounding stay in worklog, and no code path touches `tempo_worklog_id`.

## More ideas

- **Pin customer at session start via `$.ui.ask`:** Needs `worklog session-hint --json` returning `{pin_needed, customers[], text}`. The customer list comes from GET `/billing/registry`, then run `worklog pin <customer> --session <id>`. Skip `main` and `master`. M.
- **Unpinned client-folder push guard:** `tool.call` denies `git push` or `gh pr create` when the folder is multi-tenant and the session is unpinned. Needs a read-only `worklog pin --status --session --cwd` returning JSON. M.
- **Re-pin on branch switch:** Compare `git branch --show-current` on `prompt.submit` and `CwdChanged`. If the branch or the folder's customer changed, `$.ui.ask`, then `worklog pin`. M.
- **Ticket chip in the AbovePrompt band:** Shows "GENAI-9129 (manual|guess|none)" from the newest block of today. Yield on `props.hasSurvey`. A pick button POSTs `/blocks/:id/ticket`. M.
- **Billable-hours meter in the SessionMode footer:** Reads GET `/export/<today>` (read-only, never `--mark`). Shows hours only if the customer is unresolved. Needs a daemon `folder` filter or a client-side reimplementation of `work_folder_for_path`. M.
- **Billing export pane showing blanks:** Unresolved Viðskiptamaður or Verkefni render as dim "None". `$.ui.copy` copies the text. Marking exported (POST `/export/:day/mark`) sits behind `$.ui.ask` and runs only after copy. Check the real `/billing/lines/status` shape first, because it returns a job state, not hours. M.
- **Model-callable Jira tools:** `$.tool.register` for ticket find, start and create as `mcp__worklog__*`, with `tool.check` returning `ask` on create. The gain over the existing skill plus Bash is typed args and an enforced confirmation. Done is never exposed. M.
- **Done prompt after `gh pr merge`:** Run `worklog collect github --days 1` first, then `worklog ticket hints --json`. Then `$.ui.ask`, and move to Done only on an explicit Yes. A toast-only variant at `turn.complete`, throttled by day plus hint key in `$.store`, is the cheaper version. S-M.
- **Capture health line:** At `session.start`, check `/health` and `worklog hook status --json`. Show a line only on failure (daemon down, hooks missing). "No events for N min" needs a new `GET /sessions/:id/last-event`, and idle sessions would false-alarm. S.
- **Block feedback after each answer:** `turn.complete {text}` shows the last block and its ticket from GET `/blocks/<today>`. Debounce to 2-3 minutes and skip `POST /infer` unless the block is stale. M.
- **No-ticket hint plus `/worklog start`:** A `PromptHint` shows only when the branch has no key and the cwd is under `~/Desktop/Work`. It points to `worklog ticket find` then `start`. M.
- **Session-end kick of infer:** `$.http.fetch` POST `/infer` on `session.end`, without waiting past about 800 ms. The shared limit is about 1.5 s. Low value, because the schedule and the web UI already run infer. S.

## Dropped

- **Auto-opening "glance" pane from a timer:** Unasked panes need 144 columns or silently return `{isPlaced:false}`. It also duplicates the status line and `/worklog`.
- **Throttled per-tool capture replacing hook rows:** Breaks a rule. Pre/PostToolUse are the heartbeats that stop blocks shredding, and capture would depend on the mod being loaded.
- **Subagent activity folded into the parent session:** Not needed. Subagent tool calls already fire the settings hooks with the parent `session_id`. At most, add `SubagentStart` to `hook::EVENTS`.
- **Topic-drift ticket suggestion:** Imaginary link. Mod `$.state` is invisible to the separate `hook-run` process, and a mod that re-sends `hook-run` duplicates events.
- **Worktree-to-project-root shim as a mod:** Wrong place. Do it in Rust inside `hook_run::project_root` if ever needed. Low value, since `.claude/worktrees` is already handled.
- **Task-aware prompt context on ticket talk:** Overlaps the shipped worklog skill and the branch-context block.
- **Any mod that re-sends `hook-run` or runs `worklog session-hint` as a probe:** Breaks a rule. Re-sending duplicates events, because `source_id` includes `now()`. The probe writes an inherited pin as a side effect and returns prose, not structured data.
- **Desktop-only surfaces (Svg, attach family):** Excluded by your terminal-only scope.

## Gotchas

- **Pane width rule:** A pane opened by a user command (`/worklog review`) seats at any width and docks from 110 columns in fullscreen. A pane opened from a timer or `session.start` needs 144 columns (110 once you have opened that id), or `$.ui.open` returns `{isPlaced:false}`. Status lines, toasts, `PromptHint` and `AbovePrompt` have no width rule.
- **Status line is one slot per plugin:** `$.ui.status` is drawn as `⚠ plugin: text`, with no real statusline API. Merge the day total and the nudge into one string, or send the nudge as a toast.
- **Reaching the daemon:** `$.http.fetch` can hit `http://127.0.0.1:9323` directly. The `socketPath` option for `api.sock` caps at about 100 bytes, so TCP is safer. `$.process.run` takes argv with no shell, a 30 s default timeout, and rejects on spawn failure or timeout, so wrap it in try/catch. `worklog summary` and `week` can start the daemon.
- **Persistence:** `$.state` survives hot reload but not the session, and timers are cancelled on reload. Re-create timers in `session.start`. `$.store` persists on disk and is shared by every session on the machine, which suits once-a-day dedupe. Timers run only while a Claude Code session is open, so nudges are best-effort.
- **Hook limits and registration:** `session.start` is awaited before the first prompt, so no `$.ui.ask` there. `session.end` hooks share about 1.5 s. `$.command.register` throws on built-in names, so register it last. `$.ui.ask` rejects under `-p`. The `prompt.context` result replaces the block list, and unstable text costs prompt-cache hits. A plain `git checkout` fires no event, so recheck the branch on `turn.complete` or a timer.