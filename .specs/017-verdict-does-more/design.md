# Design: Verdict does more

## 1. Contract files

- Rust: `rust/crates/worklog-core/src/verdict_contract.rs` (`use crate::verdict_contract::*`)
- Web: `web/lib/verdict_contract.ts`
- Both are orchestrator-owned. A missing type or constant is an escalation.

| canonical | identifier | defined in | banned synonyms |
|---|---|---|---|
| decision log | `verdict_decisions` table, `DecisionRow` | contract + schema.sql | history, audit, feedback |
| ranking | `Ranking { ranking, abstain, agreed }` | contract | guess list, scores |
| order check | `Ranking.agreed` | contract | consistency, double-check |
| shortlist | `SHORTLIST_MAX` options | contract | candidates list, narrowed |
| line check | `LineCheck` on `tempo_line_texts.check_status` | contract | validation, lint |
| review | `ReviewLine`, `GET /review` | contract | audit, inbox |
| auto-send | `AUTO_SEND_KEY`, `tempo_line_texts.auto_sent_at` | contract | autosync, auto-sync |
| Verdict state | `VerdictState`, `GET /verdict/status` | contract | helper status, health |

Instants are RFC3339 UTC strings. Day bucketing and the 17:00 hour use `$WORKLOG_TZ` as elsewhere.

## 2. Trust boundaries

| boundary | untrusted input | parse | failure |
|---|---|---|---|
| Verdict HTTP response | JSON | `serde_json` into `Ranking` | any error → `Ok(None)` for that one call, counted as not answering |
| `verdict_decisions.ranking` / `events.verdict_ranking` | JSON text | `serde_json` into `Ranking` | bad row → treated as no ranking, never a panic |
| envfile keys | `on`/`off` text | exact match | anything else → the key's default |

## 3. Schema (T001 only)

- `verdict_decisions(id INTEGER PRIMARY KEY, kind TEXT NOT NULL CHECK(kind IN ('project','ticket','line_text')), source TEXT NOT NULL CHECK(source IN ('verdict','owner')), subject TEXT NOT NULL, state_json TEXT NOT NULL, options TEXT NOT NULL /*JSON array*/, ranking TEXT /*JSON Ranking*/, chosen TEXT, previous TEXT, decided_at TEXT NOT NULL)`; indexes on `(kind, subject)` and `(decided_at)`. Never named in `purge.rs` (FR-09).
- `events.verdict_ranking TEXT` (JSON `Ranking`), `tempo_line_texts.check_status TEXT CHECK(check_status IN ('passed','needs_look'))`, `.auto_sent_at TEXT`, `.confirmed_at TEXT`, `.send_error TEXT`: each via its own `ensure_*` in `db.rs`.

## 4. Verdict HTTP shape (T002 server, T003 client)

- `POST /classify` body `{"state": <json>, "options": [<id>...], "examples": {<id>: [<text>...]}}` (`examples` optional) → `{"ranking": [{"id","probability"}...≤3], "abstain": <f64>, "agreed": <bool>}`. `options` length 1 → `ranking` of that one id, `abstain` 1.0, `agreed` false.
- `POST /match` unchanged; line checks use it.
- `split_groups` / `merge_groups` and their self-test are deleted.

## 5. Module boundaries

- `verdict_decisions.rs` · may import: contract, rusqlite · exports: `record`, `list_since`, `examples_for`, `latest_for`, `unchecked_count`
- `verdict_supervisor.rs` · may import: contract, envfile, verdict · exports: `spawn`, `status`, `set_enabled`, `retry`, `uv_args`
- `routing_shortlist.rs` · exports: `shortlist(conn, event, day) -> Vec<String>`
- `ticket_verdict.rs` · exports: `ticket_options(conn, block) -> Vec<String>`, `pick(conn, classifier, block) -> Result<Option<String>>`
- `line_check.rs` · exports: `check(client, line_text, ticket_summary) -> Result<Option<LineCheck>>`
- `scorecard.rs` · exports: `run(conn, classifier, apply: bool) -> Result<Scorecard>`
- `auto_send.rs` · exports: `ready_lines`, `run_if_due`, `review_lines`, `confirm`
- Anything not listed is a bug. New modules each get a sibling `*_test.rs` via `#[path]`, like `routing.rs`, to stay under the 400-line guard.

## 6. Shared resources

