from __future__ import annotations

from worklog.config import CompaniesConfig


def classify(
    config: CompaniesConfig,
    *,
    project_path: str | None = None,
    repo: str | None = None,
    jira_issue: str | None = None,
    gcal_calendar: str | None = None,
    gcal_summary: str | None = None,
) -> str | None:
    """Resolve which company a piece of activity belongs to. First match wins."""
    for company in config.companies:
        if project_path:
            for prefix in company.path_prefixes:
                if project_path.startswith(prefix):
                    return company.name
        if repo and repo in company.github_repos:
            return company.name
        if jira_issue:
            project_key = jira_issue.split("-", 1)[0]
            if project_key in company.jira_projects:
                return company.name
        if gcal_calendar and gcal_calendar in company.gcal_calendars:
            return company.name
        if gcal_summary:
            lowered = gcal_summary.lower()
            if any(kw.lower() in lowered for kw in company.gcal_keywords):
                return company.name
    return config.default_company
