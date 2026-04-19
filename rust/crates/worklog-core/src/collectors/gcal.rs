//! Google Calendar collector — ports `src/worklog/collectors/gcal.py`.
//!
//! Drop-in compatible with the Python runtime: reads/writes the same
//! `google_credentials.json` + `google_token.json` files under
//! `~/.config/worklog/` (or `$WORKLOG_HOME`), calls the same v3 REST
//! endpoint, and upserts events as `source="gcal"` with a stable
//! `source_id = "<calendar>:<eventId>"` so the Python-era data survives
//! the port without any migration.
//!
//! Synchronous / blocking by design — matches every other collector
//! (github, jira, tempo). We reach OAuth2's token endpoint directly with
//! reqwest rather than pulling in `yup-oauth2` or `oauth2` because the
//! surface we need is tiny (parse a JSON file, refresh via
//! `grant_type=refresh_token`, persist) and we already depend on
//! reqwest + serde_json. One fewer transitive dep tree to vet.

use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Duration, NaiveDate, SecondsFormat, Utc};
use reqwest::blocking::Client;
use rusqlite::Connection;
use serde::Deserialize;
use tracing::{debug, warn};

use crate::models::Event;
use crate::repo;

use super::CollectReport;

/// Google Calendar v3 production base. Tests swap in an httpmock URL.
pub const GCAL_API: &str = "https://www.googleapis.com/calendar/v3";

/// Google's OAuth2 token endpoint. Tests swap in an httpmock URL.
pub const OAUTH_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// Single scope the Python version requested — read-only calendar access.
pub const SCOPE: &str = "https://www.googleapis.com/auth/calendar.readonly";

/// Env var that overrides the calendar list. Comma-separated. Matches
/// the Python `WORKLOG_GOOGLE_CALENDARS` behaviour.
pub const ENV_CALENDARS: &str = "WORKLOG_GOOGLE_CALENDARS";

/// Authentication + configuration for Google Calendar.
///
/// Token and credentials paths both live under `Paths::config_dir`.
/// `api_base` / `oauth_base` are constructor-injectable so httpmock
/// tests never escape to the real internet.
#[derive(Debug, Clone)]
pub struct GcalAuth {
    pub token_path: PathBuf,
    pub credentials_path: PathBuf,
    pub calendars: Vec<String>,
    pub api_base: String,
    pub oauth_base: String,
}

impl GcalAuth {
    /// Resolve from `Paths` + environment. Returns the struct even if
    /// credentials files are missing — `collect_with` surfaces that
    /// error with an actionable message.
    pub fn from_paths() -> Result<Self> {
        let paths = crate::paths::Paths::resolve()?;
        let calendars = std::env::var(ENV_CALENDARS)
            .ok()
            .map(|s| {
                s.split(',')
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| vec!["primary".into()]);
        Ok(Self {
            token_path: paths.config_dir.join("google_token.json"),
            credentials_path: paths.config_dir.join("google_credentials.json"),
            calendars,
            api_base: GCAL_API.into(),
            oauth_base: OAUTH_TOKEN_URL.into(),
        })
    }
}

/// Minimal mirror of Google's authorized-user JSON. The Python code
/// round-trips via `Credentials.from_authorized_user_file` /
/// `Credentials.to_json()` which produces exactly this shape.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredToken {
    pub token: String,
    pub refresh_token: String,
    pub token_uri: String,
    pub client_id: String,
    pub client_secret: String,
    pub scopes: Vec<String>,
    /// RFC3339 expiry. Optional because Google occasionally omits it
    /// from the first-auth response; treat missing as "expired".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiry: Option<String>,
}

