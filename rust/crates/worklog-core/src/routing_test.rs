use super::*;
use crate::billing_registry::{upsert_customer, upsert_folder, Customer, FolderMap};
use crate::db::open_memory;
use crate::models::Event;
use crate::repo;

fn pin(conn: &Connection, folder: &str, customer: Option<&str>) {
    upsert_folder(
        conn,
        &FolderMap {
            id: None,
            folder: folder.into(),
            customer: customer.map(str::to_owned),
            verkefni: None,
            billable: true,
        },
    )
    .unwrap();
}

fn set_details(conn: &Connection, id: i64, details: &str) {
    conn.execute(
        "UPDATE events SET details = ?1 WHERE id = ?2",
        params![details, id],
    )
    .unwrap();
}

fn set_container(conn: &Connection, id: i64, container: &str) {
    conn.execute(
        "UPDATE events SET container = ?1 WHERE id = ?2",
        params![container, id],
    )
    .unwrap();
}

fn fix(conn: &Connection, id: i64, folder: &str) -> RoutedEvent {
    label_event(
        conn,
        id,
        &LabelRequest {
            folder: folder.into(),
            always: None,
        },
    )
    .unwrap()
}

struct FixedGuess {
    folder: String,
    confidence: f64,
}

impl Classifier for FixedGuess {
    fn classify(&self, _state: &Value, _options: &[String]) -> Result<Option<Guess>> {
        Ok(Some(Guess {
            folder: self.folder.clone(),
            confidence: self.confidence,
        }))
    }
}

#[test]
fn rule_beats_model() {
    let conn = open_memory().unwrap();
    pin(&conn, "aws-cert", None);
    conn.execute(
        "INSERT INTO routing_rules (kind, pattern, folder) VALUES ('domain', 'aws.tomasari.is', 'aws-cert')",
        [],
    )
    .unwrap();

    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
    let eid = repo::upsert_event(
        &conn,
        &Event::minimal(
            SOURCE_FIREFOX,
            "e1",
            "2026-04-20T09:00:00+00:00",
            "AWS Console",
        ),
    )
    .unwrap();
    set_details(&conn, eid, "https://aws.tomasari.is/console");

    // Model would say "other-folder" with high confidence — the rule must
    // still win because a rule-matched event never reaches the classifier.
    let model = FixedGuess {
        folder: "other-folder".into(),
        confidence: 0.99,
    };
    let stats = route_day(&conn, day, &model, 0.9).unwrap();
    assert_eq!(stats.rules_applied, 1);
    assert_eq!(stats.guesses_applied, 0);

    let routed = routed_for_day(&conn, day).unwrap();
    assert_eq!(routed.len(), 1);
    assert_eq!(routed[0].folder.as_deref(), Some("aws-cert"));
    assert_eq!(routed[0].label_origin, Some(LabelOrigin::Rule));
}

#[test]
fn threshold_applies() {
    let conn = open_memory().unwrap();
    pin(&conn, "aws-cert", None);
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
    repo::upsert_event(
        &conn,
        &Event::minimal(
            SOURCE_FIREFOX,
            "e1",
            "2026-04-20T09:00:00+00:00",
            "New site",
        ),
    )
    .unwrap();

    let above = FixedGuess {
        folder: "aws-cert".into(),
        confidence: 0.95,
    };
    let stats = route_day(&conn, day, &above, 0.9).unwrap();
    assert_eq!(stats.guesses_applied, 1);
    let routed = routed_for_day(&conn, day).unwrap();
    assert_eq!(routed[0].label_origin, Some(LabelOrigin::Guess));
    assert_eq!(routed[0].label_confidence, Some(0.95));
    assert_eq!(routed[0].folder.as_deref(), Some("aws-cert"));

    let conn2 = open_memory().unwrap();
    pin(&conn2, "aws-cert", None);
    repo::upsert_event(
        &conn2,
        &Event::minimal(
            SOURCE_FIREFOX,
            "e2",
            "2026-04-20T09:00:00+00:00",
            "New site",
        ),
    )
    .unwrap();
    let below = FixedGuess {
        folder: "aws-cert".into(),
        confidence: 0.5,
    };
    let stats2 = route_day(&conn2, day, &below, 0.9).unwrap();
    assert_eq!(stats2.guesses_applied, 0);
    let routed2 = routed_for_day(&conn2, day).unwrap();
    assert_eq!(routed2[0].folder, None);
    assert_eq!(routed2[0].label_origin, None);
}

