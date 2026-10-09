use axum::body::{self, Body};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use crate::daemon::{router, state_from_conn};
use crate::db::open_memory;

async fn call(path: &str) -> (StatusCode, serde_json::Value) {
    let state = state_from_conn(open_memory().unwrap());
    let response = router(state)
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_default())
}

#[tokio::test(flavor = "current_thread")]
async fn stats_returns_the_contract_keys() {
    let (status, v) = call("/stats?from=2026-09-01&to=2026-09-03").await;
    assert_eq!(status, StatusCode::OK);
    for key in [
        "from",
        "to",
        "today",
        "first_day",
        "totals",
        "daily",
        "punchcard",
        "flow_blocks",
        "tools",
        "shell",
        "slack_channels",
        "domains",
        "folders",
        "helpers",
        "tickets",
        "prompt",
        "records",
        "estimates",
        "ticket_origin",
        "sync",
    ] {
        assert!(v.get(key).is_some(), "missing {key}");
    }
    assert_eq!(v["daily"].as_array().unwrap().len(), 3);
    assert_eq!(v["punchcard"].as_array().unwrap().len(), 7);
}

#[tokio::test(flavor = "current_thread")]
async fn stats_defaults_to_an_empty_one_day_range() {
    let (status, v) = call("/stats").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["daily"].as_array().unwrap().len(), 1);
    assert!(v["first_day"].is_null());
}

#[tokio::test(flavor = "current_thread")]
async fn stats_rejects_bad_ranges_and_dates() {
    for path in [
        "/stats?from=2026-09-05&to=2026-09-04",
        "/stats?from=soon",
        "/stats?to=2026-13-40",
        "/stats?from=0001-01-01&to=9999-12-31",
        "/stats?from=1960-01-01&to=1960-01-02",
    ] {
        assert_eq!(call(path).await.0, StatusCode::BAD_REQUEST, "{path}");
    }
}
