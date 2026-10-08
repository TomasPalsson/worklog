# Design — note blocks (spec 019)

## 1. Contract files + language

- Rust: `rust/crates/worklog-core/src/note_block_contract.rs` — `use crate::note_block_contract::*`.
- Web: `web/lib/noteBlock.ts` — `import { … } from "@/lib/noteBlock"`.
- Both are orchestrator-owned. A type you need that is not there is an escalation.

| canonical | identifier | plural | defined in | banned synonyms |
|---|---|---|---|---|
| rough note | `rough_note` (column, JSON field), `note` (request field) | — | schema.sql / contract | memo, raw_text, summary |
| description origin | `description_origin`, `DescriptionOrigin` | — | contract | source, author, text_origin |
| note block | a block with `rough_note IS NOT NULL` | note blocks | spec glossary | quick block, manual entry |
| note job | background AI write for one block, keyed by block id `i64` | note jobs | §5 | task, worker |
| note writer | `note_writer.rs` prepare/invoke/commit | — | §4 | describer, estimator |

- Times: ISO-8601 UTC in DB (as `log_time` writes them). `start` in requests is local HH:MM under `WORKLOG_TZ`.
- Async: route handlers are `async`; core functions are sync and take `&Connection`.

## 2. Trust boundaries

| boundary | untrusted input | parse fn | failure |
|---|---|---|---|
| `POST /blocks/note` | `NoteBlockBody` JSON | `ticket_log::log_time` (day, start, minutes, note length, compressed day) + key check via existing `validated_key` (daemon_tasks.rs) | 400 via existing `hub_error`, whole request |
| model reply | JSON `{description}` | `note_writer::invoke_note` checks non-empty string ≤ 500 chars after trim | job `failed` with the reason; description untouched |
| `blocks` row read | `description_origin` text | `DescriptionOrigin::parse` | unknown → treat as `Hand` (never overwrite) |

## 3. Error taxonomy

Request errors reuse `HubError` → `hub_error` (no new variant). Job errors are strings in the status body: `REASON_HAND_EDITED`, `REASON_NOT_A_NOTE_BLOCK`, or the invoker's own message. Status JSON is exactly the line-text shape: `{"state":"idle"|"running"|"done"|"failed","reason"?:string}`.

## 4. Module boundaries

- `note_writer.rs` (core, new) · may import: `note_block_contract`, `ticket_log`, `estimate::ModelInvoker`, `block_service::MARK_DIRTY_IF_SYNCED`, `change_log`, `repo`, `models` · exports: `log_note_block`, `prepare_note`, `invoke_note`, `commit_note`, `NotePrep`.
- `daemon_note_block.rs` (daemon child module via `#[path]`, new) · may import: `note_writer`, `note_block_contract`, `estimate::build_regenerate_invoker`, `super::{with_conn, ApiError, Shared}` · exports: `log_note`, `regenerate`, `status`.
- Web `lib/daemonNoteBlock.ts` → `app/actions-note-block.ts` → components. Components never import `lib/daemon*` directly.

Signatures:

```rust
pub fn log_note_block(conn: &Connection, body: &NoteBlockBody, today: NaiveDate) -> Result<Block>;
pub struct NotePrep { pub block_id: i64, pub note: String, pub jira_issue: String, pub ticket_summary: Option<String>, pub minutes: i64 }
pub fn prepare_note(conn: &Connection, block_id: i64) -> std::result::Result<NotePrep, String>;
pub fn invoke_note(prep: &NotePrep, invoker: &dyn ModelInvoker, model: &str) -> std::result::Result<String, String>;
pub fn commit_note(conn: &Connection, block_id: i64, text: &str, force: bool) -> std::result::Result<(), String>;
```

`commit_note` is one guarded UPDATE: `SET description=?, description_origin='ai', dirty=<MARK_DIRTY_IF_SYNCED> WHERE id=? AND rough_note IS NOT NULL AND (?force OR description_origin IN ('note','ai'))`; 0 rows → `Err(REASON_HAND_EDITED)` (or `REASON_NOT_A_NOTE_BLOCK` when `rough_note` is NULL). Then `change_log::refresh_day_logged(.., ChangeSource::Claude)`. It does NOT touch `estimated_by` (stays `'manual'`, so estimate/infer keep skipping the block).

