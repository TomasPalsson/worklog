"""Claude Code hook collector.

Reads a JSON event from stdin, upserts an event row and (for lifecycle events)
maintains the `sessions` table for real duration tracking. Never prints to
stdout; errors go to stderr and we exit 0 so Claude is never blocked.
"""

from __future__ import annotations

import json
import re
import sys
from datetime import UTC, datetime
from typing import Any

from worklog.db import connect, init_db, upsert_event
from worklog.sessions import close_session, open_session, reap_stale

JIRA_KEY_RE = re.compile(r"\b([A-Z][A-Z0-9]{1,9}-\d+)\b")

# Which hook events pair with which lifecycle action.
_CLOSING_EVENTS: dict[str, str] = {
    "Stop": "stop",
    "SessionEnd": "session_end",
}


def _first_jira_key(*parts: str | None) -> str | None:
    for p in parts:
        if p and (m := JIRA_KEY_RE.search(p)):
            return m.group(1)
    return None


def _prompt_of(payload: dict[str, Any]) -> str | None:
    return payload.get("prompt") or payload.get("user_prompt")


def _title_for(event: str, prompt: str | None) -> str:
    return f"{event} — {str(prompt)[:80]}" if prompt else event


def handle(payload: dict[str, Any]) -> None:
    """Process a single Claude Code hook event."""
    event = payload.get("hook_event_name") or payload.get("event") or "unknown"
    session_id = payload.get("session_id", "no-session")
    cwd = payload.get("cwd") or payload.get("project_path")
    prompt = _prompt_of(payload)
    now = datetime.now(UTC)

    jira_issue = _first_jira_key(prompt, cwd)

    init_db()
    with connect() as conn:
        upsert_event(
            conn,
            source="claude",
            source_id=f"{session_id}:{event}:{now.isoformat()}",
            started_at=now,
            title=_title_for(event, prompt),
            details=payload.get("transcript_path"),
            project_path=cwd,
            jira_issue=jira_issue,
            session_id=session_id,
            raw_json=json.dumps(payload),
        )
        open_session(conn, session_id=session_id, started_at=now, project_path=cwd)
        if close_reason := _CLOSING_EVENTS.get(event):
            close_session(
                conn, session_id=session_id, ended_at=now, end_source=close_reason
            )
        reap_stale(conn, now=now)


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except json.JSONDecodeError as e:
        print(f"worklog hook: invalid JSON on stdin: {e}", file=sys.stderr)
        return 0
    try:
        handle(payload)
    except Exception as e:  # noqa: BLE001 - hook must never break user's session
        print(f"worklog hook: {e}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
