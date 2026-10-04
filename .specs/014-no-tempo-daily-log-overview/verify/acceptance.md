# Acceptance — 014 Logged (run 2026-10-04 @ 3a1b869)

Commands re-run now unless marked "live" (CHK013.md, same day, branch daemon + web on a DB copy).

| criterion | command run now | exit | showed | met? |
|---|---|---|---|---|
| B1/B2 FR-09/FR-10 flag rule | `cargo test --manifest-path rust/Cargo.toml logged_contract` | 0 | 7 passed | yes |
| B3 FR-13 not fetched ≠ 0h | `cargo test … logged::tests` | 0 | 3 passed (empty store → not_fetched; weekend 0-row → off; dismiss/undismiss) | yes |
| B4 FR-07 re-fetch replaces range | `cargo test … tempo_remote` | 0 | 6 passed | yes |
| B5–B8, B13 routes, 42-day cap, reason 1–80, no Tempo on dismiss, undismiss | `cargo test … daemon_logged` | 0 | 11 passed | yes |
| dismissals table, no schema bump | `cargo test … tempo_day_dismissals` | 0 | 1 passed | yes |
| B9/B10 FR-01–06, FR-08, FR-11 views, auto-fetch, refresh, errors, dismiss UI | `cd web && bun test components/Logged.test.tsx app/actions-logged.test.ts lib/format.test.ts …` | 0 | 129 tests across 8 files, 0 fail | yes |
| B11 FR-14 one menu, current marked | same run (AppNav.test.tsx) | 0 | in the 129 | yes |
| B12 FR-15 no billing on Day/Week/Tasks | same run (DayHeader, TaskHours, ChangeNotices, BlockCard) + live DOM scan | 0 | in the 129; live: only the menu's Billing link | yes |
| FR-16 /billing keeps working, export moved there | live `/billing?from=2026-10-02` | — | menu marks Billing; "Open billing export" present; one theme button | yes (live) |
| FR-18 month total vs required | live month header | — | "9h logged · 172.7h required" | yes (live) |
| §6 all suites green | see PASS-b2934dc.md (cargo test/clippy/fmt, bun test/typecheck/build) | 0 | all green | yes |
| V-01 a: Logged opens on this month and fetches by itself | live | — | redirect to 2026-10; "From Tempo · updated 14:52" | yes (live) |
| V-01 b: **every day's hours match Tempo's website** | — | — | Numbers come straight from Tempo's API via the new pull, but the model cannot open tempo.io, and a direct Tempo API cross-check was blocked (it would read the user's keychain login). | **NO — needs the user** |
| V-01 c: under-target day flagged | live | — | 28–30 Sep, 1 Oct "Short" | yes (live) |
| V-01 d: "dentist" survives reload | live | — | "Marked fine: dentist" after hard reload | yes (live) |
| V-01 e: Day page shows no Billing | live | — | only the menu link | yes (live) |

**Result: one row unmet (V-01 b). `Verified:` is NOT written.** It needs a person's eyes on Tempo's website; no task can close it.
