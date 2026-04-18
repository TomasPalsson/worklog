# CLAUDE.md — worklog

Python 3.12 + uv + Typer + FastAPI. CLI at `src/worklog/cli.py`, web at
`src/worklog/web/app.py`, collectors in `src/worklog/collectors/`.

## Commands

```bash
uv run worklog <cmd>        # run the CLI
uv run pytest               # tests
uv run ruff check src       # lint
uv run mypy src             # types
```

## Conventions

- Stdlib datetimes are always UTC (`datetime.now(UTC)`). ISO-8601 in DB.
- Collectors MUST be idempotent: dedupe on `(source, source_id)` via
  `db.upsert_event`.
- Never print from `worklog hook run` except to stderr — it's wired to Claude
  Code and stdout would surface in the user's session.
- `company` on an event may be `None` (unassigned) — UI reassigns.
- `tempo_worklog_id` is the canary that prevents double-syncing; never clear it.

## Adding a collector

1. New module in `src/worklog/collectors/`.
2. Expose `collect(*, since, until, settings=None) -> int`.
3. Register in `cli.collect`.
4. Call `classify()` with the right signal before `upsert_event`.