/// Normalise a Google-Calendar `dateTime` or `date` string to a UTC
/// RFC3339 string, matching Python `_to_utc` semantics:
///
/// * `"2026-04-18T09:00:00+02:00"` → `"2026-04-18T07:00:00Z"`
/// * `"2026-04-18T09:00:00Z"` → `"2026-04-18T09:00:00Z"`
/// * `"2026-04-18"` (all-day) → `"2026-04-18T00:00:00Z"`
///
/// Normalisation is load-bearing — the bucketer does lexicographic
/// comparison on `started_at`, so non-UTC offsets silently land events
/// on the wrong day.
pub fn to_utc(raw: &str) -> Result<String> {
    // Full RFC3339 with offset (incl. Z) — most common path.
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
        return Ok(dt
            .with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::Secs, true));
    }
    // All-day event: bare `YYYY-MM-DD`. Anchor at midnight UTC so the
    // bucketer's lexicographic `started_at` range lands it on the
    // stated day, matching Python `datetime.combine(d, time.min)`.
    if let Ok(date) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        let midnight = date
            .and_hms_opt(0, 0, 0)
            .expect("00:00:00 is always a valid time")
            .and_utc();
        return Ok(midnight.to_rfc3339_opts(SecondsFormat::Secs, true));
    }
    Err(anyhow!(
        "gcal: could not parse date/datetime {raw:?} \
         (expected RFC3339 datetime or YYYY-MM-DD)"
    ))
}

/// Read `token.json`, POST a refresh if the stored token has expired
/// (or will within 60s), write the updated token back to disk, and
/// return the live access token.
pub fn refresh_access_token(auth: &GcalAuth, client: &Client) -> Result<String> {
    let raw = std::fs::read_to_string(&auth.token_path).with_context(|| {
        format!(
            "reading {} — run `worklog collect gcal --auth` to re-authenticate",
            auth.token_path.display()
        )
    })?;
    let mut token: StoredToken = serde_json::from_str(&raw)
        .with_context(|| format!("parsing {} as JSON", auth.token_path.display()))?;

    if !needs_refresh(&token) {
        return Ok(token.token);
    }

    if token.refresh_token.is_empty() {
        return Err(anyhow!(
            "gcal: token at {} is expired and has no refresh_token — \
             re-authenticate with `worklog collect gcal --auth`",
            auth.token_path.display()
        ));
    }
    if token.client_id.is_empty() || token.client_secret.is_empty() {
        return Err(anyhow!(
            "gcal: token at {} is missing client_id/client_secret; \
             re-authenticate with `worklog collect gcal --auth`",
            auth.token_path.display()
        ));
    }

    debug!("gcal: refreshing access token");
    let form = [
        ("grant_type", "refresh_token"),
        ("refresh_token", token.refresh_token.as_str()),
        ("client_id", token.client_id.as_str()),
        ("client_secret", token.client_secret.as_str()),
    ];
    let resp = client
        .post(&auth.oauth_base)
        .form(&form)
        .send()
        .context("POST to OAuth token endpoint")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_else(|_| "<unreadable>".into());
        return Err(anyhow!(
            "gcal: token refresh failed ({status}): {body} — \
             re-authenticate with `worklog collect gcal --auth`"
        ));
    }
    let parsed: RefreshResponse = resp.json().context("parsing refresh response")?;
    let expires_in = parsed.expires_in.unwrap_or(3600);
    let new_expiry = Utc::now() + Duration::seconds(expires_in);
    token.token = parsed.access_token.clone();
    token.expiry = Some(new_expiry.to_rfc3339_opts(SecondsFormat::Secs, true));
    if let Some(parent) = auth.token_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::write(
        &auth.token_path,
        serde_json::to_string_pretty(&token).context("serialising refreshed token")?,
    )
    .with_context(|| format!("writing {}", auth.token_path.display()))?;

    Ok(parsed.access_token)
}

/// Treat "no expiry" as expired — the Python client does the same via
/// `Credentials.expired` returning True when `expiry` is None.
fn needs_refresh(token: &StoredToken) -> bool {
    let Some(expiry) = token.expiry.as_deref() else {
        return true;
    };
    let Ok(expiry) = DateTime::parse_from_rfc3339(expiry) else {
        warn!(expiry, "gcal: unparseable expiry in token.json — forcing refresh");
        return true;
    };
    // 60s buffer so a refresh kicked off right before expiry completes
    // before the backend would reject the old access_token.
    expiry.with_timezone(&Utc) <= Utc::now() + Duration::seconds(60)
}

