use super::*;
use crate::billing_registry::{upsert_customer, upsert_folder, Customer, FolderMap};
use crate::db::open_memory;
use crate::deild_contract::ChangeField;
use crate::models::Event;
use crate::repo;
use crate::routing_contract::Guess;
use crate::verdict_contract::{DecisionKind, DecisionSource, RankedOption, Ranking};
use crate::verdict_decisions;

fn pin(conn: &Connection, folder: &str, customer: Option<&str>) {
    upsert_folder(
        conn,
        &FolderMap {
            id: None,
            folder: folder.into(),
            customer: customer.map(str::to_owned),
            verkefni: None,
            billable: true,
            multi_tenant: false,
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

struct PanicsIfCalled;

impl Classifier for PanicsIfCalled {
    fn classify(
        &self,
        _state: &Value,
        _options: &[String],
        _examples: &BTreeMap<String, Vec<String>>,
    ) -> Result<Option<Ranking>> {
        panic!("classifier must not be called for an event named by rule (FR-10)");
    }
}

struct FixedGuess {
    folder: String,
    confidence: f64,
    runner_up: f64,
    abstain: f64,
}

impl Classifier for FixedGuess {
    fn classify(
        &self,
        _state: &Value,
        _options: &[String],
        _examples: &BTreeMap<String, Vec<String>>,
    ) -> Result<Option<Ranking>> {
        Ok(Some(self.ranking(true)))
    }
}

impl FixedGuess {
    fn ranking(&self, agreed: bool) -> Ranking {
        let opt = |id: &str, probability| RankedOption {
            id: id.into(),
            probability,
        };
        Ranking {
            ranking: vec![
                opt(&self.folder, self.confidence),
                opt("runner-up", self.runner_up),
            ],
            abstain: self.abstain,
            agreed,
        }
    }
}

/// Same scores as the wrapped guess, but the reversed-order pass disagreed.
struct Disagrees(FixedGuess);

impl Classifier for Disagrees {
    fn classify(
        &self,
        _state: &Value,
        _options: &[String],
        _examples: &BTreeMap<String, Vec<String>>,
    ) -> Result<Option<Ranking>> {
        Ok(Some(self.0.ranking(false)))
    }
}

/// A fixed answer, whatever its shape.
struct Fixed(Ranking);

impl Classifier for Fixed {
    fn classify(
        &self,
        _state: &Value,
        _options: &[String],
        _examples: &BTreeMap<String, Vec<String>>,
    ) -> Result<Option<Ranking>> {
        Ok(Some(self.0.clone()))
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
            ranking: None,
        },
        options: options.into_iter().map(str::to_owned).collect(),
        state: serde_json::json!({}),
        examples: BTreeMap::new(),
    }
}

/// The accepted guesses of a `decide` pass.
fn filed_by(
    pending: &[Pending],
    classifier: &dyn Classifier,
    rule: RouteRule,
) -> Vec<(i64, Guess)> {
    decide(pending, classifier, rule)
        .into_iter()
        .filter_map(|a| a.guess.map(|g| (a.id, g)))
        .collect()
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

    let routed = routed_for_day(&conn, day, false).unwrap();
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
            "aws-cert New site",
        ),
    )
    .unwrap();

    let model = FixedGuess {
        folder: "aws-cert".into(),
        confidence: 0.079,
        runner_up: 0.065,
        abstain: 0.060,
    };
    let stats = route_day(&conn, day, &model, default_rule()).unwrap();
    assert_eq!(stats.guesses_applied, 1);
    let routed = routed_for_day(&conn, day, false).unwrap();
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
    let guesses = filed_by(&items, &model, default_rule());
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
    let guesses = filed_by(&items, &model, rule);
    assert!(
        guesses.is_empty(),
        "winner clears the abstain score but not the runner-up ratio"
    );
}

// FR-10 (spec 004 amendment 2026-09-23): an event whose title or details
// name exactly one project by an exact `github.com/<org>/<key>` or
// `Desktop/Work/<key>` mention is filed with origin Link, never sent to the model —
// the owner never created a rule, so the badge must not call it one (2026-09-23 amendment).

