# Spec: Firefox add-on popup refresh

**Created**: 2026-09-30 · **Route**: dispatch

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: The Worklog Firefox add-on popup is one "Paused" checkbox. The Owner can't tell if browsing is being counted, and time outside work hours (e.g. 05:30) is silently thrown away with no way to opt in.

**Solution**: The popup shows what is happening right now (status, counted tab, minutes today, time left), offers one context-aware button (Pause / Start recording), and links to the review site.

**Who it's for**: The Owner — the one person who runs the worklog daemon and the add-on on their own machine.

**MVP cut line**: everything tagged `MUST` in §4.1 ships. `SHOULD` is v1.1. `MAY` is backlog.

**Key decision**: "Start recording" is a daemon-held override of the work-hours filter that auto-stops the next calendar day at the configured work-hours end — 17:00 by default (D-01), so the daemon stays the single enforcer of work hours.

## 1. Context

### 1.1 Problem statement

The Owner started working at 05:30 in Firefox and nothing was logged. The daemon drops heartbeats outside Mon-Fri 09:00-17:00, and the add-on never shows whether a heartbeat was stored, filtered, or failed. The only control is a checkbox.

**Current workaround**: Change `WORKLOG_WORK_HOURS` in the review UI Settings, then change it back.

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Owner | Browses for work, opens the popup to check or change recording | Glances for 2 seconds; must understand the state without reading |

**Primary actor**: Owner.
**Hidden stakeholders**: Billing export — extra firefox events outside work hours feed block estimation.

## 2. Scope

### 2.1 In scope

- A "Start recording" override of the work-hours filter, with a next-day 17:00 auto-stop (D-01).
- One context-aware button replacing the "Paused" checkbox (D-03).
- A status panel: status, counted tab, minutes today, time left until auto-stop (D-02).
- An "Open worklog" link to the review site.
- A redesigned popup look, produced with `/design:vary`, with UX as the top priority.

### 2.2 Non-goals

> Binding. A change here is an amendment, not an interpretation.

- No editing work hours from the popup — stays in the web UI Settings. — user, Q4
- No Chrome/Chromium version of the add-on. — user, Q4
- No signing or publishing the add-on to addons.mozilla.org. — user, Q4
- No history or charts in the popup — today only. — user, Q4

## 3. Journeys

### Journey 1 — Record outside work hours (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | 06:00 on a weekday, daemon running | Owner opens popup, presses "Start recording", browses 2 min | Status reads "Recording"; minutes today rises by ~2; time left reads until tomorrow 17:00 |
| Error | Daemon not running | Owner opens popup | Status reads "Worklog isn't running" with a one-line hint; the button is disabled, not silently broken |
| Edge | Recording started yesterday 06:00, never stopped | Clock passes today 17:00 | Heartbeats outside work hours are filtered again; popup shows "Outside work hours" and "Start recording" |

### Journey 2 — Pause during work hours (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | 10:00 weekday | Owner presses "Pause" | Status reads "Paused"; no heartbeats are sent; button reads "Resume" |
| Error | Paused, daemon stops | Owner opens popup | Status reads "Worklog isn't running"; paused state is kept |
| Edge | Private window or "Personal" container focused | Owner opens popup | Status says this tab is not counted and why |

## 4. Requirements

