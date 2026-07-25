# Spec: Billing Export (per-day invoicing-form line items)

> **One-sentence summary**: Turn a day's tracked blocks into the line items the external invoicing form is filled in from, deriving what it safely can and leaving the rest blank.

**Status**: Built (v2 — retargeted at the real form)
**Size**: Medium
**Author**: Mr Claude (for Tomas)
**Created**: 2026-07-24
**Last updated**: 2026-07-25
**Version**: 2.0

---

## ⚠️ v2 amendment — read this first

v1 (below) specified a provisional shape, `repo · description · hours · type`,
because the target system hadn't been seen yet. The user then supplied the
actual invoicing form, which **supersedes that shape**. What shipped:

| Form field | Source | Notes |
|---|---|---|
| Dagsetning | the day | rendered `dd.mm.yyyy` |
| **Viðskiptamaður** | folder pin → else customer alias matched in the block's Jira ticket summary / description → else **blank** | ambiguous match ⇒ blank, never a coin-flip |
| **Verkefni (deild)** | registry folder pin **only** | never model-guessed; blank ⇒ user picks |
| Tegund skráningar | `Almenn skráning` | constant |
| Taxti | `Dagvinna` | constant |
| **Tímar** | overlap-safe union of block intervals, ½h-rounded | comma decimal (`5,5`) |
| **Reikningshæfi** | folder pin, default `Reikningshæft` | the only two values are `Reikningshæft` / `Óreikningshæft` |
| **Texti á reikning** | block description, unmodified | |

**Dropped as never used by this user**: `Tengiliður`, `Rukka fyrir akstur`,
`Útkall á bakvakt`, `External Id`.

**Superseded v1 decisions**
- *Line unit*: `(customer, verkefni, task)` — not `(repo, task)`.
- *Personal time*: **excluded entirely**; there is no Work/Personal column.
  (v1 included it as `type: Personal`.)
- *Registry storage*: SQLite (schema v9), edited in the web UI. The user
  explicitly rejected editing a config file, so there is no TOML.
- *Copy-paste blob*: removed. The panel is a one-line-at-a-time wizard in
  the form's field order; only `Tímar` and `Texti á reikning` get copy
  buttons, because every other field is a dropdown the user picks.

**Defect found and fixed while building**: folder derivation used the path
basename, so `sjukra/.claude/worktrees/mega-audit` billed as `mega-audit`
and `sjukra/app` as `app`. ~40% of this user's `sjukra` events live in
worktrees, so a customer's day was being fragmented. `work_folder_for_path`
now strips worktree scaffolding and collapses sub-dirs to the project root.

**Grounding facts** established from the user's real database (they change
the design, so they are recorded here): `events.repo` is ~99.96% NULL (only
the GitHub collector sets it), so the billable key is the `~/Desktop/Work/…`
folder from `project_path`; and Jira keys are sparse and polluted with
estimator misfires (`CVE`, `RFC`, `IEEE`), so they are not a customer key.

Sections 1–9 below are the v1 record, kept for the reasoning trail. Where
they conflict with this amendment, the amendment wins.

---

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: The team moved off Tempo. There is no longer a way to turn a day's work into the per-line-item breakdown the company needs to charge customers.

**Solution**: A repo-centric billing export — group a day's blocks by repo + task, sum overlap-safe hours (rounded to 0.5h), tag each line Work/Personal, and render it for manual copy-paste (text) plus CSV/JSON download. Available as `worklog export` (CLI) and an Export panel in the web day view. Tempo sync is left fully intact and untouched.

**Who it's for**: The developer using worklog to log time, who at end-of-day copies billable line items into a separate invoicing system.

**Non-goals (v1)**:
- Removing, disabling, or modifying Tempo sync (it stays exactly as-is, alongside).
- Mapping repo → customer (the external system does that; the export emits `repo`).
- Multi-day / week / range export (v1 is strictly one day).
- Pushing directly to the external billing system over an API (manual copy-paste only).
- Editing line items inside the export view (blocks are edited in the existing review UI first).
- LLM-summarising multi-block descriptions at export time (deterministic join in v1; LLM is a v1.1 candidate).

