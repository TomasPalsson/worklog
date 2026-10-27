# claim-check — 028 acceptance + PASS-45df725

Re-run 2026-10-20 at HEAD 72db0de. Code is identical to 45df725 (`git diff --stat 45df725 HEAD` touches only 4 files under .specs/). Exit read from `<cmd> > log 2>&1; echo $?`, no pipe.

## acceptance.md (12 rows; each: exit 0, `1 passed; 0 failed`, 2358 filtered out)

| Criterion | Re-run | Test body exercises the criterion? | Verdict |
|---|---|---|---|
| FR-01 | exit 0, 1 passed | Yes: loops all 8 Appendix B good texts, `validate(text) == Ok(text)` | CONFIRMED |
| FR-02 | exit 0, 1 passed | Yes: 8 phrases x (as-is, UPPERCASE); asserts reason contains "vísbend" | CONFIRMED |
| FR-03 | exit 0, 1 passed | Yes: "five point five", "sex komma fimm" rejected; "five points", plain number words pass | CONFIRMED |
| FR-04 | exit 0, 1 passed | Yes: Slack/slack/SLACK rejected, "slackers-lista" passes | CONFIRMED |
| FR-05 | exit 0, 1 passed | Yes: "Mældu ..." rejected; only first word counts; 3-letter boundary | CONFIRMED |
| FR-06 Sjúkra/VÍS/BYKO re-ask | exit 0, 1 passed | Yes: ticket_customer "Sjúkra", registry has VÍS(VIS) + BYKO; reply names "Vis, Byko og Sjúkra"; asserts 2 calls and re-ask contains "nefnir annan viðskiptavin: BYKO, VÍS"; final text is the clean reply | CONFIRMED |
| FR-06 (rule) | exit 0, 1 passed | Yes: `foreign_customers(text, Some("Sjúkra"), registry)` == ["BYKO","VÍS"] | CONFIRMED |
| FR-07 | exit 0, 1 passed | Yes: asserts 4 phrases (clues, colleagues/Slack, digits, own customer) are in SYSTEM_PROMPT_IS | CONFIRMED |
| FR-08 | exit 0, 1 passed | Yes: manual save "Unnu í Slack rás" keeps text exactly and status NeedsLook (uses a Slack text, not literally B-bad 1; the criterion text only requires a rule-breaking text) | CONFIRMED |
| FR-09 manual | exit 0, 1 passed | Yes: summary "Hertz - Greiningarfundur" seeded in jira_tickets; text "hertz - greiningarfundur" (case differs, allowed by FR-09 "ignoring case and spaces"); NeedsLook | CONFIRMED |
| FR-09 meeting | exit 0, 1 passed | Yes: meeting titled exactly "Hertz - Greiningarfundur" on a ticket with that summary; stored text unchanged, status needs_look | CONFIRMED |
| FR-10 | exit 0, 1 passed | Yes: re-ask message to the model contains "vísbendingar", "tölustöfum", "Slack", "eintölu" | CONFIRMED |

## PASS-45df725.md

| Claim | Command | Result | Verdict |
|---|---|---|---|
| rust test exit 0, 2555 executed | `cargo test --manifest-path rust/Cargo.toml > log 2>&1; echo $?` | exit 0; summed `test result` lines: 2555 passed, 0 failed, 2 ignored | CONFIRMED (2555 = passed; 2 ignored not counted as executed) |
| clippy exit 0 | `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings > log 2>&1; echo $?` | exit 0 | CONFIRMED |
| fmt --check exit 0 | `cargo fmt --manifest-path rust/Cargo.toml --all -- --check > log 2>&1; echo $?` | exit 0 | CONFIRMED |
| acceptance row: 12 rows, each exit 0 with 1 test | see above | 12 rows, all exit 0, 1 passed | CONFIRMED |
| slop-check / review-diff / `[skip ci]` grep rows | not run (outside the requested three gates) | n/a | NOT-CHECKABLE here |

Summary: 16 confirmed, 0 unsupported, 0 inconclusive, 1 not-checkable.
