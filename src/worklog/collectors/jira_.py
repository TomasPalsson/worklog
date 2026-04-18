"""Jira collector: fetches issues touched by the user in a date range.

Named `jira_` to avoid shadowing the installed `jira` package.
"""

from __future__ import annotations

from datetime import date, timedelta

from dateutil.parser import isoparse
from jira import JIRA

from worklog.classify import classify
from worklog.config import Settings, load_companies
from worklog.db import connect, init_db, upsert_event


def collect(
    *,
    since: date,
    until: date | None = None,
    settings: Settings | None = None,
) -> int:
    settings = settings or Settings()
    if not (settings.jira_base_url and settings.jira_email and settings.jira_token):
        raise RuntimeError("WORKLOG_JIRA_BASE_URL / JIRA_EMAIL / JIRA_TOKEN not set")
    until = until or (date.today() + timedelta(days=1))

    client = JIRA(
        server=settings.jira_base_url,
        basic_auth=(settings.jira_email, settings.jira_token),
    )
    jql = (
        f'assignee = currentUser() AND updated >= "{since.isoformat()}" '
        f'AND updated < "{until.isoformat()}"'
    )
    issues = client.search_issues(jql, maxResults=200, fields="summary,updated,project")

    config = load_companies()
    init_db()
    count = 0

    with connect() as conn:
        for issue in issues:
            key = issue.key
            updated = isoparse(issue.fields.updated)
            project_key = key.split("-", 1)[0]
            company = classify(config, jira_issue=key)
            upsert_event(
                conn,
                source="jira",
                source_id=key,
                started_at=updated,
                title=f"{key}: {issue.fields.summary}",
                details=f"project={project_key}",
                jira_issue=key,
                company=company,
            )
            count += 1
    return count
