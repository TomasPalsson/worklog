# worklog

Personal time-tracker for the developer who hates timers. Aggregates activity
from Claude Code, GitHub, Google Calendar, and Jira into one local event log,
lets you review and clean it up in a web UI, and syncs the result to Tempo.

> **Rewrite status:** the project is transitioning from Python to Rust in
> five stages. **Stages 1.1, 1.2, 2, 3, 4, and 5 are live** — the Rust
> binary owns `worklog version`, `worklog setup`, `worklog db …`,
> `worklog secret …`, `worklog hook …`, `worklog schedule …`,
> `worklog collect jira|github|all`, `worklog sync`, `worklog infer`,
> `worklog estimate`, `worklog hook-run`, `worklog daemon`,
> `worklog web`, and now `worklog self-update` + `worklog dev`
> (signed Ed25519 delta-patch releases with auto-rollback). The Python
> FastAPI UI has been retired; Python now keeps only the Google Calendar
> collector until Stage 2.1. See [`rust/README.md`](rust/README.md) and
> the [Rewrite roadmap](#rewrite-roadmap) below.

## Architecture

```
┌──────────────────────────── your Mac ─────────────────────────────┐
│                                                                    │
│   ~/.local/share/worklog/worklog.db   (single source of truth,     │
│                                        WAL mode — safe RO readers  │
│                                        during concurrent writes)   │
│             ▲                     ▲                                │
│             │                     │                                │
│     writes (unix socket           │ reads direct                   │
│      OR TCP 127.0.0.1:9323)       │ via bun:sqlite                 │
│             │                     │                                │
│    ┌────────┴────────┐        ┌───┴───────────────────┐            │
│    │ worklog (Rust)  │        │ Docker: worklog-web   │            │
│    │  · CLI          │        │  · Bun + Next.js 15   │            │
│    │  · Collectors   │        │  · Server Components  │            │
│    │  · Estimator    │───────▶│  · Server Actions →   │            │
│    │  · axum daemon  │  TCP   │     host.docker       │            │
│    │  · web orch.    │        │     .internal:9323    │            │
│    └────────┬────────┘        └───────────────────────┘            │
│             │  spawns/manages                                       │
│             ▼                                                       │
└─────────────────────────────────────────────────────────────────────┘
```

Activity is classified to a Jira ticket via rules + the AI estimator. Unmatched
rows appear as "unassigned" in the web UI and can be reassigned by hand. The
web container reads SQLite directly via `bun:sqlite` (WAL mode = safe
concurrent reads), and writes flow through Next.js Server Actions → the Rust
daemon over TCP (Docker Desktop can't proxy live unix sockets through its
macOS VM, hence TCP between the container and host).

Only the Rust daemon ever writes to the DB — Server Actions are just a thin
shim that forwards the mutation and calls `revalidatePath`.

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
| `worklog collect jira` | rust | Refresh Jira ticket cache (Atlassian `/search/jql`) |
| `worklog collect github [--days]` | rust | Pull commits + PRs authored by the user |
| `worklog collect all` | mixed | jira + github via Rust, then gcal via Python |
| `worklog collect gcal` | python | Google Calendar events (Rust port in Stage 2.1) |
| `worklog sync [--day] [--dry-run]` | rust | POST reviewed blocks to Tempo Cloud v4 |
| `worklog infer [--day]` | rust | Gap-timeout clustering of events into blocks |
| `worklog estimate [--day] [--model]` | rust | `claude -p` fills jira + description + minutes |
| `worklog hook-run` | rust | New Rust hook-event handler invoked by Claude Code |
| `worklog daemon [--socket] [--tcp]` | rust | Axum API server — unix socket + TCP 127.0.0.1:9323 |
| `worklog web up [--port]` | rust | Bring up the dockerised Next.js review UI (and checks the daemon is reachable) |
| `worklog web down / status / logs / build` | rust | Container lifecycle |
| `worklog serve [--port]` | rust | Alias for `worklog web up` (legacy) |
| `worklog self-update [--check] [--dry-run] [--force]` | rust | Signed Ed25519 delta-patch self-updater with atomic swap + rollback |
| `worklog dev keygen / sign / make-patch / apply-patch` | rust | Maintainer tooling: Ed25519 keypair, detached signatures, bsdiff patches |
| `worklog doctor` | python | Environment + DB + auth sanity report |
| `worklog init` | python | Create dirs + DB (prefer `worklog setup`) |
| `worklog hook run` | python | Legacy hook handler (delegates to `hook-run` when Rust is present) |
| `worklog collect [all\|github\|gcal\|jira]` | python | Pull remote activity |
| `worklog today [--day …]` | python | Terminal summary |
| `worklog day [--day …]` | python | One-shot daily flow (collect → infer → estimate → `worklog web up`) |
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
| 2 | Rust collectors — jira tickets, github commits + PRs, tempo sync, httpmock test harness, Python delegation | ✅ shipped |
| 2.1 | Google Calendar collector (OAuth via browser flow + refresh-token cache) | ⏳ |
| 3 | axum unix-socket API + Rust infer + Rust estimator + Rust hook-run + `worklog daemon` | ✅ shipped |
| 4 | Next.js + Bun web container, dockerized, replaces FastAPI; `worklog web` orchestration; daemon gains TCP listener for Docker Desktop | ✅ shipped |
| 5 | Signed Ed25519 delta-patch self-updater, atomic swap with auto-rollback; `worklog dev {keygen,sign,make-patch,apply-patch}`; Python `worklog upgrade` routes to `worklog self-update` by default | ✅ shipped |

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
