# CHK002 — Details view on 2026-09-25 (2026-09-27)

Setup: backup copy of the live DB → branch binary `db migrate`, `collect shell|reflog|transcripts --days 3`,
`infer --day 2026-09-25`; branch daemon on 127.0.0.1:9433 over that copy; `next dev -p 3334` with
`WORKLOG_DAEMON_URL=http://127.0.0.1:9433`. Live DB and live daemon untouched.

Block: 5282, vitinn-infra, 16:57–18:22 (85 min).

- Day page: every block card has a "Details" link (`/2026-09-25/block/<id>`), 24 of 24 blocks.
- `GET /blocks/5282/details` → 222 rows: 5 claude_turn, 115 claude_tool, 66 claude_helper,
  32 claude_work, 2 claude (hook), 2 git_reflog.
- Completeness cross-check against the DB, same span, vitinn-infra or folderless:
  claude_turn 5 / claude_tool 115 / claude_helper 66 / shell 0 — identical to the view.
  (No shell command ran in that span.)
- Page: filter chips Claude + git; two Claude sessions fold prompt → tool calls → files; helper
  groups nested inside the session ("adversary: verify:image-digest · 5", "general-purpose: Judge
  the code interpreter spec · 4", "claude working · 12" = background-job minutes, …).
- 222 expand buttons (one per row); expanding a prompt shows its full stored record incl. raw text.
- Filter: Claude off → 0 session groups, only the 2 git rows; Claude on → 11 groups again.
- Console: no errors or warnings.
- Fix made during the check: expanded JSON ran off the right edge → `.block-details-raw` now wraps
  (`white-space: pre-wrap`); re-checked, no horizontal overflow.

Screenshots: CHK002-details-top.png, CHK002-session-open.png, CHK002-row-expanded.png.
