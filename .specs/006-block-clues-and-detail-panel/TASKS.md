# Tasks — Block clues, attribution and detail view
Approved: 2026-09-27 by user
Spec: spec.md · Design: design.md · Base: 7952c55 · Route: dispatch · Test: `cargo test --manifest-path rust/Cargo.toml && (cd web && bun test)`

## Behaviors
| ID | Given / When / Then | Task | Proven by |
|----|---------------------|------|-----------|
| B1 (P0) | Given a personal-owner commit on GitHub, when collection runs, then nothing is stored | T002 | github personal_owner_is_skipped |
| B2 (P0) | Given an org commit whose sha is in a local clone, when collected, then it gets that clone's folder | T003 | local_clone sha_present_sets_folder |
| B3 (P0) | Given an org commit whose sha is in no clone, when a day is inferred, then it is in no block and listed as elsewhere | T003, T004, T006 | infer elsewhere_event_joins_no_block |
| B4 (P0) | Given a folderless event inside a block span, when inferred, then the block's project and bounds are unchanged | T004 | infer folderless_never_votes_or_extends |
| B5 (P0) | Given the upgrade, when it runs, then every day is re-inferred and `exported_at`/Tempo id/manual text survive | T005 | upgrade_006 carries_all_fields |
| B6 (P0) | Given a value holding a token, when stored or sent, then it reads `[secret]` | T007, T018 | scrub fixtures; clues_send no_forbidden_fields |
| B7 (P1) | Given helper activity, when a block is inferred, then its duration is unchanged | T004, T011 | infer helper_adds_no_time |
| B8 (P1) | Given a block, when the owner opens Details, then one filterable timeline with folded sessions shows | T016, T017 | detailRows.test.ts; BlockDetails.test.tsx |
| B9 (P0) | Given a billing line, when texts are generated, then it holds 2–3 Icelandic sentences with no numbers/paths | T020 | line_text validate_* |
| B10 (P0) | Given a hand-edited line text, when generation re-runs, then it is unchanged | T020 | line_text manual_never_overwritten |

