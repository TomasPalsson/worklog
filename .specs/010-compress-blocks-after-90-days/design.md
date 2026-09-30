# Design — Compress blocks after 90 days

## 1. Contract file + language

Contract: `rust/crates/worklog-core/src/digest_contract.rs` (copied verbatim from `.specs/010-compress-blocks-after-90-days/digest_contract.rs` by T001; owner: orchestrator).

| canonical | identifier | defined in | banned synonyms |
|---|---|---|---|
| card | `BlockDigest` | digest_contract.rs | summary, archive, snapshot |
| card table | `block_digest` | sql/schema.sql | block_summary, block_archive |
| horizon | `HORIZON_DAYS`, `digest::horizon(today) -> NaiveDate` | contract / block_digest.rs | retention, cutoff_days |
| compressed day | `digest::day_is_compressed(conn, day) -> Result<bool>` | block_digest.rs | frozen, archived |
| refusal text | `DAY_COMPRESSED` | contract | any other wording |

- Table (T001, `sql/schema.sql`, `CREATE TABLE IF NOT EXISTS` — `db::migrate` applies it):
  `block_digest(block_id INTEGER PRIMARY KEY REFERENCES blocks(id) ON DELETE CASCADE, version INTEGER NOT NULL, built_at TEXT NOT NULL, json TEXT NOT NULL)`
- Timestamps UTC ISO-8601; `day` compared as `YYYY-MM-DD` text. All sync (rusqlite).

Module `rust/crates/worklog-core/src/block_digest.rs` (T001) exports exactly:
```rust
pub fn horizon(today: NaiveDate) -> NaiveDate;                       // today - HORIZON_DAYS
pub fn eval_evidence(events: &[Event]) -> (Vec<String>, Vec<String>); // lifted from block_eval.rs:112-122, unchanged logic
pub fn build_digest(conn: &Connection, block_id: i64) -> Result<BlockDigest>;
pub fn write_digest(conn: &Connection, block_id: i64, d: &BlockDigest) -> Result<bool>; // INSERT OR IGNORE; true if written
pub fn digest_for_block(conn: &Connection, block_id: i64) -> Result<Option<BlockDigest>>;
pub fn day_is_compressed(conn: &Connection, day: &str) -> Result<bool>;
```

## 2. Trust boundaries

| boundary | input | parse fn | failure |
|---|---|---|---|
| `block_digest.json` row | JSON text | `serde_json::from_str::<BlockDigest>` | per block: log to stderr, treat as `None` |
| claude_turn raw_json (prompts) | deflated JSON | `raw_json` decoder | per event: skip |
| prompts on card | user text | `scrub::scrub_secrets` before truncation | always applied |

## 3. Error taxonomy

Refusals on a compressed day are `anyhow::bail!("{DAY_COMPRESSED}: {day}")`. Daemon maps any error whose text starts with `DAY_COMPRESSED` to HTTP 409 with the existing error body shape; CLI exits 2.

## 5. Shared resources

- DB handle: `&Connection` passed in; nobody opens their own.
- Clock: `today` is a parameter everywhere (`purge` computes it once via `tz`); no `Utc::now()` inside block_digest.rs.

## 6. Deliberately duplicated

- none — the one place logic must not drift (eval evidence) is shared via `eval_evidence`, not copied.

## 7. Decisions

- In the context of readers for old blocks, facing 8 call sites that join `block_events`, we chose "read `digest_for_block` first; if `Some`, use it, else run today's query" and rejected a view/virtual table, to keep each reader's diff local, accepting 8 small branches. Makes hard: billing.rs, tenant_split.rs, tenant_clues.rs, personal.rs, daemon.rs, block_details.rs, block_eval.rs.
- In the context of rebuilds, facing `persist_blocks` deleting all blocks of a day, we chose a guard on `day_is_compressed` inside `persist_blocks` and `merge_same_ticket_adjacent` and re-description, rejected a clock-based guard, so it follows data not dates. Makes hard: infer.rs, estimate.rs.
- In the context of purge, we chose one transaction: write missing cards → delete events/sessions/transcript cache by age → delete orphan session_pins, and rejected deleting blocks. Makes hard: purge.rs.

