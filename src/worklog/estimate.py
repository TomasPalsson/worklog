"""Block estimator — calls `claude -p --json-schema` to fill
jira_issue + minutes + description for each block.

The model picks from the user's open-ticket cache. It may also pick a ticket
that's NOT in the cache iff that ticket key appears literally (regex match) in
one of the block's event titles/details — that's the "absolutely certain"
escape hatch the user asked for. Anything else is rejected as a hallucination.
"""

from __future__ import annotations

import json
import math
import re
import sqlite3
import subprocess
from datetime import date
from typing import Any

from dateutil.parser import isoparse

from worklog.db import connect

JIRA_KEY_RE = re.compile(r"\b([A-Z][A-Z0-9]{1,9}-\d+)\b")

ESTIMATE_SCHEMA: dict[str, Any] = {
    "type": "object",
    "properties": {
        "jira_issue": {
            "type": ["string", "null"],
            "description": (
                "Jira issue key to log against. MUST be chosen from "
                "candidate_tickets OR from literal_matches. Use null if "
                "neither list is confident enough."
            ),
        },
        "minutes": {
            "type": "integer",
            "description": "Estimated duration in minutes.",
        },
        "description": {
            "type": "string",
            "description": "Tempo worklog description in Jira imperative style, max 120 chars.",
        },
    },
    "required": ["jira_issue", "minutes", "description"],
    "additionalProperties": False,
}

SYSTEM_PROMPT = """\
You are a Jira/Tempo worklog assistant. Given a JSON array of work events that
happened inside one contiguous time block, plus a candidate list of the user's
open Jira tickets, produce exactly one Tempo worklog entry.

Rules:
- jira_issue: pick the single best matching ticket from candidate_tickets. You
  may also pick a key from literal_matches (keys that appeared verbatim in
  event content — e.g. in a commit message or branch name). If NEITHER list
  gives you a confident match, return null. Never invent a key.
- description: Jira-style imperative (e.g. "Implement OAuth token refresh",
  "Review PR for billing module"). Avoid first-person ("I", "we"). For
  meetings, "Attend <topic> sync".
- minutes: prefer block_duration_minutes; only deviate if the events clearly
  don't fill the block (e.g. a single 2-min commit in a 60-min gap). Round to
  the nearest 15.
- Output ONLY a JSON object matching the schema. No prose, no code fences.
"""

DEFAULT_MODEL = "claude-haiku-4-5"
ROUND_MINUTES = 15


def _load_open_tickets(conn: sqlite3.Connection) -> list[dict[str, Any]]:
    return [
        {"key": r["key"], "summary": r["summary"], "status": r["status"]}
        for r in conn.execute(
            "SELECT key, summary, status FROM jira_tickets ORDER BY updated DESC"
        ).fetchall()
    ]


def _collect_literal_matches(events: list[sqlite3.Row]) -> list[str]:
    """Jira keys that appear verbatim anywhere in event titles or details."""
    found: list[str] = []
    seen: set[str] = set()
    for e in events:
        for blob in (e["title"], e["details"]):
            if not blob:
                continue
            for m in JIRA_KEY_RE.findall(blob):
                if m not in seen:
                    seen.add(m)
                    found.append(m)
    return found


def build_user_message(
    block_row: sqlite3.Row,
    events: list[sqlite3.Row],
    open_tickets: list[dict[str, Any]] | None = None,
    literal_matches: list[str] | None = None,
) -> str:
    started = isoparse(block_row["started_at"])
    ended = isoparse(block_row["ended_at"])
    duration_min = int((ended - started).total_seconds() / 60)
    payload = {
        "block_duration_minutes": duration_min,
        "inferred_jira_issue": block_row["jira_issue"],
        "candidate_tickets": open_tickets or [],
        "literal_matches": literal_matches or [],
        "events": [
            {
                "type": e["source"],
                "timestamp": e["started_at"],
                "summary": (e["title"] or "")[:200],
                "details": (e["details"] or "")[:200] if e["details"] else None,
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


def _validate_ticket_choice(
    claimed: str | None,
    candidates: list[dict[str, Any]],
    literals: list[str],
) -> str | None:
    """Reject any key that wasn't in candidates or a literal match.

    Protects against Claude inventing keys. Null is fine — means "no ticket".
    """
    if not claimed:
        return None
    if claimed in {c["key"] for c in candidates}:
        return claimed
    if claimed in literals:
        return claimed
    return None


def estimate_day(day: date, *, model: str = DEFAULT_MODEL) -> dict[str, int]:
    """Fill jira_issue + description + minutes for every un-estimated block on `day`."""
    day_iso = day.isoformat()
    stats = {"estimated": 0, "skipped": 0, "failed": 0}

    with connect() as conn:
        blocks = conn.execute(
            "SELECT * FROM blocks WHERE day = ? ORDER BY started_at",
            (day_iso,),
        ).fetchall()
        open_tickets = _load_open_tickets(conn)

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
            literals = _collect_literal_matches(events)

            user_msg = build_user_message(block, events, open_tickets, literals)
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
            ticket = _validate_ticket_choice(
                reply.get("jira_issue"), open_tickets, literals
            )
            # If the block already had an inferred ticket and Claude didn't
            # confidently pick one, keep the inference.
            if ticket is None and block["jira_issue"]:
                ticket = block["jira_issue"]

            conn.execute(
                """
                UPDATE blocks
                   SET description = ?,
                       duration_seconds = ?,
                       jira_issue = ?,
                       estimated_by = 'claude_p'
                 WHERE id = ?
                """,
                (reply["description"], minutes * 60, ticket, block["id"]),
            )
            stats["estimated"] += 1
    return stats


__all__ = [
    "DEFAULT_MODEL",
    "ESTIMATE_SCHEMA",
    "SYSTEM_PROMPT",
    "build_user_message",
    "estimate_day",
    "parse_response",
]
