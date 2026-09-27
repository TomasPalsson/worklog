Discovered: infer_allocations::db_tests::two_sessions_same_customer_matches_no_customer_registry flakes under parallel cargo test (passes serially) — defer
Discovered: git.rs and billing_registry.rs are over the 400-line size guard (pre-existing) — defer
Discovered: that flake's cause — tz.rs/purge.rs tests set WORKLOG_TZ process-wide while build_day_blocks runs twice in the same test, so the two shapes can differ (1 fail in 5 runs on HEAD) — defer
Ruling: T002 'no timeout on git branch --show-current' — dropped at re-score 15 — local-only command, SessionStart hook is bounded by Claude Code — a hang would delay session start
Ruling: T007 duplicated 'latest pin <= t' lookup (tenant_split.rs vs session_customers.rs) — dropped at re-score 68 — copies are identical today — future drift if one changes
Ruling: T004 ≤600-char start text not capped for large customer lists — dropped at re-score 60 — ~20 customers needed to exceed; today's registry is far below — a long hint if the registry grows
Ruling: start_text_inherits_branch_pin creates+removes a random tempdir under the real ~/Desktop/Work — accepted — start_text has no injectable root; adding one means editing billing.rs/git.rs — a stray dir if the test is killed mid-run
