# Migrating from `uv tool install worklog` to the curl installer

Older versions of worklog shipped as a Python package installed via
`uv tool install`, which post-install rebuilt the Rust binary. As of
v0.3.x, the tool is pure Rust and ships as a signed binary from GitHub
Releases — no Python runtime, no build step.

## One-step migration

```bash
uv tool uninstall worklog
curl -fsSL https://raw.githubusercontent.com/TomasPalsson/worklog/main/install.sh | bash
```

The installer detects the old `uv tool install worklog` and refuses to
run unless you uninstall first (or pass `--force`). That's a guard,
not a limitation — a silent overlay would leave two binaries both
answering to `worklog` on PATH.

## What carries over without action

- **Database** (`~/.local/share/worklog/worklog.db`) — unchanged schema.
- **Config files** (`~/.config/worklog/.env`,
  `google_credentials.json`, `google_token.json`) — unchanged paths and
  formats. Your Google Calendar token keeps working after the port.
- **OS keychain secrets** — unchanged.
- **Claude Code hook** (`~/.claude/settings.json`) — still points at
  `worklog hook-run`, which now resolves to the Rust binary.
- **Scheduled collection** (launchd plist / systemd user unit) — still
  points at `worklog collect all`, which now runs the Rust collectors.

## What's different

- `worklog day` is now a Rust command, not a Python orchestrator. It
  runs the same pipeline (collect → infer → estimate → web up) but
  without a Python subprocess in the chain, and `--day` now drives the
  collection window (not "today" as before).
- `worklog upgrade` is the signed self-updater (Stage 5) — it verifies
  every release against the embedded Ed25519 pubkey before swapping.
- `worklog collect gcal` is a native Rust collector (not
  `google-api-python-client` wrapped in Typer).
- `worklog init` and `worklog today` are gone. `worklog setup` and
  `worklog day` cover their use cases.

## Rolling back

If something breaks and you need the Python release back:

```bash
rm ~/.local/bin/worklog
uv tool install 'git+ssh://git@github.com/TomasPalsson/worklog.git@v0.2.0'
```

v0.2.0 is the last `uv tool install` release. File an issue if you
need the rollback for real — it shouldn't be necessary, but the
escape hatch exists.
