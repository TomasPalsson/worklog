# Design — Jira assistant

## 1. Contract file + language

Contract: `rust/crates/worklog-core/src/jira_assist_contract.rs` (T001 copies `.specs/013-jira-assistant/contract.rs`). Web mirrors `StatusHint` in `web/lib/types.ts` by hand.

| canonical | identifier | plural | defined in | banned synonyms |
|---|---|---|---|---|
| ticket view | `TicketView` | — | contract | TicketInfo, IssueView |
| start | `start_ticket` / `StartResult` | — | contract | begin, activate |
| move | `move_ticket` / `MoveBody` | — | contract | transition_to (for the by-name move) |
| allowed account | `AllowedAccount` | `allowed_accounts` | contract | TempoAccount (that is the Tempo list) |
| account suggestion | `AccountSuggestion` | `suggestions` | contract | guess, match |
| clue | `clue` | `clues` | DB `account_clues` | keyword, hint (hint is a status hint) |
| decision | `account_decisions` row | — | DB | history, log |
| status hint | `StatusHint` | `hints` | contract | suggestion (that is an account) |

Async/sync: core is sync (`reqwest::blocking`, `rusqlite`); daemon handlers wrap Jira calls in `spawn_blocking`, DB in `with_conn`, as `daemon_tasks.rs` does.

## 2. Trust boundaries

| boundary | untrusted input | parse fn | failure |
|---|---|---|---|
| CLI/daemon body | key, account id, markdown | `validated_key` (daemon_tasks.rs) + `is_writable_key` + allowed-list lookup | 400 per request |
| Jira createmeta | allowed values JSON | `jira::allowed_accounts_with` | 502, never an empty list treated as "all allowed" |
| GitHub search | `pull_request.merged_at` | serde `Option<String>` | missing → not merged |
| Ticket text | emoji | `ticket_text::has_emoji` | 400 |

## 3. Error taxonomy

Reuse `tempo_hub_contract::HubError` + `daemon_tasks::hub_error`. No new variant. CLI prints the daemon's error body on one line to stderr and exits 1.

## 4. Module boundaries

- `jira_assist_contract` · layer 0 · may import: serde, tempo_hub_contract · exports: the contract.
- `collectors/jira` · layer 1 · adds `allowed_accounts_with`, `search_accounted_with`, `fetch_account_with`; may not import any new module.
- `ticket_text` · layer 1 · `markdown_to_adf(&str) -> Value`, `has_emoji(&str) -> bool`; pure.
- `account_clues` · layer 2 · may import: db, contract, ticket_text · exports: `relearn`, `suggest`, `record_decision`.
- `ticket_assist` · layer 2 · may import: collectors/jira, contract, account_clues, ticket_text · exports: `start_ticket_with`, `move_ticket_with`, `assist_create_with`.
- `status_hints` · layer 2 · may import: db, contract · exports: `done_hints(conn) -> Result<Vec<StatusHint>>` (DB only, no network).
- `daemon_assist.rs` · `#[path]` child of daemon.rs like `daemon_tasks.rs`; the only place new routes live.
- CLI `worklog-cli/src/ticket_cmd.rs` · talks only to `daemon_client`.

## 5. Shared resources

- DB: `account_clues(account_id TEXT, account_name TEXT, clue TEXT, hits INTEGER, wrong INTEGER, PRIMARY KEY(account_id, clue))`, `account_decisions(id INTEGER PK, decided_at TEXT, summary TEXT, picked_id TEXT, guessed_id TEXT, correct INTEGER, clues TEXT /* JSON array */)`, `account_ticket_counts(account_id TEXT PK, account_name TEXT, tickets INTEGER)`. Added to `sql/schema.sql`; `SCHEMA_VERSION` 19 → 20 — T003 owns the bump, nobody else touches it.
- PR merged: `events.raw_json` for `github_pr` gains `merged_at: Option<String>` on `RawRecord::Commit` (`clues_contract.rs:31`, `#[serde(default)]`); written by T009, read by T010.
- HTTP client: `crate::http::client()`; Jira auth: `JiraAuth::from_secrets()`; account field: secret `jira_account_field_id`.

