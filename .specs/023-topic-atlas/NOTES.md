# Notes — 023 topic atlas

- Ruling: Base commit carries the new contract (Rust + TS) and empty module stubs; the crate does not compile until T002 rewrites stats_topics against it, so T003/T004 run after T002 (2026-10-11, orchestrator)
- Ruling: flow:build-slices workflow failed to start its per-task child (plugin reloaded mid-session); tasks dispatched as direct developer subagents, max 4–6 concurrent on disjoint files (2026-10-11, orchestrator)
- Ruling: T014, T015–T029 appended after reviews / live checks: compile unblock, review fixes (river hit-testing, label clipping, mirror tab stops, naming timeout/back-off/batches, RefreshSoon re-arm), live-data fixes (honest novelty warm-up, river trimming, heartbeats grid), design-evaluator fixes (round 1 composite 3.15) (2026-10-11, orchestrator)
- Progress (2026-10-11): T002–T029 ticked. Sandbox: DB copy /Users/tomas/.claude/jobs/491be4f8/tmp/wl, daemon :9343 (release, WORKLOG_SECRETS_FILE), helper :9334, web dev :3344. Screenshots: verify/atlas-shot2.mjs <outdir> 30d <dark|light> <width>. Claude-in-Chrome extension not connected (list_connected_browsers = []); checks in system Chrome via puppeteer. Remaining: evaluator ≥ 4.0, CHK001 evidence, gates, PR.
- Discovered: web/components/stats/TopicGalaxy.tsx hover-isolation list omits topic-list rows and .sx-gal-hit — defer
- Discovered: DayMirror.tsx / DayCompass.tsx / TopicGalaxy.tsx each carry their own topicHue helper — defer
- Discovered: web/app/stats/charts-time.css:28 `.st-svg .art-label` jumps to 18px at ≤900px for other charts — defer
- Discovered: click-to-pin cross-linking between atlas modules (design evaluator fix 12) — defer
- Discovered: web/components/stats/TopicRiver.tsx labelBox uses a fixed CHAR_PX estimate; overlap check approximate for wide glyphs — defer
- Discovered: rust/crates/worklog-core/src/topic_names.rs:55 parse_ids drops unparseable ids silently (corrupt members row could Jaccard-match) — defer
- Discovered: rust/crates/worklog-core/src/daemon_stats.rs is 402 lines, over the 400-line size guard — defer
