from __future__ import annotations

from pathlib import Path

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict

CONFIG_DIR = Path.home() / ".config" / "worklog"
DATA_DIR = Path.home() / ".local" / "share" / "worklog"
DB_PATH = DATA_DIR / "worklog.db"
ENV_PATH = CONFIG_DIR / ".env"


class Settings(BaseSettings):
    model_config = SettingsConfigDict(
        env_file=str(ENV_PATH),
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


def ensure_dirs() -> None:
    CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    DATA_DIR.mkdir(parents=True, exist_ok=True)
