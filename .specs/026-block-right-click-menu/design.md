# Design — Right-click menu on blocks

Seam: T003 (core), T004 (route) and T005 (web) share the redescribe request, result and error reasons; T002, T005 and T006 share the menu component and the new web action file.

## 1. Contract file + language

- Rust: `rust/crates/worklog-core/src/redescribe_contract.rs` (written, compiles). TS mirror: `web/lib/redescribe.ts`. Tasks import; neither is edited by a task.

| canonical | identifier | defined in | banned synonyms |
|---|---|---|---|
| instruction | `instruction` | contract | prompt, hint, guidance, userPrompt |
| redescribe | `redescribe` (route, module, fn) | contract | regenerate (taken by note/line jobs), rewrite, reprompt |
| block menu | `BlockContextMenu` | T002 | RightClickMenu, BlockMenu |
| instruction cap | `MAX_INSTRUCTION_CHARS` = 500 | contract | — |

Sync convention: `POST /blocks/:id/redescribe` is synchronous like `POST /blocks/:id/estimate` (no job, no poll).

## 2. Trust boundaries

| boundary | untrusted input | parse fn | failure |
|---|---|---|---|
| HTTP body → daemon | `{instruction}` | `RedescribeBody` + trim + char count in `daemon_redescribe.rs` | whole request, 400 with a contract reason |
| model reply → DB | `{description}` JSON | serde into a private reply struct in `redescribe.rs` | whole request, 502-style `ApiError` as today's estimate; DB untouched |
| block row → prompt | description, rough_note, is_personal | `redescribe::prepare` | 400 with contract reason |

## 3. Error taxonomy

Reasons are the `REASON_*` consts in the contract; the route returns them as `ApiError::bad_request` (400) bodies. Missing block → 404 (same as estimate_block). Model failure → the same error mapping `estimate_block` uses today. Order of checks: missing (404) → instruction empty → instruction too long → personal → note block → no description.

## 4. Module boundaries

- `redescribe.rs` (core) · may import: `redescribe_contract`, `clues_send::build_block_input`, `estimate::{ModelInvoker, resolve_provider-style factory}`, `undo`, `block_service::MARK_DIRTY_IF_SYNCED`, `change_log` · exports: `prepare`, `invoke`, `commit`, `redescribe_block_with`.
- `daemon_redescribe.rs` · may import: `redescribe`, `redescribe_contract`, daemon `Shared`/`with_conn`/`ApiError` · exports: `redescribe` handler. `daemon.rs` gets only `mod daemon_redescribe;`, one `.route(...)` line and the `include!` test module line.
- `web/lib/daemonBlockMenu.ts` (client) → `web/app/actions-block-menu.ts` (server actions) → `web/components/BlockContextMenu.tsx`. `app/actions.ts` is not edited (its module mock is shared by 9 test files).

## 5. Shared resources

| resource | constructed by | passed how | received by |
|---|---|---|---|
| DB connection | daemon `with_conn` | `&Connection` | `redescribe::prepare/commit` |
| model invoker | `estimate::build_invoker` (same provider as Describe) | `&impl ModelInvoker` | `redescribe::invoke` |
| test fake model | `estimate::FixedInvoker` / route `TEST_REPLY` pattern from `daemon_note_block.rs:23-31` | static under `#[cfg(test)]` | route tests |

## 6. Deliberately duplicated

- The prompt for redescribe is its own const in `redescribe.rs`; do not add a mode or flag to `estimate::SYSTEM_PROMPT` or `prepare_block_estimate` (D-01: estimate writes hours and ticket, redescribe never does).
- The 500-char output cut copies the note writer's behaviour inline; do not change `note_writer`.

## 7. Decisions

- In the context of the redescribe write, facing D-01 (description only), we chose a separate `commit` that runs `UPDATE blocks SET description=?, dirty={MARK_DIRTY_IF_SYNCED}` inside an `undo::record(BlockChange::Text)` transaction, and rejected reusing `estimate::commit_block_estimate`, to keep minutes/ticket/estimated_by untouched, accepting a second small write path. Makes hard: `redescribe.rs`.
- In the context of web actions, facing a process-wide `@/app/actions` mock in 9 test files, we chose a new `actions-block-menu.ts`, rejected adding to `actions.ts`, accepting one more actions file. Makes hard: `actions-block-menu.ts`, its two test mocks.

