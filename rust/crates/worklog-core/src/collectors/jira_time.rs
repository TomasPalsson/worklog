use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::blocking::Client;

use super::jira::{get_all_pages, get_json, str_at, JiraAuth};
use crate::estimate_progress_contract::RawWorklog;
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
}
