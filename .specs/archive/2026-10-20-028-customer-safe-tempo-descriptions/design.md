# Design — Customer-safe Tempo descriptions

## 1. Contract (Rust, `worklog-core`)

No new contract file: the seam is three public functions, declared here verbatim.

| canonical | identifier | defined in | banned synonyms |
|---|---|---|---|
| text rules | `line_text::validate(text: &str) -> Result<String, String>` (signature unchanged; calls `unsafe_reason` after its existing checks) | `line_text.rs` | `check_text`, `lint` |
| customer-safe rules | `line_text::unsafe_reason(text: &str) -> Option<String>` — only the five Constraints rules below; `Some(Icelandic reason)` on the first broken rule | `line_text.rs` | — |
| known customer | `pub struct KnownCustomer { pub name: String, pub aliases: Vec<String> }` (`Serialize, Deserialize, Clone, Debug, PartialEq`) | `line_text.rs` | `CustomerNames` |
| foreign customers | `line_text::foreign_customers(text: &str, ticket_customer: Option<&str>, known: &[KnownCustomer]) -> Vec<String>` | `line_text.rs` | — |
| static line check | `line_check::static_check(text: &str, ticket_summary: &str) -> Option<LineCheck>` | `line_check.rs` | — |

All sync. Existing `validate` reasons stay English and unchanged. New reasons are Icelandic and contain these keywords, which tests assert on: "vísar í vísbendingar", "nefnir Slack", "tala skrifuð í orðum", "fyrsta sögn í fleirtölu", "nefnir annan viðskiptavin: <names>".

## 2. Trust boundaries

| boundary | untrusted input | parse fn | failure |
|---|---|---|---|
| model reply | line text | `line_text::validate` then `foreign_customers` | re-ask, max 3 (existing loop) |
| user message JSON | `ticket_customer`, `known_customers` keys written by `prepare` | `serde_json::from_str` in `write_with_system` | missing/invalid keys → skip the customer rule only |

## 7. Decisions

- In the context of `tempo_line_writer`, facing six call sites of `prepare`/`write` (`cli.rs` ×3, `daemon_tempo_lines.rs`, `tempo_line_redescribe.rs`, `collectors/tempo.rs`), we chose to carry the customer data **inside the user message JSON** (`"ticket_customer"`, `"known_customers"`) that `prepare` already returns, and to read it back in `write_with_system`, rejecting a new `prepare` return type, to change no call site, accepting ~20 customer names sent to the model (which the prompt also uses: describe only `ticket_customer`). Makes hard: `tempo_line_writer.rs` only.
- In the context of `foreign_customers`, facing aliases that are Icelandic words ("festi", "sýni", "kirkjan") and two registry rows sharing aliases (RU / Háskólinn í Reykjavík: `ru.is`), we chose word-bounded matching where a name/alias that is **all uppercase** (an acronym: VIS, BYKO, IAV, RU, RL, MMS) matches case-insensitively (so "Vis" hits VÍS), any other name/alias matches **case-sensitively** ("Festi" never hits the verb "festi"), and an **all-lowercase alias is skipped** ("kirkjan", "efnisveita"); customers sharing any alias are one group, rejecting `Registry::customer_in_text` (case-insensitive, returns one hit), accepting that "Festi" opening a sentence is a false hit. APRÓ (and its aliases) never counts. Makes hard: `line_text.rs`.
- `ticket_customer` comes from `Registry::customer_in_text` over the ticket summary, then its parent summary when stored; `None` when ambiguous. With `None`, only two or more distinct customer groups are foreign; one named customer passes. `prepare` adds `ticket_customer` and `known_customers` to its JSON; `add_ticket` already keeps unknown keys, so they reach the model and `write_with_system` reads them back.

## Constraints

- Icelandic text is stored and shown exactly as typed; a check only ever sets `check_status`, never edits text.
- Evidence-talk phrases (case-insensitive substring): "liggja fyrir", "liggur fyrir", "vísbending", "staðfesta ekki", "staðfestir ekki", "ekki staðfest", "óljóst hvort", "ekki ljóst hvort".
- Spelled numbers: English `zero|one|…|nine|ten` immediately before or after `point`, or Icelandic `núll|einn|tveir|þrír|fjórir|fimm|sex|sjö|átta|níu` around `komma` (case-insensitive, word-bounded).
- Plural first verb: the text's first word is alphabetic, at least 3 letters, and ends in "u".
- The word "Slack" (case-insensitive, word-bounded) is rejected.

## Complexity Tracking

| Violation | Why needed | Simpler alternative rejected because |
|---|---|---|

## Contract for T001 — Text rules and prompt
USAGE      `line_text::validate("Mældu biðtíma. Sendi tillögur í Slack.")` → `Err(<Icelandic reason naming the rule>)`; good Appendix B texts → `Ok(text)` unchanged.
CONTRACT   `rust/crates/worklog-core/src/line_text.rs` — keep `validate`'s signature.
CALLS      `pub fn validate(text: &str) -> Result<String, String>`; new `pub fn unsafe_reason(text: &str) -> Option<String>`; `SYSTEM_PROMPT_IS` gains the FR-07 rules. Line-size guard: put the rules in a new `line_text_rules.rs` re-exported from `line_text.rs` if `line_text.rs` would pass its size limit (add that file to your report).
LIMITS     Constraints above, verbatim. Re-ask message in `tempo_line_writer::write_with_system` names the rules (FR-10).
DUPLICATE  none — because the rules live in one place.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T002 — Foreign-customer rule on ticket lines
USAGE      `foreign_customers("…frá Vis, Byko og Sjúkra.", Some("Sjúkra"), &known)` → `["BYKO","VÍS"]` (registry names, sorted); `prepare` writes `"ticket_customer"` and `"known_customers"` into its JSON; `write_with_system` rejects a reply whose `foreign_customers` is non-empty, reason "nefnir annan viðskiptavin: <names>".
CONTRACT   `line_text.rs` (`KnownCustomer`, `foreign_customers`), `tempo_line_writer.rs`.
CALLS      `Registry::load(conn)`, `Registry::customer_in_text(&str)` (existing, billing_registry.rs).
LIMITS     Non-empty when: the text names a customer other than `ticket_customer`, or two or more customers. APRÓ never counts. Shared-alias customers count once.
DUPLICATE  none — because one caller.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T003 — Flag meeting and hand-typed lines
USAGE      `line_check::static_check("Hertz - Greiningarfundur", "Hertz - Greiningarfundur ")` → `Some(LineCheck::NeedsLook)`; a passing text → `None`. `tempo_lines::set_text` stores the text as typed, then sets `check_status` from `static_check`; the meeting branch in `daemon_tempo_lines.rs` does the same instead of `None`.
CONTRACT   `line_check.rs` (`static_check`), `line_text::validate` from T001.
CALLS      `pub fn static_check(text: &str, ticket_summary: &str) -> Option<LineCheck>`; existing `tempo_lines::set_check`.
LIMITS     Returns `Some(NeedsLook)` when `line_text::unsafe_reason` is `Some` (NOT full `validate`: hand-typed and meeting lines are often one sentence) or the text equals the summary (lowercase, all whitespace removed); otherwise `None` — never `Passed`, nobody checked it. Never edits text (FR-08). `set_text` looks up `jira_tickets.summary` and writes the result into its own UPDATE instead of `check_status = NULL` (`set_check` only touches generated rows). The meeting branch passes `static_check(text, &summary)` instead of `None`; if `set_check`'s origin guard drops it, relax the guard for that call only.
DUPLICATE  none.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.
