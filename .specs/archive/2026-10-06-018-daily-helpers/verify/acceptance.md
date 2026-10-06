# Acceptance — 018 daily helpers

Every command run NOW from the repo root at the commit named below; Exit is the shell exit code. A filter that ran 0 tests is recorded as exit -1 (unmet).

| Criterion | Command | Exit | Tests run |
|---|---|---|---|
| FR-01 Tempo failure shows status and body | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib collectors::tempo::tests::sync_report_names_tempos_status_and_body_for_a_rejected_write` | 0 | 1 |
| FR-02 unreadable body never an empty reason | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib collectors::tempo::tests::error_body_` | 0 | 3 |
| FR-03 no start/end time shows "fill in" | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib billing::tests::a_line_with_no_blocks_says_fill_in_for_both_times` | 0 | 1 |
| FR-03 web formatLineTime | `cd web && bun test lib/export.test.ts -t "fill in"` | 0 | 2 |
| FR-04 same billed hours screen = Tempo (Rust rounding) | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib billing_round::tests::rounds_up_to_half_hour_with_zero_floor_below_15_minutes` | 0 | 1 |
| FR-04 web shows server seconds, no rounding | `cd web && bun test lib/format.test.ts components/LineHours.test.tsx` | 0 | 34 |
| FR-05 one rounding place (no web mirror) | `! grep -rn "Math.ceil(seconds / 1800)" web/lib web/components web/app` | 0 | — (grep) |
| FR-06 read Tempo before write | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib collectors::tempo::tests::matching_hand_entry_is_not_sent_and_the_line_is_marked` | 0 | 1 |
| FR-07 30 min same / 31 min different | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib tempo_match::tests::tolerance_is_inclusive_at_30_minutes_and_exclusive_at_31` | 0 | 1 |
| FR-08 different work is sent | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib collectors::tempo::tests::different_work_is_sent_as_today` | 0 | 1 |
| FR-09 Verdict unavailable still sends | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib collectors::tempo::tests::an_unavailable_matcher_still_sends` | 0 | 1 |
| FR-09 Verdict abstain/empty is unchecked | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib tempo_match::tests::matcher_error_or_empty_answer_is_unchecked` | 0 | 1 |
| FR-10 marked line not re-checked | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib collectors::tempo::tests::a_line_already_marked_is_skipped_without_reading_tempo` | 0 | 1 |
| FR-10 edit sends it to a re-check | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib tempo_match::tests::already_holds_until_text_or_hours_change` | 0 | 1 |
| FR-11/12 four red rows naming the fault | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib preflight::tests::one_failure_per_check_gives_four_red_rows_naming_the_fault` | 0 | 1 |
| FR-13 Send disabled while red; Send anyway | `cd web && bun test components/TaskSyncConfirm.test.tsx` | 0 | 4 |
| FR-14 read-back mismatch red, per day | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib preflight::tests::read_back_mismatch_is_red_and_names_the_day` | 0 | 1 |
| FR-14 read-back pulls fresh from Tempo | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib daemon_preflight::tests::read_back_compares_against_the_fresh_pull_not_stored_rows` | 0 | 1 |
| FR-14 web read-back row | `cd web && bun test components/ReadBackRow.test.tsx` | 0 | 2 |
| FR-15 terminal checklist stops on red | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-cli red_declined_stops_the_send` | 0 | 1 |
| FR-16 auto-send skips the checklist | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib auto_send::tests::the_17_00_run_sends_a_ready_line_beside_a_red_checklist` | 0 | 1 |
| FR-17 terminal entry point | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-cli json_prints_the_draft_and_never_posts` | 0 | 1 |
| FR-17 Day page entry point | `cd web && bun test components/StandupButton.test.tsx -t "drafts on click"` | 0 | 1 |
| FR-18 three answers in order | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib standup::tests::draft_returns_the_models_three_answers_in_order` | 0 | 1 |
| FR-19 sources, 300-char cut, scrubbed | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib standup::tests::` | 0 | 20 |
| FR-20 edited text is what gets posted | `cd web && bun test components/StandupButton.test.tsx -t "posts the edited text"` | 0 | 1 |
| FR-21 reply in today's Daily thread (mock Slack) | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib slack_post::tests::replies_in_the_thread_of_todays_daily_message` | 0 | 1 |
| FR-22 nothing posted without confirm (terminal) | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-cli declining_prints_the_draft_and_posts_nothing` | 0 | 1 |
| FR-23 missing thread / channel / refusal | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib slack_post::tests::` | 0 | 13 |
| FR-24 (SHOULD) regenerate | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib standup::tests::regenerate_sends_previous_draft_and_a_rewording_instruction` | 0 | 1 |
| FR-25 every change type undoable | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib undo::tests::undo_` | 0 | 15 |
| FR-26 last 20 kept | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib undo::tests::keeps_exactly_the_last_twenty_changes` | 0 | 1 |
| FR-27 refuse on synced, change nothing | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib undo::tests::refuses_synced_block_and_changes_nothing` | 0 | 1 |
| FR-28 never touches sent marker | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib undo::tests::undo_never_touches_sent_or_exported_markers` | 0 | 1 |
| FR-29 Undo on block-change confirmations | `cd web && bun test lib/toast.test.ts components/BlockCard.test.tsx components/TicketGroup.test.tsx components/IgnoredLine.test.tsx` | 0 | 81 |
| FR-30 ask from terminal | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-cli hits_print_date_block_time_and_ticket_in_given_order` | 0 | 1 |
| FR-30 /wl ask in Claude Code (whole mod suite) | `claude plugin test mods/worklog` | 0 | 136 |
| FR-31 up to 5, newest first, date/time/ticket | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib ask::tests::returns_five_newest_first_with_date_time_and_ticket` | 0 | 1 |
| FR-32 where did I stop | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib ask::tests::where_stopped_returns_last_three_prompts_and_files_of_that_repo_only` | 0 | 1 |
| FR-33 ask changes no data (Ruling: derived index only) | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib ask::tests::search_changes_nothing` | 0 | 1 |
| FR-33 daemon ask leaves blocks untouched | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib daemon_ask::tests::ask_leaves_blocks_untouched` | 0 | 1 |
| FR-34 customer month prints (terminal) | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-cli report_prints_text_by_default_and_csv_on_flag` | 0 | 1 |
| FR-35 grouped by deild with texts | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib report::tests::groups_hours_by_deild_with_texts` | 0 | 1 |
| FR-36 change from previous month / n/a | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib report::tests::empty_previous_month_is_na_not_zero_delta` | 0 | 1 |
| FR-37 unresolved stays empty | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib report::tests::unresolved_deild_stays_empty_in_csv` | 0 | 1 |
| FR-38 (SHOULD) CSV | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib report::tests::csv_has_header_one_row_per_deild_and_quotes_cells` | 0 | 1 |
| FR-39/41/42 footer nudge (whole mod suite) | `claude plugin test mods/worklog` | 0 | 136 |
| FR-40 three nudge kinds | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib nudges::tests::` | 0 | 14 |
| FR-43 recap: sent, held back, coverage, gaps | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib recap::tests::` | 0 | 18 |
| FR-44 gap actions | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib recap::tests::pick_ticket_adds_a_billable_manual_block_on_that_key` | 0 | 1 |
| FR-45 recap inside the 17:00 run | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib recap::tests::the_17_00_run_builds_the_recap_once_and_later_ticks_do_not_rebuild` | 0 | 1 |
| FR-46 recap on the Day page | `cd web && bun test components/RecapBanner.test.tsx` | 0 | 12 |
| B1 | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib collectors::tempo::tests::sync_report_names_tempos_status_and_body_for_a_rejected_write` | 0 | 1 |
| B3 | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib undo::tests::undo_merge_restores_every_block_and_event_link` | 0 | 1 |
| B6 | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib collectors::tempo::tests::matching_hand_entry_is_not_sent_and_the_line_is_marked` | 0 | 1 |
| B7 | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib recap::tests::only_the_three_longest_gaps_longest_first` | 0 | 1 |
| §5 nudge freshness ≤ 10 min | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib nudges::tests::cached_reviews_show_until_just_past_ten_minutes` | 0 | 1 |
| §5 Tempo read ≤ 10 s then send | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib collectors::tempo::tests::a_slow_tempo_read_gives_up_after_the_read_budget_and_still_sends` | 0 | 1 |
| §5 one ask request indexes ≤ 5 blocks (T043) | `cargo test -q --manifest-path rust/Cargo.toml -p worklog-core --lib daemon_ask::tests::get_ask_indexes_at_most_one_batch` | 0 | 1 |

| Human-only | Evidence | Exit |
|---|---|---|
| FR-21/22/23, B4 live Slack Post path | verify/CHK001.md (Owner chose Copy; Post mock-tested only) | human |
| FR-43..46, B7 live 17:00 recap | verify/CHK002.md (deferred until install + auto-send on) | human |
| §5 standup ≤ 30 s | NOTES.md: 16.01 s / 9.49 s on the DB copy | human |
| §5 ask ≤ 1 s | NOTES.md: 0.032 s / 0.034 s warm; first index 45 s → batched by T043 | human |
| §5 footer ≤ 50 ms | NOTES.md: not measured live (second daemon would contend for :9324) | human |