`block_service::set_description` gains one SQL fragment: `description_origin = CASE WHEN rough_note IS NOT NULL THEN 'hand' ELSE description_origin END`.

Routes (registered in `daemon.rs` `router()` next to `/blocks/:id/estimate`):

| route | body | reply |
|---|---|---|
| `POST /blocks/note` | `NoteBlockBody` | `Block` JSON; starts a note job for the new id |
| `POST /blocks/:id/note/regenerate` | `RegenerateNoteBody` | `{"started":bool,"reason"?}` |
| `GET /blocks/:id/note/status` | — | status JSON (§3) |

## 5. Shared resources

| resource | constructed by | passed how | received by |
|---|---|---|---|
| sqlite conn | daemon `AppState` | `with_conn` (never held across the model call) | `daemon_note_block` |
| job tracker | `AppState.note_jobs: JobTracker<i64>` | `state.note_jobs` | `daemon_note_block` |
| model invoker | `estimate::build_regenerate_invoker(NOTE_THINKING_TOKENS)` inside `spawn_blocking` | arg to `invoke_note` | tests pass `estimate::FixedInvoker` |

`JobTracker` becomes `JobTracker<K = BillingLineKey>` with a hand-written `Default` (derive would demand `K: Default`); existing callers compile unchanged. Migration numbering: T001 owns the `SCHEMA_VERSION` bump (20 → 21).

## 6. Deliberately duplicated

- The poll loop (1 s, 30 s cap, `router.refresh()` on done) lives in one hook `web/components/useNoteJob.ts`; do not reuse `BillingGroup`'s `pollUntilSettled` — it is keyed by billing line.
- The note prompt is its own const in `note_writer.rs`; do not extend `estimate::SYSTEM_PROMPT` (it picks tickets and minutes).
- Ticket field: a native `<input list>` + `<datalist>` of the page's open tickets; do not add a picker mode to `TicketCombobox` (it assigns on pick).

## 7. Decisions

- In the context of the blocks table, facing "manual" already meaning both hand-logged and hand-edited, we chose two new nullable columns `rough_note`, `description_origin` and rejected reusing `estimated_by`, to let the AI write without ever clobbering a hand edit, accepting a schema bump. Makes hard: schema.sql, db.rs, models.rs, repo.rs.
- In the context of the regenerate control, facing an existing Sparkles button that calls the event-based estimator, we chose to route Sparkles to the note regenerate on note blocks and rejected a second button, accepting that Sparkles means two things. Makes hard: BlockCard.tsx, group-actions.ts.

## Complexity Tracking

| Violation | Why needed | Simpler alternative rejected because |
|---|---|---|
| Generic `JobTracker<K>` | note jobs are keyed by block id | a copy of the tracker duplicates 40 lines of lock logic |

## Contract for T001 — Note columns on blocks
CONTRACT   rust/crates/worklog-core/src/note_block_contract.rs — import from it. A type you need that is not there is an escalation, never a local declaration.
NAMES      rough_note · description_origin · DescriptionOrigin (banned: memo, raw_text, source, text_origin)
MODULE     schema.sql, db.rs, models.rs, repo.rs · core layer
CALLS      add `rough_note TEXT` and `description_origin TEXT CHECK (description_origin IN ('note','ai','hand'))` to `blocks` in sql/schema.sql; `ensure_blocks_note_columns(conn)` in db.rs via `add_columns_if_missing`, registered in `migrate()`; SCHEMA_VERSION 20 → 21 and the db_test assertion; `Block { #[serde(default)] pub rough_note: Option<String>, #[serde(default)] pub description_origin: Option<DescriptionOrigin> }`; every `SELECT … FROM blocks` row mapper in repo.rs reads both.
DUPLICATE  none
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T002 — Note writer core
CONTRACT   rust/crates/worklog-core/src/note_block_contract.rs — import from it.
NAMES      rough_note · description_origin · note writer · NotePrep
MODULE     src/note_writer.rs (+ note_writer_test.rs via `#[path]`) · core · may import: note_block_contract, ticket_log, estimate::ModelInvoker, block_service::MARK_DIRTY_IF_SYNCED, change_log, repo, models
CALLS      the five signatures in §4 verbatim. `log_note_block` = `ticket_log::log_time(conn, &body.jira_issue, &LogTimeBody{day,start,minutes,description: note}, today)` after refusing (InvalidInput "ends after midnight") when local start + minutes passes 24:00, then `UPDATE blocks SET rough_note=?, description_origin='note' WHERE id=?`. `invoke_note` rejects an empty or >500-char reply (FR-14). Prompt: one sentence-to-three-sentence past-tense work description for a timesheet, from the note, ticket key + summary (from `jira_tickets` if cached) and minutes; no invented facts; reply schema `{description: string}`. Plus the `set_description` CASE fragment in block_service.rs (§4).
DUPLICATE  the note prompt is its own const — do not touch estimate::SYSTEM_PROMPT
THE FIVE   (1)–(5) as in T001.

