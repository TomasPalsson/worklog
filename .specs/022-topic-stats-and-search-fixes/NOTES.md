# Notes — 022 topic stats and search fixes

- Ruling: T001 changed ask_test `invoice AND` from 0 to 1 hit — the spec'd behaviour change (FR-01/FR-02: "and" is a stopword, so the query is "invoice"); the literal-operator safety is kept by the new `AND` → 0 assert and `query("invoice AND") == "\"invoice\""` (2026-10-11, orchestrator)
- Ruling: T001 touched daemon_ask.rs:105 (outside its files:) — the `"kafka OR (` test expected 0 hits under AND; under OR it is 1. Same behaviour change, exact count kept; accepted (2026-10-11, orchestrator)
- Ruling: T008 streams Topics through Suspense so a slow helper never blocks /stats (beyond the brief, in its own files) (2026-10-11, orchestrator)
- Ruling: T006 left its fix-ladder tie-break edits uncommitted; orchestrator committed them as 3b04efc under T006's files only (2026-10-11, orchestrator)
- Discovered: rust/crates/worklog-core/templates/verdict_server.py:~38 — _load_engine raises a bare KeyError when WORKLOG_VERDICT_MODEL_* env is missing (pre-existing) — defer
- Discovered: rust/crates/worklog-core/src/billing.rs billing_tenant_tests::rows_for_day_with_deildir_under_200ms — timing test flakes under parallel load (pre-existing) — defer
