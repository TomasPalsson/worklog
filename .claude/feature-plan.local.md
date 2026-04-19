# Feature: v0.4 — daemon auto-install, CLI polish, rolling purge, richer hook capture, web dark mode

## Description

Bundle of five improvements that collectively take worklog from "works end-to-end" to "set it and forget it":

1. **Hook prompt capture** — current Claude Code hook stores only the first 80 chars of the user prompt in `event.title`; the estimator reads `event.details` (which is the transcript path). Store the full prompt (capped 4KB) in `details` so `estimate.rs` has substance to summarise from even on pure-Claude days.
2. **Data purge** — `worklog db purge [--days N] [--dry-run]` that deletes events + blocks older than N days (default 30) *only when* the block is safe to delete (`tempo_worklog_id` present OR `estimated_by = 'gap'`). Never touches `estimated_by='manual'` blocks or unsynced work. Runs opportunistically at the top of `worklog day`.
3. **Daemon auto-install** — `worklog daemon install/uninstall/status` writes a launchd plist / systemd user unit with KeepAlive, so the daemon runs at login and restarts on crash. `worklog day` and `worklog web up` probe `/healthz` and auto-install if missing. Wizard prompts on first `worklog setup`.
4. **CLI polish** — clap `Styles` + grouped `help_heading` sections, `miette` at the binary boundary for boxed error messages, `comfy-table` for status/doctor/day summaries, an inline banner on bare `worklog` invocation.
5. **Web dark mode** — extend `globals.css` OKLCH tokens with a dark counterpart (4-tier lightness system, desaturated accents). Defaults to `prefers-color-scheme`, overridable via a ThemeToggle in the DayHeader. Persisted in a cookie for SSR-safe no-flash rendering.

Version bump: `0.3.0 → 0.4.0` (feat:).

## Assumptions (baked into plan, correct at approval if wrong)

- Purge cutoff is **30 days rolling** from today, not strict "20-19 cycle". Rationale: user said "month old data" first; the 20-19 framing is justification that 30-day data is already synced.
- Purge safety: delete blocks where `tempo_worklog_id IS NOT NULL OR estimated_by = 'gap'` AND their events. Keep unsynced + manual-edited blocks regardless of age.
- Prompt cap: **4 KiB per prompt body** stored in `event.details`. Anything larger gets truncated with `"…<truncated N chars>"` suffix.
- Daemon label: `is.p5.worklog.daemon` (parallel to `is.p5.worklog.collect`).
- Daemon health check: HTTP GET `http://127.0.0.1:9323/healthz` with 500ms timeout. No healthz endpoint exists yet — adding a trivial one.
- `--json` output paths NEVER get ANSI/tables. Keep machine-readable surfaces clean.
- **Dark mode default**: follow system (`prefers-color-scheme`) with an explicit toggle in DayHeader that overrides. Persist via `theme` cookie so SSR can emit correct `data-theme="…"` on `<html>` and avoid a flash-of-wrong-theme.
- **Dark mode palette**: 4-tier lightness system per the design rule — `oklch(0.12/0.16/0.20/0.24 0.005 85)` for bg/raised/sunk/border. Accents desaturated 10-15% (sage/amber/terracotta/violet lightness ~0.62, chroma ~0.08).

## Behavior Inventory

