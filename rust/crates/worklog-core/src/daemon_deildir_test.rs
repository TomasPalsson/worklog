use axum::body::{self, Body};
use axum::http::{Request, StatusCode};
use tower::ServiceExt; // for `.oneshot`

use crate::daemon::router;
use crate::db::open_memory;

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
