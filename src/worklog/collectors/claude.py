"""Claude Code hook collector.

Reads a JSON event from stdin (as Claude Code hooks do), extracts the useful
bits, classifies it against the companies config, and upserts into the event
store. Never prints to stdout (hook output gets surfaced as tool output) —
errors go to stderr and we exit 0 so we never block the user's workflow.
"""

from __future__ import annotations

import json
import re
import sys
from datetime import UTC, datetime
from typing import Any

from worklog.classify import classify
from worklog.config import load_companies
from worklog.db import connect, init_db, upsert_event

# Matches ACME-123 style keys anywhere in a string (conservative — 2-10 upper letters).
JIRA_KEY_RE = re.compile(r"\b([A-Z][A-Z0-9]{1,9}-\d+)\b")


def _jira_from_text(*parts: str | None) -> str | None:
    for p in parts:
        if not p:
            continue
        m = JIRA_KEY_RE.search(p)
        if m:
            return m.group(1)
    return None


def handle(payload: dict[str, Any]) -> None:
    """Process a single Claude Code hook event."""
    event_name = payload.get("hook_event_name") or payload.get("event") or "unknown"
    session_id = payload.get("session_id", "no-session")
    cwd = payload.get("cwd") or payload.get("project_path")
    transcript_path = payload.get("transcript_path")

    now = datetime.now(UTC)
    source_id = f"{session_id}:{event_name}:{now.isoformat()}"

    # SessionStart / UserPromptSubmit / Stop / SubagentStop / PreToolUse / PostToolUse
    # We treat Stop as the canonical "session segment ended" marker when it has
    # a start time; otherwise log as point-in-time events.
    title_bits = [event_name]
    user_prompt = payload.get("prompt") or payload.get("user_prompt")
    if user_prompt:
        title_bits.append(str(user_prompt)[:80])
    title = " — ".join(title_bits)

    jira_issue = _jira_from_text(user_prompt, cwd)

    config = load_companies()
    company = classify(config, project_path=cwd, jira_issue=jira_issue)

    init_db()
    with connect() as conn:
        upsert_event(
            conn,
            source="claude",
            source_id=source_id,
            started_at=now,
            title=title,
            details=transcript_path,
            project_path=cwd,
            jira_issue=jira_issue,
            company=company,
            raw_json=json.dumps(payload),
        )


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
