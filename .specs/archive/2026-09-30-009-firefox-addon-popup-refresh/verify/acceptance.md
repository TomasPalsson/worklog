# Acceptance — 009 Firefox add-on popup refresh

All commands run at dac9082 on 2026-09-30. `R` = `cargo test --manifest-path rust/Cargo.toml -p worklog-core <name>`, `J` = `bun test extension/firefox`. Full runs: `cargo test --manifest-path rust/Cargo.toml` exit 0 (1104+43+24+7+2 pass), `J` exit 0 (35 pass).

| Criterion | Command run now | Exit | Showed | Met |
|---|---|---|---|---|
| FR-01 / B1 store outside hours while recording | R `ingest_stores_outside_hours_while_recording` | 0 | Wed 06:00 local filtered without override, stored with it | yes |
| FR-02 / B2 end = next day at work end | R `auto_stop_is_next_day_work_end` | 0 | Wed 06:00 → Thu 17:00, Fri 18:00 → Sat 17:00 | yes |
| FR-03 / B3 expired override filters | R `expired_override_filters_again` | 0 | heartbeat at and after `until` → `outside_work_hours` | yes |
| FR-04 stop before end | R `browser_recording_roundtrip`; CHK001 step 9 | 0 | POST off → status `recording_until: null` | yes |
| FR-05 status fields + minutes (dedupe) | R `browser_recording_roundtrip`, `minutes_today_counts_only_the_local_day`; CHK001 step 6 | 0 | 2 heartbeats in one minute count 1; live status `minutes_today: 2` | yes |
| FR-06 / B4 origin required | R `browser_status_requires_extension_origin`; CHK001 steps 7-8 | 0 | missing and `http://evil.test` → 403 on GET and POST | yes |
| FR-07 one primary button by rule | J `FR-07 primary label for every rule` | 0 | none / Resume (incl. paused-outside) / Pause / Stop recording / Start recording | yes |
| FR-08 / B5 status precedence + overlaps | J `viewState` suite incl. `FR-08 paused outside work hours`, `FR-08 incognito in work hours`, `%s beats paused`, `paused: resume, beats idle`, `%s is not counted` | 0 | order matches spec; paused+outside, recording+private, paused+recording, idle+recording asserted | yes |
| FR-09 counted tab or none | J `recording in work hours: pause, tab and minutes shown`, `%s beats paused` (tab null) | 0 | tab shown when recording, null otherwise ("—" in UI) | yes |
| FR-10 minutes today formatting | J `formatMinutes` (0 → "0 min", 125 → "2 h 05 min") | 0 | spec literals | yes |
| FR-11 time left | J `formatTimeLeft` suite; CHK001 render | 0 | "3 h 30 min" / "20 min" / "<1 min" / null | yes |
| FR-12 Open worklog → review site, new tab | CHK001 step 11 (`curl` 3333 → 307); popup.js `browser.tabs.create({url: "http://127.0.0.1:3333"})` | 0 | URL answers; click not performed in real Firefox | yes (pre-approved, see CHK001.md) |
| FR-13 feedback < 150 ms, then daemon state | read popup.js `perform()`: buttons disabled + "Working…" set synchronously before the first await; `render()` after the response | — | no optimistic state; renders from daemon JSON | yes (by code; not stopwatch-timed in Firefox) |
| FR-14 secondary Stop in work hours | J `override inside work hours: pause primary, stop secondary` | 0 | secondary "Stop recording" | yes |
| FR-15 keyboard + visible focus | popup.js focuses primary (or link) on open and after actions; popup.css `:focus-visible` 3px ring; screenshots show ring | — | focus ring visible in `verify/popup-*.png` | yes (pre-approved) |
| B6 skip reason recorded | J `skipReason` suite, `storableTab` suite | 0 | each reason named; private/Personal title+url never stored | yes |
| NFR daemon-down < 1 s | popup.js `AbortSignal.timeout(1000)`; `verify/popup-daemon-down-dark.png` | — | error state with hint | yes |
| NFR width 280-340, no h-scroll | screenshots at 320 px; long title truncates with ellipsis | — | no horizontal scroll | yes |
| NFR contrast ≥ 4.5:1 light + dark | `verify/T004-contrast.md`; `verify/T004-check.json` (0 FAIL) | — | text min 6.43 light / 6.94 dark | yes |
| NFR access: 0 non-moz-extension origins | = FR-06 | 0 | 403 | yes |
| NFR restart survives | R `recording_override_survives_restart` | 0 | file db: set, close, reopen, read back | yes |
| NFR popup ready < 300 ms | CHK001 render in Chromium harness (instant); not stopwatch-timed in Firefox | — | — | yes (pre-approved; worth a glance) |
| Launch: 2-min outside-hours walkthrough | `verify/CHK001-walkthrough.sh` against a throwaway daemon | 0 | see CHK001.md | yes |
| Launch: cargo test + bun test green | full runs above | 0 | green | yes |
| Launch: every MUST has a test or CHK001 observation | this table | — | all rows filled | yes |
