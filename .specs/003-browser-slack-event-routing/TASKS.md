# Tasks — Browser + Slack events routed to the right block
Approved: 2026-09-23 by user
Spec: spec.md · Design: design.md · Base: 0fa7830 · Route: dispatch · Test: `cargo test --manifest-path rust/Cargo.toml && (cd web && bun test) && bun test extension/firefox`

## Behaviors
| ID | Given / When / Then | Task | Proven by |
|----|---------------------|------|-----------|
| B1 (P0) | Given a heartbeat inside work hours from a normal tab, when ingested, then one firefox event per minute is stored with url, title, container | T002 | browser_ingest::tests::stores_one_event_per_minute |
| B2 (P0) | Given a heartbeat that is incognito, in "Personal", or at 08:59/17:01, when ingested, then nothing is stored | T002 | browser_ingest::tests::filters_private_personal_and_off_hours |
| B3 (P0) | Given a request without a moz-extension Origin, when it hits /browser/heartbeat, then 403 and nothing stored | T003 | daemon::tests::heartbeat_rejects_web_origin |
| B4 (P0) | Given Slack messages the user sent, when collected twice, then each is stored once with channel + text | T004 | collectors::slack::tests::collect_is_idempotent |
| B5 (P0) | Given a matching hard rule, when a day is routed, then the event is labelled with origin rule | T005 | routing::tests::rule_beats_model |
| B6 (P0) | Given no rule and a model score ≥ threshold, when routed, then origin guess with the score; below threshold → unsorted | T005 | routing::tests::threshold_applies |
| B7 (P0) | Given an unsorted event, when blocks are inferred, then it is in no block; once labelled, it joins the project's block | T005 | routing::tests::unsorted_excluded_labelled_joins_block |
| B8 (P0) | Given the model helper unreachable, when routed, then every non-rule event stays unsorted and routing succeeds | T006 | laya::tests::unreachable_is_none |
| B9 (P0) | Given a relabel with always=domain, when saved, then a rule exists and other unsorted/guessed events on that domain get the project | T005 | routing::tests::always_creates_rule_and_applies |
| B10 (P0) | Given a container naming exactly one customer, when routed, then the model only sees that customer's folders | T005 | routing::tests::container_narrows_options |
| B11 (P0) | Given the add-on, when a normal tab is focused and the user active, then it posts once per 60 s; paused/idle/incognito/Personal → no post | T008 | extension/firefox/heartbeat.test.js |
| B12 (P0) | Given a day with routed events, when the day page loads, then unsorted events are listed and every routed event shows source + origin | T011 | web/components/UnsortedList.test.tsx |

## Phase 1 — Capture and storage
Goal: browser heartbeats and sent Slack messages land in the database, filtered and deduplicated.
Independent test: `cargo test --manifest-path rust/Cargo.toml browser_ingest slack db::` — green with no UI.
- [ ] T001 Schema v11 and module stubs — files: rust/crates/worklog-core/sql/schema.sql, rust/crates/worklog-core/src/db.rs, rust/crates/worklog-core/src/lib.rs, rust/crates/worklog-core/src/browser_ingest.rs, rust/crates/worklog-core/src/routing.rs, rust/crates/worklog-core/src/laya.rs, rust/crates/worklog-core/src/collectors/mod.rs, rust/crates/worklog-core/src/collectors/slack.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core db::`
- [ ] T002 [P] Heartbeat ingest with work-hours and privacy filters (B1, B2) — files: rust/crates/worklog-core/src/browser_ingest.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core browser_ingest` — after: T001
- [ ] T004 [P] Slack collector for the user's sent messages (B4) — files: rust/crates/worklog-core/src/collectors/slack.rs, rust/crates/worklog-core/src/secrets.rs, rust/crates/worklog-cli/src/cli.rs — verify: `cargo test --manifest-path rust/Cargo.toml slack` — after: T001

