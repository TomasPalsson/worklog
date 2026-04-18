"""GitHub collector: pulls commits and PRs authored by the user in a date range.

Uses the REST search API to avoid per-repo enumeration. Commits and PRs are
stored with stable source_ids so re-running is idempotent.
"""

from __future__ import annotations

from datetime import date, datetime, timedelta

import httpx
from dateutil.parser import isoparse

from worklog.classify import classify
from worklog.config import Settings, load_companies
from worklog.db import connect, init_db, upsert_event

GH_API = "https://api.github.com"


def collect(
    *,
    since: date,
    until: date | None = None,
    settings: Settings | None = None,
) -> int:
    """Fetch commits + PRs between `since` and `until` (exclusive). Returns count."""
    settings = settings or Settings()
    if not settings.github_token:
        raise RuntimeError("WORKLOG_GITHUB_TOKEN not set")

    until = until or (date.today() + timedelta(days=1))
    headers = {
        "Authorization": f"Bearer {settings.github_token}",
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
    }
    user = settings.github_user
    count = 0
    config = load_companies()
    init_db()

    with httpx.Client(headers=headers, timeout=30) as client, connect() as conn:
        # Commits
        q = f"author:{user} author-date:{since.isoformat()}..{until.isoformat()}"
        r = client.get(f"{GH_API}/search/commits", params={"q": q, "per_page": 100})
        r.raise_for_status()
        for item in r.json().get("items", []):
            sha = item["sha"]
            repo = item["repository"]["full_name"]
            commit = item["commit"]
            ts = isoparse(commit["author"]["date"])
            title = commit["message"].splitlines()[0][:200]
            company = classify(config, repo=repo)
            upsert_event(
                conn,
                source="github_commit",
                source_id=sha,
                started_at=ts,
                duration_seconds=None,
                title=title,
                details=commit["message"],
                repo=repo,
                company=company,
            )
            count += 1

        # PRs (opened, reviewed, or merged by user)
        q = (
            f"author:{user} type:pr "
            f"created:{since.isoformat()}..{until.isoformat()}"
        )
        r = client.get(f"{GH_API}/search/issues", params={"q": q, "per_page": 100})
        r.raise_for_status()
        for item in r.json().get("items", []):
            repo = "/".join(item["repository_url"].split("/")[-2:])
            ts = isoparse(item["created_at"])
            company = classify(config, repo=repo)
            upsert_event(
                conn,
                source="github_pr",
                source_id=str(item["id"]),
                started_at=ts,
                ended_at=isoparse(item["closed_at"]) if item.get("closed_at") else None,
                title=f"PR #{item['number']}: {item['title']}",
                details=item.get("body"),
                repo=repo,
                company=company,
            )
            count += 1

    return count


def parse_date(s: str) -> date:
    return datetime.strptime(s, "%Y-%m-%d").date()