## Complexity Tracking

| Violation | Why needed | Simpler alternative rejected because |
|-----------|-----------|--------------------------------------|

## Constraints

- Instruction: 1–500 chars after trim (`MAX_INSTRUCTION_CHARS`).
- Rewritten description: ≤ 500 chars.
- Never write `tempo_worklog_id`, `exported_at`, `duration_seconds`, `jira_issue`, `ticket_origin` or `estimated_by` in the redescribe path.

## Contract for T003 — Core redescribe

USAGE      `let r = redescribe::redescribe_block_with(&conn, block_id, "this was the login bug fix", &FixedInvoker(json!({"description":"Fixed the login bug"})), "test-model")?;` → `RedescribeResult { block_id, description }`
CONTRACT   rust/crates/worklog-core/src/redescribe_contract.rs — import from it.
NAMES      instruction (banned: prompt, hint) · redescribe (banned: regenerate, rewrite)
MODULE     src/redescribe.rs · exports: prepare, invoke, commit, redescribe_block_with
CALLS      `pub fn prepare(conn:&Connection, block_id:i64, instruction:&str) -> Result<RedescribePrep>` (bails with contract REASON_* on personal / note / empty description; instruction already validated by caller) · `pub fn invoke<I: ModelInvoker>(prep:&RedescribePrep, invoker:&I, model:&str) -> Result<String>` · `pub fn commit(conn:&Connection, prep:&RedescribePrep, description:&str) -> Result<RedescribeResult>`
LIMITS     description output ≤ 500 chars, empty reply is an error (spec §5)
DUPLICATE  own system prompt const; do not touch estimate.rs prompts
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T004 — Redescribe route

USAGE      `POST /blocks/42/redescribe {"instruction":"this was the login bug fix"}` → 200 `{"block_id":42,"description":"Fixed the login bug"}`; `{"instruction":"  "}` → 400 `instruction is empty`
CONTRACT   rust/crates/worklog-core/src/redescribe_contract.rs — import from it.
NAMES      route `/blocks/:id/redescribe` · handler `redescribe`
MODULE     src/daemon_redescribe.rs · exports: redescribe handler; daemon.rs gets wiring lines only
CALLS      `pub async fn redescribe(State(state): State<Shared>, AxumPath(id): AxumPath<i64>, Json(body): Json<RedescribeBody>) -> Result<Json<RedescribeResult>, ApiError>` — validate instruction (trim, 1..=MAX_INSTRUCTION_CHARS chars), `with_conn(prepare)`, `spawn_blocking(invoke)` with no lock held, `with_conn(commit)`
LIMITS     instruction 1–500 chars after trim → else 400 REASON_EMPTY_INSTRUCTION / REASON_INSTRUCTION_TOO_LONG
DUPLICATE  copy the `TEST_REPLY` fake-model pattern from daemon_note_block.rs:23-31 into this file
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T005 — Web regenerate-with-prompt

USAGE      `const r = await redescribeBlock(block.id, day, instruction); if (r.ok) toast.ok("Description rewritten"); else toast.error(\`Rewrite failed — ${r.error}\`);`
CONTRACT   web/lib/redescribe.ts — import from it.
NAMES      instruction · redescribeBlock (action) · redescribe (client fn in web/lib/daemonBlockMenu.ts)
MODULE     web/lib/daemonBlockMenu.ts `redescribe(id:number, instruction:string): Promise<RedescribeResult>` → `call("POST", \`/blocks/${id}/redescribe\`, { instruction })`; web/app/actions-block-menu.ts `"use server"` `redescribeBlock(blockId:number, day:string, instruction:string): Promise<ActionResult<RedescribeResult>>` revalidating `/${day}`
LIMITS     Submit disabled when trimmed instruction is empty; input `maxLength={MAX_INSTRUCTION_CHARS}`
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.
