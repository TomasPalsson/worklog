# Design — 027 super block regenerate with prompt

## 1. Contract files (orchestrator-owned; tasks import, never redeclare)

- Rust: `rust/crates/worklog-core/src/tempo_line_contract.rs` → `RedescribeTempoLineBody { day, jira_issue, instruction }`, `REASON_TEXT_CHANGED`; plus `TempoLineKey`, `TempoLine` (existing). Limits/reasons reused from `rust/crates/worklog-core/src/redescribe_contract.rs`: `MAX_INSTRUCTION_CHARS` (500), `REASON_EMPTY_INSTRUCTION`, `REASON_INSTRUCTION_TOO_LONG`.
- Web: `web/lib/tempo_line_contract.ts` → `RedescribeTempoLineBody`, `REASON_TEXT_CHANGED`, `TempoLineKey`, `TempoLine`; `web/lib/redescribe.ts` → `MAX_INSTRUCTION_CHARS`.

| canonical | identifier | defined in | banned synonyms |
|---|---|---|---|
| line text | `TempoLine.text` / `tempo_line_texts.text` | tempo_line_contract | group description, summary, comment |
| line key | `TempoLineKey { day, jira_issue }` | tempo_line_contract | group id, ticket key |
| Owner's text | `text_origin = 'manual'` (`LineTextOrigin::Manual`) | clues_contract | edited, custom |
| instruction | `instruction: String` | tempo_line_contract | prompt, hint, note |

Sync convention: core fns are sync and take `&Connection`; the daemon never holds the connection across the model call (prepare → spawn_blocking write → commit), as `daemon_tempo_lines::generate_tempo_lines_with` does.

## 2. Trust boundaries

| boundary | untrusted input | parse | failure |
|---|---|---|---|
| `POST /tempo/lines/redescribe` | JSON body | axum `Json<RedescribeTempoLineBody>`; trim instruction; char count | 400 per reason, nothing touched |
| model reply | JSON | `line_text::reply_to_text` + `line_text::validate`, ≤3 tries (`tempo_line_writer`) | 502 with the last reason |

## 3. Error taxonomy (route → status, body `{"error": <message>}` via `ApiError`)

| case | status | message |
|---|---|---|
| empty instruction after trim | 400 `BadRequest` | `REASON_EMPTY_INSTRUCTION` |
| > `MAX_INSTRUCTION_CHARS` chars | 400 `BadRequest` | `REASON_INSTRUCTION_TOO_LONG` |
| no Tempo line for the key (no live non-personal blocks on that ticket that day) | 404 `NotFound` | `no Tempo line for <ticket> on <day>` |
| model error or 3 failed checks | 502 `BadGateway` | the writer's reason |
| text changed while writing | 409 `Conflict` | `REASON_TEXT_CHANGED` |

## 5. Shared resources

Invoker: `estimate::build_regenerate_invoker(line_text::LINE_TEXT_THINKING_TOKENS)`, built inside `spawn_blocking`. Model: `line_text::LINE_TEXT_MODEL`. Route-test seam: a `#[cfg(test)] static TEST_REPLY` exactly as `daemon_redescribe.rs`.

## 6. Deliberately duplicated