#[test]
fn exact_repo_mention_files_by_link() {
    let conn = open_memory().unwrap();
    pin(&conn, "vitinn-infra", None);
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
    let eid = repo::upsert_event(
        &conn,
        &Event::minimal(SOURCE_SLACK, "e1", "2026-04-20T09:00:00+00:00", "deploy"),
    )
    .unwrap();
    set_details(
        &conn,
        eid,
        "PR link → https://github.com/Heman10x/vitinn-infra/pull/42",
    );

    let stats = route_day(&conn, day, &PanicsIfCalled, default_rule()).unwrap();
    assert_eq!(stats.rules_applied, 1);
    assert_eq!(stats.guesses_applied, 0);

    let routed = routed_for_day(&conn, day, false).unwrap();
    assert_eq!(routed[0].folder.as_deref(), Some("vitinn-infra"));
    // Named-project hits are `Link`, not `Rule` — the owner never created a
    // rule; the event's own text named the repo (spec: browser/Slack event
    // routing, item B).
    assert_eq!(routed[0].label_origin, Some(LabelOrigin::Link));
}

#[test]
fn path_mention_files_by_link() {
    let conn = open_memory().unwrap();
    pin(&conn, "vitinn-infra", None);
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
    let eid = repo::upsert_event(
        &conn,
        &Event::minimal(
            SOURCE_FIREFOX,
            "e1",
            "2026-04-20T09:00:00+00:00",
            "terminal",
        ),
    )
    .unwrap();
    set_details(&conn, eid, "cd ~/Desktop/Work/vitinn-infra");

    let stats = route_day(&conn, day, &PanicsIfCalled, default_rule()).unwrap();
    assert_eq!(stats.rules_applied, 1);
    assert_eq!(stats.guesses_applied, 0);

    let routed = routed_for_day(&conn, day, false).unwrap();
    assert_eq!(routed[0].folder.as_deref(), Some("vitinn-infra"));
    assert_eq!(routed[0].label_origin, Some(LabelOrigin::Link));
}

#[test]
fn two_named_projects_is_no_match() {
    let conn = open_memory().unwrap();
    pin(&conn, "vitinn-infra", None);
    pin(&conn, "other-repo", None);
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
    let eid = repo::upsert_event(
        &conn,
        &Event::minimal(SOURCE_SLACK, "e1", "2026-04-20T09:00:00+00:00", "deploy"),
    )
    .unwrap();
    set_details(
        &conn,
        eid,
        "https://github.com/org/vitinn-infra and https://github.com/org/other-repo",
    );

    let (rule_hits, pending) = load_pending(&conn, day).unwrap();
    assert!(
        rule_hits.is_empty(),
        "two distinct named projects must not be filed by rule"
    );
    assert_eq!(pending.len(), 1);
}

#[test]
fn prefix_is_not_a_match() {
    let conn = open_memory().unwrap();
    pin(&conn, "vitinn-infra", None);
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
    let eid = repo::upsert_event(
        &conn,
        &Event::minimal(SOURCE_SLACK, "e1", "2026-04-20T09:00:00+00:00", "deploy"),
    )
    .unwrap();
    set_details(&conn, eid, "https://github.com/org/vitinn-infra-old/pull/1");

    let (rule_hits, pending) = load_pending(&conn, day).unwrap();
    assert!(
        rule_hits.is_empty(),
        "vitinn-infra-old must not match the vitinn-infra option"
    );
    assert_eq!(pending.len(), 1);
}

#[test]
fn unknown_repo_is_no_match() {
    let conn = open_memory().unwrap();
    pin(&conn, "vitinn-infra", None);
    recent_work(&conn, "vitinn-infra");
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
    let eid = repo::upsert_event(
        &conn,
        &Event::minimal(SOURCE_SLACK, "e1", "2026-04-20T09:00:00+00:00", "deploy"),
    )
    .unwrap();
    set_details(&conn, eid, "https://github.com/org/some-unknown-repo");

    let (rule_hits, pending) = load_pending(&conn, day).unwrap();
    assert!(
        rule_hits.is_empty(),
        "a mentioned repo that isn't a known project key must not be filed by rule"
    );
    assert_eq!(pending.len(), 1);
}

// Security fix: a firefox event's `title` is the visited page's <title>,
// which any website controls — a page must not be able to get itself filed
// under a real project just by naming it in its own title. Only `details`
// (the URL the user actually visited) may be scanned.
struct AlwaysNone;

impl Classifier for AlwaysNone {
    fn classify(
        &self,
        _state: &Value,
        _options: &[String],
        _examples: &BTreeMap<String, Vec<String>>,
    ) -> Result<Option<Ranking>> {
        Ok(None)
    }
}

