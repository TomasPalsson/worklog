# Design — Billing deildir and super blocks

Seams only. The spec says what; this says what two tasks must agree on.

## 1. Contract files + language

- Rust: `rust/crates/worklog-core/src/deild_contract.rs` (source: `contracts/deild_contract.rs`, copied verbatim by T001).
- Web: `web/lib/deildir.ts` (source: `contracts/deildir.ts`, copied verbatim by T001).
- Both typecheck standalone (checked 2026-09-26). Task agents import; only the orchestrator edits.

| canonical | identifier | plural | defined in | banned synonyms |
|---|---|---|---|---|
| deild | `Deild` / `deild` | `deildir` | deild_contract | department, dept, verkefni (in new identifiers), project |
| split row | `ShareRow` | `rows` | deild_contract | share, part, allocation |
| block split | `BlockShares` | — | deild_contract | CustomerShares (v1, read-only) |
| billing slice | `BillingSlice` | `slices` | deild_contract | CustomerSlice (v1), portion |
| change | `BlockChange` | `changes` | deild_contract | event, notification, diff |
| source | `ChangeSource` | — | deild_contract | origin, author, actor |
| batch | `batch: String` | `batches` | deild_contract | run, job |

Times are ISO-8601 UTC strings in the DB (CLAUDE.md). Interval ends are epoch seconds. All Rust DB code is sync on `&Connection`; daemon handlers wrap it in the existing `with_conn`.

## 2. Trust boundaries

| boundary | untrusted input | parse fn | failure |
|---|---|---|---|
| `POST /billing/deildir` body | `Deild` JSON | serde + `billing_deildir::upsert_deild` (trims, rejects empty name, duplicate) | 400, whole request |
| `POST /blocks/:id/customer-shares` body | `{ rows: ShareRow[] }` | `tenant_shares::validate_rows` (customer exists, fractions in (0,1], sum 1±`SHARE_TOLERANCE`) | 400, whole request |
| `block_customer_shares` DB row | `rows_json` or v1 `shares` JSON | `tenant_shares::parse_rows` (v1 map → rows with `deild: None`) | row ignored + `warn!` |
| `block_resolution_snapshots.parts_json` | JSON | `change_log` serde | treat as no snapshot (no change logged) |
| `GET /changes?after=` | query int | axum `Query<i64>` default 0 | 400 |

## 3. Error taxonomy

No new variants. Validation → `ApiError::bad_request(msg)` (400, body shape as today); missing block → `ApiError::NotFound`. Web actions surface both through the existing `ActionResult` `{ ok:false, error }`.

## 4. Module boundaries

- `deild_contract` · layer 0 · may import: serde, `tenant_contract` · exports: the contract types.
- `billing_deildir` · layer 1 · may import: contract, rusqlite, `billing_registry::alias_matches` · exports: `list_deildir, upsert_deild, delete_deild, deild_in_text`.
- `tenant_shares` · layer 1 · may import: contract, `tenant_contract`, `billing_registry` · exports: `load_rows, save_rows, clear_shares, validate_rows, parse_rows, slices_from_rows`.
- `billing` · layer 2 · may import: layer 0–1 · exports adds: `resolve_block_slices`.
- `change_log` · layer 3 · may import: contract, `billing::resolve_block_slices`, `repo` · exports: `new_batch, refresh_day, feed, unseen, mark_seen, purge_old`.
- writers (`estimate`, `infer`, `routing`, `block_service`, `daemon*`) call `change_log::refresh_day` after their write; nothing else calls it. Anything not listed is a bug.

## 5. Shared resources

- DB handle: the caller's `&Connection`; never open a second one.
- Migration numbering: T001 owns `SCHEMA_VERSION` 13 → 14 and every new table/column. No other task edits `schema.sql` or `db.rs`.
- Clock: `Utc::now()` inside `change_log` only.
- Web polling: one interval in `ChangeNotices`, period `LIVE_POLL_SECONDS`.

## 6. Deliberately duplicated

- Keyword matching reuses `billing_registry::alias_matches`; do not write a second matcher, and do not move it.
- The (customer, deild) display string `"Sjúkra·Rekstur"` is built separately in Rust (`change_log`) and TS (`BlockCustomerSplit`); no shared formatter.

## 7. Decisions

