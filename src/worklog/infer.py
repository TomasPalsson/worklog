"""Block inference: event stream → contiguous time blocks.

Gap-timeout clustering with the following rules:
- Gap > TIMEOUT → new block
- Calendar events carry authoritative duration (e.g. a 45-min meeting is 45min
  even if it's the only event); they also *start* a new block at their start
  time and end the previous one at that boundary
- Point events (commits, Jira updates, Claude prompts) extend the block end by
  +CREDIT (WakaTime-style terminal credit)
- Blocks shorter than MIN_BLOCK are discarded
- Blocks longer than MAX_BLOCK are flagged for human review

No more "company" split — everything is one activity stream. The block's
jira_issue is inherited only when all events in the block agree on a ticket.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime, timedelta

TIMEOUT = timedelta(minutes=20)
CREDIT = timedelta(minutes=2)
MIN_BLOCK = timedelta(minutes=5)
MAX_BLOCK = timedelta(hours=4)

CALENDAR_SOURCES = frozenset({"gcal"})


@dataclass
class InferEvent:
    ts: datetime
    source: str
    duration_seconds: int | None = None
    jira_issue: str | None = None
    event_id: int | None = None

    @property
    def end(self) -> datetime:
        if self.source in CALENDAR_SOURCES and self.duration_seconds:
            return self.ts + timedelta(seconds=self.duration_seconds)
        return self.ts + CREDIT

    @property
    def is_calendar(self) -> bool:
        return self.source in CALENDAR_SOURCES


@dataclass
class Block:
    day: str
    started_at: datetime
    ended_at: datetime
    duration_seconds: int
    event_count: int
    event_ids: list[int]
    jira_issue: str | None = None
    flagged: bool = False
    # True if this block was spawned by a calendar event. Calendar blocks are
    # closed units — nothing extends them, because meeting time is authoritative.
    _is_calendar: bool = field(default=False, repr=False)
    _events: list[InferEvent] = field(default_factory=list, repr=False)


def _new_block(e: InferEvent) -> Block:
    return Block(
        day=e.ts.date().isoformat(),
        started_at=e.ts,
        ended_at=e.end,
        duration_seconds=int((e.end - e.ts).total_seconds()),
        event_count=1,
        event_ids=[e.event_id] if e.event_id is not None else [],
        jira_issue=e.jira_issue,
        flagged=False,
        _is_calendar=e.is_calendar,
        _events=[e],
    )


def _extend_block(block: Block, e: InferEvent) -> None:
    block.ended_at = max(block.ended_at, e.end)
    block.duration_seconds = int((block.ended_at - block.started_at).total_seconds())
    block.event_count += 1
    if e.event_id is not None:
        block.event_ids.append(e.event_id)
    block._events.append(e)


def _finalize(block: Block) -> Block | None:
    duration = block.ended_at - block.started_at
    if duration < MIN_BLOCK:
        return None
    if duration > MAX_BLOCK:
        block.flagged = True
    issues = {e.jira_issue for e in block._events if e.jira_issue}
    block.jira_issue = next(iter(issues)) if len(issues) == 1 else None
    return block


def build_blocks(events: list[InferEvent]) -> list[Block]:
    """Cluster a day's events into blocks. Input need not be sorted."""
    usable = sorted(events, key=lambda e: e.ts)

    blocks: list[Block] = []
    current: Block | None = None

    for e in usable:
        if current is None:
            current = _new_block(e)
            continue

        gap = e.ts - current.ended_at
        # Calendar blocks are authoritative and closed — never extend them.
        # Calendar events always start a new block so meetings aren't absorbed
        # into surrounding code/commit work.
        if e.is_calendar or current._is_calendar or gap > TIMEOUT:
            if (finalized := _finalize(current)) is not None:
                blocks.append(finalized)
            current = _new_block(e)
        else:
            _extend_block(current, e)

    if current is not None and (finalized := _finalize(current)) is not None:
        blocks.append(finalized)

    blocks.sort(key=lambda b: b.started_at)
    return blocks
