from __future__ import annotations

from pathlib import Path

import yaml
from pydantic import BaseModel, Field
from pydantic_settings import BaseSettings, SettingsConfigDict

CONFIG_DIR = Path.home() / ".config" / "worklog"
DATA_DIR = Path.home() / ".local" / "share" / "worklog"
DB_PATH = DATA_DIR / "worklog.db"
COMPANIES_PATH = CONFIG_DIR / "companies.yaml"


class CompanyRule(BaseModel):
    """Maps an activity source to a company.

    Match order inside a company: path_prefixes → github_repos → jira_projects →
    gcal_calendars → gcal_keywords. First match wins across all companies.
    """

    name: str
    tempo_account_key: str | None = None
    jira_issue_default: str | None = None
    path_prefixes: list[str] = Field(default_factory=list)
    github_repos: list[str] = Field(default_factory=list)
    jira_projects: list[str] = Field(default_factory=list)
    gcal_calendars: list[str] = Field(default_factory=list)
    gcal_keywords: list[str] = Field(default_factory=list)


class CompaniesConfig(BaseModel):
    companies: list[CompanyRule] = Field(default_factory=list)
    default_company: str | None = None


class Settings(BaseSettings):
    model_config = SettingsConfigDict(
        env_file=".env",
        env_prefix="WORKLOG_",
        extra="ignore",
    )

    github_token: str | None = None
    github_user: str = "TomasPalsson"

    jira_base_url: str | None = None
    jira_email: str | None = None
    jira_token: str | None = None

    tempo_token: str | None = None
    tempo_base_url: str = "https://api.tempo.io/4"

    google_credentials_path: Path = CONFIG_DIR / "google_credentials.json"
    google_token_path: Path = CONFIG_DIR / "google_token.json"
    google_calendars: list[str] = Field(default_factory=lambda: ["primary"])


def load_companies() -> CompaniesConfig:
    if not COMPANIES_PATH.exists():
        return CompaniesConfig()
    with COMPANIES_PATH.open() as f:
        data = yaml.safe_load(f) or {}
    return CompaniesConfig(**data)


def ensure_dirs() -> None:
    CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    DATA_DIR.mkdir(parents=True, exist_ok=True)
