# Feature Plan — Rust hook + Time Inference + `claude -p` Estimator

**Branch:** `feature/rust-hook-and-time-inference`
**Size:** Medium (3–5 days)
**Research:** 4-agent overkill swarm complete — findings baked into this plan.

## Why

Current hook is Python (~150–250ms cold start per Claude event — not OK on a blocking path). Durations are fake (15-min placeholder per commit/meeting). Time estimation is wrong. This PR replaces the hook with a sub-15ms Rust binary, adds real duration inference from event clustering, and uses `claude -p --json-schema` to turn each block into a Jira-worthy description + sanity-checked minutes.

## Scope

**In:**
1. Rust `worklog-hook` binary (replaces Python hook).
2. `sessions` table + SessionStart↔Stop/SessionEnd pairing → real durations for Claude activity.
3. `blocks` table + gap-timeout clustering algorithm (20-min threshold, calendar authoritative, +2min credit).
4. `worklog infer` CLI command (builds blocks from events).
5. `worklog estimate` CLI command (calls `claude -p --json-schema`, fills descriptions + sanity-check minutes).
6. Web UI + Tempo sync rewired to operate on `blocks`, not raw events.
7. `worklog hook install` prefers Rust binary if present, falls back to Python.
8. `worklog doctor` diagnostic command.
9. Inline SQL schema shared between Python and Rust (`src/worklog/schema.sql`, `include_str!` into Rust).

**Out (documented as follow-ups):**
- Homebrew tap / crates.io publishing.
- Anthropic SDK path (research recommended it for caching; `claude -p` is simpler and reuses existing Claude Code auth).
- Cross-platform CI; macOS arm64 only.
- Multi-device sync / cloud backup.
- GitHub remote + actual PR (project has no remote).

## Research Findings (abridged)

- **Hook schema**: `session_id`, `hook_event_name`, `cwd`, `transcript_path`, `permission_mode`, plus per-event fields (`source` on SessionStart, `prompt` on UserPromptSubmit). `SessionEnd` exists with `source: logout|prompt_input_exit|...`. `CLAUDE_PROJECT_DIR` env provided.
- **Perf budget**: <50ms for blocking events (UserPromptSubmit, Stop, SubagentStop). Rust target: <15ms wall-clock.
- **Rust stack**: `serde_json` + `rusqlite` (bundled) + `anyhow` + `serde-saphyr`. Profile: `opt-level=3, lto="thin", codegen-units=1, panic="abort", strip="symbols"`.
- **SQLite concurrency**: `journal_mode=WAL`, `synchronous=NORMAL`, `busy_timeout=150`. No retry on busy — log & exit 0.
- **Clustering**: gap-timeout method (Spiliopoulou 1999), τ=20min, calendar duration authoritative, +2min credit per point event, company mismatch forces split, `MIN_BLOCK=5min`, `MAX_BLOCK=4h` (flag if exceeded), round up to 15min for Tempo.
- **`claude -p`**: flags = `-p --model haiku --output-format json --json-schema '<schema>' --system-prompt '...'`. Input via stdin. Parse envelope: `json.loads(json.loads(stdout)["result"])`. Retry with explicit JSON-only nudge on parse failure (max 2 retries).

## Behavior Inventory (Given/When/Then)

| # | Given | When | Then |
|---|---|---|---|
| B1 | Rust hook receives SessionStart JSON on stdin | it parses and runs | event row inserted with `source=claude`, `started_at=now`, `duration_seconds=null` |
| B2 | Rust hook receives Stop for a prior open SessionStart | it pairs them | matching row gets `ended_at=now`, `duration_seconds=(end-start)` |
| B3 | Rust hook invoked with SessionStart older than 5min still unpaired | new event arrives | reaper sets `ended_at=started_at+5min` on the stale session before proceeding |
| B4 | Rust hook encounters malformed JSON, missing config, or SQLite busy >150ms | any failure | warning to stderr, exit 0 (never blocks Claude) |
| B5 | Rust hook exec time on M-series arm64 | steady-state invocation | <15ms wall-clock p50 (measured with hyperfine) |
| B6 | Events within 20min gap for same company | `worklog infer --day X` | collapsed into one block, `started_at=first_event.ts`, `ended_at=last_event.ts+credit` |
| B7 | Calendar event during coding block, same company | infer runs | block extends to cover calendar span (authoritative) |
| B8 | Acme event at 10:00, Side at 10:15, Acme at 10:30 | infer runs | 3 blocks, not 2 (company mismatch splits) |
| B9 | Block built from events | `worklog estimate --day X` | `claude -p` called with schema, block gets `description` + validated `minutes` (integer, rounded to 15) |
| B10 | `claude -p` returns malformed JSON twice | estimate runs | estimate falls back to gap-based minutes + concatenated event titles, logs warning |
| B11 | User runs `worklog sync --day X` | blocks exist with descriptions | one Tempo worklog per `(day, company, jira_issue)` using block duration, not 15min placeholder |
| B12 | `worklog hook install` runs on machine with `worklog-hook` in PATH | install | `~/.claude/settings.json` registers Rust binary path; falls back to `worklog hook run` if not found |
| B13 | `worklog doctor` runs | any state | prints status of: DB schema version, config file, Rust binary presence+version, Claude hook registration, `claude -p` availability |
| B14 | Python hook and Rust hook both write to the same SQLite concurrently | both invoked in quick succession | WAL mode + busy_timeout prevents `SQLITE_BUSY` errors, both rows land |
| B15 | Web UI opened after infer | /?day=X | shows blocks (not raw events) grouped by company with editable duration + reassignable company + description |

