# Performance — feature/delete-python

## Verdict
PASS

## Checked
- `rust/crates/worklog-core/src/collectors/gcal.rs` (all 844 lines)
- `rust/crates/worklog-cli/src/cli.rs` — `cmd_day` (925-1020) and `cmd_collect` (716-783)
- `.github/workflows/release.yml`
- `install.sh`, `scripts/release-smoke.sh`

## Findings

### MINOR — `zstd -19` is overkill in the release workflow
**File:** `.github/workflows/release.yml`, `scripts/release-smoke.sh`
**Confidence:** HIGH

Level 19 is BTULTRA2 — the slowest search strategy. On a stripped Rust binary (~5-10 MB) it spends 20-40s per artifact for 1-3% size saving vs `-11`. The self-updater verifies the zst bytes but is indifferent to which level produced it.

**Suggested fix:** `zstd -11 -f -o …`

### MINOR — `cmd_day` collectors run sequentially
**File:** `rust/crates/worklog-cli/src/cli.rs:950-974`
**Confidence:** MEDIUM

Three independent remote hosts (jira/github/gcal) could run in parallel for ~2-5s EOD latency improvement. Design is intentionally blocking/synchronous — noted as such in the gcal.rs module doc. Parallelising needs `Mutex<Connection>` or a channel; complexity not worth it absent user complaints. Left as-is.

## Passes (explicit)

- HTTP client reused across pages + calendars (no reconnects in hot loop).
- No unbounded buffering — events upserted per-item within each 250-item page.
- SQLite connection never held open across network calls.
- 60s token-refresh buffer is well-calibrated (matches `google-auth` Python).
- `install.sh` has no polling loops, no cold caches, no unnecessary subshells.
