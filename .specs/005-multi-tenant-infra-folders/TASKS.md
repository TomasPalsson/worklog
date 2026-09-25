# Tasks — Multi-tenant infra folders
Approved: 2026-09-25 by user
Spec: spec.md · Design: design.md · Base: 79ce984 · Route: dispatch · Test: `cargo test --manifest-path rust/Cargo.toml && cd web && bun test`

## Behaviors
| ID | Given / When / Then | Task | Proven by |
|----|---------------------|------|-----------|
| B1 (P0) | Given a fresh DB, when migrated, then vitinn-infra and genai-infra are multi-tenant with their roots (FR-02) | T001 | seed_marks_infra_folders_multi_tenant |
| B2 (P0) | Given the flag toggled, when the registry reloads, then it persists (FR-01) | T001 | multi_tenant_flag_round_trips |
| B3 (P0) | Given tenant dirs `sjukra`, `apro-prod`, `byko-datalake`, when listed, then Alias Sjúkra, Alias APRÓ, Unmatched (FR-03, FR-04) | T002 | tenants_match_aliases_and_leave_unmatched |
| B4 (P0) | Given a link or Ignored mark, when listed, then Link / Ignored wins over alias (FR-05) | T002 | link_and_ignore_beat_alias |
| B5 (P0) | Given clues Sjúkra@13:40 and MMS@15:30, when split, then each second goes to the nearest clue (FR-06) | T003 | split_gives_seconds_to_nearest_clue |
| B6 (P0) | Given house + Sjúkra clues, when split, then 100% Sjúkra (FR-07) | T003 | house_clues_dropped_when_other_customer_present |
| B7 (P0) | Given a branch and a path clue in one minute, when split, then the path wins (FR-08) | T003 | stronger_clue_wins_its_minute |
| B8 (P0) | Given no timestamped clue, when split, then `None` and billing falls back (FR-09) | T003, T005 | no_clue_block_falls_back |
| B9 (P0) | Given random clues, when split, then slices are disjoint and sum to the block (FR-12) | T003 | slices_sum_exactly_property |
| B10 (P0) | Given saved 50/50 shares, when the day is re-inferred, then the shares still apply (FR-10, FR-11) | T004 | shares_survive_reinfer |
| B11 (P0) | Given shares summing to 0.9, when saved, then "Shares must add up to 100%" (J3 error) | T004 | shares_must_sum_to_one |
| B12 (P0) | Given 2026-09-24-shaped fixture, when exported, then a Sjúkra line and an APRÓ line (J1) | T005 | mixed_day_exports_customer_lines |
| B13 (P0) | Given a non-multi-tenant folder, when exported, then output unchanged (FR-13) | T005 | existing billing tests |
| B14 (P1) | Given a 30-block 2 000-event day, when exported, then < 500 ms (§5) | T005 | export_latency_under_500ms |
| B15 (P0) | Given the daemon, when the tenant/shares routes are called, then they round-trip and bad input is 400 | T006 | tenant_routes_round_trip |
| B16 (P0) | Given an Unmatched tenant, when the Owner links it in the Billing panel, then it shows as Link (J2) | T007 | BillingTenantSection.test.tsx |
| B17 (P1) | Given a split block, when the card renders, then it shows the split and origin; editing saves shares (FR-10, FR-14) | T008 | BlockCustomerSplit.test.tsx |

## Phase 1 — Tenant data
Goal: worklog knows which folders hold many customers, which tenants they have, and who each tenant belongs to.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core tenant` — green with billing untouched.
- [x] T001 Schema, seed and the multi_tenant flag (B1, B2) — files: rust/crates/worklog-core/sql/schema.sql, rust/crates/worklog-core/src/db.rs, rust/crates/worklog-core/src/billing_registry.rs, rust/crates/worklog-core/src/lib.rs, rust/crates/worklog-core/src/tenants.rs, rust/crates/worklog-core/src/tenant_shares.rs, rust/crates/worklog-core/src/tenant_clues.rs, rust/crates/worklog-core/src/tenant_split.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core multi_tenant` — done: f45ac05
- [x] T002 [P] Tenant discovery and links (B3, B4) — files: rust/crates/worklog-core/src/tenants.rs, rust/crates/worklog-core/src/tenants_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core tenants::` — after: T001 — done: f5b179e
- [x] T004 [P] Hand-set shares storage and carving (B10, B11) — files: rust/crates/worklog-core/src/tenant_shares.rs, rust/crates/worklog-core/src/tenant_shares_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core tenant_shares` — after: T001 — done: 90d7225

## Phase 2 — The split
Goal: a multi-tenant block turns into per-customer slices, and the billing export bills each customer its own time.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core` — green with the web UI untouched.
- [x] T003 Clues and the split rules (B5–B9) — files: rust/crates/worklog-core/src/tenant_clues.rs, rust/crates/worklog-core/src/tenant_split.rs, rust/crates/worklog-core/src/tenant_split_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core tenant_split` — after: T002, T004 — done: 539777d
- [x] T005 [P] Billing export uses slices (B8, B12–B14) — files: rust/crates/worklog-core/src/billing.rs, rust/crates/worklog-core/src/billing_tenant_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core billing` — after: T003 — done: 0f80a1a
- [x] T006 [P] Daemon routes for tenants and shares (B15) — files: rust/crates/worklog-core/src/daemon.rs, rust/crates/worklog-core/src/daemon_tenants.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon_tenants` — after: T003 — done: 09a6efb

## Phase 3 — Review UI
Goal: the Owner can tick a folder multi-tenant, link leftover tenants, and adjust one block's split.
Independent test: `cd web && bun test && bun run typecheck` — green.
- [x] T007 Billing panel: multi-tenant tick and tenant list (B16) — files: web/lib/tenants.ts, web/app/tenant-actions.ts, web/components/BillingTenantSection.tsx, web/components/BillingTenantSection.test.tsx, web/components/BillingFolderSection.tsx, web/components/BillingRegistry.tsx, web/lib/types.ts — verify: `cd web && bun test components/BillingTenantSection.test.tsx && bun run typecheck` — after: T006 — done: 8f01b28
- [ ] T008 Block customer split editor (B17) — files: web/components/BlockCustomerSplit.tsx, web/components/BlockCustomerSplit.test.tsx, web/components/BlockCard.tsx — verify: `cd web && bun test components/BlockCustomerSplit.test.tsx && bun run typecheck` — after: T007
- [ ] CHK001 human-verify yesterday's export — files: web/components/ExportPanel.tsx — verify: human: re-run 2026-09-24; export shows a Sjúkra line for blocks 4098, 4101, 4102 (~2.5 h) and an APRÓ line for the rest of vitinn-infra; change one block's split, re-estimate, the split is still there — after: T008

## Gates
- [ ] G001 project gates clean — files: . — verify: `flow check --fix`
- [ ] G002 branch review clean — files: . — verify: `test -f PASS-$(git rev-parse --short HEAD).md`
- [ ] G003 verification evidence exists — files: . — verify: `test -s verify/`
