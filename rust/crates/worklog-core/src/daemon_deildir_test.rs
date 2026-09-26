use axum::body::{self, Body};
use axum::http::{Request, StatusCode};
use rusqlite::params;
use tower::ServiceExt; // for `.oneshot`

use crate::billing_registry::{upsert_folder, FolderMap};
use crate::change_log;
use crate::daemon::router;
use crate::db::open_memory;
use crate::deild_contract::{ChangeField, ChangeSource};
use crate::models::Event;
use crate::repo;

async fn read_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn deild_routes_round_trip_and_appear_in_registry() {
    let state = crate::daemon::state_from_conn(open_memory().unwrap());

    let resp = router(state.clone())
        .oneshot(
            Request::post("/billing/deildir")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"customer":"Sjúkra","name":"Rekstur","keywords":["ops"]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let id = read_json(resp).await["id"].as_i64().unwrap();

    // GET /billing/registry gains `deildir`, design.md's contract.
    let resp = router(state.clone())
        .oneshot(
            Request::get("/billing/registry")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = read_json(resp).await;
    let deildir = v["deildir"].as_array().unwrap();
    assert_eq!(deildir.len(), 1);
    assert_eq!(deildir[0]["customer"], "Sjúkra");
    assert_eq!(deildir[0]["name"], "Rekstur");
    assert_eq!(deildir[0]["keywords"], serde_json::json!(["ops"]));

    let resp = router(state.clone())
        .oneshot(
            Request::post(format!("/billing/deildir/{id}/delete"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(read_json(resp).await["removed"], true);

    let resp = router(state)
        .oneshot(
            Request::get("/billing/registry")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(read_json(resp).await["deildir"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn deild_route_rejects_empty_name() {
    let state = crate::daemon::state_from_conn(open_memory().unwrap());

    let resp = router(state)
        .oneshot(
            Request::post("/billing/deildir")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"customer":"Sjúkra","name":"   "}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test(flavor = "current_thread")]
async fn deild_route_rejects_duplicate_customer_name() {
    let state = crate::daemon::state_from_conn(open_memory().unwrap());

    let resp = router(state.clone())
        .oneshot(
            Request::post("/billing/deildir")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"customer":"Sjúkra","name":"Rekstur"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = router(state)
        .oneshot(
            Request::post("/billing/deildir")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"customer":"Sjúkra","name":"Rekstur","keywords":["ops"]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let err = read_json(resp).await["error"].as_str().unwrap().to_owned();
    assert!(err.contains("Sjúkra") && err.contains("Rekstur"), "{err}");
}

/// A4: a deild upsert whose keyword matches today's block re-resolves it —
/// logged as a Deild change with source Keyword.
#[tokio::test(flavor = "current_thread")]
async fn deild_upsert_logs_a_keyword_change_for_todays_block() {
    let conn = open_memory().unwrap();
    let today = crate::tz::local_date(chrono::Utc::now()).to_string();

    upsert_folder(
        &conn,
        &FolderMap {
            id: None,
            folder: "acme".into(),
            customer: Some("Sjúkra".into()),
            verkefni: None,
            billable: true,
            multi_tenant: false,
        },
    )
    .unwrap();

    conn.execute(
        &format!(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds, description)
             VALUES ('{today}', '{today}T09:00:00Z', '{today}T09:30:00Z', 1800, 'ops work')"
        ),
        [],
    )
    .unwrap();
    let block_id = conn.last_insert_rowid();

    let mut event = Event::minimal("claude", "e1", format!("{today}T09:05:00Z"), "worked");
    event.project_path = Some("/tmp/acme".into());
    let event_id = repo::upsert_event(&conn, &event).unwrap();
    conn.execute(
        "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
        params![block_id, event_id],
    )
    .unwrap();

    // Seed the pre-upsert snapshot: Sjúkra has no matching deild yet, so
    // the block's deild is Blank.
    change_log::refresh_day(&conn, &today, ChangeSource::Rebuild, "seed").unwrap();

    let state = crate::daemon::state_from_conn(conn);
    let resp = router(state.clone())
        .oneshot(
            Request::post("/billing/deildir")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"customer":"Sjúkra","name":"Vöktun","keywords":["ops"]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let conn = state.conn.lock().await;
    let changes = change_log::feed(&conn, 0).unwrap().changes;
    let deild_change = changes
        .iter()
        .find(|c| c.field == ChangeField::Deild)
        .expect("deild upsert must re-resolve today's block");
    assert_eq!(deild_change.source, ChangeSource::Keyword);
}
