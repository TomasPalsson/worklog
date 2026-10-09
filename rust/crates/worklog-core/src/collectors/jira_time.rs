use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::blocking::Client;

use super::jira::{get_all_pages, get_json, str_at, JiraAuth};
use super::tempo::{get_hub_json, TempoAuth};
use crate::estimate_progress_contract::RawWorklog;
use crate::tempo_hub_contract::TEMPO_PAGE_LIMIT;
use crate::tz;

/// Original estimate in seconds per requested key. A key Jira does not
/// return, or one without an estimate, maps to `None`, never 0.
pub fn fetch_estimates_with(
    auth: &JiraAuth,
    keys: &[String],
    client: &Client,
) -> Result<HashMap<String, Option<i64>>> {
    if keys.is_empty() {
        return Ok(HashMap::new());
    }
    let quoted: Vec<String> = keys
        .iter()
        .map(|k| format!("\"{}\"", k.replace(['"', '\\'], "")))
        .collect();
    let jql = format!("key in ({})", quoted.join(","));
    let url = format!("{}/rest/api/3/search/jql", auth.base_url);
    let body = get_json(
        auth,
        &url,
        &[
            ("jql", jql.as_str()),
            ("maxResults", &keys.len().to_string()),
            ("fields", "timetracking"),
        ],
        "search estimates",
        client,
    )?;
    let mut out: HashMap<String, Option<i64>> = keys.iter().map(|k| (k.clone(), None)).collect();
    for issue in body["issues"].as_array().into_iter().flatten() {
        if let Some(key) = str_at(issue, &["key"]) {
            out.insert(
                key,
                issue["fields"]["timetracking"]["originalEstimateSeconds"].as_i64(),
            );
        }
    }
    Ok(out)
}

/// Every author's worklogs on `key`, across all pages.
pub fn fetch_worklogs_with(auth: &JiraAuth, key: &str, client: &Client) -> Result<Vec<RawWorklog>> {
    let url = format!("{}/rest/api/3/issue/{key}/worklog", auth.base_url);
    get_all_pages(auth, &url, "worklogs", "issue worklogs", client)?
        .iter()
        // Worklogs by deleted users carry no accountId; they can't be attributed.
        .filter_map(|w| Some(raw_worklog(w, str_at(w, &["author", "accountId"])?)))
        .collect()
}

/// Every author's worklogs on `key` as Tempo reports them. Jira's own worklog
/// list only shows the Tempo app as author, so real people come from Tempo.
pub fn fetch_tempo_worklogs_with(
    tempo: &TempoAuth,
    jira: &JiraAuth,
    key: &str,
    client: &Client,
) -> Result<Vec<RawWorklog>> {
    let issue = get_json(
        jira,
        &format!("{}/rest/api/3/issue/{key}", jira.base_url),
        &[("fields", "summary")],
        "issue id",
        client,
    )?;
    let id = str_at(&issue, &["id"]).context("issue has no id")?;
    let url = format!("{}/worklogs/issue/{id}", tempo.base_url);
    let mut raw: Vec<serde_json::Value> = Vec::new();
    loop {
        let page: serde_json::Value = get_hub_json(
            tempo,
            client,
            &url,
            &[
                ("limit", TEMPO_PAGE_LIMIT.to_string()),
                ("offset", raw.len().to_string()),
            ],
        )?;
        let results = page["results"].as_array().cloned().unwrap_or_default();
        let full_page = results.len() == TEMPO_PAGE_LIMIT;
        raw.extend(results);
        if !full_page {
            break;
        }
    }
    let mut names: HashMap<String, String> = HashMap::new();
    raw.iter()
        .map(|w| {
            let account_id =
                str_at(w, &["author", "accountId"]).context("worklog has no author")?;
            let start = str_at(w, &["startDateTimeUtc"]).context("worklog has no start")?;
            let start = DateTime::parse_from_rfc3339(&start)
                .with_context(|| format!("worklog start {start:?}"))?;
            let name = names
                .entry(account_id.clone())
                .or_insert_with(|| {
                    get_json(
                        jira,
                        &format!("{}/rest/api/3/user", jira.base_url),
                        &[("accountId", account_id.as_str())],
                        "user name",
                        client,
                    )
                    .ok()
                    .and_then(|u| str_at(&u, &["displayName"]))
                    .unwrap_or_else(|| account_id.clone())
                })
                .clone();
            Ok(RawWorklog {
                worklog_id: format!("tempo:{}", w["tempoWorklogId"]),
                account_id,
                name,
                day: tz::local_date(start.with_timezone(&Utc)),
                seconds: w["timeSpentSeconds"].as_i64().unwrap_or(0),
            })
        })
        .collect()
}

