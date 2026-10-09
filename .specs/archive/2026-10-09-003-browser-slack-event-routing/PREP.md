# Prep — Browser + Slack events routed to the right block
Gathered: 2026-09-23 · Questions: 17 of 12 (user lifted the cap: "don't feel capped") · Route: dispatch · Status: ready for spec
Lint override: prep-lint rejects 17 > 12 questions; user chose to keep one PREP.md over splitting — user, 2026-09-23

## Decisions
- D-01 The router labels each browser/Slack event with a project; worklog's normal inference (`split_by_project`) builds the blocks. It never picks a block id. — user, Q1
- D-02 Besides `~/Desktop/Work` folders, the user can create named projects in the UI (e.g. "AWS cert"); a named project takes a customer + Verkefni pin exactly like a folder. — user, Q2
- D-03 Browsing comes from a Firefox extension that sends active-tab heartbeats (tab focused, user not idle) to the local daemon on 127.0.0.1:9323; the Firefox history file is not used. — user, Q3
- D-04 Each heartbeat stores the full URL and page title, locally only; private windows are never recorded. — user, Q4
- D-05 The extension records only inside work hours (default Mon–Fri 09:00–17:00, user said "9-17ish" so it must be editable) and has a pause button on its toolbar icon. — user, Q5
- D-06 Each heartbeat also stores the Firefox container name (Multi-Account Containers); it is a routing signal, e.g. to tell AWS console accounts apart. — user, Q5
- D-07 A container stands for one AWS account (usually one customer, but a customer can have several, e.g. apro-*); it narrows routing to that customer's projects but does not pick the project alone. Only the fingerprint (account) containers and "Personal" matter; the rest (Work, Banking, Shopping, Facebook, Apro Sandbox) are unused. — user, Q6
- D-08 Tabs in the "Personal" container are never recorded, same as private windows. — user, Q7
- D-09 Slack collects only messages the user sent (channels + DMs), storing channel name and text, via a Slack user token kept in the keychain (`secrets::KNOWN_KEYS`). — user, Q8
- D-10 Every user fix (event → project) is stored as a labelled example (few-shot hints now, fine-tuning data later). A fix becomes a hard rule only when the user ticks "always for this site / channel / container"; hard rules are checked in code before Laya. — user, Q10
- D-11 Laya's label is applied only at ≥90% probability (editable setting) and marked "guessed"; below that the event goes to an "unsorted" list for the user. — user, Q11
- D-12 Blocks built from guessed browser/Slack events go into the billing export like any other block: no accept step, no extra flag. The user reviews every line while copying it into the invoice form by hand. Customer/Verkefni still come only from pins (CLAUDE.md invariant unchanged). — user, Q12
- D-13 The extension source lives in this repo and ships as an unlisted, Mozilla-signed `.xpi` (installs permanently in normal Firefox). — user, Q14
- D-14 The user creates the Slack app (user token) in the APRÓ workspace themselves; no admin approval step is needed. — user, Q15
- D-15 Slack is collected at all hours — every Slack message the user sends counts as work; the work-hours window applies to the extension only. — user, Q16
- D-16 All collected data is visible in the review UI: every browser heartbeat and Slack message, its source (Firefox / container / Slack channel), and whether its project label came from a hard rule, a user fix, or a Laya guess. — user, Q17
## Not this
- Removing Jira from worklog (separate cleanup). — user, Q13
- Fixing the export's Teg. vinnu / Hvað / Akstur fields (Núll verð, Námskeið, Veikindi). — user, Q13
- Fine-tuning Laya. — user, Q13
- Browsers other than Firefox. — user, Q13
- Reading Slack messages the user received. — user, Q13
- Importing Firefox's existing history. — user, Q13
## Discretion
- Heartbeat interval, idle threshold, and how heartbeats become events (e.g. merged runs) — must fit infer's gap-timeout clustering.
- Where the "unsorted" list and the fix / "always" controls live in the review UI.
- Browser/Slack events follow the existing purge rules (`purge.rs`).
- Laya runtime (user delegated, Q9: "whatever you think is best"). Recommendation: optional Python sidecar installed by `worklog setup` via `uv`, called over localhost; if absent, unknown events go to "unsorted" and nothing else breaks. No Python becomes required for the core binary.
## Assumptions
- A-15 Account containers in use: rl-prod, apro-demo, sjukra-prod, ru-prod, apro-datalake, apro-datalake-sandbox, sensa-prod-mfa, p5-sso-tomas, byko-prod, apro-web, mms-prod; likely ru=Háskólinn í Reykjavík, mms=Menntaskólinn við Sund, sjukra=Sjúkratryggingar Íslands, sensa=Sensa ehf — evidence: user screenshot 4 — confidence: medium — unconfirmed
- A-14 Browser is Firefox; the goal is time on a site such as https://aws.tomasari.is/ — evidence: none — confidence: high — confirmed Q2 (user)
- A-01 Events carry `project_path`; blocks have no project column — a block's project is derived from its linked events (`block_events` junction) — evidence: rust/crates/worklog-core/sql/schema.sql:18, :54, :83 — confidence: high — unconfirmed
- A-02 `infer::split_by_project` cuts a time cluster at project_path transitions; events with no project_path inherit the surrounding project. So labelling a browser/Slack event with a project folder routes it into that project's block with no separate block-picking step — evidence: rust/crates/worklog-core/src/infer.rs:201, :284 — confidence: medium — confirmed Q1
- A-03 Blocks are rebuilt on every re-inference and matched to old ones by `started_at`; a stored "event → block id" link would go stale, a stored "event → project" label survives — evidence: rust/crates/worklog-core/src/infer.rs:428, :480 — confidence: high — confirmed Q1
- A-04 Blocks never overlap in time by construction (one sorted event stream per day) — evidence: rust/crates/worklog-core/src/infer.rs:159 — confidence: medium — unconfirmed
- A-05 Billing pins are keyed by project-root folder; `Verkefni` comes only from a folder pin, customer from pin or unambiguous alias — evidence: rust/crates/worklog-core/src/billing_registry.rs:176-192 — confidence: high — unconfirmed
- A-06 No correction/feedback storage exists today — evidence: rust/crates/worklog-core/sql/schema.sql — confidence: medium — unconfirmed
- A-07 No "move event to another block" daemon route exists (ticket, duration, description, delete, personal, split, merge do) — evidence: rust/crates/worklog-core/src/daemon.rs:90-124 — confidence: medium — unconfirmed
- A-08 The shipped product has no Python: signed Rust binary via install.sh + bun Docker web container — evidence: install.sh:1-50, web/Dockerfile:1-45, .github/workflows/release.yml:1-40 — confidence: high — unconfirmed
- A-09 Tokens live in the OS keychain via `secrets::KNOWN_KEYS`; a Slack token would be one more key — evidence: rust/crates/worklog-core/src/secrets.rs:15, :171 — confidence: high — unconfirmed
- A-10 Collectors run from launchd/systemd as `worklog collect all` — evidence: rust/crates/worklog-core/src/schedule.rs:189 — confidence: high — unconfirmed
- A-11 Past invoice logs: much logged time has no code trail (cert study "Námskeið", SOW writing, meetings, sales), and one customer on one day often splits across several Verkefni (e.g. VÍS 15.09: áskrift + sérverkefni) — evidence: user screenshots 1-3 (2026-06-22 → 2026-09-22) — confidence: high — corrected Q2 (user: only AWS cert study and meetings have no Claude Code trail; SOW and sales work do have one; meetings already come from gcal)
- A-12 Export fills constant `Dagvinna` / `Almenn skráning`; the real form also uses `Núll verð`, `Námskeið`, `Veikindi`, `Akstur` — evidence: rust/crates/worklog-core/src/billing.rs:6-16 vs screenshots — confidence: high — unconfirmed
- A-13 Jira still drives billing task grouping and the personal override though the user no longer uses Jira — evidence: rust/crates/worklog-core/src/billing.rs:394, rust/crates/worklog-core/src/personal.rs:238 — confidence: high — unconfirmed
## Verify
- Study on https://aws.tomasari.is/ for 1h in a normal tab → that day's review UI shows a ~1h block under the "AWS cert" project; send a Slack message in a customer channel → it lands in that customer's block; every browser/Slack event and its source is visible in the UI; `cargo test --manifest-path rust/Cargo.toml` and `cd web && bun test` are green. — user, Q17
## Open
