# Claim check — acceptance.md + PASS-5783062.md

Re-run at HEAD 5783062 in this worktree; no `next dev` running (pgrep exit 1), so build was run.
R = `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib`; W = `cd web && bun test`.

| Claim | Command run | Exit | Verdict | Note |
|---|---|---|---|---|
| FR-01, FR-03, B1 | R matches_any_word_and_ranks_best_first; R equal_rank_ties_break_newest_first | 0 | CONFIRMED | each ok; test asserts ids == [both, one] with a missing-word query and newest single-word block; tie test asserts [new, old] |
| FR-02 | R query_joins_words_with_or_and_drops_stopwords | 0 | CONFIRMED | ok (ask::tests) |
| FR-04 | R ask::tests (24); operator_characters_are_text_not_syntax, search_changes_nothing, blank_and_operator_queries_are_data_not_errors, ask_leaves_blocks_untouched; R daemon_ask (6) | 0 | CONFIRMED | counts 24 and 6 match |
| FR-05, B2 | R routing_noise::tests (8); W UnsortedList.test.tsx (31 pass; noise groups tests incl. "tags a group whose host is listed") | 0 | CONFIRMED | 8 pass, 31 pass; human CHK001 part not re-checked |
| FR-06 | W UnsortedList "confirms first, then dismisses only tagged groups' events with no rule kind" | 0 | CONFIRMED | passes |
| FR-07 | W UnsortedList "filing a group" (3 pass) | 0 | CONFIRMED | doc's own caveat stands: no tagged-group filing test |
| FR-08 | python3 .../verdict_server.py --self-test | 0 | CONFIRMED | prints "verdict_server self-test OK" (output wording differs trivially from "self-test OK") |
| FR-09, B5 | R ensure_does_not_re_embed_an_unchanged_block / _text_changed / _model_revision_changed | 0 | CONFIRMED | each ok |
| FR-10 | R topic_count_follows_rounded_sqrt_and_clamps; R topic_name_is_top_ticket_summary_else_longest_block_description; W TopicsSection.test.tsx | 0 | CONFIRMED | 11 web pass |
| FR-11 | R switches_count_topic_changes_in_started_order_and_longest_run_is_wall_clock | 0 | CONFIRMED | ok |
| FR-12 | R drift_assigns_before_blocks_to_nearest_topic; R topics_report_loads_exactly_the_right_blocks_and_counts_skipped | 0 | CONFIRMED | ok. Minor doc error: the Shows column says seconds_before "600 / 0"; the test actually asserts (6000 now, 300 before) and (3600, 50). Test still supports FR-12 |
| FR-13 | R sprawl_needs_min_blocks_and_ranks_widest_first | 0 | CONFIRMED | ok; cap-of-3 test exists (sprawl_is_capped_at_top_three) but not run per doc; not in scope |
| FR-14 | R similar_days_rank_twin_first_and_exclude_the_target_day | 0 | CONFIRMED | ok |
| FR-15, B4 | R topics_report_maps_a_helper_error_to_helper_down; W TopicsSection "null report and helper_down" | 0 | CONFIRMED | ok; "other sections render" not test-asserted, as the doc says |
| FR-16 | R topics_report_writes_only_block_embeddings | 0 | CONFIRMED | ok (daemon_stats test) |
| FR-17 (SHOULD) | W TopicsSection.test.tsx | 0 | CONFIRMED (as "not in MVP") | 11 pass; the doc itself states no hover test; not a proof of FR-17 |
| FR-18 | R too_few_blocks_just_below_min_and_ready_at_min; R topics_route_with_no_blocks_is_too_few_not_an_error; W "too_few_blocks shows" | 0 | CONFIRMED | all ok |
| FR-19 | R embed_rejects_a_vector_count_mismatch; embed_rejects_a_wrong_dimension_vector; topics_report_stores_nothing_when_a_later_batch_is_bad | 0 | CONFIRMED | all ok |
| B3 | R topics_report_loads_exactly_the_right_blocks_and_counts_skipped; R stats_topics::tests (19 pass); W TopicsSection.test.tsx | 0 | CONFIRMED | doc's `R stats_topics::tests` is a loose filter; 19 pass |
| G001 fmt | cargo fmt --manifest-path rust/Cargo.toml --all -- --check | 0 | CONFIRMED | |
| G001 clippy | cargo clippy ... --all-targets --all-features -- -D warnings | 0 | CONFIRMED | |
| G001 rust tests | cargo test --manifest-path rust/Cargo.toml | 0 | CONFIRMED | 2016+78+53+7+2 passed, 0 failed; matches |
| G001 web tests | cd web && bun test | 0 | CONFIRMED | 1563 pass, 0 fail |
| G001 typecheck | cd web && bun run typecheck | 0 | CONFIRMED | |
| G001 build | cd web && bun run build | 0 | CONFIRMED | no dev server was running |
| G001 helper self-test | verdict_server.py --self-test | 0 | CONFIRMED | |
| G002 review workflow, G003 | not re-run | - | NOT-CHECKABLE | G002 needs the workflow transcript wf_edb45356-cca; G003 is a trivial test -s (not requested) |

Non-CONFIRMED rows: G002 only (NOT-CHECKABLE). FR-17 is confirmed only as "not implemented".
