# Design — 014 Logged

Only what two tasks must agree on. Spec says WHAT; this says the shared shapes.

## 1. Contract files + language

- Rust: `rust/crates/worklog-core/src/logged_contract.rs` (registered in `lib.rs`). Owner: orchestrator (T001).
- TS mirror: `web/lib/logged_contract.ts`. Owner: orchestrator (T001). Hand-written mirror, same as `tempo_line_contract`.

```rust
pub const LOGGED_MAX_RANGE_DAYS: i64 = 42;      // inclusive from..=to
pub const DISMISS_REASON_MAX_CHARS: usize = 80; // after trim, chars not bytes
#[serde(rename_all = "snake_case")]
pub enum DayState { NotFetched, Pending, Off, Full, Under, Dismissed }
pub struct LoggedEntry { tempo_worklog_id: String, issue_id: i64, jira_issue: Option<String>,
                         seconds: i64, description: String, owner: WorklogOwner }
pub struct LoggedDay { day: String, logged_seconds: i64, required_seconds: Option<i64>,
                       state: DayState, dismissal_reason: Option<String>, entries: Vec<LoggedEntry> }
pub struct LoggedRange { from: String, to: String, today: String,
                         days: Vec<LoggedDay>, pulled_at: Option<String> }
pub struct RangeBody { from: String, to: String }
pub struct DismissBody { day: String, reason: String }
pub struct DayBody { day: String }
pub fn day_state(day, today, logged_seconds, required_seconds: Option<i64>,
                 entry_count: usize, dismissed: bool) -> DayState
```

`day_state` precedence (first match wins) — the single definition of the flag:
1. `required_seconds == None && entry_count == 0` → `NotFetched`
2. `day >= today` → `Pending`
3. `required_seconds` is `None` or `Some(0)` → `Off`
4. `logged_seconds >= required` → `Full`
5. `dismissed` → `Dismissed`
6. else → `Under`

| canonical | identifier | plural | defined in | banned synonyms |
|---|---|---|---|---|
| logged range | `LoggedRange` | — | logged_contract | overview, report, timesheet |
| logged day | `LoggedDay` | `days` | logged_contract | row, cell |
| entry | `LoggedEntry` | `entries` | logged_contract | worklog row, item |
| dismissal | `dismissal_reason` / `tempo_day_dismissals` | — | logged_contract / schema.sql | excuse, note, ack |
| day state | `DayState` | — | logged_contract | status, flag kind |

Days are `YYYY-MM-DD` strings; `today` is the daemon's local date via `tz.rs` (`$WORKLOG_TZ`). Seconds are `i64`/`number`. `pulled_at` is RFC3339 UTC = `MAX(pulled_at)` over the range's rows, `None` when no rows.

## 2. Trust boundaries

| boundary | untrusted input | parse fn | failure |
|---|---|---|---|
| `GET /logged?from&to`, `POST /logged/pull` | from/to strings | `parse_range` in `daemon_logged.rs` | 400 when not dates, from > to, or > 42 days |
| `POST /logged/dismiss` | day, reason | same file | 400 on bad day or reason empty / > 80 chars after trim |
| Tempo API | worklogs, schedule | existing `list_worklogs_with` / `user_schedule_with` | 502, DB untouched |

## 3. Error taxonomy

Reuse `ApiError` + `hub_error` (`daemon_tasks.rs`): 400 / 404 / 502, body `{"error": string}`. Web: `DaemonError` → `{ok:false, error}` via the `run` pattern in `actions-hub.ts`. No new variants.

## 4. Module boundaries

- `logged_contract.rs` · may import: serde, `tempo_hub_contract::WorklogOwner` · exports: §1.
- `logged.rs` · may import: logged_contract, rusqlite · exports: `logged_range(conn, from, to, today) -> Result<LoggedRange>`, `dismiss(conn, day, reason, now)`, `undismiss(conn, day)`.
- `tempo_remote.rs` · adds `store_range(conn, from, to, worklogs, schedule, pulled_at)`; `store_week` delegates to it.
- `daemon_logged.rs` (child of `daemon.rs` via `#[path]`, like `daemon_week.rs`) · exports handlers `get_range`, `pull_range`, `dismiss_day`, `undismiss_day`.
- `web/lib/daemonLogged.ts` · `getLogged(from,to)`, `pullLogged(from,to)`, `dismissDay(day,reason)`, `undismissDay(day)`; all return `LoggedRange` / `LoggedDay`.
- `web/app/actions-logged.ts` · "use server" wrappers, `revalidatePath("/logged", "layout")` on writes.

## 5. Shared resources

