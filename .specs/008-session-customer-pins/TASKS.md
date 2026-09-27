Approved: 2026-09-27 by user
Base: 7ef08c1
# Tasks — Session customer pins
Spec: spec.md · Design: design.md · Base: 83ceaff · Route: dispatch · Test: `cargo test --manifest-path rust/Cargo.toml`

## Behaviors
| ID | Given / When / Then | Task | Proven by |
|----|---------------------|------|-----------|
| B1 (P0) | Given a known customer name or alias, when pinned, then a pin row is stored; given an unknown name, then nothing is stored and the known customers are returned | T001 | session_pins_test::unknown_customer_is_refused |
| B2 (P0) | Given a non-default branch with a pin, when asked for its pin, then the latest is returned; given `main`, then none | T001 | session_pins_test::default_branch_never_inherits |
| B3 (P0) | Given a repo on branch feat/x, when the branch is read, then "feat/x"; detached or non-repo → none | T002 | git::tests::current_branch_reads_the_checked_out_branch |
| B4 (P0) | Given `worklog pin Sjukra --session s1`, when run in a /Work repo, then exit 0 and the pin resolves to Sjúkra; a typo exits 2 listing customers | T003 | cli::pin_refuses_unknown_customer |
| B5 (P0) | Given a session starting in a multi-tenant /Work folder, when the start hook runs, then it prints the instruction; with a branch pin it inherits and prints the customer; elsewhere it prints nothing; on any error it prints nothing and exits 0 | T004 | session_pins_test::start_text_inherits_branch_pin |
| B6 (P0) | Given worklog hooks installed, when installed again or uninstalled, then both the recorder and the start instruction are present exactly once, or both gone | T005 | hook::tests::install_keeps_recorder_and_start_hint |
| B7 (P0) | Given a pinned session and a text guess naming another customer, when the day is rebuilt, then the session's events carry the pinned customer, and a mid-session re-pin moves later events | T006 | session_customers_test::pin_beats_text_guess |
| B8 (P0) | Given a block whose sessions are pinned to one customer and no saved split, when its customer line is built, then one slice with origin pinned; a saved split still wins | T007 | tenant_split_test::pinned_session_gives_pinned_slice |
| B9 (P0) | Given a pinned slice, when the day view or the block detail renders, then "Customer: X" with the "pinned" tag and a Change button | T008 | BlockCustomerSplit.test.tsx pinned origin |

## Phase 1 — Pins exist
Goal: Claude can pin a customer to its session from the command line, and a new session on the same branch inherits it.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core session_pins` and `cargo test --manifest-path rust/Cargo.toml -p worklog-cli pin` — green.
- [x] T001 [P] pin store and customer lookup (B1, B2) — files: rust/crates/worklog-core/sql/schema.sql, rust/crates/worklog-core/src/session_pins.rs, rust/crates/worklog-core/src/session_pins_test.rs, rust/crates/worklog-core/src/lib.rs, rust/crates/worklog-core/src/billing_registry.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core session_pins` — done: 47cb872
- [x] T002 [P] current branch reader (B3) — files: rust/crates/worklog-core/src/git.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core current_branch` — done: 55ec9f0
- [x] T003 pin command (B4) — files: rust/crates/worklog-cli/src/cli.rs, rust/crates/worklog-cli/tests/cli.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-cli pin` — after: T001, T002 — done: 4942182
- [ ] T004 start instruction and inheritance (B5) — files: rust/crates/worklog-core/src/session_pins.rs, rust/crates/worklog-core/src/session_pins_test.rs, rust/crates/worklog-cli/src/cli.rs, rust/crates/worklog-cli/tests/cli.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core start_text` — after: T003
- [ ] T005 installer keeps both hooks (B6) — files: rust/crates/worklog-core/src/hook.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core hook::tests` — after: T004

## Phase 2 — Pins drive blocks
Goal: a pinned session's minutes land in its customer's lane, and the block's customer line says "pinned".
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core` — green, and `bash scripts/verify-inference.sh` exits 0.
- [x] T006 [P] pins beat the text guess in lanes (B7) — files: rust/crates/worklog-core/src/session_customers.rs, rust/crates/worklog-core/src/session_customers_test.rs, rust/crates/worklog-core/src/infer_allocations.rs, rust/crates/worklog-core/src/infer_allocations_db_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core pin_beats_text_guess` — after: T001 — done: 0ac1465
- [x] T007 [P] pinned customer line (B8) — files: rust/crates/worklog-core/src/tenant_contract.rs, rust/crates/worklog-core/src/tenant_split.rs, rust/crates/worklog-core/src/tenant_split_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core pinned_session_gives_pinned_slice` — after: T001 — done: ce7997a

## Phase 3 — You can see it
Goal: the Owner sees "pinned" in the day view and the block detail view and can still change it.
Independent test: `cd web && bun test && bun run typecheck` — green.
- [ ] T008 show pinned in day and detail views (B9) — files: web/lib/tenants.ts, web/components/BlockCustomerSplit.tsx, web/components/BlockCustomerSplit.test.tsx, web/app/[day]/block/[id]/page.tsx — verify: `cd web && bun test components/BlockCustomerSplit.test.tsx` — after: T007
- [ ] CHK001 human-verify the pin flow live — files: .specs/008-session-customer-pins/verify/CHK001.md — verify: human: in vitinn-infra on a feature branch, a new session told "work on the Sjúkra config", then /clear + /flow:next on the same branch; after the day rebuild both sessions' blocks show "Sjúkra (pinned)" in the day view and Claude never asked — after: T005, T006, T008

## Gates
- [ ] G001 project gates clean — files: . — verify: `flow check --fix`
- [ ] G002 branch review clean — files: . — verify: `flow pass`
- [ ] G003 verification evidence exists — files: . — verify: `test -s .specs/008-session-customer-pins/verify/`
- [ ] G004 web gates clean — files: web — verify: `cd web && bun test && bun run typecheck`
