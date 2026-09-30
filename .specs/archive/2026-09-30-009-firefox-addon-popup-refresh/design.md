# Design — Firefox add-on popup refresh

Seam: the daemon (T001, T002) and the add-on (T003, T004) must agree on two
HTTP shapes and one storage key. Nothing else crosses.

## 1. Contract file + names

Rust contract: `rust/crates/worklog-core/src/routing_contract.rs` (owner: T001).
JS mirror: the literal JSON below — the add-on has no build step, so the shapes
are copied verbatim into `extension/firefox/popup-state.js` doc comments.

| canonical | identifier | defined in | banned synonyms |
|---|---|---|---|
| recording override end | `recording_until` (RFC3339 UTC, or `null`) | routing_contract.rs | override_until, record_end, stop_at |
| meta key | `BROWSER_RECORDING_UNTIL_KEY = "browser_recording_until"` | routing_contract.rs | — |
| status payload | `BrowserStatus` | routing_contract.rs | BrowserState, Status |
| in work hours now | `in_work_hours: bool` | BrowserStatus | working, is_work_time |
| minutes today | `minutes_today: i64` | BrowserStatus | count, total_minutes |
| last heartbeat (add-on) | storage.local `lastHeartbeat` = `{ ts, title, url, stored, reason }`; `reason` ∈ daemon reasons (`outside_work_hours`, `incognito`, `personal_container`) ∪ local skips (`paused`, `idle`, `unfocused`, `daemon_down`), `null` when stored | background.js | lastSent, lastTick |

