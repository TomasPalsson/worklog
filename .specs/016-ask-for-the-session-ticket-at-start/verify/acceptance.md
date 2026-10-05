# Acceptance — 016 ask for the session ticket at start

Run 2026-10-05 at b383d00. `M` = `claude plugin test mods/worklog` (109 pass, 0 fail). `H` = `cargo test --manifest-path rust/Cargo.toml -p worklog-cli ticket_hint` (5 pass).

| Criterion | Command | Exit | What it showed |
|---|---|---|---|
| FR-01 / B2 one question at start | M | 0 | "a fresh work session asks one ticket question…", "session start returns before the Owner answers"; real dialog in verify/CHK001.md |
| FR-02 / B3 no question outside work, `-p`, stored answer | M | 0 | "a non-interactive start asks nothing", "outside a work folder asks nothing", "a stored answer for the session id asks nothing" |
| FR-03 / B7 ask again after `/clear` | M | 0 | "clear asks once for the new session id", "clear drops the old recorded key and hand-off…" |
| FR-04 / B1 ≤4 choices in order | M | 0 | lib.test.ts `ticketChoices:` tests (7) + real dialog order in verify/CHK001-dialog.txt |
| FR-05 / B4 pick or type key records it | M | 0 | "picking the branch ticket records it…", "picking a recent ticket label records its key", "typed text that is exactly a key records it" |
| FR-06 / B5 Skip/Esc/non-key records nothing, no second ask | M | 0 | "Skip records nothing", "a dismissed dialog records nothing…not asked again", "Skip stores the answer…"; real Skip in CHK001 |
| FR-07 / B6 Create hand-off on each prompt until recorded | M | 0 | "Create hands off on every prompt…", "Claude running ticket use for this session records the key and ends the hand-off" (T006) |
| FR-08 / B8 chosen beats branch in context + commit/PR | M | 0 | "a recorded key replaces the branch ticket in context and attribution…", "the recorded key survives the post-turn branch refresh" |
| FR-09 / B10 hint never asks | H | 0 | `ticket_hint_never_asks` + 4 case tests |
| FR-10 / B11 non-key text → find hand-off | M | 0 | "non-key Other text hands off find with that text" |
| B9 / NFR daemon down or slow, cap 1.5 s | M | 0 | "daemon down still offers…", "the tasks lookup is capped at 1500 ms…" |
| NFR questions per session ≤ 1 | M | 0 | "the answer is stored under the session key so a second start asks nothing" |
| NFR 0 new network targets | `grep -n "fetch\|http" mods/worklog/hooks/ticket.ts` | 0 | only `$.http.fetch` via `daemonGet` (127.0.0.1:9323) |
| Launch: real session, pick, `worklog block list` | human | human | verify/CHK001.md — dialog + Skip seen live; pick → block list NOT done live (would write to the real DB); Owner to confirm on next real session |
