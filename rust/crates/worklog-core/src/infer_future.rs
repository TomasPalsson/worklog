//! Time that hasn't happened yet is never work.

use chrono::{DateTime, Utc};

use crate::infer::InferEvent;

/// An event starting after `now` (a calendar entry later today) is
/// dropped, and a calendar event still running is cut off at `now`.
/// Re-inference later picks up the rest.
pub(crate) fn drop_future(events: Vec<InferEvent>, now: DateTime<Utc>) -> Vec<InferEvent> {
    events
        .into_iter()
        .filter(|e| e.ts <= now)
        .map(|mut e| {
            if e.is_calendar() {
                let elapsed = (now - e.ts).num_seconds();
                e.duration_seconds = e.duration_seconds.map(|d| d.min(elapsed));
            }
            e
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 9, h, m, 0).unwrap()
    }

    fn calendar(h: u32, m: u32, secs: i64) -> InferEvent {
        InferEvent {
            ts: at(h, m),
            source: "gcal".into(),
            duration_seconds: Some(secs),
            jira_issue: None,
            event_id: None,
            project_path: None,
            session_id: None,
            title: None,
            lane_tag: None,
        }
    }

    #[test]
    fn calendar_time_after_now_is_not_counted() {
        // Regression (2026-10-09): an evening calendar event became a 6h
        // block at 10:21 that morning, before it had happened.
        let events = vec![
            calendar(9, 0, 30 * 60),   // over: kept whole
            calendar(10, 0, 60 * 60),  // in progress at 10:20: cut to 20 min
            calendar(16, 0, 6 * 3600), // not started: dropped
        ];
        let kept = drop_future(events, at(10, 20));
        let spans: Vec<_> = kept.iter().map(|e| (e.ts, e.end())).collect();
        assert_eq!(spans, vec![(at(9, 0), at(9, 30)), (at(10, 0), at(10, 20))]);
    }
}