#[test]
fn title_only_mention_is_not_a_match() {
    let conn = open_memory().unwrap();
    pin(&conn, "vitinn-infra", None);
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
    let eid = repo::upsert_event(
        &conn,
        &Event::minimal(
            SOURCE_FIREFOX,
            "e1",
            "2026-04-20T09:00:00+00:00",
            "github.com/aproorg/vitinn-infra/pull/1",
        ),
    )
    .unwrap();
    set_details(&conn, eid, "https://example.com/");

    let stats = route_day(&conn, day, &AlwaysNone, default_rule()).unwrap();
    assert_eq!(
        stats.rules_applied, 0,
        "a project mention in the page-controlled title, absent from details, must not be filed by rule"
    );

    let routed = routed_for_day(&conn, day, false).unwrap();
    assert_ne!(
        routed[0].label_origin,
        Some(LabelOrigin::Rule),
        "title-only mention must not be labelled by rule"
    );
}

// The options-narrowing check is unchanged by T003 (`p.options.contains`
// gated the pre-fix body too), so both bodies always agree here — this
// can never be red-first against the pre-fix code. It's kept as a direct
// `decide()`-level regression alongside `decide_drops_guess_outside_narrowed_options`.
fn ranked(top: f64, second: Option<f64>, abstain: f64, agreed: bool) -> Ranking {
    let mut ranking = vec![RankedOption {
        id: "aws-cert".into(),
        probability: top,
    }];
    ranking.extend(second.map(|probability| RankedOption {
        id: "other".into(),
        probability,
    }));
    Ranking {
        ranking,
        abstain,
        agreed,
    }
}

fn filed(ranking: Ranking, rule: RouteRule) -> Vec<(i64, Guess)> {
    filed_by(
        &[pending_with_options(vec!["aws-cert"])],
        &Fixed(ranking),
        rule,
    )
}

fn rule_of(abstain_margin: f64, runner_up_ratio: f64) -> RouteRule {
    RouteRule {
        abstain_margin,
        runner_up_ratio,
    }
}

// Order check (spec 017 FR-07): scores that would file are still dropped when
// the reversed-order pass disagreed. Catches a filter that ignores `agreed`.
#[test]
fn unsorted_when_order_check_disagrees() {
    let items = vec![pending_with_options(vec!["aws-cert"])];
    let model = Disagrees(FixedGuess {
        folder: "aws-cert".into(),
        confidence: 0.9,
        runner_up: 0.1,
        abstain: 0.05,
    });
    assert!(filed_by(&items, &model, default_rule()).is_empty());
}

// Exactly at the abstain margin files (0.5 >= 0.25 x 2.0); catches `>`.
#[test]
fn files_exactly_at_abstain_margin() {
    let got = filed(ranked(0.5, Some(0.1), 0.25, true), rule_of(2.0, 1.0));
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].1.folder, "aws-cert");
    assert_eq!(got[0].1.confidence, 0.5);
}

// Just under the abstain margin drops; catches a margin that is not applied.
#[test]
fn unsorted_just_under_abstain_margin() {
    assert!(filed(ranked(0.49, Some(0.1), 0.25, true), rule_of(2.0, 1.0)).is_empty());
}

// Exactly at the runner-up ratio files (0.5 >= 0.25 x 2.0); catches `>`.
#[test]
fn files_exactly_at_runner_up_ratio() {
    assert_eq!(
        filed(ranked(0.5, Some(0.25), 0.1, true), rule_of(1.0, 2.0)).len(),
        1
    );
}

// Just under the runner-up ratio drops; catches a ratio taken against abstain only.
#[test]
fn unsorted_just_under_runner_up_ratio() {
    assert!(filed(ranked(0.49, Some(0.25), 0.1, true), rule_of(1.0, 2.0)).is_empty());
}

// A one-entry ranking has no runner-up to beat; catches indexing [1] or
// treating a missing second as a failure.
#[test]
fn files_when_ranking_has_no_second() {
    assert_eq!(
        filed(ranked(0.5, None, 0.1, true), rule_of(1.0, 2.0)).len(),
        1
    );
}

// An empty ranking has no top; catches indexing [0] (panic) or filing "".
#[test]
fn unsorted_when_ranking_is_empty() {
    let empty = Ranking {
        ranking: vec![],
        abstain: 0.0,
        agreed: true,
    };
    assert!(filed(empty, default_rule()).is_empty());
}

// The top must be one of the event's own options even when the second
// entry is; catches checking any ranked id instead of the top.
#[test]
fn unsorted_when_only_runner_up_is_an_option() {
    let r = Ranking {
        ranking: vec![
            RankedOption {
                id: "not-an-option".into(),
                probability: 0.9,
            },
            RankedOption {
                id: "aws-cert".into(),
                probability: 0.1,
            },
        ],
        abstain: 0.01,
        agreed: true,
    };
    assert!(filed(r, default_rule()).is_empty());
}

