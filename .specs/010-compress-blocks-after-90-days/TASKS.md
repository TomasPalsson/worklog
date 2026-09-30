Approved: 2026-09-30 by user
Base: 98bbdf7
# Tasks — Compress blocks after 90 days
Spec: spec.md · Design: design.md · Base: e5aa122 · Route: dispatch · Test: `cargo test --manifest-path rust/Cargo.toml && (cd web && bun test)`

## Behaviors
| ID | Given / When / Then | Task | Proven by |
|----|---------------------|------|-----------|
| B1 (P0) | Given a fixture block with events, when its card is built, then eval fields equal block_eval's evidence and every cap in the contract holds | T001 | block_digest_test::card_matches_eval_evidence_and_caps |
| B2 (P0) | Given a personal block, when its card is built, then prompts, files, change titles, branches and tool counts are empty; given a `ghp_`-shaped token in a prompt, change title, branch or file, then it is scrubbed before truncation; eval titles stay unscrubbed | T001 | block_digest_test::personal_and_secrets |
| B3 (P0) | Given a compressed day, when re-infer (persist_blocks — covers POST /infer, allocation save/delete, `worklog infer --day`, `worklog day`), same-ticket merge or LLM re-description runs, then it fails with `day is compressed: <day>` and the day's blocks are byte-identical | T002 | infer::tests::compressed_day_refuses_rebuild, estimate::tests::compressed_day_refuses_merge_and_redescribe |
| B10 (P0) | Given a compressed day, when the daemon gets a refused action it answers 409; when it gets a hand edit (description, split, ignore, personal, ticket, mark exported) it succeeds | T004 | daemon tests compressed_day_allow_and_refuse |
| B4 (P0) | Given a block whose events were deleted after carding, when eval evidence and a billing export are computed, then both equal the pre-deletion results | T003 | billing_lifecycle_test::eval_and_billing_survive_compression |
| B5 (P0) | Given a carded block with no events, when GET /blocks/:id/digest and GET /days/:day run, then the card is returned and the day summary keeps its source count and path | T004 | daemon tests digest_route_and_summary |
| B6 (P1) | Given a card from the daemon, when the detail panel renders a block with no events, then the card's change titles and prompts are shown | T005 | BlockDetails.test.tsx card fallback |
| B7 (P0) | Given days older and newer than the horizon, when the run fires, then old blocks have cards, zero blocks are deleted, rows per spec §4.3 are gone (a block spanning midnight keeps its events; sessions only when no events remain; pins only after cards), new ones untouched; a second run the same day is a no-op; a card-build or snapshot failure leaves zero cards and zero deletes; default cycle settings delete no block; `worklog db purge --dry-run` prints horizon, carded, card bytes median/max, deleted counts, bytes freed and changes zero rows | T006 | purge tests compress_*, cli db_purge_dry_run_reports |
| B8 (P0) | Given collect with since older than the horizon, then since clamps to the horizon | T006 | cli b41_effective_since_clamps_forward_to_cutoff (updated) |
| B9 (P0) | Given `worklog eval "<q>" --details`, then each matched block's card fields print after the table | T007 | cli eval_details_prints_cards |
| B11 (P0) | Given a block with events, when its card is built, then `path_counts`/`invoice_title_counts` hold the event count of each listed path/title (same order), and `folder_path` is the most-used path whose project root is `folder` (ties: smallest), `None` when none maps | T008 | block_digest_test::card_counts_and_folder_path |
| B12 (P0) | Given two blocks on one export line — block 1 titles A×1, B×1; block 2 titles A×1, B×5 — when both are carded and their events deleted, then the line's title (B) and paths equal the pre-deletion export | T009 | billing_lifecycle_test::multi_block_titles_survive_compression |
| B13 (P0) | Given a work block whose most-used raw path lies outside its billed folder, when it is carded and its events deleted, then GET /days/:day `project_path` equals the pre-deletion value | T010 | daemon tests day_summary_path_survives_compression |