| ID | Given | When | Then | Priority |
|----|-------|------|------|----------|
| B1 | Claude hook fires with a 200-char user prompt | hook_run persists the event | event.details contains the full 200-char prompt verbatim | P0 |
| B2 | Claude hook fires with a 10 KiB user prompt | hook_run persists the event | event.details contains first 4096 bytes + `"…<truncated 5904 chars>"` suffix | P0 |
| B3 | Claude hook fires with no prompt (SessionStart, Stop) | hook_run persists the event | event.details falls back to transcript path (pre-existing behaviour) | P0 |
| B4 | estimate.rs::build_user_message processes a claude event | it formats the payload | the event's `details` field is truncated at 800 chars (was 200) so Claude sees real prompts | P0 |
| B5 | Block is >30d old AND has tempo_worklog_id | purge runs with default cutoff | block + joined events are deleted | P0 |
| B6 | Block is >30d old AND estimated_by='gap' | purge runs with default cutoff | block + joined events are deleted | P0 |
| B7 | Block is >30d old AND tempo_worklog_id IS NULL AND estimated_by IS NULL | purge runs | block is preserved (unsynced work) | P0 |
| B8 | Block is >30d old AND estimated_by='manual' | purge runs | block is preserved regardless of sync state | P0 |
| B9 | Block is exactly 29d old AND synced | purge runs with default cutoff | block is preserved (not yet 30d) | P1 |
| B10 | User runs `worklog db purge --dry-run` | the command executes | nothing is deleted; count of what would be deleted is printed | P0 |
| B11 | `worklog daemon install` invoked on macOS | plist is written | `~/Library/LaunchAgents/is.p5.worklog.daemon.plist` exists, KeepAlive=true, RunAtLoad=true, ProgramArguments points at resolved worklog binary + `daemon` | P0 |
| B12 | `worklog daemon install` invoked on linux | systemd unit is written | `~/.config/systemd/user/worklog-daemon.service` exists, `Restart=on-failure`, `WantedBy=default.target` | P0 |
| B13 | `worklog daemon install` invoked twice | second install | is idempotent — overwrites atomically, no orphan | P0 |
| B14 | `worklog daemon uninstall` invoked | unit file | is removed, status reports `installed=false` | P0 |
| B15 | `worklog daemon status` invoked | daemon plist exists + process running | status reports `installed=true`, `running=true` | P1 |
| B16 | `worklog day` invoked and daemon not running | day runs | auto-install triggered (prints "installing daemon…"), waits for /healthz, proceeds | P0 |
| B17 | `worklog day` invoked and daemon is running | day runs | no re-install, just proceeds | P0 |
| B18 | `worklog setup` wizard runs (first time) | reaches install-daemon step | prompts "Install the worklog daemon to start at login? [Y/n]" | P1 |
| B19 | Bare `worklog` (no subcommand) runs | clap shows help | banner string appears, commands grouped under section headers ("Setup", "Data collection", "Review & sync", "Daemon", "Release ops") | P0 |
| B20 | Hard error in any command | binary exits non-zero | error is boxed via miette (colored, indented, hint if available); stdout stays clean | P0 |
| B21 | `worklog doctor` invoked | diagnostics print | status is rendered as a comfy-table (not line-by-line); each row shows marker + check + status + note | P1 |
| B22 | `worklog --json <anything>` invoked | JSON output path active | no ANSI codes, no tables, no banner — pure JSON on stdout | P0 |
| B23 | `cargo test` on CI | runs all tests | all tests pass including platform-gated daemon install on macOS runner + linux runner | P0 |
| B24 | Web UI loads, no `theme` cookie, system set to dark | first render | `<html>` has `data-theme="dark"`, no flash, body bg is OKLCH ~0.12 (paper-dark, not pure black) | P0 |
| B25 | Web UI loads, `theme=light` cookie, system set to dark | first render | `<html>` has `data-theme="light"` (cookie wins over system) | P0 |
| B26 | User clicks ThemeToggle (sun→moon) | click fires | `<html>` gets `data-theme="dark"`, cookie is written, all state colors (sage/amber/terracotta/violet) remain legible (contrast ≥ 4.5:1 for text, ≥ 3:1 for UI) | P0 |
| B27 | Web UI is hydrated in dark mode | focus lands on a button | focus ring (`--ring`) is visible against the dark bg (≥ 3:1 contrast) | P0 |
| B28 | `viewport.themeColor` metadata is read by the browser on theme switch | switch fires | `<meta name="theme-color">` is updated to match current theme's bg | P1 |
| B29 | `bun test` runs in web/ | all tests pass | ThemeToggle component test covers cycle light→dark→system, cookie writes verified | P0 |

## Phases

### Phase 1: hook prompt capture — RED

**Mandate**: Write failing tests ONLY.

