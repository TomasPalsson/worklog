# Design — spec 006

## 1. Contract file + language
- Rust: `rust/crates/worklog-core/src/clues_contract.rs` (draft: `.specs/006-block-clues-and-detail-panel/clues_contract.rs`, installed verbatim by T001). Web: the matching TS types go in `web/lib/types.ts` under a `// spec 006` block, written by T001.

| canonical | identifier | defined in | banned synonyms |
|---|---|---|---|
| raw record | `RawRecord` | clues_contract.rs | payload, blob, detail_json |
| helper activity | `SOURCE_CLAUDE_HELPER`, `HelperKind` | clues_contract.rs | subagent_event, child |
| session message | `SOURCE_CLAUDE_MESSAGE` | clues_contract.rs | teammate_msg |
| done elsewhere | `events.elsewhere` (INTEGER 0/1) | schema.sql | remote, foreign, offsite |
| scrub | `scrub::scrub_secrets(&str) -> String` | scrub.rs | redact (that name is `estimate::redact_code`, keep it) |
| description input | `DescriptionInput` | clues_contract.rs | prompt_payload, context |
| billing line | `BillingLineKey {day, folder, customer}` | clues_contract.rs | super block (UI copy only), group |
| line text | `billing_line_texts` table, `LineTextOrigin` | schema.sql / contract | invoice_desc |

- Times: UTC ISO-8601 strings, as everywhere in this repo. All new code sync (no async in worklog-core).

## 2. Trust boundaries
| boundary | untrusted shape | parse fn | failure |
|---|---|---|---|
| fish history / reflog / transcripts / hook stdin | free text | collector parser → `scrub_secrets` → `RawRecord` | per line: skip the line, never the file |
| `git cat-file -e <sha>` in a clone | exit status | `local_clone::sha_is_local(folder, sha) -> bool` | any error → `false` (elsewhere) |
| `events.raw_json` read back | JSON written by older code (may be NULL) | `serde_json::from_str::<RawRecord>` | NULL or parse error → row shown without detail |
| description writer output | model text | `line_text::validate(&str) -> Result<String, String>` | reject → keep previous text / fallback (FR-27) |

## 3. Error taxonomy
No new error type: daemon routes use the existing `ApiError` in daemon.rs (404 unknown block/line, 400 bad body). Adding a variant is an escalation.

## 4. Module boundaries
- `scrub.rs` · exports `scrub_secrets` · imports std/regex only.
- `local_clone.rs` · exports `sha_is_local`, `folder_for_repo(repo: &str) -> Option<String>` (reuses `billing::work_folder_for_path` + the submodule map — call, don't copy) .
- `clues_send.rs` · exports `build_block_input(conn, block_id) -> Result<DescriptionInput>`, `build_line_input(conn, &BillingLineKey) -> Result<DescriptionInput>` · the ONLY producer of anything sent off-machine.
- `line_text.rs` · exports `validate`, `generate_for_day(conn, day, &dyn Estimator)`, `set_manual(conn, &BillingLineKey, &str)`, `text_for(conn, &BillingLineKey) -> Option<(String, LineTextOrigin)>`.
- `block_details.rs` · exports `details_for_block(conn, block_id) -> Result<Vec<DetailRow>>` (events linked + helper/message rows in the block span).
- Anything not listed is a bug. New code never goes into daemon.rs/estimate.rs/billing.rs beyond the call-site lines each task names (files are far over the 400-line guard).

## 5. Shared resources
- DB handle: `&rusqlite::Connection` passed in; never opened inside a module.
- Schema: T001 owns `schema.sql` + `db.rs` migration steps for the whole spec (`events.elsewhere`, `billing_line_texts`); bump `SCHEMA_VERSION` once.
- Estimator: the existing `estimate.rs` `ClaudeSubprocess` / provider trait is reused for line texts; no second model client.

## 6. Deliberately duplicated
- none — because every shared helper already exists (`work_folder_for_path`, submodule map, `redact_code`) and is called, not re-implemented.

## 7. Decisions
- In the context of block building, facing `is_none_or` letting folderless events match any lane (infer_lanes.rs:105), we chose "folderless events are placed after lanes are built, only inside an existing block span" and rejected giving them a synthetic key, to achieve D-08, accepting a second pass over the day's events. Makes hard: infer_lanes.rs, infer.rs.
- In the context of helper activity, facing D-05 (no time), we chose separate `events.source` values excluded from `is_human` and from block bounds, and rejected a flag column, to achieve a compile-visible rule, accepting two new source strings in the UI label map. Makes hard: infer_lanes.rs, web/lib/eventRows.ts.
- In the context of billing lines being computed, not stored (billing.rs:570), we chose a `billing_line_texts` table keyed by `BillingLineKey`, and rejected storing text on blocks, to achieve D-12, accepting that a re-keyed line (customer change) loses its generated text (spec §8 Q1). Makes hard: billing.rs export text selection.
- In the context of the Details view, facing hundreds of rows per block, we chose a dedicated route `web/app/[day]/block/[id]/page.tsx`, and rejected an in-flow drawer, to achieve D-10/D-11, accepting one extra navigation. Makes hard: web routing.

## Complexity Tracking
| Violation | Why needed | Simpler alternative rejected because |
|---|---|---|

## Contract for T001 — Contract, schema and migration
CONTRACT   .specs/006-block-clues-and-detail-panel/clues_contract.rs → copy verbatim to rust/crates/worklog-core/src/clues_contract.rs; `pub mod clues_contract;` in lib.rs.
CALLS      schema.sql: `ALTER TABLE events ADD COLUMN elsewhere INTEGER NOT NULL DEFAULT 0`; `CREATE TABLE billing_line_texts(day TEXT NOT NULL, folder TEXT NOT NULL, customer TEXT NOT NULL DEFAULT '', text TEXT NOT NULL, origin TEXT NOT NULL CHECK(origin IN ('generated','manual')), updated_at TEXT NOT NULL, PRIMARY KEY(day, folder, customer))`; matching idempotent steps in db.rs `migrate`.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.
