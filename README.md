# worklog

Unified work-time tracker. Aggregates activity from Claude Code, GitHub, Google
Calendar, and Jira into a single local event log, lets you review and classify
it per company in a web UI, and syncs the result to Tempo.

## Quickstart

```bash
uv sync
uv run worklog init
uv run worklog hook install      # registers the Claude Code hook
uv run worklog collect all       # pull last 7 days from GitHub / GCal / Jira
uv run worklog serve             # open http://127.0.0.1:8765 to review + sync
```

## How it works

```
┌──────────────┐  ┌────────┐  ┌───────┐  ┌──────┐
│ Claude hook  │  │ GitHub │  │ GCal  │  │ Jira │
└──────┬───────┘  └───┬────┘  └───┬───┘  └───┬──┘
       │              │           │          │
       └──────────────┴────┬──────┴──────────┘
                           ▼
                ┌─────────────────────┐
                │ ~/.local/share/     │
                │   worklog/worklog.db│  ← classified per company
                └──────────┬──────────┘
                           ▼
                ┌─────────────────────┐
                │ Review UI (FastAPI) │  ← edit durations + companies
                └──────────┬──────────┘
                           ▼
                ┌─────────────────────┐
                │ Tempo Cloud (v4)    │  ← one worklog per (day, company, issue)
                └─────────────────────┘
```

Activity is classified to a company via rules in `~/.config/worklog/companies.yaml`
(path prefix, GitHub repo, Jira project, calendar, or keyword match). Unmatched
rows appear as "unassigned" in the web UI and can be reassigned by hand.

## Configuration

Secrets: `~/.config/worklog/.env` (see `.env.example`).

Companies: `~/.config/worklog/companies.yaml` (scaffold written by `worklog init`).

Google OAuth: download a Desktop-app OAuth client from GCP → save as
`~/.config/worklog/google_credentials.json`. The first `collect gcal` opens a
browser once; token is cached.

## Commands

| Command | Purpose |
|---|---|
| `worklog init` | Create config + DB |
| `worklog hook install` | Add Claude Code hook to `~/.claude/settings.json` |
| `worklog hook uninstall` | Remove it |
| `worklog collect [all\|github\|gcal\|jira]` | Pull remote activity |
| `worklog today [--day YYYY-MM-DD]` | Terminal summary |
| `worklog sync [--day YYYY-MM-DD] [--dry-run/--no-dry-run]` | Push to Tempo |
| `worklog serve` | Start review UI |

## Dev

```bash
uv run pytest
uv run ruff check src
uv run mypy src
```

## Storage

- Events: `~/.local/share/worklog/worklog.db` (SQLite)
- Config: `~/.config/worklog/`
- All data stays local until you run `worklog sync`.
