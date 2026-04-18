from __future__ import annotations

import os
from pathlib import Path

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


def _resolve_paths() -> tuple[Path, Path]:
    """Return (config_dir, data_dir). Honours $WORKLOG_HOME so Python
    and Rust agree on which DB to open when a user sets the override.

    Rust's `paths::resolve` collapses $WORKLOG_HOME into a SINGLE
    directory (config_dir == data_dir == root). We match that
    behaviour so a user who sets `WORKLOG_HOME=/tmp/worklog-dev` gets
    `/tmp/worklog-dev/worklog.db` from both languages.
    """
    override = os.environ.get("WORKLOG_HOME")
    if override:
        root = Path(override).expanduser()
        return (root, root)
    return (
        Path.home() / ".config" / "worklog",
        Path.home() / ".local" / "share" / "worklog",
    )


CONFIG_DIR, DATA_DIR = _resolve_paths()
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
