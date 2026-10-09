# Acceptance — 020 (2026-10-09)
| Criterion | Command | Exit | Tests run | What it showed |
|---|---|---|---|---|
| FR-01 (estimate read from Jira) | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- estimates_parse_seconds_and_missing_is_none_not_zero` | 0 | 1 | Mocked Jira search parses estimate seconds; missing estimate is None not 0. |
| FR-02 (all authors, all pages) | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- worklogs_follow_pages_and_keep_every_author` | 0 | 1 | Worklogs followed across pages, every author kept with id/name/day/seconds. |
| FR-03 (10-minute cache) | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- exactly_ten_minutes_is_fresh_eleven_is_stale fresh_cache_makes_no_jira_call cache_one_second_past_ten_minutes_is_stale` | 0 | 3 | Fresh cache makes no Jira call; 11-min-old cache refetches (daemon + store staleness tests). |
| FR-03a (sync marks stale) | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- sync_marks_one_ticket_stale_so_a_one_minute_old_cache_refetches` | 0 | 1 | After sync marks the ticket stale, a 1-minute-old cache is refetched. |
| FR-04 (keep cache, per-ticket error) | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- jira_failure_keeps_cache_and_flags_the_ticket` | 0 | 1 | Jira failure keeps cached numbers and flags the ticket error. |
| FR-04a (Jira numbers from HH:MM) | `(cd web && bun test components/EstimateBar.test.tsx -t "shows when Jira numbers were pulled")` | 0 | 1 | Renders "Jira numbers from <clock>". |
| FR-04b (Try again action) | `(cd web && bun test components/EstimateBar.test.tsx -t "failed with")` | 0 | 2 | Failed state offers Try again and calls onRetry once (with and without cache). |
| FR-04b (refetch only that ticket) | `(cd web && bun test components/BlockCard.test.tsx -t "a per-ticket retry forces that ticket")` | 0 | 1 | Retry forces that ticket only and keeps the others. |
| FR-05 (used of estimate on block) | `(cd web && bun test components/EstimateBar.test.tsx -t "shows the approved over state")` | 0 | 1 | Head shows "5h 30m" "of 4h" for a ticket with an estimate. |
| FR-05a (X left / X over words) | `(cd web && bun test components/EstimateBar.test.tsx -t "shows the approved over state|names the tone")` | 0 | 2 | Shows "1h 30m over" and "3h left". |
| FR-06 (segment per person, You first, hours desc) | `(cd web && bun test lib/progress.test.ts -t "FR-06: You first")` | 0 | 1 | You first even when a teammate has more hours; teammates by hours desc. |
| FR-06a (more than 4 -> Others) | `(cd web && bun test lib/progress.test.ts -t "B6/FR-06a")` | 0 | 1 | 4 stay named; 5 and 6 fold into Others. |
| FR-07 (striped +this block segment) | `(cd web && bun test lib/progress.test.ts -t "B4|FR-07")` | 0 | 2 | Pending block adds its duration as a pending segment; pending 0 adds nothing. |
| FR-07a (head says once synced) | `(cd web && bun test components/EstimateBar.test.tsx -t "shows the approved over state|without pending hours")` | 0 | 2 | "once synced" shown with pending hours, absent without. |
| FR-08 (flag marks estimate) | `(cd web && bun test components/EstimateBar.test.tsx -t "sizes segments against max|the flag leaves the end")` | 0 | 2 | Flag positioned at the estimate (72.73%), at end when not over. |
| FR-08a (amber zone past estimate) | `(cd web && bun test components/EstimateBar.test.tsx -t "shows the approved over state|exactly at the estimate")` | 0 | 2 | .ep-overzone present when over, absent exactly at estimate. |
| FR-09 (tone thresholds 79/80/100/101) | `(cd web && bun test lib/progress.test.ts -t "FR-09: tone at 79%")` | 0 | 1 | Tone ok/low/low/over at 79/80/100/101 percent. |
| FR-09 (tone icon and words) | `(cd web && bun test components/EstimateBar.test.tsx -t "names the tone with an icon label")` | 0 | 1 | On track / Running low / Over icons with labels. |
| FR-10 (legend initials and hours) | `(cd web && bun test components/EstimateBar.test.tsx -t "shows the approved over state")` | 0 | 1 | Legend lists names, initials TP/JG and hours 2h, 2h 30m. |
| FR-10a (legend ends with This block, not in Tempo yet) | `(cd web && bun test components/EstimateBar.test.tsx -t "shows the approved over state")` | 0 | 1 | Legend has "This block, not in Tempo yet" and "+1h". |
| FR-11 (running-total chart, dashed estimate line) | `(cd web && bun test components/ProgressChart.test.tsx -t "labels the chart|draws the dashed estimate line")` | 0 | 2 | Chart labelled with days/estimate; dashed estimate line drawn. |
| FR-11 (per-person running totals, lib) | `(cd web && bun test lib/progress.test.ts -t "FR-11: days are the union")` | 0 | 1 | Per-person running totals carry over skipped days. |
| FR-11a (last 14 days, older folded, WORKLOG_TZ) | `(cd web && bun test lib/progress.test.ts -t "FR-11a")` | 0 | 2 | 20 logged days give 14 points with older hours folded; exactly 14 not folded. TZ part: see next row. |
| FR-11a ($WORKLOG_TZ bucketing, added by T013) | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- worklog_day_follows_worklog_tz tempo_worklog_day_follows_worklog_tz` | 0 | 2 | With WORKLOG_TZ=-05:00, a 02:00Z Jira worklog and a 02:00Z Tempo worklog both land on 2026-10-01 (UTC date would be 10-02). |
| FR-12 (Show as table) | `(cd web && bun test components/ProgressChart.test.tsx -t "offers Show as table")` | 0 | 1 | Table view with cumulative per-person totals. |
| FR-13 (hover/arrow-key tooltip) | `(cd web && bun test components/ProgressChart.test.tsx -t "arrow keys walk the days")` | 0 | 1 | Arrow keys walk days; tooltip shows that day's totals. |
| FR-14 (no estimate: one muted line) | `(cd web && bun test components/EstimateBar.test.tsx -t "EstimateBar no estimate")` | 0 | 2 | Estimate absent and 0 show only the muted Jira line. |
| FR-15 (loading text + placeholder bar) | `(cd web && bun test components/EstimateBar.test.tsx -t "loading shows the Jira line")` | 0 | 1 | Loading shows "Loading hours from Jira…" and a placeholder bar. |
| FR-15a (page does not wait for estimate) | `(cd web && bun test components/BlockCard.test.tsx -t "B8")` | 0 | 1 | Block paints while the progress call never resolves. |
| B1 | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- estimates_parse_seconds_and_missing_is_none_not_zero worklogs_follow_pages_and_keep_every_author` | 0 | 2 | Estimate and every worklog collected across 2 pages. |
| B2 | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- exactly_ten_minutes_is_fresh_eleven_is_stale cache_one_second_past_ten_minutes_is_stale` | 0 | 2 | 11 min stale/refetched, 10 min fresh. |
| B3 | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- jira_failure_keeps_cache_and_flags_the_ticket` | 0 | 1 | Cached view returned with error flag on Jira failure. |
| B4 | `(cd web && bun test lib/progress.test.ts -t "B4")` | 0 | 1 | You/Jón Geir/pending, used 5h30m, over 1h30m. |
| B5 | `(cd web && bun test lib/progress.test.ts -t "FR-09: tone at 79%")` | 0 | 1 | ok/low/low/over. |
| B6 | `(cd web && bun test lib/progress.test.ts -t "B6/FR-06a")` | 0 | 1 | 4 named + Others. |
| B7 | `(cd web && bun test components/EstimateBar.test.tsx -t "EstimateBar no estimate")` | 0 | 2 | Only the muted Jira line. |
| B8 | `(cd web && bun test components/BlockCard.test.tsx -t "B8")` | 0 | 1 | Paints block and "Loading hours from Jira…" while unresolved. |
| B9 (jira_time Tempo fetch) | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- tempo_worklogs_map_fields_and_look_up_each_name_once tempo_worklogs_page_by_offset_until_a_short_page` | 0 | 2 | Tempo worklogs mapped (accountId, name, day, seconds), paged by offset. |
| B9 (daemon uses Tempo authors) | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- tempo_configured_people_are_the_tempo_authors` | 0 | 1 | With Tempo configured, people are the Tempo authors. |
