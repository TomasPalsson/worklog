use super::*;
use axum::body::{self, Body};
use axum::http::{Request, StatusCode};
use rusqlite::{params, OptionalExtension};
use tower::ServiceExt; // for `.oneshot`

use crate::billing_registry::{Customer, FolderMap};
use crate::daemon::{router, state_from_conn};
use crate::db::open_memory;
use crate::deild_contract::ChangeField;
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

    // Round trip: save customer shares (v2 rows), then read them back as slices.
    let resp = router(state.clone())
        .oneshot(
            Request::post(format!("/blocks/{block_id}/customer-shares"))
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"rows":[{"customer":"Acme","deild":null,"fraction":1.0}]}"#,
                ))
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

    // Bad input: rows that don't sum to 100% → 400, exact text.
    let resp = router(state.clone())
        .oneshot(
            Request::post(format!("/blocks/{block_id}/customer-shares"))
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"rows":[{"customer":"Acme","deild":null,"fraction":0.5}]}"#,
                ))
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

/// A fallback slice (no timestamped clue, and the description doesn't
/// unambiguously name a non-house customer) still bills to the folder's
/// normal pin/text resolution — same ladder as `billing::rows_for_day` —
/// so the card shows "APRÓ · guess" instead of "Unresolved".
#[tokio::test(flavor = "current_thread")]
async fn fallback_slice_fills_customer_from_registry_resolution() {
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
    // "APRÓ" is the House customer — `summary_customer` filters it out
    // when computing the split's own Fallback customer, so it stays
    // `None` there; the folder's normal `registry.resolve` (unfiltered)
    // does still find it in the description.
    crate::billing_registry::upsert_customer(
        &conn,
        &Customer {
            id: None,
            name: "APRÓ".into(),
            aliases: Vec::new(),
        },
    )
    .unwrap();

    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds, description)
         VALUES ('2026-04-18', '2026-04-18T09:00:00+00:00', '2026-04-18T09:30:00+00:00', 1800, 'APRÓ analyzer')",
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

    let state = state_from_conn(conn);
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
    assert_eq!(slices[0]["customer"], "APRÓ");
}

/// A `Fallback` slice's customer must resolve exactly like
/// `billing::rows_for_day`: from the Jira ticket summary when the block
/// itself has no description. Before the fix the route only looked at
/// `block.description`, so a ticket-summary-only match billed one customer
/// on export while the review card showed no customer at all.
#[tokio::test(flavor = "current_thread")]
async fn fallback_slice_uses_ticket_summary_like_billing() {
    let conn = open_memory().unwrap();
    crate::billing_registry::upsert_folder(
        &conn,
        &FolderMap {
            id: None,
            folder: "genai-infra".into(),
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
            name: "Sjúkra".into(),
            aliases: Vec::new(),
        },
    )
    .unwrap();
    conn.execute(
        "INSERT INTO jira_tickets (key, summary) VALUES ('GENAI-1219', ?1)",
        params!["Document analyzer fyrir Sjúkra"],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds)
         VALUES ('2026-04-18', 'GENAI-1219', '2026-04-18T09:00:00+00:00', '2026-04-18T09:30:00+00:00', 1800)",
        [],
    )
    .unwrap();
    let block_id = conn.last_insert_rowid();

    let mut event = Event::minimal("claude", "a", "2026-04-18T09:05:00+00:00", "worked");
    event.project_path = Some("genai-infra".into());
    let event_id = repo::upsert_event(&conn, &event).unwrap();
    conn.execute(
        "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
        params![block_id, event_id],
    )
    .unwrap();

    let state = state_from_conn(conn);
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
    assert_eq!(
        slices[0]["customer"], "Sjúkra",
        "route must resolve the same customer billing::rows_for_day would: {v:#?}"
    );
}

async fn upsert_customer(state: &Shared, name: &str) {
    let conn = state.conn.lock().await;
    crate::billing_registry::upsert_customer(
        &conn,
        &Customer {
            id: None,
            name: name.into(),
            aliases: Vec::new(),
        },
    )
    .unwrap();
}