**MVP cut line**: Everything tagged `MUST` in Section 4 ships for v1. `SHOULD` items are v1.1 candidates.

**Key decision**: The export is **additive** — a new `BillingRow` contract computed in `worklog-core`, rendered by pluggable format renderers, surfaced via a new daemon route + CLI command + web panel. A new nullable `blocks.exported_at` column becomes the billing "canary" that (a) prevents double-billing a day and (b) keeps `purge` working now that blocks will stop receiving `tempo_worklog_id`.

---

## 1. Context

### 1.1 Problem Statement

The developer used to review a day's blocks and `worklog sync` them to Tempo Cloud, which handled customer billing downstream. The team has moved off Tempo. The developer now needs to hand a *different* system the day's billable work as discrete line items — one per distinct task — each carrying the repository worked on, a human description, the hours spent, and whether it is billable Work or non-billable Personal time. Nothing in worklog produces this shape today.

**Current workaround**: None. Without Tempo there is no export; the developer would have to hand-transcribe from the terminal `summary`/`week` views, which are per-ticket (not per-repo) and show `Nh MMm` (not decimal billing hours).

**Business rationale**: The company bills customers from this breakdown. Every day not exported is revenue that cannot be invoiced accurately.

### 1.2 User Roles

| Role | Description | Volume | Key characteristic |
|------|-------------|--------|--------------------|
| Developer (primary) | Logs time via worklog, reviews the day, exports billable line items | 1 (solo) | Wants a clean, minimal, copy-pasteable breakdown; hates re-transcribing |

**Primary actor**: Developer.
**Hidden stakeholders**: The external invoicing system (downstream consumer of the exported text/CSV/JSON — the export shape is its contract). The `purge` retention job (depends on a "has been billed" marker to stay correct).

### 1.3 Prior Art & Alternatives Considered

| Option | Status | Why rejected / why not this |
|--------|--------|-----------------------------|
| Keep using Tempo sync | Rejected | Team has moved off Tempo entirely. |
| Rip out Tempo, replace with export | Rejected (v1) | Large, cross-cutting, risky; breaks the create-ticket account picker. Deferred. |
| Repurpose `tempo_worklog_id` as the export marker | Rejected | It is the Tempo dedup canary; CLAUDE.md forbids clobbering it. A separate `exported_at` is cleaner and additive. |
| Add export alongside Tempo, new `exported_at` canary | **Selected** | Smallest safe change; keeps Tempo working; keeps `purge` correct. |

---

## 2. Scope

### 2.1 In Scope

- A `worklog export --day <YYYY-MM-DD> [--format text|csv|json] [--mark]` CLI command.
- A daemon route `GET /export/{day}` returning the computed rows + day metadata as JSON.
- A web "Export" panel on the day view: renders the rows, copy-to-clipboard (text), download CSV, download JSON.
- Row computation in `worklog-core`: group a day's blocks by (dominant repo, task), union-safe hours, Work/Personal type, description (deterministic).
- Deriving a block's **dominant repo** from its events.
- Including **both** Work and Personal blocks (Personal blocks are excluded from Tempo sync but MUST appear in the export, tagged `Personal`).
- A new nullable `blocks.exported_at` column + an idempotent "mark exported" operation (CLI `--mark`, web button).
- Updating `purge` so an exported block is purge-eligible (parity with a synced block).

### 2.2 Out of Scope (Non-Goals)

- **Tempo removal/modification**: Tempo sync, `/sync`, `/accounts`, `TempoAccount`, the create-ticket account picker, the `dirty` flag, and all sync UI remain unchanged.
- **repo → customer mapping**: The external system owns it.
- **Range/week export**: v1 is one day only.
- **API push to the billing system**: manual copy-paste.
- **In-export editing**: use the existing review UI.
- **LLM description summarisation at export time**: deterministic join in v1.

