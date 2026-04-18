"""Jira collector: fetches issues touched by the user, and caches the user's
open tickets.

Named `jira_` to avoid shadowing the installed `jira` package.

Two responsibilities:
1. Log Jira issues touched in the date range as events (raw activity stream).
2. Refresh the `jira_tickets` cache of the user's open tickets — feeds the UI
   picker and the estimator's candidate list.
"""

from __future__ import annotations

from datetime import date, timedelta

from dateutil.parser import isoparse
from jira import JIRA

from worklog.config import Settings
from worklog.db import connect, init_db, upsert_event, upsert_jira_ticket

_DONE_CATEGORIES = {"Done", "done", "Complete", "complete"}


def _client(settings: Settings) -> JIRA:
    if not (settings.jira_base_url and settings.jira_email and settings.jira_token):
        raise RuntimeError("WORKLOG_JIRA_BASE_URL / JIRA_EMAIL / JIRA_TOKEN not set")
    # Without `timeout` the `jira` library inherits `requests`' default of
    # no timeout, so a stalled Atlassian API would hang `worklog collect
    # jira` indefinitely. 30s is generous for a JQL query.
    return JIRA(
        server=settings.jira_base_url,
        basic_auth=(settings.jira_email, settings.jira_token),
        timeout=30,
    )


def fetch_open_tickets(
    *,
    settings: Settings | None = None,
    client: JIRA | None = None,
) -> int:
    """Refresh the `jira_tickets` cache with the user's open tickets.

    Returns the number of tickets written.
    """
    settings = settings or Settings()
    client = client or _client(settings)
    jql = "assignee = currentUser() AND statusCategory != Done"
    issues = client.search_issues(
        jql, maxResults=200, fields="summary,status,updated,project"
    )

    init_db()
    count = 0
    with connect() as conn:
        for issue in issues:
            fields = issue.fields
            status_name = getattr(fields.status, "name", None)
            cat = getattr(
                getattr(fields.status, "statusCategory", None), "name", None
            )
            if cat in _DONE_CATEGORIES:
                continue
            upsert_jira_ticket(
                conn,
                key=issue.key,
                summary=fields.summary or "",
                status=status_name,
                project_key=issue.key.split("-", 1)[0],
                updated=fields.updated,
            )
            count += 1
    return count


def collect(
    *,
    since: date,
    until: date | None = None,
    settings: Settings | None = None,
) -> int:
    """Log Jira activity in [since, until) as events and refresh the ticket cache."""
    settings = settings or Settings()
    until = until or (date.today() + timedelta(days=1))

    client = _client(settings)
    jql = (
        f'assignee = currentUser() AND updated >= "{since.isoformat()}" '
        f'AND updated < "{until.isoformat()}"'
    )
    issues = client.search_issues(jql, maxResults=200, fields="summary,updated,project")

    init_db()
    count = 0

    with connect() as conn:
        for issue in issues:
            key = issue.key
            updated = isoparse(issue.fields.updated)
            project_key = key.split("-", 1)[0]
            upsert_event(
                conn,
                source="jira",
                source_id=key,
                started_at=updated,
                title=f"{key}: {issue.fields.summary}",
                details=f"project={project_key}",
                jira_issue=key,
            )
            count += 1

    # Refresh open-ticket cache with the same authenticated client.
    fetch_open_tickets(settings=settings, client=client)
    return count
