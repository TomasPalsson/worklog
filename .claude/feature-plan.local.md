# Feature: Delete Python — Pure-Rust worklog

**Branch:** `feature/delete-python`  **Size:** Large

## Why

Python is a second implementation of half the product (collectors, infer,
estimate) and a thin Typer shim for the other half. That drift has
already bitten us: `worklog day` is Python-only and shells out to a stale
Rust binary that 404s on `web up`. CLAUDE.md's north-star says
"everything else is Rust" — this feature makes that literally true by
deleting the Python package and replacing `uv tool install` with a
signed curl installer fed by a GitHub Actions release pipeline.

## Decisions (user-approved 2026-04-19)

- **Path B** — delete Python entirely, no hybrid.
- **Gcal**: port to Rust now.
- **Install**: `curl … | sh` only. No Homebrew tap.
- **Signing**: Ed25519 private key in GHA secret. CI signs on tag push.
- **Platforms**: `aarch64-apple-darwin` + `x86_64-unknown-linux-gnu`. No
  intel mac, no linux-arm, no windows.

## Assumptions (since you said "you decide")

- Install path: `$HOME/.local/bin/worklog`.
- Installer URL: `https://raw.githubusercontent.com/TomasPalsson/worklog/main/install.sh`.
- Release artifact names: `worklog-<target>` (version carried by the signed manifest, not filename).
- `install.sh` is bash (not pure POSIX) — available on both targets, stays readable.
- Migration: detect-and-warn on existing `uv tool install worklog`, not auto-uninstall.
- CLAUDE.md rewrite: remove the "Python 3.12 + uv + Typer" carve-out. New invariant: pure Rust CLI + Next.js web UI.

## Risks (plan highest-first)

1. **Gcal OAuth port** — `google-api-python-client` wraps OAuth2 gracefully; Rust equivalents are verbose. Risk: users lose calendar events. → Phase 1 (first).
2. **Signing key rotation** — once pubkey X is shipped, switching to pubkey Y breaks upgrades for every user on X. → Get it right in Phase 3.
3. **Existing user migration** — anyone on `uv tool install` must uninstall old before curling new. Silent-fail strands them. → Installer detects + instructs.

## Behavior Inventory

| # | Given | When | Then |
|---|-------|------|------|
| 1 | Rust binary from this branch | `worklog day` | Rust dispatches collect → infer → estimate → web up; no Python process spawned |
| 2 | Rust binary, `credentials.json` present | `worklog collect gcal --since 2026-04-01 --until 2026-04-19` | Events fetched via Calendar API, normalised to UTC, upserted by `(source='gcal', source_id=<iCalUID>)` |
| 3 | Rust binary, no credentials | `worklog collect gcal` | Actionable error pointing at the expected path |
| 4 | Rust binary, expired refresh token | `worklog collect gcal` | Attempts refresh, if that fails instructs user to re-auth |
| 5 | Tag `v0.3.0` pushed | GHA release workflow runs | Per platform: `worklog-<target>`, `worklog-<target>.sig`, signed `manifest.json` uploaded |
| 6 | Tag pushed with missing signing secret | GHA release workflow | Fails loudly, no release created |
| 7 | Fresh macOS arm64, no worklog | `curl -fsSL …/install.sh \| bash` | Downloads target-correct binary, verifies signature against embedded pubkey, installs to `~/.local/bin/worklog`, prints version |
| 8 | Installer gets a tampered binary | verification step | Refuses to install, exits non-zero |
| 9 | `uv tool install worklog` already present | `install.sh` runs | Detects, warns, instructs `uv tool uninstall worklog`, exits non-zero unless `--force` |
| 10 | Rust binary installed via new installer | `worklog upgrade` | Calls `self-update`, pulls latest signed release |
| 11 | Any `worklog <cmd>` | invocation | No Python interpreter in the chain |

## Out of Scope (follow-ups)

- Homebrew tap.
- Intel mac / linux-arm / windows.
- Shell completions (clap has `clap_complete`; separate PR).
- Docker image of the CLI (web container stays).

## Files

**New**
- `rust/crates/worklog-core/src/collectors/gcal.rs` — OAuth2 + Calendar v3 REST
- `rust/crates/worklog-core/src/collectors/gcal/oauth.rs` — token refresh, file cache
- `rust/crates/worklog-cli/src/day.rs` — `worklog day` orchestrator
- `.github/workflows/release.yml` — build-sign-upload on tag push
- `install.sh` — curl-piped installer at repo root
- `docs/MIGRATION.md` — uv → curl, one page
- Tests: `gcal_oauth.rs`, `gcal_collector.rs`, `day_cmd.rs`, `tests/release/smoke.sh`, `tests/install/*.bats`

