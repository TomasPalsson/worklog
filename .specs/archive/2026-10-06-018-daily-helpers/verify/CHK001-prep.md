# CHK001 — prep (not yet verified by a human)

Check: the Owner presses Standup, edits, presses Post, and the reply appears in today's Daily thread within 30 s of the click.

Not run by the orchestrator, because:
- Posting writes a real message into the team's Slack Daily thread as the Owner (outward, not reversible from here).
- The Daily channel setting and the /standup routes exist only on branch flow/daily-helpers; the installed daemon on :9323 is the released build, so this needs this branch's daemon + web running in its place.

Automated evidence already green (2026-10-06):
- cargo test -p worklog-core --lib standup → exit 0 (12)
- cargo test -p worklog-core --lib slack_post → exit 0 (13)
- cargo test -p worklog-core --lib daemon_standup → exit 0 (8)
- cargo test -p worklog-cli → exit 0 (worklog standup confirm-before-post: 6 tests)
- cd web && bun test components/StandupButton → exit 0 (13)

To verify by hand:
1. Build and run this branch's daemon and web in place of the installed one.
2. Settings → Daily standup → set the channel (e.g. #daily).
3. Day page (today) → Standup → edit one line → Post.
4. Confirm the reply lands under today's "Daily:thread" message within 30 s; note the permalink here.
Known risk to watch: Slack search.messages needs a user token (xoxp); a bot token fails with not_allowed_token_type.
