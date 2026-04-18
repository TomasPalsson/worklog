from worklog.classify import classify
from worklog.config import CompaniesConfig, CompanyRule


def _cfg() -> CompaniesConfig:
    return CompaniesConfig(
        default_company="Default",
        companies=[
            CompanyRule(
                name="Acme",
                path_prefixes=["/Users/tomas/projects/acme-"],
                github_repos=["AcmeCorp/api"],
                jira_projects=["ACME"],
                gcal_keywords=["acme"],
            ),
            CompanyRule(
                name="Side",
                jira_projects=["SIDE"],
            ),
        ],
    )


def test_path_match() -> None:
    assert classify(_cfg(), project_path="/Users/tomas/projects/acme-api") == "Acme"


def test_repo_match() -> None:
    assert classify(_cfg(), repo="AcmeCorp/api") == "Acme"


def test_jira_match() -> None:
    assert classify(_cfg(), jira_issue="SIDE-42") == "Side"


def test_keyword_match() -> None:
    assert classify(_cfg(), gcal_summary="Acme standup") == "Acme"


def test_default_fallback() -> None:
    assert classify(_cfg(), repo="other/repo") == "Default"