**Modified**
- `rust/crates/worklog-cli/src/cli.rs` — add `Cmd::Day`
- `rust/crates/worklog-core/src/updater/pubkey.rs` — real pubkey (replace zeros)
- `rust/crates/worklog-core/src/lib.rs` — export `gcal`
- `README.md` — install section rewrite
- `CLAUDE.md` — drop Python invariants
- `rust/README.md` — brief

**Deleted**
- `src/worklog/` (entire tree)
- `tests/` (all Python tests)
- `pyproject.toml`, `uv.lock`, `.python-version`
- `setup_gcal.py` (verify existence)

## TDD Phases

### Phase 1 — Gcal collector (RISKIEST, FIRST)

**RED.** Write failing tests in `rust/crates/worklog-core/tests/`:
- `gcal_oauth::refresh_token()` loads `credentials.json` + `token.json`, refreshes when `expires_in < 60s`, writes updated token back.
- `gcal::collect(since, until)` paginates `calendars/<id>/events?timeMin=…&timeMax=…`, yields events normalised to UTC, upserts as `(source='gcal', source_id=<iCalUID>)`.
- Re-collecting same iCalUID → idempotent (single row).
- `status: "cancelled"` events skipped.
- All-day events (no `dateTime`) — fixture + assertion pinned to chosen behavior.
- Use `httpmock` for API, fixtures under `tests/fixtures/gcal/`.

Gate: `cargo test -p worklog-core gcal` exits NON-ZERO.

**GREEN.** Implement `gcal.rs` + `gcal/oauth.rs` using `reqwest` (already in deps), `oauth2` crate, `url`. Reuse `repo::upsert_event`. Mirror `src/worklog/collectors/gcal.py::_to_utc` for TZ.

Gate: `cargo test gcal` passes. `cargo clippy -- -D warnings`.

**REFACTOR.** Apply `clean-code`. Extract OAuth from collector. Match other collectors' `fn collect(since, until, settings) -> Result<CollectReport>` shape. Wire into CLI dispatch.

### Phase 2 — `worklog day` in Rust

**RED.** Test `Cmd::Day` dispatches the pipeline:
- `day::run(day, model)` calls collect_all → infer → estimate → web_up in order.
- Each step's failure is captured + reported but doesn't abort the next (matches Python `[yellow]!` behavior).
- `--no-serve` skips `web up`.
- Output shape close enough to Python that muscle memory transfers.

Gate: `cargo test day` NON-ZERO.

**GREEN.** Implement `day.rs`. Reuse existing `cmd_collect`/`cmd_infer`/`cmd_estimate`/`cmd_web_up` internals; extract helpers if needed. `owo-colors` for ticks.

Gate: `cargo test` passes.

**REFACTOR.** Apply `clean-code`. Share "capture + report + continue" helper. Audit `cli.rs` for port-dead-code.

### Phase 3 — GHA release + real signing key

**RED.** Smoke harness `tests/release/smoke.sh`:
- Run `scripts/release.sh` (local version of CI assemble) against a dummy tag.
- Assert three files per platform, manifest shape (version = tag, `result_sha256` set, targets listed).
- Assert missing `WORKLOG_RELEASE_PRIVATE_KEY` fails cleanly.

Gate: `bash tests/release/smoke.sh` NON-ZERO.

**GREEN.**
- Generate keypair: `worklog dev keygen --out /tmp/wl-release-keys`.
- Paste pubkey const into `updater/pubkey.rs`.
- Print `gh secret set` command for the user to run (private key stays on their machine until they paste it).
- Write `.github/workflows/release.yml`:
  - Trigger: tag `v*`.
  - Matrix: `aarch64-apple-darwin` (runs-on `macos-14`), `x86_64-unknown-linux-gnu` (runs-on `ubuntu-24.04`).
  - `cargo build --release --target <target>`.
  - `worklog dev sign` binary + manifest.
  - `gh release create` with artifacts attached.

Gate: smoke tests pass. Pre-release tag `v0.3.0-rc.1` builds green in CI.

**REFACTOR.** Composite action for repeated steps. Add `dry-run` input that builds+signs but skips `gh release create`.

### Phase 4 — `install.sh` curl installer

