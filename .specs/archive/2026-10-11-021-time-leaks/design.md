# Design — 021 Time leaks

## 1. Contract files

Rust: `rust/crates/worklog-core/src/time_leaks_contract.rs` (module `time_leaks_contract`, listed in `lib.rs`).
Web: `web/lib/time_leaks_contract.ts` mirrors it field for field (snake_case keys, as serde sends them).
Both are written by T000 and owned by the orchestrator. A missing shape is an escalation.

```rust
//! Spec 021 shared shapes. Mirrored by `web/lib/time_leaks_contract.ts`.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IgnoreReason { Noise, Duplicate }
impl IgnoreReason {
    pub fn as_str(self) -> &'static str;          // "noise" | "duplicate"
    pub fn parse(s: &str) -> Option<IgnoreReason>;
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LooseEndMissing { Tempo, Export }

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct LooseEnd { pub block_id: i64, pub day: String, pub jira_issue: String,
    pub started_at: String, pub ended_at: String, pub seconds: i64,
    pub description: Option<String>, pub missing: Vec<LooseEndMissing> }

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct LooseEnds { pub cycle_start: String, pub cycle_end: String, pub items: Vec<LooseEnd> }

pub fn collect_ok_key(source: &str) -> String;   // "collect.<source>.last_ok"
pub fn collect_err_key(source: &str) -> String;  // "collect.<source>.last_err"

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SourceHealth { pub source: String, pub last_ok: Option<String>,
    pub last_err: Option<String>, pub events_24h: i64 }

pub const GITHUB_MAX_PAGES: u32 = 10;
pub const FOCUS_MAX_GAP_SECONDS: i64 = 900;
pub const DEILD_SUGGESTION_SOURCE: &str = "block_customer_shares"; // all history, ties → most recent

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FocusRun { pub jira_issue: String, pub started_at: String, pub ended_at: String, pub seconds: i64 }

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FocusSummary { pub day: String, pub switches: u32, pub longest: Option<FocusRun> }

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DeildSuggestion { pub customer: String, pub deild: String, pub uses: u32 }
```

| canonical | identifier | defined in | banned synonyms |
|---|---|---|---|
| ignore reason | `IgnoreReason`, column `blocks.ignore_reason` | contract | why, cause, kind |
| loose end | `LooseEnd` | contract | orphan, unsent, pending |
| source health | `SourceHealth` | contract | collector status, heartbeat |
| focus summary | `FocusSummary` | contract | streak, deep work |
| deild suggestion | `DeildSuggestion` | contract | hint, guess, default |

Times are ISO-8601 UTC strings, as everywhere in the DB. Durations are whole seconds (`i64`). Meta values: `last_ok` = ISO time; `last_err` = `"<ISO time> <message>"` (split on the first space).

## 2. Trust boundaries

| boundary | untrusted input | parse fn | failure |
|---|---|---|---|
| `POST /blocks/:id/ignore` body | `{ignored: bool, reason?: string}` | serde into `Option<IgnoreReason>` | unknown reason → `ApiError::bad_request` (400) |
| `blocks.ignore_reason` row | any TEXT | `IgnoreReason::parse` | unknown → `None`, never panic |
| meta `collect.*` values | any TEXT | split on first space | malformed → shown raw |
| GitHub search JSON | `total_count`, `items` | existing serde structs | missing field → stop paging, keep collected items |

## 3. Errors

Daemon: reuse `ApiError` (daemon.rs). No new variant. Bad input = `BadRequest`; missing block = `NotFound`.
CLI: `anyhow` as today. Web: a failed section is a name in `DayLoadNotice`'s `failed: string[]`.

## 5. Shared resources

DB handle: the existing `&Connection` parameter; no task opens its own. Today/day: the existing `tz` helpers. Billing cycle: `purge::configured_cycle_start_day()`, `purge::configured_close_day()`, `purge::cycle_start_on_or_before()`. Schema change: one column via the existing `ensure_blocks_*` pattern in `db.rs`; `SCHEMA_VERSION` stays 21 (Ruling, spec 018: a bump broke four db_test pins).

## 6. Deliberately duplicated

- GitHub paging loop lives in `github.rs`; do not extract a shared helper with `slack.rs` or `jira.rs`.
- The web contract file duplicates the Rust shapes by hand; no codegen.

## 7. Decisions