fn raw_worklog(w: &serde_json::Value, account_id: String) -> Result<RawWorklog> {
    let started = str_at(w, &["started"]).context("worklog has no started")?;
    let started = DateTime::parse_from_str(&started, "%Y-%m-%dT%H:%M:%S%.3f%z")
        .with_context(|| format!("worklog started {started:?}"))?;
    Ok(RawWorklog {
        worklog_id: str_at(w, &["id"]).context("worklog has no id")?,
        name: str_at(w, &["author", "displayName"]).unwrap_or_else(|| account_id.clone()),
        account_id,
        day: tz::local_date(started.with_timezone(&Utc)),
        seconds: w["timeSpentSeconds"].as_i64().unwrap_or(0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http;
    use crate::tempo_hub_contract::TEMPO_PAGE_LIMIT;
    use chrono::NaiveDate;
    use httpmock::prelude::*;
    use serde_json::json;

    fn auth(server: &MockServer) -> JiraAuth {
        JiraAuth {
            base_url: server.base_url(),
            email: "a@b.is".into(),
            token: "tok".into(),
        }
    }

    fn wl(id: &str, acct: &str, name: &str, started: &str, secs: i64) -> serde_json::Value {
        json!({"id": id, "author": {"accountId": acct, "displayName": name},
               "started": started, "timeSpentSeconds": secs})
    }

    #[test]
    fn estimates_parse_seconds_and_missing_is_none_not_zero() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/rest/api/3/search/jql");
            then.status(200).json_body(json!({"issues": [
                {"key": "G-1", "fields": {"timetracking": {"originalEstimateSeconds": 14400}}},
                {"key": "G-2", "fields": {"timetracking": {}}},
                {"key": "G-3", "fields": {}}
            ]}));
        });
        let keys: Vec<String> = ["G-1", "G-2", "G-3", "G-4"].map(String::from).to_vec();
        let got = fetch_estimates_with(&auth(&server), &keys, &http::client().unwrap()).unwrap();
        assert_eq!(got["G-1"], Some(14400));
        assert_eq!(got["G-2"], None); // wrong: unwrap_or(0)
        assert_eq!(got["G-3"], None); // wrong: unwrap_or(0)
        assert_eq!(got["G-4"], None); // wrong: omit keys Jira did not return
        assert_eq!(got.len(), 4);
    }

    #[test]
    fn estimates_empty_keys_make_no_request() {
        let server = MockServer::start();
        let m = server.mock(|when, then| {
            when.any_request();
            then.status(200).json_body(json!({"issues": []}));
        });
        let got = fetch_estimates_with(&auth(&server), &[], &http::client().unwrap()).unwrap();
        assert!(got.is_empty());
        assert_eq!(m.hits(), 0); // wrong: search with `key in ()`
    }

    #[test]
    fn estimates_http_error_is_an_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.any_request();
            then.status(500);
        });
        let keys = vec!["G-1".to_string()];
        // wrong: swallow into an empty map
        assert!(fetch_estimates_with(&auth(&server), &keys, &http::client().unwrap()).is_err());
    }

    #[test]
    fn worklogs_follow_pages_and_keep_every_author() {
        let server = MockServer::start();
        let p1 = server.mock(|when, then| {
            when.method(GET)
                .path("/rest/api/3/issue/G-1/worklog")
                .query_param("startAt", "0");
            then.status(200)
                .json_body(json!({"startAt": 0, "total": 3, "worklogs": [
                    wl("1", "me", "Tomas", "2026-04-17T09:00:00.000+0000", 7200),
                    wl("2", "jon", "Jón Geir", "2026-04-17T10:00:00.000+0000", 3600)
                ]}));
        });
        let p2 = server.mock(|when, then| {
            when.method(GET)
                .path("/rest/api/3/issue/G-1/worklog")
                .query_param("startAt", "2");
            then.status(200)
                .json_body(json!({"startAt": 2, "total": 3, "worklogs": [
                    wl("3", "jon", "Jón Geir", "2026-04-18T10:00:00.000+0000", 5400)
                ]}));
        });
        let got = fetch_worklogs_with(&auth(&server), "G-1", &http::client().unwrap()).unwrap();
        p1.assert();
        p2.assert(); // wrong: first page only
        assert_eq!(got.len(), 3);
        assert_eq!(
            got[2],
            RawWorklog {
                worklog_id: "3".into(),
                account_id: "jon".into(),
                name: "Jón Geir".into(),
                day: NaiveDate::from_ymd_opt(2026, 4, 18).unwrap(),
                seconds: 5400,
            }
        );
        assert_eq!(got[0].name, "Tomas"); // wrong: only the caller's worklogs
    }

    #[test]
    fn worklog_day_is_taken_in_utc_not_from_the_offset_text() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/rest/api/3/issue/G-1/worklog");
            then.status(200)
                .json_body(json!({"startAt": 0, "total": 1, "worklogs": [
                    wl("1", "me", "Tomas", "2026-04-17T23:30:00.000-0500", 60)
                ]}));
        });
        let got = fetch_worklogs_with(&auth(&server), "G-1", &http::client().unwrap()).unwrap();
        // 23:30-05:00 is 04:30 UTC on the 18th; wrong: slice the first 10 chars
        assert_eq!(got[0].day, NaiveDate::from_ymd_opt(2026, 4, 18).unwrap());
    }

    #[test]
    fn worklog_without_account_id_is_skipped_and_bad_started_errors() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/rest/api/3/issue/G-1/worklog");
            then.status(200)
                .json_body(json!({"startAt": 0, "total": 2, "worklogs": [
                    {"id": "9", "author": {"displayName": "Gone"},
                     "started": "2026-04-17T09:00:00.000+0000", "timeSpentSeconds": 60},
                    wl("1", "me", "Tomas", "2026-04-17T09:00:00.000+0000", 60)
                ]}));
        });
        let got = fetch_worklogs_with(&auth(&server), "G-1", &http::client().unwrap()).unwrap();
        assert_eq!(got.len(), 1); // wrong: attribute to an empty account id
        assert_eq!(got[0].worklog_id, "1");

        let bad = MockServer::start();
        bad.mock(|when, then| {
            when.method(GET).path("/rest/api/3/issue/G-1/worklog");
            then.status(200)
                .json_body(json!({"startAt": 0, "total": 1, "worklogs": [
                    wl("1", "me", "Tomas", "not-a-date", 60)
                ]}));
        });
        // wrong: default to today
        assert!(fetch_worklogs_with(&auth(&bad), "G-1", &http::client().unwrap()).is_err());
    }

    #[test]
    fn worklogs_http_error_is_an_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.any_request();
            then.status(401);
        });
        // wrong: Ok(vec![])
        assert!(fetch_worklogs_with(&auth(&server), "G-1", &http::client().unwrap()).is_err());
    }

    fn tempo_auth(server: &MockServer) -> TempoAuth {
        TempoAuth {
            token: "ttok".into(),
            author: "me".into(),
            base_url: server.base_url(),
        }
    }

    fn tw(id: i64, acct: &str, start: &str, secs: i64) -> serde_json::Value {
        json!({"tempoWorklogId": id, "author": {"accountId": acct},
               "startDateTimeUtc": start, "timeSpentSeconds": secs})
    }

    fn issue_mock(jira: &MockServer) {
        jira.mock(|when, then| {
            when.method(GET).path("/rest/api/3/issue/G-1");
            then.status(200).json_body(json!({"id": "10042"}));
        });
    }

    #[test]
    fn tempo_worklogs_map_fields_and_look_up_each_name_once() {
        let jira = MockServer::start();
        let tempo = MockServer::start();
        issue_mock(&jira);
        let page = tempo.mock(|when, then| {
            when.method(GET)
                .path("/worklogs/issue/10042")
                .header("authorization", "Bearer ttok")
                .query_param("offset", "0");
            then.status(200).json_body(json!({"results": [
                tw(7, "jon", "2026-04-17T23:30:00Z", 3600),
                tw(8, "jon", "2026-04-18T01:00:00Z", 1800),
                tw(9, "me", "2026-04-17T09:00:00Z", 60)
            ]}));
        });
        let jon = jira.mock(|when, then| {
            when.method(GET)
                .path("/rest/api/3/user")
                .query_param("accountId", "jon");
            then.status(200)
                .json_body(json!({"displayName": "Jón Geir"}));
        });
        jira.mock(|when, then| {
            when.method(GET)
                .path("/rest/api/3/user")
                .query_param("accountId", "me");
            then.status(200).json_body(json!({"displayName": "Tomas"}));
        });
        let got = fetch_tempo_worklogs_with(
            &tempo_auth(&tempo),
            &auth(&jira),
            "G-1",
            &http::client().unwrap(),
        )
        .unwrap();
        assert_eq!(
            got[0],
            RawWorklog {
                worklog_id: "tempo:7".into(), // wrong: bare id collides with Jira ids
                account_id: "jon".into(),
                name: "Jón Geir".into(),
                day: NaiveDate::from_ymd_opt(2026, 4, 17).unwrap(),
                seconds: 3600,
            }
        );
        assert_eq!(got.len(), 3);
        assert_eq!(got[1].name, "Jón Geir");
        assert_eq!(got[2].name, "Tomas");
        page.assert_hits(1); // wrong: re-request after a short page
        jon.assert_hits(1); // wrong: one user lookup per worklog
    }

    #[test]
    fn tempo_worklogs_page_by_offset_until_a_short_page() {
        let jira = MockServer::start();
        let tempo = MockServer::start();
        issue_mock(&jira);
        jira.mock(|when, then| {
            when.method(GET).path("/rest/api/3/user");
            then.status(200).json_body(json!({"displayName": "Jon"}));
        });
        let full: Vec<_> = (0..TEMPO_PAGE_LIMIT as i64)
            .map(|i| tw(i, "jon", "2026-04-17T09:00:00Z", 60))
            .collect();
        let p1 = tempo.mock(|when, then| {
            when.method(GET)
                .path("/worklogs/issue/10042")
                .query_param("offset", "0");
            then.status(200).json_body(json!({ "results": full }));
        });
        let p2 = tempo.mock(|when, then| {
            when.method(GET)
                .path("/worklogs/issue/10042")
                .query_param("offset", TEMPO_PAGE_LIMIT.to_string());
            then.status(200)
                .json_body(json!({"results": [tw(5000, "jon", "2026-04-18T09:00:00Z", 120)]}));
        });
        let got = fetch_tempo_worklogs_with(
            &tempo_auth(&tempo),
            &auth(&jira),
            "G-1",
            &http::client().unwrap(),
        )
        .unwrap();
        p1.assert_hits(1);
        p2.assert_hits(1); // wrong: stop after a full first page
        assert_eq!(got.len(), TEMPO_PAGE_LIMIT + 1);
        assert_eq!(got[TEMPO_PAGE_LIMIT].worklog_id, "tempo:5000");
    }

    #[test]
    fn tempo_worklog_name_falls_back_to_account_id_when_lookup_fails() {
        let jira = MockServer::start();
        let tempo = MockServer::start();
        issue_mock(&jira);
        jira.mock(|when, then| {
            when.method(GET).path("/rest/api/3/user");
            then.status(404);
        });
        tempo.mock(|when, then| {
            when.method(GET).path("/worklogs/issue/10042");
            then.status(200)
                .json_body(json!({"results": [tw(1, "jon", "2026-04-17T09:00:00Z", 60)]}));
        });
        let got = fetch_tempo_worklogs_with(
            &tempo_auth(&tempo),
            &auth(&jira),
            "G-1",
            &http::client().unwrap(),
        )
        .unwrap();
        assert_eq!(got[0].name, "jon"); // wrong: propagate the lookup error
    }

    #[test]
    fn tempo_worklogs_issue_lookup_and_tempo_errors_are_errors() {
        let jira = MockServer::start();
        let tempo = MockServer::start();
        jira.mock(|when, then| {
            when.method(GET).path("/rest/api/3/issue/G-1");
            then.status(404);
        });
        // wrong: swallow the issue-id failure into Ok(vec![])
        assert!(fetch_tempo_worklogs_with(
            &tempo_auth(&tempo),
            &auth(&jira),
            "G-1",
            &http::client().unwrap()
        )
        .is_err());

        let jira = MockServer::start();
        issue_mock(&jira);
        tempo.mock(|when, then| {
            when.any_request();
            then.status(500);
        });
        // wrong: treat a Tempo failure as no worklogs
        assert!(fetch_tempo_worklogs_with(
            &tempo_auth(&tempo),
            &auth(&jira),
            "G-1",
            &http::client().unwrap()
        )
        .is_err());
    }
}
