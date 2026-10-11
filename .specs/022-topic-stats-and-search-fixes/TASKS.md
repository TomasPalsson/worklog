Approved: 2026-10-11 by user (pre-approved: /flow:next --finish)
# Tasks — Topic stats and search fixes
Spec: spec.md · Design: design.md · Base: 8148ed7 · Route: dispatch · Test: `cargo test --manifest-path rust/Cargo.toml && (cd web && bun test)`

## Behaviors
| ID | Given / When / Then | Task | Proven by |
|----|---------------------|------|-----------|
| B1 (P0) | Given a block matching only some query words, when the Owner asks, then it is found, best match first | T001 | ask_test any_word_matches, best_match_first |
| B2 (P0) | Given a host dismissed ≥2× before and never labelled otherwise, when the Owner opens the day, then its group is tagged and can be bulk-dismissed with one confirm | T002, T003 | routing_noise_test, UnsortedList.test noise tag |
| B3 (P0) | Given the helper is up and ≥6 described blocks, when the Owner opens /stats, then Topics shows topics, switches, drift, catch-all tickets and similar days | T004–T008 | stats_topics_test, daemon_stats_test topics_ready, TopicsSection.test |
| B4 (P0) | Given the helper is down, when the Owner opens /stats, then Topics says so and every other section renders | T007, T008 | daemon_stats_test topics_helper_down, TopicsSection.test helper_down |
| B5 (P1) | Given vectors already stored, when /stats/topics runs again, then no text is re-embedded | T005 | embed_test second_call_sends_nothing |

## Phase 1 — Ask finds past work
Goal: `worklog ask` and the daemon's /ask match any non-stopword, best match first, without changing data.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core ask` — the CLI and daemon both route through `ask::search`, so the unit tests over a real in-memory DB with its FTS index cover both callers.
- [x] T001 Ask matches any word, best first (B1) — files: rust/crates/worklog-core/src/ask.rs, rust/crates/worklog-core/src/ask_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core ask` — done: 772879d

## Phase 2 — Noise groups flagged and cleared
Goal: the day view tags browser groups from always-dismissed hosts and dismisses them all with one confirmed click.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core routing_noise && (cd web && bun test components/UnsortedList.test.tsx)` — the query, the route and the tag-and-dismiss UI.
- [x] T002 Noise hosts query and route (B2) — files: rust/crates/worklog-core/src/routing_noise.rs, rust/crates/worklog-core/src/routing_noise_test.rs, rust/crates/worklog-core/src/routing.rs, rust/crates/worklog-core/src/daemon.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core routing_noise` — done: 36ca0f0
- [x] T003 Noise tag and bulk dismiss in the day view (B2) — files: web/lib/daemon.ts, web/app/[day]/page.tsx, web/components/UnsortedList.tsx, web/components/UnsortedList.test.tsx — verify: `cd web && bun test components/UnsortedList.test.tsx && bun run typecheck` — after: T002 — done: bace73c

## Phase 3 — Topics on the Statistics page
Goal: /stats shows a Topics section built from block vectors that the local helper computes, and degrades to a message when the helper is down.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core stats_topics && cargo test --manifest-path rust/Cargo.toml -p worklog-core embed && cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon_stats && (cd web && bun test components/stats/TopicsSection.test.tsx) && python3 rust/crates/worklog-core/templates/verdict_server.py --self-test` — helper route, vector store, topic math, daemon route and UI.
- [x] T004 [P] Helper /embed route (B3) — files: rust/crates/worklog-core/templates/verdict_server.py, rust/crates/worklog-core/src/verdict_supervisor.rs, rust/crates/worklog-core/src/verdict_supervisor_test.rs, rust/crates/worklog-cli/src/cli.rs — verify: `python3 rust/crates/worklog-core/templates/verdict_server.py --self-test && cargo test --manifest-path rust/Cargo.toml uv_args` — done: 218dbfc
- [x] T005 [P] Vector store and /embed client (B3, B5) — files: rust/crates/worklog-core/sql/schema.sql, rust/crates/worklog-core/src/embed.rs, rust/crates/worklog-core/src/embed_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core embed` — done: a7e7068
- [x] T006 [P] Topic math (B3) — files: rust/crates/worklog-core/src/stats_topics.rs, rust/crates/worklog-core/src/stats_topics_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core stats_topics` — done: 3b04efc
- [x] T007 /stats/topics route with helper-down fallback (B3, B4) — files: rust/crates/worklog-core/src/daemon_stats.rs, rust/crates/worklog-core/src/daemon_stats_test.rs, rust/crates/worklog-core/src/daemon.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon_stats` — after: T005, T006 — done: 5eb6398
- [x] T008 [P] Topics section on /stats (B3, B4) — files: web/lib/daemonStats.ts, web/lib/daemonTimeout.ts, web/components/stats/TopicsSection.tsx, web/components/stats/TopicsSection.test.tsx, web/app/stats/page.tsx — verify: `cd web && bun test components/stats/TopicsSection.test.tsx && bun run typecheck` — done: 7734637
- [ ] CHK001 Owner checks the noise tag and the Topics section in Chrome on real data — files: web/components/UnsortedList.tsx, web/components/stats/TopicsSection.tsx — verify: human: the day view shows "dismissed N× before" on always-dismissed hosts and /stats shows named topics with hours — after: T003, T007, T008
- [x] T009 Helper pins a sentence-transformers that can load the model (B3) — files: rust/crates/worklog-core/src/verdict_supervisor.rs, rust/crates/worklog-core/src/verdict_supervisor_test.rs, rust/crates/worklog-cli/src/cli.rs — verify: `cargo test --manifest-path rust/Cargo.toml uv_args` — after: T004 — done: 54e10e9
- [x] T010 Warm /stats/topics under 2 s: block text query starts from the block's events (B3) — files: rust/crates/worklog-core/src/embed.rs, rust/crates/worklog-core/src/embed_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core embed` — after: T007 — done: 16105f3

## Gates
- [ ] G001 project gates clean — files: . — verify: `flow check --fix --since 8148ed7`
- [ ] G002 branch review clean — files: . — verify: `flow pass`
- [ ] G003 verification evidence exists — files: . — verify: `test -s verify/`
