Approved: 2026-09-26 by user
Base: f9b8044
# Tasks — Billing deildir and super blocks
Spec: spec.md · Design: design.md · Base: 370947d · Route: dispatch · Test: `cargo test --manifest-path rust/Cargo.toml && (cd web && bun test)`

## Behaviors
| ID | Given / When / Then | Task | Proven by |
|----|---------------------|------|-----------|
| B1 (P0) | Given a customer, when the Owner adds deildir with keywords, then they persist and list under it (FR-01) | T002, T004 | billing_deildir::tests::round_trip, BillingCustomerSection.test |
| B2 (P0) | Given a deild name on a customer, when it is added again, then the save is refused (FR-02) | T002 | billing_deildir::tests::duplicate_refused |
| B3 (P0) | Given a block, when it resolves, then each slice's deild follows manual → keyword → folder default → blank (FR-03) | T006 | billing::tests::deild_ladder_* |
| B4 (P0) | Given any work block, when the Owner saves Sjúkra·Rekstur 33 / Sjúkra·Áskrift 33 / APRÓ 34, then it reloads identically (FR-04) | T005, T007, T008 | tenant_shares::tests::rows_round_trip, BlockCustomerSplit.test |
| B5 (P0) | Given rows totalling 90% or a row with no customer, when saving, then nothing is written (FR-05) | T005, T008 | tenant_shares::tests::validate_rows_*, BlockCustomerSplit.test |
| B6 (P0) | Given the 2026-09-25 shape, when rows_for_day runs, then one line per (customer, deild), blank deild split by folder, tickets never split (FR-06) | T006 | billing::tests::super_blocks_group_by_customer_and_deild |
| B7 (P0) | Given overlapping slices on one line, when hours are computed, then overlap counts once (FR-07) | T006 | billing::tests::super_block_hours_are_a_union |
| B8 (P0) | Given a hand-set split, when estimate runs, then the split is byte-identical (FR-08) | T011 | estimate::tests::estimate_leaves_manual_split |
| B9 (P0) | Given an automatic write that moves a customer/deild/split/description, when it commits, then one change row per field with source + batch (FR-09) | T010, T011 | change_log::tests::*, writer tests |
| B10 (P0) | Given the page open, when a batch of 20 changes lands, then exactly one pop-up naming its source (FR-10) | T013 | ChangeNotices.test |
| B11 (P0) | Given unseen changes, when the app opens, then a catch-up shows them and opening marks them seen (FR-11) | T012, T013 | daemon change route test, ChangeNotices.test |
| B12 (P0) | Given a rebuild that only adds a block, when it commits, then nothing is logged (FR-12) | T010 | change_log::tests::new_block_logs_nothing |
| B13 (P1) | Given a super block, when the Owner picks another deild on its header, then every slice on that line moves (FR-13) | T009 | daemon_tenants::tests::move_line_deild |
| B15 (P0) | Given the Owner edits a split, when it saves, then a change with source "you" is logged and appears in the catch-up but never as a pop-up (FR-15) | T011, T013 | block_service/daemon writer test, ChangeNotices.test |
| B14 (P1) | Given folder pins with a Verkefni, when the DB upgrades, then those become the pinned customer's deildir (FR-14) | T001 | db::tests::v14_seeds_deildir_from_pins |

## Phase 1 — Deildir exist
Goal: the Owner can keep a list of deildir with keywords under each customer.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core billing_deildir` — green with the billing view untouched.
- [ ] T001 Contract files and schema v14 (B14) — files: rust/crates/worklog-core/src/deild_contract.rs, rust/crates/worklog-core/src/billing_deildir.rs, rust/crates/worklog-core/src/change_log.rs, rust/crates/worklog-core/src/lib.rs, rust/crates/worklog-core/sql/schema.sql, rust/crates/worklog-core/src/db.rs, web/lib/deildir.ts — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core db::`
- [ ] T002 Deild registry functions (B1, B2) — files: rust/crates/worklog-core/src/billing_deildir.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core billing_deildir` — after: T001
- [ ] T003 Deild daemon routes — files: rust/crates/worklog-core/src/daemon.rs, rust/crates/worklog-core/src/billing_registry.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon::tests::deild` — after: T002
- [ ] T004 Deildir editor in Settings → Billing (B1) — files: web/components/BillingCustomerSection.tsx, web/components/BillingCustomerSection.test.tsx, web/lib/types.ts, web/lib/daemon.ts, web/app/actions.ts, web/lib/useBillingRegistry.ts — verify: `cd web && bun test components/BillingCustomerSection.test.tsx && bun run typecheck` — after: T003

