# CLAUDE.md — worklog

Hybrid Python + Rust tool. Python 3.12 + uv + Typer lives in
`src/worklog/` (CLI shim + Google Calendar collector). Everything else
— other collectors, infer, estimator, tempo sync, axum daemon,
self-updater — is Rust, under `rust/crates/{worklog-core,worklog-cli}/`.
The review UI is a dockerised Next.js + Bun app in `web/`, talking to
the Rust daemon over TCP `127.0.0.1:9323`. FastAPI was retired in stage 4.

## Commands

```bash
uv run worklog <cmd>         # Python entry (delegates to Rust when installed)
uv run pytest                # Python tests
uv run ruff check src        # Python lint
uv run mypy src              # Python types

cargo test --manifest-path rust/Cargo.toml           # Rust tests
cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings

cd web && bun test && bun run typecheck              # web tests + TS
```

## Conventions

- Stdlib datetimes are always UTC (`datetime.now(UTC)` in Python, `Utc::now()`
  in Rust). ISO-8601 in DB. Gcal events are normalised to UTC at the
  collector boundary (`_to_utc` in `src/worklog/collectors/gcal.py`).
- Day bucketing for blocks uses the user's local TZ, driven by
  `$WORKLOG_TZ` as a fixed offset (e.g. `-05:00`, `+01:00`, `UTC`).
  Default is UTC. Named zones like `America/New_York` are not
  supported; DST-observers update the env var when DST flips.
- Collectors MUST be idempotent: dedupe on `(source, source_id)` via
  `db.upsert_event` (Python) / `repo::upsert_event` (Rust).
- Never print from `worklog hook run` except to stderr — it's wired to Claude
  Code and stdout would surface in the user's session.
- `company` on an event may be `None` (unassigned) — UI reassigns.
- `tempo_worklog_id` is the canary that prevents double-syncing; never clear it.
  Accepts "" AND NULL as "unsynced" (see `tempo::normalise_tempo_id`).
- `estimated_by = 'manual'` blocks MUST NOT be overwritten by re-estimation
  (both Python and Rust skip them).
- `rust/crates/worklog-core/sql/schema.sql` must stay byte-identical to
  `src/worklog/schema.sql`. A test enforces it.

## Adding a collector (Python)

1. New module in `src/worklog/collectors/`.
2. Expose `collect(*, since, until, settings=None) -> int`.
3. Register in `cli.collect`.
4. Call `classify()` with the right signal before `upsert_event`.

## Adding a collector (Rust)

1. New module in `rust/crates/worklog-core/src/collectors/`.
2. Expose `collect*()` returning `CollectReport`.
3. Wire into `worklog-cli`'s `Cmd::Collect` dispatch.
4. Use `repo::upsert_event` to insert with idempotent dedupe.
