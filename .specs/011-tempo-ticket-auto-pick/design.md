# Design — Tempo ticket auto-pick (spec 011)

## 1. Contract files

- Rust: `rust/crates/worklog-core/src/tempo_line_contract.rs` (registered in `lib.rs`). Tasks `use crate::tempo_line_contract::{...}`.
- Web: `web/lib/tempo_line_contract.ts`. Tasks `import type { ... } from "@/lib/tempo_line_contract"` (or the relative path the neighbours use).
- Both typecheck today (`cargo check -p worklog-core`, `bunx tsc --noEmit`). Orchestrator owns both; a missing type is an escalation.

| canonical | identifier | plural | defined in | banned synonyms |
|---|---|---|---|---|
| ticket origin | `TicketOrigin` / `blocks.ticket_origin` | — | contract | ticket_source, assigned_by, pick_origin |
| ticket line | `TempoLine` | `lines` | contract | tempo_row, worklog_line, group_line |
| line key | `TempoLineKey { day, jira_issue }` | — | contract | issue_key, line_id |
| hours override | `hours_override_seconds` (i64 seconds) | — | contract | override_hours, manual_hours, minutes |
| union seconds | `union_seconds` | — | contract | total_seconds, sum_seconds |
| effective seconds | `effective_seconds` | — | contract | billed_seconds, final_seconds |
| line text origin | `LineTextOrigin` (reused from `clues_contract`) | — | clues_contract | text_source |

- Time: hours travel as integer **seconds**, multiples of `HALF_HOUR_SECONDS`. The UI shows hours; it converts at the edge.
- Day: `YYYY-MM-DD` string, same as every other daemon route.
- Rust DB fns are sync `fn(&Connection, ..)`; daemon handlers wrap them in `with_conn`. LLM calls never run inside `with_conn` (prepare → invoke → commit, as in `daemon_line_text.rs`).

## 2. Trust boundaries

| boundary | untrusted input | parse | failure |
|---|---|---|---|
| `POST /tempo/lines/hours` | `SetTempoLineHours.seconds` | `tempo_lines::set_hours` rejects `Some(s)` unless `s > 0 && s % 1800 == 0` | 400, nothing written |
| `POST /tempo/lines/text` | `SetTempoLineText.text` | trim; blank = clear | never fails on content |
| `blocks.ticket_origin` DB value | any string / NULL | `TicketOrigin::parse` → `None` for unknown | reads as `None` (≈ auto) |
| web number input | free text "2", "2.5", "" | `""` → clear; else hours×3600, must be a multiple of 1800 | inline error, no request |

## 3. Errors

No new error type. Daemon handlers return the same shapes `daemon_line_text.rs` uses: 400 for invalid input, 404 when the day has no line for that `jira_issue`. Web actions return the existing `ActionResult` from `run(fn, day)` (`web/app/actions-line-text.ts`).

## 5. Shared resources

| resource | constructed by | passed how | received by |
|---|---|---|---|
| SQLite conn | daemon `Shared` / CLI `db::open` | `&Connection` | all `tempo_lines` fns |
| Schema version | T001 only: `SCHEMA_VERSION = 17` + `ensure_*` fns | — | nobody else bumps it |
| LLM invoker | `estimate::build_regenerate_invoker` (daemon) / `estimate::build_invoker` (CLI) | `&dyn ModelInvoker` | `tempo_lines::generate_text` |
| Line summary | `collectors::tempo::summarize_descriptions` + `cap_description` (made `pub(crate)` by T005) | fn call | `tempo_lines`, sync |
| Union helper | `billing::union_seconds` + `billing::block_interval` (already `pub(crate)`) | fn call | `tempo_lines` only |

## 6. Deliberately duplicated

- `tempo_line_texts` is a **sibling** of `billing_line_texts`, not a new kind inside it. Do not generalise `line_text.rs`, `BillingLineKey` or `JobTracker`: the billing key is wired into its PK and SQL, and billing must stay unchanged (non-goal).
- Regenerate is a plain request/response (one LLM call per line). Do not copy the billing async job + status polling.
- The web edit UI copies the BillingGroup text-edit pattern into TicketGroup. Only `OriginIcon` and `originLabel` get exported from BillingGroup and shared; nothing else is extracted.

## 7. Decisions