## Phase 1 — The card exists
Goal: any block can be turned into a small summary card that holds everything eval, billing and estimating need.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core block_digest` — green.
- [x] T001 card table and builder (B1, B2) — files: rust/crates/worklog-core/src/digest_contract.rs, rust/crates/worklog-core/src/block_digest.rs, rust/crates/worklog-core/src/block_digest_test.rs, rust/crates/worklog-core/sql/schema.sql, rust/crates/worklog-core/src/lib.rs, rust/crates/worklog-core/src/block_eval.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core block_digest` — done: 9eb411e

## Phase 2 — Old days are safe to compress
Goal: every screen and command works on a block whose raw events are gone, and nothing can rebuild an old day from nothing.
Independent test: `cargo test --manifest-path rust/Cargo.toml && (cd web && bun test)` — green, with purge still unchanged.
- [x] T002 [P] freeze compressed days (B3) — files: rust/crates/worklog-core/src/infer.rs, rust/crates/worklog-core/src/estimate.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core compressed_day` — after: T001 — done: 8d7af4c
- [x] T003 [P] eval and billing read the card (B4) — files: rust/crates/worklog-core/src/block_eval.rs, rust/crates/worklog-core/src/billing.rs, rust/crates/worklog-core/src/tenant_split.rs, rust/crates/worklog-core/src/tenant_clues.rs, rust/crates/worklog-core/src/personal.rs, rust/crates/worklog-core/src/billing_lifecycle_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core survive_compression` — after: T001 — done: 91a5c7e
- [x] T004 [P] daemon serves the card and maps refusals to 409 (B5, B10) — files: rust/crates/worklog-core/src/daemon.rs, rust/crates/worklog-core/src/block_details.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core -- digest_route compressed_day_allow_and_refuse` — after: T001 — done: 415c910
- [x] T007 [P] eval prints cards (B9) — files: rust/crates/worklog-cli/src/eval_cmd.rs, rust/crates/worklog-cli/tests/cli.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-cli eval_details` — after: T001 — done: 2f3442c
- [x] T005 detail panel shows the card (B6) — files: web/lib/daemon.ts, web/lib/types.ts, web/components/BlockDetails.tsx, web/components/BlockDetails.test.tsx — verify: `cd web && bun test BlockDetails && bun run typecheck` — after: T004 — done: 5ad9e36
- [x] T008 card keeps counts and the folder's path (B11) — files: rust/crates/worklog-core/src/block_digest.rs, rust/crates/worklog-core/src/block_digest_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core block_digest` — after: T001 — done: aa254ad
- [x] T009 [P] billing sums card counts across blocks (B12) — files: rust/crates/worklog-core/src/billing.rs, rust/crates/worklog-core/src/billing_lifecycle_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core survive_compression` — after: T008 — done: bde3ee1
- [x] T010 [P] day summary shows the card's folder path (B13) — files: rust/crates/worklog-core/src/daemon.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core day_summary_path` — after: T008 — done: 46fc939
- [x] T011 block page passes the card to the detail panel (B6) — files: web/app/[day]/block/[id]/page.tsx — verify: `cd web && bun test BlockDetails && bun run typecheck` — after: T005 — done: 2727cf0
- [x] T012 [P] multi-block billing parity test on built cards (B12) — files: rust/crates/worklog-core/src/billing_lifecycle_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core multi_block_titles_survive_compression` — after: T009 — done: cdfbde1

## Phase 3 — Compression runs
Goal: once a day, everything older than 90 days is squeezed into cards and the raw data is deleted; blocks are never deleted.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core purge` — green.
- [x] T006 compression run replaces block deletion (B7, B8) — files: rust/crates/worklog-core/src/purge.rs, rust/crates/worklog-core/src/daemon.rs, rust/crates/worklog-cli/src/cli.rs — verify: `cargo test --manifest-path rust/Cargo.toml -- purge compress effective_since` — after: T002, T003, T004, T007, T009, T010 — done: 3312c64
- [ ] CHK001 human-verify on a copy of your real database — files: rust/crates/worklog-core/src/purge.rs — verify: human: on a copy with `--days 3`, the dry-run shows card sizes (median ≤ 1 KB) and freed bytes, and one `worklog eval` query gives the same total before and after compressing — after: T006

## Gates
- [ ] G001 project gates clean — files: . — verify: `flow check --fix`
- [ ] G002 branch review clean — files: . — verify: `flow pass`
- [ ] G003 verification evidence exists — files: . — verify: `test -s verify/`