| resource | constructed by | passed how | received by |
|---|---|---|---|
| SQLite connection | daemon `with_conn` / CLI `db::open` | `&Connection` argument | every module above |
| Verdict client | `VerdictClassifier::new()` | `&dyn Classifier` argument | routing, ticket_verdict, scorecard |
| Verdict `/match` client | `verdict::match_texts` | function call | line_check |
| envfile | `envfile::read` / `upsert` | direct call | supervisor, auto_send, settings |

## 7. Deliberately duplicated

- `SHORTLIST_MAX`, `EXAMPLE_CHARS_*` are restated as literals in `verdict_server.py`; do not make Python read Rust.
- Ticket-option and project-option shortlists are separate functions; do not merge them into one generic shortlist.

## 8. Decisions

- In the context of routing, facing the measured drop from 96% to 72% correct at 25 options, we chose a ≤6 shortlist and rejected grouping, accepting that a project idle for 14 days needs a pin or link to be offered. Makes hard: `routing_shortlist.rs`, `verdict_server.py`.
- In the context of filing, facing order flips (~4.5%), we chose to act only when `agreed` is true, accepting two model passes per event. Makes hard: `verdict_server.py`, `routing.rs`.
- In the context of readiness, facing `ticket_origin = 'auto'` being shared by Verdict and the cloud model, we chose the decision log as the proof of a Verdict pick and rejected a new origin value, keeping the CHECK constraint unchanged. Makes hard: `auto_send.rs`.

## Contract for T001 — decision log and schema
CONTRACT  rust/crates/worklog-core/src/verdict_contract.rs — import from it.
CALLS     record(&Connection, &DecisionRow) -> Result<i64>; list_since(&Connection, DecisionKind, since: &str) -> Result<Vec<DecisionRow>>; examples_for(&Connection, folder: &str, limit: usize) -> Result<Vec<String>> (titles of events the Owner corrected TO folder, newest first); latest_for(&Connection, DecisionKind, subject: &str) -> Result<Option<DecisionRow>>; unchecked_count(&Connection, day: &str) -> Result<u32>
THE FIVE  see §5 of design reference.

## Contract for T002 — Verdict server
SHAPE     §4 above, verbatim. Keep `--self-test` runnable without rlcd.

## Contract for T003 — Verdict client and filing rule
CALLS     trait Classifier { fn classify(&self, state: &Value, options: &[String], examples: &BTreeMap<String, Vec<String>>) -> Result<Option<Ranking>>; }; verdict::match_texts(query: &str, texts: &[String]) -> Result<Vec<bool>>; decide accepts iff agreed && top ∈ options && top ≥ abstain×margin && top ≥ second×ratio.

## Contract for T004 — Verdict supervisor
CALLS     GET /verdict/status?day= → VerdictStatus; POST /verdict/enabled {"on": bool}; POST /verdict/retry. uv found via UV_CANDIDATES then PATH.

## Contract for T009 — auto-send and review
CALLS     GET /review → [ReviewLine]; POST /review/confirm {"day","jira_issue"?}; ready iff ticket origin manual|event, or auto with latest_for(Ticket, block id) source Verdict and chosen = jira_issue; and check_status passed or text_origin manual.

## UI system (T010–T012, from /design:design, extend mode)
The web app has an established system; nothing is re-rolled. Build ONLY from existing tokens and classes in `web/app/globals.css`: warm tinted oklch neutrals (`--bg`, `--bg-raised`, `--bg-sunk`, `--border`, `--border-strong`, `--fg`, `--fg-muted`, `--fg-subtle`), semantic `--sage*` (good/primary), `--amber*` (needs attention), `--terracotta*` (error), `--slate*` (neutral); radii `--radius-sm/md/lg`; focus `box-shadow: var(--ring)` via the global `:focus-visible`; hover transitions 120ms `cubic-bezier(0.25, 1, 0.5, 1)`; icons from `lucide-react` at size 13–15, strokeWidth 1.75. House rules (ChangeNotices.tsx header): actions are underlined text (`.review-toggle` style) or `.action-btn`; attention = a 3px left rule (`--amber`), error = 3px `--terracotta` rule; no pill-badge stacks, no uppercase micro-labels on the day page, no new colours, no new fonts. New CSS goes in ONE appended, component-prefixed block at the end of `globals.css` (`.verdict-*`, `.review-sec-*`); never edit an existing rule. Colour is never the only signal: every state has words. Every button names its outcome. All states (loading/pending, error, empty) are designed below; pending = `useTransition` + `disabled` + `<Loader2 className="spin" size={13}/>` and a label change.
Server actions are injected as optional props in tests (house pattern, see IgnoredLine.test.tsx) because `@/app/actions` is mocked process-wide.