#[derive(Debug, Deserialize)]
struct RefreshResponse {
    access_token: String,
    /// Seconds. Google almost always sets this but the spec marks it
    /// optional, so we treat missing as "1h" (Google's default).
    #[serde(default)]
    expires_in: Option<i64>,
}

/// Fetch events in `[since, until)` from every configured calendar and
/// upsert them as `source="gcal"`, `source_id="<cal>:<eventId>"`.
///
/// Convenience wrapper around `collect_with` that builds a default
/// reqwest client.
pub fn collect(
    conn: &Connection,
    auth: &GcalAuth,
    since: NaiveDate,
    until: NaiveDate,
) -> Result<CollectReport> {
    collect_with(conn, auth, since, until, &crate::http::client()?)
}

/// Test-injectable variant. Every HTTP call goes through `client`;
/// swap in an httpmock client in tests.
pub fn collect_with(
    conn: &Connection,
    auth: &GcalAuth,
    since: NaiveDate,
    until: NaiveDate,
    client: &Client,
) -> Result<CollectReport> {
    ensure_credentials_exist(auth)?;
    let access_token = refresh_access_token(auth, client)?;

    let time_min = format!("{since}T00:00:00Z");
    let time_max = format!("{until}T00:00:00Z");

    let mut report = CollectReport {
        source: "gcal",
        ..Default::default()
    };

    for cal in &auth.calendars {
        let mut page_token: Option<String> = None;
        loop {
            let url = format!(
                "{}/calendars/{}/events",
                auth.api_base.trim_end_matches('/'),
                urlencoding::encode(cal)
            );
            let mut query: Vec<(&str, String)> = vec![
                ("timeMin", time_min.clone()),
                ("timeMax", time_max.clone()),
                ("singleEvents", "true".into()),
                ("orderBy", "startTime".into()),
                ("maxResults", "250".into()),
            ];
            if let Some(t) = &page_token {
                query.push(("pageToken", t.clone()));
            }
            let resp = client
                .get(&url)
                .bearer_auth(&access_token)
                .query(&query)
                .send()
                .with_context(|| format!("GET {url}"))?;
            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().unwrap_or_else(|_| "<unreadable>".into());
                return Err(anyhow!(
                    "gcal: events.list failed for {cal} ({status}): {body}"
                ));
            }
            let page: EventsList = resp.json().context("parsing events.list response")?;
            for ev in page.items.into_iter() {
                if ev.status.as_deref() == Some("cancelled") {
                    report.skipped += 1;
                    continue;
                }
                let Some(start_raw) = ev.start.as_ref().and_then(GcalDate::pick) else {
                    debug!(id = %ev.id, "gcal: skipping event with no start");
                    report.skipped += 1;
                    continue;
                };
                let started_at = to_utc(start_raw)?;
                let ended_at = match ev.end.as_ref().and_then(GcalDate::pick) {
                    Some(s) => Some(to_utc(s)?),
                    None => None,
                };
                let duration_seconds = match (&ended_at, &started_at) {
                    (Some(e), s) => {
                        let a = DateTime::parse_from_rfc3339(s)?;
                        let b = DateTime::parse_from_rfc3339(e)?;
                        Some((b - a).num_seconds())
                    }
                    _ => None,
                };
                let event = Event {
                    id: None,
                    source: "gcal".into(),
                    source_id: format!("{cal}:{}", ev.id),
                    started_at,
                    ended_at,
                    duration_seconds,
                    title: ev.summary.unwrap_or_else(|| "(no title)".into()),
                    details: ev.description,
                    repo: None,
                    project_path: None,
                    jira_issue: None,
                    session_id: None,
                    tempo_worklog_id: None,
                    raw_json: None,
                };
                repo::upsert_event(conn, &event)?;
                report.events_written += 1;
            }
            match page.next_page_token {
                Some(next) if !next.is_empty() => page_token = Some(next),
                _ => break,
            }
        }
    }

    Ok(report)
}