#[test]
fn unsorted_excluded_labelled_joins_block() {
    let conn = open_memory().unwrap();
    pin(&conn, "aws-cert", None);
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();

    let mut ids = Vec::new();
    for i in 0..5 {
        let ts = format!("2026-04-20T09:0{i}:00+00:00");
        let id = repo::upsert_event(
            &conn,
            &Event::minimal(SOURCE_FIREFOX, format!("hb{i}"), &ts, "AWS console"),
        )
        .unwrap();
        ids.push(id);
    }

    // Unsorted: excluded from inference entirely.
    let events = crate::infer::load_day_events(&conn, day).unwrap();
    assert!(
        events.is_empty(),
        "unsorted events must not reach inference"
    );

    for id in &ids {
        fix(&conn, *id, "aws-cert");
    }

    let events = crate::infer::load_day_events(&conn, day).unwrap();
    assert_eq!(events.len(), 5, "labelled events must reach inference");
    let blocks = crate::infer::build_blocks(events);
    assert_eq!(blocks.len(), 1);
    assert!(blocks[0].duration_seconds >= 300);
}

#[test]
fn always_creates_rule_and_applies() {
    let conn = open_memory().unwrap();
    pin(&conn, "lighthouse", None);
    pin(&conn, "other", None);
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();

    let e1 = repo::upsert_event(
        &conn,
        &Event::minimal(SOURCE_FIREFOX, "e1", "2026-04-20T09:00:00+00:00", "GH"),
    )
    .unwrap();
    set_details(&conn, e1, "https://github.com/x/y");

    let e2 = repo::upsert_event(
        &conn,
        &Event::minimal(SOURCE_FIREFOX, "e2", "2026-04-20T09:05:00+00:00", "GH2"),
    )
    .unwrap();
    set_details(&conn, e2, "https://github.com/x/z");

    // Earlier hand-fix — must not be touched by the new "always" rule.
    let e3 = repo::upsert_event(
        &conn,
        &Event::minimal(SOURCE_FIREFOX, "e3", "2026-04-20T09:10:00+00:00", "GH3"),
    )
    .unwrap();
    set_details(&conn, e3, "https://github.com/x/w");
    fix(&conn, e3, "other");

    let updated = label_event(
        &conn,
        e1,
        &LabelRequest {
            folder: "lighthouse".into(),
            always: Some(RuleKind::Domain),
        },
    )
    .unwrap();
    assert_eq!(updated.label_origin, Some(LabelOrigin::Fix));

    let rules = list_rules(&conn).unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].pattern, "github.com");
    assert_eq!(rules[0].folder, "lighthouse");

    let routed = routed_for_day(&conn, day).unwrap();
    let ev2 = routed.iter().find(|r| r.id == e2).unwrap();
    assert_eq!(ev2.folder.as_deref(), Some("lighthouse"));
    assert_eq!(ev2.label_origin, Some(LabelOrigin::Rule));

    let ev3 = routed.iter().find(|r| r.id == e3).unwrap();
    assert_eq!(
        ev3.folder.as_deref(),
        Some("other"),
        "earlier fix labels stay"
    );
    assert_eq!(ev3.label_origin, Some(LabelOrigin::Fix));
}

#[test]
fn always_rule_relabels_previously_guessed_events() {
    let conn = open_memory().unwrap();
    pin(&conn, "lighthouse", None);
    pin(&conn, "other", None);
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();

    let e1 = repo::upsert_event(
        &conn,
        &Event::minimal(SOURCE_FIREFOX, "e1", "2026-04-20T09:00:00+00:00", "GH"),
    )
    .unwrap();
    set_details(&conn, e1, "https://github.com/x/y");

    let e2 = repo::upsert_event(
        &conn,
        &Event::minimal(SOURCE_FIREFOX, "e2", "2026-04-20T09:05:00+00:00", "GH2"),
    )
    .unwrap();
    set_details(&conn, e2, "https://github.com/x/z");

    // e2 already carries a model guess — the new "always" rule must still
    // retroactively correct it (B9), not just events that are still NULL.
    commit_labels(
        &conn,
        &[],
        &[(
            e2,
            Guess {
                folder: "other".into(),
                confidence: 0.95,
            },
        )],
    )
    .unwrap();

    label_event(
        &conn,
        e1,
        &LabelRequest {
            folder: "lighthouse".into(),
            always: Some(RuleKind::Domain),
        },
    )
    .unwrap();

    let routed = routed_for_day(&conn, day).unwrap();
    let ev2 = routed.iter().find(|r| r.id == e2).unwrap();
    assert_eq!(ev2.folder.as_deref(), Some("lighthouse"));
    assert_eq!(ev2.label_origin, Some(LabelOrigin::Rule));
}