## Contract for T010 — Settings: Verdict control
NEW `web/components/VerdictControl.tsx` (client), rendered by `SettingsPanel.tsx` as its own `<section className="settings-section">` placed before the routing settings.
- `<h3>Verdict</h3>` + `<p className="settings-hint">Files Slack messages and browser tabs into projects, picks clear tickets and checks Tempo text before it is sent.</p>`
- Switch: native checkbox inside `label.settings-field` (house pattern), text "Run Verdict". Toggling calls `setVerdictEnabled(on)` (new server action → daemon `POST /verdict/enabled {"on"}` via `web/lib/daemon.ts`) IMMEDIATELY, not via the Save footer; disabled while pending; on error toast.error with the daemon's message and the box flips back.
- State line `<p className="verdict-state" data-state={state} role="status">`: an 8px dot (`::before`, colour per state) + words. Exact copy: off → "Off"; starting → "Starting…"; running → "Running"; not_answering → "Not answering"; needs_uv → "Needs uv — install it with `brew install uv`, then press Retry"; stopped → "Stopped: {error}". Dot colours: off `--fg-subtle`, starting `--slate`, running `--sage`, not_answering/needs_uv `--amber`, stopped `--terracotta`; text uses the matching `-ink` token (off/starting `--fg-muted`).
- Retry: `.action-btn` "Retry" shown only for stopped and needs_uv → `retryVerdict()` (→ `POST /verdict/retry`); pending label "Retrying…".
- Scorecard: when `status.scorecard` is non-null, `<p className="settings-hint verdict-scorecard">Last night's check: {scorecard}</p>`; hidden when null.
- Auto-send: second checkbox `label.settings-field` "Send ready lines to Tempo at 17:00" + hint "Only lines with a clear ticket and checked text are sent. The rest wait on their day." This one IS part of the settings form (`SettingsFormState.autoSend`, diffed by `buildSettingsUpdate`, saved with the footer Save) because it is a setting (`AUTO_SEND_KEY`).
- Status is fetched with `fetchVerdictStatus(day)` (→ `GET /verdict/status?day=`) when the panel opens; while loading show `settings-hint` "Checking Verdict…"; on fetch error show "Couldn't reach worklog — Verdict's state is unknown." (amber-ink) and keep the checkbox enabled.
- Tests (VerdictControl.test.tsx): each of the 6 states renders its exact copy; Retry shown only for stopped/needs_uv and calls the injected retry; toggling calls the injected setEnabled with the new value and is disabled while pending; scorecard line shown/hidden; auto-send round-trips through buildSettingsUpdate.

