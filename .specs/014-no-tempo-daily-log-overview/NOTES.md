Ruling: T002 verify changed from `db_test` (filter matched 0 tests) to `tempo_day_dismissals` (runs the new test) — 2026-10-04
Ruling: T003 verify split into two cargo runs — `cargo test a b` is rejected by cargo (one TESTNAME only) — 2026-10-04
Design: ui.md evaluator round 1 = 3.48 (C), round 2 = 3.65 (C); round-2 fixes 2-5 applied (one 'Short' label, plain Tempo error reasons, today marker, short-day list). Open: render + re-evaluate balance/dark at CHK013 — 2026-10-04
Ruling: T009 files widened (before dispatch) with LoggedHeader.tsx + LoggedEntries.tsx — ui.md §2 header and §6 entry list are shared by all three views; one component each beats three copies — 2026-10-04
Discovered: web test 'log time > submits, refetches and closes' fails on base 67fc0ec too (787/788) — defer
Discovered: T005/T009 tests replaced whole modules with bun mock.module (process-wide) and broke 174 other tests; fixed by spreading the real module — folded into T009 commit
Ruling: T010 files widened (before dispatch) with SettingsPanel.tsx (trigger rendered as a menu link, ui.md §1) and LoggedHeader.tsx (drop its ThemeToggle now AppNav owns it) — 2026-10-04
Ruling: T010 review fix — AppNav imported usePathname from next/dist internals to dodge 6 pre-existing partial next/navigation test mocks; fix the mocks (add usePathname, the repo's 'export every name' convention) and import from next/navigation. T010 files widened with those 6 test files — 2026-10-04
Ruling: T012 files widened with BlockCard.test.tsx — its 'billing move alert' test asserts the customer-move toast that FR-15 removes; flip it to assert no toast (behaviour change per spec, not a weakened test) — 2026-10-04
Discovered: CHK013 live check — Logged links render browser-blue/underlined, source badges stretch full row, AppNav theme tooltip overflows the viewport (page scrolls sideways), week rows show '0h of 0h' on days off — fold into one CSS/markup fix before CHK013 tick
Ruling: TaskWorkLog 'submits, refetches and closes' expected todayISO() but the form (correctly) uses the fixture's daemon today 2026-10-02 — test pinned to the fixture day; deterministic, not weakened — 2026-10-04
Review (gating): 5 lenses over 67fc0ec..c63b661 — 0 fatal, 0 significant, 12 minor; blind re-score max 75 (F10 dismiss no-Tempo test mock not wired), all 12 dropped (<80). Converge pass: no unmet work — 2026-10-04
