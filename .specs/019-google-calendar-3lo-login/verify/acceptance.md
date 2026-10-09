# Acceptance — 019 Google Calendar 3LO login

Run 2026-10-09 at HEAD c79ffcb. Each test ran alone with `--exact`; Exit is 0 only when the
runner reported `1 passed` (a filtered-out or skipped test counts as 1).

| Criterion | What | Command | Exit |
|---|---|---|---|
| FR-01 | --auth reaches the login | `cargo test -p worklog-cli collect_gcal_auth_runs_login_and_reports_missing_credentials --exact` | 0 |
| FR-02 | reads installed.client_id/secret | `cargo test -p worklog-core happy_path_writes_token_the_refresh_routine_accepts --exact` | 0 |
| FR-03a | missing creds names path | `cargo test -p worklog-core missing_credentials_names_the_path_and_writes_nothing --exact` | 0 |
| FR-03b | unparsable creds names path | `cargo test -p worklog-core unparsable_credentials_names_the_path --exact` | 0 |
| FR-04 | read-only scope, offline, consent | `cargo test -p worklog-core consent_url_asks_for_read_only_offline_access_on_loopback --exact` | 0 |
| FR-05 | PKCE verifier matches challenge | `cargo test -p worklog-core happy_path_writes_token_the_refresh_routine_accepts --exact` | 0 |
| FR-06 | forged state stops, token unchanged | `cargo test -p worklog-core forged_state_stops_at_once_and_leaves_token_unchanged --exact` | 0 |
| FR-06b | code without state rejected | `cargo test -p worklog-core code_without_state_is_rejected --exact` | 0 |
| FR-07 | link printed when browser can't open | `cargo test -p worklog-core unsupported_opener_still_prints_the_consent_link --exact` | 0 |
| FR-08 | token shape accepted by refresh routine | `cargo test -p worklog-core happy_path_writes_token_the_refresh_routine_accepts --exact` | 0 |
| FR-09 | token file 0600 | `cargo test -p worklog-core happy_path_writes_token_the_refresh_routine_accepts --exact` | 0 |
| FR-10 | denied login leaves token unchanged | `cargo test -p worklog-core denied_consent_reports_cancelled_in_browser_and_terminal --exact` | 0 |
| FR-11a | times out, token unchanged | `cargo test -p worklog-core consent_never_finished_times_out_and_leaves_token_unchanged --exact` | 0 |
| FR-11b | timeout constant is 300 s | `cargo test -p worklog-core default_opts_use_the_documented_constants --exact` | 0 |
| FR-12 | cancel message in browser + terminal | `cargo test -p worklog-core denied_consent_reports_cancelled_in_browser_and_terminal --exact` | 0 |
| FR-13 | no refresh token -> reset hint | `cargo test -p worklog-core response_without_refresh_token_tells_owner_how_to_reset_consent --exact` | 0 |
| FR-14a | gcal --auth accepted (FR-03 msg) | `cargo test -p worklog-cli collect_gcal_auth_runs_login_and_reports_missing_credentials --exact` | 0 |
| FR-14b | --auth on other targets rejected | `cargo test -p worklog-cli collect_gcal_auth_rejects_other_targets --exact` | 0 |
| FR-15 | token endpoint 400 -> status, unchanged | `cargo test -p worklog-core token_endpoint_error_carries_status_and_leaves_token_unchanged --exact` | 0 |
| FR-16 | listener bind failure -> one-line message (spec: code review, no test) | `grep -q 'opening the local listener for the Google redirect' rust/crates/worklog-core/src/collectors/gcal_auth.rs` | 0 |
| NFR-timeout | 300 s wait limit | `cargo test -p worklog-core default_opts_use_the_documented_constants --exact` | 0 |
| NFR-loopback | listener bound to loopback | `cargo test -p worklog-core listener_binds_loopback_only --exact` | 0 |
| NFR-favicon | unrelated requests don't end the wait | `cargo test -p worklog-core unrelated_requests_do_not_end_the_wait --exact` | 0 |
| NFR-0600 | token file 0600 | `cargo test -p worklog-core happy_path_writes_token_the_refresh_routine_accepts --exact` | 0 |
| Launch: clippy + fmt | gates | `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings` / `cargo fmt --manifest-path rust/Cargo.toml --all -- --check` | 0 |
| Launch: whole suite | gates | `cargo test --manifest-path rust/Cargo.toml` (1971 passed, 0 failed) | 0 |
| NFR-owner-time / Launch CHK001 | real login, today's meetings land | human — see verify/CHK001.md | human |

## Behaviors (TASKS.md)

| Behavior | Rows above that prove it | Exit |
|---|---|---|
| B1 consent → 0600 token the refresh code accepts | FR-02, FR-05, FR-08, FR-09 | 0 |
| B2 deny / forged state / timeout / endpoint error / no refresh token → fails, old token unchanged | FR-06, FR-10, FR-11a, FR-15, FR-13 | 0 |
| B3 no credentials → error names the file | FR-03a, FR-03b | 0 |
| B4 `collect gcal --auth` reaches login; `--auth` elsewhere rejected | FR-01, FR-14a, FR-14b | 0 |

Note: FR-14b returned 1 once on an earlier pass that ran while 21 review agents were compiling the
same tree; it then passed 5/5 alone and again in this full pass. The test is hermetic (TempDir home).
FR-16 is "code review, no test" by the spec's own measure; the row proves the one-line context exists.
