# Acceptance — 011 Tempo ticket auto-pick (HEAD e8fd89b, 2026-10-01)

Commands run now:
- A = `cargo test --manifest-path rust/Cargo.toml -p worklog-core -- block_service::tests infer_allocations::db_tests estimate::tests tempo_lines daemon_tempo_lines collectors::tempo::` → exit 0, 159 passed, 0 failed
- C = `cargo test --manifest-path rust/Cargo.toml -p worklog-cli generate_line_texts` → exit 0, 3 passed
- W = `cd web && bun test components/BlockCard.test.tsx components/TicketGroup.test.tsx app/actions-tempo-lines.test.ts` → exit 0, 41 pass, 0 fail (3/3 runs; one earlier concurrent run failed, see NOTES.md)
- Full suites: `cargo test --manifest-path rust/Cargo.toml` exit 0; `cd web && bun test` exit 0 (378 pass)

| Criterion | Cmd | Exit | Proven by (test that ran) | Met |
|---|---|---|---|---|
| FR-01 / B1 hand assign or clear → manual | A | 0 | block_service::assign_ticket_records_manual_origin_on_set_and_on_clear | yes |
| FR-02 / B2 event key → event origin | A | 0 | infer_allocations::db_tests::event_key_is_stored_with_event_origin | yes |
| FR-03 / B4 estimator pick → auto | A | 0 | estimate::estimate_day_marks_a_newly_picked_ticket_as_auto, estimate_block_marks_a_newly_picked_ticket_as_auto | yes |
| FR-04 / B3 / B5 manual ticket (and manual NULL) survives rebuild, estimate, split, auto-merge | A | 0 | db_tests::manual_ticket_survives_rebuild_with_a_different_event_key, manually_cleared_ticket_stays_empty_after_rebuild; estimate::estimate_day_keeps_manual_ticket_but_writes_description, estimate_day_keeps_manually_cleared_ticket_empty, estimate_day_with_does_not_overwrite_a_block_marked_manual_mid_batch, merge_keeps_manual_origin_when_either_block_is_manual; block_service::split_tail_inherits_manual_ticket_origin | yes |
| FR-05 / B10 "auto" tag for event/auto/NULL, hidden for manual | W | 0 | auto ticket tag › shows beside a ticket the Owner did not set / shows for a pre-spec row… / is hidden for a manually set ticket | yes |
| FR-06 stored description on each line | A, W | 0 | daemon_tempo_lines::get_lists_the_day_line_with_union_hours_and_no_stored_text; TicketGroup line text › shows the stored text | yes |
| FR-07 / B11 edit description | A, W | 0 | daemon_tempo_lines::text_route_stores_manual_text_and_404s_for_unknown_line; TicketGroup › saves edited text on Cmd-Enter | yes |
| FR-08 regenerate | A, W | 0 | daemon_tempo_lines::generation_stores_text_once_and_skips_fresh_and_manual_lines_unless_forced (forced path the route uses), regenerate_route_404s_for_unknown_line; TicketGroup › regenerates the line | yes (route success path covered via its forced helper, not a route-level 200 test) |
| FR-09 / B6 union hours rounded to 0.5h | A | 0 | tempo_lines::lines_for_day_unions_overlaps_and_skips_personal_and_unticketed | yes |
| FR-10 / B11 set and clear override | A, W | 0 | tempo_lines::set_hours_validates_and_override_drives_effective_seconds; daemon hours_route_sets_override_400s_invalid_and_404s_unknown_line; TicketGroup › saves a half-hour override / clears the override | yes |
| FR-11 / B8 sync sends stored text + effective hours | A | 0 | collectors::tempo::sync_sends_stored_line_text_and_hours_override (httpmock body) ; CHK001 dry-run payload 7200 s + edited text | yes |
| FR-12 / B7 edit marks synced blocks dirty | A | 0 | tempo_lines::set_text_stores_manual_blank_clears_and_dirties_only_synced_blocks, commit_generated_keeps_override_and_dirties_synced_blocks | yes |
| FR-13 / B9 generate missing/stale in estimate run + `worklog day` | A, C | 0 | daemon_tempo_lines::estimate_route_reports_tempo_lines_count, estimate_pass_counts_only_newly_generated_lines; tempo_lines::pending_generation_covers_missing_and_stale_generated_text_only; cli::generate_line_texts_stores_model_text_and_reports_count | yes |
| FR-14 fallback summary when no stored text | W | 0 | TicketGroup › shows the fallback when nothing is generated | yes |
| FR-15 sync generates, stores, sends when no text | A | 0 | collectors::tempo::sync_generates_and_stores_text_when_line_has_none | yes |
| FR-16 non-half-hour → inline error, nothing saved | A, W | 0 | TicketGroup › rejects a non-half-hour value inline without calling the action, shows a daemon rejection inline; daemon hours route 400 | yes |
| FR-17 regenerate replaces Owner-edited text | A | 0 | tempo_lines::manual_text_survives_generation_unless_forced (forced replaces) | yes |
| A7 failed generation never stored | A, C | 0 | tempo_lines::generate_text_is_none_when_the_model_call_fails; daemon failed_generation_stores_no_text_and_is_not_counted; collectors::tempo::failed_summary_sends_joined_fallback_and_stores_no_text; cli::generate_line_texts_with_failing_model_stores_nothing_and_warns | yes |
| NFR ≤ 250 chars to Tempo | A | 0 | collectors::tempo::sync_caps_long_stored_line_text | yes |
| NFR 0.5h granularity | A | 0 | tempo_lines::set_hours_validates_and_override_drives_effective_seconds | yes |
| NFR 1 click for a correct day | — | — | CHK001: Tickets view needed no clicks before Sync; real Sync not pressed (would post to the company Tempo) — dry-run payload is what Sync sends | yes (by dry run) |