### 4.1 Functional requirements

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | The daemon MUST store heartbeats outside work hours while a recording override is active | Unit test: heartbeat at 06:00 with override → stored; without → filtered `outside_work_hours` |
| FR-02 | MUST | Starting a recording MUST set its end to the next calendar day at the work-hours end time, in the configured day offset — even when that day is not a work day | Unit tests: start Wed 06:00 → Thu 17:00; start Fri 18:00 → Sat 17:00 |
| FR-03 | MUST | An override past its end MUST no longer bypass the filter | Unit test: heartbeat after end → filtered |
| FR-04 | MUST | The Owner MUST be able to stop a recording before its end | Handler test: stop → status shows no override |
| FR-05 | MUST | The daemon MUST report status: in work hours, override end, minutes counted today (distinct firefox minutes on the local day), work-hours string | Handler test on the status endpoint; unit test: two heartbeats in one minute count as 1 |
| FR-06 | MUST | The recording and status endpoints MUST reject requests without an add-on origin | Handler tests: missing origin → 403; `Origin: http://evil.test` → 403 |
| FR-07 | MUST | The popup MUST show exactly one primary button (D-03), chosen by the first rule that matches: daemon down → none (disabled, with hint); paused → "Resume"; inside work hours → "Pause"; recording active → "Stop recording"; else → "Start recording" | Unit test per rule, incl. paused-outside-hours → "Resume" |
| FR-08 | MUST | The popup MUST show one status (D-02), the first that matches: Worklog isn't running → Not counted (private window / Personal container) → Paused → Not counted (idle / window unfocused) → Recording (in work hours, or override active) → Outside work hours | Unit test per status plus the overlaps: paused+outside, recording+private, paused+recording, idle+recording |
| FR-09 | MUST | The popup MUST show the tab currently being counted, or say none is | Unit test |
| FR-10 | MUST | The popup MUST show minutes counted today | Unit test on formatting ("0 min", "1 h 05 min") |
| FR-11 | MUST | The popup MUST show time left until auto-stop while a recording is active | Unit test on formatting |
| FR-12 | MUST | The popup MUST offer an "Open worklog" link that opens the review site in a new tab | human: CHK001 |
| FR-13 | MUST | A button press MUST give visible feedback within 150 ms and the new state after the daemon answers | human: CHK001 |
| FR-14 | SHOULD | While a recording is active during work hours, the popup SHOULD offer a secondary "Stop recording" action | Unit test on view-state |
| FR-15 | SHOULD | The popup SHOULD be fully usable by keyboard with visible focus | human: CHK001 |

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| Popup ready | status visible < 300 ms after open with daemon up | Manual stopwatch in CHK001 |
| Daemon-down detection | error state shown < 1 s | Status fetch timeout 1000 ms in code |
| Popup width | 280–340 px, no horizontal scroll | CHK001 |
| Text contrast | ≥ 4.5:1 in light and dark | `/design:vary` check script |
| Access | 0 endpoints accept a non-`moz-extension://` Origin | FR-06 handler tests |
| Restart | override survives 1 daemon restart (stored in `meta`) | Unit test: set, reopen connection, read back |

## 6. Launch criteria

- [ ] Outside work hours: open the popup, press "Start recording", browse ~2 min; popup shows ~2 min counted today and the time left until auto-stop; "Open worklog" opens http://127.0.0.1:3333. — user, Q5
- [ ] `cargo test --manifest-path rust/Cargo.toml` and `bun test extension/firefox` are green. — user, Q5
- [ ] Every MUST in §4.1 has a passing test or a CHK001 observation.

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | Review site is at http://127.0.0.1:3333 (web/docker-compose.yml:22) | High | Link opens the wrong page |
| A2 | Only the add-on may call the new endpoints; the moz-extension Origin check is enough auth (daemon.rs:2322) | High | Any local page could start recording |
| A3 | "17:00" means the configured work-hours end time, next calendar day, in `$WORKLOG_TZ` | Medium | Auto-stop fires at the wrong hour |
| A4 | Pause stays until the Owner presses Resume (today's behaviour) | Medium | Owner forgets and loses a day |
| A5 | One firefox event = one counted minute (browser_ingest.rs dedupes per minute) | High | Minutes today is wrong |
| A6 | The override is stored in the `meta` table, survives daemon restarts | High | Restart silently ends recording |

## 8. Open questions

None blocking. A3 and A4 are defaults the build uses; the Owner may overturn either with `/flow:spec --amend`.

## Appendix A — Glossary

| Term | Means |
|------|-------|
| Heartbeat | One POST from the add-on per minute describing the focused tab |
| Override / recording | The daemon-held "store even outside work hours until <end>" state |
| Work hours | `WORKLOG_WORK_HOURS`, default `Mon-Fri 09:00-17:00` |
