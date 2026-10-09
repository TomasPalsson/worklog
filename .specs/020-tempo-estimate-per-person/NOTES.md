# Notes — 020

- Discovered: rust/crates/worklog-core/src/collectors/jira.rs:584 — `get_all_pages` stops paging when a response has no `total` (silent one-page truncation); Jira's worklog endpoint sends `total`, so not hit here — defer
- Discovered: rust/crates/worklog-core/src/collectors/jira_time.rs — worklogs use Jira's default page size, not §5's "100 per page"; paging is correct, only the call count differs — defer
- Discovered: rust/crates/worklog-core/src/ticket_progress.rs:mark_stale — writes epoch into `pulled_at`, so a failed refresh after a sync would read "Jira numbers from 1970"; the web must treat the epoch `pulled_at` as unknown — fold into T008
- Discovered: rust/crates/worklog-core/sql/schema.sql — `ticket_progress_worklogs.worklog_id` is the sole PK; Jira ids are globally unique, so harmless — defer
- Ruling: T002 also made `get_json` and `get_all_pages` `pub(crate)` (visibility only) to reuse paging/error handling instead of copying it; design said `str_at` only.
- Discovered: web/lib/types.ts — 721 lines vs the 400-line size guard (688 before this feature); design §1 appends here — defer