#[test]
fn decide_drops_guess_outside_narrowed_options() {
    let event = RoutedEvent {
        id: 1,
        source: SOURCE_FIREFOX.into(),
        started_at: "2026-04-20T09:00:00+00:00".into(),
        title: "portal".into(),
        details: None,
        container: Some("Sjúkra".into()),
        folder: None,
        label_origin: None,
        label_confidence: None,
    };
    let pending = vec![Pending {
        event,
        options: vec!["sjukra-portal".into()],
        state: serde_json::json!({}),
    }];
    // Confidence clears the threshold, but the folder isn't one of this
    // event's narrowed options — B10 requires the guess be dropped, not
    // just that `options` itself was narrowed.
    let model = FixedGuess {
        folder: "mms-portal".into(),
        confidence: 0.99,
    };
    let guesses = decide(&pending, &model, 0.9);
    assert!(
        guesses.is_empty(),
        "a guess naming a folder outside the narrowed options must be dropped"
    );
}

#[test]
fn label_event_does_not_partial_commit_when_always_rule_fails() {
    let conn = open_memory().unwrap();
    pin(&conn, "somefolder", None);
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
    let eid = repo::upsert_event(
        &conn,
        &Event::minimal(SOURCE_SLACK, "e1", "2026-04-20T09:00:00+00:00", "chan"),
    )
    .unwrap();
    // No container set — a Container-kind rule can't be derived from this
    // event, so rule_pattern() must fail before anything is written.

    let err = label_event(
        &conn,
        eid,
        &LabelRequest {
            folder: "somefolder".into(),
            always: Some(RuleKind::Container),
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("container"));

    let routed = routed_for_day(&conn, day).unwrap();
    let ev = routed.iter().find(|r| r.id == eid).unwrap();
    assert_eq!(
        ev.folder, None,
        "label_event must not partially commit the fix label when the always-rule step fails"
    );
    assert_eq!(ev.label_origin, None);
}

#[test]
fn container_narrows_options() {
    let conn = open_memory().unwrap();
    upsert_customer(
        &conn,
        &Customer {
            id: None,
            name: "Sjúkra".into(),
            aliases: vec![],
        },
    )
    .unwrap();
    upsert_customer(
        &conn,
        &Customer {
            id: None,
            name: "MMS".into(),
            aliases: vec![],
        },
    )
    .unwrap();
    pin(&conn, "sjukra-portal", Some("Sjúkra"));
    pin(&conn, "mms-portal", Some("MMS"));

    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
    let eid = repo::upsert_event(
        &conn,
        &Event::minimal(SOURCE_FIREFOX, "e1", "2026-04-20T09:00:00+00:00", "portal"),
    )
    .unwrap();
    set_container(&conn, eid, "Sjúkra");

    let (rule_hits, pending) = load_pending(&conn, day).unwrap();
    assert!(rule_hits.is_empty());
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].options, vec!["sjukra-portal".to_string()]);
}

#[test]
fn project_keys_include_billing_folder_pins() {
    let conn = open_memory().unwrap();
    pin(&conn, "sjukra", None);
    let keys = project_keys(&conn).unwrap();
    assert!(keys.contains(&"sjukra".to_string()));
}

#[test]
fn label_event_rejects_an_unknown_folder() {
    let conn = open_memory().unwrap();
    let eid = repo::upsert_event(
        &conn,
        &Event::minimal(SOURCE_FIREFOX, "e1", "2026-04-20T09:00:00+00:00", "x"),
    )
    .unwrap();
    let err = label_event(
        &conn,
        eid,
        &LabelRequest {
            folder: "ghost-project".into(),
            always: None,
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("no longer exists"));
}