## TDD Phase Plan

**Phase rule**: each phase is Red → Green → Refactor (the refactor step MUST apply `clean-code` skill). Phase commits visibly show this ordering in `git log`.

### Phase A — Shared schema + sessions/blocks tables (Python)
- **T-A1 (Red)**: tests for `sessions` + `blocks` table creation, `events.session_id` column, schema migration from v1→v2 preserves data.
- **T-A2 (Green)**: extract schema to `src/worklog/schema.sql`, load via `importlib.resources`; write `scripts/migrate_v2.py`.
- **T-A3 (Refactor)**: `clean-code` pass on `db.py`.
- Covers: B1 (prereq), B14 (prereq).

### Phase B — Python hook writes session_id; reaper logic
- **T-B1 (Red)**: tests for SessionStart→Stop pairing (same session_id), reaper for stale sessions.
- **T-B2 (Green)**: implement in `collectors/claude.py`.
- **T-B3 (Refactor)**: `clean-code` pass.
- Covers: B2, B3.

### Phase C — Rust workspace + hook binary
- **T-C1 (Red)**: Rust integration tests — feed fixture JSON on stdin, assert row in SQLite with correct fields. Benchmark test: <15ms p50 via `hyperfine` in CI.
- **T-C2 (Green)**: `rust/hook/` crate, `main.rs`, `db.rs`, `classify.rs`, `config.rs`, schema via `include_str!`.
- **T-C3 (Refactor)**: `clean-code` pass (Rust edition: clippy pedantic + idiomatic patterns).
- Covers: B1, B2, B3, B4, B5, B14.

### Phase D — Block inference
- **T-D1 (Red)**: pytest cases for every edge case in B6–B8 plus the three edge cases from research (lunch gap, overlap, all-hands).
- **T-D2 (Green)**: `src/worklog/infer.py` with pure function `build_blocks(events) -> list[Block]`; CLI `worklog infer`.
- **T-D3 (Refactor)**: `clean-code` pass.
- Covers: B6, B7, B8.

### Phase E — `claude -p` estimator
- **T-E1 (Red)**: tests for subprocess call with mocked `claude` binary; schema validation path; malformed-JSON fallback.
- **T-E2 (Green)**: `src/worklog/estimate.py`; CLI `worklog estimate`.
- **T-E3 (Refactor)**: `clean-code` pass; extract prompt to `src/worklog/prompts/estimate_block.txt`.
- Covers: B9, B10.

### Phase F — Tempo sync rewire
- **T-F1 (Red)**: tests that sync reads from `blocks` (not events), uses real `duration_seconds`, no 15-min placeholder.
- **T-F2 (Green)**: rewrite `tempo.py` to join `blocks → events` for event ids, mark block `tempo_worklog_id`.
- **T-F3 (Refactor)**: `clean-code` pass.
- Covers: B11.

### Phase G — Web UI rewire to blocks
- **T-G1 (Red)**: FastAPI TestClient tests: `/?day=X` returns blocks grouped by company; POST `/blocks/{id}/duration` updates.
- **T-G2 (Green)**: update `web/app.py` + templates.
- **T-G3 (Refactor)**: `clean-code` pass; extract shared datetime helpers.
- Covers: B15.

### Phase H — Installer + doctor
- **T-H1 (Red)**: tests for `worklog hook install` prefers Rust binary; `worklog doctor` output format.
- **T-H2 (Green)**: update `cli.py:hook()`; add `cli.py:doctor()`.
- **T-H3 (Refactor)**: `clean-code` pass.
- Covers: B12, B13.

## Data Model Deltas

```sql
-- new table
CREATE TABLE sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT UNIQUE NOT NULL,       -- Claude session_id
    started_at TEXT NOT NULL,
    ended_at TEXT,
    end_source TEXT,                       -- stop|subagent_stop|session_end|reaper
    project_path TEXT,
    event_count INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_sessions_started ON sessions(started_at);

-- new table
CREATE TABLE blocks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    day TEXT NOT NULL,                     -- YYYY-MM-DD for fast filter
    company TEXT NOT NULL,
    jira_issue TEXT,
    started_at TEXT NOT NULL,
    ended_at TEXT NOT NULL,
    duration_seconds INTEGER NOT NULL,
    description TEXT,
    estimated_by TEXT,                     -- claude_p|gap|manual
    flagged INTEGER NOT NULL DEFAULT 0,    -- 1 if exceeds MAX_BLOCK or manual review needed
    tempo_worklog_id TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX idx_blocks_day ON blocks(day);
CREATE INDEX idx_blocks_tempo ON blocks(tempo_worklog_id);

-- new join table
CREATE TABLE block_events (
    block_id INTEGER NOT NULL REFERENCES blocks(id) ON DELETE CASCADE,
    event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    PRIMARY KEY (block_id, event_id)
);

-- events table gets a session_id column for Claude pairing
ALTER TABLE events ADD COLUMN session_id TEXT;
CREATE INDEX idx_events_session ON events(session_id);
```

