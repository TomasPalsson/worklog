# Design — Estimate progress per person (020)

## 1. Contract file + language

Rust: `rust/crates/worklog-core/src/estimate_progress_contract.rs` (written by T001; every Rust task imports it). Web mirror: the `TicketProgress*` types appended to `web/lib/types.ts` by T005. The field names are identical (snake_case JSON).

```rust
pub const PROGRESS_STALE_SECS: i64 = 600;          // §5 cache freshness
pub const PROGRESS_CHART_DAYS: usize = 14;         // A4
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PersonHours { pub account_id: String, pub name: String, pub is_you: bool,
    pub seconds: i64, pub by_day: Vec<(NaiveDate, i64)> }      // by_day: per-day logged, ascending
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TicketProgress { pub key: String, pub estimate_seconds: Option<i64>,
    pub people: Vec<PersonHours>,                    // you first, then seconds desc; NOT folded
    pub logged_seconds: i64,                         // == sum(people.seconds)
    pub pulled_at: Option<String>,                   // ISO-8601 UTC; None = never fetched
    pub error: Option<ProgressError> }
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProgressError { JiraUnavailable, NotConfigured }
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DayProgress { pub day: NaiveDate, pub tickets: Vec<TicketProgress> }
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RawWorklog { pub worklog_id: String, pub account_id: String, pub name: String,
    pub day: NaiveDate, pub seconds: i64 }           // one Jira worklog, day via tz::local_date
```

| canonical | identifier | plural | defined in | banned synonyms |
|---|---|---|---|---|
| ticket progress | `TicketProgress` | `tickets` | contract | estimate_view, budget, burn |
| person hours | `PersonHours` | `people` | contract | collaborator, author_hours |
| estimate | `estimate_seconds` | — | contract | original_estimate, allowance |
| logged | `logged_seconds` | — | contract | spent, used (`used` is web-only: logged + pending) |
| pending | `pending_seconds` | — | web `lib/progress.ts` | unsynced, this_block |

Seconds are i64 everywhere. Days are `NaiveDate`, bucketed with `tz::local_date` (CLAUDE.md: `$WORKLOG_TZ`). Timestamps are UTC ISO-8601. The daemon handlers are async; the collector and store are sync and run in `spawn_blocking` (the `daemon_week::pull_week` pattern).

## 2. Trust boundaries

| boundary | untrusted input | parse fn | failure granularity |
|---|---|---|---|
| Jira search (estimates) | JSON `issues[].fields.timetracking.originalEstimateSeconds` (may be absent) | `jira_time::fetch_estimates_with` | missing field → `None`, never 0 |
| Jira `/issue/{key}/worklog` | paged JSON `worklogs[].author{accountId,displayName}`, `started`, `timeSpentSeconds` | `jira_time::fetch_worklogs_with` | one ticket fails → that ticket's `error`, others proceed |
| `progress` DB rows | written by us, read back | `ticket_progress::view` | missing row → `pulled_at: None` |

## 3. Error taxonomy

`ProgressError` (contract, closed). Jira transport/5xx/401 → `JiraUnavailable`; no Jira credentials → `NotConfigured`. The HTTP route always returns 200 with `DayProgress`; errors ride per ticket. Web copy: `JiraUnavailable` → "Couldn't load hours for KEY — Jira didn't answer. Your blocks are safe."; `NotConfigured` → "Connect Jira in Settings to see estimates."

## 4. Module boundaries

- `collectors/jira_time.rs` · may import: `collectors::jira` (`JiraAuth`, `str_at` — T002 makes it `pub(crate)`, its only edit to jira.rs), `http`, `tz`, contract · exports: `fetch_estimates_with`, `fetch_worklogs_with`
- `ticket_progress.rs` · may import: contract, `db` · exports: `store_ticket`, `stale_keys`, `view`, `day_ticket_keys`, `mark_stale`
- `daemon_progress.rs` · may import: contract, `ticket_progress`, `jira_time`, `collectors::tempo::resolve_account_id`, `secrets` · exports: `get_day_progress`
- `web/lib/progress.ts` · pure; may import `@/lib/types` only · exports: `barModel`, `chartModel`, `toneFor`, `initials`, `pendingSeconds`
- Anything not listed is a bug.

