"""Google Calendar collector: pulls events from configured calendars.

Uses OAuth installed-app flow. Token is cached at Settings.google_token_path.
For Workspace accounts, download OAuth client credentials from GCP and save
to ~/.config/worklog/google_credentials.json.
"""

from __future__ import annotations

from datetime import UTC, date, datetime, time, timedelta

from dateutil.parser import isoparse
from google.auth.transport.requests import Request
from google.oauth2.credentials import Credentials
from google_auth_oauthlib.flow import InstalledAppFlow
from googleapiclient.discovery import build

from worklog.config import Settings
from worklog.db import connect, init_db, upsert_event


def _to_utc(raw: str) -> datetime:
    """Parse a Google Calendar start/end field and return it as UTC.

    Google returns either:
      - A full RFC3339 string with offset, e.g. ``2026-04-18T09:00:00+02:00``
      - A bare date for all-day events, e.g. ``2026-04-18``

    Without normalisation, non-UTC offset strings break the lexicographic
    ``started_at >= ? AND < ?`` comparison used by both the Python and
    Rust event bucketers: ``2026-04-18T09:00:00+02:00`` sorts *later*
    than ``2026-04-18T07:00:00+00:00`` even though they represent the
    same instant. Events from non-UTC calendars would land on the wrong
    day or fall outside the window entirely.

    All-day events are anchored at midnight UTC on the stated date,
    which matches what the previous implementation did accidentally via
    ``datetime.combine(..., time.min)``.
    """
    parsed = isoparse(raw)
    if isinstance(parsed, date) and not isinstance(parsed, datetime):
        # All-day event — `isoparse("2026-04-18")` returns a `date`.
        parsed = datetime.combine(parsed, time.min)
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=UTC)
    return parsed.astimezone(UTC)

SCOPES = ["https://www.googleapis.com/auth/calendar.readonly"]


def _creds(settings: Settings) -> Credentials:
    creds: Credentials | None = None
    if settings.google_token_path.exists():
        creds = Credentials.from_authorized_user_file(
            str(settings.google_token_path), SCOPES
        )
    if creds and creds.expired and creds.refresh_token:
        creds.refresh(Request())
    elif not creds or not creds.valid:
        if not settings.google_credentials_path.exists():
            raise RuntimeError(
                f"Missing {settings.google_credentials_path}. "
                "Create OAuth client in GCP console and download JSON."
            )
        flow = InstalledAppFlow.from_client_secrets_file(
            str(settings.google_credentials_path), SCOPES
        )
        creds = flow.run_local_server(port=0)
        settings.google_token_path.parent.mkdir(parents=True, exist_ok=True)
        settings.google_token_path.write_text(creds.to_json())
    return creds


def collect(
    *,
    since: date,
    until: date | None = None,
    settings: Settings | None = None,
) -> int:
    settings = settings or Settings()
    until = until or (date.today() + timedelta(days=1))
    service = build("calendar", "v3", credentials=_creds(settings))

    init_db()
    count = 0

    start_iso = datetime.combine(since, time.min).isoformat() + "Z"
    end_iso = datetime.combine(until, time.min).isoformat() + "Z"

    with connect() as conn:
        for cal in settings.google_calendars:
            page_token: str | None = None
            while True:
                resp = (
                    service.events()
                    .list(
                        calendarId=cal,
                        timeMin=start_iso,
                        timeMax=end_iso,
                        singleEvents=True,
                        orderBy="startTime",
                        pageToken=page_token,
                    )
                    .execute()
                )
                for ev in resp.get("items", []):
                    if ev.get("status") == "cancelled":
                        continue
                    start = ev.get("start", {}).get("dateTime") or ev.get("start", {}).get("date")
                    end = ev.get("end", {}).get("dateTime") or ev.get("end", {}).get("date")
                    if not start:
                        continue
                    started_at = _to_utc(start)
                    ended_at = _to_utc(end) if end else None
                    duration = (
                        int((ended_at - started_at).total_seconds())
                        if ended_at
                        else None
                    )
                    summary = ev.get("summary", "(no title)")
                    upsert_event(
                        conn,
                        source="gcal",
                        source_id=f"{cal}:{ev['id']}",
                        started_at=started_at,
                        ended_at=ended_at,
                        duration_seconds=duration,
                        title=summary,
                        details=ev.get("description"),
                    )
                    count += 1
                page_token = resp.get("nextPageToken")
                if not page_token:
                    break
    return count