### 2.3 Adjacent Systems

| System | Relationship | Constraint |
|--------|-------------|------------|
| Tempo sync (`collectors/tempo.rs`, `Cmd::Sync`, `/sync`, `/accounts`) | Coexists | MUST remain fully functional and unchanged. |
| `purge` (`purge.rs`) | Reads block billing state | MUST treat `exported_at` as a valid "billed" marker so worked blocks stay purge-eligible. |
| Estimator/review UI | Produces block descriptions | Export reuses `block.description`; does not re-estimate. |
| External invoicing system | Consumes the export | The text/CSV/JSON row shape is its (informal) contract; must stay stable + extensible. |

---

## 3. User Journeys

### Journey 1 — Export a day from the CLI (Priority: P1)

**Actor**: Developer.
**Starting condition**: A day has inferred/estimated blocks in the DB.
**Goal**: Get the day's billable line items as text to paste into the billing system.

**Happy path**:
1. Developer runs `worklog export --day 2026-07-23`.
2. worklog groups the day's blocks by (dominant repo, task), computes overlap-safe hours per group, tags each Work/Personal.
3. Terminal prints one line per group in the format `repo: <repo>  description: <desc>  time: <N,N> hrs  type: <Work|Personal>`.

**Error path — database not initialised**:
1. Developer runs `worklog export` before `worklog db migrate`.
2. worklog exits non-zero with `db not initialized. Run 'worklog db migrate' first.` (mirrors existing commands).

**Edge cases**:
- **Empty day**: prints a "no blocks for <day>" notice, exits 0.
- **Personal-only day**: prints only `type: Personal` lines.
- **Block with no repo** (pure meeting/Jira/gcal, no `repo`/`project_path`): repo renders as `—`; the line still appears.
- **`--format csv|json`**: emits CSV rows / a JSON array instead of the text lines.

**Acceptance criteria**:

| ID | Given | When | Then | Priority |
|----|-------|------|------|----------|
| AC-001 | A day with 2 work blocks on repo `genai-infra` under different tickets, one 4h and one 5h30m block | `worklog export --day D` | Output has two `repo: genai-infra` lines, `time: 4 hrs` and `time: 5,5 hrs`, both `type: Work` | MUST |
| AC-002 | A day with one personal block on repo `some-app` (~2h) | `worklog export --day D` | Output has one `repo: some-app … time: 2 hrs type: Personal` line | MUST |
| AC-003 | Two work blocks, same repo, same ticket, 1h + 1h | `worklog export --day D` | They collapse to ONE line with `time: 2 hrs` | MUST |
| AC-004 | Two work blocks on the same ticket whose wall-clock intervals overlap by 30m (e.g. a meeting during coding), 1h + 1h | `worklog export --day D` | The line's hours reflect the interval **union**, not the naive 2h sum | MUST |
| AC-005 | No blocks exist for the day | `worklog export --day D` | Exit 0, prints a "no blocks" notice, no rows | MUST |
| AC-006 | Any day with blocks | `worklog export --day D --format json` | Output parses as a JSON array of `{repo, description, hours, seconds, type}` | MUST |
| AC-007 | db not initialised | `worklog export` | Exit non-zero with the "db not initialized" message | MUST |
| AC-008 | A day with N grouped rows | Rendering the same day as text, csv, and json | All three contain the same N line items (same repo/description/hours/type set) | MUST |

### Journey 2 — Export a day from the web review UI (Priority: P1)

**Actor**: Developer.
**Starting condition**: Reviewing a day in the web UI.
**Goal**: Copy the day's billable line items, or download them.

**Happy path**:
1. Developer clicks **Export** on the day view.
2. A panel opens showing the grouped rows in a table (repo, description, hours, type) with a day total.
3. Developer clicks **Copy** → the text-format lines are on the clipboard; a toast confirms.
4. (Optional) Developer clicks **Download CSV** / **Download JSON**.

**Error path — daemon unreachable**:
1. The panel's fetch fails.
2. The panel shows an inline error ("Couldn't load export — is the daemon running?") and a Retry; no crash.

