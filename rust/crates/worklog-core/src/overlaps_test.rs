use super::*;
use crate::db::open_memory;
use chrono::TimeZone;

fn at(h: u32, m: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 23, h, m, 0).unwrap()
}

const A: &str = "/Users/dev/Desktop/Work/vitinn-infra";
const B: &str = "/Users/dev/Desktop/Work/lyfjastofnun";
const PERSONAL: &str = "/Users/dev/Desktop/Projects/worklog";
// `lane_key`/`OverlapProject::project` are the folded folder name, not
// the full path — same key `billing::work_folder_for_path` returns.
const A_KEY: &str = "vitinn-infra";
const B_KEY: &str = "lyfjastofnun";

fn ev(h: u32, m: u32, source: &str, project: &str) -> InferEvent {
    InferEvent {
        ts: at(h, m),
        source: source.into(),
        duration_seconds: None,
        jira_issue: None,
        event_id: None,
        project_path: Some(project.into()),
    }
}

#[test]
fn two_work_projects_overlapping_for_an_hour_is_one_overlap() {
    let mut events: Vec<InferEvent> = (0..20).map(|i| ev(10, i * 3, "claude_turn", A)).collect();
    events.extend((0..20).map(|i| ev(10, i * 3, "claude_work", B)));
    let overlaps = compute_overlaps(&events);
    assert_eq!(overlaps.len(), 1, "got {overlaps:?}");
    let o = &overlaps[0];
    assert!(o.minutes >= 10);
    let projects: BTreeSet<&str> = o.projects.iter().map(|p| p.project.as_str()).collect();
    assert_eq!(projects, BTreeSet::from([A_KEY, B_KEY]));
}

#[test]
fn a_single_project_day_has_no_overlap() {
    let events: Vec<InferEvent> = (0..20).map(|i| ev(10, i * 3, "claude_turn", A)).collect();
    assert!(compute_overlaps(&events).is_empty());
}

#[test]
fn personal_activity_never_counts_toward_an_overlap() {
    let mut events: Vec<InferEvent> = (0..20).map(|i| ev(10, i * 3, "claude_turn", A)).collect();
    events.extend((0..20).map(|i| ev(10, i * 3, "claude_work", PERSONAL)));
    assert!(
        compute_overlaps(&events).is_empty(),
        "personal activity must not create a WORK/WORK overlap"
    );
}

#[test]
fn overlap_shorter_than_ten_minutes_is_dropped() {
    let mut events: Vec<InferEvent> =
        vec![ev(10, 0, "claude_turn", A), ev(10, 5, "claude_turn", A)];
    events.extend([ev(10, 0, "claude_work", B), ev(10, 5, "claude_work", B)]);
    assert!(compute_overlaps(&events).is_empty());
}

#[test]
fn human_and_background_events_are_split_per_project() {
    let mut events: Vec<InferEvent> = (0..20)
        .map(|i| ev(10, i * 3, "claude_turn", A)) // human
        .collect();
    events.extend((0..20).map(|i| ev(10, i * 3, "claude_work", B))); // background
    let overlaps = compute_overlaps(&events);
    let a = overlaps[0]
        .projects
        .iter()
        .find(|p| p.project == A_KEY)
        .unwrap();
    assert_eq!(a.human_events, 20);
    assert_eq!(a.background_events, 0);
    let b = overlaps[0]
        .projects
        .iter()
        .find(|p| p.project == B_KEY)
        .unwrap();
    assert_eq!(b.human_events, 0);
    assert_eq!(b.background_events, 20);
}