- In the context of who set a ticket, facing that `estimated_by` describes the description, not the ticket (`schema.sql:80`), we chose a new nullable `blocks.ticket_origin` column and rejected reusing `estimated_by`, to lock hand-set tickets without touching description logic, accepting one migration. Makes hard: `db.rs`, `schema.sql`, `models.rs`, `repo.rs`.
- In the context of line text freshness, facing the scheduler adding blocks all day, we chose a `source_hash` column (hash of the sorted, trimmed descriptions) so a **generated** row whose hash no longer matches is regenerated; manual rows are never touched. Rejected: never regenerating (stale text goes to Tempo). Makes hard: `tempo_lines.rs`.
- In the context of sync, facing "Tempo gets exactly what the line shows", sync reads `tempo_lines::line_for` for text + `effective_seconds` on the aggregated path only; MixedLegacy keeps its per-block path. Makes hard: `collectors/tempo.rs`.

## Complexity Tracking

| Violation | Why needed | Simpler alternative rejected because |
|-----------|-----------|--------------------------------------|
| none | — | — |

---

## Contract for T001 — Schema: ticket_origin + tempo_line_texts
CONTRACT   rust/crates/worklog-core/src/tempo_line_contract.rs — import from it.
NAMES      ticket origin = `blocks.ticket_origin`; ticket line table = `tempo_line_texts`
CALLS      `blocks.ticket_origin TEXT NULL CHECK(ticket_origin IN ('event','auto','manual'))`
           `tempo_line_texts(day TEXT NOT NULL, jira_issue TEXT NOT NULL, text TEXT NULL, text_origin TEXT NULL CHECK(text_origin IN ('generated','manual')), source_hash TEXT NULL, hours_override_seconds INTEGER NULL CHECK(hours_override_seconds IS NULL OR (hours_override_seconds > 0 AND hours_override_seconds % 1800 = 0)), updated_at TEXT NOT NULL, PRIMARY KEY(day, jira_issue))`
           add both to `sql/schema.sql`; add `ensure_blocks_ticket_origin` in `db.rs` (PRAGMA table_info pattern of `ensure_events_elsewhere`); bump `SCHEMA_VERSION` to 17. Existing rows keep `ticket_origin` NULL.
DUPLICATE  none
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T002 — Block.ticket_origin + manual on assign
CONTRACT   rust/crates/worklog-core/src/tempo_line_contract.rs — import from it.
NAMES      ticket origin = `TicketOrigin` / `Block.ticket_origin` (banned: ticket_source, assigned_by)
CALLS      `models::Block` gains `#[serde(default)] pub ticket_origin: Option<TicketOrigin>` (last field). Every struct literal adds `ticket_origin: None`. `repo.rs` block SELECTs read the column via `TicketOrigin::parse`.
           `block_service::assign_ticket(conn, block_id, key: Option<&str>) -> Result<Block>` — signature unchanged; both branches also `SET ticket_origin = 'manual'`.
