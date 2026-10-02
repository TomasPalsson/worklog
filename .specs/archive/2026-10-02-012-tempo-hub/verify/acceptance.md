# Acceptance — 012 Tempo hub (run 2026-10-02 at 3ae2914)

Rust commands: `cargo test --manifest-path rust/Cargo.toml -p worklog-core <filter>`. Web commands: `cd web && bun test <file>`.

| Criterion | Behavior | Command (filter / file) | Exit | Showed |
|---|---|---|---|---|
| FR-01 open assigned tickets listed | B1 | `task_board` | 0 | 10 passed — `done_and_external_tickets_without_lines_are_hidden_but_null_category_stays` |
| FR-02 other worked tickets listed unassigned | B2 | `task_board` | 0 | `worked_ticket_outside_the_assigned_set_is_listed_unassigned` |
| FR-03 week/today hours from effective hours | B3 | `task_board` | 0 | `hours_override_counts_instead_of_union` |
| FR-04 full refresh marks unreturned done | B4 | `collectors::jira` | 0 | 20 passed — `refresh_pages_fully_stores_category_and_marks_unreturned_done` |
| FR-04a failed page marks nothing | B16 | `collectors::jira` | 0 | `refresh_failing_page_does_not_mark_done` |
| FR-05 list live transitions | B5 | `collectors::jira` | 0 | `list_transitions_maps_target_status_and_category` |
| FR-06 apply transition, cache = Jira status | B5 | `daemon_tasks` | 0 | 9 passed — `transition_stores_the_status_jira_reports_afterwards` |
| FR-07 comment 1–5000 chars, else 400 | B6 | `daemon_tasks` | 0 | `comment_that_is_empty_blank_or_too_long_is_a_400` |
| FR-08 draft keeps only a live transition id | B7 | `task_draft` | 0 | 12 passed — `draft_drops_a_foreign_transition_id` |
| FR-09 pull a week's worklogs + schedule | B8 | `daemon_week` | 0 | 5 passed — `pull_stores_the_weeks_worklogs_and_schedule_and_reports_counts` |
| FR-10 owner = worklog / outside | B8 | `tempo_remote` | 0 | 5 passed — `classifies_owner_by_block_tempo_worklog_id` |
| FR-11 sync skips line with outside worklog | B9 | `collectors::tempo` | 0 | 32 passed — `sync_skips_line_with_outside_worklog_b9` |
| FR-12 / 12a logged and synced hours | B10 | `week_closeout` | 0 | 7 passed — `logged_sums_ticket_lines_and_synced_counts_only_clean_lines` |
| FR-12b in-Tempo and outside hours | B10 | `week_closeout` | 0 | `tempo_and_outside_seconds_and_latest_pull_come_from_pulled_rows` |
| FR-12c required or "not pulled" | B10 | `components/WeekCloseout.test.tsx` | 0 | 10 pass — "shows 'not pulled' when required is unknown and never flags it" |
| FR-12d unticketed hours, pending count | B10 | `week_closeout` | 0 | `unticketed_sums_raw_seconds_of_untagged_work_blocks_only`, `line_with_one_unsynced_block_is_pending_and_not_synced` |
| FR-12e gap flag | B17 | `components/WeekCloseout.test.tsx` | 0 | "flags 7.5h…", "does not flag exactly 8h", "does not flag a day that requires 0h", "gives a gap row the warning class" |
| FR-13 week sync, pending days in order | B12 | `lib/weekSync.test.ts` | 0 | 4 pass — "B12: pulls, syncs each pending day in order, then pulls again" |
| FR-13a stop at failing day | B15 | `lib/weekSync.test.ts` | 0 | "B15: stops at the failing day and never sends the next one" |
| FR-13b no double start; Post disabled in flight | — | `components/WeekCloseout.test.tsx`, `components/TaskBoard.test.tsx` | 0 | "disables both buttons while a pull is in flight"; "Post is disabled while a post is in flight" (mutation-checked) |
| FR-14 headers link to My Tasks | B14 | `components/DayHeader.test.tsx` | 0 | 10 pass — "day header links to /tasks", "week header links to /tasks" |
| FR-15 pull before and after | B12 | `lib/weekSync.test.ts` | 0 | call order pull, days, pull |
| Jira 400 on transition → 502 with body | B11 | `daemon_tasks` | 0 | `jira_rejecting_a_transition_is_a_502_with_jiras_body_and_no_cache_change` |
| Task card links to Jira, shows week/today hours | B13 | `components/TaskBoard.test.tsx` | 0 | "splits assigned tickets from other worked ones and shows hours" |
| Journey 1 happy: Post applies kept suggestion + comment | — | `components/TaskBoard.test.tsx` | 0 | "Post applies the kept suggested transition before the comment" |
| Journey 1 error: rejected transition keeps draft | — | `components/TaskBoard.test.tsx` | 0 | "a rejected transition shows Jira's error, keeps the draft, and posts no comment" |
| Journey 2 error: failed day named, later days unsent | — | `components/WeekCloseout.test.tsx` | 0 | "names the failed day and its error and does not send later days" |
| Launch: all project gates green | — | see PASS-3ae2914.md | 0 | Rust 1347 passed, web 414 pass, fmt/clippy/typecheck/build 0 |
| Launch: Owner check (real Jira + Tempo) | — | human | — | On the user's word ("checked … push through"); not witnessed by the model — see CHK016.md |

All rows met. The Owner-check row rests on the user's statement, not on an observed run.