## Contract for T011 — Day page: Verdict line, one-tap choices, needs-a-look
NEW `web/components/VerdictBanner.tsx`, rendered in `app/[day]/page.tsx` directly above `UnsortedList`; status from `GET /verdict/status?day=` fetched server-side in the page (a fetch failure renders nothing).
- running → renders nothing. Otherwise `<div className="verdict-line" data-tone="quiet|warn" role="status">` reusing the `.review-line`/`.review-summary` look plus a 3px left rule: quiet = `--border-strong`, warn = `--amber`. Copy (exact, spec Journey 2): off (quiet) "Verdict is off · {N} events not checked" + action "Turn on" (→ setVerdictEnabled(true)); starting (quiet) "Verdict is starting"; not_answering (warn) "Verdict isn't answering · {N} events not checked" + "Turn on" (→ retryVerdict()); needs_uv (warn) "Verdict needs uv · {N} events not checked"; stopped (warn) "Verdict stopped: {error}" + "Retry" (→ retryVerdict()). When N = 0 drop the " · N events not checked" part; N = 1 says "1 event". Actions are `.review-toggle`-style underlined text buttons, pending label "Turning on…"/"Retrying…", errors shown inline after the line in `--terracotta-ink` ("Couldn't turn Verdict on: {msg}").
- UnsortedList one-tap (FR-15): `RoutedEvent` gains `ranking?: RankedOption[]` (lib/types.ts, from the daemon's stored ranking). For an unsorted group whose first event has a ranking with ≥1 option that is in `folderOptions`, render inside `.sort-row-controls`, before the PalettePicker: `<span className="verdict-picks-label">Verdict suggests</span>` then up to 3 `.bd-chip` buttons labelled with the folder name (no percentages), `aria-label="File under {folder}"`. Tapping files the group exactly as choosing that folder in the PalettePicker does (same action, respects the "always" checkbox). The PalettePicker stays. No ranking → no chips, unchanged row.
- needs-a-look (FR-20): `TempoLine` gains `check_status?: LineCheck | null`. In `TicketGroup` summary, when `check_status === "needs_look"` and the text is generated, show `<span className="est-badge" data-kind="look">` with lucide `Eye` (11px) + "needs a look", `title="Verdict found this text vague after one rewrite — edit it before it is sent"`; `data-kind="look"` styled amber-ink / amber-bg border (new appended rule). Hidden for passed/null.
- Tests: VerdictBanner — nothing for running; exact copy for each other state; "3 events not checked" with N=3; N=0 drops the count; actions call injected handlers. UnsortedList — chips render top ≤3 in order, tapping calls the label action with that folder, no chips without ranking, PalettePicker still present.

## Contract for T012 — Review section on today's page
NEW `web/components/ReviewSection.tsx`, rendered in `app/[day]/page.tsx` ONLY when `day === todayISO()`, placed between ActionBar and DayStrip; data from `GET /review` (new `fetchReviewLines()` in lib/daemon.ts). Empty list or fetch failure → renders nothing (FR-35).
- Shell: `<section className="review-sec" aria-labelledby>` styled like `.sort-tray` (1px border, radius-lg, bg-raised, mb 20px). Header row: title "Sent to Tempo — check these" (14px/600 fg) + count "{n} lines" (fg-subtle, tabular-nums) + hint "Confirm each line, or fix it; a fix updates the same Tempo worklog."
- Grouped by day, newest first. Day heading row: weekday + date (e.g. "Mon 5 Oct", from lib/format) + link "Open day" (→ `/{day}`) + text action "Confirm all {n}" (→ `confirmReview(day)`; only counts sent lines).
- Line row (`li.review-sec-line`, grid: ticket | hours | text | actions): ticket key mono 13px; hours as decimal "1.5 h" tabular-nums; text fg, wraps fully (no clamp). Sent rows: actions "Looks right" (→ `confirmReview(day, jira_issue)`) and "Edit". Not-sent rows: 3px `--terracotta` left rule, "Not sent: {error}" in terracotta-ink, and `.action-btn` "Send again" (→ `runSync(day, false, jira_issue)`), no "Looks right".
- Edit (inline, replaces the row content): hours `<input type="number" step="0.25" min="0.25">` labelled "Hours", text `<textarea>` labelled "Text" (visible labels, not placeholders), buttons "Save to Tempo" (`.action-btn.primary`) and "Cancel". Save = `saveTempoLineHours` and/or `saveTempoLineText` (existing actions, only for changed fields) → `runSync(day, false, jira_issue)` (sync PUTs the existing worklog) → `confirmReview(day, jira_issue)`; pending label "Saving…"; on any error stay in edit mode and show the message under the buttons in terracotta-ink. Save disabled until something changed and hours ≥ 0.25. Changing the ticket is NOT offered here: the "Open day" link is the path for that (Ruling in NOTES.md).
- A confirmed/saved line leaves the list optimistically; when the last line goes, the section disappears.
- Tests: hidden on empty; groups newest first; confirm calls injected confirm with (day, issue) and removes the row; "Confirm all" calls with (day) only; not-sent row shows the message + Send again and no Looks right; edit save calls hours→sync→confirm in order (assert sync, NOT a create), and an error keeps edit mode open.

## Contract for T013 — daemon payloads for the day page
Found by T011's review: the web reads `RoutedEvent.ranking` and `TempoLine.check_status`, but the daemon never serialises them.
- `RoutedEvent` (routing_contract.rs) gains `#[serde(default, skip_serializing_if = "Option::is_none")] pub ranking: Option<Vec<RankedOption>>` = the stored `events.verdict_ranking` JSON's `ranking` list; `to_routed` (routing_rows.rs) selects and parses it; NULL or unparseable JSON → `None`, never a panic or an error. Update every `RoutedEvent { .. }` literal.
- `TempoLine` (tempo_line_contract.rs) gains `#[serde(default)] pub check_status: Option<LineCheck>` (serialised `"passed"`/`"needs_look"`/null, matching web/lib/verdict_contract.ts `LineCheck`); tempo_lines.rs selects `tempo_line_texts.check_status` into it; unknown strings → `None`.
- Tests: GET /days/{day}/routed JSON carries `ranking` for an event with a stored ranking and omits it otherwise; bad JSON row → no ranking; GET /tempo/lines/{day} JSON carries `"check_status":"needs_look"` for a flagged line and null for an unchecked one.

## Contract for T014 — design-evaluator fixes (evaluator round 1: C, 3.35)
Append-only CSS in the existing `/* verdict */` block at the end of globals.css; never edit an earlier rule.
1. Settings checkboxes (VerdictControl.tsx, both "Run Verdict" and "Send ready lines to Tempo at 17:00"): `className="settings-field verdict-check"`, checkbox BEFORE its text span, plus `aria-label` equal to the visible text. CSS: `.verdict-check { flex-direction: row; align-items: center; gap: 8px; }` `.verdict-check input { width: auto; padding: 0; background: none; border: 0; margin: 0; flex: none; }` `.verdict-check > span { color: var(--fg); font-weight: 400; font-size: 13px; }`. Test: `getByRole("checkbox", { name: "Run Verdict" })` and the auto-send one resolve.
2. Apply-now vs Save: under Run Verdict add hint "Turns on or off at once." ; the auto-send hint ends with " Saved with Save changes."
3. `.verdict-line[data-tone="quiet"] { border-left-color: var(--fg-subtle); }`
4. ReviewSection: "Looks right" pending shows `<Loader2 className="spin" size={13}/>` + "Confirming…"; Edit focuses the Hours input on open; Cancel returns focus to that row's Edit button; a visually-hidden `aria-live="polite"` region announces "Confirmed {jira_issue}" / "Saved {jira_issue} to Tempo" when a row leaves.
5. ReviewSection edit: when hours < 0.25 show `<p className="review-sec-error">Hours must be at least 0.25.</p>` and set `aria-invalid` on the Hours input; when nothing changed the button stays disabled with no message.
6. VerdictBanner needs_uv gets the same "Retry" action as stopped (→ retryVerdict()).
Tests cover each item; never weaken an existing assertion.

## Contract for T015 — design-evaluator round 2 fixes (round 2: C, 3.63)
Append-only CSS at the end of the existing verdict/review block in globals.css; never edit an earlier rule.
1. ReviewSection header: title "Tempo lines — check these"; count `{n} line` / `{n} lines` (singular at 1).
2. Day row action: render only when the day has ≥ 1 sent row; label "Confirm all {n}" when every row of that day is sent and n > 1, "Looks right" is enough when n = 1 (hide the day action then), and "Confirm {n} sent" when the day also has not-sent rows.
3. Not-sent row: after "Not sent: {error}" add the hint "Change the ticket or text from Open day, then Send again." (fg-muted, 12px).
4. Hours: reuse the day header's formatter from `web/lib/format.ts` (grep `export function format` there; the header shows e.g. "2.5h") instead of the hand-rolled "{n} h"; keep 2-decimal precision for non-quarter seconds (existing tests must still pass — adapt the expected string only if the shared formatter yields the same value in its own format, never weaken the precision assertion).
5. VerdictControl: the Run Verdict checkbox is `disabled` until the first status fetch resolves (success OR failure); on fetch failure it becomes enabled again (contract T010 keeps it usable with the amber message).
6. CSS: `.verdict-line .review-toggle, .review-sec .review-toggle { min-height: 24px; display: inline-flex; align-items: center; }`.
Tests for 1–5; never weaken an existing assertion.

## Contract for T016 — G002 fix + design round 3 fixes
A. (G002, fatal, score 88) `estimate::estimate_day` (estimate.rs:189) is what cli.rs:3332, cli.rs:3489 and daemon.rs:1949 call, and it never runs `ticket_verdict::apply`. Fix at the root, in estimate.rs only: `estimate_day` calls `estimate_day_with_verdict(conn, day, model, &invoker, &crate::verdict::VerdictClassifier::new(), crate::daemon::configured_route_rule())` for both provider arms. Verdict unreachable/off → `classify` yields no answer → the model's pick stands (existing fail-safe; never an error that fails estimation). Test: a test that calls the PUBLIC path used by callers (factor `estimate_day` into a thin wrapper over a testable `estimate_day_for(conn, day, model, invoker, classifier, rule)` if needed) and proves a clear Verdict pick is applied and an unreachable classifier leaves the model pick — so the wiring itself is covered, not only the inner function.
B. ReviewSection: after a row leaves ("Looks right", Save, Confirm all), move focus to the next row's first action button, else the previous row's, else the section's `h2` (give it `tabIndex={-1}`); when the section unmounts, focus nothing special. "Confirm all"/"Confirm n sent" pending shows `<Loader2 className="spin" size={13}/>` like the row buttons.
C. TicketGroup needs-a-look: keep `title`, add visible text after the badge in `.billing-text-hint` style: "Vague after one rewrite — edit before it is sent", and `aria-describedby` from the badge to it.
D. VerdictControl Run Verdict: while toggling show `<Loader2 className="spin" size={13}/>` and the label "Turning on…"/"Turning off…" in place of "Run Verdict" (aria-label stays "Run Verdict").
E. CSS (append-only, in the existing verdict block): `.est-badge[data-kind="look"] { border-color: var(--amber); }`.
Tests for A–D; never weaken an existing assertion.

## Contract for T017 — converge-pass gaps (acceptance rows with no proving test)
Tests only, except item 5 (a missing FR-19 feature). Never weaken an existing test.
1. FR-13 (routing_test.rs): an unfiled event whose channel/title shape matches a folder's stored examples exactly, but whose own text is unrelated, is NOT filed when Verdict's answer for the event's own text does not clear the rule (abstain or disagreeing order check). Prove no path files an event from examples alone (e.g. the shortlist/examples are only option text; decide() still needs a clearing Ranking).
2. Per-call timeout (verdict.rs tests): a `pub(crate) const CLASSIFY_TIMEOUT: Duration = Duration::from_secs(10)` used by `VerdictClassifier::new()`; a test asserts the const is 10 s, and a behaviour test with an httpmock `/classify` that delays longer than a short test client timeout (`with_client` with e.g. 200 ms) yields "no answer" (Ok(None)), so the event stays unfiled and counts as not checked.
3. Event-text room (verdict_server.py --self-test): assert that the worst-case option block (SHORTLIST_MAX=6 options, each with the longest folder id seen in the self-test fixtures, plus the full 600-char example budget) leaves ≥ 200 of 512 tokens for the event text, using a conservative 3 chars/token estimate when no tokenizer is loaded (state the estimate in a comment); if the real tokenizer is importable offline, use it instead.
4. FR-09 compression (verdict_decisions_test.rs): run the purge/compression entry points that exist in purge.rs (grep `compress` there) over a DB with decision rows older than any retention window and assert every verdict_decisions row survives.
5. FR-19 fixture (scorecard.rs + scorecard_test.rs): the nightly scorecard also runs a built-in fixture of 5 good and 5 vague Tempo lines (each with a ticket summary) through `line_check` using the live classifier and appends "line check fixture: N/10 right" to the scorecard string; when Verdict is unreachable it appends "line check fixture: skipped (Verdict not answering)". Test with a mock classifier: all-correct → "10/10 right"; unreachable → "skipped".

## Contract for T018 — example budget fits the 512-token input (FR-12 vs NFR "Event text room")
Measured with the real model tokenizer (`~/.local/share/worklog/verdict-model/tokenizer.json`): rlcd packs question + event + ALL option labels into ONE sequence truncated at 512 (core/engine_encoder.py build_model_input, truncation=True). Realistic worst case (6 options with 32-char folder ids + 600 chars of Slack/Jira-title examples) = 395 tokens → only 117 left for the event (3.05 chars/token). The self-test's "xxxx" text tokenizes at 5.6 chars/token and hid this.
1. `EXAMPLE_CHARS_TOTAL` 600 → 300 in rust verdict_contract.rs AND verdict_server.py (per-example 60 and 5 per option unchanged). Update routing_shortlist_test.rs's cap test to the new total with the SAME boundary rigour (exactly-at-cap kept, one-over dropped; comment says why) — this follows a spec amendment, not a weakening.
2. spec.md FR-12: "capped at 600 characters" → "capped at 300 characters" and append to the row's check: "(amended 2026-10-06: 600 left the event 117 of 512 tokens, measured)".
3. verdict_server.py `_event_text_room`: build the worst case from REALISTIC text (folder ids of 32 chars, example titles shaped like Slack/Jira titles at the 60-char cap, filling EXAMPLE_CHARS_TOTAL), count tokens with `tokenizers.Tokenizer.from_file(<model dir>/tokenizer.json)` when both the package and file are available (model dir from `WORKLOG_VERDICT_MODEL_DIR`, else `~/.local/share/worklog/verdict-model`), else estimate at 3 chars/token; assert ≥ 200 left of MODEL_TOKENS. Remove the 4 chars/token assumption and its comment.
