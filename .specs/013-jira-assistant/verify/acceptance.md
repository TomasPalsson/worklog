# Acceptance — 013 Jira assistant @ 613d22c (2026-10-04)

All commands run this session; `cargo test --manifest-path rust/Cargo.toml` exit 0 (1465 passed) covers every Rust row.

| Criterion | Proof (command / test) | Exit | Status |
|---|---|---|---|
| B1 FR-02 start moves To Do | `cargo test -p worklog-core ticket_assist` (start tests) | 0 | met |
| B2 FR-03 others untouched | same (In Progress/Done/GOJ get 0 POSTs) | 0 | met |
| B3 FR-04 never auto-Done | start only targets "In Progress"; `status_hints` read-only | 0 | met |
| B4 FR-05 move by name, non-GENAI refused | `ticket_assist`, `daemon_assist` move tests | 0 | met |
| B5 FR-07 create + start, bare-number account | `ticket_assist` create test | 0 | met |
| B6 FR-08/09/11/24/25 refusals | `ticket_assist` (allowed list, empty/500, emoji, failed move, failed decision log) | 0 | met |
| B7 FR-10 ADF shapes | `cargo test -p worklog-core ticket_text` (11) | 0 | met |
| B8 FR-12 suggest ≤3 + empty log relearns | `account_clues` (9), `daemon_assist` suggest_on_an_empty_clue_log_relearns_first | 0 | met |
| B9 FR-13 relearn | `account_clues` relearn_rebuilds | 0 | met |
| B10 FR-14/15/23 decisions, 2-wrong drop | `account_clues` wrong_twice_drops; CLI `--guessed <id>` | 0 | met |
| B11 FR-16/17 merged PR → hint | `github` records_merged_at, `status_hints` (3) | 0 | met (first Jira key per PR only — Ruling) |
| B12 FR-18 web chip | `cd web && bun test TaskDoneHint` (4) | 0 | met |
| B13 FR-21 session-start hints | `cargo test -p worklog-cli session_hint` | 0 | met |
| FR-01 ticket get | `cargo test -p worklog-cli ticket_cmd` view render | 0 | met |
| FR-06 find ≤5, open assigned first | `ticket find` uses FIND_LIMIT=5 | 0 | partial — ordering is Jira's updated-DESC (Ruling) |
| FR-19/20 skill routing + text rules | `cargo test -p worklog-core skill` (8); `worklog skill install` writes references/jira.md | 0 | met |
| Launch: ticket text format picked | verify/CHK001.md (Sample 2) | — | met (pre-approved pick) |
| Launch: clippy/tests/typecheck green | verify/gates.md | 0 | met (1 pre-existing web test fails on base too) |
| Launch: live Claude Code chat (CHK002) | not run — needs the new binary installed, the daemon restarted, and real Jira writes | — | **unmet** |