## Phase 2 — Super blocks and splits
Goal: every (customer, deild) is one billing line, and any block can be split by % across (customer, deild) rows.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core billing` — green with notices untouched.
- [ ] T005 [P] Split storage v2 (B4, B5) — files: rust/crates/worklog-core/src/tenant_shares.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core tenant_shares` — after: T001
- [ ] T006 Deild resolution and super-block grouping (B3, B6, B7) — files: rust/crates/worklog-core/src/billing.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core billing::` — after: T002, T005
- [ ] T007 Split routes v2 for every work block (B4) — files: rust/crates/worklog-core/src/daemon_tenants.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon_tenants` — after: T006
- [ ] T008 Split editor rows customer + deild + % (B4, B5) — files: web/components/BlockCustomerSplit.tsx, web/components/BlockCustomerSplit.test.tsx, web/app/tenant-actions.ts — verify: `cd web && bun test components/BlockCustomerSplit.test.tsx && bun run typecheck` — after: T007, T004
- [ ] T009 Move a super block's deild from its header (B13) — files: rust/crates/worklog-core/src/daemon_tenants.rs, rust/crates/worklog-core/src/daemon.rs, web/components/BillingGroup.tsx, web/components/BillingPins.tsx, web/components/BillingGroup.test.tsx, web/app/tenant-actions.ts — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core move_line_deild && cd web && bun test components/BillingGroup.test.tsx` — after: T008, T003

## Phase 3 — Change notices
Goal: every automatic change is logged with its source and reaches the Owner as one pop-up per batch, live or on return.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core change_log && (cd web && bun test components/ChangeNotices.test.tsx)`
- [ ] T010 Change log: snapshot, diff, feed (B9, B12) — files: rust/crates/worklog-core/src/change_log.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core change_log` — after: T006
- [ ] T011 Wire every writer to the change log (B8, B9, B15) — files: rust/crates/worklog-core/src/estimate.rs, rust/crates/worklog-core/src/infer.rs, rust/crates/worklog-core/src/routing.rs, rust/crates/worklog-core/src/block_service.rs, rust/crates/worklog-core/src/daemon.rs, rust/crates/worklog-core/src/daemon_tenants.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core` — after: T010, T009
- [ ] T012 Change routes (B11) — files: rust/crates/worklog-core/src/daemon.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon::tests::changes` — after: T011
- [ ] T013 Live pop-ups and catch-up (B10, B11, B15) — files: web/components/ChangeNotices.tsx, web/components/ChangeNotices.test.tsx, web/lib/toast.ts, web/components/ToastHost.tsx, web/app/layout.tsx, web/lib/daemon.ts, web/app/actions.ts, web/app/globals.css — verify: `cd web && bun test components/ChangeNotices.test.tsx && bun run typecheck` — after: T012, T004
- [ ] T014 [P] Amend the CLAUDE.md billing rule for keyword-guessed deildir — files: CLAUDE.md — verify: `grep -q "keyword" CLAUDE.md` — after: T001
- [ ] CHK001 human-verify on 2026-09-25 real data — files: . — verify: human: add deildir to Sjúkra and APRÓ; the Billing view shows one line per (customer, deild); split one vitinn-infra block Sjúkra·Rekstur 50 / APRÓ·AI hraðall 50 and both lines update; re-run estimate → one batched pop-up naming Claude, hand-set split unchanged

## Gates
- [ ] G001 project gates clean — files: . — verify: `flow check --fix`
- [ ] G002 branch review clean — files: . — verify: `flow pass`
- [ ] G003 verification evidence exists — files: . — verify: `test -s verify/`