/// Fail before the first HTTP call when neither a token nor credentials
/// file exist on disk. The collector is best-effort up to that point,
/// but a totally unauthenticated call would just emit a confusing HTTP
/// error; the actionable message names the exact file the user needs.
fn ensure_credentials_exist(auth: &GcalAuth) -> Result<()> {
    if auth.token_path.is_file() {
        return Ok(());
    }
    if !auth.credentials_path.is_file() {
        return Err(anyhow!(
            "gcal: missing {} — download the OAuth client credentials \
             from Google Cloud Console and save them there, then run \
             `worklog collect gcal --auth` to complete first-time login",
            auth.credentials_path.display()
        ));
    }
    Err(anyhow!(
        "gcal: {} exists but {} is missing — run `worklog collect gcal --auth` \
         to complete first-time OAuth login",
        auth.credentials_path.display(),
        auth.token_path.display()
    ))
}

/// Google's `events.list` response. Only the fields we actually read.
#[derive(Debug, Deserialize)]
struct EventsList {
    #[serde(default)]
    items: Vec<GcalEvent>,
    #[serde(default, rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GcalEvent {
    id: String,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    start: Option<GcalDate>,
    #[serde(default)]
    end: Option<GcalDate>,
}

#[derive(Debug, Deserialize)]
struct GcalDate {
    #[serde(default, rename = "dateTime")]
    date_time: Option<String>,
    #[serde(default)]
    date: Option<String>,
}

impl GcalDate {
    fn pick(&self) -> Option<&str> {
        self.date_time.as_deref().or(self.date.as_deref())
    }
}

/// Default token.json path — exposed so `worklog doctor` can show it.
pub fn default_token_path(paths: &crate::paths::Paths) -> PathBuf {
    paths.config_dir.join("google_token.json")
}

/// Default credentials.json path — exposed so `worklog doctor` can show it.
pub fn default_credentials_path(paths: &crate::paths::Paths) -> PathBuf {
    paths.config_dir.join("google_credentials.json")
}

