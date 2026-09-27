# Design — Session customer pins

Only what two tasks must agree on.

## 1. Shared names

| canonical | identifier | defined in | banned synonyms |
|---|---|---|---|
| pin table | `session_pins(session_id TEXT NOT NULL, customer TEXT NOT NULL, from_at TEXT NOT NULL, folder TEXT NOT NULL, branch TEXT, source TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')), PRIMARY KEY (session_id, from_at))` + `CREATE INDEX IF NOT EXISTS idx_session_pins_branch ON session_pins(folder, branch, from_at)` | `rust/crates/worklog-core/sql/schema.sql` (idempotent CREATE, no version bump) | customer_pins, session_customers table |
| pin source | `'claude'` (pin command) · `'inherited'` (copied at session start) | same | manual, auto |
| pin row | `pub struct SessionPin { pub session_id: String, pub customer: String, pub from_at: DateTime<Utc>, pub folder: String, pub branch: Option<String>, pub source: String }` | `rust/crates/worklog-core/src/session_pins.rs` | Pin, CustomerPin |
| customer lookup | `Registry::customer_named(&self, name: &str) -> Option<String>` — exact, case-insensitive match on a customer name or one of its aliases (reuse `parse_aliases`) | `rust/crates/worklog-core/src/billing_registry.rs` | resolve_name, find_customer |
| current branch | `pub fn current_branch(dir: &Path) -> Option<String>` — `git -C dir branch --show-current`; `None` on detached HEAD, non-repo or error | `rust/crates/worklog-core/src/git.rs` | branch_name, head_branch |
| default branch | `pub fn is_default_branch(branch: &str) -> bool` — `main` or `master` | `rust/crates/worklog-core/src/session_pins.rs` | — |
| slice origin | `SplitOrigin::Pinned` (serde `"pinned"`); web `SplitOrigin` gains `"pinned"`, label `"pinned"` | `tenant_contract.rs`, `web/lib/tenants.ts` | Session, Claude |

Folder = `billing::work_folder_for_path(cwd)`. All times UTC ISO-8601.

## 2. Trust boundaries

| boundary | untrusted input | handled by |
|---|---|---|
| pin command name argument | free text typed by the model | `Registry::customer_named`; `None` → refuse, exit 2, print customers |
| start-hook stdin | Claude Code SessionStart JSON (`session_id`, `cwd`) | serde parse; any failure → print nothing, exit 0 |

## 3. Exit codes (pin command)

`0` pinned · `2` unknown customer (stderr lists known customers) · `1` any other error. The start hook always exits `0`.

## 4. Module boundaries

- `session_pins` · may import: `db`, `billing`, `billing_registry`, `git`, `personal` · exports: `SessionPin`, `pin`, `inherit_for_session`, `pins_for_sessions`, `pin_for_branch`, `is_default_branch`, `start_text`.
- `session_customers::tag_sessions` gains a `pins: &[SessionPin]` parameter; a pinned session's events take the pin covering their time (latest `from_at` ≤ event ts), then the text guess fills the rest. The ≥2-distinct-customers-per-folder rule counts pinned and text-resolved customers together.
- `tenant_split::tenant_slices_for_block`: after the Owner's saved shares, before clues — if the block's linked sessions' pins covering the block name exactly one customer, return one `Pinned` slice for the whole block.
- `hook_run` is untouched and stays silent. The start instruction is a separate CLI subcommand `worklog session-hint` that prints to stdout.

## 5. Shared resources

- DB handle: callers pass `&Connection`; the pin command and the start hook open the DB directly (`db::open(&paths.db)`), like `worklog db info` — they must work with the daemon down.
- Hook install: `hook.rs` owns `~/.claude/settings.json`. `is_worklog_handler` sweeps every "worklog" command, so install must re-add BOTH `worklog hook-run` (all EVENTS) and `worklog session-hint` (SessionStart only) in the same pass.

## 6. Deliberately duplicated

none — because each seam has one owner file above.

## 7. Decisions

