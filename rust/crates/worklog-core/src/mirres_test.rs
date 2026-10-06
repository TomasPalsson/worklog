//! Tests for the Mirres billable-status pull and its line storage.

use super::*;
use crate::db;
use crate::tempo_lines;
use httpmock::prelude::*;

const DAY: &str = "2026-10-05";

fn auth(server: &MockServer) -> MirresAuth {
    MirresAuth {
        gateway_url: server.url("/mcp"),
        token_url: server.url("/oauth2/token"),
        client_id: "cid".into(),
        client_secret: "sec".into(),
    }
}

fn keys(k: &[&str]) -> Vec<String> {
    k.iter().map(|s| s.to_string()).collect()
}

fn project(v: Value) -> Project {
    serde_json::from_value(v).unwrap()
}

fn projects_json() -> Value {
    json!({
        "projects": [{
            "tempo_account_key": "ACME", "project_name": "Vefur",
            "project_type": "Útseld vinna", "billable": true,
            "customer": {"short_name": "Acme", "name": "Acme ehf."}
        }],
        "not_found": ["GONE"]
    })
}

#[test]
fn sse_payload_takes_last_data_line_or_plain_text() {
    assert_eq!(sse_payload("{\"a\":1}"), "{\"a\":1}");
    let sse = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1}\n\n";
    assert_eq!(sse_payload(sse), "{\"jsonrpc\":\"2.0\",\"id\":1}");
}

#[test]
fn token_fetch_sends_basic_auth_and_grant_type() {
    let server = MockServer::start();
    let m = server.mock(|when, then| {
        when.method(POST)
            .path("/oauth2/token")
            .header("authorization", http::basic_auth_header("cid", "sec"))
            .body_contains("grant_type=client_credentials");
        then.status(200).json_body(json!({"access_token": "tok-1"}));
    });
    let token = fetch_token_with(&auth(&server), &http::client().unwrap()).unwrap();
    assert_eq!(token, "tok-1");
    m.assert();
}

#[test]
fn get_projects_parses_plain_and_wrapped_shapes() {
    let client = http::client().unwrap();
    let plain = MockServer::start();
    let m = plain.mock(|when, then| {
        when.method(POST)
            .path("/mcp")
            .header("authorization", "Bearer tok")
            .json_body_partial(
                r#"{"method":"tools/call","params":{"name":"mirres___get_projects",
                "arguments":{"account_keys":["ACME","GONE"],"as_of_date":"2026-10-05"}}}"#,
            );
        then.status(200)
            .json_body(json!({"jsonrpc":"2.0","id":1,"result": projects_json()}));
    });
    let r =
        get_projects_with(&auth(&plain), "tok", &keys(&["ACME", "GONE"]), DAY, &client).unwrap();
    m.assert();
    assert_eq!(r.projects.len(), 1);
    assert_eq!(r.not_found, vec!["GONE"]);

    let wrapped = MockServer::start();
    let text = json!({"statusCode": 200, "body": projects_json()}).to_string();
    wrapped.mock(|when, then| {
        when.method(POST).path("/mcp");
        then.status(200).json_body(
            json!({"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text": text}]}}),
        );
    });
    let r = get_projects_with(&auth(&wrapped), "tok", &keys(&["ACME"]), DAY, &client).unwrap();
    assert_eq!(r.projects[0].tempo_account_key, "ACME");
    assert_eq!(r.not_found, vec!["GONE"]);
}

#[test]
fn get_projects_errors_on_is_error_and_rpc_error_and_skips_empty() {
    let client = http::client().unwrap();
    let server = MockServer::start();
    let mut m = server.mock(|when, then| {
        when.method(POST).path("/mcp");
        then.status(200).json_body(json!({"jsonrpc":"2.0","id":1,
            "result":{"isError":true,"content":[{"type":"text","text":"boom"}]}}));
    });
    let err = get_projects_with(&auth(&server), "tok", &keys(&["A"]), DAY, &client).unwrap_err();
    assert!(err.to_string().contains("boom"), "{err}");
    m.delete();
    server.mock(|when, then| {
        when.method(POST).path("/mcp");
        then.status(200)
            .json_body(json!({"jsonrpc":"2.0","id":1,"error":{"code":-1,"message":"nope"}}));
    });
    let err = get_projects_with(&auth(&server), "tok", &keys(&["A"]), DAY, &client).unwrap_err();
    assert!(err.to_string().contains("nope"), "{err}");
    // Empty keys never hit the network (the mock above would error otherwise).
    let none = get_projects_with(&auth(&server), "tok", &[], DAY, &client).unwrap();
    assert!(none.projects.is_empty());
}