## Complexity Tracking

| Violation | Why needed | Simpler alternative rejected because |
|---|---|---|

## Contract for T001 — card table and builder
CONTRACT   rust/crates/worklog-core/src/digest_contract.rs — copy verbatim from the spec dir, then import from it.
NAMES      card=BlockDigest, table=block_digest, horizon(), day_is_compressed(), eval_evidence()
MODULE     rust/crates/worklog-core/src/block_digest.rs · may import: digest_contract, repo, models, billing, personal, tenant_split, clues_send, clues_collect, scrub, raw_json · exports: the six fns in §1
CALLS      billing::work_folder_for_block, personal::dominant_project_path_for_block, billing distinct_paths / dominant_title queries, tenant_split::pinned_customer_for_block, clues_send::build_block_input (Err for personal → estimation fields empty); scrub::scrub_secrets on prompts, change_titles, branches, files BEFORE truncation; eval_repos/eval_titles never scrubbed (parity)
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T002 — freeze compressed days
CONTRACT   rust/crates/worklog-core/src/digest_contract.rs (DAY_COMPRESSED)
CALLS      block_digest::day_is_compressed(conn, day) at the top of infer::persist_blocks, estimate::merge_same_ticket_adjacent, and the re-description entry in estimate.rs; bail with DAY_COMPRESSED
THE FIVE   as T001.

## Contract for T003 — eval and billing read the card
CALLS      block_digest::digest_for_block(conn, id) first in block_eval::block_state (eval_repos/eval_titles), billing::work_folder_for_block (folder), dominant_title_for_blocks (invoice_titles[0]), distinct_paths_for_blocks (paths), tenant_split::pinned_customer_for_block (pinned_customer), tenant_clues::clues_for_block, personal::dominant_project_path_for_block (project_path); block_eval uses block_digest::eval_evidence for the live path
THE FIVE   as T001.

## Contract for T004 — daemon serves the card
CALLS      GET /blocks/:id/digest → 200 BlockDigest JSON | 404; stitch_day_summary uses event_count / events_by_source / project_path from the card when present; DAY_COMPRESSED errors → 409
THE FIVE   as T001.

## Contract for T005 — detail panel shows the card
CALLS      web/lib/daemon.ts getBlockDigest(id): Promise<BlockDigest | null> (404 → null); type BlockDigest in web/lib/types.ts mirrors the Rust field names (snake_case)
THE FIVE   as T001.

## Contract for T006 — compression run
CALLS      purge::run / prune_if_due / purge_rows reworked; horizon = block_digest::horizon(today); latch meta key `last_prune_cutoff` stores the horizon date so it runs at most once a day, advanced only on commit; one transaction per run (cards → deletes per spec §4.3 → orphan pins); snapshot stays `worklog.db.preprune`; PurgeReport gains `blocks_carded: i64` and `card_bytes: i64`, `blocks_deleted` stays and must be 0; cli `effective_since` clamps to horizon
THE FIVE   as T001.

## Contract for T007 — eval prints cards
CALLS      `worklog eval "<q>" --details` → after the table, per matched block: day, time, then non-empty card fields (change_titles, prompts, branches, active_minutes, folder)
THE FIVE   as T001.

## Contract for T008 — card keeps counts and the folder's path
CONTRACT   rust/crates/worklog-core/src/digest_contract.rs (DIGEST_VERSION 2: path_counts, invoice_title_counts, folder_path) — already edited by the orchestrator; import, never edit.
CALLS      block_digest builder fills `path_counts[i]` = events carrying `paths[i]`, `invoice_title_counts[i]` = eligible events (source not `claude%`, trimmed title) carrying `invoice_titles[i]`, both before any cap and in list order; `folder_path` = most-used `project_path` among the block's events with `billing::work_folder_for_path(p) == folder` (ties: lexicographically smallest), `None` when none; same stage-2 rule as daemon `stitch_day_summary`. Personal blocks: counts and folder_path follow the same rules as paths/project_path (not estimation fields).
THE FIVE   as T001.

