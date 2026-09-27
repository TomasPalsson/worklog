use super::*;
use crate::db::open_memory;

fn fresh() -> Connection {
    open_memory().expect("in-memory db")
}

#[test]
fn upsert_event_dedupes_on_source_pair() {
    let c = fresh();
    let mut e = Event::minimal("github_commit", "abc", "2026-04-18T09:00:00Z", "first");
    let id1 = upsert_event(&c, &e).unwrap();
    e.title = "second".into();
    let id2 = upsert_event(&c, &e).unwrap();
    assert_eq!(id1, id2);
    assert_eq!(count_events(&c).unwrap(), 1);
    let title: String = c
        .query_row(
            "SELECT title FROM events WHERE id = ?1",
            params![id1],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(title, "second");
}

#[test]
fn upsert_event_skips_the_write_when_nothing_changes() {
    // The 15-min tick re-collects thousands of unchanged events; rewriting an
    // identical row still dirties its page and appends a WAL frame.
    let c = fresh();
    let mut e = Event::minimal("github_commit", "abc", "2026-04-18T09:00:00Z", "first");
    e.ended_at = Some("2026-04-18T09:05:00Z".into());
    upsert_event(&c, &e).unwrap();
    // A re-collect without ended_at keeps it (COALESCE) — still no change.
    e.ended_at = None;
    upsert_event(&c, &e).unwrap();
    assert_eq!(
        c.changes(),
        0,
        "identical re-collect must not rewrite the row"
    );
    e.title = "second".into();
    upsert_event(&c, &e).unwrap();
    assert_eq!(c.changes(), 1, "a changed value is still written");
}

#[test]
fn upsert_event_recompresses_raw_json_identically() {
    // S1: raw_json is deflated to a BLOB when that's smaller. Compression
    // must be deterministic, or the no-op WHERE clause (`events.raw_json
    // IS NOT excluded.raw_json`) would see a "changed" BLOB on every
    // re-collect of the same event and rewrite the row every tick.
    let c = fresh();
    let mut e = Event::minimal("github_commit", "abc", "2026-04-18T09:00:00Z", "first");
    e.raw_json = Some(
        r#"{"message":"a fairly repetitive commit message commit message commit message"}"#.into(),
    );
    upsert_event(&c, &e).unwrap();
    upsert_event(&c, &e).unwrap();
    assert_eq!(
        c.changes(),
        0,
        "re-upserting the identical event must not rewrite the row"
    );
    let got = load_day_events(&c, "2026-04-18").unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(
        got[0].raw_json, e.raw_json,
        "raw_json must decode byte-identical"
    );
}

#[test]
fn upsert_event_preserves_tempo_worklog_id_on_re_collect() {
    // CLAUDE.md canary: tempo_worklog_id MUST NEVER be cleared.
    // A re-collect pass (which passes tempo_worklog_id=None on the
    // Event struct because collectors don't know about sync state)
    // must not overwrite a previously-synced value. The ON CONFLICT
    // UPDATE must COALESCE, not unconditionally overwrite.
    let c = fresh();
    let mut e = Event::minimal("github_commit", "abc", "2026-04-18T09:00:00Z", "first");
    let id = upsert_event(&c, &e).unwrap();
    // Simulate the sync step having marked this event as synced.
    c.execute(
        "UPDATE events SET tempo_worklog_id = 'tw-42' WHERE id = ?1",
        params![id],
    )
    .unwrap();
    // Re-collect: the event is seen again, same source_id, no
    // tempo_worklog_id in the Event struct.
    assert!(e.tempo_worklog_id.is_none());
    e.title = "updated".into();
    upsert_event(&c, &e).unwrap();
    let stored: Option<String> = c
        .query_row(
            "SELECT tempo_worklog_id FROM events WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        stored.as_deref(),
        Some("tw-42"),
        "tempo_worklog_id canary must survive re-collect"
    );
}

#[test]
fn upsert_event_preserves_session_id_when_new_is_none() {
    // Parity with Python: session_id uses COALESCE so an existing
    // non-null value is not clobbered by a collector that doesn't
    // know about it.
    let c = fresh();
    let e = Event::minimal("claude", "evt-1", "2026-04-18T09:00:00Z", "first");
    let id = upsert_event(&c, &e).unwrap();
    c.execute(
        "UPDATE events SET session_id = 'sess-a' WHERE id = ?1",
        params![id],
    )
    .unwrap();
    // Re-upsert with session_id = None on the struct.
    upsert_event(&c, &e).unwrap();
    let stored: Option<String> = c
        .query_row(
            "SELECT session_id FROM events WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored.as_deref(), Some("sess-a"));
}

#[test]
fn upsert_event_preserves_project_path_on_re_collect() {
    // routing::set_label writes project_path via a raw UPDATE, never
    // through upsert_event. A later re-collect (rolling-window collect,
    // a resent heartbeat) calls upsert_event with project_path=None on
    // the Event struct — that must not wipe the routing label.
    let c = fresh();
    let e = Event::minimal("firefox", "tab-1", "2026-04-18T09:00:00Z", "first");
    let id = upsert_event(&c, &e).unwrap();
    c.execute(
        "UPDATE events SET project_path = 'aws-cert', label_origin = 'rule' WHERE id = ?1",
        params![id],
    )
    .unwrap();
    assert!(e.project_path.is_none());
    upsert_event(&c, &e).unwrap();
    let stored: Option<String> = c
        .query_row(
            "SELECT project_path FROM events WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        stored.as_deref(),
        Some("aws-cert"),
        "project_path label must survive re-collect"
    );
}

#[test]
fn upsert_event_scrubs_secrets_in_title_and_details() {
    // FR-11/D-03: every stored value is scrubbed, not just raw_json —
    // a collector's title/details must never carry a live secret.
    let c = fresh();
    let token = format!("ghp_{}", "a".repeat(36));
    let mut e = Event::minimal(
        "shell",
        "s1",
        "2026-04-18T09:00:00Z",
        format!("curl -H token {token}"),
    );
    e.details = Some(format!("uses {token} to auth"));
    let id = upsert_event(&c, &e).unwrap();
    let (title, details): (String, Option<String>) = c
        .query_row(
            "SELECT title, details FROM events WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(!title.contains(&token), "title leaked: {title}");
    assert!(
        !details.as_deref().unwrap_or("").contains(&token),
        "details leaked: {details:?}"
    );
}

#[test]
fn load_day_events_filters_by_iso_day() {
    let c = fresh();
    upsert_event(
        &c,
        &Event::minimal("gcal", "a", "2026-04-18T08:00:00Z", "A"),
    )
    .unwrap();
    upsert_event(
        &c,
        &Event::minimal("gcal", "b", "2026-04-18T23:59:59Z", "B"),
    )
    .unwrap();
    upsert_event(
        &c,
        &Event::minimal("gcal", "c", "2026-04-19T00:00:00Z", "C"),
    )
    .unwrap();

    let got = load_day_events(&c, "2026-04-18").unwrap();
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].title, "A");
    assert_eq!(got[1].title, "B");
}

#[test]
fn list_blocks_for_day_orders_by_start() {
    let c = fresh();
    c.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
         VALUES ('2026-04-18','2026-04-18T10:00:00Z','2026-04-18T10:30:00Z',1800),
                ('2026-04-18','2026-04-18T09:00:00Z','2026-04-18T09:15:00Z', 900)",
        [],
    )
    .unwrap();
    let blocks = list_blocks_for_day(&c, "2026-04-18").unwrap();
    assert_eq!(blocks.len(), 2);
    assert!(blocks[0].started_at < blocks[1].started_at);
}

