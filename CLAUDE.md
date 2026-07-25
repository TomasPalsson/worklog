# CLAUDE.md — worklog

Pure-Rust CLI (`rust/crates/{worklog-core,worklog-cli}/`) plus a
dockerised Next.js + Bun review UI in `web/`. The Rust daemon listens
on unix socket `api.sock` + TCP `127.0.0.1:9323`; the web container
reads SQLite directly via `bun:sqlite` and writes via Server Actions
that call the daemon.

## Commands

```bash
cargo test  --manifest-path rust/Cargo.toml
cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings
cargo fmt   --manifest-path rust/Cargo.toml --all -- --check

cd web && bun test && bun run typecheck && bun run build
```

`bun run build` and `next dev` share `web/.next`, so running the build
while a dev server is up clobbers the running server's chunks — it then
dies with `Cannot find module './vendor-chunks/*.js'`. Stop the dev
server before a build, or recover with
`pkill -f "next dev"; rm -rf web/.next` and restart it.

Release smoke (no network, no tag push):

```bash
bash scripts/release-smoke.sh
bash tests/install/smoke.sh
```

## Conventions

- Stdlib datetimes are UTC (`Utc::now()`). ISO-8601 in DB. Gcal
  events are normalised to UTC at the collector boundary
  (`gcal::to_utc` in `rust/crates/worklog-core/src/collectors/gcal.rs`).
- Day bucketing for blocks uses the user's local TZ via `$WORKLOG_TZ`
  as a fixed offset (e.g. `-05:00`, `+01:00`, `UTC`). Default is UTC.
  Named zones (`America/New_York`) are not supported; DST observers
  update the env var when DST flips.
- Collectors MUST be idempotent: dedupe on `(source, source_id)` via
  `repo::upsert_event`.
- Never print to stdout from `worklog hook-run` — it's wired to Claude
  Code and stdout would surface in the user's session. Everything
  goes to stderr.
- `tempo_worklog_id` is the canary that prevents double-syncing —
  **never clear it**. Accepts `""` AND `NULL` as "unsynced" (see
  `tempo::normalise_tempo_id`).
- `estimated_by = 'manual'` blocks MUST NOT be overwritten by
  re-estimation.
- **Billing export** (`billing.rs` + `billing_registry.rs`) targets the
  external invoicing form, not Tempo. Two rules are load-bearing:
  nothing that lands on an invoice is invented — `Viðskiptamaður` comes
  from a folder pin or an unambiguous customer-alias match, `Verkefni`
  **only** from an explicit pin, and anything unresolved stays `None`
  for the user to fill in; and a line's hours are the **union** of its
  blocks' intervals (`round_to_half_hour`), never a naive sum, so
  overlapping blocks can't be double-billed. Personal blocks are
  excluded outright.
- A block's billable folder is the **project root** under
  `~/Desktop/Work` — `billing::work_folder_for_path` strips
  `/.claude/worktrees/*` and collapses sub-dirs, because the path
  basename is a branch name for worktree events.
- `exported_at` is the billing analog of `tempo_worklog_id`: set by
  `block_service::mark_exported` (idempotent), and accepted by
  `purge.rs` as proof a block was billed. Without it, work blocks
  become unpurgeable once Tempo sync stops being used.
- The billing registry lives in SQLite and is edited **only** through
  the review UI (Settings-adjacent Billing panel → daemon
  `/billing/*`). Do not add a config-file path for it.
- The embedded Ed25519 release pubkey lives at
  `rust/crates/worklog-core/src/updater/pubkey.rs`. The matching
  private key lives only in the `WORKLOG_RELEASE_PRIVATE_KEY` GHA
  secret. CI signs on every tag push; a unit test asserts the
  placeholder has been replaced so no accidental rebuild can ship
  an unsigned binary.

## Adding a collector

1. New module in `rust/crates/worklog-core/src/collectors/`.
2. Expose `collect(conn, auth, since, until) -> Result<CollectReport>`
   plus a test-injectable `collect_with(... client)` variant.
3. Wire into `worklog-cli`'s `Cmd::Collect` dispatch (`CollectTarget`).
4. Use `repo::upsert_event` for idempotent dedupe.
5. Inline `#[cfg(test)] mod tests` with httpmock fixtures; mirror the
   github/jira patterns.

## Release pipeline

- Push a tag `v*` → `.github/workflows/release.yml` runs on
  `macos-14` (arm64) + `ubuntu-24.04` (x86_64), signs each asset and
  the manifest, and creates a GitHub Release with eight files.
- Users install via:
  `curl -fsSL https://raw.githubusercontent.com/TomasPalsson/worklog/main/install.sh | bash`
- Subsequent upgrades: `worklog upgrade` → `worklog self-update`,
  which re-verifies every signature against the embedded pubkey.