## Phase 2 — Routing
Goal: every browser/Slack event gets a project from a rule, a confident guess, or waits in unsorted — and labelled ones join the right block.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core routing laya` — green with the model helper absent.
- [ ] T005 [P] Router core: rules, container narrowing, threshold, labels, infer exclusion (B5, B6, B7, B9, B10) — files: rust/crates/worklog-core/src/routing.rs, rust/crates/worklog-core/src/infer.rs, rust/crates/worklog-core/src/billing.rs, rust/crates/worklog-core/src/billing_registry.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core routing` — after: T001
- [ ] T006 Laya client, helper script and `worklog laya serve|status` (B8) — files: rust/crates/worklog-core/src/laya.rs, rust/crates/worklog-core/templates/laya_server.py, rust/crates/worklog-cli/src/cli.rs — verify: `cargo test --manifest-path rust/Cargo.toml laya` — after: T001, T004
- [ ] T003 Daemon routes: heartbeat, routed events, label, rules, status, settings keys (B3) — files: rust/crates/worklog-core/src/daemon.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon::` — after: T002, T005, T006
- [ ] T007 Route before inference in `collect all` and `POST /infer` — files: rust/crates/worklog-cli/src/cli.rs, rust/crates/worklog-core/src/daemon.rs — verify: `cargo test --manifest-path rust/Cargo.toml` — after: T003, T004, T006

## Phase 3 — Firefox add-on
Goal: an installable add-on that reports focused, active, work-hours tab time and can be paused.
Independent test: `bun test extension/firefox` — green with no daemon running.
- [ ] T008 [P] Firefox add-on: heartbeat logic, container/incognito skip, idle, pause popup (B11) — files: extension/firefox/manifest.json, extension/firefox/background.js, extension/firefox/heartbeat.js, extension/firefox/heartbeat.test.js, extension/firefox/popup.html, extension/firefox/popup.js, extension/firefox/README.md — verify: `bun test extension/firefox`
- [ ] CHK001 human-verify the signed add-on installs in normal Firefox and survives a restart — files: extension/firefox/manifest.json — verify: human: user runs `web-ext sign --channel unlisted` with their AMO keys, installs the .xpi, restarts Firefox, the add-on is still enabled — after: T008

## Phase 4 — Review UI
Goal: the user sees every browser/Slack event with its source, sorts the unsorted ones, and manages rules and settings.
Independent test: `cd web && bun test && bun run typecheck` — green against a stubbed daemon.
- [ ] T010 Web types, daemon client and server actions — files: web/lib/types.ts, web/lib/daemon.ts, web/app/actions.ts, web/lib/types.test.ts — verify: `cd web && bun test lib/types.test.ts && bun run typecheck` — after: T003
- [ ] T011 [P] Unsorted list, label picker with "always", source + origin badges (B12) — files: web/components/UnsortedList.tsx, web/components/UnsortedList.test.tsx, web/components/EventList.tsx, web/components/SourceBadges.tsx, web/app/[day]/page.tsx — verify: `cd web && bun test components/UnsortedList.test.tsx` — after: T010
- [ ] T012 [P] Settings: work hours, threshold, Slack token, rules list, source status; Billing label copy for named projects — files: web/components/SettingsPanel.tsx, web/components/SettingsPanel.test.tsx, web/components/BillingRegistry.tsx — verify: `cd web && bun test components/SettingsPanel.test.tsx` — after: T010
- [ ] CHK002 human-verify the day page with real data — files: web/components/UnsortedList.tsx — verify: human: user sees a day's Firefox and Slack events with container/channel and rule/fix/guess tags, sorts one unsorted event, and it moves into a block — after: T011, T012, T007

## Gates
- [ ] G001 project gates clean — files: . — verify: `flow check --fix`
- [ ] G002 branch review clean — files: . — verify: `test -f PASS-$(git rev-parse --short HEAD).md`
- [ ] G003 verification evidence exists — files: . — verify: `test -s verify/`