#### Tasks
| ID | Task | Files | Depends On |
|----|------|-------|------------|
| T1.1 | Add tests for B1, B2, B3 in hook_run.rs | rust/crates/worklog-core/src/hook_run.rs | — |
| T1.2 | Add test for B4 in estimate.rs (details truncation bumped to 800 for claude source) | rust/crates/worklog-core/src/estimate.rs | — |

**Red Gate**: `cargo test --manifest-path rust/Cargo.toml` — MUST exit non-zero with assertions failing on `event.details` content.

**Commit**: `test(hook,estimate): add failing tests for full prompt capture [RED]`

### Phase 1: hook prompt capture — GREEN

**Mandate**: Make B1-B4 pass. No new tests.

#### Tasks
| ID | Task | Files | Depends On |
|----|------|-------|------------|
| T1.3 | Add `cap_prompt(s: &str, max: usize) -> String` to hook_run.rs (4096 byte cap + `"…<truncated N chars>"` suffix) | hook_run.rs | T1.1 |
| T1.4 | Rewrite `handle()`: when prompt is Some, set `details = Some(cap_prompt(prompt, 4096))`; when None, keep existing transcript path | hook_run.rs | T1.3 |
| T1.5 | In `build_user_message`, bump event details trunc from 200 to 800 for `source='claude'` (gated) | estimate.rs | T1.2 |

**Green Gate**: `cargo test --manifest-path rust/Cargo.toml` passes. `git diff --name-only` shows only `hook_run.rs` + `estimate.rs` (no test file changes).

**Commit**: `feat(hook,estimate): capture full user prompt (capped 4KiB) for richer block descriptions`

### Phase 1: hook prompt capture — REFACTOR

- Extract 800-char truncation constant named by source (`const CLAUDE_DETAILS_TRUNC: usize = 800;`).
- Verify `cap_prompt` handles multi-byte UTF-8 boundaries (char-based cap, not byte cap).
- Re-run tests after each tweak.

**Commit**: `refactor(hook): name the prompt-cap constants, char-safe truncation`

---

### Phase 2: data purge — RED

**Mandate**: Write failing tests ONLY.

#### Tasks
| ID | Task | Files | Depends On |
|----|------|-------|------------|
| T2.1 | Create `rust/crates/worklog-core/src/purge.rs` with stub `purge(conn, cutoff_days: i64, dry_run: bool) -> Result<PurgeReport>` returning `anyhow::bail!("not implemented")` | purge.rs (new) | — |
| T2.2 | Declare `pub mod purge;` in `lib.rs` | lib.rs | T2.1 |
| T2.3 | Tests for B5-B10 in `purge.rs` with `db::open_memory()` fixtures | purge.rs | T2.1 |

**Red Gate**: `cargo test -p worklog-core purge` — MUST exit non-zero.

**Commit**: `test(purge): add failing tests for retention policy [RED]`

### Phase 2: data purge — GREEN

#### Tasks
| ID | Task | Files | Depends On | Parallelizable |
|----|------|-------|------------|----------------|
| T2.4 | Implement `purge()` — delete in transaction: rows from `block_events` for matching blocks, then `blocks`, then orphan `events` older than cutoff that aren't referenced by any surviving block. Use SQL filters: `WHERE day < date('now', '-30 days') AND (tempo_worklog_id IS NOT NULL OR estimated_by = 'gap') AND (estimated_by IS NULL OR estimated_by != 'manual')` | purge.rs | T2.3 | No |
| T2.5 | Add `Cmd::Db` already exists — extend `DbCmd::Purge { days: u32, dry_run: bool }` variant | cli.rs | — | Yes |
| T2.6 | `fn cmd_db_purge(...)` calling `purge::purge()`, rendering a comfy-table summary (deferred; use plain output for Phase 2, replace in Phase 5) | cli.rs | T2.4, T2.5 | No |

**Green Gate**: `cargo test --manifest-path rust/Cargo.toml` passes. Test B10 verifies `dry_run=true` returns a non-zero `would_delete_blocks` count but `SELECT COUNT(*) FROM blocks` is unchanged.

**Commit**: `feat(purge): add 30d retention for synced blocks + orphan events`

### Phase 2: data purge — REFACTOR

