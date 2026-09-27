//! Tests for `line_text_jobs::JobTracker`.

use super::*;

fn key(folder: &str) -> BillingLineKey {
    BillingLineKey {
        day: "2026-08-01".to_string(),
        folder: folder.to_string(),
        customer: "Acme Corp".to_string(),
    }
}

#[test]
fn untracked_key_is_none() {
    let tracker = JobTracker::default();
    assert_eq!(tracker.state(&key("a")), None);
}

#[test]
fn try_start_then_second_call_for_same_key_is_rejected() {
    let tracker = JobTracker::default();
    let k = key("a");
    assert!(tracker.try_start(k.clone()));
    assert_eq!(tracker.state(&k), Some(JobState::Running));
    assert!(
        !tracker.try_start(k),
        "a running job must reject a second start"
    );
}

#[test]
fn finish_ok_then_running_can_restart() {
    let tracker = JobTracker::default();
    let k = key("a");
    tracker.try_start(k.clone());
    tracker.finish(k.clone(), Ok(()));
    assert_eq!(tracker.state(&k), Some(JobState::Done));
    assert!(tracker.try_start(k.clone()), "a finished job can restart");
    assert_eq!(tracker.state(&k), Some(JobState::Running));
}

#[test]
fn finish_err_records_failed_with_reason() {
    let tracker = JobTracker::default();
    let k = key("a");
    tracker.try_start(k.clone());
    tracker.finish(k.clone(), Err("boom".to_string()));
    assert_eq!(
        tracker.state(&k),
        Some(JobState::Failed("boom".to_string()))
    );
}

#[test]
fn distinct_keys_are_independent() {
    let tracker = JobTracker::default();
    tracker.try_start(key("a"));
    assert_eq!(tracker.state(&key("b")), None);
}
