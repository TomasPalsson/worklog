Approved: 2026-10-11 by user (pre-approved: /goal directive, /flow:next --finish)
# Tasks — Topic atlas
Spec: spec.md · Design: design.md · Base: 9f27bd4 · Route: dispatch · Test: `cargo test --manifest-path rust/Cargo.toml && (cd web && bun test)`

## Behaviors
| ID | Given / When / Then | Task | Proven by |
|----|---------------------|------|-----------|
| B1 (P0) | Given anisotropic vectors, when topics are built, then groups separate and no topic exceeds 25% on the live copy | T002 | topic_metrics_test centered_split, replay |
| B2 (P0) | Given topics, when the report is built, then each has keywords and an AI or keyword name, cached and reused | T003, T005 | topic_terms_test, topic_names_test, daemon_stats_test naming_* |
| B3 (P0) | Given ≥10 blocks, when the report is built, then each block has a deterministic map position | T004 | topic_map_test |
| B4 (P0) | Given a report, when /stats renders, then the six graphics show with How lines, tips and keyboard focus | T006–T012 | component tests, CHK001 |
| B5 (P0) | Given the helper is down, when /stats renders, then the helper message shows and nothing is written | T005, T012 | daemon_stats_test, TopicsSection.test |

## Phase 1 — Honest, named themes on a map
Goal: /stats/topics returns centered-vector topics with keywords, cached AI names, map points, per-day metrics, river and highlights; the web shows them as the Topic atlas.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core topic && cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon_stats && (cd web && bun test components/stats)` — Rust math, naming cache, route and every atlas component together.
- [ ] T002 [P] Centered clustering + per-block/day metrics (B1) — files: rust/crates/worklog-core/src/stats_topics.rs, rust/crates/worklog-core/src/stats_topics_test.rs, rust/crates/worklog-core/src/topic_metrics.rs, rust/crates/worklog-core/src/topic_metrics_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core topic_metrics && cargo test --manifest-path rust/Cargo.toml -p worklog-core stats_topics`
- [ ] T003 [P] c-TF-IDF keywords + MMR examples (B2) — files: rust/crates/worklog-core/src/topic_terms.rs, rust/crates/worklog-core/src/topic_terms_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core topic_terms` — after: T002
- [ ] T004 [P] Exact t-SNE map (B3) — files: rust/crates/worklog-core/src/topic_map.rs, rust/crates/worklog-core/src/topic_map_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core topic_map` — after: T002
- [ ] T005 AI naming cache + route wiring (B2, B5) — files: rust/crates/worklog-core/src/topic_names.rs, rust/crates/worklog-core/src/topic_names_test.rs, rust/crates/worklog-core/sql/schema.sql, rust/crates/worklog-core/src/daemon_stats.rs, rust/crates/worklog-core/src/daemon_stats_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core topic_names && cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon_stats` — after: T002, T003, T004
- [ ] T013 Atlas art set (icons, empty states) (B4) — files: web/components/stats/AtlasArt.tsx, web/components/stats/AtlasArt.test.tsx — verify: `cd web && bun test components/stats/AtlasArt.test.tsx`
- [ ] T006 [P] Topic galaxy (B4) — files: web/components/stats/TopicGalaxy.tsx, web/components/stats/TopicGalaxy.test.tsx, web/app/stats/galaxy.css — verify: `cd web && bun test components/stats/TopicGalaxy.test.tsx` — after: T013
- [ ] T007 [P] Topic heartbeats (B4) — files: web/components/stats/TopicHeartbeats.tsx, web/components/stats/TopicHeartbeats.test.tsx, web/app/stats/heartbeats.css — verify: `cd web && bun test components/stats/TopicHeartbeats.test.tsx` — after: T013
- [ ] T008 [P] Topic river (B4) — files: web/components/stats/TopicRiver.tsx, web/components/stats/TopicRiver.test.tsx, web/app/stats/river.css, web/components/stats/ActivityStream.tsx — verify: `cd web && bun test components/stats/TopicRiver.test.tsx components/stats/ActivityStream.test.tsx` — after: T013
- [ ] T009 [P] Day mirror (B4) — files: web/components/stats/DayMirror.tsx, web/components/stats/DayMirror.test.tsx, web/app/stats/mirror.css — verify: `cd web && bun test components/stats/DayMirror.test.tsx` — after: T013
- [ ] T010 [P] Day compass (B4) — files: web/components/stats/DayCompass.tsx, web/components/stats/DayCompass.test.tsx, web/app/stats/compass.css — verify: `cd web && bun test components/stats/DayCompass.test.tsx` — after: T013
- [ ] T011 [P] First times & oddballs (B4) — files: web/components/stats/TopicMoments.tsx, web/components/stats/TopicMoments.test.tsx, web/app/stats/moments.css — verify: `cd web && bun test components/stats/TopicMoments.test.tsx` — after: T013
- [ ] T012 Topic atlas section assembly (B4, B5) — files: web/components/stats/TopicsSection.tsx, web/components/stats/TopicsSection.test.tsx, web/components/stats/RefreshSoon.tsx, web/app/stats/page.tsx — verify: `cd web && bun test components/stats/TopicsSection.test.tsx && bun run typecheck` — after: T006, T007, T008, T009, T010, T011
- [ ] CHK001 Owner-view check in Claude in Chrome on the installed app + design evaluator ≥ 4.0 — files: web/components/stats/TopicsSection.tsx — verify: human: all six graphics render with AI names, tips and How lines in light, dark and 375 px — after: T005, T012

## Gates
- [ ] G001 project gates clean — files: . — verify: `cargo fmt --manifest-path rust/Cargo.toml --all -- --check && cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings && cargo test --manifest-path rust/Cargo.toml && (cd web && bun test && bun run typecheck && bun run build)`
- [ ] G002 branch review clean — files: . — verify: `flow pass`
- [ ] G003 verification evidence exists — files: . — verify: `test -s .specs/023-topic-atlas/verify/acceptance.md`
