# CHK001 — phase A on real data (2026-09-27)

Method: `sqlite3 ~/.local/share/worklog/worklog.db ".backup <copy>"` (the live DB was
never written), then `WORKLOG_HOME=<copy> WORKLOG_GITHUB_USER=TomasPalsson
rust/target/debug/worklog db migrate` from this branch → schema v14 → v15, which runs
`upgrade_006::run` once.

## Before (live DB, v14)
Commits/PRs linked into blocks on 2026-09-24/25: 29 (block, repo) pairs, almost all
`TomasPalsson/{worklog,flow,ads-seo,aws-exam}` — see `CHK001-before-commits-in-blocks.txt`.

## After (copy, v15)
- `SELECT count(*) FROM events WHERE repo LIKE 'TomasPalsson/%'` → **0** (FR-02).
- Org github rows left: aproorg/LibreChat PR → `~/Desktop/Work/LibreChat`,
  LibreChat-AI/LibreChat PR → `~/Desktop/Work/LibreChat`, 2× aproorg/code-interpreter
  PR → `~/Desktop/Work/code-interpreter`; elsewhere = 0 for all (clones exist).
- Commits/PRs in blocks on 09-24/25: exactly one —
  `2026-09-24 09:27-09:43 aproorg/LibreChat PR #80` in a LibreChat-folder block.
  No worklog / flow / ads-seo commit in any block.
- The code-interpreter PR #5 (09-25 15:19) is no longer glued into the 15:09–16:16
  block; it carries its own folder and forms no block (a lone 2-minute event).
- Manual descriptions: 4 before, 4 after. exported_at: 0 before (nothing to carry on
  real data; carry proven by `upgrade_006 carries_all_fields`).

## Block counts per day (count | minutes)
| day | before (live) | after (copy) |
|---|---|---|
| 09-21 | 6 / 218 | 19 / 270 |
| 09-22 | 20 / 364 | 31 / 492 |
| 09-23 | 31 / 970 | 31 / 970 |
| 09-24 | 22 / 577 | 25 / 669 |
| 09-25 | 24 / 550 | 24 / 550 |
| 09-26 | 12 / 411 | 12 / 411 |
| 09-27 | 4 / 161 | 4 / 145 |

The 09-21/22/24 differences are from re-inference itself, not the new rules: the
released `worklog 0.12.0 infer` on a second copy gives identical 19/270, 31/492,
25/669. Those days were last built by older code or hand-edited; D-09 accepts a full
re-infer.