| resource | constructed by | passed how | received by |
|---|---|---|---|
| SQLite conn | daemon `Shared` | `with_conn` | logged.rs, tempo_remote.rs |
| Tempo auth | `TempoAuth::from_secrets()` | arg | `pull_range` only |
| Table `tempo_day_dismissals(day TEXT PK, reason TEXT NOT NULL, dismissed_at TEXT NOT NULL)` | `sql/schema.sql` `CREATE TABLE IF NOT EXISTS` | — | logged.rs only. No `SCHEMA_VERSION` bump |
| Routes | `daemon.rs` `router()` | — | `GET /logged`, `POST /logged/pull`, `POST /logged/dismiss`, `POST /logged/undismiss` |
| Shared top menu | `web/components/AppNav.tsx`, mounted once in `web/app/layout.tsx` | — | every page; page headers keep only their own date controls |

## 6. Deliberately duplicated

- Range date math (month grid, shift month) lives in `web/lib/format.ts` beside `mondayOf`/`shiftWeek`; do not add a date library.
- `run<T>` stays duplicated in `actions-logged.ts` as `actions-hub.ts` does — do not extract.

## 7. Decisions

- In the context of the flag, facing three views + the Rust reader needing one answer, we chose `day_state` in the contract and rejected computing it in TS, to keep one definition testable in Rust, accepting a round trip for state. Makes hard: logged_contract.rs, logged_contract.ts.
- In the context of auto-fetch, facing Tempo latency, we chose render-stored-then-`pullLogged`-on-mount (client) and rejected fetching in the server component, so a page never waits on Tempo, accepting a brief stale render. Makes hard: LoggedFetch.tsx.

## Contract for T003 — Range store + logged reader
CONTRACT   rust/crates/worklog-core/src/logged_contract.rs — import from it. A type you need that is not there is an escalation, never a local declaration.
NAMES      LoggedRange, LoggedDay (days), LoggedEntry (entries), dismissal_reason / tempo_day_dismissals, DayState — banned: overview, report, row, excuse, note, status
MODULE     logged.rs · may import: logged_contract, rusqlite · exports: logged_range, dismiss, undismiss; tempo_remote.rs adds store_range
CALLS      logged_range(conn:&Connection, from:NaiveDate, to:NaiveDate, today:NaiveDate) -> Result<LoggedRange>; dismiss(conn, day:NaiveDate, reason:&str, now:&str) -> Result<()>; undismiss(conn, day:NaiveDate) -> Result<()>; store_range(conn, from, to, &[PulledWorklog], &[RequiredDay], pulled_at:&str) -> Result<PullReport-like counts>; day_state (contract)
DUPLICATE  none
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T004 — Daemon routes
CONTRACT   rust/crates/worklog-core/src/logged_contract.rs — import from it.
NAMES      as T003
MODULE     daemon_logged.rs · child of daemon.rs via #[path] · may import: logged, tempo_remote, collectors::tempo, daemon_tasks::hub_error · exports: get_range, pull_range, dismiss_day, undismiss_day
CALLS      GET /logged?from=&to= -> LoggedRange; POST /logged/pull RangeBody -> LoggedRange (Tempo fetch on blocking pool, lock released, then store_range, then logged_range); POST /logged/dismiss DismissBody -> LoggedDay; POST /logged/undismiss DayBody -> LoggedDay
DUPLICATE  none
THE FIVE   (1)–(5) as above.

## Contract for T005 — Web client + actions
CONTRACT   web/lib/logged_contract.ts — import from it.
NAMES      as T003
MODULE     web/lib/daemonLogged.ts (uses `call` from web/lib/daemon.ts) · web/app/actions-logged.ts ("use server")
CALLS      getLogged(from,to): Promise<LoggedRange>; pullLogged(from,to): Promise<LoggedRange>; dismissDay(day,reason): Promise<LoggedDay>; undismissDay(day): Promise<LoggedDay>; actions: loadLogged, refreshLogged, dismissLoggedDay, undismissLoggedDay → ActionResult<…>
DUPLICATE  run<T> stays local to actions-logged.ts
THE FIVE   (1)–(5) as above.

## Contract for T009 — Logged views
CONTRACT   web/lib/logged_contract.ts — import from it.
NAMES      as T003
MODULE     web/app/logged/** pages; web/components/Logged*.tsx, DismissDay.tsx · may import: actions-logged, format.ts, logged_contract
CALLS      server pages call getLogged; LoggedFetch (client) calls refreshLogged(from,to) on mount + refresh button, then router.refresh(); DismissDay calls dismissLoggedDay
DUPLICATE  none
THE FIVE   (1)–(5) as above.
