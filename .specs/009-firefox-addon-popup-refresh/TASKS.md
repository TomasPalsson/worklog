Approved: 2026-09-30 by user
# Tasks — Firefox add-on popup refresh
Spec: spec.md · Design: design.md · Base: 1a6034c · Route: dispatch · Test: `cargo test --manifest-path rust/Cargo.toml && bun test extension/firefox`

## Behaviors
| ID | Given / When / Then | Task | Proven by |
|----|---------------------|------|-----------|
| B1 (P0) | Given an active override, when a 06:00 heartbeat arrives, then it is stored | T001 | ingest_stores_outside_hours_while_recording |
| B2 (P0) | Given start Wed 06:00 (or Fri 18:00) and hours Mon-Fri 09:00-17:00, when auto_stop_at runs, then it returns Thu 17:00 (or Sat 17:00) local | T001 | auto_stop_is_next_day_work_end |
| B3 (P0) | Given an expired override, when a heartbeat arrives outside hours, then it is filtered | T001 | expired_override_filters_again |
| B4 (P0) | Given the add-on origin, when it POSTs recording on/off and GETs status, then status reflects it; without origin → 403 | T002 | browser_recording_roundtrip, browser_status_requires_extension_origin |
| B5 (P0) | Given each daemon/paused/skip combination, when viewState runs, then status and primary button follow FR-07/FR-08 precedence | T003 | popup-state.test.js |
| B6 (P1) | Given a skipped tick, when background runs, then lastHeartbeat records the skip reason | T004 | heartbeat.test.js skipReason cases |

## Phase 1 — Daemon recording override
Goal: the daemon can record outside work hours on request, auto-stops next day at work end, and reports status.
Independent test: `cargo test --manifest-path rust/Cargo.toml browser` — green with the add-on untouched.
- [x] T001 Recording override, auto-stop, minutes-today in core (B1, B2, B3); update the one caller at daemon.rs `ingest_heartbeat(c, &hb, &hours, offset)` to pass `None` so the crate compiles — files: rust/crates/worklog-core/src/routing_contract.rs, rust/crates/worklog-core/src/browser_ingest.rs, rust/crates/worklog-core/src/daemon.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core browser_ingest` — done: cf78426
- [x] T002 GET /browser/status and POST/OPTIONS /browser/recording (B4) — files: rust/crates/worklog-core/src/daemon.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core browser_` — after: T001 — done: ad74e67
- [x] T005 [P] NFR Restart: test that an override set on a file-backed db survives closing and reopening the connection (converge: acceptance row unmet) — files: rust/crates/worklog-core/src/browser_ingest.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core recording_override_survives_restart` — after: T001 — done: dac9082

## Phase 2 — Popup
Goal: the popup shows status, counted tab, minutes today and time left, has one context-aware button, and links to the review site — designed with /design:vary, UX first.
Independent test: `bun test extension/firefox` — green with the daemon stubbed.
- [x] T003 [P] Pure popup view-state + formatters (B5) — files: extension/firefox/popup-state.js, extension/firefox/popup-state.test.js — verify: `bun test extension/firefox/popup-state.test.js` — done: 1f89ca0
- [x] T004 Popup UI via /design:vary + background lastHeartbeat + skip reasons (B6) — files: extension/firefox/background.js, extension/firefox/heartbeat.js, extension/firefox/heartbeat.test.js, extension/firefox/popup.html, extension/firefox/popup.js, extension/firefox/popup.css, extension/firefox/README.md — verify: `bun test extension/firefox && npx --yes web-ext lint --source-dir extension/firefox` — after: T002, T003 — done: 0e60923
- [x] CHK001 human-verify the popup end to end outside work hours — files: extension/firefox/popup.html — verify: human: user loads the add-on, presses Start recording, browses ~2 min, sees ~2 min today + time left, Open worklog opens http://127.0.0.1:3333, and says the popup feels clear at a glance — after: T004 — done: 0e60923 by user
- [x] T006 [P] FR-08 overlap status asserts: paused + outside work hours → status "paused"; recording in work hours + incognito (not paused) → "not_counted" (converge: acceptance row partial) — files: extension/firefox/popup-state.test.js — verify: `bun test extension/firefox/popup-state.test.js` — after: T003 — done: bccc6fd

## Gates
- [x] G001 project gates clean — files: . — verify: `flow check --fix` — done: b72a207
- [x] G002 branch review clean — files: . — verify: `flow pass` — done: b72a207
- [x] G003 verification evidence exists — files: . — verify: `test -s verify/` — done: b72a207

## Notes for T004 (UX is the priority — user, mid-spec)
- Run `/design:vary` for the visual direction; the check script must pass (contrast ≥ 4.5:1, light + dark).
- One glance = one answer: the headline status is the largest text; the primary button is the only filled control.
- Every state has words, not just colour. Daemon down shows a fix hint, never a dead button without explanation.
- Button press shows pending state within 150 ms; re-render from the daemon's response, never optimistically lie.
- Keyboard: primary button focused on open, visible focus ring, Enter/Space work, link is a real `<a>`/button.
- Width 280–340 px, no horizontal scroll; long tab titles truncate with ellipsis and full text in `title`.
