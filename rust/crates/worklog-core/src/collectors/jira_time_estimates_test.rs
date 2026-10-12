use super::*;
use crate::http;
use httpmock::prelude::*;
use serde_json::json;

fn auth(server: &MockServer) -> JiraAuth {
    JiraAuth {
        base_url: server.base_url(),
        email: "a@b.is".into(),
        token: "tok".into(),
    }
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
fn estimates_follow_remaining_when_jira_has_it() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/rest/api/3/search/jql");
        then.status(200).json_body(json!({"issues": [
            // GENAI-1987: 34h estimate, remaining hand-lowered to 27h, nothing logged.
            {"key": "G-1", "fields": {"timetracking": {"originalEstimateSeconds": 122400,
                "remainingEstimateSeconds": 97200}}},
            // Normal: 34h estimate, 10h logged, Jira auto-lowered remaining to 24h.
            {"key": "G-2", "fields": {"timetracking": {"originalEstimateSeconds": 122400,
                "remainingEstimateSeconds": 86400, "timeSpentSeconds": 36000}}},
            // Overrun: 40h logged on 34h, Jira floors remaining at 0.
            {"key": "G-3", "fields": {"timetracking": {"originalEstimateSeconds": 122400,
                "remainingEstimateSeconds": 0, "timeSpentSeconds": 144000}}},
            // No remaining field: the estimate as before.
            {"key": "G-4", "fields": {"timetracking": {"originalEstimateSeconds": 14400}}},
            // Done early: remaining zeroed after 1h of a 4h estimate.
            {"key": "G-5", "fields": {"timetracking": {"originalEstimateSeconds": 14400,
                "remainingEstimateSeconds": 0, "timeSpentSeconds": 3600}}},
            // Remaining only, no original estimate.
            {"key": "G-6", "fields": {"timetracking": {"remainingEstimateSeconds": 7200}}},
            // Remaining zeroed with nothing logged: keep the estimate, not a 0 budget.
            {"key": "G-7", "fields": {"timetracking": {"originalEstimateSeconds": 14400,
                "remainingEstimateSeconds": 0}}}
        ]}));
    });
    let keys: Vec<String> = ["G-1", "G-2", "G-3", "G-4", "G-5", "G-6", "G-7"]
        .map(String::from)
        .to_vec();
    let got = fetch_estimates_with(&auth(&server), &keys, &http::client().unwrap()).unwrap();
    assert_eq!(got["G-1"], Some(97200)); // wrong: 122400, the bar shows "of 34h"
    assert_eq!(got["G-2"], Some(122400));
    assert_eq!(got["G-3"], Some(122400)); // wrong: 144000 hides the overrun
    assert_eq!(got["G-4"], Some(14400));
    assert_eq!(got["G-5"], Some(3600));
    assert_eq!(got["G-6"], Some(7200));
    assert_eq!(got["G-7"], Some(14400)); // wrong: Some(0) hides the bar
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
