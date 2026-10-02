Ruling: 2026-10-01 CHK016 gained `after: T013, T014, T015` — it had no dependency and was scheduled in wave 1; ordering fix only, no scope change.
Discovered: rust/crates/worklog-core/src/db.rs over 400-line size guard (430) after T001 — defer
Discovered: daemon_week.rs parse_monday/upstream_as_bad_gateway duplicate daemon_tasks.rs requested_monday/hub_error — fold into gate fix
Discovered: week close-out Gap cell is styled like OK cells (sage) — needs a gap class in WeekCloseout.tsx + warning colour — fold into gate fix
Ruling: 2026-10-02 user chose to defer CHK016 (live Jira/Tempo check) — gates, review and draft PR proceed; CHK016 stays open on the PR for the user.