/// B4: a three-row split naming one customer twice (different deildir)
/// round-trips through the v2 routes exactly.
#[tokio::test(flavor = "current_thread")]
async fn split_rows_round_trip_names_one_customer_twice() {
    let (state, block_id) = seed();
    upsert_customer(&state, "Sjúkra").await;
    upsert_customer(&state, "APRÓ").await;

    let resp = router(state.clone())
        .oneshot(
            Request::post(format!("/blocks/{block_id}/customer-shares"))
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"rows":[
                        {"customer":"Sjúkra","deild":"Rekstur","fraction":0.33},
                        {"customer":"Sjúkra","deild":"Áskrift","fraction":0.33},
                        {"customer":"APRÓ","deild":"AI hraðall","fraction":0.34}
                    ]}"#,
                ))
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
    assert_eq!(slices.len(), 3, "{v:#?}");
    let pairs: Vec<(String, String)> = slices
        .iter()
        .map(|s| {
            (
                s["customer"].as_str().unwrap().to_string(),
                s["deild"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert!(
        pairs.contains(&("Sjúkra".to_string(), "Rekstur".to_string())),
        "{pairs:?}"
    );
    assert!(
        pairs.contains(&("Sjúkra".to_string(), "Áskrift".to_string())),
        "{pairs:?}"
    );
    assert!(
        pairs.contains(&("APRÓ".to_string(), "AI hraðall".to_string())),
        "{pairs:?}"
    );
    assert!(slices.iter().all(|s| s["origin"] == "manual"));
}

/// B5: rows that total 90% are refused, and the block keeps whatever it
/// had before — nothing is written.
#[tokio::test(flavor = "current_thread")]
async fn split_rows_rejects_total_below_100_and_writes_nothing() {
    let (state, block_id) = seed();
    upsert_customer(&state, "Sjúkra").await;

    let resp = router(state.clone())
        .oneshot(
            Request::post(format!("/blocks/{block_id}/customer-shares"))
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"rows":[{"customer":"Sjúkra","deild":null,"fraction":0.9}]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(read_json(resp).await["error"], "Shares must add up to 100%");

    let conn = state.conn.lock().await;
    let rows: Option<String> = conn
        .query_row(
            "SELECT rows_json FROM block_customer_shares WHERE day = '2026-04-18'",
            [],
            |r| r.get(0),
        )
        .optional()
        .unwrap();
    assert!(
        rows.is_none(),
        "a rejected split must write nothing: {rows:?}"
    );
}

/// B5: a row naming an unknown or blank customer is refused.
#[tokio::test(flavor = "current_thread")]
async fn split_rows_rejects_unknown_or_empty_customer() {
    let (state, block_id) = seed();

    let resp = router(state.clone())
        .oneshot(
            Request::post(format!("/blocks/{block_id}/customer-shares"))
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"rows":[{"customer":"Ghost","deild":null,"fraction":1.0}]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(read_json(resp).await["error"], "Customer no longer exists");

    let resp = router(state)
        .oneshot(
            Request::post(format!("/blocks/{block_id}/customer-shares"))
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"rows":[{"customer":"","deild":null,"fraction":1.0}]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(read_json(resp).await["error"], "Customer no longer exists");
}

/// A blank-string deild is no deild — the route normalises it to `null`
/// before it ever reaches storage.
#[tokio::test(flavor = "current_thread")]
async fn save_customer_shares_normalises_blank_deild_to_null() {
    let (state, block_id) = seed();

    let resp = router(state.clone())
        .oneshot(
            Request::post(format!("/blocks/{block_id}/customer-shares"))
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"rows":[{"customer":"Acme","deild":"","fraction":1.0}]}"#,
                ))
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
    let v = read_json(resp).await;
    let slices = v.as_array().unwrap();
    assert_eq!(slices.len(), 1);
    assert!(
        slices[0]["deild"].is_null(),
        "a blank deild must be stored as null, not \"\": {v:#?}"
    );
}

fn insert_bare_block(conn: &rusqlite::Connection, started_at: &str, ended_at: &str) -> i64 {
    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
         VALUES ('2026-04-18', ?1, ?2, 1800)",
        params![started_at, ended_at],
    )
    .unwrap();
    conn.last_insert_rowid()
}

async fn set_rows(state: &Shared, block_id: i64, rows_json: &str) {
    let resp = router(state.clone())
        .oneshot(
            Request::post(format!("/blocks/{block_id}/customer-shares"))
                .header("content-type", "application/json")
                .body(Body::from(format!(r#"{{"rows":{rows_json}}}"#)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

/// FR-13/B13: moving a super block's deild from its line header moves
/// every slice on that line — two whole blocks land fully on the new
/// deild, and a split block only moves the part naming that customer,
/// leaving its other customer's share untouched.
#[tokio::test(flavor = "current_thread")]
async fn move_line_deild_moves_every_slice_on_the_line() {
    let conn = open_memory().unwrap();
    let state = state_from_conn(conn);
    upsert_customer(&state, "APRÓ").await;
    upsert_customer(&state, "Sjúkra").await;

    let (block_a, block_b, block_c) = {
        let conn = state.conn.lock().await;
        (
            insert_bare_block(
                &conn,
                "2026-04-18T09:00:00+00:00",
                "2026-04-18T09:30:00+00:00",
            ),
            insert_bare_block(
                &conn,
                "2026-04-18T10:00:00+00:00",
                "2026-04-18T10:30:00+00:00",
            ),
            insert_bare_block(
                &conn,
                "2026-04-18T11:00:00+00:00",
                "2026-04-18T11:30:00+00:00",
            ),
        )
    };

    set_rows(
        &state,
        block_a,
        r#"[{"customer":"APRÓ","deild":"AI hraðall","fraction":1.0}]"#,
    )
    .await;
    set_rows(
        &state,
        block_b,
        r#"[{"customer":"APRÓ","deild":"AI hraðall","fraction":1.0}]"#,
    )
    .await;
    set_rows(
        &state,
        block_c,
        r#"[{"customer":"APRÓ","deild":"AI hraðall","fraction":0.5},
            {"customer":"Sjúkra","deild":"Rekstur","fraction":0.5}]"#,
    )
    .await;

    let resp = router(state.clone())
        .oneshot(
            Request::post("/billing/lines/deild")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"day":"2026-04-18","block_ids":[{block_a},{block_b},{block_c}],
                        "customer":"APRÓ","from_deild":"AI hraðall","to_deild":"Rekstur"}}"#
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let body = read_json(resp).await;
    assert_eq!(status, StatusCode::OK, "{body:#?}");

    let rows = {
        let conn = state.conn.lock().await;
        crate::billing::rows_for_day(&conn, "2026-04-18").unwrap()
    };

    assert!(
        !rows
            .iter()
            .any(|r| r.verkefni.as_deref() == Some("AI hraðall")),
        "AI hraðall line must be gone: {rows:#?}"
    );

    let apro_rekstur = rows
        .iter()
        .find(|r| r.customer.as_deref() == Some("APRÓ") && r.verkefni.as_deref() == Some("Rekstur"))
        .unwrap_or_else(|| panic!("no APRÓ·Rekstur line: {rows:#?}"));
    assert_eq!(apro_rekstur.block_count, 3, "{rows:#?}");
    assert_eq!(apro_rekstur.seconds, 1800 + 1800 + 900, "{rows:#?}");

    let sjukra_rekstur = rows
        .iter()
        .find(|r| r.customer.as_deref() == Some("Sjúkra"))
        .unwrap_or_else(|| panic!("no Sjúkra line: {rows:#?}"));
    assert_eq!(sjukra_rekstur.verkefni.as_deref(), Some("Rekstur"));
    assert_eq!(sjukra_rekstur.block_count, 1, "{rows:#?}");
    assert_eq!(sjukra_rekstur.seconds, 900, "{rows:#?}");
}

/// A block whose customer-A slice resolves with no customer at all (a
/// blank fallback, not a saved split) can't become a manual row, so the
/// move must fail loudly rather than drop that block's time.
#[tokio::test(flavor = "current_thread")]
async fn move_line_deild_rejects_block_with_unresolved_slice() {
    let conn = open_memory().unwrap();
    let state = state_from_conn(conn);
    upsert_customer(&state, "APRÓ").await;

    let block_id = {
        let conn = state.conn.lock().await;
        insert_bare_block(
            &conn,
            "2026-04-18T09:00:00+00:00",
            "2026-04-18T09:30:00+00:00",
        )
    };

    let resp = router(state.clone())
        .oneshot(
            Request::post("/billing/lines/deild")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"day":"2026-04-18","block_ids":[{block_id}],
                        "customer":"APRÓ","from_deild":null,"to_deild":"Rekstur"}}"#
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let conn = state.conn.lock().await;
    let rows: Option<String> = conn
        .query_row(
            "SELECT rows_json FROM block_customer_shares WHERE day = '2026-04-18'",
            [],
            |r| r.get(0),
        )
        .optional()
        .unwrap();
    assert!(
        rows.is_none(),
        "a rejected move must write nothing: {rows:?}"
    );
}

/// Personal blocks never reach the split routes — both GET and POST
/// refuse them with the same 400.
#[tokio::test(flavor = "current_thread")]
async fn split_routes_reject_personal_blocks() {
    let conn = open_memory().unwrap();
    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds, is_personal)
         VALUES ('2026-04-18', '2026-04-18T09:00:00+00:00', '2026-04-18T09:30:00+00:00', 1800, 1)",
        [],
    )
    .unwrap();
    let block_id = conn.last_insert_rowid();
    let state = state_from_conn(conn);

    let resp = router(state.clone())
        .oneshot(
            Request::get(format!("/blocks/{block_id}/customer-slices"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(read_json(resp).await["error"], "Block is personal");

    let resp = router(state)
        .oneshot(
            Request::post(format!("/blocks/{block_id}/customer-shares"))
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"rows":[{"customer":"Acme","deild":null,"fraction":1.0}]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(read_json(resp).await["error"], "Block is personal");
}

/// B9/B15: a saved split that only re-shuffles fractions between the same
/// customers is logged as a Split change with source User.
#[tokio::test(flavor = "current_thread")]
async fn save_customer_shares_logs_a_user_split_change() {
    let (state, block_id) = seed();
    upsert_customer(&state, "Beta").await;

    let save = |first: f64, second: f64| {
        Request::post(format!("/blocks/{block_id}/customer-shares"))
            .header("content-type", "application/json")
            .body(Body::from(format!(
                r#"{{"rows":[{{"customer":"Acme","deild":null,"fraction":{first}}},{{"customer":"Beta","deild":null,"fraction":{second}}}]}}"#,
            )))
            .unwrap()
    };

    let resp = router(state.clone()).oneshot(save(0.6, 0.4)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = router(state.clone()).oneshot(save(0.5, 0.5)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let conn = state.conn.lock().await;
    let changes = change_log::feed(&conn, 0).unwrap().changes;
    let split_change = changes
        .iter()
        .find(|c| c.field == ChangeField::Split)
        .expect("the second save must log a Split change");
    assert_eq!(split_change.source, ChangeSource::User);
}
