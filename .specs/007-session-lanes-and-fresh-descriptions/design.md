# Design — Session lanes and fresh descriptions

Only what two tasks must agree on. Everything else is the task's call.

## 1. Shared names

| canonical | identifier | defined in | banned synonyms |
|---|---|---|---|
| lane tag | `InferEvent.lane_tag: Option<String>` (the session's customer name) | `rust/crates/worklog-core/src/infer.rs` | sub_lane, session_key, customer_tag |
| lane key | `infer_lanes::lane_key(e)` → folder key, plus `"#<lane_tag>"` when `lane_tag` is `Some` | `infer_lanes.rs` | — |
| lane folder | `infer_lanes::lane_folder(e)` → the folder key WITHOUT the tag (today's `lane_key` body) | `infer_lanes.rs` | — |
| session tagging | `session_customers::tag_sessions(events: &mut [InferEvent], registry: &crate::billing_registry::Registry)` | `session_customers.rs` (new) | — |
| long block | `estimate::LONG_BLOCK_MINUTES: i64 = 90` | `estimate.rs` | — |

## 2. Trust boundaries

| boundary | untrusted input | handled by |
|---|---|---|
| event titles/branches fed to customer match | free text from prompts | `Registry::customer_in_text` (existing; 0 or 2+ hits → `None`) |

## 4. Module boundaries

- `session_customers` · may import: `infer::InferEvent`, `infer_lanes::lane_folder`, `billing_registry::Registry` · exports: `tag_sessions`.
- Allocation windows and overlap shares are keyed by **folder**, so `infer_allocations::events_by_key` and `overlaps` MUST switch to `lane_folder`; only lane ownership (`infer_lanes` internals) uses the tagged `lane_key`.
- Billing reads `project_path`, never `lane_tag`, so a split-off block's billing folder is unchanged (FR-05).

## 6. Deliberately duplicated

none — because each task owns one file.

## 7. Decisions

- In the context of `infer_lanes`, facing "lane ownership is keyed by one string per event", we chose a `lane_tag` field folded into `lane_key` and rejected passing a session→customer map through `build_blocks_by_project`, to keep every lane pass unchanged, accepting 21 struct-literal edits (`lane_tag: None`).
  Makes hard: infer.rs, infer_lanes.rs, and the 4 test files that build `InferEvent` literals.
- In the context of `build_day_blocks`, facing "it is the one rebuild every caller uses", we call `tag_sessions` there right after `load_day_events`, rejected tagging inside `load_day_events` (it has no registry), accepting one `Registry::load` per rebuild.
  Makes hard: infer_allocations.rs.
- In the context of persisting blocks, facing "carry copies description by start/overlap regardless of size", we gate the description carry with a pure `infer_carry::keeps_description(prior: (i64,i64), new: (i64,i64), estimated_by: Option<&str>) -> bool` (minutes since epoch), rejected changing the claim logic, accepting that ticket/tempo id still carry as today.
  Makes hard: infer.rs persist loop.

## Contract for T001 — lane_tag field and tagged lane key
CONTRACT   rust/crates/worklog-core/src/infer.rs — add `pub lane_tag: Option<String>` to `InferEvent` with a doc line.
NAMES      lane tag = `lane_tag`; lane key = `lane_key`; lane folder = `lane_folder`.
CALLS      `pub(crate) fn lane_folder(e: &InferEvent) -> Option<String>` (today's lane_key body); `lane_key(e)` = `lane_folder(e)` + `"#" + tag` when tagged.
           `infer_allocations::events_by_key` and `overlaps` call `lane_folder`.
THE FIVE   (1) NEVER invent a field or name not in this block. (2) Every `InferEvent { .. }` literal gets `lane_tag: None`. (3) NEVER change lane ownership behaviour for untagged events. (4) NEVER edit files outside `files:`. (5) Keep `infer_lanes.rs` ≤ 400 lines — trim a comment if needed.

## Contract for T002 — session customer tagging
CONTRACT   rust/crates/worklog-core/src/session_customers.rs (new) + `mod session_customers;` in lib.rs.
CALLS      `pub(crate) fn tag_sessions(events: &mut [InferEvent], registry: &crate::billing_registry::Registry)`
           Group events with `session_id` by (`lane_folder(e)`, session). Text = that session's titles + jira_issue joined by "\n". Customer = `registry.customer_in_text(&text)`.
           Per folder: if resolved sessions name ≥ 2 distinct customers, set `lane_tag = Some(customer)` on every event of each resolved session. Otherwise tag nothing. Unresolved sessions and session-less events are never tagged.
THE FIVE   (1) NEVER guess a customer — only `customer_in_text`. (2) NEVER tag when a folder has < 2 distinct customers. (3) NEVER touch `project_path`. (4) NEVER edit files outside `files:`. (5) Tests build a `Registry` directly, no DB.

## Contract for T003 — wire tagging into the day rebuild
CALLS      In `infer_allocations::build_day_blocks`: `let mut events = load_day_events(..)?; tag_sessions(&mut events, &Registry::load(conn)?);` before building.
THE FIVE   (1) NEVER add a second rebuild path. (2) NEVER edit files outside `files:`.

## Contract for T004 — drop stale descriptions on big change
CALLS      `pub(crate) fn keeps_description(prior: (i64, i64), new: (i64, i64), estimated_by: Option<&str>) -> bool` in infer_carry.rs.
           true when `estimated_by == Some("manual")`; else false when `abs(new_len - prior_len) >= 30` or `max_len * 2 > min_len * 3` (growth and shrink alike); else true.
           In infer.rs persist: when false, write `description = None` and `estimated_by = None` for that block (ticket, tempo id, exported_at carry as today).
THE FIVE   (1) NEVER drop a `manual` description. (2) NEVER change which prior row is claimed. (3) NEVER edit files outside `files:`.

## Contract for T005 — long blocks described as tasks
CALLS      `pub const LONG_BLOCK_MINUTES: i64 = 90;` in estimate.rs. The block's user message carries `"describe_as_tasks": true` when `block_duration_minutes >= LONG_BLOCK_MINUTES`; SYSTEM_PROMPT gains one rule: when `describe_as_tasks` is true, write up to 3 imperative tasks joined by "; ", ≤ 140 chars for the whole joined string.
THE FIVE   (1) NEVER change behaviour for blocks < 90 min. (2) NEVER edit files outside `files:`.
