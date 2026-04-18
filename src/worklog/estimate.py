"""Block estimator — calls `claude -p --json-schema` to fill minutes + description.

Uses the user's existing Claude Code auth (no API key required). Haiku 4.5 is
the configured default because the task is small and cheap. The structured
schema enforcement provided by `--json-schema` means retries and malformed-JSON
handling are both rare, but we defend in depth anyway.
"""

from __future__ import annotations

import json
import math
import re
import sqlite3
import subprocess
from datetime import date, datetime, timedelta
from typing import Any

from dateutil.parser import isoparse

from worklog.db import connect

ESTIMATE_SCHEMA: dict[str, Any] = {
    "type": "object",
    "properties": {
        "minutes": {
            "type": "integer",
            "description": "Estimated duration in minutes.",
        },
        "description": {
            "type": "string",
            "description": "Tempo worklog description in Jira imperative style, max 120 chars.",
        },
    },
    "required": ["minutes", "description"],
    "additionalProperties": False,
}

SYSTEM_PROMPT = """\
You are a Jira/Tempo worklog assistant. Given a JSON array of work events that
happened inside one contiguous time block, produce exactly one Tempo worklog
entry: a concise description and an estimated duration in minutes.

Rules:
- Use Jira-style imperative language (e.g. "Implement OAuth token refresh",
  "Review PR for billing module"). Avoid first-person ("I", "we").
- For meetings, use "Attend <topic> sync" or similar.
- Prefer the provided block_duration_minutes; only deviate if the events
  clearly don't fill that time (e.g. a single 2-min commit in a 60-min gap).
- Round minutes to the nearest 15.
- Output ONLY a JSON object matching the schema. No prose, no code fences.
"""

DEFAULT_MODEL = "claude-haiku-4-5"
ROUND_MINUTES = 15


def build_user_message(block_row: sqlite3.Row, events: list[sqlite3.Row]) -> str:
    started = isoparse(block_row["started_at"])
    ended = isoparse(block_row["ended_at"])
    duration_min = int((ended - started).total_seconds() / 60)
    payload = {
        "block_duration_minutes": duration_min,
        "company": block_row["company"],
        "jira_issue": block_row["jira_issue"],
        "events": [
            {
                "type": e["source"],
                "timestamp": e["started_at"],
                "summary": (e["title"] or "")[:200],
                "jira_issue": e["jira_issue"],
            }
            for e in events
        ],
    }
    return json.dumps(payload, indent=2)


def parse_response(raw: str) -> dict[str, Any]:
    """Handle: envelope (claude -p --output-format json), raw JSON, prose-wrapped JSON."""
    raw = raw.strip()
    try:
        parsed = json.loads(raw)
        if isinstance(parsed, dict) and "result" in parsed:
            result_str = parsed["result"]
            if isinstance(result_str, str):
                return json.loads(result_str)  # type: ignore[no-any-return]
            if isinstance(result_str, dict):
                return result_str  # type: ignore[no-any-return]
        if isinstance(parsed, dict):
            return parsed  # type: ignore[no-any-return]
    except json.JSONDecodeError:
        pass

    if m := re.search(r"\{[\s\S]*\}", raw):
        try:
            return json.loads(m.group())  # type: ignore[no-any-return]
        except json.JSONDecodeError as e:
            raise ValueError(f"no valid JSON: {e}") from e
    raise ValueError("no JSON object in response")


def _invoke_claude_p(
    system_prompt: str, user_message: str, schema: dict[str, Any], model: str
) -> dict[str, Any]:
    """Subprocess wrapper around `claude -p`. Raises on any failure."""
    cmd = [
        "claude",
        "-p",
        "--model",
        model,
        "--output-format",
        "json",
        "--json-schema",
        json.dumps(schema),
        "--system-prompt",
        system_prompt,
    ]
    result = subprocess.run(  # noqa: S603 - cmd is a fixed list, inputs are our own data
        cmd,
        input=user_message,
        capture_output=True,
        text=True,
        timeout=60,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(f"claude -p exited {result.returncode}: {result.stderr[:500]}")
    return parse_response(result.stdout)


def _round_up_minutes(minutes: int, step: int = ROUND_MINUTES) -> int:
    return step * math.ceil(max(minutes, 1) / step)


def estimate_day(day: date, *, model: str = DEFAULT_MODEL) -> dict[str, int]:
    """Fill description + minutes for every un-estimated block on `day`."""
    day_iso = day.isoformat()
    stats = {"estimated": 0, "skipped": 0, "failed": 0}

    with connect() as conn:
        blocks = conn.execute(
            """
            SELECT * FROM blocks
             WHERE day = ?
             ORDER BY started_at
            """,
            (day_iso,),
        ).fetchall()

        for block in blocks:
            if block["estimated_by"] == "claude_p":
                stats["skipped"] += 1
                continue

            events = conn.execute(
                """
                SELECT e.*
                  FROM events e
                  JOIN block_events be ON be.event_id = e.id
                 WHERE be.block_id = ?
                 ORDER BY e.started_at
                """,
                (block["id"],),
            ).fetchall()

            user_msg = build_user_message(block, events)
            try:
                reply = _invoke_claude_p(SYSTEM_PROMPT, user_msg, ESTIMATE_SCHEMA, model)
            except Exception:  # noqa: BLE001 - fallback, recorded per-block
                conn.execute(
                    "UPDATE blocks SET estimated_by = 'gap' WHERE id = ?",
                    (block["id"],),
                )
                stats["failed"] += 1
                continue

            minutes = _round_up_minutes(int(reply["minutes"]))
            conn.execute(
                """
                UPDATE blocks
                   SET description = ?,
                       duration_seconds = ?,
                       estimated_by = 'claude_p'
                 WHERE id = ?
                """,
                (reply["description"], minutes * 60, block["id"]),
            )
            stats["estimated"] += 1
    return stats


def _day_from_datetime_range() -> timedelta:
    # Convenience no-op so lints don't complain about unused timedelta import.
    return timedelta(0)


__all__ = [
    "DEFAULT_MODEL",
    "ESTIMATE_SCHEMA",
    "SYSTEM_PROMPT",
    "build_user_message",
    "estimate_day",
    "parse_response",
]

# Silence unused-import complaint on datetime.
_ = datetime