DUPLICATE  none
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T003 — infer: event origin + manual lock
CONTRACT   rust/crates/worklog-core/src/tempo_line_contract.rs — `TicketOrigin`, `ticket_locked`.
CALLS      `infer::persist_blocks` (infer.rs:547) — the carried prior row now brings `(jira_issue, ticket_origin)` together. Rule per new block:
           prior `ticket_locked(origin)` → keep prior `jira_issue` (even NULL) and `Manual`;
           else prior has a ticket → keep it and its origin (today's `carry.jira_issue.or(..)`);
           else `unique_jira_issue` key → that key, `Event`; else NULL/NULL.
           INSERT writes `ticket_origin` via `as_str()`.
DUPLICATE  none
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T004 — estimator: auto origin + manual lock
CONTRACT   rust/crates/worklog-core/src/tempo_line_contract.rs — `TicketOrigin`.
CALLS      Every estimator write of `blocks.jira_issue` in `estimate.rs` (day pass ~:662 and the single-block path) becomes:
           `ticket_origin = 'manual'` → leave `jira_issue` and `ticket_origin` unchanged (description/minutes still written);
           else validated key `Some(k)` and `k` ≠ current → `jira_issue = k, ticket_origin = 'auto'`;
           `Some(k)` = current → unchanged; `None` → leave the current ticket and origin unchanged.
           Do it in the UPDATE's SQL (CASE on `ticket_origin`) so a race with `assign_ticket` can't win.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T005 — tempo_lines module
CONTRACT   rust/crates/worklog-core/src/tempo_line_contract.rs — `TempoLine`, `TempoLineKey`, `SetTempoLineText`, `SetTempoLineHours`, `HALF_HOUR_SECONDS`.
MODULE     rust/crates/worklog-core/src/tempo_lines.rs · may import: contract, clues_contract, billing (`union_seconds`, `block_interval`), collectors::tempo (`round_to_half_hour`, `summarize_descriptions`, `cap_description`), estimate (`ModelInvoker`), repo, rusqlite, anyhow
CALLS      `pub fn lines_for_day(conn: &Connection, day: &str) -> Result<Vec<TempoLine>>` — one per distinct non-empty `jira_issue` among the day's non-personal blocks, sorted by key
           `pub fn line_for(conn: &Connection, key: &TempoLineKey) -> Result<Option<TempoLine>>` — `None` when no block has that ticket that day
           `pub fn set_text(conn: &Connection, body: &SetTempoLineText) -> Result<Option<TempoLine>>` — manual; blank clears text
           `pub fn set_hours(conn: &Connection, body: &SetTempoLineHours) -> Result<Option<TempoLine>>` — validates per design §2
           `pub fn pending_generation(conn: &Connection, day: &str, force: Option<&TempoLineKey>) -> Result<Vec<(TempoLineKey, Vec<String>, String)>>` — (key, descriptions, source_hash) for lines with no text or a generated text whose hash changed; `force` = that one key even if manual
           `pub fn generate_text(invoker: Option<&dyn ModelInvoker>, key: &TempoLineKey, descriptions: &[String], model: &str) -> String` — `summarize_descriptions` + `cap_description`
           `pub fn commit_generated(conn: &Connection, key: &TempoLineKey, text: &str, source_hash: &str, force: bool) -> Result<()>` — never overwrites a manual row unless `force`
           set_text / set_hours / commit_generated set `dirty = 1` on that line's blocks whose `tempo_worklog_id` is non-empty. A row with text NULL and override NULL is deleted.
           In collectors/tempo.rs only: make `summarize_descriptions` and `cap_description` `pub(crate)`.
DUPLICATE  sibling of line_text.rs — do not generalise it (design §6)
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T006 — sync sends stored line
CONTRACT   rust/crates/worklog-core/src/tempo_line_contract.rs
CALLS      In `collectors/tempo.rs::sync_group_aggregated` (~:391): seconds = `tempo_lines::line_for(..).effective_seconds` (replacing the sum at ~:410); text = stored `text`, else `generate_text` with the sync's invoker then `commit_generated(.., force=false)` and send that. Not on dry run: dry run sends nothing and writes nothing, and uses `fallback_text` when text is `None`. Zero-seconds skip stays. `sync_block_legacy` unchanged.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T007 — daemon routes
CONTRACT   rust/crates/worklog-core/src/tempo_line_contract.rs
MODULE     rust/crates/worklog-core/src/daemon_tempo_lines.rs · handlers only, like daemon_line_text.rs
CALLS      `GET /tempo/lines/:day` → `Vec<TempoLine>`
           `POST /tempo/lines/text` body `SetTempoLineText` → `TempoLine` (404 if `None`)
           `POST /tempo/lines/hours` body `SetTempoLineHours` → `TempoLine` (400 invalid, 404 if `None`)
           `POST /tempo/lines/regenerate` body `TempoLineKey` → `TempoLine`: `pending_generation(force=Some)` under `with_conn`, `generate_text` outside it with `estimate::build_regenerate_invoker`, `commit_generated(force=true)` under `with_conn`
           `run_estimate` (daemon.rs ~:1783): after the billing line-texts pass, run the same three steps for `pending_generation(day, None)` (force=false); add `tempo_lines: <count generated>` to its JSON.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T008 — worklog day generates line texts
CALLS      In `worklog-cli/src/cli.rs` `cmd_day`, after the estimate step (~:3395): `tempo_lines::pending_generation(&conn, day, None)`, `generate_text` with `estimate::build_invoker()` (skip quietly on invoker error), `commit_generated(force=false)`. Output a one-line count with the existing `style::` helpers; on error `style::warn` and continue.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T009 — web client + actions
CONTRACT   web/lib/tempo_line_contract.ts — `TempoLine`, `TempoLineKey`.
CALLS      `web/lib/daemonTempoLines.ts`: `tempoLines(day): Promise<TempoLine[]>`, `setTempoLineText(key, text)`, `setTempoLineHours(key, seconds: number | null)`, `regenerateTempoLine(key)` — all via `call` from `lib/daemon.ts`.
           `web/app/actions-tempo-lines.ts` ("use server"): `saveTempoLineText`, `saveTempoLineHours`, `regenerateTempoLineText` — each wraps `run(fn, day)` like `actions-line-text.ts`.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T010 — auto tag on BlockCard
CONTRACT   web/lib/tempo_line_contract.ts — `TicketOrigin`, `isAutoTicket`.
CALLS      `web/lib/types.ts` `Block` gains `ticket_origin?: TicketOrigin | null` (one line). BlockCard renders a small "auto" tag beside the ticket when `isAutoTicket(block.jira_issue, block.ticket_origin)`.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T011 — TicketGroup line editing
CONTRACT   web/lib/tempo_line_contract.ts — `TempoLine`, `HALF_HOUR_SECONDS`.
CALLS      `app/[day]/page.tsx` loads `tempoLines(day)` and passes the matching `TempoLine` to each assigned TicketGroup (`line?: TempoLine`). TicketGroup shows `line.text ?? line.fallback_text` with edit (Cmd-Enter save, Esc cancel, empty resets) + regenerate, copied from BillingGroup; `OriginIcon`/`originLabel` exported from BillingGroup. Hours shown = `line.effective_seconds`; an hours input saves via `saveTempoLineHours` (empty = clear; non-half-hour = inline error). Unassigned group: no controls. Test-only prop overrides for the three actions, as BillingGroup does.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T012 — a failed model call is never stored
CONTRACT   spec.md A7: "A failed generation leaves the line on its fallback summary." Stored `text` is only ever a real model summary, a verbatim single description, "Work on {issue}", or the no-invoker joined text.
CALLS      `collectors/tempo.rs`: split `summarize_descriptions` — new `pub(crate) fn try_summarize_descriptions(invoker, issue, descriptions, model, dry_run) -> Option<String>` returns `None` ONLY when the invoker was called and returned `Err` or an empty/missing `description`; every other branch (empty → "Work on {issue}", single → verbatim capped, dry_run, no invoker → joined) returns `Some`. `summarize_descriptions` becomes `try_summarize_descriptions(..).unwrap_or_else(<joined fallback>)` with identical output to today.
           `tempo_lines::generate_text(..) -> Option<String>` (was `String`), built on `try_summarize_descriptions(.., false)`.
           Callers on `None`: daemon `generate_tempo_lines` (daemon_tempo_lines.rs) skips `commit_generated` for that line (not counted in `run_estimate`'s `tempo_lines`; regenerate route returns the line unchanged); `sync_group_aggregated` sends `line.fallback_text` and commits nothing; `cli.rs generate_line_texts` warns `line text generation failed for <issue>` and skips — delete `FailureRecordingInvoker`, it is no longer needed.
TESTS      existing `tempo_lines_test.rs` generate_text asserts wrap the same expected strings in `Some(..)` (no expected value changes); add: failing invoker → `None` (tempo_lines_test); daemon run with failing invoker stores no text (daemon_tempo_lines_test); sync with failing invoker sends the joined fallback and `tempo_line_texts` has no text for that line (collectors::tempo). `generate_line_texts_with_failing_model_stores_nothing_and_warns` must keep passing unchanged.
DUPLICATE  none
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T015 — the manual lock survives split and auto-merge
CONTRACT   spec FR-04: the scheduler MUST NOT change a manual-origin ticket. `TicketOrigin` from tempo_line_contract.rs.
CALLS      `block_service::split_block` (block_service.rs:360): the tail INSERT copies the source block's `ticket_origin` alongside `jira_issue`.
           `estimate::merge_same_ticket_adjacent` / `merge_block_into` (estimate.rs:968, :1026): when either merged block has `ticket_origin = 'manual'`, the surviving block's `ticket_origin` is set to `'manual'` (in SQL, in the same UPDATE/transaction as the merge).
           `block_service::merge_blocks` (user-initiated) is out of scope — unchanged.
TESTS      inline `#[cfg(test)]` in each file: split a manual block → tail origin `Manual`; auto-merge of a manual + auto adjacent pair on one ticket → survivor origin `Manual`; auto-merge of two auto blocks → origin unchanged.
DUPLICATE  none
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T016 — stored line text is capped when sent
CONTRACT   spec.md:106 NFR — description sent to Tempo ≤ 250 chars via the existing `cap_description` applied to stored text.
CALLS      `collectors/tempo.rs::sync_group_aggregated`: `Some(text) => cap_description(&text)` (stored text, manual or generated). Storage is not changed — the Owner's full text stays in `tempo_line_texts`.
TESTS      collectors::tempo: a stored manual line text of 300 chars syncs (dry run or httpmock) with a `description` equal to `cap_description` of it (length ≤ 250).
DUPLICATE  none
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.
