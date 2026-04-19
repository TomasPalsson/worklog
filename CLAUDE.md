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