- In the context of `change_log`, facing resolution computed at read time (spec A3), we chose a stored `ResolutionSnapshot` per (day, started_at) diffed by `refresh_day(conn, day, source, batch) -> Result<usize>` after each write, and rejected each writer emitting its own change events, to catch indirect moves (a description rewrite that changes the customer), accepting one extra resolve pass per written day. Makes hard: `change_log.rs`, every writer call site.
- In the context of live notices, facing a web app with no push channel (server components, no SSE), we chose polling `GET /changes?after=<cursor>` every `LIVE_POLL_SECONDS`, and rejected SSE/websockets, to keep one transport, accepting ≤10 s latency. Makes hard: `ChangeNotices.tsx`, `daemon.rs` routes.
- In the context of splits, facing v1 `{customer: fraction}` rows already saved, we chose a new nullable `rows_json` column read in preference and v1 `shares` converted on read, and rejected rewriting v1 rows in the migration, to keep the upgrade reversible. Makes hard: `tenant_shares.rs`.
- In the context of grouping, facing D-03, we chose key `(customer, deild)` when deild is set else `(customer, folder)`, dropping the ticket from the key. Makes hard: `billing.rs` rows_for_day.

## Complexity Tracking

| Violation | Why needed | Simpler alternative rejected because |
|---|---|---|
| Snapshot table | Customer/deild are never stored, so there is no "old value" to diff | Writer-emitted events miss indirect moves (FR-09) |

## Contract for T001 — Contract files and schema v14
CONTRACT   copy contracts/deild_contract.rs → rust/crates/worklog-core/src/deild_contract.rs and contracts/deildir.ts → web/lib/deildir.ts, byte-for-byte.
MODULE     lib.rs adds `pub mod deild_contract; pub mod billing_deildir; pub mod change_log;` (the latter two as doc-comment-only files).
CALLS      schema.sql: `billing_deildir(id INTEGER PRIMARY KEY, customer TEXT NOT NULL, name TEXT NOT NULL, keywords TEXT NOT NULL DEFAULT '', created_at TEXT NOT NULL, UNIQUE(customer, name))`; `block_customer_shares` + `rows_json TEXT` (nullable, via checked ALTER like `ensure_billing_folder_map_multi_tenant`); `block_resolution_snapshots(day TEXT, started_at TEXT, description TEXT, parts_json TEXT NOT NULL, PRIMARY KEY(day, started_at))`; `block_changes(id INTEGER PRIMARY KEY, day TEXT, started_at TEXT, field TEXT, old_value TEXT, new_value TEXT, source TEXT, batch TEXT, created_at TEXT, seen_at TEXT)`. `SCHEMA_VERSION = 14`. FR-14 seed: once, for each folder pin with customer and verkefni set, insert (customer, verkefni) into billing_deildir if absent.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T002 — Deild registry
CONTRACT   rust/crates/worklog-core/src/deild_contract.rs — import `Deild`.
CALLS      `list_deildir(conn:&Connection) -> Result<Vec<Deild>>`; `upsert_deild(conn, d:&Deild) -> Result<i64>` (bail on empty name or duplicate (customer,name) with a message naming both); `delete_deild(conn, id:i64) -> Result<bool>`; `deild_in_text(deildir:&[Deild], customer:&str, text:&str) -> Option<String>` — exactly one of that customer's deildir has a keyword matching via `billing_registry::alias_matches`, else None. Keywords stored newline-separated like aliases.
THE FIVE   as T001.

## Contract for T003 — Deild daemon routes
CALLS      `POST /billing/deildir` (Deild) → `{ "id": i64 }`; `POST /billing/deildir/:id/delete` → `{ "removed": bool }`; `GET /billing/registry` gains `deildir: Vec<Deild>`. Registry writes (customers, folders, deildir) end with `change_log::refresh_day` for today and yesterday, source `Keyword` — added later by T011, not here.
THE FIVE   as T001.

## Contract for T004 — Deildir editor (web)
CONTRACT   web/lib/deildir.ts — import `Deild`.
CALLS      server actions `saveDeild(d: Deild): Promise<ActionResult<{id:number}>>`, `deleteDeild(id:number): Promise<ActionResult<{removed:boolean}>>`; `BillingRegistry.deildir: Deild[]` in web/lib/types.ts. UI: under each customer row, its deildir with name + keywords inputs, add and delete.
THE FIVE   as T001.

## Contract for T005 — Split storage v2
CONTRACT   deild_contract — `ShareRow`, `BlockShares`, `SHARE_TOLERANCE`, `BillingSlice`.
CALLS      `load_rows(conn, day, started_at) -> Result<Option<BlockShares>>`; `save_rows(conn, s:&BlockShares, registry:&Registry) -> Result<()>` (writes `rows_json`, leaves v1 `shares` as `'{}'`); `validate_rows(rows:&[ShareRow], registry:&Registry) -> Result<()>`; `parse_rows(rows_json:Option<&str>, shares_json:&str) -> Vec<ShareRow>`; `slices_from_rows(start:i64, end:i64, rows:&[ShareRow]) -> Vec<BillingSlice>` (origin Manual, deild_origin Manual, ordered by (customer, deild), last absorbs rounding). Keep the v1 functions until T007 stops calling them.
THE FIVE   as T001.

