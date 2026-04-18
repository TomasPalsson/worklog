# worklog

Personal time-tracker for the developer who hates timers. Aggregates activity
from Claude Code, GitHub, Google Calendar, and Jira into one local event log,
lets you review and clean it up in a web UI, and syncs the result to Tempo.

> **Rewrite status:** the project is transitioning from Python to Rust in
> five stages. **Stages 1.1 + 1.2 are live** — the Rust binary owns
> `worklog version`, `worklog setup`, `worklog db …`, `worklog secret …`,
> `worklog hook install|uninstall|status`, and `worklog schedule …`. Every
> other command stays Python until its stage lands. See
> [`rust/README.md`](rust/README.md) and the
> [Rewrite roadmap](#rewrite-roadmap) below.

## Architecture (target)

```
┌─────────────────────── your Mac ──────────────────────────┐
│                                                            │
│   ~/.local/share/worklog/worklog.db   (single source of    │
│                                        truth, WAL mode)    │
│             ▲               ▲                              │
│             │               │                              │
│     writes via unix         │ reads direct (RO)            │
│      socket (axum IPC)      │ (bun:sqlite)                 │
│             │               │                              │
│    ┌────────┴────────┐  ┌───┴─────────────┐                │
│    │ worklog (Rust)  │  │ Docker:         │                │
│    │ · CLI           │  │ worklog-web     │                │
│    │ · Collectors    │  │ (Bun + Next.js) │                │
│    │ · Estimator     │  │                 │                │
│    │ · axum IPC      │  │ Server Actions  │                │
│    │ · Self-updater  │  │   → api.sock    │                │
│    └────────┬────────┘  └─────────────────┘                │
│             │  spawns/manages                              │
│             ▼                                              │
└────────────────────────────────────────────────────────────┘
```

Activity is classified to a Jira ticket via rules + the AI estimator. Unmatched
rows appear as "unassigned" in the web UI and can be reassigned by hand. The
web UI is read-heavy and connects directly to the same SQLite file via a
read-only mount — mutations cross a unix socket so only the Rust binary writes.

Today (Stage 1.1) the Rust binary handles db + secret + setup; the Python CLI
still owns collectors, estimator, hook, and the FastAPI web UI. Stages 2–5
migrate the rest.

## Install

```bash
# Python half (existing)
uv tool install git+ssh://git@github.com/TomasPalsson/worklog.git

# Rust half (built + installed by `worklog upgrade`)
worklog upgrade
```

`worklog upgrade` reinstalls the Python half via `uv tool install`, then
builds the Rust release binary and drops it at
`~/.worklog/bin/worklog-rs`. Python commands auto-delegate to it. If you
don't have `cargo`, grab Rust from <https://rustup.rs>.

## Quickstart

```bash
worklog setup          # interactive onboarding — preflight + secrets + db
worklog hook install   # register the Claude Code hook
worklog collect all    # pull last 7 days from GitHub / GCal / Jira
worklog serve          # review UI at http://127.0.0.1:8765
```

## Commands

Commands marked **rust** are implemented by the new binary and auto-delegated
from the `worklog` entrypoint. Everything else is Python (for now).

| Command | Owner | Purpose |
|---|---|---|
| `worklog version` | rust | Show Python + Rust binary versions |
| `worklog setup` | rust | Preflight + interactive secret capture + db migrate |
| `worklog db migrate` | rust | Initialize / migrate the DB (idempotent) |
| `worklog db info` | rust | One-line row-count summary |
| `worklog db path` | rust | Print the resolved DB path |
| `worklog secret set <key>` | rust | Stash a credential in the OS keychain |
| `worklog secret get <key>` | rust | Read a credential to stdout |
| `worklog secret rm <key>` | rust | Delete a credential |
| `worklog secret list` | rust | List known keys and whether each is set |
| `worklog hook install / uninstall / status` | rust | Claude Code hook wiring in `~/.claude/settings.json` |
| `worklog schedule install / uninstall / status` | rust | Scheduled collection (launchd or systemd --user) |
| `worklog doctor` | python | Environment + DB + auth sanity report |
| `worklog init` | python | Create dirs + DB (prefer `worklog setup`) |
| `worklog hook run` | python | Invoked by Claude Code; still Python until Stage 2 |
| `worklog collect [all\|github\|gcal\|jira]` | python | Pull remote activity |
| `worklog today [--day …]` | python | Terminal summary |
| `worklog infer [--day …]` | python | Rebuild blocks for a day |
| `worklog estimate [--day …]` | python | Run Claude estimator |
| `worklog sync [--day …] [--dry-run]` | python | Push to Tempo |
| `worklog serve` | python | Start the FastAPI review UI |
| `worklog day [--day …]` | python | One-shot daily flow |
| `worklog upgrade` | python | Reinstall Python + rebuild Rust binary |

Any Rust subcommand also accepts `--json` for structured output.

## Storage

- **DB:** `~/.local/share/worklog/worklog.db` (SQLite, WAL mode). The Rust
  crate embeds a byte-identical copy of the schema — a unit test fails if
  the two ever drift.
- **Config:** `~/.config/worklog/` (`.env`, `companies.yaml`, OAuth creds).
- **Secrets:** OS keychain under service name `worklog` (macOS Keychain,
  Linux secret-service, Windows Credential Manager). The old `.env` file
  still works — secrets migrate lazily as you re-enter them via
  `worklog secret set …` or re-run `worklog setup`.
- **Binaries:** `~/.worklog/bin/`.
- All data stays local until you run `worklog sync`.

Override everything with `$WORKLOG_HOME=<dir>` — collapses db, socket,
config, bin, logs, releases into one root. Primarily for tests and power
users.

## Rewrite roadmap

| Stage | Scope | Status |
|---|---|---|
| 1.1 | Rust skeleton: workspace, db + schema share, paths, secrets, `setup` wizard, CLI passthroughs | ✅ shipped |
| 1.2 | Scheduled collection (launchd/systemd), hook install inside Rust, wizard integration | ✅ shipped |
| 2 | Rust collectors (jira/github/gcal/tempo) — deprecate Python collectors | ⏳ |
| 3 | axum unix-socket API + Rust estimator | ⏳ |
| 4 | Next.js + Bun web container, dockerized, replaces FastAPI | ⏳ |
| 5 | Signed delta-patch self-updater with auto-rollback | ⏳ |

## Dev

### Python

```bash
uv sync
uv run pytest
uv run ruff check src
uv run mypy src
uv run worklog <cmd>
```

### Rust

```bash
cd rust
just check                  # fmt --check + clippy -D warnings + cargo test
just test                   # cargo test --all
just demo                   # run the wizard against /tmp/worklog-demo
cargo build --release --bin worklog
```

CI runs fmt + clippy + tests on Linux and macOS
([`.github/workflows/rust.yml`](.github/workflows/rust.yml)).

## Files of interest

- `rust/crates/worklog-core/` — shared data layer (paths, db, repo, secrets)
- `rust/crates/worklog-cli/` — the `worklog` Rust binary + `setup` wizard
- `rust/crates/worklog-core/sql/schema.sql` — canonical schema, byte-identical
  mirror of `src/worklog/schema.sql` (drift is test-enforced)
- `src/worklog/cli.py` — Python CLI, now auto-delegating `db`/`secret`/`version`
  to the Rust binary via `_exec_rust`
- `src/worklog/web/` — FastAPI review UI (until Stage 4 replaces it)
