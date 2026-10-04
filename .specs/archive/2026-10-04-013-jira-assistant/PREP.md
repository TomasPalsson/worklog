# Prep — Jira assistant (CLI + skill + status hints + ticket creator)
Gathered: 2026-10-04 · Questions: 14 of 24 (user lifted the 12 cap, Q7) · Route: dispatch · Status: ready for spec

## Decisions
- D-01 All three pieces are in scope: (1) `worklog` CLI + skill to get/work on a ticket, (2) status-change suggestions (ToDo / In Progress / Blocked / Done), (3) ticket creator with billing-account learning that creates a ticket when none is found. — user, Q1
- D-02 Status-change hints show in both places: in Claude Code (via the skill, e.g. starting work on a ticket → "move to In Progress?") and on the web My Tasks page. — user, Q2
- D-03 Some status moves may happen automatically, but a move to Done is never automatic — it always asks first. — user, Q3
- D-04 The only automatic move is ToDo/Backlog → In Progress when work starts on the ticket. Every other move (Blocked, Done, back to ToDo) is a suggestion the user confirms. — user, Q4
- D-05 "Work started" means only an explicit start in Claude Code ("let's work on GENAI-9129" → skill runs the start command). Logged time / blocks on a ticket never trigger the auto move. — user, Q5
- D-06 The billing-account clue log (account → clues, ticket count, decision history) lives in worklog's SQLite DB, not a JSON file. — user, Q6
- D-07 When no ticket matches, Claude shows ONE confirm with the new ticket's title + billing account together; one "yes" creates it, then it goes straight to In Progress (work is starting). — user, Q7
- D-08 New ticket text: no emojis, no fixed 4-section template, never the user's chat text pasted verbatim — Claude writes it properly. Sections appear only when they have content; length scales with the task; extra useful info (context, links, systems) is welcome. — user, Q8
- D-09 Ticket title and description are always written in English. — user, Q9
- D-10 "Move to Done?" is suggested (never applied) when (a) a GitHub PR for that ticket is merged, or (b) the user says the work is finished in Claude Code. — user, Q11
- D-11 "Move to Blocked?" is suggested only when the user says they're waiting on something (customer, access, someone else). Idle time never triggers it. — user, Q12
- D-12 Account learning keeps the LibreChat rules: (1) first run / "relearn" bootstraps from the last 200 GENAI tickets with an Account set; (2) only accounts in Jira's fresh create-metadata list are ever suggested; (3) a clue wrong 2+ times for an account is dropped; (4) an account the user names is used without asking (still logged). — user, Q13
## Not this
- Never delete a Jira ticket. — user, Q10
- Never assign a ticket to anyone other than the user unless the user names them. — user, Q10
- Never write to tickets outside the GENAI project (reading any ticket key is fine). — user, Q10
- No emojis in ticket text. No copy-pasting the user's chat message as the description. — user, Q8
- No automatic move to Done, ever. — user, Q3
- Never post to Slack (from the user's LibreChat prompt: "a post can't be taken back").
## Discretion
- What `worklog ticket get` prints (summary, status, description, comments, account, time logged — spec picks).
- Whether/how the web shows the clue log.
- Build order of the three pieces — user, Q1 ("any order you think is the most appropriate").
## Assumptions
- A-07 Ticket descriptions are sent as ONE plain ADF paragraph, so markdown headings/bullets/checkboxes would show as raw text in Jira — evidence: rust/crates/worklog-core/src/collectors/jira.rs:353 (adf_doc) — confidence: high — unconfirmed
- A-01 Jira read/write calls already exist in core: search, create (with account field), transitions, transition, status, detail, comment — evidence: rust/crates/worklog-core/src/collectors/jira.rs:213,386,492,516,619,760 — confidence: high — unconfirmed
- A-02 Daemon already exposes them over HTTP: /tickets/search, /tickets/create, /tickets/:key/{detail,transitions,transition,comment,draft} — evidence: rust/crates/worklog-core/src/daemon.rs:147-217 — confidence: high — unconfirmed
- A-03 An AI draft already suggests at most one transition + a comment from the ticket's recent line texts (spec 012, web only) — evidence: rust/crates/worklog-core/src/task_draft.rs:1 — confidence: high — unconfirmed
- A-04 No `worklog ticket …` CLI subcommand exists; the CLI has no get/create ticket verbs — evidence: rust/crates/worklog-cli/src/cli.rs:86-538 — confidence: high — unconfirmed
- A-05 The bundled `worklog` skill is installed by `worklog skill install` from skills/worklog/ and has no Jira-ticket recipes — evidence: rust/crates/worklog-core/src/skill.rs:1, skills/worklog/SKILL.md:75 — confidence: high — unconfirmed
- A-06 Ticket creation reads the account field id from the `jira_account_field_id` secret (user's is customfield_11530); there is no account-clue learning — evidence: rust/crates/worklog-core/src/daemon.rs:1055 — confidence: high — unconfirmed
## Verify
- Fresh Claude Code chat: "let's work on GENAI-9129" → Claude runs `worklog ticket get GENAI-9129`, shows the ticket, and it moves to In Progress in Jira. Then "work on the Innnes SSO thing" (no ticket) → one yes/no with title + account → a new, well-formatted GENAI ticket appears in Jira (In Progress, account set). — user, Q14
## Open
- The LibreChat skill's ticket format is "awful"; user wants a good-looking one (Q7). Look is ungrillable → spec shows 2 rendered samples to pick from.
