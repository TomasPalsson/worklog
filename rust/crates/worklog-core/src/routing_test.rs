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
    runner_up: f64,
    abstain: f64,
}

impl Classifier for FixedGuess {
    fn classify(&self, _state: &Value, _options: &[String]) -> Result<Option<Guess>> {
        Ok(Some(Guess {
            folder: self.folder.clone(),
            confidence: self.confidence,
            runner_up: self.runner_up,
            abstain: self.abstain,
        }))
    }
}

fn rule(abstain_margin: f64) -> RouteRule {
    RouteRule {
        abstain_margin,
        runner_up_ratio: 0.0,
    }
}

fn default_rule() -> RouteRule {
    RouteRule {
        abstain_margin: crate::routing_contract::DEFAULT_ABSTAIN_MARGIN,
        runner_up_ratio: crate::routing_contract::DEFAULT_RUNNER_UP_RATIO,
    }
}

/// A single pending event offering `options`, for testing `decide` in
/// isolation without a database.
fn pending_with_options(options: Vec<&str>) -> Pending {
    Pending {
        event: RoutedEvent {
            id: 1,
            source: SOURCE_FIREFOX.into(),
            started_at: "2026-04-20T09:00:00+00:00".into(),
            title: "t".into(),
            details: None,
            container: None,
            folder: None,
            label_origin: None,
            label_confidence: None,
        },
        options: options.into_iter().map(str::to_owned).collect(),
        state: serde_json::json!({}),
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
        runner_up: 0.0,
        abstain: 0.0,
    };
    let stats = route_day(&conn, day, &model, rule(0.9)).unwrap();
    assert_eq!(stats.rules_applied, 1);
    assert_eq!(stats.guesses_applied, 0);

    let routed = routed_for_day(&conn, day).unwrap();
    assert_eq!(routed.len(), 1);
    assert_eq!(routed[0].folder.as_deref(), Some("aws-cert"));
    assert_eq!(routed[0].label_origin, Some(LabelOrigin::Rule));
}

// B1-B4 (spec 004): a guess is filed only when the winner clears both the
// abstain margin and the runner-up ratio against the model's own scores,
// never a single absolute-confidence threshold. Scores are the exact ones
// measured in spec.md's FR-01/FR-02 examples.

#[test]
fn files_when_winner_beats_abstain_and_runner_up() {
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

    let model = FixedGuess {
        folder: "aws-cert".into(),
        confidence: 0.079,
        runner_up: 0.065,
        abstain: 0.067,
    };
    let stats = route_day(&conn, day, &model, default_rule()).unwrap();
    assert_eq!(stats.guesses_applied, 1);
    let routed = routed_for_day(&conn, day).unwrap();
    assert_eq!(routed[0].label_origin, Some(LabelOrigin::Guess));
    assert_eq!(routed[0].label_confidence, Some(0.079));
    assert_eq!(routed[0].folder.as_deref(), Some("aws-cert"));
}

// The pre-fix body was `confidence >= rule.abstain_margin` — it ignores
// `guess.abstain` entirely. Since abstain is always <= 1.0, that raw check
// is provably at least as strict as `confidence >= abstain * margin` for
// any margin (abstain * margin <= margin <= confidence whenever the raw
// check holds), so no valid scores can make the raw check accept while the
// new abstain-margin check rejects. This scenario is spec.md's own
// FR-01/edge-case example (winner beats runner-up ×1.29 but only ties
// abstain) verified by hand computation; it can't be red-first against the
// pre-fix code for the reason above, only `unsorted_when_runner_up_too_close`
// (below) can be, because the pre-fix code never reads `runner_up_ratio` at all.
#[test]
fn unsorted_when_winner_ties_abstain() {
    let items = vec![pending_with_options(vec!["aws-cert"])];
    let model = FixedGuess {
        folder: "aws-cert".into(),
        confidence: 0.070,
        runner_up: 0.054,
        abstain: 0.070,
    };
    let guesses = decide(&items, &model, default_rule());
    assert!(
        guesses.is_empty(),
        "winner must clear the abstain score by the margin, not just tie it"
    );
}

#[test]
fn unsorted_when_runner_up_too_close() {
    let items = vec![pending_with_options(vec!["aws-cert"])];
    // abstain_margin is deliberately far below the default (1.05) so the
    // pre-fix body (`confidence >= rule.abstain_margin`, blind to
    // `runner_up_ratio`) would accept this guess — proving this test
    // actually exercises the new runner-up check, not a vacuously-high
    // margin no real confidence could ever reach.
    let rule = RouteRule {
        abstain_margin: 0.05,
        runner_up_ratio: 1.10,
    };
    let model = FixedGuess {
        folder: "aws-cert".into(),
        confidence: 0.20,
        runner_up: 0.19,
        abstain: 0.01,
    };
    let guesses = decide(&items, &model, rule);
    assert!(
        guesses.is_empty(),
        "winner clears the abstain score but not the runner-up ratio"
    );
}