**RED.** `bats` tests in `tests/install/`:
- Fresh install downloads correct arm64/x86_64 binary, verifies, installs to `~/.local/bin/worklog`, +x.
- Tampered binary → exits 1, no binary installed.
- Missing manifest signature → fails.
- `uv tool install worklog` detected → warns, instructs uninstall, exits non-zero without `--force`.
- Idempotent (double-run doesn't double-install).

Gate: `bats tests/install/` NON-ZERO.

**GREEN.** Write `install.sh` (~150 lines):
- Detect target (`uname -sm` → `aarch64-apple-darwin` | `x86_64-unknown-linux-gnu`).
- `curl -fsSL` release asset + manifest + sig.
- Add a `worklog self-update --verify-only <path>` subcommand in Phase 3 if missing; installer shells out to it using the downloaded binary to self-validate against its embedded pubkey.
- Write to `~/.local/bin/worklog`, chmod +x.
- Warn if `$HOME/.local/bin` not in PATH.

Gate: tests pass; manual test on a clean VM (Runtime Verification).

**REFACTOR.** Apply `clean-code` (shell). Consolidate error messages, consistent exit codes.

### Phase 5 — Delete Python

**RED.** `tests/integration/no_python.sh` (runs after installing the Rust binary):
- During each command, `ps -ef | grep -E 'python|uv '` is empty.
- `find . -name '*.py' -not -path './target/*'` returns empty.
- `cargo test --manifest-path rust/Cargo.toml` passes.

Gate: test fails (Python files still exist).

**GREEN.**
- `git rm -r src/worklog/ tests/`
- `git rm pyproject.toml uv.lock .python-version setup_gcal.py` (verify each)
- `.gitignore` — remove `__pycache__`, `.pytest_cache`, `venv/`, `.venv/`.
- Verify nothing in Rust imports from Python (`grep -r "python\|pyproject" rust/`).

Gate: `cargo test` passes. `cd web && bun test && bun run typecheck && bun run build` passes. `find . -name '*.py'` empty (except `target/`).

**REFACTOR.** Scan for dangling `_exec_rust`, `_install_rust_binary`, `uv tool install` references in README/scripts/docs.

### Phase 6 — Docs + CLAUDE.md rewrite

**RED.** N/A — docs-only. Validation: `markdownlint` + manually running every command in the new README.

**GREEN.**
- `README.md` install section:
  ```
  curl -fsSL https://raw.githubusercontent.com/TomasPalsson/worklog/main/install.sh | bash
  ```
- CLAUDE.md first paragraph:
  > Pure-Rust CLI (`rust/crates/{worklog-core,worklog-cli}/`) + a
  > dockerised Next.js + Bun review UI in `web/`. The Rust daemon
  > listens on `127.0.0.1:9323` (TCP) and a unix socket for the web UI.
- Replace `uv run` commands with `cargo test`, `cargo clippy`, `cargo fmt`.
- Delete "Adding a collector (Python)" section.
- Add `docs/MIGRATION.md`: one page for `uv tool uninstall` → curl installer.

Gate: all docs examples run.

**REFACTOR.** Ensure CLAUDE.md invariants (UTC timestamps, `$WORKLOG_TZ`, manual-estimate skip) still coherent. Delete anything that only made sense when Python existed (schema.sql byte-parity rule — schema.sql now lives only in Rust).

## Runtime Verification

1. Clean macOS arm64 (or docker): curl-install.
2. `worklog --version` → version string.
3. `WORKLOG_HOME=/tmp/wl-verify worklog setup --non-interactive`.
4. `worklog collect gcal --since $(date -v-1d +%F) --until $(date +%F)` → events in DB.
5. `worklog day` → full pipeline, web UI opens, no python/uv in `ps`.
6. `worklog upgrade` → "already up to date."
7. Tamper with binary on disk → `worklog self-update --verify-only` refuses.
8. `find ~/.worklog -name '*.py'` empty.

## User Verification (HARD GATE — I ask, you approve)

1. `worklog day` on a real day — output feels same or better than Python.
2. `worklog collect gcal` — events appear in the UI with correct ticket assignments.
3. `worklog upgrade` — reports success.
4. Tail `worklog daemon` logs during a sync — no python subprocess.
5. Read new README install section — you'd follow it yourself on a fresh machine.

## Quality Gates (non-skippable)

**Inline.** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cd web && bun test && bun run typecheck && bun run build`.

**Agent wave (4 × sonnet, parallel).**
- Security — OAuth token handling, installer curl pipe, signing-key handling.
- Performance — Gcal pagination, day orchestrator, release assemble step.
- Type-safety — Result propagation in gcal, error-type hierarchy.
- Accessibility — n/a (no UI change); skip with documented reason.

## Plan Validation

- [x] Every phase has Red/Green/Refactor
- [x] No phase has >8 T-IDs
- [x] Riskiest phase (Gcal) first
- [x] Behavior inventory: 11 rows, happy + error paths
- [x] Runtime + user verification are specific
- [x] Browser verification skipped with reason (no UI change; web UI stays as-is)
- [x] Quality gates included
- [x] Scope bounded; follow-ups listed