- In the context of ignore, facing 10+ readers that filter only `is_personal`, we chose to keep `is_personal = 1` on ignore and teach the two CLI aggregators to check `ignored_at` first, and rejected clearing `is_personal` on ignore, to keep every invoice/Tempo filter correct, accepting that the flag means "personal or ignored". Makes hard: `block_service.rs`, `tempo_lines.rs`, `billing.rs`.
- In the context of the loose-ends list, facing billing cycles that do not align with weeks, we chose a new read-only module and route, and rejected extending `week_closeout`, to keep the weekly view unchanged, accepting a second "pending" query. Makes hard: `loose_ends.rs`, `daemon.rs`.

## Contract for T001 — CLI ignored bucket
USAGE      let agg = aggregate_day(day, &blocks); assert_eq!(agg.ignored_secs, 3600);
CALLS      `DayAgg` gains `ignored_secs: i64` and `ignored_n: usize`; ignored = `ignored_at.is_some()`, checked BEFORE `is_personal`.
NAMES      "ignored" column header; JSON key `ignored_secs`.

## Contract for T003 — Auto-send outcomes
CALLS      existing `set_outcome(conn, &key, Option<&str>)`; skipped reason text = the `SyncResult.reason`; keep `ALREADY_IN_TEMPO` lines as sent.

## Contract for T007 — Collect health core
USAGE      collect_health::record(conn, "github", Ok(())); let rows = collect_health::all(conn, now)?;
CALLS      `pub fn record(conn: &Connection, source: &str, outcome: Result<(), &str>) -> Result<()>`; `pub fn all(conn: &Connection, now: DateTime<Utc>) -> Result<Vec<SourceHealth>>`
CONTRACT   `collect_ok_key`, `collect_err_key`, `SourceHealth`.

## Contract for T008 — Collect wiring and status rows
CALLS      `collect_health::record` after every collector outcome in `cmd_collect` (Ok, Skip, Warn, RoutingFailed, Err); `collect_health::all` in `cmd_status`.

## Contract for T009 — Hook and transcript errors
CALLS      `collect_health::record(conn, "claude", Err(msg))` / `("claude_transcripts", Err(msg))`. Hook still returns Ok and prints nothing to stdout.

## Contract for T010 — Ignore reason column
CALLS      `pub fn set_ignored(conn: &Connection, block_id: i64, ignored: bool, reason: Option<IgnoreReason>) -> Result<Block>`; un-ignore clears the reason. `Block` gains `ignore_reason: Option<IgnoreReason>`.
CONTRACT   `IgnoreReason`.

## Contract for T011 — Infer carries reason
CALLS      the carry rule that copies `ignored_at` copies `ignore_reason` from the same prior row.

## Contract for T012 — Daemon ignore route
USAGE      POST /blocks/12/ignore {"ignored":true,"reason":"noise"} → Block JSON with "ignore_reason":"noise"
CALLS      body `{ ignored: bool, #[serde(default)] reason: Option<IgnoreReason> }`.

## Contract for T013 — Web ignore menu
USAGE      setIgnored(id, true, "noise"); mergeBlocks([neighbourId, id], neighbourId); open TicketCombobox
CONTRACT   `IgnoreReason` from `web/lib/time_leaks_contract.ts`.

## Contract for T014 — Loose ends core
USAGE      let le = loose_ends::for_cycle(conn, "2026-10-11")?; // LooseEnds
CALLS      `pub fn for_cycle(conn: &Connection, today: &str) -> Result<LooseEnds>`; unsynced = `COALESCE(tempo_worklog_id,'') = ''`; unexported = `COALESCE(exported_at,'') = ''`.

## Contract for T015 — Loose ends route
USAGE      GET /loose-ends → LooseEnds JSON

## Contract for T016 — Loose ends page
USAGE      const le: LooseEnds = await fetchLooseEnds(); // web/lib/daemonLooseEnds.ts

## Contract for T017 — Deild suggestion core and route
USAGE      GET /blocks/12/deild-suggestion → DeildSuggestion JSON or null
CALLS      most frequent deild in the Owner's saved split rows (`block_customer_shares`) for the block's customer; ties → most recent.

## Contract for T018 — Deild chip
CALLS      click → the existing save path in `BlockCustomerSplit` (no new write route).

## Contract for T019 — Focus summary
USAGE      let f = standup::focus_summary(day, &blocks); // FocusSummary
CALLS      `pub fn focus_summary(day: &str, blocks: &[Block]) -> FocusSummary`; skip personal and ignored; switch = a block whose `jira_issue` differs from the previous one (by started_at; None is its own value); a run = consecutive same-ticket blocks with gaps ≤ `FOCUS_MAX_GAP_SECONDS`, seconds = sum of block seconds; None never wins. Draft JSON gains `focus: FocusSummary | null`; the post body never includes it.

## Contract for T020 — Focus surfaces
CONTRACT   `FocusSummary` (Rust and web).