// The options-narrowing check is unchanged by T003 (`p.options.contains`
// gated the pre-fix body too), so both bodies always agree here — this
// can never be red-first against the pre-fix code. It's kept as a direct
// `decide()`-level regression alongside `decide_drops_guess_outside_narrowed_options`.
#[test]
fn unsorted_when_choice_not_an_option() {
    let items = vec![pending_with_options(vec!["aws-cert"])];
    let model = FixedGuess {
        folder: "not-a-project".into(),
        confidence: 0.99,
        runner_up: 0.01,
        abstain: 0.01,
    };
    let guesses = decide(&items, &model, default_rule());
    assert!(
        guesses.is_empty(),
        "a folder the helper names outside the options list must never be filed"
    );
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
                runner_up: 0.0,
                abstain: 0.0,
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
        runner_up: 0.0,
        abstain: 0.0,
    };
    let guesses = decide(&pending, &model, rule(0.9));
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
fn label_event_rejects_domain_rule_on_non_firefox_event() {
    let conn = open_memory().unwrap();
    pin(&conn, "somefolder", None);
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
    let eid = repo::upsert_event(
        &conn,
        &Event::minimal(SOURCE_SLACK, "e1", "2026-04-20T09:00:00+00:00", "chan"),
    )
    .unwrap();
    // Free-text Slack message content that happens to contain a URL must
    // not become a domain rule — that rule would then silently apply to
    // unrelated Firefox browsing history.
    set_details(&conn, eid, "check http://internal.tool/status please");

    let err = label_event(
        &conn,
        eid,
        &LabelRequest {
            folder: "somefolder".into(),
            always: Some(RuleKind::Domain),
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("firefox"), "err = {err}");

    let rules = list_rules(&conn).unwrap();
    assert!(
        rules.is_empty(),
        "a domain rule must not be created from a non-firefox event"
    );

    let routed = routed_for_day(&conn, day).unwrap();
    let ev = routed.iter().find(|r| r.id == eid).unwrap();
    assert_eq!(
        ev.folder, None,
        "label_event must not partially commit the fix label when the always-rule step fails"
    );
}

#[test]
fn label_event_rejects_slack_channel_rule_on_non_slack_event() {
    let conn = open_memory().unwrap();
    pin(&conn, "somefolder", None);
    let eid = repo::upsert_event(
        &conn,
        &Event::minimal(SOURCE_FIREFOX, "e1", "2026-04-20T09:00:00+00:00", "general"),
    )
    .unwrap();
    set_details(&conn, eid, "https://example.com");

    let err = label_event(
        &conn,
        eid,
        &LabelRequest {
            folder: "somefolder".into(),
            always: Some(RuleKind::SlackChannel),
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("slack"), "err = {err}");

    let rules = list_rules(&conn).unwrap();
    assert!(
        rules.is_empty(),
        "a slack_channel rule must not be created from a non-slack event"
    );
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

#[test]
fn editing_rule_folder_relabels_rule_labelled_events() {
    let conn = open_memory().unwrap();
    pin(&conn, "folder1", None);
    pin(&conn, "folder2", None);
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();

    let ids: Vec<i64> = ["a", "b", "c"]
        .iter()
        .enumerate()
        .map(|(i, sid)| {
            let at = format!("2026-04-20T09:0{i}:00+00:00");
            let id =
                repo::upsert_event(&conn, &Event::minimal(SOURCE_FIREFOX, *sid, &at, "X")).unwrap();
            set_details(&conn, id, "https://x.example.com/p");
            id
        })
        .collect();

    let always = |id: i64, folder: &str| {
        label_event(
            &conn,
            id,
            &LabelRequest {
                folder: folder.into(),
                always: Some(RuleKind::Domain),
            },
        )
        .unwrap();
    };
    always(ids[0], "folder1");
    let routed = routed_for_day(&conn, day).unwrap();
    let b = routed.iter().find(|r| r.id == ids[1]).unwrap();
    assert_eq!(b.label_origin, Some(LabelOrigin::Rule));

    always(ids[2], "folder2");
    let rules = list_rules(&conn).unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].folder, "folder2");

    let routed = routed_for_day(&conn, day).unwrap();
    let b = routed.iter().find(|r| r.id == ids[1]).unwrap();
    assert_eq!(b.folder.as_deref(), Some("folder2"));
    assert_eq!(b.label_origin, Some(LabelOrigin::Rule));
    let a = routed.iter().find(|r| r.id == ids[0]).unwrap();
    assert_eq!(a.folder.as_deref(), Some("folder1"), "hand fixes stay");
}