#[test]
fn unsorted_when_choice_not_an_option() {
    let items = vec![pending_with_options(vec!["aws-cert"])];
    let model = FixedGuess {
        folder: "not-a-project".into(),
        confidence: 0.99,
        runner_up: 0.01,
        abstain: 0.01,
    };
    let guesses = filed_by(&items, &model, default_rule());
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

    let routed = routed_for_day(&conn, day, false).unwrap();
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
        &[Answer {
            id: e2,
            state: serde_json::json!({}),
            options: vec!["other".into()],
            ranking: ranking_for("other", 0.95, 0.0),
            guess: Some(Guess {
                folder: "other".into(),
                confidence: 0.95,
                runner_up: 0.0,
                abstain: 0.0,
            }),
        }],
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

    let routed = routed_for_day(&conn, day, false).unwrap();
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
        ranking: None,
    };
    let pending = vec![Pending {
        event,
        options: vec!["sjukra-portal".into()],
        state: serde_json::json!({}),
        examples: BTreeMap::new(),
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
    let guesses = filed_by(&pending, &model, rule(0.9));
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

    let routed = routed_for_day(&conn, day, false).unwrap();
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

    let routed = routed_for_day(&conn, day, false).unwrap();
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

// Time-context step (2026-09-23 amendment): a Slack event with no rule/link
// hit gets filed under the project the owner's claude/shell/git_reflog
// activity dominantly named in the ±10 minute window around it.

fn claude_at(conn: &Connection, id: &str, ts: &str, project_path: &str) {
    let mut ev = Event::minimal("claude", id, ts, "x");
    ev.project_path = Some(project_path.into());
    repo::upsert_event(conn, &ev).unwrap();
}

#[test]
fn slack_context_files_from_dominant_recent_activity() {
    let conn = open_memory().unwrap();
    pin(&conn, "sjukra", None);
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
    let home = dirs::home_dir().unwrap().to_string_lossy().into_owned();
    let path = format!("{home}/Desktop/Work/sjukra");
    claude_at(&conn, "c1", "2026-04-20T08:55:00+00:00", &path);
    claude_at(&conn, "c2", "2026-04-20T08:57:00+00:00", &path);
    claude_at(&conn, "c3", "2026-04-20T09:02:00+00:00", &path);

    let eid = repo::upsert_event(
        &conn,
        &Event::minimal(SOURCE_SLACK, "e1", "2026-04-20T09:00:00+00:00", "chat"),
    )
    .unwrap();
    set_details(&conn, eid, "status update, nothing project-specific");

    let stats = route_day(&conn, day, &PanicsIfCalled, default_rule()).unwrap();
    assert_eq!(
        stats.rules_applied, 1,
        "the context step must not reach the classifier"
    );

    let routed = routed_for_day(&conn, day, false).unwrap();
    let ev = routed.iter().find(|r| r.id == eid).unwrap();
    assert_eq!(ev.folder.as_deref(), Some("sjukra"));
    assert_eq!(ev.label_origin, Some(LabelOrigin::Context));
    assert_eq!(ev.label_confidence, None);
}

#[test]
fn firefox_events_never_get_context_origin() {
    let conn = open_memory().unwrap();
    pin(&conn, "sjukra", None);
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
    let home = dirs::home_dir().unwrap().to_string_lossy().into_owned();
    let path = format!("{home}/Desktop/Work/sjukra");
    claude_at(&conn, "c1", "2026-04-20T08:55:00+00:00", &path);
    claude_at(&conn, "c2", "2026-04-20T08:57:00+00:00", &path);

    let eid = repo::upsert_event(
        &conn,
        &Event::minimal(
            SOURCE_FIREFOX,
            "e1",
            "2026-04-20T09:00:00+00:00",
            "some page",
        ),
    )
    .unwrap();
    set_details(&conn, eid, "https://example.com/");

    let (rule_hits, pending) = load_pending(&conn, day).unwrap();
    assert!(
        rule_hits.is_empty(),
        "a firefox event must never be filed by the time-context step"
    );
    assert_eq!(pending.len(), 1);
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
    let routed = routed_for_day(&conn, day, false).unwrap();
    let b = routed.iter().find(|r| r.id == ids[1]).unwrap();
    assert_eq!(b.label_origin, Some(LabelOrigin::Rule));

    always(ids[2], "folder2");
    let rules = list_rules(&conn).unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].folder, "folder2");

    let routed = routed_for_day(&conn, day, false).unwrap();
    let b = routed.iter().find(|r| r.id == ids[1]).unwrap();
    assert_eq!(b.folder.as_deref(), Some("folder2"));
    assert_eq!(b.label_origin, Some(LabelOrigin::Rule));
    let a = routed.iter().find(|r| r.id == ids[0]).unwrap();
    assert_eq!(a.folder.as_deref(), Some("folder1"), "hand fixes stay");
}