#[test]
fn get_block_returns_none_for_missing_id() {
    let c = fresh();
    assert!(get_block(&c, 9999).unwrap().is_none());
}

#[test]
fn upsert_ticket_updates_summary_in_place() {
    let c = fresh();
    let t = JiraTicket {
        key: "PROJ-1".into(),
        summary: "first".into(),
        status: Some("Open".into()),
        project_key: Some("PROJ".into()),
        updated: Some("2026-04-17T10:00:00Z".into()),
        issue_id: None,
    };
    upsert_ticket(&c, &t).unwrap();
    let mut t2 = t.clone();
    t2.summary = "updated".into();
    upsert_ticket(&c, &t2).unwrap();

    let all = list_tickets(&c).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].summary, "updated");
}

#[test]
fn list_tickets_orders_by_updated_desc() {
    let c = fresh();
    for (k, u) in [
        ("A", "2026-04-17"),
        ("B", "2026-04-18"),
        ("C", "2026-04-16"),
    ] {
        upsert_ticket(
            &c,
            &JiraTicket {
                key: k.into(),
                summary: format!("ticket {k}"),
                status: None,
                project_key: None,
                updated: Some(u.into()),
                issue_id: None,
            },
        )
        .unwrap();
    }
    let keys: Vec<String> = list_tickets(&c)
        .unwrap()
        .into_iter()
        .map(|t| t.key)
        .collect();
    assert_eq!(keys, vec!["B", "A", "C"]);
}