- Extract SQL predicate into named fn `purge_candidate_predicate() -> &'static str` for single source of truth.
- `PurgeReport` derives `serde::Serialize` for `--json`.

**Commit**: `refactor(purge): name the predicate, derive serde for reporting`

---

### Phase 3: daemon install module — RED

**Mandate**: Write failing tests ONLY.

#### Tasks
| ID | Task | Files | Depends On |
|----|------|-------|------------|
| T3.1 | Create `rust/crates/worklog-core/src/daemon_service.rs` stub: `install() -> Result<DaemonStatus>`, `uninstall()`, `status()`, all `bail!("not implemented")` | daemon_service.rs (new) | — |
| T3.2 | Declare `pub mod daemon_service;` in `lib.rs` | lib.rs | T3.1 |
| T3.3 | Platform-gated tests for B11 (macos), B12 (linux), B13, B14 using `tempdir()` + `WORKLOG_SCHEDULE_HOME` override (reuse existing env var) | daemon_service.rs | T3.1 |

**Red Gate**: `cargo test -p worklog-core daemon_service` — MUST exit non-zero.

**Commit**: `test(daemon-service): add failing tests for install/uninstall/status [RED]`

### Phase 3: daemon install module — GREEN

#### Tasks
| ID | Task | Files | Depends On |
|----|------|-------|------------|
| T3.4 | Port `schedule::atomic_write` + `Platform::current()` style into `daemon_service.rs` (copy, don't share — the interval logic is the schedule-specific bit) | daemon_service.rs | T3.3 |
| T3.5 | macOS: generate plist with `KeepAlive=true`, `RunAtLoad=true`, `ProgramArguments` = `[worklog_path, "daemon"]`, `StandardErrorPath` / `StandardOutPath` to `~/.worklog/logs/daemon.{out,err}.log` | daemon_service.rs | T3.4 |
| T3.6 | linux: generate systemd unit with `ExecStart=<worklog> daemon`, `Restart=on-failure`, `RestartSec=5`, `WantedBy=default.target` (no timer — daemon runs continuously) | daemon_service.rs | T3.4 |
| T3.7 | `status()`: parse plist/unit back, check process by label (launchctl list / systemctl status — but gate behind `WORKLOG_SCHEDULE_HOME` check for tests) | daemon_service.rs | T3.4 |
| T3.8 | CLI: add `DaemonServiceCmd::{Install, Uninstall, Status}` — but naming — rename existing `Cmd::Daemon` → `Cmd::Daemon { #[subcommand] cmd: DaemonCmd }` with `DaemonCmd::{Run, Install, Uninstall, Status}` where `Run` is the current foreground behaviour. Migration: `worklog daemon` keeps working (matches `DaemonCmd::Run` as default) | cli.rs | T3.4 |

**Green Gate**: All B11-B15 tests pass. `cargo test --manifest-path rust/Cargo.toml` overall still passes.

**Commit**: `feat(daemon): add install/uninstall/status service commands (launchd + systemd)`

### Phase 3: daemon install module — REFACTOR

- Extract plist/unit templates into `const` strings with `{command}` / `{label}` placeholders, run through `str::replace`. Mirrors `schedule.rs`.
- Share `ENV_SCHEDULE_HOME` constant with schedule module (re-export from schedule.rs so tests of both modules redirect via the same env var).

**Commit**: `refactor(daemon): share env override with schedule, template the unit files`

---

### Phase 4: daemon auto-start + wizard — RED

**Mandate**: Write failing tests ONLY.

#### Tasks
| ID | Task | Files | Depends On |
|----|------|-------|------------|
| T4.1 | Add `/healthz` endpoint in `daemon.rs` router returning `{"ok": true, "version": "..."}` + unit test | daemon.rs | — |
| T4.2 | Create `rust/crates/worklog-core/src/daemon/health.rs` (or inline helper) — `fn is_healthy(timeout: Duration) -> bool` that GETs `http://127.0.0.1:9323/healthz`. Unit test using `httpmock` | daemon.rs or new file | T4.1 |
| T4.3 | Wizard test: mock stdin with "Y\n", assert `install()` is called once; mock "n\n", assert not called | wizard.rs | — |
| T4.4 | Tests for B16, B17 using a fake `DaemonProbe` trait injected into `cmd_day` | cli.rs | T4.2 |

**Red Gate**: `cargo test` — must exit non-zero on the new tests.

**Commit**: `test(daemon,day,wizard): add failing tests for auto-start + prompt [RED]`

### Phase 4: daemon auto-start + wizard — GREEN

#### Tasks
| ID | Task | Files | Depends On |
|----|------|-------|------------|
| T4.5 | Add `/healthz` route to `daemon::router` returning JSON ok+version | daemon.rs | T4.1 |
| T4.6 | Implement `daemon::is_healthy()` (500ms blocking reqwest GET, returns false on any error) | daemon.rs or health.rs | T4.2 |
| T4.7 | In `cmd_day` + `cmd_web_up`, call `is_healthy()`; if false, call `daemon_service::install()` and poll `/healthz` up to 5s before proceeding | cli.rs | T4.6 |
| T4.8 | In `wizard.rs`, add "Install the worklog daemon to start at login?" Y/n step AFTER Claude hook setup. Default Y. On Y → `daemon_service::install()`. | wizard.rs | — |

**Green Gate**: All B16-B18 tests pass. Manual smoke: `worklog day` with daemon down triggers install + proceeds.

**Commit**: `feat(day,web,setup): auto-install daemon on first use + wizard prompt`

### Phase 4: daemon auto-start + wizard — REFACTOR

- Extract the "ensure healthy" helper into `daemon_service::ensure_running()` for reuse by day + web.
- Progress UI during install: use existing `style::spinner` with "installing background service…" message.

**Commit**: `refactor(daemon): consolidate ensure_running helper`

---

### Phase 5: CLI polish — RED

**Mandate**: Write failing tests ONLY.

#### Tasks
| ID | Task | Files | Depends On |
|----|------|-------|------------|
| T5.1 | Tests for B19: `assert_cmd::Command::cargo_bin("worklog").output()` — assert that `--help` output contains section header "Data collection" (literal string test in `cli.rs` integration tests) | tests/cli.rs | — |
| T5.2 | Tests for B20: simulate a hard error (invalid subcommand arg) and assert stderr contains `"Error: "` + indented multi-line format characteristic of miette | tests/cli.rs | — |
| T5.3 | Tests for B21: `worklog doctor` output contains Unicode box-drawing chars (`━` or `┃`) — but only when stdout is a TTY. Integration test checks pipe-mode falls back to plain output. | tests/cli.rs | — |
| T5.4 | Tests for B22: `worklog --json doctor` stdout is parseable JSON, contains NO `\x1b[` ANSI sequences, NO Unicode box characters | tests/cli.rs | — |

**Red Gate**: `cargo test` — MUST exit non-zero on B19-B22.

**Commit**: `test(cli): add failing tests for grouped help + miette errors + tables + json purity [RED]`

### Phase 5: CLI polish — GREEN

#### Tasks
| ID | Task | Files | Depends On | Parallelizable |
|----|------|-------|------------|----------------|
| T5.5 | Add to `rust/Cargo.toml` workspace deps: `miette = { version = "7", features = ["fancy"] }`, `comfy-table = "7"` | rust/Cargo.toml | — | Yes |
| T5.6 | `style.rs`: add `banner()` (multi-line raw string), `table()` returning `comfy_table::Table` with preset applied, `is_tty()` helper | style.rs | T5.5 | Yes |
| T5.7 | `cli.rs`: clap `#[command(styles = Styles::styled()...)]` + `#[command(help_heading = "…")]` per-subcommand. Groups: "Setup & diagnostics" (setup, doctor, hook, secret, db), "Data collection" (collect, infer, estimate), "Review & sync" (day, web, sync), "Daemon" (daemon, serve, schedule), "Release ops" (self-update, upgrade, dev) | cli.rs | — | Yes |
| T5.8 | `cli.rs`: on bare `worklog` (no args), print banner then clap help | cli.rs | T5.6 | No (depends T5.6, T5.7) |
| T5.9 | `main.rs`: wrap call into `miette::Result<()>`, use `.into_diagnostic()` at the outer anyhow boundary. Keep internal APIs on anyhow. | main.rs | T5.5 | No |
| T5.10 | Migrate `cmd_doctor`, `cmd_schedule_status`, `cmd_daemon_status`, `cmd_db_purge` summary output to `comfy_table` — gated on `is_tty() && !json` | cli.rs | T5.6 | No |

**Execution waves**:
- Wave 1: T5.5 + T5.7 (deps + clap attrs, no file conflicts)
- Wave 2: T5.6 (style.rs extension)
- Wave 3: T5.8 + T5.9 + T5.10 (depend on prior waves)

**Green Gate**: All B19-B22 tests pass. `cargo test --manifest-path rust/Cargo.toml` passes.

**Commit**: `feat(cli): polish help (grouped sections, color), errors (miette), tables (comfy), banner`

### Phase 5: CLI polish — REFACTOR

- Review whether every table call respects `--json` (grep for `comfy_table::Table::new`).
- Make sure banner art is ≤6 lines and uses stable Unicode chars (no zero-width-joiners).
- Final pass with `clean-code` skill: naming, duplication.

**Commit**: `refactor(cli): tighten polish layer — final pass`

---

### Phase 6: web dark mode — RED

**Mandate**: Write failing tests ONLY.

#### Tasks
| ID | Task | Files | Depends On |
|----|------|-------|------------|
| T6.1 | Add `web/components/ThemeToggle.tsx` stub exporting a component that throws `throw new Error("not implemented")` | web/components/ThemeToggle.tsx (new) | — |
| T6.2 | Add `web/lib/theme.ts` stub exporting `readThemeCookie(): "light" \| "dark" \| null` + `applyThemeAttr(value)` + `writeThemeCookie(value)` — all throwing | web/lib/theme.ts (new) | — |
| T6.3 | Write `web/components/ThemeToggle.test.tsx` using bun:test + happy-dom. Covers B26 (click cycles light→dark→system), B29 (cookie write) | web/components/ThemeToggle.test.tsx (new) | T6.1, T6.2 |
| T6.4 | Write `web/lib/theme.test.ts` covering B24 (no cookie, system=dark → dark), B25 (cookie=light, system=dark → light) | web/lib/theme.test.ts (new) | T6.2 |

**Red Gate**: `bun test` inside web/ — MUST exit non-zero.

**Commit**: `test(web): add failing tests for dark mode toggle + cookie resolution [RED]`

### Phase 6: web dark mode — GREEN

#### Tasks
| ID | Task | Files | Depends On | Parallelizable |
|----|------|-------|------------|----------------|
| T6.5 | Extend `web/app/globals.css` with `:root[data-theme="dark"] { … }` block redefining every token. 4-tier lightness system for surfaces: bg=0.12, bg-raised=0.16, bg-sunk=0.20, border=0.24 (all at chroma 0.005 hue 85). Accents desaturated: sage=oklch(0.62 0.08 145), amber=oklch(0.68 0.10 70), terracotta=oklch(0.60 0.14 25), violet=oklch(0.62 0.07 290). Fg=oklch(0.92 0.01 260), fg-muted=oklch(0.70 0.015 260). Remove the elevation-1 shadow in dark (lightness IS the elevation). Ring becomes oklch(0.42 0.03 85). | web/app/globals.css | — | Yes |
| T6.6 | Also add `@media (prefers-color-scheme: dark)` fallback block replicating the same tokens so users with no cookie but system-dark get dark without JS | web/app/globals.css | T6.5 | Yes |
| T6.7 | Implement `lib/theme.ts`: `readThemeCookie()` parses `document.cookie`; `writeThemeCookie(value)` sets `theme=<val>; path=/; max-age=31536000; SameSite=Lax`; `applyThemeAttr()` sets `document.documentElement.dataset.theme` + updates `<meta name="theme-color">`. Accepts `"light" \| "dark" \| null` (null = clear cookie, defer to system) | web/lib/theme.ts | — | Yes |
| T6.8 | Implement `ThemeToggle.tsx` — `"use client"`, three-state cycle (light → dark → system), sun/moon/monitor icons from lucide-react, aria-pressed + descriptive aria-label. Initial state pulled from cookie via `readThemeCookie()`; on mount, syncs to current `document.documentElement.dataset.theme` | web/components/ThemeToggle.tsx | T6.7 | No |
| T6.9 | `app/layout.tsx`: read the `theme` cookie server-side via `next/headers::cookies()`, set `<html data-theme={...}>` accordingly. Inject a small inline `<script>` just inside `<head>` that, if no cookie, mirrors `prefers-color-scheme` onto `data-theme` before paint — kills the flash-of-wrong-theme | web/app/layout.tsx | T6.7 | No |
| T6.10 | `components/DayHeader.tsx`: add `<ThemeToggle />` to the right edge | web/components/DayHeader.tsx | T6.8 | No |
| T6.11 | `app/layout.tsx`: update `viewport.themeColor` to an array of `{media, color}` entries so the browser picks the right one per system setting | web/app/layout.tsx | T6.5 | Yes |

**Execution waves**:
- Wave 1: T6.5, T6.6, T6.7, T6.11 (no file conflicts)
- Wave 2: T6.8, T6.9 (depend on theme.ts)
- Wave 3: T6.10 (depends on ThemeToggle)

**Green Gate**: `bun test` passes. `bun run typecheck` clean. `bun run build` succeeds.

**Commit**: `feat(web): add dark mode (system default + manual toggle, SSR no-flash)`

### Phase 6: web dark mode — REFACTOR

- Sanity-check every state color for AA contrast against the new dark bg (sage-ink / amber-ink / terracotta etc. — some may need a lightness bump in dark mode specifically)
- Ensure the inline no-flash script is CSP-safe (add `nonce` if the CSP ever tightens; for now, Next.js 15 inline-script behaviour is fine in dev)
- `data-theme` attribute on `<html>` (not `<body>`) so inherited CSS vars propagate cleanly to `<dialog>`/`<portal>` escape hatches

**Commit**: `refactor(web): contrast pass + CSP-safe no-flash script`

---

### Phase 7: Inline Gates (MANDATORY)

```bash
cargo fmt --manifest-path rust/Cargo.toml --all
cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings
cargo test  --manifest-path rust/Cargo.toml

cd web && bun test && bun run typecheck && bun run build

bash scripts/release-smoke.sh
bash tests/install/smoke.sh
```

All must pass. Fix inline.

### Phase 8: Quality Pipeline (MANDATORY)

Launch in parallel:

| Agent | Model | Artifact |
|-------|-------|----------|
| Security Scanner | sonnet | `.claude/quality/security.md` |
| Performance Analyzer | sonnet | `.claude/quality/performance.md` |
| Type-Safety Checker | sonnet | `.claude/quality/type-safety.md` |
| CLI/UX Reviewer | sonnet | `.claude/quality/ux.md` (replaces accessibility — this is a CLI, not a webpage; reviewer checks help discoverability, error-path clarity, TTY/pipe behaviour) |

Aggregate, auto-fix CRITICAL + high-confidence IMPORTANT (max 2 attempts per finding), document the rest.

### Phase 9: Runtime Verification (CLI + browser)

Execute `.claude/verification/runtime-results.md`:
1. `worklog --help` → banner present, sections grouped, colors visible in TTY
2. `worklog --help | cat` → no ANSI codes in output
3. `worklog day --help` → command-specific help renders
4. `worklog nonexistent-subcommand` → miette-style boxed error on stderr, stdout clean
5. `worklog db purge --dry-run` on a fresh DB → "would delete 0 blocks" table printout
6. Seed DB with 35-day-old synced + 35-day-old manual + 29-day-old synced blocks → `worklog db purge` → only the first is deleted
7. `worklog daemon status` (daemon not installed) → `installed=false`
8. `worklog daemon install` → plist/service file written; `launchctl list | grep worklog.daemon` (macOS) shows entry
9. `worklog day` with daemon uninstalled → auto-install triggered, day proceeds
10. `worklog daemon uninstall` → file removed, launchctl entry gone
11. Run a Claude Code hook with a 5 KiB prompt → `SELECT length(details) FROM events WHERE source='claude' ORDER BY id DESC LIMIT 1;` returns ~4.1 KiB (4096 + truncation marker)
12. `worklog --json doctor` → valid JSON, no ANSI, no Unicode box chars
13. `worklog web up` → opens http://localhost:3333. In Chrome DevTools → Rendering → "Emulate CSS prefers-color-scheme: dark" → page renders dark *without* a flash, bg is warm-dark (not pure black), text is legible
14. Click the ThemeToggle in DayHeader → cycle light → dark → system; verify `document.cookie` shows `theme=…`, `<html data-theme="…">` updates, all badge colors (sage/amber/terracotta/violet) stay legible in dark
15. Tab through the page in dark mode → every focusable element has a visible focus ring with ≥3:1 contrast against the dark bg

### Phase 10: User Verification (HARD GATE)

Present results + screenshots (recorded terminal sessions) from Phase 8. User must confirm:
1. Help output grouping makes sense for *their* daily workflow
2. Error messages are clearer than before (show one side-by-side)
3. `worklog day` feels seamless — no manual `worklog daemon` or `worklog serve` steps
4. `worklog db purge --dry-run` output is legible and accurate
5. Description quality after full-prompt capture — user runs `worklog day` on a day with Claude events and eyeballs descriptions

**STOP. Wait for explicit "approved"/"ship it".**

### Phase 11: QA Pass

Run `/qa` skill. Expect PASS or PASS-WITH-NOTES. Fix ship-blockers, rerun inline gates, rerun QA.

### Phase 12: PR Creation

- Bump `rust/Cargo.toml` workspace version `0.3.0 → 0.4.0`
- Push `feat/v0.4-daemon-polish-purge`
- Draft PR → "feat: daemon auto-install, CLI polish, rolling purge, richer hook capture (v0.4.0)"
- release-please PR on main will trigger on merge; tag push runs signed-release workflow

## Runtime Verification Steps (condensed checklist)

See Phase 8 above — all 12 steps must pass before Phase 9.

## User Verification Steps

1. `worklog --help` — do the sections/colors help you find commands faster?
2. Run `worklog nonexistent-cmd` — does the error look better than plain clap output?
3. Run `worklog daemon status` — is the output clear?
4. Run `worklog day` with daemon not installed — does it auto-install without prompts? Does it feel smooth?
5. Run `worklog day` on a day with actual Claude Code events — are block descriptions richer than before?
6. Run `worklog db purge --dry-run` — is the output accurate? Does it say what's keeping stuff (unsynced / manual)?
7. Open the web UI in light mode, click the theme toggle → dark — does the whole page feel coherent, not just inverted? Are the sage/amber badges still doing their job?
8. Reload the page while in dark — is there a flash of light before dark kicks in? (There shouldn't be.)
9. Set OS to dark mode with no cookie → open web UI → is it dark immediately?

## Done When

- All P0 behaviors in inventory (B1-B8, B10-B14, B16-B17, B19-B20, B22-B27, B29) are tested and passing
- All quality gates PASS (security, performance, type-safety, ux)
- Runtime verification PASS (all 12 steps)
- User approved explicitly
- QA swarm PASS (or PASS WITH NOTES, no ship-blockers)
- Workspace version bumped to 0.4.0
- PR merged, tag pushed, v0.4.0 release cut by CI

## Rollback Plan

- Starting commit: `73786ee` (current main HEAD)
- Revert sequence: `git revert --no-commit 73786ee..HEAD` on the feature branch and force-push
- Migrations to undo: **none** — no schema changes (the full-prompt capture reuses existing `details TEXT` column)
- Binary compatibility: v0.4.0 reads/writes the same DB schema as v0.3.0; downgrade is safe. The only breaking surface is anyone who scripted against `worklog daemon` expecting it to be the foreground command — we preserve that as `DaemonCmd::Run` default behaviour.