#[test]
fn classify_covers_classes_and_warnings() {
    let billable = project(json!({"tempo_account_key": "A", "billable": true}));
    assert_eq!(classify(&billable), (BillingClass::Billable, None));

    let included = project(json!({"tempo_account_key": "A", "billable": false,
        "included_hours": {"contract_status": "OK", "counts_as_billed": true}}));
    assert_eq!(classify(&included), (BillingClass::Included, None));

    let plain = project(json!({"tempo_account_key": "A", "billable": false}));
    assert_eq!(classify(&plain), (BillingClass::NotBillable, None));

    let no_contract = project(json!({"tempo_account_key": "A", "billable": false,
        "included_hours": {"contract_status": "NO_CONTRACT", "counts_as_billed": false}}));
    assert_eq!(
        classify(&no_contract).1.as_deref(),
        Some("Samning vantar í Mirres (NO_CONTRACT)")
    );

    let used_up = project(json!({"tempo_account_key": "A", "billable": false,
        "included_hours": {"contract_status": "OK", "usage_status": "USED_UP", "counts_as_billed": true}}));
    assert_eq!(
        classify(&used_up).1.as_deref(),
        Some("Innifaldir tímar uppurnir")
    );

    let near = project(json!({"tempo_account_key": "A", "billable": false,
        "included_hours": {"contract_status": "OK", "usage_status": "NEAR_LIMIT",
            "remaining_hours": 1.5, "counts_as_billed": true}}));
    assert_eq!(
        classify(&near).1.as_deref(),
        Some("Innifaldir tímar að klárast (1,5 klst eftir)")
    );
}

#[test]
fn fetch_day_billing_maps_ticket_to_account_to_mirres() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/rest/api/3/issue/APRO-1");
        then.status(200)
            .json_body(json!({"fields": {"customfield_10100": {"id": 42, "value": "Acme"}}}));
    });
    server.mock(|when, then| {
        when.method(GET).path("/rest/api/3/issue/APRO-2");
        then.status(200)
            .json_body(json!({"fields": {"customfield_10100": null}}));
    });
    server.mock(|when, then| {
        when.method(GET).path("/rest/api/3/issue/APRO-3");
        then.status(200)
            .json_body(json!({"fields": {"customfield_10100": {"id": 43, "value": "Gone"}}}));
    });
    server.mock(|when, then| {
        when.method(GET).path("/accounts");
        then.status(200).json_body(json!({"results": [
            {"id": 42, "key": "ACME", "name": "Acme"},
            {"id": 43, "key": "GONE", "name": "Gone"}]}));
    });
    server.mock(|when, then| {
        when.method(POST).path("/oauth2/token");
        then.status(200).json_body(json!({"access_token": "tok"}));
    });
    server.mock(|when, then| {
        when.method(POST).path("/mcp");
        then.status(200)
            .json_body(json!({"jsonrpc":"2.0","id":1,"result": projects_json()}));
    });
    let jira_auth = JiraAuth {
        base_url: server.base_url(),
        email: "x".into(),
        token: "t".into(),
    };
    let tempo_auth = TempoAuth {
        token: "tt".into(),
        author: "me".into(),
        base_url: server.base_url(),
    };
    let rows = fetch_day_billing_with(
        DAY,
        &keys(&["APRO-1", "APRO-2", "APRO-3"]),
        &jira_auth,
        "customfield_10100",
        &tempo_auth,
        &auth(&server),
        &http::client().unwrap(),
    )
    .unwrap();
    assert_eq!(rows.len(), 2, "APRO-2 has no account, so no row");
    assert_eq!(rows[0].0, "APRO-1");
    assert_eq!(rows[0].1.project.as_deref(), Some("Acme · Vefur"));
    assert_eq!(rows[0].1.class, BillingClass::Billable);
    assert_eq!(rows[1].0, "APRO-3");
    assert_eq!(
        rows[1].1.warning.as_deref(),
        Some("Ekki virkt Mirres-verkefni")
    );
    assert_eq!(rows[1].1.project, None);
}

fn seed_block(conn: &Connection, issue: &str) {
    conn.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds)
         VALUES (?1, ?2, '2026-10-05T09:00:00Z', '2026-10-05T10:00:00Z', 3600)",
        params![DAY, issue],
    )
    .unwrap();
}

fn billing(project: &str, class: BillingClass) -> LineBilling {
    LineBilling {
        account_key: "ACME".into(),
        project: Some(project.into()),
        project_type: None,
        class,
        warning: None,
    }
}

#[test]
fn store_day_replaces_rows_and_lines_carry_billing() {
    let conn = db::open_memory().unwrap();
    seed_block(&conn, "APRO-1");
    seed_block(&conn, "APRO-2");
    store_day(
        &conn,
        DAY,
        &[("APRO-1".into(), billing("old", BillingClass::NotBillable))],
    )
    .unwrap();
    store_day(
        &conn,
        DAY,
        &[(
            "APRO-1".into(),
            billing("Acme · Vefur", BillingClass::Included),
        )],
    )
    .unwrap();
    let lines = tempo_lines::lines_for_day(&conn, DAY).unwrap();
    let b = lines[0].billing.as_ref().expect("APRO-1 has billing");
    assert_eq!(b.project.as_deref(), Some("Acme · Vefur"));
    assert_eq!(b.class, BillingClass::Included);
    assert_eq!(lines[1].billing, None, "APRO-2 was never stored");
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM mirres_line_billing", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);
}
