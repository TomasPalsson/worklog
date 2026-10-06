use super::*;

fn kv(k: &str, s: &str) -> (String, String) {
    (k.to_string(), s.to_string())
}

fn guess(key: &str, confidence: f64, runner_up: f64) -> Guess {
    Guess {
        folder: key.into(),
        confidence,
        runner_up,
        abstain: 0.1,
    }
}

fn rule() -> RouteRule {
    RouteRule {
        abstain_margin: 2.0,
        runner_up_ratio: 2.0,
    }
}

#[test]
fn sure_guess_is_picked_and_likely() {
    let c = [kv("A-1", "a"), kv("A-2", "b")];
    let o = pick_outcome(&c, Some(guess("A-1", 0.8, 0.1)), rule());
    assert_eq!(o.picked.as_deref(), Some("A-1"));
    assert_eq!(o.likely.as_deref(), Some("A-1"));
    assert_eq!(o.confidence, Some(0.8));
}

#[test]
fn unsure_guess_is_likely_only() {
    let c = [kv("A-1", "a"), kv("A-2", "b")];
    let o = pick_outcome(&c, Some(guess("A-1", 0.4, 0.35)), rule());
    assert_eq!(o.picked, None);
    assert_eq!(o.likely.as_deref(), Some("A-1"));
    assert_eq!(o.confidence, Some(0.4));
}

#[test]
fn no_usable_guess_is_empty() {
    let c = [kv("A-1", "a"), kv("A-2", "b")];
    assert_eq!(pick_outcome(&c, None, rule()), PickOutcome::default());
    let stranger = Some(guess("Z-9", 0.99, 0.0));
    assert_eq!(pick_outcome(&c, stranger, rule()), PickOutcome::default());
}

#[test]
fn candidates_dedupe_and_reuse_task_summary() {
    let tasks = vec![kv("A-1", "first"), kv("A-1", "dup")];
    assert_eq!(
        pick_candidates(tasks.clone(), Some("A-1")),
        vec![kv("A-1", "first")]
    );
    assert_eq!(
        pick_candidates(tasks, Some("B-2")),
        vec![kv("A-1", "first"), kv("B-2", "B-2")]
    );
    assert_eq!(pick_candidates(vec![kv("A-1", "x")], None).len(), 1);
}

#[test]
fn pick_records_once_and_never_overwrites_the_owner() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = dir.path().join("w.db");
    let conn = worklog_core::db::open(&db).unwrap();
    let get = |s| worklog_core::session_tickets::get(&conn, s).unwrap();
    record_unless_set(&db, "s1", "GENAI-1").unwrap();
    assert_eq!(get("s1").as_deref(), Some("GENAI-1"));
    worklog_core::session_tickets::set(&conn, "s2", "OWN-9").unwrap();
    record_unless_set(&db, "s2", "GENAI-1").unwrap();
    assert_eq!(get("s2").as_deref(), Some("OWN-9"));
}