**Edge cases**:
- **Empty day**: panel shows "No blocks to export for this day."
- **Clipboard blocked** (permissions): toast shows an error; the text remains selectable in the panel as a fallback.

**Acceptance criteria**:

| ID | Given | When | Then | Priority |
|----|-------|------|------|----------|
| AC-010 | A day with grouped billable rows | Developer opens the Export panel | The panel lists one row per group with repo, description, billed hours, and Work/Personal type | MUST |
| AC-011 | The Export panel is open | Developer clicks Copy | `navigator.clipboard` receives the text-format lines and a success toast appears | MUST |
| AC-012 | The Export panel is open | Developer clicks Download CSV | A `.csv` file with a header row + one row per line item downloads | MUST |
| AC-013 | The daemon returns an error | Developer opens the Export panel | An inline error + Retry is shown; the app does not crash | MUST |

### Journey 3 — Mark a day exported + keep purge correct (Priority: P2)

**Actor**: Developer (mark) + System (purge).
**Starting condition**: A day has been exported/copied.
**Goal**: Record that the day was billed so it isn't billed twice and so it can be purged later.

**Happy path**:
1. Developer runs `worklog export --day D --mark` (or clicks "Mark exported" in the panel).
2. Every block included in that day's export gets `exported_at = now` (idempotent — re-running does not change already-set values unless re-marking is explicit).
3. `worklog purge` now treats those blocks as billed and purge-eligible after the retention window.

**Error path — nothing to mark**:
1. Developer marks a day with no blocks.
2. worklog reports "0 blocks marked", exits 0.

**Edge cases**:
- **Re-exporting an already-marked day**: the export still renders (read-only), and the CLI/panel indicates the day was already exported (shows the `exported_at` date). `--mark` is idempotent.
- **A block edited after being marked**: out of scope for v1 — no "dirty since export" tracking (documented assumption).

**Acceptance criteria**:

| ID | Given | When | Then | Priority |
|----|-------|------|------|----------|
| AC-020 | A day with 3 blocks, none exported | `worklog export --day D --mark` | All 3 blocks have a non-null `exported_at`; command reports 3 marked | MUST |
| AC-021 | A worked block with `exported_at` set but no `tempo_worklog_id`, older than the retention window | `worklog purge` | The block is eligible for purge (parity with a synced block) | MUST |
| AC-022 | A block already has `exported_at` set | `worklog export --day D --mark` again | `exported_at` is not overwritten; command reports it as already-exported | SHOULD |

---

## 4. Functional Requirements

### 4.1 Core Requirements

