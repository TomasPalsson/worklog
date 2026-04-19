# worklog

Personal time-tracker for the developer who hates timers. Pulls
activity from Claude Code, GitHub, Google Calendar, and Jira into one
local SQLite file, clusters it into blocks, uses `claude -p` to pick a
ticket and write a description for each block, and syncs the result to
Tempo Cloud after you review it in a local web UI.

Everything runs on your machine. Nothing leaves until you push a Tempo
sync — and you review every block before that happens.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/TomasPalsson/worklog/main/install.sh | bash
```

The script downloads a signed release binary for your platform
(macOS arm64 or linux x86_64) from GitHub Releases, verifies it, and
drops it at `~/.local/bin/worklog`. After that:

```bash
worklog setup          # one-shot onboarding: db + secrets + Claude hook
worklog day            # end-of-day: collect → infer → estimate → review UI
```

Upgrading is a single command and always verifies the signature:

```bash
worklog upgrade        # signed self-update via the release pipeline
```

> Migrating from the old `uv tool install`? See
> [`docs/MIGRATION.md`](docs/MIGRATION.md) — one step.

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

Events are classified to a Jira ticket by the AI estimator; unmatched
ones appear as "unassigned" in the web UI and are reassigned by hand.
The web container reads SQLite directly (WAL mode = concurrent reads
are safe), and writes flow through Next.js Server Actions → the Rust
daemon over TCP (Docker Desktop can't proxy live unix sockets through
its macOS VM, hence TCP between the container and host).

Only the Rust daemon writes to the DB — Server Actions are a thin
shim that forwards the mutation and calls `revalidatePath`.

## Quickstart

```bash
worklog setup          # preflight + interactive secrets + db migrate
worklog hook install   # register the Claude Code hook
worklog day            # full daily pipeline (collect → infer → estimate → UI)
```

To pull everything without running the UI:

```bash
worklog day --no-serve
```

## Commands

| Command | Purpose |
|---|---|
| `worklog version` | Print the embedded version |
| `worklog setup` | Preflight + secrets capture + db migrate |
| `worklog doctor` | Environment, DB, secrets sanity report |
| `worklog day [--day] [--no-serve] [--model]` | Full daily pipeline: collect → infer → estimate → web UI |
| `worklog collect [all\|jira\|github\|gcal] [--days]` | Pull remote activity |
| `worklog infer [--day]` | Gap-timeout clustering of events into blocks |
| `worklog estimate [--day] [--model]` | `claude -p` fills jira + description + minutes |
| `worklog sync [--day] [--dry-run]` | POST reviewed blocks to Tempo Cloud |
| `worklog web up [--port]` | Bring up the dockerised Next.js UI |
| `worklog web down / status / logs / build` | Container lifecycle |
| `worklog daemon [--socket] [--tcp]` | Axum API server (unix + TCP) |
| `worklog hook [install\|uninstall\|status]` | Claude Code hook wiring |
| `worklog schedule [install\|uninstall\|status]` | Scheduled collection (launchd / systemd --user) |
| `worklog secret [set\|get\|rm\|list]` | Credentials in the OS keychain |
| `worklog db [migrate\|info\|path]` | DB operations |
| `worklog upgrade` | Signed self-update |
| `worklog self-update [--check\|--dry-run\|--force]` | Lower-level alias for `upgrade` |
| `worklog dev [keygen\|sign\|make-patch\|apply-patch]` | Maintainer tooling |

Every subcommand accepts `--json` for structured output.

## Storage

- **DB:** `~/.local/share/worklog/worklog.db` (SQLite, WAL mode).
- **Config:** `~/.config/worklog/` (`.env`, `google_credentials.json`,
  `google_token.json`).
- **Secrets:** OS keychain under service name `worklog` (macOS Keychain,
  Linux secret-service, Windows Credential Manager). The `.env` file
  still works — secrets migrate lazily as you set them via
  `worklog secret set …` or re-run `worklog setup`.
- **Binaries + releases:** `~/.local/share/worklog/{bin,releases}/`.

Override everything with `$WORKLOG_HOME=<dir>` — collapses db, socket,
config, bin, logs, releases into one root. Primarily for tests and
power users.

## Dev

```bash
cargo test  --manifest-path rust/Cargo.toml
cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings
cargo fmt   --manifest-path rust/Cargo.toml --all

cd web && bun test && bun run typecheck && bun run build
```

Local release smoke (no network, no tag push):

```bash
bash scripts/release-smoke.sh
bash tests/install/smoke.sh
```

CI: [`.github/workflows/rust.yml`](.github/workflows/rust.yml) runs
fmt + clippy + tests on Linux and macOS on every push / PR;
[`.github/workflows/release.yml`](.github/workflows/release.yml) builds,
signs, and publishes on every `v*` tag.

## Files of interest

- `rust/crates/worklog-core/` — data layer: paths, db, repo, secrets,
  collectors, infer, estimator, signed updater
- `rust/crates/worklog-cli/` — the `worklog` binary: CLI + setup wizard
- `rust/crates/worklog-core/sql/schema.sql` — canonical SQL schema
- `web/` — the Next.js + Bun review UI (dockerised)
- `install.sh` — curl-piped installer
- `scripts/release-smoke.sh` — host-side dry-run of the release pipeline
