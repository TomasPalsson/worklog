# Tasks — Customer-safe Tempo descriptions
Approved: 2026-10-20 by user
Verified: 2026-10-20 by user (pre-approved: /flow:next --finish)
Spec: spec.md · Design: design.md · Base: fcf966f · Route: dispatch · Test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core`

## Behaviors
| ID | Given / When / Then | Task | Proven by | Key |
|----|---------------------|------|-----------|-----|
| FR-01 (P0) | Given a good Appendix B text, when it is validated, then it is accepted unchanged | T001 | validate_accepts_good_period_texts | line-text/good-texts-pass |
| FR-02 (P0) | Given a text with evidence talk, when it is validated, then it is rejected naming the rule | T001 | validate_rejects_evidence_talk | line-text/no-evidence-talk |
| FR-03 (P0) | Given a number spelled out in words, when it is validated, then it is rejected | T001 | validate_rejects_spelled_numbers | line-text/digits-not-words |
| FR-04 (P0) | Given a text naming Slack, when it is validated, then it is rejected | T001 | validate_rejects_slack | line-text/no-slack |
| FR-05 (P0) | Given a text starting with a plural past verb, when it is validated, then it is rejected | T001 | validate_rejects_plural_first_verb | line-text/first-person-verb |
| FR-06 (P0) | Given a Sjúkra ticket line naming VÍS and BYKO, when it is written, then it is rejected and re-asked | T002 | foreign_customers_on_ticket_line | line-text/own-customer-only |
| FR-07 (P0) | Given the Icelandic system prompt, when it is read, then it states the four new rules | T001 | prompt_states_customer_safe_rules | line-text/prompt-rules |
| FR-08 (P0) | Given a hand-typed text breaking a rule, when it is saved, then it is kept as typed and flagged | T003 | manual_text_breaking_rule_is_flagged_not_changed | tempo-lines/manual-flagged |
| FR-09 (P0) | Given a hand-typed or meeting text equal to the ticket summary, when it is saved, then it is flagged | T003 | static_check_flags_summary_copy | tempo-lines/summary-copy-flagged |
| FR-10 (P1) | Given a rejected reply, when the writer re-asks, then the message names the broken rule | T001 | reask_message_names_rule | line-text/reask-names-rule |

## Review focus
Icelandic texts with þ, ð, æ and customer aliases that are ordinary words ("festi", "sýni") — T001, T002

## Phase 0 — Baseline
Goal: the test suite's result at Base is on record before anything changes.
Independent test: `test -s .specs/028-customer-safe-tempo-descriptions/verify/baseline-tests.txt` — the baseline file exists.
- [x] T000 Record the test suite at Base — files: .specs/028-customer-safe-tempo-descriptions/verify/baseline-tests.txt — verify: `test -s .specs/028-customer-safe-tempo-descriptions/verify/baseline-tests.txt` — done: 8524e76

## Phase 1 — A generated line can't hedge, spell numbers, name Slack or start plural
Goal: a model reply with evidence talk, spelled numbers, Slack or a plural first verb is rejected and re-asked, and the prompt tells the model why.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- line_text tempo_line_writer` — Appendix B good texts pass, bad 1, 3, 4, 5, 6 fail, and the writer re-asks with the reason.
- [x] T001 A generated line with evidence talk, a spelled version, Slack or a plural verb is re-asked (FR-01, FR-02, FR-03, FR-04, FR-05, FR-07, FR-10) — files: rust/crates/worklog-core/src/line_text.rs, rust/crates/worklog-core/src/line_text_prompt.rs, rust/crates/worklog-core/src/line_text_test.rs, rust/crates/worklog-core/src/tempo_line_writer.rs, rust/crates/worklog-core/src/tempo_line_writer_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- line_text tempo_line_writer` — follow: rust/crates/worklog-core/src/line_text.rs — after: T000 — done: 4c6a1ec

## Phase 2 — A ticket line only names its own customer
Goal: a ticket line that names another customer, or several, is rejected and re-asked; the model is told the ticket's customer.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- foreign_customers tempo_line_writer` — B-bad 2 on a Sjúkra ticket is re-asked, a VÍS text on a VÍS ticket passes.
- [x] T002 A ticket line naming another customer is re-asked (FR-06) — files: rust/crates/worklog-core/src/line_text.rs, rust/crates/worklog-core/src/line_text_test.rs, rust/crates/worklog-core/src/tempo_line_writer.rs, rust/crates/worklog-core/src/tempo_line_writer_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- foreign_customers tempo_line_writer` — no_claim: the model is stubbed; whether a real model obeys the prompt is not exercised — after: T001 — done: 9eb90f6

## Phase 3 — Meeting and hand-typed lines get flagged, never rewritten
Goal: a meeting or hand-typed line that breaks a rule or copies the ticket summary shows "needs a look", with its text untouched.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- line_check tempo_lines daemon_tempo_lines` — a manual save of B-bad 1 keeps its text and is flagged; "Hertz - Greiningarfundur" is flagged.
- [x] T003 Meeting and hand-typed lines that break a rule or copy the ticket title show "needs a look" (FR-08, FR-09) — files: rust/crates/worklog-core/src/line_check.rs, rust/crates/worklog-core/src/line_check_test.rs, rust/crates/worklog-core/src/tempo_lines.rs, rust/crates/worklog-core/src/tempo_lines_test.rs, rust/crates/worklog-core/src/daemon_tempo_lines.rs, rust/crates/worklog-core/src/daemon_tempo_lines_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- line_check tempo_lines daemon_tempo_lines` — after: T002 — done: 940881d

## Gates
- [x] G001 project gates clean — files: . — verify: `flow check --fix --since fcf966f` — done: 45df725
- [x] G002 branch review clean — files: . — verify: `flow pass` — done: 45df725
- [x] G003 verification evidence exists — files: . — verify: `test -s verify/` — done: 45df725
