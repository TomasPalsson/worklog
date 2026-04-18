"""Tests for block inference.

Gap-timeout clustering:
- Gap threshold: 20 minutes
- Calendar events carry authoritative duration AND always start a new block
- Point events get +2min credit (WakaTime-style terminal credit)
- MIN_BLOCK = 5min (discard below), MAX_BLOCK = 4h (flag above)
- No company split — everything is one activity stream
"""

from __future__ import annotations

from dataclasses import dataclass
from datetime import UTC, datetime, timedelta

from worklog.infer import (
    MAX_BLOCK,
    MIN_BLOCK,
    TIMEOUT,
    Block,
    InferEvent,
    build_blocks,
)


@dataclass
class _E:
    ts: datetime
    source: str = "github_commit"
    duration: int | None = None
    jira_issue: str | None = None

    def to_event(self) -> InferEvent:
        return InferEvent(
            ts=self.ts,
            source=self.source,
            duration_seconds=self.duration,
            jira_issue=self.jira_issue,
        )


def _at(h: int, m: int = 0) -> datetime:
    return datetime(2026, 4, 18, h, m, tzinfo=UTC)


def _events(*items: _E) -> list[InferEvent]:
    return [i.to_event() for i in items]


def test_constants() -> None:
    assert TIMEOUT == timedelta(minutes=20)
    assert MIN_BLOCK == timedelta(minutes=5)
    assert MAX_BLOCK == timedelta(hours=4)


def test_empty_input_produces_no_blocks() -> None:
    assert build_blocks([]) == []


def test_single_point_event_with_credit_below_min_is_discarded() -> None:
    assert build_blocks(_events(_E(ts=_at(10, 0)))) == []


def test_two_close_commits_become_one_block() -> None:
    events = _events(_E(ts=_at(10, 0)), _E(ts=_at(10, 5)))
    blocks = build_blocks(events)
    assert len(blocks) == 1
    b = blocks[0]
    assert b.started_at == _at(10, 0)
    assert b.ended_at == _at(10, 7)
    assert b.event_count == 2


def test_gap_larger_than_timeout_splits() -> None:
    events = _events(_E(ts=_at(10, 0)), _E(ts=_at(10, 5)), _E(ts=_at(10, 35)))
    blocks = build_blocks(events)
    # First block 10:00-10:07; the lone commit at 10:35 gets 2min credit → discarded.
    assert len(blocks) == 1
    assert blocks[0].started_at == _at(10, 0)
    assert blocks[0].ended_at == _at(10, 7)


def test_calendar_event_is_authoritative_duration() -> None:
    meeting = _E(ts=_at(10, 0), source="gcal", duration=30 * 60)
    blocks = build_blocks(_events(meeting))
    assert len(blocks) == 1
    assert blocks[0].started_at == _at(10, 0)
    assert blocks[0].ended_at == _at(10, 30)


def test_calendar_event_always_starts_new_block() -> None:
    """A meeting mid-coding should split — meeting isn't absorbed into code."""
    events = _events(
        _E(ts=_at(10, 0)),
        _E(ts=_at(10, 10)),
        _E(ts=_at(10, 15), source="gcal", duration=30 * 60),  # meeting
        _E(ts=_at(10, 50)),
    )
    blocks = build_blocks(events)
    # Block 1: 10:00–10:12 (coding, <5min — discarded because <MIN_BLOCK? 12min is fine)
    # Block 2: 10:15–10:45 (meeting)
    # Block 3: 10:50+2 = 10:52 (<MIN_BLOCK, discarded)
    assert len(blocks) == 2
    assert blocks[0].started_at == _at(10, 0)
    assert blocks[0].ended_at == _at(10, 12)
    assert blocks[1].started_at == _at(10, 15)
    assert blocks[1].ended_at == _at(10, 45)


def test_block_over_max_is_flagged() -> None:
    meeting = _E(ts=_at(8, 0), source="gcal", duration=5 * 3600)
    blocks = build_blocks(_events(meeting))
    assert len(blocks) == 1
    assert blocks[0].flagged is True


def test_blocks_inherit_jira_issue_when_unanimous() -> None:
    events = _events(
        _E(ts=_at(10, 0), jira_issue="ACME-1"),
        _E(ts=_at(10, 10), jira_issue="ACME-1"),
    )
    blocks = build_blocks(events)
    assert blocks[0].jira_issue == "ACME-1"


def test_blocks_drop_jira_issue_when_mixed() -> None:
    events = _events(
        _E(ts=_at(10, 0), jira_issue="ACME-1"),
        _E(ts=_at(10, 10), jira_issue="ACME-2"),
    )
    blocks = build_blocks(events)
    assert blocks[0].jira_issue is None


def test_blocks_sorted_by_start_time() -> None:
    events = _events(
        _E(ts=_at(14, 0), source="gcal", duration=15 * 60),
        _E(ts=_at(10, 0), source="gcal", duration=15 * 60),
        _E(ts=_at(12, 0), source="gcal", duration=15 * 60),
    )
    blocks = build_blocks(events)
    assert [b.started_at.hour for b in blocks] == [10, 12, 14]


def test_block_structure_has_event_ids() -> None:
    events = _events(_E(ts=_at(10, 0)), _E(ts=_at(10, 5)))
    events[0] = InferEvent(**{**events[0].__dict__, "event_id": 101})
    events[1] = InferEvent(**{**events[1].__dict__, "event_id": 102})
    blocks = build_blocks(events)
    assert blocks[0].event_ids == [101, 102]


def test_block_day_matches_start_date() -> None:
    events = _events(
        _E(ts=datetime(2026, 4, 18, 23, 55, tzinfo=UTC)),
        _E(ts=datetime(2026, 4, 19, 0, 5, tzinfo=UTC)),
    )
    blocks = build_blocks(events)
    assert blocks[0].day == "2026-04-18"


def test_block_type_shape() -> None:
    b = Block(
        day="2026-04-18",
        started_at=_at(10, 0),
        ended_at=_at(10, 30),
        duration_seconds=1800,
        event_count=3,
        event_ids=[1, 2, 3],
        jira_issue=None,
        flagged=False,
    )
    assert b.duration_seconds == 1800