## Phase 1 — Attribution (phase A, own PR)
Goal: every event is in the right project; personal GitHub is gone; foreign-machine commits are listed, not billed; every stored day is rebuilt.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core infer && cargo test --manifest-path rust/Cargo.toml -p worklog-core upgrade_006` — green with collectors B–D untouched.
- [x] T001 Contract, schema, migration and empty module files — files: rust/crates/worklog-core/src/clues_contract.rs, rust/crates/worklog-core/src/lib.rs, rust/crates/worklog-core/src/collectors/mod.rs, rust/crates/worklog-core/sql/schema.sql, rust/crates/worklog-core/src/db.rs, rust/crates/worklog-core/src/scrub.rs, rust/crates/worklog-core/src/local_clone.rs, rust/crates/worklog-core/src/upgrade_006.rs, rust/crates/worklog-core/src/elsewhere.rs, rust/crates/worklog-core/src/block_details.rs, rust/crates/worklog-core/src/clues_send.rs, rust/crates/worklog-core/src/line_text.rs, rust/crates/worklog-core/src/collectors/claude_tools.rs, rust/crates/worklog-core/src/collectors/claude_helpers.rs, web/lib/types.ts — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core db` — done: b6ad1da
- [x] T002 Skip personal-owner commits and PRs at collection (FR-01, B1) — files: rust/crates/worklog-core/src/collectors/github.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core collectors::github` — after: T001 — done: 7fe7cb3
- [x] T003 Repo → local folder and sha presence; set project_path or elsewhere on commits/PRs (FR-03, FR-04, B2) — files: rust/crates/worklog-core/src/local_clone.rs, rust/crates/worklog-core/src/collectors/github.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core local_clone` — after: T002 — done: a681432
- [x] T004 [P] Folderless events placed only inside existing spans; elsewhere + helper/message sources never vote, extend or add time (FR-07, FR-08, FR-09, FR-17, B3, B4, B7) — files: rust/crates/worklog-core/src/infer_lanes.rs, rust/crates/worklog-core/src/infer.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core infer` — after: T001 — done: 9635d25
- [x] T005 Upgrade step: delete personal-owner rows, carry `exported_at`, re-infer every stored day once (FR-02, FR-10, B5) — files: rust/crates/worklog-core/src/upgrade_006.rs, rust/crates/worklog-core/src/db.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core upgrade_006` — after: T003, T004 — done: 5ae6e37
- [x] T006 Done-elsewhere list per day + move into a block (FR-05, FR-06) — files: rust/crates/worklog-core/src/elsewhere.rs, rust/crates/worklog-core/src/daemon.rs, web/lib/daemon.ts, web/app/actions.ts, web/components/ElsewhereList.tsx, web/components/ElsewhereList.test.tsx, web/app/[day]/page.tsx — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core elsewhere && (cd web && bun test components/ElsewhereList.test.tsx)` — after: T005 — done: 77e36b1
- [x] CHK001 Owner checks phase A on real data after the phase-A PR merges (D-01) — files: . — verify: human: owner opens 2026-09-24 and 2026-09-25 and confirms no worklog/flow/ads-seo commit sits in any block and every commit is in the right project or "done elsewhere" — after: T006 — done: 77e36b1 by user

## Phase 2 — Capture everything locally (phase B)
Goal: every existing source stores what it reads, secret-scrubbed, including helper activity.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core collectors scrub` — green with the UI untouched.
- [x] T007 Secret scrubber (FR-11, B6) — files: rust/crates/worklog-core/src/scrub.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core scrub` — after: CHK001 — done: fe87315
- [x] T008 [P] Shell: full command + cwd into RawRecord::Shell (FR-12) — files: rust/crates/worklog-core/src/collectors/fish.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core collectors::fish` — after: T007 — done: eb2aa5a
- [x] T009 [P] Reflog: full message into RawRecord::Reflog (FR-13) — files: rust/crates/worklog-core/src/collectors/reflog.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core collectors::reflog` — after: T007 — done: d4cd3d8
- [x] T010 [P] Transcripts: prompt text, tool inputs, outputs capped at 2 KB, files (FR-14, FR-15) — files: rust/crates/worklog-core/src/collectors/claude_tools.rs, rust/crates/worklog-core/src/collectors/claude_transcripts.rs, rust/crates/worklog-core/src/collectors/claude_transcripts_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core collectors::claude` — after: T007 — done: 488d59b
- [x] T011 Helper activity and session messages linked to parent session by transcript session/turn id, never cwd (FR-16, FR-18, B7) — files: rust/crates/worklog-core/src/collectors/claude_helpers.rs, rust/crates/worklog-core/src/collectors/claude_transcripts.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core collectors::claude_helpers` — after: T010 — done: c393363
- [x] T012 [P] Hook payload into RawRecord::Hook, scrubbed; cwd collapsed like the transcripts (A11) — files: rust/crates/worklog-core/src/hook_run.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core hook_run` — after: T007 — done: 16fda37
- [x] T013 [P] Commit/PR body into RawRecord::Commit — files: rust/crates/worklog-core/src/collectors/github.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core collectors::github` — after: T007 — done: f051ccb
- [x] T014 Measure §5 on real data: DB growth for 2026-09-25 ≤ 5 MB, 0 scrubber-pattern hits in events — files: scripts/verify-006-capture.sh — verify: `bash scripts/verify-006-capture.sh` — after: T008, T009, T011, T012, T013 — done: c6b6b3b

## Phase 3 — Details view (phase C)
Goal: from any block, the owner opens one filterable timeline showing every stored detail.
Independent test: `cd web && bun test lib/detailRows.test.ts components/BlockDetails.test.tsx` — green against fixture data.
- [x] T015 Block details query + `GET /blocks/:id/details` (FR-20) — files: rust/crates/worklog-core/src/block_details.rs, rust/crates/worklog-core/src/daemon.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core block_details` — after: T014 — done: 315c3e4
- [x] T016 Detail rows: fold sessions prompt → tools → files, nest helpers, source filter (FR-21, FR-22, FR-23) — files: web/lib/detailRows.ts, web/lib/detailRows.test.ts, web/lib/daemon.ts — verify: `cd web && bun test lib/detailRows.test.ts` — after: T015 — done: 198fd60
- [x] T017 Details page + "Details" control on the block card; inline list untouched (FR-19, FR-24, FR-25, FR-34) — files: web/app/[day]/block/[id]/page.tsx, web/components/BlockDetails.tsx, web/components/BlockDetails.test.tsx, web/components/BlockCard.tsx, web/app/globals.css — verify: `cd web && bun test components/BlockDetails.test.tsx components/BlockCard.test.tsx && bun run typecheck` — after: T016 — done: 4a3acb9
- [x] CHK002 Owner reviews the Details view on 2026-09-25 — files: web/components/BlockDetails.tsx — verify: human: owner opens the vitinn-infra block's Details and confirms every prompt, tool call and shell command in its span is there and readable — after: T017 — done: 4a3acb9 by user

## Phase 4 — Billing-line texts (phase D)
Goal: each billing line carries 2–3 Icelandic sentences a boss understands, built only from the D-02 send-list.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core clues_send line_text billing` — green with a fake estimator.
- [x] T018 D-02 input builders; no forbidden field ever serialized (FR-28, B6) — files: rust/crates/worklog-core/src/clues_send.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core clues_send` — after: CHK002 — done: 27e4a77
- [ ] T019 Per-block writer sends only `DescriptionInput` (FR-29, A13 leak fix) — files: rust/crates/worklog-core/src/estimate.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core estimate` — after: T018
- [ ] T020 Line texts: Icelandic prompt, validate, generate per day, manual never overwritten, failure keeps previous text (FR-26, FR-27, FR-31, FR-35, B9, B10) — files: rust/crates/worklog-core/src/line_text.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core line_text` — after: T018
- [ ] T021 Export uses the stored line text, else today's joined/fallback text (FR-32) — files: rust/crates/worklog-core/src/billing.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core billing` — after: T020
- [ ] T022 Edit + regenerate a line text from the billing UI, show the not-generated flag (FR-30, FR-33, FR-35) — files: rust/crates/worklog-core/src/daemon.rs, web/lib/daemon.ts, web/app/actions.ts, web/components/BillingGroup.tsx, web/components/BillingGroup.test.tsx — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon && (cd web && bun test components/BillingGroup.test.tsx)` — after: T019, T021
- [ ] CHK003 Owner's PREP Verify on 2026-09-25 — files: . — verify: human: no personal-account commit in any work block; the vitinn-infra Details view shows every prompt, tool call and shell command; its billing line reads as 2–3 Icelandic sentences the owner would send to their boss unedited — after: T022

## Gates
- [ ] G001 project gates clean — files: . — verify: `flow check --fix`
- [ ] G002 branch review clean — files: . — verify: `flow pass`
- [ ] G003 verification evidence exists — files: . — verify: `test -s verify/`