#[test]
fn ignore_rule_dismisses_matching_event_on_route_day() {
    let conn = open_memory().unwrap();
    conn.execute(
        "INSERT INTO routing_rules (kind, pattern, folder) VALUES ('slack_channel', '#random', '__ignore__')",
        [],
    )
    .unwrap();
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
    let id = repo::upsert_event(
        &conn,
        &Event::minimal(SOURCE_SLACK, "s1", "2026-04-20T09:00:00+00:00", "#random"),
    )
    .unwrap();

    let model = PanicsIfCalled;
    let stats = route_day(&conn, day, &model, default_rule()).unwrap();
    assert_eq!(stats.rules_applied, 1);

    let row = fetch_event(&conn, id).unwrap().unwrap();
    assert_eq!(row.label_origin.as_deref(), Some("dismissed"));
    assert!(row.project_path.is_none());
    assert!(row.label_confidence.is_none());
}

#[test]
fn routed_for_day_excludes_dismissed_events() {
    let conn = open_memory().unwrap();
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
    let id = repo::upsert_event(
        &conn,
        &Event::minimal(SOURCE_SLACK, "s1", "2026-04-20T09:00:00+00:00", "#random"),
    )
    .unwrap();
    crate::routing_dismiss::dismiss_event(&conn, id, None).unwrap();

    let routed = routed_for_day(&conn, day, false).unwrap();
    assert!(
        routed.is_empty(),
        "a dismissed event must not appear in the routed list"
    );
}

#[test]
fn routed_for_day_include_hidden_returns_dismissed_and_noise() {
    let conn = open_memory().unwrap();
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
    let dismissed_id = repo::upsert_event(
        &conn,
        &Event::minimal(SOURCE_SLACK, "s1", "2026-04-20T09:00:00+00:00", "#random"),
    )
    .unwrap();
    crate::routing_dismiss::dismiss_event(&conn, dismissed_id, None).unwrap();
    let noise_id = repo::upsert_event(
        &conn,
        &Event::minimal(SOURCE_FIREFOX, "f1", "2026-04-20T09:05:00+00:00", "news"),
    )
    .unwrap();
    conn.execute(
        "UPDATE events SET label_origin = 'noise' WHERE id = ?1",
        params![noise_id],
    )
    .unwrap();

    assert!(
        routed_for_day(&conn, day, false).unwrap().is_empty(),
        "default list must exclude both dismissed and noise"
    );

    let hidden = routed_for_day(&conn, day, true).unwrap();
    let ids: Vec<i64> = hidden.iter().map(|e| e.id).collect();
    assert!(ids.contains(&dismissed_id));
    assert!(ids.contains(&noise_id));
}

/// B9: a hard rule resolving a block's only event moves the block's
/// customer from unresolved to the folder's pin — logged as one Customer
/// change with source Verdict.
#[test]
fn route_day_moving_a_block_customer_is_logged_as_verdict() {
    let conn = open_memory().unwrap();
    pin(&conn, "aws-cert", Some("APRÓ"));
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

    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
         VALUES ('2026-04-20', '2026-04-20T09:00:00+00:00', '2026-04-20T09:30:00+00:00', 1800)",
        [],
    )
    .unwrap();
    let block_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
        params![block_id, eid],
    )
    .unwrap();

    // Seed the pre-route snapshot: the event is still unlabelled, so the
    // block's customer is unresolved.
    change_log::refresh_day(&conn, "2026-04-20", ChangeSource::Rebuild, "seed").unwrap();

    let stats = route_day(&conn, day, &PanicsIfCalled, default_rule()).unwrap();
    assert_eq!(stats.rules_applied, 1);

    let changes = change_log::feed(&conn, 0).unwrap().changes;
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].field, ChangeField::Customer);
    assert_eq!(changes[0].source, ChangeSource::Verdict);
    assert_eq!(changes[0].new.as_deref(), Some("APRÓ 100%"));
}

fn loose_event(conn: &Connection, sid: &str, title: &str) -> i64 {
    repo::upsert_event(
        conn,
        &Event::minimal(SOURCE_SLACK, sid, "2026-04-20T09:00:00+00:00", title),
    )
    .unwrap()
}

fn apr20() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 4, 20).unwrap()
}