- In the context of the start hook, facing "`worklog hook-run` must never print (CLAUDE.md)", we chose a second subcommand `worklog session-hint` and rejected printing from hook-run, accepting an installer change. Makes hard: hook.rs, cli.rs.
- In the context of inheritance, facing "a /clear'd session has a new session id", we chose to copy the branch's latest pin into `session_pins` for the new session at start (`source='inherited'`), rejected resolving branch at infer time (events store no branch), accepting one row per session. Makes hard: session_pins.rs.
- In the context of the Owner's UI change, we reuse the existing saved-split editor (it already wins over everything) and never rewrite pins. Makes hard: none.

## Contract for T001 — pin store and customer lookup
NAMES      pin table, pin source, pin row, customer lookup, default branch (§1, verbatim).
CALLS      `pub fn pin(conn: &Connection, registry: &Registry, session_id: &str, cwd: &Path, name: &str, at: DateTime<Utc>, branch: Option<&str>) -> Result<SessionPin, PinError>` where `pub enum PinError { UnknownCustomer { known: Vec<String> }, Other(anyhow::Error) }`; `pub fn pin_for_branch(conn, folder: &str, branch: &str) -> Result<Option<SessionPin>>` (latest by from_at, never for a default branch); `pub fn pins_for_sessions(conn, session_ids: &[String]) -> Result<Vec<SessionPin>>`.
THE FIVE   (1) NEVER invent a field or name not in §1. (2) NEVER store an unresolved name. (3) NEVER bump SCHEMA_VERSION — idempotent CREATE only. (4) NEVER edit files outside files:. (5) NEVER abbreviate inside an identifier.

## Contract for T002 — current branch
CALLS      `pub fn current_branch(dir: &Path) -> Option<String>` in git.rs, with a test against a temp git repo (init, commit, checkout -b feat/x → Some("feat/x"); detached → None; non-repo → None).
THE FIVE   (1) NEVER panic. (2) NEVER edit files outside files:. (3) NEVER abbreviate inside an identifier.

## Contract for T003 — pin command
CALLS      `worklog pin <customer> --session <id> [--at <rfc3339>]` (cwd = process cwd; branch via `git::current_branch`) → `session_pins::pin`; prints `Pinned <session-short> to <Customer> from <HH:MM>`; exit codes §3.
THE FIVE   (1) NEVER call the daemon. (2) NEVER edit files outside files:. (3) NEVER print a stack trace on exit 2.

## Contract for T004 — start instruction
CALLS      `worklog session-hint` reads SessionStart JSON on stdin → `session_pins::start_text(conn, registry, session_id, cwd, now) -> Result<Option<String>>`; prints the text if `Some`. Rules: not under /Work or folder not multi-tenant → None. Non-default branch with a branch pin → insert an `inherited` pin for this session and return "This session is for <C> (pinned from branch <b>). If you switch customer, run: worklog pin <name> --session <id>". Else → the instruction: work out the customer from the repo, the prompt, the files you touch; when clear, pin it without asking: `worklog pin <name> --session <id>`; when unclear and the Owner is present, ask once; when nobody is present (background/unattended), pin nothing. List the known customers. ≤ 600 chars.
THE FIVE   (1) NEVER exit non-zero. (2) NEVER print on error. (3) NEVER touch hook_run.rs. (4) NEVER edit files outside files:.

## Contract for T006 — pins drive lanes
CALLS      `tag_sessions(events: &mut [InferEvent], registry: &Registry, pins: &[SessionPin])`; `build_day_blocks` loads `pins_for_sessions` for the day's session ids.
THE FIVE   (1) NEVER let a text guess override a pin. (2) NEVER tag a folder with < 2 distinct customers. (3) NEVER edit files outside files:.

## Contract for T007 — pinned customer line
CALLS      `SplitOrigin::Pinned`; step placed after `load_shares`, before `clues_for_block` in `tenant_slices_for_block`.
THE FIVE   (1) NEVER override the Owner's saved shares. (2) NEVER emit Pinned when the pins name 0 or 2+ customers. (3) NEVER edit files outside files:.