fn external_flag(conn: &Connection, key: &str) -> i64 {
    conn.query_row(
        "SELECT external FROM jira_tickets WHERE key = ?1",
        params![key],
        |r| r.get(0),
    )
    .unwrap()
}

#[test]
fn upsert_external_ticket_inserts_with_external_flag() {
    let c = fresh();
    upsert_external_ticket(
        &c,
        &JiraTicket {
            key: "EXT-1".into(),
            summary: "from search".into(),
            status: Some("To Do".into()),
            project_key: Some("EXT".into()),
            updated: Some("2026-04-17T10:00:00Z".into()),
            issue_id: Some("10001".into()),
        },
    )
    .unwrap();
    assert_eq!(external_flag(&c, "EXT-1"), 1);
    // Still visible to list_tickets so the picker can render its
    // summary once the user has picked it.
    let all = list_tickets(&c).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].summary, "from search");
}

#[test]
fn upsert_external_does_not_downgrade_assigned_ticket() {
    let c = fresh();
    upsert_ticket(
        &c,
        &JiraTicket {
            key: "MINE-1".into(),
            summary: "assigned to me".into(),
            status: Some("In Progress".into()),
            project_key: Some("MINE".into()),
            updated: Some("2026-04-17T10:00:00Z".into()),
            issue_id: None,
        },
    )
    .unwrap();
    assert_eq!(external_flag(&c, "MINE-1"), 0);
    upsert_external_ticket(
        &c,
        &JiraTicket {
            key: "MINE-1".into(),
            summary: "fresh summary".into(),
            status: Some("In Progress".into()),
            project_key: Some("MINE".into()),
            updated: Some("2026-04-18T11:00:00Z".into()),
            issue_id: None,
        },
    )
    .unwrap();
    // A subsequent external-search upsert MUST NOT flip external
    // back to 1 — that would hide an actually-assigned ticket from
    // the estimator.
    assert_eq!(external_flag(&c, "MINE-1"), 0);
}

#[test]
fn upsert_ticket_promotes_external_to_assigned() {
    let c = fresh();
    upsert_external_ticket(
        &c,
        &JiraTicket {
            key: "FLIP-1".into(),
            summary: "picked manually".into(),
            status: Some("To Do".into()),
            project_key: Some("FLIP".into()),
            updated: Some("2026-04-17T10:00:00Z".into()),
            issue_id: None,
        },
    )
    .unwrap();
    assert_eq!(external_flag(&c, "FLIP-1"), 1);
    upsert_ticket(
        &c,
        &JiraTicket {
            key: "FLIP-1".into(),
            summary: "picked manually".into(),
            status: Some("In Progress".into()),
            project_key: Some("FLIP".into()),
            updated: Some("2026-04-18T11:00:00Z".into()),
            issue_id: None,
        },
    )
    .unwrap();
    // assignee=currentUser() refresh now returns this key — promote
    // it (external→0) so the estimator can see it.
    assert_eq!(external_flag(&c, "FLIP-1"), 0);
}
