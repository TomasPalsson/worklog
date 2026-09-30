# Prep — Firefox add-on popup refresh
Gathered: 2026-09-30 · Questions: 5 of 12 · Route: dispatch · Status: ready for spec

## Decisions
- D-01 "Start recording now" is a start/stop switch that records browser time even outside work hours; if not stopped, it auto-stops at 17:00 the next day. — user, Q1
- D-02 Popup shows all four: status (recording / outside work hours / paused / daemon not running), the tab being counted, minutes counted today, time left until auto-stop. — user, Q2
- D-03 One button replaces the "Paused" checkbox: inside work hours it reads "Pause" (and "Resume" when paused); outside work hours it reads "Start recording" (and "Stop" while recording). — user, Q3
## Not this
- No editing work hours from the popup — stays in the web UI Settings. — user, Q4
- No Chrome/Chromium version of the add-on. — user, Q4
- No signing or publishing the add-on to addons.mozilla.org. — user, Q4
- No history or charts in the popup — today only. — user, Q4
## Discretion
- Visual design: must look clearly more polished than the bare checkbox (user screenshot, Q2); match the worklog web UI's look. Ungrillable — build decides, or run /design:vary.
## Assumptions
- A-01 Popup today is one "Paused" checkbox, 160px wide, no status shown — evidence: extension/firefox/popup.html:19 — confidence: high — confirmed Q2
- A-02 Daemon drops heartbeats outside work hours (default Mon-Fri 09:00-17:00), so "start recording now" needs a daemon change, not just a popup change — evidence: rust/crates/worklog-core/src/browser_ingest.rs:99, rust/crates/worklog-core/src/routing_contract.rs:21 — confidence: high — confirmed Q1
- A-03 Add-on never learns if a heartbeat was stored or filtered; fetch errors are swallowed — evidence: extension/firefox/background.js:42-50 — confidence: high — unconfirmed
- A-04 Worklog review site is at http://127.0.0.1:3333 — evidence: web/docker-compose.yml:22 — confidence: high — confirmed Q5
- A-05 Daemon only accepts heartbeats with a moz-extension:// Origin — evidence: rust/crates/worklog-core/src/daemon.rs:2322 — confidence: high — unconfirmed
- A-06 "17:00" = the configured work-hours end time, in $WORKLOG_TZ — evidence: none — confidence: medium — unconfirmed
## Verify
- Outside work hours: open the popup, press "Start recording", browse ~2 min; popup shows ~2 min counted today and the time left until auto-stop; "Open worklog" link opens http://127.0.0.1:3333. `cargo test --manifest-path rust/Cargo.toml` and the add-on tests (extension/firefox/*.test.js) are green. — user, Q5
## Open
- Q: does a work-hours Pause auto-resume the next work day, or stay paused until pressed? → deferred to spec
- Q: A-06 — is the auto-stop the configured work-hours end or literally 17:00? → deferred to spec
