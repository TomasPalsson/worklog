# Acceptance — 025 pin ticket hours

Run 2026-10-15 at HEAD (after the G002 review fix).

| Criterion | Command | Exit | Showed |
|---|---|---|---|
| FR-01 APRÓ line <15 min stays 0; ≥15 min rounds as before | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- apro_lines_keep_the_fifteen_minute_floor is_apro_rejects_lookalikes rounds_up_to_half_hour_with_zero_floor_below_15_minutes` | 0 | 3 passed (APRÓ spellings + APRO- key → 0; Aprosoft/Aprocon/Aprilia not APRÓ; old rounding table unchanged) — verify/accept-FR-01.log |
| FR-02 non-APRÓ line 1–899 s → 1800 | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- short_work_on_a_billable_line_rounds_up_to_half_an_hour project_name_never_makes_a_line_apro billable_rounding_never_drops_tracked_time_to_zero` | 0 | 3 passed (no Mirres data, other customer, "APRO · Internal" project → 1800; 1→1800, 899→1800, 1801→3600) — verify/accept-FR-02.log |
| FR-03 `worklog ticket hours` pins / clears / refuses (core) | `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib -- pin_hours` | 0 | 6 passed (pin before blocks, re-pin, clear, refuse 0/-1800/2700/1, synced blocks go dirty) — verify/accept-FR-03a.log |
| FR-03 `worklog ticket hours` CLI parses 0,5 / 0.5 / --clear / --json | `cargo test --manifest-path rust/Cargo.toml -p worklog-cli --test cli -- ticket_hours` | 0 | 3 passed — verify/accept-FR-03b.log; sandbox run in verify/CHK004.md |
| FR-04 worklog skill teaches the pin, no confirm | `bash -c 'grep -q "worklog ticket hours" skills/worklog/SKILL.md && grep -q "worklog ticket hours" skills/worklog/references/jira.md'` | 0 | both files carry the recipe — verify/accept-FR-04.log |
| FR-05 web UI Tempo line shows the pinned 0.5h | see verify/CHK004.md | human | sandbox: pin → dry-run sends 1800s from the same lines_for_day the UI reads; live Claude+browser loop not run (needs install after merge) |
