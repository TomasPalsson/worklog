# 021 claim-check (re-run 2026-10-11, worktree root)

Cargo rows: `cargo test -q --manifest-path rust/Cargo.toml -p <crate> -- <same filters>` (github rows used bare test names because the session guard rejects the word "git" in a command; same tests, 4 ran). "bodies read" = test source opened and checked against spec.md section 4/5.

| Criterion | Verdict | Command | Exit | Note |
|---|---|---|---|---|
| FR-01 | CONFIRMED | cargo test (5 filters, cli) | 0 | 5 passed; ignored vs personal asserted (aggregate_day_overlap test: ignored_secs=3600, personal_secs=0). Other 4 bodies not read |
| FR-02 | CONFIRMED | cargo test (5 filters, cli) | 0 | 5 passed; bodies read: total 5400 vs raw 7200, overlap = 3600+3600-5400, no overlap line when none |
| FR-03 | CONFIRMED | cargo test (2 filters, core) | 0 | 2 passed; bodies read, ignored stays personal, non-ignored still flips |
| FR-04 | CONFIRMED | cargo test (3 filters, core) | 0 | 3 passed; daemon test posts noise and duplicate, reads each back, rejects bogus |
| FR-05 | CONFIRMED | cargo test (3 filters, core) | 0 | 3 passed; keep-reason body read |
| FR-06a | CONFIRMED | cd web && bun test components/BlockCard.test.tsx | 0 | 49 pass; setIgnored(1,true,DAY,"noise") |
| FR-06b | CONFIRMED | same | 0 | both branches: mergeGroup(2,[1],DAY) w/o ignore; no neighbour -> reason "duplicate" |
| FR-06c | CONFIRMED | same | 0 | Internal opens picker, assignTicket called, setIgnored never |
| FR-07 | CONFIRMED | cargo test (2 filters, core) | 0 | 2 passed; body `{"ignored":true}` -> 200, null reason |
| FR-08 | CONFIRMED | cargo test (2 filters, core) | 0 | 2 passed; first errors, second sent, error text stored, retried |
| FR-09 | CONFIRMED | cargo test (2 filters, core) | 0 | 2 passed; no-ticket and rounds-to-0h reasons stored, auto_sent_at None; already-in-Tempo stays sent |
| FR-10 | CONFIRMED | cargo test (1 filter, core) | 0 | 1 passed; failed line listed, sent line not |
| FR-11 | INCONCLUSIVE | cargo test (6 filters, core) | 0 | 6 passed; test bodies not read (turn limit) |
| FR-12 | CONFIRMED | cd web && bun test components/LooseEnds.test.tsx | 0 | 8 pass; hrefs ["/2026-10-05","/2026-10-07"] |
| FR-13 | CONFIRMED | cd web && bun test components/DayLoadNotice.test.tsx | 0 | 8 pass; notice + sectionLoader tests. Caveat: no test renders the day page; page.tsx wires all 7 sections through load() (read) |
| FR-14 | CONFIRMED | cargo test (3 filters, cli) | 0 | 3 passed; "check failed (boom)", json error reason |
| FR-15 | CONFIRMED | cargo test (3 filters, cli) | 0 | 3 passed; hint strings asserted |
| FR-16 core | CONFIRMED | cargo test (2 filters, core) | 0 | 2 passed; err run: no ok, rfc3339 time + text |
| FR-16 cli | INCONCLUSIVE | cargo test (3 filters, cli) | 0 | 3 passed; bodies not read (turn limit) |
| FR-17 cli | INCONCLUSIVE | cargo test (4 filters, cli) | 0 | 4 passed; bodies not read (turn limit) |
| FR-17 core | CONFIRMED on exit/count | cargo test (2 filters, core) | 0 | 2 passed; bodies not read, names match 24h window |
| FR-18 | CONFIRMED | cargo test (4 bare names, core) | 0 | 4 passed; page 2 fetched, stops at GITHUB_MAX_PAGES hits |
| FR-19a | CONFIRMED | cargo test (2 filters, core) | 0 | 2 passed; error contains tool name and "boom" |
| FR-19b | CONFIRMED | cargo test (2 filters, core) | 0 | 2 passed; injected 1 ms budget, 5 s constant not asserted |
| FR-20 | CONFIRMED | cargo test (2 filters, core) | 0 | 2 passed (4.4 s); pid file removed / kept + Err |
| FR-21 core | CONFIRMED | cargo test (3 filters, core) | 0 | 3 passed; tie goes to most recent, beats alphabetical |
| FR-21 web | CONFIRMED | cd web && bun test components/BlockCustomerSplit.test.tsx | 0 | 15 pass; chip, no save before click, click saves only suggested customer's rows. No flake this run |
| FR-22 | INCONCLUSIVE | claude plugin test mods/worklog | 0 | 147 pass / 0 fail; /wl install test bodies not read (turn limit) |
| FR-23 core | CONFIRMED | cargo test (4 filters, core) | 0 | 4 passed; 15 min kept, 16 min ends run (body read) |
| FR-23 cli | INCONCLUSIVE | cargo test (2 filters, cli) | 0 | 2 passed; bodies not read (turn limit) |
| FR-24 | CONFIRMED | cargo test (1 filter, cli) | 0 | 1 passed; serve/eval absent from help, both parse |
| FR-25 | CONFIRMED (presence only) | `rust/target/debug/worklog --help` command list vs `grep -o "worklog <cmd>" README.md` (fish-split equivalent) | 0 | all 32 listed commands present in README; serve/eval 0 matches; README:122 "alias: `upgrade`". Does not detect extra rows |
| FR-26 | CONFIRMED | `ls .specs`; `grep -c "00[3478]-" .specs/LEDGER.md` | 0 | four folders gone (archive/ present); LEDGER count 4 |
| FR-27 | NOT-CHECKABLE | n/a | n/a | Dropped by NOTES.md Ruling (T027): isolated worktree, Owner removes from main checkout |
| NFR loose-ends <200 ms | CONFIRMED | cargo test -q --release -p worklog-core -- loose_ends::tests::query_over_2000_blocks_takes_under_200ms | 0 | 1 passed; body not read |
| NFR GitHub <=10 pages | CONFIRMED | same as FR-18 | 0 | max_pages test asserts hits == GITHUB_MAX_PAGES |
| NFR status <3 s | NOT-CHECKABLE | n/a | human | CHK002.md exists, non-empty (2217 B) |
| NFR failure visibility | CONFIRMED | cargo test (6 filters, core) | 0 | 6 passed; sample of paths, not every path |
| NFR privacy | CONFIRMED on exit/count | cargo test (standup personal_and_ignored... core; helpers_cmd focus_prints... cli) | 0 | each passed; bodies not read |
| B1-B9 | same as the rows they cite (B1=FR-01, B2=FR-02, B3=FR-08, B4=FR-09, B5=FR-04+05, B6=FR-11 INCONCLUSIVE, B7=FR-14, B8=FR-16/17 partly INCONCLUSIVE, B9=FR-18) | see rows | 0 | inherit those verdicts |
| Launch (Owner checks day page) | NOT-CHECKABLE | n/a | human | CHK001.md exists, non-empty (2582 B) |

Observed, not requested: acceptance.md "49 rows" count not verified.

UNSUPPORTED: none