| ID | Actor | Requirement | Priority | Acceptance Link |
|----|-------|-------------|----------|-----------------|
| FR-001 | System | MUST compute, for a given day, a list of billing rows grouped by (dominant repo, task) where task = jira_issue if present, else the block description, else the block id | MUST | AC-001, AC-003 |
| FR-002 | System | MUST compute each row's hours as the **union** of its blocks' intervals (start → start+duration), never the naive duration sum, then round to the nearest 0.5h | MUST | AC-004 |
| FR-003 | System | MUST derive a block's dominant repo from its events: most-frequent `events.repo`, else the basename of the most-frequent `events.project_path`, else none (`—`) | MUST | AC-001 |
| FR-004 | System | MUST include Personal blocks in the export tagged `type: Personal`, and Work blocks tagged `type: Work` | MUST | AC-002 |
| FR-005 | System | MUST render a row's hours with a comma decimal separator and a trailing unit (`5,5 hrs`, `4 hrs`, `2 hrs`) in the text format | MUST | AC-001, AC-002 |
| FR-006 | Developer | MUST be able to run `worklog export --day D` and see the text-format rows in the terminal | MUST | AC-001 |
| FR-007 | Developer | MUST be able to choose `--format text\|csv\|json`; the row data is identical, only the rendering differs | MUST | AC-006 |
| FR-008 | System | MUST expose `GET /export/:day` on the daemon returning `{day, rows[], exported_at, rendered:{text,csv,json}}` as JSON — where the day-level `exported_at` = the latest `exported_at` across the day's included blocks (null if none exported) — computed by the same core function the CLI uses | MUST | AC-010 |
| FR-009 | Developer | MUST be able to open an Export panel in the web day view showing the rows and a day total | MUST | AC-010 |
| FR-010 | Developer | MUST be able to copy the text-format rows to the clipboard from the panel, with a success/failure toast | MUST | AC-011 |
| FR-011 | Developer | MUST be able to download the rows as CSV and as JSON from the panel | MUST | AC-012 |
| FR-012 | Developer | MUST be able to mark a day's blocks exported (`--mark` / panel button), setting `exported_at` on each of the day's blocks | MUST | AC-020 |
| FR-013 | System | MUST treat a block with `exported_at` set as purge-eligible (equivalent to a synced block) in `purge.rs` | MUST | AC-021 |
| FR-014 | System | MUST leave all Tempo sync behaviour, routes, and UI unchanged | MUST | — |
| FR-015 | Developer | SHOULD find that re-marking an already-exported day does not overwrite existing `exported_at` values and reports the day as already exported (with its date) | SHOULD | AC-022 |
| FR-016 | System | All output formats MUST render from the same `rows_for_day` result via a single format-selecting renderer; the text/csv/json of one day MUST contain the same set of line items (adding a format is a new renderer arm only) | MUST | AC-006, AC-008 |

### 4.2 Data Requirements

| Entity | Description | Key attributes (logical) | Relationships |
|--------|-------------|--------------------------|---------------|
| Billing row | One billable line item for a day | repo, description, type (Work/Personal), raw seconds, billed hours, contributing block ids | Derived from ≥1 Block |
| Block (extended) | Existing time block, plus a billing marker | + exported_at (when the block was last exported/billed) | Unchanged otherwise |

**Data retention**: `exported_at` persists with the block. `purge` may hard-delete a block once it has `exported_at` (or `tempo_worklog_id`, or is a `gap`) and is past the retention window.

**Data sensitivity**: Descriptions may contain work detail; no new PII beyond what blocks already hold. Export is local (clipboard/file); no network transmission by worklog.

---

## 5. Non-Functional Requirements

### 5.1 Performance

| Metric | Target | Condition | Measurement |
|--------|--------|-----------|-------------|
| `worklog export --day` compute+render | < 500 ms | A day with ≤ 200 blocks, no network calls | manual timing / test |
| `GET /export/{day}` | < 300 ms | Same, warm daemon | manual timing |

**Performance budget decision**: Export is deterministic and offline (no LLM, no network). Any network dependency in the v1 export path is a defect.

### 5.2 Security

**Authentication**: None new — CLI is local; the daemon binds localhost (unix socket + `127.0.0.1:9323`) exactly as today.
**Authorization**: Single-user local tool; no new roles.
**Data protection**: No new secrets. Export writes only to the local clipboard/filesystem at the user's explicit action.
**Threat surface**: CSV injection — a description beginning with `=`, `+`, `-`, `@` could be interpreted as a formula by a spreadsheet. The CSV renderer MUST neutralise this (prefix-guard or quoting) and MUST quote/escape delimiters and newlines.

### 5.3 Reliability & Availability

**Uptime target**: No SLA (local tool).
**Graceful degradation**: If the daemon is down, the web panel shows an inline error + Retry; the CLI reads the DB directly and does not depend on the daemon.
**Recovery behavior**: Export compute is pure/read-only; re-running is always safe. `--mark` is idempotent.

### 5.4 Error Handling

