"""Red-phase tests for block inference.

Algorithm (from research): gap-timeout clustering.
- Gap threshold: 20 minutes
- Calendar events carry authoritative duration
- Point events get +2min credit (WakaTime-style terminal credit)
- Company mismatch forces a split regardless of gap
- MIN_BLOCK = 5min (discard below), MAX_BLOCK = 4h (flag above)
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
    """Short alias for building test events."""

    ts: datetime
    source: str = "github_commit"
    company: str = "Acme"
    duration: int | None = None
    jira_issue: str | None = None

    def to_event(self) -> InferEvent:
        return InferEvent(
            ts=self.ts,
            source=self.source,
            company=self.company,
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
    # One commit → 2min credit → below MIN_BLOCK(5min) → discarded
    assert build_blocks(_events(_E(ts=_at(10, 0)))) == []


def test_two_close_commits_become_one_block() -> None:
    events = _events(_E(ts=_at(10, 0)), _E(ts=_at(10, 5)))
    blocks = build_blocks(events)
    assert len(blocks) == 1
    b = blocks[0]
    assert b.started_at == _at(10, 0)
    # Last commit + 2min credit
    assert b.ended_at == _at(10, 7)
    assert b.company == "Acme"
    assert b.event_count == 2


def test_gap_larger_than_timeout_splits() -> None:
    # 25-min gap > TIMEOUT(20) → two blocks
    events = _events(_E(ts=_at(10, 0)), _E(ts=_at(10, 5)), _E(ts=_at(10, 35)))
    blocks = build_blocks(events)
    # First block: 10:00-10:07 (valid). Second block alone has 2min credit — discarded.
    assert len(blocks) == 1
    assert blocks[0].started_at == _at(10, 0)
    assert blocks[0].ended_at == _at(10, 7)


def test_calendar_event_is_authoritative_duration() -> None:
    meeting = _E(ts=_at(10, 0), source="gcal", duration=30 * 60)  # 30min
    blocks = build_blocks(_events(meeting))
    assert len(blocks) == 1
    assert blocks[0].started_at == _at(10, 0)
    assert blocks[0].ended_at == _at(10, 30)


def test_company_mismatch_forces_split_even_with_small_gap() -> None:
    events = _events(
        _E(ts=_at(10, 0), company="Acme"),
        _E(ts=_at(10, 15), company="Side"),
        _E(ts=_at(10, 30), company="Acme"),
    )
    blocks = build_blocks(events)
    # Three separate companies — but each single-event block is below MIN_BLOCK.
    # Give them a calendar so they're real: use 10-min meetings.
    events = _events(
        _E(ts=_at(10, 0), company="Acme", source="gcal", duration=10 * 60),
        _E(ts=_at(10, 15), company="Side", source="gcal", duration=10 * 60),
        _E(ts=_at(10, 30), company="Acme", source="gcal", duration=10 * 60),
    )
    blocks = build_blocks(events)
    assert [b.company for b in blocks] == ["Acme", "Side", "Acme"]


def test_block_over_max_is_flagged() -> None:
    meeting = _E(ts=_at(8, 0), source="gcal", duration=5 * 3600)  # 5h
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
    # Block spans both issues — can't pick one; leave unset.
    assert blocks[0].jira_issue is None


def test_events_without_company_are_dropped() -> None:
    events = _events(
        _E(ts=_at(10, 0), company=""),
        _E(ts=_at(10, 5), company="Acme"),
        _E(ts=_at(10, 10), company="Acme"),
    )
    blocks = build_blocks(events)
    # Only the two Acme events form a block.
    assert len(blocks) == 1
    assert blocks[0].event_count == 2


def test_blocks_sorted_by_start_time() -> None:
    events = _events(
        _E(ts=_at(14, 0), company="Acme", source="gcal", duration=15 * 60),
        _E(ts=_at(10, 0), company="Acme", source="gcal", duration=15 * 60),
        _E(ts=_at(12, 0), company="Side", source="gcal", duration=15 * 60),
    )
    blocks = build_blocks(events)
    assert [b.started_at.hour for b in blocks] == [10, 12, 14]


def test_block_structure_has_event_ids() -> None:
    events = _events(_E(ts=_at(10, 0)), _E(ts=_at(10, 5)))
    # attach ids
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
    # 10-min block across midnight — day is the start date
    assert blocks[0].day == "2026-04-18"


def test_block_type_shape() -> None:
    b = Block(
        day="2026-04-18",
        company="Acme",
        started_at=_at(10, 0),
        ended_at=_at(10, 30),
        duration_seconds=1800,
        event_count=3,
        event_ids=[1, 2, 3],
        jira_issue=None,
        flagged=False,
    )
    assert b.duration_seconds == 1800