All instants are UTC RFC3339. Day bucketing uses `crate::tz::day_offset()`.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BrowserStatus {
    pub in_work_hours: bool,
    pub recording_until: Option<DateTime<Utc>>, // None when off or expired
    pub minutes_today: i64,
    pub work_hours: String,                     // e.g. "Mon-Fri 09:00-17:00"
}
```

## 2. Trust boundaries

| boundary | untrusted input | parse | failure |
|---|---|---|---|
| `POST /browser/recording` | `{ "on": bool }` | axum `Json<RecordingRequest>` | 400 on bad JSON, 403 on non-extension Origin |
| `GET /browser/status` | Origin header only | `require_extension_origin` | 403 |
| meta row `browser_recording_until` | stored string | `DateTime::parse_from_rfc3339` | unparseable → treated as `None` (log warn) |
| popup ← daemon | JSON body | `popup-state.js` treats any fetch error / non-2xx / 1000 ms timeout as `daemonDown` | status "Worklog isn't running" |

## 3. Endpoints (T002 implements, T004 calls)

- `GET  /browser/status` → `200 BrowserStatus`, header `Access-Control-Allow-Origin: <origin>`.
- `POST /browser/recording` body `{ "on": true }` → sets `recording_until = auto_stop_at(now)`; `{ "on": false }` → deletes the meta row. Returns `200 BrowserStatus` + ACAO header.
- `OPTIONS /browser/recording` → reuse `browser_heartbeat_preflight` (allows POST, content-type).
- Existing `POST /browser/heartbeat` unchanged on the wire; its response `{stored, reason}` is what `lastHeartbeat` records.

## 7. Decisions

- In the context of the work-hours filter, facing "the daemon is the only work-hours enforcer" (README), we chose a daemon-held `recording_until` in `meta`, checked inside `ingest_heartbeat`, and rejected an add-on `force: true` flag, to keep one enforcer, accepting one extra endpoint. Makes hard: browser_ingest.rs, daemon.rs.
- In the context of Pause, facing D-03, we keep `paused` in add-on `storage.local` (unchanged) and rejected moving it to the daemon, accepting that pause state is per-browser. Makes hard: background.js, popup.js.

## Complexity Tracking

| Violation | Why needed | Simpler alternative rejected because |
|-----------|-----------|--------------------------------------|

## Contract for T001 — Recording override + status in core
CONTRACT   rust/crates/worklog-core/src/routing_contract.rs — you own it: add BrowserStatus and BROWSER_RECORDING_UNTIL_KEY exactly as §1.
NAMES      recording_until, BrowserStatus, in_work_hours, minutes_today, BROWSER_RECORDING_UNTIL_KEY
MODULE     rust/crates/worklog-core/src/browser_ingest.rs · may import: crate::{models, repo, routing_contract, purge::{meta_get, meta_set}}
CALLER     daemon.rs heartbeat handler: change only its ingest_heartbeat call to pass `None` (T002 replaces it).
CALLS      impl WorkHours { pub fn auto_stop_at(&self, now: DateTime<Utc>, offset: FixedOffset) -> DateTime<Utc> }  // next local calendar day at end_time
           pub fn ingest_heartbeat(conn, hb, hours, offset, recording_until: Option<DateTime<Utc>>) -> Result<IngestOutcome>  // bypass work-hours only when hb.ts < recording_until
           pub fn recording_until(conn: &Connection, now: DateTime<Utc>) -> Result<Option<DateTime<Utc>>>  // None when missing, unparseable or <= now
           pub fn set_recording(conn: &Connection, until: Option<DateTime<Utc>>) -> Result<()>  // None deletes the row
           pub fn minutes_today(conn: &Connection, now: DateTime<Utc>, offset: FixedOffset) -> Result<i64>  // SELECT COUNT(*) FROM events WHERE source = SOURCE_FIREFOX AND started_at >= ?1 AND started_at < ?2, bounds = local midnight of `now` and +1 day, converted to UTC and formatted `to_rfc3339_opts(Secs, true)` (matches stored `...Z`). Rows are already one per minute (upsert on minute source_id).
DUPLICATE  none — because no shared helper exists for these.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T002 — Daemon endpoints
CONTRACT   rust/crates/worklog-core/src/routing_contract.rs — import BrowserStatus; never redeclare it.
NAMES      recording_until, BrowserStatus, in_work_hours, minutes_today
MODULE     rust/crates/worklog-core/src/daemon.rs · may import: browser_ingest, routing_contract, tz
CALLS      GET /browser/status, POST+OPTIONS /browser/recording exactly as design §3; both call require_extension_origin; the existing heartbeat handler passes browser_ingest::recording_until(c, Utc::now())? into ingest_heartbeat.
DUPLICATE  none — reuse browser_heartbeat_preflight for the OPTIONS route.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T003 — Popup view-state
CONTRACT   the JSON in design §1/§3, copied verbatim into a doc comment at the top of popup-state.js.
NAMES      recording_until, in_work_hours, minutes_today, lastHeartbeat, daemonDown, paused
MODULE     extension/firefox/popup-state.js · pure ES module, no `browser.*` calls · exports: viewState, formatMinutes, formatTimeLeft
CALLS      PRECEDENCE status: daemon_down > not_counted(incognito|personal_container) > paused > not_counted(idle|unfocused) > recording(in_work_hours || recording_until > now) > outside.
           primary: daemon_down → null; paused → resume; in_work_hours → pause; recording_until > now → stop; else → start. secondary: {stop} only when in_work_hours && recording_until > now && !paused.
           export function viewState({ status /* BrowserStatus | null */, daemonDown, paused, lastHeartbeat, now /* Date */ }) → { status: "recording"|"paused"|"outside"|"not_counted"|"daemon_down", headline, detail, primary: { label, action: "pause"|"resume"|"start"|"stop" } | null, secondary: { label, action: "stop" } | null, tab: { title, url } | null, minutesToday: string, timeLeft: string | null }
DUPLICATE  PERSONAL_CONTAINER stays in heartbeat.js; do not move it.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T004 — Popup UI + background wiring
CONTRACT   popup-state.js exports (T003) — import them; never re-derive status in popup.js.
NAMES      lastHeartbeat, recording_until, BrowserStatus
MODULE     extension/firefox/{background.js,heartbeat.js,heartbeat.test.js,popup.html,popup.js,popup.css,README.md}
CALLS      fetch GET/POST as design §3 with AbortSignal.timeout(1000); heartbeat.js gains `export function skipReason(state) → null | "paused"|"incognito"|"personal_container"|"idle"|"unfocused"` and shouldSend becomes `skipReason(state) === null`; background.js writes storage.local lastHeartbeat every tick (sent or skipped); "Open worklog" → browser.tabs.create({ url: "http://127.0.0.1:3333" }).
DUPLICATE  HEARTBEAT_URL base stays a literal in each file that needs it; no shared config module.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.