#[test]
fn two_interleaved_projects_each_get_their_own_activity_spans() {
    // A: 10:00–10:57 (step 3min, credit 2min -> one bridged span).
    let mut events: Vec<InferEvent> = (0..20).map(|i| ev(10, i * 3, "claude_turn", A)).collect();
    // B: overlaps A's second half, running past it: 10:30–11:27.
    events.extend((0..20).map(|i| ev(10 + (30 + i * 3) / 60, (30 + i * 3) % 60, "claude_work", B)));
    let activity = compute_activity(&events);
    assert_eq!(activity.len(), 2, "{activity:?}");

    let a = activity.iter().find(|p| p.project == A_KEY).unwrap();
    assert_eq!(a.spans.len(), 1, "{a:?}");
    assert_eq!(a.spans[0].started_at, at(10, 0));
    assert_eq!(a.spans[0].ended_at, at(10, 59));

    let b = activity.iter().find(|p| p.project == B_KEY).unwrap();
    assert_eq!(b.spans.len(), 1, "{b:?}");
    assert_eq!(b.spans[0].started_at, at(10, 30));
    assert_eq!(b.spans[0].ended_at, at(11, 29));
}

#[test]
fn activity_spans_bridge_gaps_up_to_five_minutes_but_not_beyond() {
    let events = vec![
        ev(9, 0, "claude_turn", A),  // ends 9:02
        ev(9, 7, "claude_turn", A),  // 5min after prior end -> bridged
        ev(9, 20, "claude_turn", A), // 11min after prior end -> new span
    ];
    let activity = compute_activity(&events);
    assert_eq!(activity.len(), 1);
    let spans = &activity[0].spans;
    assert_eq!(spans.len(), 2, "{spans:?}");
    assert_eq!(spans[0].started_at, at(9, 0));
    assert_eq!(spans[0].ended_at, at(9, 9));
    assert_eq!(spans[1].started_at, at(9, 20));
    assert_eq!(spans[1].ended_at, at(9, 22));
}

#[test]
fn personal_activity_is_excluded_from_day_activity() {
    let mut events: Vec<InferEvent> = (0..20).map(|i| ev(10, i * 3, "claude_turn", A)).collect();
    events.extend((0..20).map(|i| ev(10, i * 3, "claude_work", PERSONAL)));
    let activity = compute_activity(&events);
    assert_eq!(activity.len(), 1, "{activity:?}");
    assert_eq!(activity[0].project, A_KEY);
}

#[test]
fn day_overlaps_attaches_the_saved_allocation() {
    let conn = open_memory().unwrap();
    let day = NaiveDate::from_ymd_opt(2026, 9, 23).unwrap();
    for e in (0..20).map(|i| ev(10, i * 3, "claude_turn", A)) {
        crate::repo::upsert_event(
            &conn,
            &crate::models::Event::minimal(
                &e.source,
                format!("a{}", e.ts),
                e.ts.to_rfc3339(),
                "work",
            ),
        )
        .unwrap();
    }
    for e in (0..20).map(|i| ev(10, i * 3, "claude_work", B)) {
        let mut ev_row = crate::models::Event::minimal(
            &e.source,
            format!("b{}", e.ts),
            e.ts.to_rfc3339(),
            "work",
        );
        ev_row.project_path = Some(B.to_string());
        crate::repo::upsert_event(&conn, &ev_row).unwrap();
    }
    // Set project_path on the A rows too (Event::minimal leaves it None).
    conn.execute(
        &format!("UPDATE events SET project_path = '{A}' WHERE source = 'claude_turn'"),
        [],
    )
    .unwrap();

    // Save against the overlap's REAL boundaries (event timestamps
    // extended by InferEvent::end()'s credit, not a round hour).
    let before = day_overlaps(&conn, day).unwrap();
    assert_eq!(before.len(), 1);
    let window = &before[0];

    let mut shares = BTreeMap::new();
    shares.insert(A_KEY.to_string(), 0.7);
    shares.insert(B_KEY.to_string(), 0.3);
    save_allocation(&conn, day, window.started_at, window.ended_at, &shares).unwrap();

    let overlaps = day_overlaps(&conn, day).unwrap();
    assert_eq!(overlaps.len(), 1);
    assert_eq!(overlaps[0].allocation.as_ref().unwrap().shares, shares);
}