#[test]
fn load_pending_offers_the_shortlist_not_every_project() {
    let conn = open_memory().unwrap();
    for i in 0..30 {
        pin(&conn, &format!("zq-x{i:02}"), None);
    }
    loose_event(&conn, "e1", "zq-x07 deploy");
    let (_, pending) = load_pending(&conn, apr20()).unwrap();
    // Catches options = every project key (30).
    assert_eq!(pending[0].options, vec!["zq-x07".to_string()]);
}

#[test]
fn load_pending_attaches_past_fixes_as_examples() {
    let conn = open_memory().unwrap();
    pin(&conn, "zq-a", None);
    let old = loose_event(&conn, "old", "Standup notes");
    fix(&conn, old, "zq-a");
    loose_event(&conn, "new", "zq-a again");
    let (_, pending) = load_pending(&conn, apr20()).unwrap();
    // Catches an empty examples map.
    assert_eq!(
        pending[0].examples["zq-a"],
        vec!["Standup notes".to_string()]
    );
}

struct Records(std::cell::RefCell<Vec<BTreeMap<String, Vec<String>>>>);

impl Classifier for Records {
    fn classify(
        &self,
        _state: &Value,
        _options: &[String],
        examples: &BTreeMap<String, Vec<String>>,
    ) -> Result<Option<Ranking>> {
        self.0.borrow_mut().push(examples.clone());
        Ok(None)
    }
}

#[test]
fn decide_hands_the_events_examples_to_the_classifier() {
    let mut p = pending_with_options(vec!["aws-cert"]);
    p.examples
        .insert("aws-cert".into(), vec!["a past fix".into()]);
    let model = Records(Default::default());
    let answers = decide(&[p], &model, default_rule());
    // Catches passing BTreeMap::new().
    assert_eq!(
        model.0.borrow()[0]["aws-cert"],
        vec!["a past fix".to_string()]
    );
    // No answer from the helper: nothing to log or store.
    assert!(answers.is_empty());
}

fn ranking_for(folder: &str, top: f64, abstain: f64) -> Ranking {
    Ranking {
        ranking: vec![
            RankedOption {
                id: folder.into(),
                probability: top,
            },
            RankedOption {
                id: "other".into(),
                probability: 0.01,
            },
        ],
        abstain,
        agreed: true,
    }
}

/// An event whose title equals a stored example of `zq-a`, with the example in `pending`.
fn event_matching_stored_example(conn: &Connection) -> i64 {
    pin(conn, "zq-a", None);
    let old = loose_event(conn, "old", "Standup notes");
    fix(conn, old, "zq-a");
    let new = loose_event(conn, "new", "Standup notes");
    let (_, pending) = load_pending(conn, apr20()).unwrap();
    assert_eq!(
        pending[0].examples["zq-a"],
        vec!["Standup notes".to_string()],
        "the examples are offered to the classifier as option text"
    );
    new
}

fn label_of(conn: &Connection, id: i64) -> Option<String> {
    fetch_event(conn, id).unwrap().unwrap().label_origin
}

#[test]
fn matching_a_stored_example_alone_never_files_an_event() {
    // catches: filing on example match without a clearing ranking
    let conn = open_memory().unwrap();
    let id = event_matching_stored_example(&conn);
    // Verdict has no answer for the event's own text: abstain.
    route_day(&conn, apr20(), &AlwaysNone, default_rule()).unwrap();
    assert_eq!(label_of(&conn, id), None);
}

#[test]
fn an_example_match_does_not_file_when_the_answer_abstains() {
    // catches: ignoring the abstain margin when examples match
    let conn = open_memory().unwrap();
    let id = event_matching_stored_example(&conn);
    let ranking = ranking_for("zq-a", 0.10, 0.50);
    route_day(&conn, apr20(), &Fixed(ranking), default_rule()).unwrap();
    assert_eq!(label_of(&conn, id), None);
}

#[test]
fn an_example_match_does_not_file_when_the_order_check_disagrees() {
    // catches: skipping the agreed check when examples match
    let conn = open_memory().unwrap();
    let id = event_matching_stored_example(&conn);
    let mut ranking = ranking_for("zq-a", 0.95, 0.01);
    ranking.agreed = false;
    route_day(&conn, apr20(), &Fixed(ranking), default_rule()).unwrap();
    assert_eq!(label_of(&conn, id), None);
}

#[test]
fn an_example_match_files_only_with_a_clearing_answer() {
    // catches: a vacuous suite where nothing ever files
    let conn = open_memory().unwrap();
    let id = event_matching_stored_example(&conn);
    route_day(
        &conn,
        apr20(),
        &Fixed(ranking_for("zq-a", 0.95, 0.01)),
        default_rule(),
    )
    .unwrap();
    assert_eq!(label_of(&conn, id).as_deref(), Some("guess"));
}