| Error condition | Actor-visible behavior | System behavior | Recovery path |
|----------------|----------------------|-----------------|---------------|
| db not initialised (CLI) | `db not initialized. Run 'worklog db migrate' first.` | Exit non-zero | Run migrate |
| Invalid `--day` | `invalid day 'X' (expected YYYY-MM-DD)` | Exit non-zero | Fix the arg |
| Daemon unreachable (web) | Inline "Couldn't load export — is the daemon running?" + Retry | Server Action returns `{ok:false,error}` | Start daemon, retry |
| Clipboard blocked (web) | Error toast; text stays selectable in the panel | Catch the rejected clipboard promise | Manual select-copy |
| Empty day | "no blocks for <day>" (CLI) / "No blocks to export" (web) | Exit 0 / render empty state | none needed |

### 5.5 Scalability

**Concurrency target**: Single user. N/A beyond current daemon behaviour.
**Growth assumption**: Blocks/day is small (tens). Re-evaluate only if a day exceeds thousands of blocks (won't happen).
**Bottleneck hypothesis**: The block↔event join for repo derivation; already indexed and per-day scoped.

### 5.6 Observability

**Required logging**: Reuse the daemon's existing `tracing` mutation-audit line for the mark-exported write. No new dashboards.
**Required metrics**: None for v1.
**Alerting threshold**: None.

### 5.7 Accessibility

**Standard**: Match the existing review UI (the Export panel reuses the `settings-dialog` modal shell: `role="dialog"`, `aria-modal`, Escape-to-close, focusable controls).
**Keyboard navigation**: Panel open/close, Copy, Download, and Mark are all keyboard-reachable.

### 5.8 Browser / Platform Support

| Platform | Support | Notes |
|----------|---------|-------|
| Desktop Chrome/Firefox/Safari (current) | Full | Matches existing web UI support |
| `navigator.clipboard` unavailable | Degraded | Copy shows an error toast; text remains selectable |

---

## 6. Success Criteria

### 6.1 Launch Criteria (go/no-go)

- [ ] All `MUST` ACs pass (`cargo test`, `bun test`).
- [ ] `worklog export --day` produces the exact text format from AC-001/002 on a seeded day.
- [ ] Web Export panel renders rows, copies text, downloads CSV/JSON; browser-verified.
- [ ] `purge` treats an `exported_at`-only block as purge-eligible (AC-021).
- [ ] Tempo sync tests remain green (no regression); `cargo clippy -D warnings` and `cargo fmt --check` pass; `bun run typecheck && bun run build` pass.
- [ ] CSV renderer neutralises formula-injection and escapes delimiters.

### 6.2 Post-Launch Health Metrics

| Metric | Target | Measurement | Review trigger |
|--------|--------|-------------|----------------|
| Export used daily | Developer exports each work day | Anecdotal | If the developer reverts to hand-transcribing |
| Billing accuracy | Hours match the developer's expectation | Spot check | If a customer invoice is disputed |

### 6.3 What "Failure" Looks Like

Ships, all ACs pass, but the exported line grouping (repo + task) doesn't match how the developer actually thinks about billable tasks — producing too many tiny lines or over-merging distinct work — so they still hand-edit every export. The load-bearing assumption is that (dominant repo, task) is the right billing unit.

---

## 7. Constraints & Assumptions

### 7.1 Technical Constraints

| Constraint | Rationale | Impact on design |
|-----------|-----------|-----------------|
| Rust core + daemon + Next.js/Bun web (existing) | Established stack | Row computation in `worklog-core`; daemon route; web renders JSON |
| Web reads/writes only via the daemon (no `bun:sqlite`) | WAL stale-read bug under Docker | New `GET /export/{day}` route; web must not read SQLite directly |
| `round_to_half_hour` already exists (Rust + `format.ts`) | Consistency with what UI shows as billable | Reuse it; do not reimplement rounding |
| `union_seconds`/`block_interval` semantics (start+duration, union) — these live in `worklog-cli::cli.rs` | Prevents double-billing | The core billing module reimplements the same union convention (or the helpers are lifted into core); row hours MUST use it, never a naive duration sum |
| Daemon serialises DB access through a single `Mutex<Connection>`; CLI opens its own connection in a single process | No concurrent-writer race on the new `exported_at` marker | No extra locking needed for `mark_exported` |
| CLAUDE.md: never clear/repurpose `tempo_worklog_id`; `estimated_by='manual'` never overwritten | Project invariants | Use a **new** `exported_at`; don't touch the estimator |

### 7.2 Assumptions

| ID | Assumption | Confidence | Owner | How to validate |
|----|-----------|------------|-------|-----------------|
| A-001 | (dominant repo, task) is the correct billing line-item unit | High | Developer | The developer confirmed auto-group by repo+task |
| A-002 | Work=`!is_personal`, Personal=`is_personal` is the right Work/Personal mapping | High | Developer | Matches the example; confirm on first test |
| A-003 | Existing `block.description` is good enough as the billing description; personal blocks needing a description get a deterministic fallback | Medium | Developer | First real export; refine if too technical |
| A-004 | Deterministic description join (no LLM) is acceptable for multi-description groups in v1 | Medium | Developer | First real export |
| A-005 | Decimal comma + `hrs` suffix is the desired text format | Medium | Developer | Matches the example; trivially themeable |
| A-006 | No "edited since exported" (dirty-for-billing) tracking is needed in v1 | Medium | Developer | If re-billing accuracy becomes an issue, add it |

### 7.3 Dependencies

| Dependency | Type | Owner | Status | Risk if delayed |
|-----------|------|-------|--------|-----------------|
| Daemon running (web path only) | Informational | worklog | Available | Web panel shows error; CLI unaffected |

---

## 8. Open Questions

| ID | Question | Impact if unresolved | Owner | Deadline |
|----|---------|----------------------|-------|----------|
| Q-001 | Exact text line format (spacing, `hrs` vs `h`, whether to show the ticket) | Cosmetic; trivially adjustable at the text renderer | Developer | At first test |
| Q-002 | Should the CLI `export` go through the daemon (like `summary`) or read the DB directly (like `sync`)? | Chosen: CLI reads DB directly via core so it works without the daemon; daemon route reuses the same core fn for web | Mr Claude | Resolved |

---

## 9. Revision History

| Version | Date | Author | Changes | Reason |
|---------|------|--------|---------|--------|
| 1.0 | 2026-07-24 | Mr Claude | Initial draft | New billing-export feature after moving off Tempo |

---

## Appendix

### A. Glossary

| Term | Definition in this spec |
|------|-------------------------|
| Billing row / line item | One `{repo, description, hours, type}` entry for a day; the unit the external system bills from |
| Dominant repo | The most-frequent `events.repo` (else folder name of most-frequent `project_path`) across a block's events |
| Task | The grouping key within a repo: `jira_issue` if present, else the block description, else the block id |
| Work / Personal | Work = `!is_personal` (billable); Personal = `is_personal` (non-billable, but still shown in the export) |
| Billed hours | Overlap-safe union seconds of a group's block intervals, rounded to the nearest 0.5h |
| Exported (canary) | `blocks.exported_at` timestamp — set when a day is marked exported; the billing analog of `tempo_worklog_id` |
| Block interval | `[started_at, started_at + duration_seconds)` — the canonical span used everywhere for time math (NOT `ended_at`) |

### B. Mockups

No mockups — behavior is fully specified by the acceptance criteria in Section 3. The Export panel reuses the existing `settings-dialog` modal shell.

### C. Reference Documents

| Document | What it answers | Link |
|----------|----------------|------|
| CLAUDE.md | Stack, invariants (canary, manual-estimate, TZ bucketing) | ./CLAUDE.md |
| tempo.rs | The sync/rollup patterns this mirrors (grouping, round_to_half_hour, LLM join) | rust/crates/worklog-core/src/collectors/tempo.rs |
| personal.rs | `dominant_project_path_for_block` — the template for dominant-repo derivation | rust/crates/worklog-core/src/personal.rs |
| cli.rs `cmd_summary`/`cmd_week` | Per-day rollup + `union_seconds`/`block_interval`/`human_dur` patterns | rust/crates/worklog-cli/src/cli.rs |
