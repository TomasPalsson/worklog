Approved: 2026-10-12 by user (pre-approved: /goal directive)
# Tasks — Topic star field
Spec: spec.md · Design: design.md · Base: ff1224b · Route: dispatch · Test: `cargo test --manifest-path rust/Cargo.toml && (cd web && bun test)`

## Behaviors
| ID | Given / When / Then | Task | Proven by |
|----|---------------------|------|-----------|
| B1 (P0) | Given centered vectors, when topics are built, then k is the best-silhouette cut with no topic under 3 blocks, up to 20 | T101 | stats_topics_test choose_k_* |
| B2 (P0) | Given topic labels, when the map is laid out, then no topic is split into separate map groups, in 2-D and 3-D | T102, T103 | topic_map_test split_fixture, daemon_stats_test star_positions |
| B3 (P0) | Given up to 20 topics, when colours are assigned, then neighbouring ids stay distinguishable everywhere | T104 | topicColor.test |
| B4 (P0) | Given a report, when the owner opens Star field, then a 3-D field renders and flies with WASD, drag, hover, click, reset | T105, T106 | starfield.test, StarField.test, TopicGalaxy.test, CHK001 |

## Phase 1 — Topics that hold together
Goal: /stats/topics picks k by silhouette and returns gravity-held 2-D and 3-D positions.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core stats_topics && cargo test --manifest-path rust/Cargo.toml -p worklog-core topic_map && cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon_`
- [ ] T101 [P] Topic count chosen by silhouette over one merge history (B1) — files: rust/crates/worklog-core/src/stats_topics.rs, rust/crates/worklog-core/src/stats_topics_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core stats_topics`
- [ ] T102 [P] Cluster gravity and 3-D layout (B2) — files: rust/crates/worklog-core/src/topic_map.rs, rust/crates/worklog-core/src/topic_map_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core topic_map`
- [ ] T103 Daemon fills star positions; topics pipeline split into daemon_topics.rs (B2) — files: rust/crates/worklog-core/src/daemon_stats.rs, rust/crates/worklog-core/src/daemon_stats_test.rs, rust/crates/worklog-core/src/daemon_topics.rs, rust/crates/worklog-core/src/daemon_topics_test.rs, rust/crates/worklog-core/src/daemon.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon_stats && cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon_topics` — after: T101, T102

## Phase 2 — Star field
Goal: the galaxy card toggles to a flyable 3-D star field.
Independent test: `cd web && bun test components/stats && bun run typecheck`
- [ ] T104 [P] Topic colours for up to 20 topics (B3) — files: web/components/stats/topicColor.ts, web/components/stats/topicColor.test.ts, web/components/stats/TopicGalaxy.test.tsx, web/components/stats/DayCompass.test.tsx, web/components/stats/DayMirror.test.tsx, web/components/stats/TopicHeartbeats.test.tsx, web/components/stats/TopicRiver.test.tsx — verify: `cd web && bun test components/stats`
- [ ] T105 [P] Star field scene, camera and canvas view (B4) — files: web/components/stats/starfield.ts, web/components/stats/starfield.test.ts, web/components/stats/StarField.tsx, web/components/stats/StarField.test.tsx, web/app/stats/starfield.css — verify: `cd web && bun test components/stats/starfield.test.ts components/stats/StarField.test.tsx && bun run typecheck`
- [ ] T106 Map / Star field toggle on the galaxy card (B4) — files: web/components/stats/TopicGalaxy.tsx, web/components/stats/TopicGalaxy.test.tsx, web/app/stats/galaxy.css — verify: `cd web && bun test components/stats/TopicGalaxy.test.tsx && bun run typecheck` — after: T104, T105

## Checks
- [ ] CHK001 Live-copy check: k, split topics = 0, nearest-same ≥ 0.95 on API output; Star field flown in Chrome (WASD, drag, hover, click, reset), dark + light, 0 console errors — verify: `test -s .specs/024-topic-starfield/verify/CHK001.md`

## Gates
- [ ] G001 Full test suite, lint, build — verify: `cargo fmt --manifest-path rust/Cargo.toml --all -- --check && cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings && cargo test --manifest-path rust/Cargo.toml && cd web && bun test && bun run typecheck && bun run build`
- [ ] G002 Branch review, findings fixed — verify: `test -n "$(ls .specs/024-topic-starfield/review/ 2>/dev/null)"`