// ───────────────────────── tests (RED phase) ─────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;
    use crate::repo;
    use httpmock::prelude::*;
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    // ------- to_utc -----------------------------------------------------

    #[test]
    fn to_utc_rfc3339_with_non_utc_offset_is_normalised() {
        // Same instant as 07:00:00 UTC; the bucketer compares strings,
        // so the +02:00 form would sort after "2026-04-18T08:00:00Z"
        // and end up on the wrong day without normalisation.
        let got = to_utc("2026-04-18T09:00:00+02:00").unwrap();
        assert_eq!(got, "2026-04-18T07:00:00Z");
    }

    #[test]
    fn to_utc_rfc3339_already_utc_round_trips() {
        let got = to_utc("2026-04-18T09:00:00Z").unwrap();
        assert_eq!(got, "2026-04-18T09:00:00Z");
    }

    #[test]
    fn to_utc_bare_date_anchors_at_midnight_utc() {
        // All-day events arrive with `date` only, no `dateTime`. The
        // Python version returned `datetime.combine(d, time.min)` with
        // UTC tzinfo; we pin the same shape here so bucketing still
        // lands them on the stated day.
        let got = to_utc("2026-04-18").unwrap();
        assert_eq!(got, "2026-04-18T00:00:00Z");
    }

    #[test]
    fn to_utc_rejects_garbage() {
        // Actionable error; a user with a corrupt calendar feed needs
        // to see which value failed to parse, not a silent skip.
        let err = to_utc("not a date").unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("not a date"),
            "expected the bad input in the error message, got: {msg}"
        );
    }

    // ------- refresh_access_token --------------------------------------

    fn write_token(path: &std::path::Path, expiry: Option<&str>, refresh_token: &str) {
        let tok = StoredToken {
            token: "old-access".into(),
            refresh_token: refresh_token.into(),
            token_uri: OAUTH_TOKEN_URL.into(),
            client_id: "cid".into(),
            client_secret: "csecret".into(),
            scopes: vec![SCOPE.into()],
            expiry: expiry.map(str::to_owned),
        };
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, serde_json::to_string(&tok).unwrap()).unwrap();
    }

    fn auth_for(tmp: &std::path::Path, oauth_base: String) -> GcalAuth {
        GcalAuth {
            token_path: tmp.join("google_token.json"),
            credentials_path: tmp.join("google_credentials.json"),
            calendars: vec!["primary".into()],
            api_base: "http://api.invalid".into(),
            oauth_base,
        }
    }

    #[test]
    fn refresh_access_token_posts_and_writes_back_when_expired() {
        let server = MockServer::start();
        let refresh_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/token")
                .body_contains("grant_type=refresh_token")
                .body_contains("refresh_token=rt-xyz");
            then.status(200).json_body(json!({
                "access_token": "new-access",
                "expires_in": 3600,
                "token_type": "Bearer",
            }));
        });

        let tmp = tempdir().unwrap();
        let token_path = tmp.path().join("google_token.json");
        // Expired one hour ago.
        write_token(&token_path, Some("2020-01-01T00:00:00Z"), "rt-xyz");

        let auth = auth_for(tmp.path(), format!("{}/token", server.base_url()));
        let client = crate::http::client().unwrap();
        let access = refresh_access_token(&auth, &client).unwrap();
        assert_eq!(access, "new-access");

        refresh_mock.assert();

        // token.json must be rewritten with the new access token so the
        // next invocation can skip the refresh roundtrip.
        let persisted: StoredToken =
            serde_json::from_str(&fs::read_to_string(&token_path).unwrap()).unwrap();
        assert_eq!(persisted.token, "new-access");
        assert_eq!(persisted.refresh_token, "rt-xyz");
        assert!(
            persisted.expiry.is_some(),
            "expiry must be recomputed after refresh"
        );
    }

    #[test]
    fn refresh_access_token_skips_network_when_still_valid() {
        // Expiry is an hour in the future — the function must return
        // the cached token without any HTTP traffic. We arm a mock that
        // WOULD respond to /token and then assert zero hits on it; that
        // beats observing absence-of-network directly.
        let server = MockServer::start();
        let refresh_mock = server.mock(|when, then| {
            when.method(POST).path("/token");
            then.status(200).json_body(json!({
                "access_token": "should-not-be-called",
                "expires_in": 3600,
            }));
        });

        let tmp = tempdir().unwrap();
        let token_path = tmp.path().join("google_token.json");
        let future = (chrono::Utc::now() + chrono::Duration::hours(1))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        write_token(&token_path, Some(&future), "rt-valid");

        let auth = auth_for(tmp.path(), format!("{}/token", server.base_url()));
        let client = crate::http::client().unwrap();
        let access = refresh_access_token(&auth, &client).unwrap();
        assert_eq!(access, "old-access");
        refresh_mock.assert_hits(0);
    }

    #[test]
    fn refresh_access_token_missing_refresh_token_errors_actionably() {
        let tmp = tempdir().unwrap();
        let token_path = tmp.path().join("google_token.json");
        // Expiry in the past AND no refresh_token — unrecoverable.
        write_token(&token_path, Some("2020-01-01T00:00:00Z"), "");

        let auth = auth_for(tmp.path(), "http://unreachable.invalid/token".into());
        let client = crate::http::client().unwrap();
        let err = refresh_access_token(&auth, &client).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.to_lowercase().contains("re-auth")
                || msg.to_lowercase().contains("refresh")
                || msg.to_lowercase().contains("worklog collect gcal"),
            "expected re-auth guidance, got: {msg}"
        );
    }

    // ------- collect_with ----------------------------------------------

    /// Set up a mock Google Calendar + OAuth server and a valid token.
    fn setup_gcal_env(
        tmp: &std::path::Path,
    ) -> (MockServer, GcalAuth, Connection) {
        let server = MockServer::start();
        let token_path = tmp.join("google_token.json");
        let future = (chrono::Utc::now() + chrono::Duration::hours(1))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        write_token(&token_path, Some(&future), "rt-xyz");

        let auth = GcalAuth {
            token_path: token_path.clone(),
            credentials_path: tmp.join("google_credentials.json"),
            calendars: vec!["primary".into()],
            api_base: server.base_url(),
            oauth_base: format!("{}/token", server.base_url()),
        };
        let conn = open_memory().unwrap();
        (server, auth, conn)
    }

    #[test]
    fn collect_writes_events_with_stable_source_id() {
        let tmp = tempdir().unwrap();
        let (server, auth, conn) = setup_gcal_env(tmp.path());

        server.mock(|when, then| {
            when.method(GET)
                .path("/calendars/primary/events")
                .query_param("timeMin", "2026-04-18T00:00:00Z")
                .query_param("timeMax", "2026-04-19T00:00:00Z");
            then.status(200).json_body(json!({
                "items": [{
                    "id": "evt-1",
                    "status": "confirmed",
                    "summary": "1:1 with Alice",
                    "description": "weekly sync",
                    "start": {"dateTime": "2026-04-18T09:00:00+02:00"},
                    "end":   {"dateTime": "2026-04-18T10:00:00+02:00"},
                }],
            }));
        });

        let report = collect_with(
            &conn,
            &auth,
            NaiveDate::from_ymd_opt(2026, 4, 18).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 19).unwrap(),
            &crate::http::client().unwrap(),
        )
        .unwrap();
        assert_eq!(report.events_written, 1);
        assert_eq!(report.source, "gcal");

        let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
        assert_eq!(events.len(), 1);
        let ev = &events[0];
        assert_eq!(ev.source, "gcal");
        assert_eq!(ev.source_id, "primary:evt-1");
        assert_eq!(ev.started_at, "2026-04-18T07:00:00Z");
        assert_eq!(ev.ended_at.as_deref(), Some("2026-04-18T08:00:00Z"));
        assert_eq!(ev.duration_seconds, Some(3600));
        assert_eq!(ev.title, "1:1 with Alice");
        assert_eq!(ev.details.as_deref(), Some("weekly sync"));
    }

    #[test]
    fn collect_skips_cancelled_events() {
        let tmp = tempdir().unwrap();
        let (server, auth, conn) = setup_gcal_env(tmp.path());

        server.mock(|when, then| {
            when.method(GET).path("/calendars/primary/events");
            then.status(200).json_body(json!({
                "items": [
                    {
                        "id": "evt-cancelled",
                        "status": "cancelled",
                        "summary": "Skip me",
                        "start": {"dateTime": "2026-04-18T09:00:00Z"},
                        "end":   {"dateTime": "2026-04-18T10:00:00Z"},
                    },
                    {
                        "id": "evt-kept",
                        "status": "confirmed",
                        "summary": "Keep me",
                        "start": {"dateTime": "2026-04-18T11:00:00Z"},
                        "end":   {"dateTime": "2026-04-18T12:00:00Z"},
                    },
                ],
            }));
        });

        collect_with(
            &conn,
            &auth,
            NaiveDate::from_ymd_opt(2026, 4, 18).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 19).unwrap(),
            &crate::http::client().unwrap(),
        )
        .unwrap();

        let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source_id, "primary:evt-kept");
    }

    #[test]
    fn collect_handles_all_day_events() {
        let tmp = tempdir().unwrap();
        let (server, auth, conn) = setup_gcal_env(tmp.path());

        server.mock(|when, then| {
            when.method(GET).path("/calendars/primary/events");
            then.status(200).json_body(json!({
                "items": [{
                    "id": "evt-allday",
                    "status": "confirmed",
                    "summary": "Public holiday",
                    "start": {"date": "2026-04-18"},
                    "end":   {"date": "2026-04-19"},
                }],
            }));
        });

        collect_with(
            &conn,
            &auth,
            NaiveDate::from_ymd_opt(2026, 4, 18).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 19).unwrap(),
            &crate::http::client().unwrap(),
        )
        .unwrap();

        let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
        assert_eq!(events.len(), 1);
        let ev = &events[0];
        assert_eq!(ev.started_at, "2026-04-18T00:00:00Z");
        assert_eq!(ev.ended_at.as_deref(), Some("2026-04-19T00:00:00Z"));
        // 24h = 86400s. Pins duration computation for all-day.
        assert_eq!(ev.duration_seconds, Some(86_400));
    }

    #[test]
    fn collect_is_idempotent_on_rerun() {
        let tmp = tempdir().unwrap();
        let (server, auth, conn) = setup_gcal_env(tmp.path());

        server.mock(|when, then| {
            when.method(GET).path("/calendars/primary/events");
            then.status(200).json_body(json!({
                "items": [{
                    "id": "evt-dup",
                    "status": "confirmed",
                    "summary": "Daily standup",
                    "start": {"dateTime": "2026-04-18T09:00:00Z"},
                    "end":   {"dateTime": "2026-04-18T09:15:00Z"},
                }],
            }));
        });

        let since = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
        let until = NaiveDate::from_ymd_opt(2026, 4, 19).unwrap();
        let client = crate::http::client().unwrap();
        collect_with(&conn, &auth, since, until, &client).unwrap();
        collect_with(&conn, &auth, since, until, &client).unwrap();

        let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
        assert_eq!(
            events.len(),
            1,
            "dedup on (source='gcal', source_id='primary:evt-dup') must hold"
        );
    }

    #[test]
    fn collect_paginates_via_page_token() {
        let tmp = tempdir().unwrap();
        let (server, auth, conn) = setup_gcal_env(tmp.path());

        // First page — has nextPageToken.
        server.mock(|when, then| {
            when.method(GET)
                .path("/calendars/primary/events")
                .matches(|req| {
                    let q = req.query_params.clone().unwrap_or_default();
                    !q.iter().any(|(k, _)| k == "pageToken")
                });
            then.status(200).json_body(json!({
                "items": [{
                    "id": "evt-p1",
                    "status": "confirmed",
                    "summary": "First",
                    "start": {"dateTime": "2026-04-18T09:00:00Z"},
                    "end":   {"dateTime": "2026-04-18T09:30:00Z"},
                }],
                "nextPageToken": "page-2",
            }));
        });
        // Second page — echoes pageToken=page-2.
        server.mock(|when, then| {
            when.method(GET)
                .path("/calendars/primary/events")
                .query_param("pageToken", "page-2");
            then.status(200).json_body(json!({
                "items": [{
                    "id": "evt-p2",
                    "status": "confirmed",
                    "summary": "Second",
                    "start": {"dateTime": "2026-04-18T11:00:00Z"},
                    "end":   {"dateTime": "2026-04-18T11:30:00Z"},
                }],
            }));
        });

        collect_with(
            &conn,
            &auth,
            NaiveDate::from_ymd_opt(2026, 4, 18).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 19).unwrap(),
            &crate::http::client().unwrap(),
        )
        .unwrap();

        let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
        assert_eq!(events.len(), 2, "both pages must be consumed");
        let ids: Vec<_> = events.iter().map(|e| e.source_id.as_str()).collect();
        assert!(ids.contains(&"primary:evt-p1"));
        assert!(ids.contains(&"primary:evt-p2"));
    }

    #[test]
    fn collect_missing_token_and_credentials_errors_actionably() {
        let tmp = tempdir().unwrap();
        // NB: no token.json, no credentials.json written.
        let auth = GcalAuth {
            token_path: tmp.path().join("google_token.json"),
            credentials_path: tmp.path().join("google_credentials.json"),
            calendars: vec!["primary".into()],
            api_base: "http://api.invalid".into(),
            oauth_base: "http://unreachable.invalid/token".into(),
        };
        let conn = open_memory().unwrap();

        let err = collect_with(
            &conn,
            &auth,
            NaiveDate::from_ymd_opt(2026, 4, 18).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 19).unwrap(),
            &crate::http::client().unwrap(),
        )
        .unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("google_credentials.json") || msg.contains("credentials"),
            "expected credentials path in error, got: {msg}"
        );
    }
}
