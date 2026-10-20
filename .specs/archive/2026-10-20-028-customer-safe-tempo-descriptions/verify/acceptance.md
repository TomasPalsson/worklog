# Acceptance — 028 customer-safe Tempo descriptions

Run 2026-10-20 at 45df725. Exit = `<cmd> > log 2>&1; echo $?`, read from the log.

| Criterion | Command | Exit | Showed |
|---|---|---|---|
| FR-01 good Appendix B texts pass unchanged | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- --exact line_text::tests::appendix_b_good_texts_pass_unchanged` | 0 | ok, 1 passed, 0 failed |
| FR-02 evidence talk rejected naming the rule | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- --exact line_text::tests::every_evidence_phrase_is_rejected_in_any_case` | 0 | ok, 1 passed, 0 failed |
| FR-03 spelled numbers rejected | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- --exact line_text::tests::spelled_version_numbers_are_rejected_but_prose_numbers_pass` | 0 | ok, 1 passed, 0 failed |
| FR-04 Slack rejected | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- --exact line_text::tests::slack_is_rejected_as_a_whole_word_only` | 0 | ok, 1 passed, 0 failed |
| FR-05 plural first verb rejected | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- --exact line_text::tests::plural_first_verb_is_rejected_by_its_first_word_only` | 0 | ok, 1 passed, 0 failed |
| FR-06 Sjúkra ticket line naming VÍS and BYKO rejected and re-asked | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- --exact tempo_line_writer::tests::foreign_customer_reply_is_reasked_with_the_names` | 0 | ok, 1 passed, 0 failed |
| FR-06 (rule) other customers listed for a Sjúkra ticket | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- --exact line_text::tests::foreign_customers_lists_other_customers_sorted_by_registry_name` | 0 | ok, 1 passed, 0 failed |
| FR-07 prompt states the four new rules | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- --exact tempo_line_writer::tests::prompt_carries_every_new_rule` | 0 | ok, 1 passed, 0 failed |
| FR-08 hand-typed rule-breaking text kept and flagged | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- --exact tempo_lines::tests::tempo_lines_manual_save_keeps_a_rule_breaking_text_and_flags_it` | 0 | ok, 1 passed, 0 failed |
| FR-09 hand-typed text equal to ticket summary flagged | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- --exact tempo_lines::tests::tempo_lines_manual_save_copying_the_ticket_summary_is_flagged` | 0 | ok, 1 passed, 0 failed |
| FR-09 meeting text equal to ticket summary flagged untouched | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- --exact daemon::tests::daemon_tempo_lines::daemon_tempo_lines_meeting_named_like_the_ticket_is_flagged_untouched` | 0 | ok, 1 passed, 0 failed |
| FR-10 re-ask message names the broken rule | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- --exact tempo_line_writer::tests::reask_names_the_new_rules` | 0 | ok, 1 passed, 0 failed |
