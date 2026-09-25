use super::*;
use axum::body::{self, Body};
use axum::http::{Request, StatusCode};
use rusqlite::params;
use tower::ServiceExt; // for `.oneshot`

use crate::billing_registry::{Customer, FolderMap};
use crate::daemon::{router, state_from_conn};
use crate::db::open_memory;
use crate::models::Event;
use crate::repo;

/// A multi-tenant folder `vitinn-infra` with customer `Acme`, and one
/// block whose only event's `project_path` resolves to that folder (a
/// bare name with no `/`, so `billing::work_folder_for_path`'s fallback —
/// last path segment — returns it verbatim regardless of the host's real
/// `~/Desktop/Work`). Returns the shared state and the seeded block id.
fn seed() -> (Shared, i64) {
    let conn = open_memory().unwrap();
    crate::billing_registry::upsert_folder(
        &conn,
        &FolderMap {
            id: None,
            folder: "vitinn-infra".into(),
            customer: None,
            verkefni: None,
            billable: true,
            multi_tenant: true,
        },
    )
    .unwrap();
    crate::billing_registry::upsert_customer(
        &conn,
        &Customer {
            id: None,
            name: "Acme".into(),
            aliases: Vec::new(),
        },
    )
    .unwrap();

    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
         VALUES ('2026-04-18', '2026-04-18T09:00:00+00:00', '2026-04-18T09:30:00+00:00', 1800)",
        [],
    )
    .unwrap();
    let block_id = conn.last_insert_rowid();

    let mut event = Event::minimal("claude", "a", "2026-04-18T09:05:00+00:00", "worked");
    event.project_path = Some("vitinn-infra".into());
    let event_id = repo::upsert_event(&conn, &event).unwrap();
    conn.execute(
        "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
        params![block_id, event_id],
    )
    .unwrap();

    (state_from_conn(conn), block_id)
}

async fn read_json(resp: axum::response::Response) -> Value {
    let bytes = body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn tenant_routes_round_trip() {
    let (state, block_id) = seed();

    // Bad input: the folder isn't multi-tenant → 400, design.md's exact text.
    let resp = router(state.clone())
        .oneshot(
            Request::post("/billing/tenants/link")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"folder":"not-a-folder","tenant":"t","customer":null,"ignored":false}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(read_json(resp).await["error"], "Folder is not multi-tenant");

    // Bad input: unknown customer on a multi-tenant folder → 400.
    let resp = router(state.clone())
        .oneshot(
            Request::post("/billing/tenants/link")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"folder":"vitinn-infra","tenant":"acme-prod","customer":"Ghost","ignored":false}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(read_json(resp).await["error"], "Customer no longer exists");

    // Round trip: link a tenant, then read the row back from storage.
    let resp = router(state.clone())
        .oneshot(
            Request::post("/billing/tenants/link")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"folder":"vitinn-infra","tenant":"acme-prod","customer":"Acme","ignored":false}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    {
        let conn = state.conn.lock().await;
        let customer: String = conn
            .query_row(
                "SELECT customer FROM billing_tenant_links
                  WHERE folder = 'vitinn-infra' AND tenant = 'acme-prod'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(customer, "Acme");
    }

    // GET /billing/tenants exercises the read route (no tenant dirs on
    // disk in this test, so an empty list is the correct round trip).
    let resp = router(state.clone())
        .oneshot(
            Request::get("/billing/tenants")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(read_json(resp).await.as_array().unwrap().is_empty());

    // Round trip: save customer shares, then read them back as slices.
    let resp = router(state.clone())
        .oneshot(
            Request::post(format!("/blocks/{block_id}/customer-shares"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"shares":{"Acme":1.0}}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = router(state.clone())
        .oneshot(
            Request::get(format!("/blocks/{block_id}/customer-slices"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = read_json(resp).await;
    let slices = v.as_array().unwrap();
    assert_eq!(slices.len(), 1);
    assert_eq!(slices[0]["customer"], "Acme");
    assert_eq!(slices[0]["origin"], "manual");
    assert_eq!(
        slices[0]["intervals"],
        json!([[1_776_502_800i64, 1_776_504_600i64]])
    );

    // Bad input: shares that don't sum to 100% → 400, exact text.
    let resp = router(state.clone())
        .oneshot(
            Request::post(format!("/blocks/{block_id}/customer-shares"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"shares":{"Acme":0.5}}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(read_json(resp).await["error"], "Shares must add up to 100%");

    // Round trip: clear the shares, then re-read — falls back to no split
    // (no clues, no description) instead of staying "manual".
    let resp = router(state.clone())
        .oneshot(
            Request::post(format!("/blocks/{block_id}/customer-shares/clear"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = router(state)
        .oneshot(
            Request::get(format!("/blocks/{block_id}/customer-slices"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = read_json(resp).await;
    let slices = v.as_array().unwrap();
    assert_eq!(slices.len(), 1);
    assert_eq!(slices[0]["origin"], "fallback");
    assert!(slices[0]["customer"].is_null());
}
