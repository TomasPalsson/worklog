# Prep — Session customer pins
Gathered: 2026-09-27 · Questions: 9 of 12 · Route: dispatch · Status: ready for spec
Seed: "what about just putting in the global CLAUDE.md that claude has to ask me what customer Im working for or something like that and he can just call a command that assigns that customer to that session?" — user, via /flow:develop-idea

## Decisions
- D-01 A pin is remembered per repo + git branch; a new session on the same branch (e.g. `/flow:next` after `/clear`, a worktree or background job of that feature) inherits it and does not ask. — user, Q2
- D-02 The default branch (`main`/`master`) never inherits a pin: each session there works the customer out from the prompt or asks (`/flow:next` never runs on main). — user, Q3
- D-03 A pinned name must resolve to a known customer (name or alias, e.g. "sjukra" → Sjúkra); anything else is refused and the command prints the customer list so Claude retries or asks. — user, Q5
- D-04 A block's pinned customer is shown — marked as pinned — in both the day view and the block detail view of the review UI, and the Owner can change it there. — user, Q7
- D-05 Priority: the Owner's change in the UI beats a Claude pin, and a pin beats the system's text guess (`tag_sessions` customer match); the guess only fills sessions with no pin. — user, Q9
## Not this
- Never ask outside `~/Desktop/Work` — anything in /Work is work, anything outside is not.
- Don't ask when the repo name (`apro-skills`, `sjukratryggingar-genai`) or the prompt ("Go into RU and do this") already makes the customer obvious — just assume it.
- Never guess when nobody is there to ask (background jobs, sub-agents, unattended runs) — leave it unknown.
- No after-the-fact model guess from prompts: on Friday 2026-09-25 it resolved the same 3 of 6 vitinn-infra sessions the existing text matcher already does, at more tokens.
- Never re-ask in a session that continues work whose customer is already known (e.g. `/flow:next` after `/clear` on the same feature) — user, Q1
## Discretion
- How Claude is told to work out and pin the customer — user left it to the spec (Q4); recommended: a separate small SessionStart hook (not `worklog hook-run`, which must stay silent) that speaks only in `~/Desktop/Work` and only when the branch has no pin.
## Assumptions
- A-01 Problem: in a shared repo most Claude sessions never name their customer in text, so worklog cannot tell whose work they are and leaves them unknown — evidence: on 2026-09-25, 8 of 11 vitinn-infra sessions resolved to no customer (spec 007 real-data run, .specs/007-session-lanes-and-fresh-descriptions/NOTES.md) — confidence: high — confirmed Q1
- A-02 When the Owner works in a shared repo (vitinn-infra) for several customers, those sessions' minutes stay unknown and the Owner splits the hours by hand afterwards — evidence: none — confidence: medium — confirmed Q1
- A-03 Today: worklog guesses from the session's prompt text with a strict single-customer match (`session_customers::tag_sessions`) and the Owner hand-sets splits for the rest — evidence: rust/crates/worklog-core/src/session_customers.rs — confidence: high — confirmed Q1
- A-04 Success: sessions in shared repos carry the right customer, including a switch of customer mid-session (a pin with a start time, re-pinned on switch), and a pin wins over the text guess — evidence: none — confidence: medium — confirmed Q1
- A-05 Riskiest assumption: the in-session Claude (Opus) can almost always work out the customer itself from the folder, its prompts, the files it edits and its branch/worktree name, so asking the Owner stays rare — evidence: none — confidence: low — confirmed Q1
- A-06 Appetite: whatever it takes — the Owner does not care how long it takes, but scope stays to what the Owner says yes to — evidence: none — confidence: high — confirmed Q6
- A-07 worklog already installs a `SessionStart` handler (`worklog hook-run`) into `~/.claude/settings.json` — evidence: rust/crates/worklog-core/src/hook.rs:27 — confidence: high — confirmed Q1
- A-08 `worklog hook-run` must never print to stdout (it would surface in the session) — evidence: CLAUDE.md "Never print to stdout from `worklog hook-run`" — confidence: high — confirmed Q1
- A-09 A session's customer reaches block building as `InferEvent.lane_tag`, set by `session_customers::tag_sessions` inside `infer_allocations::build_day_blocks` — evidence: rust/crates/worklog-core/src/infer_allocations.rs:34 — confidence: high — confirmed Q1
- A-10 If PR #56 (perf/evals) merges first, any new reader of `events.raw_json` must go through `crate::raw_json::decode_raw_json` (raw_json may be a deflated BLOB) — evidence: PR #56, not yet on main — confidence: medium — unconfirmed
## Verify
- In `vitinn-infra` on a feature branch, a new session told "work on the Sjúkra config", then `/clear` + `/flow:next` on the same branch; after the day rebuild both sessions' blocks show "Sjúkra (pinned)" in the day view, and Claude never asked the Owner. — user, Q8
## Open
- Q: Changing a pin from the UI — does it change only that session's pin, or the whole branch's remembered pin (D-01)? → deferred to spec
- Q: Command shape and storage: a CLI/daemon call that records (session_id, customer, from-timestamp), and how it feeds `lane_tag` alongside the text guess → deferred to spec
