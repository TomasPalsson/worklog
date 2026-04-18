# worklog/rust — the Rust rewrite

Stages 1.1 + 1.2 of the rewrite are here. The Python CLI keeps working in
parallel; the Rust binary owns its own copy of the schema and a
superset-compatible view of the same SQLite database at
`~/.local/share/worklog/worklog.db`.

## Layout

```
rust/
├── Cargo.toml                  # workspace
├── Justfile                    # just check | fmt | test | demo | release
├── crates/
│   ├── worklog-core/           # lib: paths, db, models, repo, secrets
│   │   ├── sql/schema.sql      # embedded via include_str! — mirror of
│   │   │                       #   ../../src/worklog/schema.sql (tested for
│   │   │                       #   byte-equality so the two cannot drift)
│   │   └── src/{lib,paths,db,models,repo,secrets,hook,schedule}.rs
│   └── worklog-cli/            # bin: the `worklog` binary
│       ├── src/{main,lib,cli,wizard}.rs
│       └── tests/cli.rs        # assert_cmd end-to-end tests
```

## Commands

```bash
worklog version                 # print version
worklog doctor                  # env + db + secret report
worklog setup                   # interactive onboarding wizard
worklog setup --non-interactive # idempotent, no prompts (CI-friendly)
worklog db migrate              # create / upgrade the db (idempotent)
worklog db info                 # row counts + schema version
worklog db path                 # print resolved db path
worklog secret set <key>        # stash a credential in the OS keychain
worklog secret get <key>        # print a credential to stdout
worklog secret rm  <key>        # delete a credential
worklog secret list             # show which KNOWN_KEYS are set
worklog hook install            # register worklog in ~/.claude/settings.json
worklog hook uninstall          # remove our handlers (leaves others alone)
worklog hook status             # show which events are hooked
worklog schedule install \
       --interval 15m           # launchd plist (macOS) or systemd timer (Linux)
worklog schedule uninstall
worklog schedule status
```

All commands accept `--json` for machine-readable output and honour
`$WORKLOG_HOME` as the root for the db / config / socket / logs.

## Keychain scoping

Secrets live under service name `worklog` in the OS keychain
(Keychain Access on macOS, secret-service on Linux, Credential Manager on
Windows). Known keys are declared in [`secrets.rs`](crates/worklog-core/src/secrets.rs#L14)
so the wizard and `doctor` can list them without introspecting the keychain.

Tests never touch the real keychain:
- Unit tests in `worklog-core` use a `cfg(test)` in-process HashMap.
- Integration tests in `worklog-cli` set `WORKLOG_SECRETS_FILE=<tempdir>/secrets.json`
  which diverts the production backend into a JSON file.

## Tests

```bash
just check     # fmt + clippy + test  (what CI runs)
just test      # cargo test --all
just demo      # run the wizard against /tmp/worklog-demo
```

CI: [`.github/workflows/rust.yml`](../.github/workflows/rust.yml) runs
`fmt --check`, `clippy -D warnings`, and `cargo test --all` on Linux + macOS.