fn stored_ranking(conn: &Connection, id: i64) -> Option<String> {
    conn.query_row(
        "SELECT verdict_ranking FROM events WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )
    .unwrap()
}

#[test]
fn an_unfiled_guess_is_still_stored_and_logged() {
    let conn = open_memory().unwrap();
    pin(&conn, "zq-b", None);
    let id = loose_event(&conn, "e1", "zq-b thing");
    let ranking = ranking_for("zq-b", 0.10, 0.50); // far below abstain: not filed
    route_day(&conn, apr20(), &Fixed(ranking.clone()), default_rule()).unwrap();

    assert_eq!(fetch_event(&conn, id).unwrap().unwrap().label_origin, None);
    let stored: Ranking = serde_json::from_str(&stored_ranking(&conn, id).unwrap()).unwrap();
    // Catches storing the ranking only for filed events.
    assert_eq!(stored, ranking);
    let row = verdict_decisions::latest_for(&conn, DecisionKind::Project, &id.to_string())
        .unwrap()
        .unwrap();
    assert_eq!(row.source, DecisionSource::Verdict);
    assert_eq!(row.chosen, None);
    assert_eq!(row.options, vec!["zq-b".to_string()]);
    assert_eq!(row.ranking, Some(ranking));
    assert!(chrono::DateTime::parse_from_rfc3339(&row.decided_at).is_ok());
    assert!(row.state_json.contains("zq-b thing"));
}

#[test]
fn a_filed_guess_logs_what_was_applied() {
    let conn = open_memory().unwrap();
    pin(&conn, "zq-c", None);
    let id = loose_event(&conn, "e1", "zq-c thing");
    let ranking = ranking_for("zq-c", 0.90, 0.10);
    let stats = route_day(&conn, apr20(), &Fixed(ranking), default_rule()).unwrap();
    assert_eq!(stats.guesses_applied, 1);
    let ev = fetch_event(&conn, id).unwrap().unwrap();
    assert_eq!(ev.label_origin.as_deref(), Some("guess"));
    assert!(stored_ranking(&conn, id).is_some());
    let row = verdict_decisions::latest_for(&conn, DecisionKind::Project, &id.to_string())
        .unwrap()
        .unwrap();
    // Catches logging chosen = None for every row.
    assert_eq!(row.chosen.as_deref(), Some("zq-c"));
}

#[test]
fn an_owner_fix_is_logged_with_the_value_it_replaced() {
    let conn = open_memory().unwrap();
    pin(&conn, "zq-d", None);
    pin(&conn, "zq-e", None);
    let id = loose_event(&conn, "e1", "something");
    let subject = id.to_string();
    let latest = || {
        verdict_decisions::latest_for(&conn, DecisionKind::Project, &subject)
            .unwrap()
            .unwrap()
    };

    fix(&conn, id, "zq-d");
    let first = latest();
    assert_eq!(first.source, DecisionSource::Owner);
    assert_eq!(first.chosen.as_deref(), Some("zq-d"));
    // From unsorted there is nothing before.
    assert_eq!(first.previous, None);

    fix(&conn, id, "zq-e");
    let second = latest();
    // Catches previous = the new folder, or never read.
    assert_eq!(second.previous.as_deref(), Some("zq-d"));
    assert_eq!(second.chosen.as_deref(), Some("zq-e"));
    assert_eq!(second.ranking, None);
}

/// One claude event in `folder` the same day, so the shortlist offers it as recent work.
fn recent_work(conn: &Connection, folder: &str) {
    let prefix = crate::billing::work_prefix().unwrap();
    let id = repo::upsert_event(
        conn,
        &Event::minimal("claude", "w1", "2026-04-20T08:00:00+00:00", "t"),
    )
    .unwrap();
    conn.execute(
        "UPDATE events SET project_path = ?1 WHERE id = ?2",
        params![format!("{prefix}/{folder}"), id],
    )
    .unwrap();
}

fn slack_event(conn: &Connection, source_id: &str) -> i64 {
    repo::upsert_event(
        conn,
        &Event::minimal(
            "slack",
            source_id,
            "2026-04-20T10:00:00+00:00",
            "nothing known",
        ),
    )
    .unwrap()
}

fn project_rows(conn: &Connection, id: i64) -> usize {
    verdict_decisions::list_since(conn, DecisionKind::Project, "")
        .unwrap()
        .iter()
        .filter(|r| r.subject == id.to_string())
        .count()
}

#[test]
fn empty_shortlist_is_neither_classified_nor_logged() {
    let conn = open_memory().unwrap();
    let id = slack_event(&conn, "empty");
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
    // PanicsIfCalled catches sending an empty option set to the classifier.
    route_day(&conn, day, &PanicsIfCalled, default_rule()).unwrap();
    // Catches logging a decision (and storing a ranking) for an event with no options.
    assert_eq!(project_rows(&conn, id), 0);
}

#[test]
fn rerouting_an_unfiled_event_keeps_one_decision_row() {
    let conn = open_memory().unwrap();
    pin(&conn, "alpha", None);
    recent_work(&conn, "alpha");
    let id = slack_event(&conn, "abstain");
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
    let abstains = Fixed(Ranking {
        ranking: vec![],
        abstain: 1.0,
        agreed: true,
    });
    route_day(&conn, day, &abstains, default_rule()).unwrap();
    assert_eq!(project_rows(&conn, id), 1, "first run logs the answer");
    route_day(&conn, day, &abstains, default_rule()).unwrap();
    // Catches a plain INSERT per run.
    assert_eq!(project_rows(&conn, id), 1);
}

#[test]
fn relabelling_to_the_same_folder_logs_no_second_fix() {
    let conn = open_memory().unwrap();
    pin(&conn, "zq-f", None);
    let id = loose_event(&conn, "e2", "something");
    fix(&conn, id, "zq-f");
    fix(&conn, id, "zq-f");
    // Catches record_fix logging when previous == chosen.
    let owner_rows = verdict_decisions::list_since(&conn, DecisionKind::Project, "")
        .unwrap()
        .into_iter()
        .filter(|r| r.source == DecisionSource::Owner && r.subject == id.to_string())
        .count();
    assert_eq!(owner_rows, 1);
}

async fn routed_json(conn: Connection) -> serde_json::Value {
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use tower::ServiceExt;
    let app = crate::daemon::router(crate::daemon::state_from_conn(conn));
    let resp = app
        .oneshot(
            Request::get("/days/2026-04-20/routed")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    serde_json::from_slice(&to_bytes(resp.into_body(), 1 << 20).await.unwrap()).unwrap()
}

fn store_ranking_json(conn: &Connection, id: i64, json: &str) {
    conn.execute(
        "UPDATE events SET verdict_ranking = ?1 WHERE id = ?2",
        params![json, id],
    )
    .unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn routed_json_carries_the_stored_ranking_list() {
    let conn = open_memory().unwrap();
    let id = loose_event(&conn, "e1", "thing");
    let stored = ranking_for("zq-b", 0.5, 0.1);
    store_ranking_json(&conn, id, &serde_json::to_string(&stored).unwrap());
    let v = routed_json(conn).await;
    // Catches serialising the whole Ranking object, or dropping the field.
    assert_eq!(
        v[0]["ranking"],
        serde_json::json!([
            {"id": "zq-b", "probability": 0.5},
            {"id": "other", "probability": 0.01}
        ])
    );
}

#[tokio::test(flavor = "current_thread")]
async fn routed_json_omits_ranking_when_none_stored() {
    let conn = open_memory().unwrap();
    loose_event(&conn, "e1", "thing");
    let v = routed_json(conn).await;
    // Catches serialising `"ranking": null` instead of omitting it.
    assert!(v[0].as_object().unwrap().get("ranking").is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn routed_json_omits_ranking_for_unparseable_stored_json() {
    let conn = open_memory().unwrap();
    let id = loose_event(&conn, "e1", "thing");
    store_ranking_json(&conn, id, "{not json");
    let v = routed_json(conn).await;
    // Catches unwrap/expect on parse (panic) or surfacing an error response.
    assert_eq!(v[0]["id"], id);
    assert!(v[0].as_object().unwrap().get("ranking").is_none());
}

#[test]
fn accepts_applies_options_abstain_margin_and_runner_up_ratio() {
    let guess = |confidence, runner_up, abstain| Guess {
        folder: "a".into(),
        confidence,
        runner_up,
        abstain,
    };
    let options = vec!["a".to_string(), "b".to_string()];
    let rule = RouteRule {
        abstain_margin: 2.0,
        runner_up_ratio: 2.0,
    };
    assert!(accepts(&guess(0.9, 0.2, 0.3), &options, rule));
    assert!(
        !accepts(&guess(0.5, 0.1, 0.3), &options, rule),
        "below abstain margin"
    );
    assert!(
        !accepts(&guess(0.5, 0.3, 0.1), &options, rule),
        "below runner-up ratio"
    );
    assert!(
        !accepts(&guess(0.9, 0.0, 0.0), &["b".to_string()], rule),
        "not in options"
    );
}
