# Type Safety — feature/delete-python

## Verdict
PASS

## Checked
- `rust/crates/worklog-core/src/collectors/gcal.rs` (844 lines, new)
- `rust/crates/worklog-cli/src/cli.rs` — `cmd_day` and gcal arm of `cmd_collect`

## Findings

All informational; none blocking.

- **`expect("00:00:00 is always a valid time")`** (gcal.rs:125). `(0,0,0)` is structurally valid; `and_hms_opt` only errors on out-of-range. Not a defect.
- **`resp.text().unwrap_or_else(…)` fallbacks** (gcal.rs:181, :292). Used only to populate an error string already on the `Err(…)` path. No data-loss risk.
- **`ev.summary.unwrap_or_else(|| "(no title)".into())`** (gcal.rs:328). Google marks summary optional; silent fallback matches prior Python behaviour.
- **`expires_in.unwrap_or(3600)`** (gcal.rs:188). OAuth2 spec marks it RECOMMENDED not REQUIRED; 3600 matches Google's documented default.
- **`needs_refresh` None-handling** (gcal.rs:207-220). `None` expiry → treat as expired. Correct.
- **`String` newtype for tokens** (gcal.rs:89-100). No struct-field mixing risk; newtype would be zero-cost but unnecessary.
- **`duration_seconds` sign** (gcal.rs:313-320). `(b - a).num_seconds()` could be negative on malformed feed; `build_blocks` recomputes independently, so no downstream contamination.
- **`source_id = "{cal}:{id}"`** (gcal.rs:324). Bound as SQLite parameter; no injection surface.
- **Concurrency** — no Arc/Mutex; single-threaded blocking by design.
