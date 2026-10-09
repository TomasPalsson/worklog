Approved: 2026-10-08 by user
Base: 2777b1b
# Tasks — Google Calendar 3LO login
Spec: spec.md · Design: design.md · Base: 7b9cbb7 · Route: dispatch · Test: `cargo test --manifest-path rust/Cargo.toml`

## Behaviors
| ID | Given / When / Then | Task | Proven by |
|----|---------------------|------|-----------|
| B1 (P0) | Given a credentials file, when consent is granted, then a 0600 token file the refresh code accepts is written | T001 | gcal_auth::tests::happy_path_writes_refreshable_token |
| B2 (P0) | Given consent is denied, a forged state, a timeout, a token-endpoint error or no refresh token, when login runs, then it fails and the old token is unchanged | T001 | gcal_auth::tests::failure_paths_keep_existing_token |
| B3 (P0) | Given no credentials file, when login runs, then the error names the file | T001 | gcal_auth::tests::missing_credentials_names_path |
| B4 (P0) | Given the CLI, when `collect gcal --auth` runs, then it reaches the login routine; `--auth` with any other target is rejected | T002 | cli tests collect_gcal_auth_* |

## Phase 1 — Login routine
Goal: worklog can get a Google token on its own, tested against a fake Google.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core gcal_auth` — green with the CLI untouched.
- [x] T001 Loopback + PKCE login routine per design §1 (B1, B2, B3) — files: rust/crates/worklog-core/src/collectors/gcal_auth.rs, rust/crates/worklog-core/src/collectors/mod.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core gcal_auth` — done: 42adf48
- [x] T003 [P] README: Google Cloud Console Desktop-app credential steps + `worklog collect gcal --auth` — files: README.md — verify: `grep -q "collect gcal --auth" README.md` — done: b2fe5c1

## Phase 2 — Command wiring
Goal: the command every error message already names actually works.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-cli collect_gcal_auth` — green.
- [x] T002 `--auth` flag on `worklog collect` calling `gcal_auth::authorize` (B4) — files: rust/crates/worklog-cli/src/cli.rs, rust/crates/worklog-cli/tests/cli.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-cli collect_gcal_auth` — after: T001 — done: ea6ce0d
- [x] CHK001 human-verify a real Google login — files: rust/crates/worklog-cli/src/cli.rs — verify: human: Owner runs `worklog collect gcal --auth`, clicks Allow, then `worklog collect gcal` shows today's meetings — after: T002 — done: ea6ce0d by user

## Gates
- [x] G001 project gates clean — files: . — verify: `flow check --fix --since 7b9cbb7` — done: 75e4177
- [x] G002 branch review clean — files: . — verify: `flow pass` — done: 75e4177
- [x] G003 verification evidence exists — files: . — verify: `test -s verify/` — done: 75e4177