## Contract for T009 — billing sums card counts
CALLS      billing::dominant_title_for_blocks adds each card's `invoice_titles[i]` with weight `invoice_title_counts[i]` (weight 1 when the counts vec is shorter — a v1 card) to the live per-event counts, then the existing count-desc/title-asc ranking; distinct_paths_for_blocks merges live `(path, COUNT(*))` rows (no SQL LIMIT) with each card's `(paths[i], path_counts[i])`, sums, ranks count-desc/path-asc, truncates to MAX_PATHS.
THE FIVE   as T001.

## Contract for T010 — day summary shows the card's folder path
CALLS      stitch_day_summary: carded block → `project_path: card.folder_path` (not `card.project_path`, which stays the personal-classify input); live path unchanged.
THE FIVE   as T001.

## Contract for T013 — dry-run estimates bytes freed
CALLS      purge::purge_rows(dry_run = true) runs the SAME transaction body as the real run (cards, then every §4.3 delete), reads `PRAGMA freelist_count` before and after the deletes, sets `bytes_freed = (after − before) × PRAGMA page_size` (floor 0), then ROLLS BACK — never commits; the separate dry-run COUNT(*) queries are deleted (the real path's counts are reused). `run()` still skips snapshot and VACUUM on dry-run and must keep the estimate. CLI prints `bytes freed: ~N (estimate)` on dry-run instead of `n/a (dry run)`.
THE FIVE   as T001.

## Contract for T015 — CLI exit 2, every collect clamps, carded wording
CALLS      main.rs: an error whose chain contains `digest_contract::DAY_COMPRESSED` ("day is compressed") prints the message and exits 2 (all other errors keep today's exit code) — test `compressed_day_exits_2` runs `worklog infer --day <compressed day>` against a DB with a card on that day and asserts exit 2 + stderr text + blocks byte-identical. cli.rs: every call that collects (incl. `worklog day --day <old>` → collect_targets) passes its since through the existing `collect_since(requested, local_today)` helper — test `day_collect_clamps_to_the_horizon` (unit-level on the since the day path computes). cli.rs `last_prune_line` / status JSON: human line says "N cards, X events" not "0 blocks". purge.rs: doc comments for `dry_run`, `bytes_freed`, `run()` and the dry-run test say the dry run executes and rolls back and returns an estimate.
THE FIVE   as T001.

## Contract for T016 — acceptance proofs (tests only unless a test exposes a bug)
CALLS      block_digest_test.rs `oversized_card_is_cut_to_every_cap`: one work block seeded past EVERY cap in digest_contract.rs (MAX_EVAL_TITLES/EVAL_TITLE_CHARS, MAX_PATHS/PROJECT_PATH_CHARS, MAX_INVOICE_TITLES/INVOICE_TITLE_CHARS, MAX_BRANCHES/BRANCH_CHARS, MAX_CHANGE_TITLES/CHANGE_TITLE_CHARS, MAX_PROMPTS/PROMPT_CHARS (+ a prompt under PROMPT_MIN_CHARS dropped), MAX_FILES, MAX_TOOLS, FOLDER_CHARS, JIRA_SUMMARY_CHARS) → assert each list len == its cap and each item's char count <= its char cap (FR-06). daemon.rs: extend `compressed_day_allow_and_refuse_allows_hand_edits` (or a sibling whose name starts `compressed_day_allow`) with POST /blocks/:id/ignore {ignored:false} after ignoring (unignore) succeeding on a compressed day (FR-10); add `prune_tick_ignores_cycle_settings`: set non-default billing-cycle settings (the same config/env the old cutoff_for_cycle read), seed blocks aged 30–89 days with events, run the daemon's prune due-check path (`prune_due_check_once` or the fn it calls with the horizon it computes) → 0 cards, 0 blocks deleted, events intact (FR-16). collectors/tempo.rs: `tempo_sync_works_on_a_compressed_day`: a day with a carded block and no events, `sync_day_with` against an httpmock Tempo server (mirror the existing tests in that file) succeeds and posts the block (FR-10 Tempo sync). If any test exposes a real bug, STOP and report it — do not fix production code beyond the listed files.
THE FIVE   as T001.