- The 3-phase daemon flow is written again in the new route, not folded into `generate_tempo_lines_with` (that fn's `force` / auto-pass / Verdict-check behaviour must not change — spec §2.2).

## 7. Decisions

- In `tempo_line_writer`, facing a fixed system prompt, we chose `pub fn write_with_system(user_msg, key, invoker, model, system: &str)` with `write` delegating to it, and rejected a new writer module, to reuse ticket context + validate + 3 tries, accepting one extra pub fn. Makes hard: tempo_line_writer.rs.
- In the commit, facing a concurrent "Edit text" during the model call, we chose `UPDATE … WHERE day=?1 AND jira_issue=?2 AND text IS ?<stored_text at prepare>` and 0 rows → `REASON_TEXT_CHANGED`, rejected a version column (schema migration), accepting that an identical re-save is not detected. Makes hard: tempo_line_redescribe.rs.
- In the web, facing a second menu needing the instruction box, we chose moving `InstructionBox` (made generic via `onSubmit`) and the viewport clamp into their own files, rejected copying them, accepting a prefactor task. Makes hard: BlockContextMenu.tsx, GroupContextMenu.tsx.

## Constraints

- Instruction: 1–500 characters after trim, counted as Unicode chars (Rust `chars().count()`, TS `[...s].length`); Icelandic þ ð æ are one char each.
- Stored text origin after a regenerate is always `manual`; `source_hash` and `check_status` are set NULL.

## Contract for T001 — InstructionBox + menu clamp prefactor

USAGE      // in BlockContextMenu (unchanged behaviour) and later GroupContextMenu:
           <InstructionBox onSubmit={(i) => redescribeBlock(block.id, day, i)} close={close} onWriting={setWriting}
             successMessage="Description rewritten" failurePrefix="Rewrite failed" />
           const pos = useMenuClamp(menuRef, x, y, [boxOpen, splitOpen]);   // {left, top}
CONTRACT   web/lib/redescribe.ts (MAX_INSTRUCTION_CHARS); web/app/actions.ts (ActionResult)
MODULE     web/components/InstructionBox.tsx exports InstructionBox · web/components/useMenuClamp.ts exports useMenuClamp
CALLS      InstructionBox(props: { onSubmit: (instruction: string) => Promise<ActionResult<unknown>>; close: () => void;
             onWriting: (writing: boolean) => void; successMessage: string; failurePrefix: string })
           useMenuClamp(ref: RefObject<HTMLElement | null>, x: number, y: number, deps: unknown[]): { left: number; top: number }
LIMITS     behaviour of BlockContextMenu's box and clamp is byte-for-byte the same: trim, 500-char cap, Writing…, Esc while writing,
           try/catch/finally, toast "<successMessage>" / "<failurePrefix> — <error>", 8px viewport margin
THE FIVE   (see design reference — copy verbatim) (1) NEVER invent a type that exists in the contract. (2) NEVER narrow a boundary param.
           (3) NEVER add a mode/flag to a shared abstraction. (4) NEVER edit outside files:. (5) NEVER abbreviate identifiers.

## Contract for T002 — tempo writer + line store prefactor

USAGE      tempo_line_writer::write_with_system(&msg, &key, invoker, LINE_TEXT_MODEL, &system)
           tempo_lines::ensure_row(&tx, &key)?; … tempo_lines::finish_write(&tx, &key)?;
MODULE     rust/crates/worklog-core/src/tempo_line_writer.rs, tempo_lines.rs
CALLS      pub fn write_with_system(user_msg: &str, key: &TempoLineKey, invoker: &dyn ModelInvoker, model: &str, system: &str) -> Result<String, String>
           pub fn write(...) — unchanged signature, now `write_with_system(.., SYSTEM_PROMPT_IS)`
           pub(crate) fn ensure_row(conn: &Connection, key: &TempoLineKey) -> Result<()>
           pub(crate) fn finish_write(conn: &Connection, key: &TempoLineKey) -> Result<()>
LIMITS     no behaviour change: existing tempo_line_writer / tempo_lines / daemon_tempo_lines tests pass unedited
THE FIVE   as T001

## Contract for T003 — core: tempo line redescribe

USAGE      let prep = tempo_line_redescribe::prepare(conn, &key, &instruction)?.ok_or(no_line)?;   // under lock
           let text = tempo_line_redescribe::write(&prep, invoker.as_ref(), LINE_TEXT_MODEL)?;      // no connection
           let line = tempo_line_redescribe::commit(conn, &prep, &text)?;                          // under lock
CONTRACT   tempo_line_contract (TempoLineKey, TempoLine, REASON_TEXT_CHANGED)
MODULE     rust/crates/worklog-core/src/tempo_line_redescribe.rs (+ _test.rs via #[path]), `pub mod` in lib.rs
CALLS      pub struct LineRedescribePrep { pub key: TempoLineKey, pub instruction: String, pub current_text: String,
             pub stored_text: Option<String>, pub user_msg: String }
           pub fn prepare(conn: &Connection, key: &TempoLineKey, instruction: &str) -> anyhow::Result<Option<LineRedescribePrep>>
             // None when tempo_lines::line_for is None. current_text = line.text else line.fallback_text.
             // user_msg = tempo_line_writer::prepare JSON + "owner_instruction" + "current_text" keys.
           pub fn write(prep: &LineRedescribePrep, invoker: &dyn ModelInvoker, model: &str) -> Result<String, String>
             // write_with_system(&prep.user_msg, &prep.key, invoker, model, &(SYSTEM_PROMPT_IS + "\n\n" + INSTRUCTION_RULE_IS))
           pub fn commit(conn: &Connection, prep: &LineRedescribePrep, text: &str) -> anyhow::Result<Option<TempoLine>>
             // db::write_tx; ensure_row; guarded UPDATE (origin 'manual', source_hash NULL, check_status NULL, updated_at);
             // 0 rows → bail!(REASON_TEXT_CHANGED); finish_write; commit; return tempo_lines::line_for
LIMITS     instruction trimmed in prepare; validation of length is the route's (T004). No meeting shortcut, no Verdict check.
THE FIVE   as T001

## Contract for T004 — route POST /tempo/lines/redescribe

USAGE      curl -X POST :9323/tempo/lines/redescribe -d '{"day":"2026-10-16","jira_issue":"ABC-1","instruction":"…"}' → 200 TempoLine
CONTRACT   tempo_line_contract::{RedescribeTempoLineBody, TempoLine, REASON_TEXT_CHANGED}; redescribe_contract limits/reasons
MODULE     rust/crates/worklog-core/src/daemon_tempo_line_redescribe.rs (child of daemon.rs via #[path], like daemon_redescribe.rs)
CALLS      pub async fn redescribe(State(state): State<Shared>, Json(body): Json<RedescribeTempoLineBody>) -> Result<Json<TempoLine>, ApiError>
LIMITS     §3 error taxonomy exactly; char-count validation before any DB access
THE FIVE   as T001

## Contract for T005 — web client + server action

USAGE      const r = await redescribeTempoLineText({ day, jira_issue }, instruction);  // ActionResult<TempoLine>
CONTRACT   web/lib/tempo_line_contract.ts (RedescribeTempoLineBody, TempoLineKey, TempoLine)
MODULE     web/lib/daemonTempoLines.ts adds redescribeTempoLine; web/app/actions-tempo-lines.ts adds redescribeTempoLineText
CALLS      redescribeTempoLine(body: RedescribeTempoLineBody): Promise<TempoLine>   // POST /tempo/lines/redescribe
           redescribeTempoLineText(key: TempoLineKey, instruction: string): Promise<ActionResult<TempoLine>>  // via the file's run(), revalidates the day
THE FIVE   as T001

## Contract for T006 — group menu on the super block header

USAGE      // TicketGroup <summary>: right-click / ContextMenu / Shift+F10 → <GroupContextMenu lineKey={lineKey} x y close />
CONTRACT   web/lib/tempo_line_contract.ts; T001 InstructionBox + useMenuClamp; T005 redescribeTempoLineText;
           useBlockContextMenu (web/components/BlockContextMenu.tsx) for open/close + keyboard; useMenuDismiss; menuKeys
MODULE     web/components/GroupContextMenu.tsx exports GroupContextMenu; wired in web/components/TicketGroup.tsx
CALLS      GroupContextMenu(props: { lineKey: TempoLineKey | undefined; x: number; y: number; close: () => void })
LIMITS     exactly one menuitem "Regenerate with prompt…"; disabled + title "Pick a ticket first" when lineKey is undefined;
           success toast "Text rewritten", failure "Rewrite failed — <error>"; TicketGroup's existing summaryRef focus
           behaviour and left-click toggle unchanged; TicketGroup.tsx stays ≤ 400 lines
THE FIVE   as T001
