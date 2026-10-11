use super::*;
use crate::models::JiraTicket;

#[test]
fn load_open_tickets_offers_external_internal_catch_alls() {
    let conn = crate::db::open_memory().unwrap();
    crate::repo::upsert_external_ticket(
        &conn,
        &JiraTicket {
            key: "APRO-7".into(),
            summary: "[O] Samstilling (Allir innri fundir)".into(),
            status: Some("In Progress".into()),
            project_key: Some("APRO".into()),
            updated: Some("2026-04-18T11:00:00Z".into()),
            issue_id: None,
        },
    )
    .unwrap();
    let candidates = load_open_tickets(&conn, "2026-04-20".parse().unwrap()).unwrap();
    let keys: Vec<&str> = candidates.iter().map(|c| c.key.as_str()).collect();
    assert_eq!(keys, vec!["APRO-7"], "APRO catch-all must be offered");
    assert!(
        SYSTEM_PROMPT.contains("APRO-"),
        "prompt must explain the internal APRO- catch-all tickets"
    );
}
