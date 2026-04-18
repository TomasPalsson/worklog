"""Tests for the Google Calendar collector's UTC normalisation.

The bucketing layer in both Python and Rust compares `started_at` as a
TEXT column. Non-UTC offset strings sort lexicographically AFTER same-
instant UTC strings, so an event stored as `2026-04-18T09:00:00+02:00`
would be missed by a UTC day-window query. `_to_utc` normalises
everything to UTC at the collector boundary.
"""

from __future__ import annotations

from datetime import UTC, datetime, timedelta, timezone

from worklog.collectors.gcal import _to_utc


def test_to_utc_converts_positive_offset() -> None:
    # 09:00 Amsterdam (+02:00) == 07:00 UTC.
    got = _to_utc("2026-04-18T09:00:00+02:00")
    assert got == datetime(2026, 4, 18, 7, 0, 0, tzinfo=UTC)
    # ISO string must end in +00:00, not the source offset.
    assert got.isoformat() == "2026-04-18T07:00:00+00:00"


def test_to_utc_converts_negative_offset() -> None:
    # 23:30 New York (-04:00 in April) == 03:30 UTC the next day.
    got = _to_utc("2026-04-18T23:30:00-04:00")
    assert got == datetime(2026, 4, 19, 3, 30, 0, tzinfo=UTC)


def test_to_utc_handles_bare_date_all_day_event() -> None:
    # Google sends all-day events as `{date: "YYYY-MM-DD"}` without a
    # time. We anchor these to 00:00 UTC on the stated date.
    got = _to_utc("2026-04-18")
    assert got == datetime(2026, 4, 18, 0, 0, 0, tzinfo=UTC)
    assert got.tzinfo == UTC


def test_to_utc_passes_through_zulu() -> None:
    got = _to_utc("2026-04-18T09:00:00Z")
    assert got == datetime(2026, 4, 18, 9, 0, 0, tzinfo=UTC)


def test_to_utc_lexicographic_sort_after_normalisation() -> None:
    """Regression: without normalisation, a +02:00 string sorts after a
    same-instant +00:00 string. After _to_utc, same instants produce
    identical strings.
    """
    a = _to_utc("2026-04-18T09:00:00+02:00").isoformat()
    b = _to_utc("2026-04-18T07:00:00+00:00").isoformat()
    assert a == b


def test_to_utc_equal_instants_from_different_zones() -> None:
    """Three representations of the same instant must all normalise to
    the same string."""
    a = _to_utc("2026-04-18T09:00:00+02:00").isoformat()
    b = _to_utc("2026-04-18T07:00:00+00:00").isoformat()
    c = _to_utc("2026-04-18T03:00:00-04:00").isoformat()
    assert a == b == c


def test_to_utc_custom_offset() -> None:
    # Use timezone(timedelta(hours=5, minutes=30)) — India Standard Time.
    expected = datetime(2026, 4, 18, 12, 0, 0, tzinfo=UTC)
    got = _to_utc("2026-04-18T17:30:00+05:30")
    assert got == expected
    assert got.tzinfo == UTC
