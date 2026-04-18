"""Block inference: event stream → contiguous time blocks per company.

Gap-timeout clustering with the following rules (see feature-plan.local.md):
- Gap > TIMEOUT → new block
- Different company → new block (regardless of gap)
- Calendar events carry authoritative duration (e.g. a 45-min meeting is 45min
  even if it's the only event)
- Point events (commits, Jira updates, Claude prompts) extend the block end by
  +CREDIT (WakaTime-style terminal credit)
- Blocks shorter than MIN_BLOCK are discarded
- Blocks longer than MAX_BLOCK are flagged for human review
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
    company: str
    duration_seconds: int | None = None
    jira_issue: str | None = None
    event_id: int | None = None

    @property
    def end(self) -> datetime:
        if self.source in CALENDAR_SOURCES and self.duration_seconds:
            return self.ts + timedelta(seconds=self.duration_seconds)
        return self.ts + CREDIT


@dataclass
class Block:
    day: str
    company: str
    started_at: datetime
    ended_at: datetime
    duration_seconds: int
    event_count: int
    event_ids: list[int]
    jira_issue: str | None = None
    flagged: bool = False
    _events: list[InferEvent] = field(default_factory=list, repr=False)


def _new_block(e: InferEvent) -> Block:
    return Block(
        day=e.ts.date().isoformat(),
        company=e.company,
        started_at=e.ts,
        ended_at=e.end,
        duration_seconds=int((e.end - e.ts).total_seconds()),
        event_count=1,
        event_ids=[e.event_id] if e.event_id is not None else [],
        jira_issue=e.jira_issue,
        flagged=False,
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
    # Inherit Jira issue only if all events agree
    issues = {e.jira_issue for e in block._events if e.jira_issue}
    block.jira_issue = next(iter(issues)) if len(issues) == 1 else None
    return block


def build_blocks(events: list[InferEvent]) -> list[Block]:
    """Cluster a day's events into blocks. Input need not be sorted."""
    usable = [e for e in events if e.company]
    usable.sort(key=lambda e: e.ts)

    blocks: list[Block] = []
    current: Block | None = None

    for e in usable:
        if current is None:
            current = _new_block(e)
            continue
        gap = e.ts - current.ended_at
        if e.company != current.company or gap > TIMEOUT:
            if (finalized := _finalize(current)) is not None:
                blocks.append(finalized)
            current = _new_block(e)
        else:
            _extend_block(current, e)

    if current is not None and (finalized := _finalize(current)) is not None:
        blocks.append(finalized)

    blocks.sort(key=lambda b: b.started_at)
    return blocks