## Contract for T006 — Resolution and grouping
CALLS      `pub fn resolve_block_slices(conn, block:&Block, folder:&str, registry:&Registry, deildir:&[Deild]) -> Result<Vec<BillingSlice>>`: manual rows win outright; else today's customer slices (tenant clues / fallback) each get a deild by `deild_in_text` then the folder pin's verkefni when the slice customer equals the pin's customer, else None. `rows_for_day` groups by `(customer, deild)` if deild set else `(customer, folder)`; `BillingRow.verkefni` = the deild; `BillingRow.ticket` = the shared ticket or None when mixed.
THE FIVE   as T001.

## Contract for T007 — Split routes v2
CALLS      `GET /blocks/:id/customer-slices` → `Vec<BillingSlice>` via `resolve_block_slices`, for every non-personal block (no multi_tenant check). `POST /blocks/:id/customer-shares` body `{ rows: ShareRow[] }` → `save_rows`. `/clear` unchanged.
THE FIVE   as T001.

## Contract for T008 — Split editor (web)
CONTRACT   web/lib/deildir.ts — `ShareRow`, `BillingSlice`, `SHARE_TOLERANCE_PERCENT`.
CALLS      `fetchCustomerSlices(blockId) : Promise<ActionResult<BillingSlice[]>>`, `saveCustomerShares(blockId, rows: ShareRow[])`. Editor rows: customer select, deild select (that customer's deildir + blank), percent; a customer may repeat; Save disabled unless total = 100 ± tolerance and every row has a customer. Display line: `Sjúkra·Rekstur 50% · APRÓ·AI hraðall 50%` + origin tag.
THE FIVE   as T001.

## Contract for T009 — Move a super block's deild
CALLS      `POST /billing/lines/deild` body `{ day, block_ids: i64[], customer, from_deild: Option<String>, to_deild: Option<String> }` → for each block: take its current slices, set `to_deild` on those with (customer, from_deild), save as manual rows. Web: the line header's Verkefni picker calls it (server action `moveLineDeild`).
THE FIVE   as T001.

## Contract for T010 — Change log
CALLS      `new_batch(source:ChangeSource) -> String` (`"<source>-<unix_ms>"`); `refresh_day(conn, day:&str, source, batch:&str) -> Result<usize>`: for each non-personal block of `day`, build `ResolutionSnapshot` from `resolve_block_slices` (fractions = slice seconds / block seconds, rounded to 0.01), diff against the stored one: customer set differs → Customer; same customers, deild differs → Deild; same pairs, fractions differ → Split; description differs → Description. No stored snapshot → store, log nothing. Blocks gone → delete their snapshot, log nothing. Returns rows logged. `feed(conn, after:i64) -> Result<ChangeFeed>`, `unseen(conn) -> Result<ChangeFeed>`, `mark_seen(conn, up_to:i64) -> Result<usize>`, `purge_old(conn) -> Result<usize>` (`CHANGE_RETENTION_DAYS`).
THE FIVE   as T001.

## Contract for T011 — Wire the writers
CALLS      one batch per run: `estimate_day_with` and the single-block describe → `Claude`; infer rebuild → `Rebuild`; `route_day` → `Verdict` for each day it touched; registry writes (customers, folders, deildir) → `Keyword` for today and yesterday; user block edits (description, personal, split, move-deild) → `User`. Call `refresh_day` after the write commits. NEVER touch rows where `estimated_by='manual'` or manual split rows (FR-08).
THE FIVE   as T001.

## Contract for T012 — Change routes
CALLS      `GET /changes?after=<i64>` → `ChangeFeed`; `GET /changes/unseen` → `ChangeFeed`; `POST /changes/seen` body `{ up_to: i64 }` → `{ "marked": usize }`. Call `purge_old` from the unseen route.
THE FIVE   as T001.

## Contract for T013 — Notices (web)
CONTRACT   web/lib/deildir.ts — `ChangeFeed`, `ChangeBatch`, `CHANGE_SOURCE_LABELS`, `LIVE_POLL_SECONDS`.
CALLS      server actions `fetchChanges(after:number)`, `fetchUnseenChanges()`, `markChangesSeen(upTo:number)`. `ChangeNotices` mounted once in `app/layout.tsx`: on mount load unseen → catch-up chip "N changes since your last visit"; every `LIVE_POLL_SECONDS` poll `after=cursor` → one toast per batch whose source ≠ `user`: "`<label>` changed `<count>` blocks · Show". Show / chip open a list of changes (old → new, source, block time); opening marks seen. `lib/toast.ts` gains an optional `{ label, onClick }` action.
THE FIVE   as T001.
