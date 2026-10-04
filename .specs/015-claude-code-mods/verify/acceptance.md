# Acceptance — 015 Claude Code terminal mod (head 867bdc3, 2026-10-04)

Every row's command was run at 867bdc3; output excerpts are from that run.

| Criterion | Command run now | Exit | What it showed |
|---|---|---|---|
| B1 branch ticket is the last-resort source | `cargo test -p worklog-core branch_key_is_used_when_prompt_and_path_have_none` | 0 | 1 passed |
| B2 prompt key beats branch key | `cargo test -p worklog-core prompt_key_beats_branch_key` | 0 | 1 passed |
| B3 install twice: files once, path once, other values kept | `cargo test -p worklog-core claude_mod` | 0 | 5 passed (install ×2, uninstall ×3) |
| B4 only work hours, shown on the PromptHint tail | `claude plugin test mods/worklog` | 0 | "session start puts on the prompt hint tail worklog hours counting only non-personal, non-ignored blocks", "refreshes every 60 seconds" pass |
| B5 one reminder across two sessions | `claude plugin test mods/worklog` | 0 | "toast once and store the marker", "existing marker for today suppresses the toast in a later session" pass |
| B6 commit/PR attribution ends with `Ticket: GENAI-9` | `claude plugin test mods/worklog` | 0 | "commit and PR attribution end with the ticket trailer" pass |
| B7 `/wl review`, `t` + `GENAI-1` + Enter posts the ticket | `claude plugin test mods/worklog` | 0 | "t sends the typed ticket on Enter, and null for empty text" pass |
| Launch 1: every MUST in §4 has a passing test | FR→test map (claim-check run) + `claude plugin test` + `cargo test` | 0 | FR-01…FR-16 each mapped to a passing test; FR-11 by `claude plugin validate` (no tool.call/permission hook) |
| Launch 2: each journey's error path is exercised | `claude plugin test mods/worklog` | 0 | down daemon (status, reminder), refused action, refused `i` keeps block, empty day, outside work folder — all pass (gaps closed by T012, T013) |
| Launch 3: install + live session; cargo test and plugin test green | `worklog hook install` (real) + user live session; `cargo test --manifest-path rust/Cargo.toml`; `claude plugin test mods/worklog` | 0 / 0 / 0 | CHK001 approved by user (verify/CHK001.md); cargo all suites ok; 56 pass, 0 fail |

All rows met.
