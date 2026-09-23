# Tasks — Verdict routing
Spec: spec.md · Design: design.md · Base: 21f964f · Route: dispatch · Test: `cargo test --manifest-path rust/Cargo.toml && (cd web && bun test)`

## Behaviors
| ID | Given / When / Then | Task | Proven by |
|----|---------------------|------|-----------|
| B1 (P0) | Given winner 0.079, abstain 0.067, runner-up 0.065, when decided with defaults, then the event is filed | T003 | routing_test::files_when_winner_beats_abstain_and_runner_up |
| B2 (P0) | Given winner 0.070, abstain 0.070, runner-up 0.054, when decided, then it stays unsorted | T003 | routing_test::unsorted_when_winner_ties_abstain |
| B3 (P0) | Given winner 0.054, runner-up 0.053, abstain 0.050, when decided, then it stays unsorted | T003 | routing_test::unsorted_when_runner_up_too_close |
| B4 (P0) | Given the helper is unreachable, errors, or names a non-project, when routing runs, then the event stays unsorted | T002, T003 | verdict::tests::unreachable_is_none, routing_test::unsorted_when_choice_not_an_option |
| B5 (P0) | Given 44 projects, when the helper decides, then any of the 44 can win | T002 | verdict_server.py --self-test |
| B6 (P0) | Given a ratio of 0.9 or 5.5, when POSTed to /settings, then 400 and the stored value is unchanged | T005 | daemon::tests::settings_rejects_out_of_range_ratio |
| B8 (P0) | Given a Slack message linking github.com/<org>/vitinn-infra/pull/802, when routing runs, then it is filed to vitinn-infra with origin rule and the model is not asked | T007 | routing_test::exact_repo_mention_files_by_rule |
| B7 (P1) | Given `worklog verdict serve`, when the uv command is built, then it carries the pinned git rev and model revision | T004 | cli verdict_serve_args_pin_revisions |

## Phase 1 — Verdict decides
Goal: the daemon files an event only when Verdict's winner clearly beats "not enough evidence" and the runner-up, and `worklog verdict serve` starts the helper.
Independent test: `cargo test --manifest-path rust/Cargo.toml && ! grep -rqi laya rust/crates` — green with the web UI untouched.
- [x] T001 Contract and rename Laya to Verdict — files: rust/crates/worklog-core/src/routing_contract.rs, rust/crates/worklog-core/src/lib.rs, rust/crates/worklog-core/src/laya.rs, rust/crates/worklog-core/src/verdict.rs, rust/crates/worklog-core/templates/laya_server.py, rust/crates/worklog-core/templates/verdict_server.py, rust/crates/worklog-core/src/routing.rs, rust/crates/worklog-core/src/daemon.rs, rust/crates/worklog-cli/src/cli.rs — verify: `cargo test --manifest-path rust/Cargo.toml --no-run && ! grep -rqi laya rust/crates` — done: 929d122
- [ ] T002 [P] Verdict helper script and client (B4, B5) — files: rust/crates/worklog-core/templates/verdict_server.py, rust/crates/worklog-core/src/verdict.rs — verify: `python3 rust/crates/worklog-core/templates/verdict_server.py --self-test && cargo test --manifest-path rust/Cargo.toml -p worklog-core verdict::` — after: T001
- [ ] T003 [P] Relative filing rule (B1, B2, B3, B4) — files: rust/crates/worklog-core/src/routing.rs, rust/crates/worklog-core/src/routing_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core routing` — after: T001
- [ ] T004 [P] Verdict serve and status commands (B7) — files: rust/crates/worklog-cli/src/cli.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-cli verdict` — after: T001
- [ ] T005 [P] Daemon ratio settings (B6) — files: rust/crates/worklog-core/src/daemon.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon::` — after: T001
- [ ] T007 Exact repo or path mention files by rule (FR-10) — files: rust/crates/worklog-core/src/routing.rs, rust/crates/worklog-core/src/routing_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core routing` — after: T003

## Phase 2 — Settings and a real day
Goal: the owner tunes both ratios in Settings, and a real day's events are filed with 0 wrong filings.
Independent test: `cd web && bun test && bun run typecheck && ! grep -rqi laya lib components app` — green with the daemon untouched.
- [ ] T006 Web ratio settings and Verdict naming — files: web/lib/types.ts, web/lib/settingsForm.ts, web/components/RoutingSettings.tsx, web/components/SettingsPanel.test.tsx, web/lib/types.test.ts, web/app/actions.test.ts — verify: `cd web && bun test && bun run typecheck && ! grep -rqi laya lib components app` — after: T005
- [ ] CHK001 human-verify a real day sorted by Verdict — files: rust/crates/worklog-core/templates/verdict_server.py — verify: human: owner runs `worklog verdict serve`, rebuilds a real day, sees clear-clue events filed and no-clue events in Unsorted with 0 wrong filings; warm per-event ≤ 0.5 s and later starts ≤ 10 s recorded — after: T002, T003, T004, T005, T006

## Gates
- [ ] G001 project gates clean — files: . — verify: `flow check --fix`
- [ ] G002 branch review clean — files: . — verify: `test -f PASS-$(git rev-parse --short HEAD).md`
- [ ] G003 verification evidence exists — files: . — verify: `test -s verify/`
