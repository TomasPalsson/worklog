Discovered: infer_allocations::db_tests::two_sessions_same_customer_matches_no_customer_registry flakes under parallel cargo test (passes serially) — defer
Discovered: git.rs and billing_registry.rs are over the 400-line size guard (pre-existing) — defer
Discovered: that flake's cause — tz.rs/purge.rs tests set WORKLOG_TZ process-wide while build_day_blocks runs twice in the same test, so the two shapes can differ (1 fail in 5 runs on HEAD) — defer
Ruling: T002 'no timeout on git branch --show-current' — dropped at re-score 15 — local-only command, SessionStart hook is bounded by Claude Code — a hang would delay session start
Ruling: T007 duplicated 'latest pin <= t' lookup (tenant_split.rs vs session_customers.rs) — dropped at re-score 68 — copies are identical today — future drift if one changes
Ruling: T004 ≤600-char start text not capped for large customer lists — dropped at re-score 60 — ~20 customers needed to exceed; today's registry is far below — a long hint if the registry grows
Ruling: start_text_inherits_branch_pin creates+removes a random tempdir under the real ~/Desktop/Work — accepted — start_text has no injectable root; adding one means editing billing.rs/git.rs — a stray dir if the test is killed mid-run
Discovered: correction to the ≤600-char ruling — on the live registry (21 customers) with a real session id the start text is already 540 chars; 2–3 more customers break NFR — fold into a new task at converge
Discovered: day page logs a React duplicate-key warning — web/app/[day]/page.tsx:174-175 give UnsortedList and ElsewhereList the same key={day} (pre-existing, untouched by this branch) — defer
Ruling: T005 bare legacy worklog-hook binary gets '<path> session-hint' — dropped at re-score 78 — binary no longer built or shipped; a failing SessionStart command does not block the session — those installs get no start hint until re-installed
Ruling: T005 no dedicated uninstall test for the SessionStart pair — dropped at re-score 20 — reviewer ran one and it passes via the generic worklog sweep — a future change to uninstall could regress unnoticed
Ruling: CHK001 ticked --by user under the Owner's standing approval for this run — the real-Claude 'never asked' leg was not run (nested claude in another dir blocked by permissions) — see verify/CHK001.md