## Contract for T003 — Daemon routes and note jobs
CONTRACT   rust/crates/worklog-core/src/note_block_contract.rs — import from it.
NAMES      note job · note_jobs · REASON_HAND_EDITED · REASON_NOT_A_NOTE_BLOCK
MODULE     src/daemon_note_block.rs (+ daemon_note_block_test.rs) · daemon child via `#[path]` like daemon_line_text.rs · may import: note_writer, note_block_contract, estimate::build_regenerate_invoker, super::{with_conn, ApiError, Shared}
CALLS      the three routes in §4; `run_note_job(state, block_id, force) -> Result<(), String>` mirrors `daemon_line_text::run_line_text_job` (prepare in with_conn → spawn_blocking invoke → commit in with_conn). `log_note` runs `validated_key(&body.jira_issue)` first, returns the Block and always tries to start a job (force=false). Status JSON exactly §3. `JobTracker<K = BillingLineKey>` generic in line_text_jobs.rs; `AppState.note_jobs: JobTracker<i64>`.
DUPLICATE  none
THE FIVE   (1)–(5) as in T001.

## Contract for T004 — Web client and actions
CONTRACT   web/lib/noteBlock.ts — import from it.
NAMES      NoteBlockBody · NoteJobStatus · RegenerateNoteResult · NoteFields
MODULE     web/lib/daemonNoteBlock.ts (uses `call` from ./daemon) · web/app/actions-note-block.ts ("use server"; same `run` + ActionResult + revalidatePath(`/${day}`) shape as actions-line-text.ts; the status action does NOT revalidate)
CALLS      `logNoteBlock(body): Promise<RawBlock>` · `regenerateNote(id, force): Promise<RegenerateNoteResult>` · `noteStatus(id): Promise<NoteJobStatus>`; actions `addNoteBlock(body)`, `regenerateNoteAction(id, day, force)`, `noteStatusAction(id)`.
DUPLICATE  none
THE FIVE   (1)–(5) as in T001.

## Contract for T005 — "+" form on the day page
CONTRACT   web/lib/noteBlock.ts — import from it.
NAMES      NOTE_MAX_CHARS · NOTE_MAX_MINUTES · TICKET_KEY_RE · NOTE_POLL_MS · NOTE_POLL_MAX_MS
MODULE     web/components/AddNoteBlock.tsx (+ .test.tsx), web/components/useNoteJob.ts (+ .test.ts) · mounted from ActionBar.tsx; page.tsx passes `tickets` and `lastEnd` (latest block `ended_at` of the day, or null)
CALLS      `useNoteJob(day): { track(blockId: number): void; running: Set<number> }` — polls `noteStatusAction` every NOTE_POLL_MS, stops at NOTE_POLL_MAX_MS, `router.refresh()` on done, toast on failed.
DUPLICATE  poll loop lives only in useNoteJob.ts; ticket field is `<input list>` + `<datalist>`
THE FIVE   (1)–(5) as in T001.

## Contract for T006 — Regenerate on note blocks
CONTRACT   web/lib/noteBlock.ts — import from it.
NAMES      description_origin · NoteFields
MODULE     web/lib/group-actions.ts (`shouldShowSparkles` → true when `rough_note`), web/components/useNoteRegenerate.ts (+ test), BlockCard.tsx (net ≤ +1 line; file is at 399 of 400)
CALLS      `useNoteRegenerate(block: Block & NoteFields, day): () => void` — when origin is "hand" asks "Replace your edit?" (inline confirm, no `window.confirm`), calls `regenerateNoteAction(id, day, force)`, then `useNoteJob(day).track(id)`. BlockCard's Sparkles click uses it when `block.rough_note`, else the existing `describeBlock`. "Writing…" placeholder while running.
DUPLICATE  none
THE FIVE   (1)–(5) as in T001.
