use axum::body::{self, Body};
use axum::http::{Request, StatusCode};
use chrono::{Duration, Utc};
use rusqlite::params;
use tower::ServiceExt; // for `.oneshot`

use crate::daemon::router;
use crate::db::open_memory;

async fn read_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn insert_change(conn: &rusqlite::Connection, started_at: &str, batch: &str) {
    conn.execute(
        "INSERT INTO block_changes (day, started_at, field, old_value, new_value, source, batch)
         VALUES ('2026-09-24', ?1, 'customer', 'A', 'B', 'user', ?2)",
        params![started_at, batch],
    )
    .unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn feed_after_cursor_returns_only_newer_changes_with_batches_and_cursor() {
    let conn = open_memory().unwrap();
    insert_change(&conn, "2026-09-24T09:00:00Z", "b1");
    let cutoff: i64 = conn
        .query_row("SELECT MAX(id) FROM block_changes", [], |r| r.get(0))
        .unwrap();
    insert_change(&conn, "2026-09-24T10:00:00Z", "b2");

    let state = crate::daemon::state_from_conn(conn);
    let resp = router(state)
        .oneshot(
            Request::get(format!("/changes?after={cutoff}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = read_json(resp).await;
    let changes = v["changes"].as_array().unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0]["started_at"], "2026-09-24T10:00:00Z");
    let batches = v["batches"].as_array().unwrap();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0]["batch"], "b2");
    assert!(v["cursor"].as_i64().unwrap() > cutoff);
}

#[tokio::test(flavor = "current_thread")]
async fn unseen_lists_unseen_and_seen_marks_them() {
    let conn = open_memory().unwrap();
    insert_change(&conn, "2026-09-24T09:00:00Z", "b1");
    let up_to: i64 = conn
        .query_row("SELECT MAX(id) FROM block_changes", [], |r| r.get(0))
        .unwrap();

    let state = crate::daemon::state_from_conn(conn);

    let resp = router(state.clone())
        .oneshot(
            Request::get("/changes/unseen")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = read_json(resp).await;
    assert_eq!(v["changes"].as_array().unwrap().len(), 1);

    let resp = router(state.clone())
        .oneshot(
            Request::post("/changes/seen")
                .header("content-type", "application/json")
                .body(Body::from(format!(r#"{{"up_to":{up_to}}}"#)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(read_json(resp).await["marked"], 1);

    // B11: a second catch-up is empty once the first has been seen.
    let resp = router(state)
        .oneshot(
            Request::get("/changes/unseen")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(read_json(resp).await["changes"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn unseen_purges_changes_older_than_thirty_days() {
    let conn = open_memory().unwrap();
    let stale = (Utc::now() - Duration::days(31))
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();
    conn.execute(
        "INSERT INTO block_changes (day, started_at, field, old_value, new_value, source, batch, created_at)
         VALUES ('2026-01-01','2026-01-01T09:00:00Z','customer','A','B','user','old-batch', ?1)",
        params![stale],
    )
    .unwrap();

    let state = crate::daemon::state_from_conn(conn);
    let resp = router(state.clone())
        .oneshot(
            Request::get("/changes/unseen")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(read_json(resp).await["changes"]
        .as_array()
        .unwrap()
        .is_empty());

    let conn = state.conn.lock().await;
    let remaining: i64 = conn
        .query_row("SELECT COUNT(*) FROM block_changes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(remaining, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn feed_rejects_non_integer_after() {
    let state = crate::daemon::state_from_conn(open_memory().unwrap());
    let resp = router(state)
        .oneshot(
            Request::get("/changes?after=abc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
