# worklog/rust — the Rust rewrite

All five stages of the rewrite are here. The Python CLI keeps working
in parallel (minus the FastAPI web, retired in stage 4, and `upgrade`,
which now delegates to `worklog self-update`); the Rust binary owns its
own copy of the schema and a superset-compatible view of the same
SQLite database at `~/.local/share/worklog/worklog.db`. Stage 3 added
the axum daemon; stage 4 added a TCP listener alongside the unix socket
(for Docker Desktop) and a `worklog web` subcommand; stage 5 added a
signed Ed25519 delta-patch self-updater with atomic swap + rollback,
and a `worklog dev` group for the release-signing workflow.

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
│   │   ├── sql/schema.sql      # embedded via include_str! — mirror of
│   │   │                       #   ../../src/worklog/schema.sql (tested for
│   │   │                       #   byte-equality so the two cannot drift)
│   │   ├── templates/docker-compose.yml  # embedded, rendered by
│   │   │                       #   `worklog web up` into the data dir
│   │   └── src/{lib,paths,db,models,repo,secrets,hook,schedule,http}.rs
│   │       + src/{infer,sessions,hook_run,estimate,block_service,daemon,web}.rs
│   │       + src/updater/{mod,crypto,delta,fetch,install,manifest,pubkey,signing}.rs
│   │       + collectors/{jira,github,tempo}.rs
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
worklog collect all             # jira tickets + github (commits + PRs)
worklog collect jira            # just the ticket cache
worklog collect github --days 7 # pull last 7 days of commits + PRs
worklog sync --day 2026-04-18 \
             --dry-run          # preview Tempo POSTs
worklog sync --day 2026-04-18   # actually sync to Tempo
worklog infer --day 2026-04-18  # cluster events into blocks
worklog estimate --day 2026-04-18 \
               --model claude-haiku-4-5
worklog hook-run                # invoked by Claude Code via stdin JSON
worklog daemon                  # axum IPC over ~/.local/share/worklog/api.sock
```

## Daemon (Stage 3.2)

`worklog daemon` binds a unix socket at `api.sock` (chmod 0600) and serves a
small HTTP/1.1 API the Next.js web UI talks to via Server Actions:

| Route | Body | Purpose |
|---|---|---|
| `GET /health` | — | liveness + version |
| `GET /blocks/:day` | — | list blocks for `YYYY-MM-DD` |
| `POST /blocks/:id/ticket` | `{"jira_issue": string \| null}` | assign ticket |
| `POST /blocks/:id/duration` | `{"minutes": u32}` | set duration (marks manual) |
| `POST /blocks/:id/description` | `{"description": string}` | set description (marks manual) |
| `POST /blocks/:id/delete` | — | remove the block |
| `POST /infer` | `{"day": "YYYY-MM-DD"}` | re-cluster events for a day |
| `POST /jira/refresh` | — | refresh open-ticket cache |

Talk to it with `curl --unix-socket $(worklog db path | xargs dirname)/api.sock …`.

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