## Verification Steps

### Browser Verification (after Phase G)
1. `uv run worklog serve` → open http://127.0.0.1:8765
2. Navigate to day with blocks. **Assert**: page renders blocks grouped by company, each with start–end time, duration, description.
3. Edit a block duration via the form. **Assert**: page reloads, new duration persisted (check via `worklog today`).
4. Click "Dry-run Tempo sync". **Assert**: response shows payloads referencing block `duration_seconds`, not 15-min multiples.
5. Open DevTools console. **Assert**: no errors.

### Runtime Verification (after Phase C, the Rust hook)
1. Install Rust binary: `cargo install --locked --path rust/hook`.
2. Simulate a SessionStart: `echo '{"hook_event_name":"SessionStart","session_id":"test-1","cwd":"/tmp","source":"startup"}' | worklog-hook`. **Assert**: exit 0, row in `events` with `source=claude`, `session_id=test-1`.
3. Send Stop: `echo '{"hook_event_name":"Stop","session_id":"test-1","cwd":"/tmp"}' | worklog-hook`. **Assert**: matching `sessions` row has `ended_at` set, `duration_seconds > 0`.
4. Benchmark: `hyperfine --warmup 3 "echo '...' | worklog-hook"`. **Assert**: p50 < 15ms on M-series.
5. Concurrent-write stress: spawn 20 Rust + 20 Python hook invocations in parallel. **Assert**: all 40 rows present, zero `SQLITE_BUSY` errors reach the user.

### User Verification Steps (Stage 6 — HARD GATE)
1. The Claude Code hook fires in real usage (you run a prompt in any repo): verify a row appears in `events` with `session_id`.
2. `worklog infer --day today` produces blocks that match your intuition about your day.
3. `worklog estimate --day today` produces readable Tempo-style descriptions.
4. Review UI at `/?day=today` shows those blocks correctly.
5. `worklog sync --dry-run` produces Tempo payloads with real durations.

## Quality Gates

Inline (Stage 3, run via `check-all`):
- `uv run pytest -q`
- `uv run ruff check src`
- `uv run mypy src`
- `cargo test --manifest-path rust/hook/Cargo.toml`
- `cargo clippy --manifest-path rust/hook/Cargo.toml -- -D warnings`
- `cargo fmt --manifest-path rust/hook/Cargo.toml --check`

Parallel quality agents (Stage 4, sonnet):
- Security — subprocess injection risk in `claude -p`, SQL injection in web handlers, SSRF, secret handling.
- Performance — Rust hook hot path (allocations, syscalls), Python infer scalability.
- Accessibility — web UI (contrast, keyboard nav, ARIA on form elements).
- Type-safety — mypy strict + clippy pedantic results.

## Assumptions Documented

1. Only macOS arm64 is targeted. Linux / Intel macs NOT tested.
2. `claude` binary is in PATH (already required by the user's Claude Code install).
3. Single-user machine; no multi-tenant concerns.
4. Python continues to own the `worklog init` schema creation; Rust only runs `CREATE TABLE IF NOT EXISTS` as safety net.
5. `claude -p` auth uses existing Claude Code subscription auth (no `--bare`/`ANTHROPIC_API_KEY` required). Adds ~200ms per call; acceptable for daily batch.
6. No PR created at end — merge directly to `main`.

## Non-goals (flagged for follow-up)

- Learning per-user adaptive gap threshold (insufficient data).
- Tempo Server/Data Center support (only Tempo Cloud v4).
- CI/CD; this is a personal tool on one machine.
- Structured logging / observability stack.

## Rollback Plan

If the Rust hook is broken: `worklog hook uninstall && worklog hook install --python` reverts to Python hook. Both share the same SQLite schema. Rollback is a one-command operation.

## Progress

- [x] Stage 1.2: research swarm
- [ ] Stage 1.3: plan (this doc)
- [ ] Stage 1.5: user approval HARD GATE ← **WAITING**
- [ ] Phase A — schema + sessions/blocks
- [ ] Phase B — Python hook session pairing
- [ ] Phase C — Rust hook
- [ ] Phase D — block inference
- [ ] Phase E — claude -p estimator
- [ ] Phase F — Tempo sync rewire
- [ ] Phase G — Web UI rewire
- [ ] Phase H — installer + doctor
- [ ] Stage 3: inline gates
- [ ] Stage 4: quality pipeline
- [ ] Stage 5: verification
- [ ] Stage 6: user verification HARD GATE
- [ ] Stage 7: QA
- [ ] Stage 8: merge summary
