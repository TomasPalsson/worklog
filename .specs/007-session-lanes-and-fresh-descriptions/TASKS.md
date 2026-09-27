Approved: 2026-09-27 by user
Base: 88d1d77
# Tasks — Session lanes and fresh descriptions
Spec: spec.md · Design: design.md · Base: 88d1d77 · Route: dispatch · Test: `cargo test --manifest-path rust/Cargo.toml`

## Behaviors
| ID | Given / When / Then | Task | Proven by |
|----|---------------------|------|-----------|
| B1 (P0) | Given a tagged event, when lanes are keyed, then its key is `folder#customer`; untagged keys are unchanged | T001 | infer_lanes_test::tagged_event_gets_its_own_lane_key |
| B2 (P0) | Given two sessions in one repo naming different customers, when tagged, then each session's events carry its customer | T002 | session_customers_test::two_customers_in_one_repo_tag_both |
| B3 (P0) | Given sessions naming one or no customer, when tagged, then nothing is tagged | T002 | session_customers_test::one_customer_tags_nothing |
| B4 (P0) | Given interleaved two-customer sessions in one repo, when the day is rebuilt, then they land in separate blocks with the repo as folder | T003 | infer_allocations_db_test::two_customer_sessions_split_into_separate_blocks |
| B5 (P0) | Given a non-manual block whose length changed ≥30 min or >1.5× (grow or shrink), when re-inferred, then its description and estimated_by are cleared | T004 | infer_carry::tests::grown_block_drops_description |
| B6 (P0) | Given a manual description, when the block grows, then it is kept | T004 | infer_carry::tests::manual_description_always_kept |
| B7 (P1) | Given a ≥90-min block, when estimated, then the model is asked for up to 3 tasks | T005 | estimate::tests::long_block_asks_for_tasks |
| B8 (P0) | Given prompts stored as `claude_turn` rows titled "prompt" with the text in `raw_json`, when the day is rebuilt, then the prompt text resolves the session's customer | T007 | infer_allocations_db_test::prompt_text_names_the_session_customer |
| B9 (P0) | Given a folder split into customer lanes competing with another folder, when blocks are built, then each folder owns exactly the minutes it owned untagged, and only that folder's minutes are divided between its lanes | T008 | infer_lane_tags::tests::split_keeps_folder_minutes |

## Phase 1 — Session lanes
Goal: one repo serving two customers yields separate blocks per customer session.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core session` — green, and all existing infer tests still green.
- [x] T001 lane_tag field and tagged lane key (B1) — files: rust/crates/worklog-core/src/infer.rs, rust/crates/worklog-core/src/infer_lanes.rs, rust/crates/worklog-core/src/infer_lanes_test.rs, rust/crates/worklog-core/src/infer_evidence_test.rs, rust/crates/worklog-core/src/infer_allocations.rs, rust/crates/worklog-core/src/infer_allocations_test.rs, rust/crates/worklog-core/src/overlaps.rs, rust/crates/worklog-core/src/overlaps_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core infer` — done: cd54c05
- [x] T002 session customer tagging (B2, B3) — files: rust/crates/worklog-core/src/session_customers.rs, rust/crates/worklog-core/src/session_customers_test.rs, rust/crates/worklog-core/src/lib.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core session_customers` — after: T001 — done: 791b666
- [x] T003 wire tagging into the day rebuild (B4) — files: rust/crates/worklog-core/src/infer_allocations.rs, rust/crates/worklog-core/src/infer_allocations_db_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core two_customer_sessions_split` — after: T002 — done: 1b91eef
- [x] T007 session text includes the owner's prompt text (B8) — files: rust/crates/worklog-core/src/infer.rs, rust/crates/worklog-core/src/infer_allocations_db_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core prompt_text_names_the_session_customer` — after: T003 — done: 94865e4
- [x] T008 customer lanes split only the minutes their folder already owns (B9) — files: rust/crates/worklog-core/src/infer_lanes.rs, rust/crates/worklog-core/src/infer_lane_tags.rs, rust/crates/worklog-core/src/infer_lane_tags_test.rs, rust/crates/worklog-core/src/lib.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core infer_lane_tags` — after: T007 — done: 274a8db

## Phase 2 — Fresh descriptions
Goal: a block that changed a lot is described again, and long blocks read as their tasks.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core description` — green, `reinference_preserves_tempo_id_and_description` still green.
- [x] T004 drop stale descriptions on big change (B5, B6) — files: rust/crates/worklog-core/src/infer_carry.rs, rust/crates/worklog-core/src/infer.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core infer_carry` — after: T003 — done: 7d2dba0
- [x] T005 [P] long blocks described as tasks (B7) — files: rust/crates/worklog-core/src/estimate.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core long_block_asks_for_tasks` — done: 095120c
- [x] T006 Friday real-data check covers splits and block count — files: scripts/verify_inference_report.py — verify: `bash scripts/verify-inference.sh` — after: T004, T005, T007, T008 — done: 31104c0
- [x] CHK001 human-verify live Friday 2026-09-25 after deploy — files: scripts/verify_inference_report.py — verify: human: Owner sees vitinn-infra Sjúkra and APRÓ work in separate blocks, the 15:09 block(s) carry a fresh multi-task description, and the day still totals about 8.5 h — after: T006 — done: 31104c0 by user

## Gates
- [ ] G001 project gates clean — files: . — verify: `flow check --fix`
- [ ] G002 branch review clean — files: . — verify: `flow pass`
- [ ] G003 verification evidence exists — files: . — verify: `test -s .specs/007-session-lanes-and-fresh-descriptions/verify/`