## 6. Deliberately duplicated

- Transition lookup by name lives in `ticket_assist`; do not refactor `daemon_tasks::transition` to share it — the web route keeps its id-based contract.
- The web "Move to Done?" chip calls the existing `loadTransitions` + transition action; do not add a web route for hints-apply.

## 7. Decisions

- In the context of creation, facing "only accounts in Jira's fresh list" (D-12), we chose `allowed_accounts_with(auth, project, issue_type, field_id, client) -> Result<Vec<AllowedAccount>>` over the Tempo `/accounts` list, to match what Jira will accept, accepting one extra call per create. Makes hard: `jira.rs`, `ticket_assist.rs`.
- In the context of clue learning, facing a single user and ≤200 tickets, we chose deterministic extraction (summary prefix before " - " + top non-stopword words) over a model call, accepting weaker first clues; Claude adds clues by passing them on create. Makes hard: `account_clues.rs`.
- In the context of the GENAI guard, facing D-10 Not-this, we chose `is_writable_key` checks in the new routes only (A10), leaving `/tickets/:key/transition` untouched. Makes hard: `daemon_assist.rs`.

## Complexity Tracking

| Violation | Why needed | Simpler alternative rejected because |
|---|---|---|

## Contract for T002 — markdown→ADF + emoji check
CONTRACT   none — pure functions.
MODULE     rust/crates/worklog-core/src/ticket_text.rs · layer 1 · exports: markdown_to_adf, has_emoji
CALLS      pub fn markdown_to_adf(md: &str) -> serde_json::Value; pub fn has_emoji(s: &str) -> bool
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T004 — Jira additions
CONTRACT   rust/crates/worklog-core/src/jira_assist_contract.rs — import AllowedAccount.
CALLS      pub fn allowed_accounts_with(auth:&JiraAuth, project:&str, issue_type:&str, field_id:&str, client:&Client) -> Result<Vec<AllowedAccount>>; pub fn search_accounted_with(auth:&JiraAuth, field_id:&str, limit:usize, client:&Client) -> Result<Vec<(String /*summary*/, AllowedAccount)>>; pub fn fetch_account_with(auth:&JiraAuth, key:&str, field_id:&str, client:&Client) -> Result<Option<AllowedAccount>>; NewIssue.description now goes through ticket_text::markdown_to_adf.
THE FIVE   (same five as above)

## Contract for T005 — account clues
CONTRACT   rust/crates/worklog-core/src/jira_assist_contract.rs — AllowedAccount, AccountSuggestion, RelearnReport, CLUE_DROP_AFTER_WRONG, SUGGEST_LIMIT.
CALLS      pub fn relearn(conn:&Connection, tickets:&[(String, AllowedAccount)]) -> Result<RelearnReport>; pub fn suggest(conn:&Connection, text:&str, allowed:&[AllowedAccount]) -> Result<Vec<AccountSuggestion>>; pub fn record_decision(conn:&Connection, summary:&str, picked:&AllowedAccount, guessed_id:Option<&str>, clues:&[String]) -> Result<()>
THE FIVE   (same five as above)

## Contract for T006 — start / move / assist-create
CONTRACT   jira_assist_contract — TicketView, StartResult, StartOutcome, AssistCreateBody, AssistCreated, is_writable_key, WRITE_PROJECT, CREATE_ISSUE_TYPE.
CALLS      pub fn start_ticket_with(auth, field_id:Option<&str>, key:&str, client) -> Result<StartResult>; pub fn move_ticket_with(auth, key:&str, to_status:&str, client) -> Result<TicketStatus>; pub fn assist_create_with(conn, auth, field_id:&str, body:&AssistCreateBody, client) -> Result<AssistCreated>
THE FIVE   (same five as above)

## Contract for T010 — done hints
CONTRACT   jira_assist_contract — StatusHint, HintReason, is_writable_key.
CALLS      pub fn done_hints(conn:&Connection) -> Result<Vec<StatusHint>> — GENAI keys, jira_tickets.status_category IS NOT 'done' (FR-16), a github_pr event with jira_issue = key and raw_json.merged_at set.
THE FIVE   (same five as above)
