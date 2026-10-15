# Prep — Pin ticket hours and never round billable time down
Gathered: 2026-10-15 · Questions: 4 of 12 · Route: dispatch · Status: ready for spec

## Decisions
- D-01 "Put 0,5h / 1h on this ticket" pins hours to the ticket's Tempo line for that day (ticket + day), not to one session; two sessions on the same ticket that day share the number. — user, Q0
- D-02 The pinned hours go to Tempo only; the billing export keeps the measured time. — user, Q0
- D-03 Billable hours are never rounded down, under any circumstances. — user, Q0
- D-04 The only non-billable tickets are those whose account is APRÓ; who created the ticket does not matter. — user, Q0
- D-05 Update the worklog skill (skills/worklog/SKILL.md) so Claude knows how to pin hours. — user, Q0
- D-06 The billing export (billing.rs / billing_registry.rs) is not changed at all; the Owner does not use it. Never-round-down applies to Tempo lines only. — user, Q1
- D-07 A ticket is APRÓ (non-billable) when its Mirres customer is APRÓ ("APRÓ ehf.", accent/case-insensitive) or its Tempo account key starts with `APRO-`; project names are ignored; a line with no Mirres data is billable. — user, Q2
## Not this
- No change to the billing export (billing.rs, billing_registry.rs). — user, Q1
- No new web UI control; the existing Tempo-line hours edit is how the pin is changed in the browser. — user, Q3
- No change to the Claude Code ticket picker mod (mods/worklog/). — user, Q3
- No change to the legacy one-entry-per-block sync path (collectors/tempo.rs `sync_block_legacy`) for already-synced days. — user, Q3
## Discretion
- CLI flag names and help text for the new pin command.
- Accepting "0,5" as well as "0.5"; rejecting values that are not a multiple of 0.5h with a clear error.
- Error copy when the ticket has no line on that day.
## Assumptions
- A-01 The pin reuses `tempo_line_texts.hours_override_seconds` (CHECK multiple of 1800, > 0), already editable in the web UI, and `effective_seconds = override.unwrap_or(union)` — evidence: rust/crates/worklog-core/sql/schema.sql:357-366, rust/crates/worklog-core/src/tempo_lines.rs:104-122 — confidence: high — unconfirmed
- A-02 No CLI command sets line hours today; only daemon `POST /tempo/lines/hours` → `tempo_lines::set_hours`. A new `worklog ticket hours <KEY> <hours> [--day] [--clear]` is needed — evidence: rust/crates/worklog-core/src/daemon.rs:255, rust/crates/worklog-core/src/tempo_lines.rs:217 — confidence: high — unconfirmed
- A-03 The only rounding-down today is the 15-minute floor: `round_to_half_hour` returns 0 under 900s, otherwise rounds up — evidence: rust/crates/worklog-core/src/billing_round.rs:7-12 — confidence: high — unconfirmed
- A-04 A ticket is APRÓ when its Mirres customer is APRÓ ("APRÓ ehf.", accent/case-insensitive) or its account key starts with `APRO-`; project names are ignored, because other customers' projects are named "APRÓ Vitinn …" — evidence: mirres_line_billing rows (e.g. BYKO-AIHRADALL-SERV.CODEINTERPRETER · "Byko · APRÓ Vitinn"), rust/crates/worklog-core/src/tempo_line_contract.rs:111-125 — confidence: medium — confirmed Q2
- A-05 A line with no Mirres data counts as billable (D-04: only APRÓ is non-billable) — evidence: rust/crates/worklog-core/src/mirres.rs:296-306 — confidence: high — confirmed Q2
- A-06 When the Owner names no ticket, Claude uses the session's pinned ticket (`session_tickets`) — evidence: rust/crates/worklog-core/src/session_tickets.rs:12-27 — confidence: high — unconfirmed
- A-07 The pin day defaults to today in `$WORKLOG_TZ` — evidence: CLAUDE.md Conventions (day bucketing) — confidence: high — unconfirmed
## Verify
- Tell Claude "put 0,5h on this ticket" → the web UI's Tempo line for that ticket today shows 0.5h; a 10-minute line on a non-APRÓ ticket shows 0.5h instead of 0; `cargo test --manifest-path rust/Cargo.toml` is green. — user, Q4
## Open