## 5. Shared resources

| resource | constructed by | passed how | received by |
|---|---|---|---|
| SQLite conn | daemon state (existing) | `&Connection` | `ticket_progress::*` |
| reqwest `Client` | `crate::http` (existing) | `&Client` | `jira_time::*` |
| clock | caller | `now: DateTime<Utc>` argument | `ticket_progress::stale_keys` (never `Utc::now()` inside) |
| schema | `sql/schema.sql` `CREATE TABLE IF NOT EXISTS` | — | T001 only |

## 6. Deliberately duplicated

- Duration formatting: the web uses the existing `formatDuration` (`web/lib/format.ts`). Do not add a new one.
- Person colours live in CSS tokens (`--p1..--p4`, from mock.html), not in TS.

## 7. Decisions

- In the context of per-person hours, facing "Tempo read-back today is own-user only" (`tempo.rs:1263`), we chose Jira `/rest/api/3/issue/{key}/worklog` and rejected Tempo `/worklogs/issue/{id}`, to get display names in one call, accepting A1. Makes hard: `jira_time.rs`.
- In the context of page speed, facing "FR-15a: nothing may wait", we chose a client fetch through a server action after first paint and rejected loading in `page.tsx`'s `Promise.all`. Makes hard: `DayProgressProvider.tsx`.

## Contract for T001 — Contract + schema
CONTRACT   rust/crates/worklog-core/src/estimate_progress_contract.rs — create exactly §1's code.
STUBS      create empty `collectors/jira_time.rs`, `ticket_progress.rs`, `ticket_progress_test.rs`, `daemon_progress.rs`, `daemon_progress_test.rs` and register them in `lib.rs` / `collectors/mod.rs`.
CALLS      schema.sql: `ticket_progress(key TEXT PK, estimate_seconds INTEGER, logged_seconds INTEGER NOT NULL DEFAULT 0, pulled_at TEXT NOT NULL)`, `ticket_progress_worklogs(worklog_id TEXT PK, key TEXT NOT NULL, account_id TEXT NOT NULL, name TEXT NOT NULL, day TEXT NOT NULL, seconds INTEGER NOT NULL)` + index on `key`.

## Contract for T002 — Jira time collector
CONTRACT   import `RawWorklog` from estimate_progress_contract.
CALLS      `fetch_estimates_with(auth: &JiraAuth, keys: &[String], client: &Client) -> Result<HashMap<String, Option<i64>>>`; `fetch_worklogs_with(auth: &JiraAuth, key: &str, client: &Client) -> Result<Vec<RawWorklog>>` (follow `startAt`/`total`).

## Contract for T003 — Progress store
CALLS      `store_ticket(conn, key, estimate: Option<i64>, logs: &[RawWorklog], now) -> Result<()>` (replace that key's rows in one transaction); `stale_keys(conn, keys, now) -> Result<Vec<String>>`; `view(conn, keys, me: Option<&str>) -> Result<Vec<TicketProgress>>`; `day_ticket_keys(conn, day) -> Result<Vec<String>>` (distinct non-personal `jira_issue` of that day's blocks); `mark_stale(conn, keys) -> Result<()>` (sets `pulled_at` to the epoch so the next `stale_keys` returns them; never deletes rows).

## Contract for T004 — Daemon route
CALLS      `GET /progress/:day[?refresh=KEY]` → `Json<DayProgress>` (`refresh` forces that one ticket, FR-04b); registered in daemon.rs next to `/days/:day`. After `run_sync` (daemon.rs) syncs a day, call `ticket_progress::mark_stale` with the synced blocks' tickets (FR-03a).
