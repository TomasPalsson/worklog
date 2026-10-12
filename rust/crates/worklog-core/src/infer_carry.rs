//! Rebuilding a day must never lose the tickets on its blocks.
//!
//! `persist_blocks` carries a prior block's ticket onto the new block that
//! starts at the same time (or overlaps it). When two ticketed blocks fuse
//! into one on rebuild — the gap between them filled in, or a split handed
//! both to one project — only one ticket fits. So before persisting, a new
//! block that covers prior blocks with DIFFERENT tickets is cut where each
//! later ticketed block began, and every piece keeps its own ticket.

use chrono::{DateTime, Utc};

use crate::infer::InferBlock;

/// `(started_at, ended_at, ticket)` of a prior block that had a ticket.
pub(crate) type Ticketed = (DateTime<Utc>, DateTime<Utc>, String);

pub(crate) fn cut_at_ticket_edges(blocks: &[InferBlock], ticketed: &[Ticketed]) -> Vec<InferBlock> {
    let mut out = Vec::new();
    for b in blocks {
        let mut hits: Vec<&Ticketed> = ticketed
            .iter()
            .filter(|(s, e, _)| *s < b.ended_at && b.started_at < *e)
            .collect();
        hits.sort_by_key(|(s, _, _)| *s);
        hits.dedup_by(|x, y| x.2 == y.2);
        // A meeting is one event with no folder: cutting it leaves pieces
        // `span_block` can't anchor to a project, so all of it would vanish.
        if b.is_calendar || hits.len() < 2 {
            out.push(b.clone());
            continue;
        }
        let mut edges: Vec<DateTime<Utc>> = vec![b.started_at];
        edges.extend(
            hits[1..]
                .iter()
                .map(|(s, _, _)| *s)
                .filter(|s| *s > b.started_at && *s < b.ended_at),
        );
        edges.push(b.ended_at);
        for w in edges.windows(2) {
            let evs = b
                .events
                .iter()
                .filter(|x| x.ts >= w[0] && x.ts < w[1])
                .cloned()
                .collect();
            // keep_thin=true: a ticket-edge cut re-slices an
            // already-approved block — a short remainder must never
            // vanish just for being small (matches apply_split's rule).
            out.extend(crate::infer_allocations::span_block(
                evs, w[0], w[1], &b.events, true,
            ));
        }
        // Deliberately NOT re-merged (R6 applies to `apply_split` only,
        // see infer_evidence::merge_touching_same_project's doc comment):
        // these pieces exist specifically to keep two different prior
        // tickets apart, and each piece's own `jira_issue` is still None
        // here — the real ticket is reattached later by persist_blocks'
        // carry matching, so a same-project/same-(absent)-ticket check at
        // this point can't tell a real split from a preserved one.
    }
    out
}

/// A block's stale description is dropped when the block's length changed a
/// lot on rebuild — a 5-minute note doesn't describe a 3-hour block. `prior`
/// and `new` are each `(started_at, ended_at)` in minutes since epoch.
/// A `manual` description is never dropped.
pub(crate) fn keeps_description(
    prior: (i64, i64),
    new: (i64, i64),
    estimated_by: Option<&str>,
) -> bool {
    if estimated_by == Some("manual") {
        return true;
    }
    let prior_len = prior.1 - prior.0;
    let new_len = new.1 - new.0;
    if (new_len - prior_len).abs() >= 30 {
        return false;
    }
    let max_len = prior_len.max(new_len);
    let min_len = prior_len.min(new_len);
    max_len * 2 <= min_len * 3
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    fn at(h: u32, m: u32) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.with_ymd_and_hms(2026, 9, 23, h, m, 0).unwrap()
    }

    /// Two ticketed blocks that fuse on rebuild (the gap between them fills
    /// in) must keep both tickets.
    #[test]
    fn fused_blocks_keep_both_tickets() {
        use crate::models::Event;
        let conn = crate::db::open_memory().unwrap();
        let day = chrono::NaiveDate::from_ymd_opt(2026, 9, 23).unwrap();
        let add = |id: String, h: u32, m: u32| {
            let mut e = Event::minimal("shell", id, at(h, m).to_rfc3339(), "git");
            e.project_path = Some("/Users/dev/Desktop/Work/vitinn-infra".into());
            crate::repo::upsert_event(&conn, &e).unwrap();
        };
        let rebuild = || {
            let blocks = crate::infer_allocations::build_day_blocks(&conn, day).unwrap();
            crate::infer::persist_blocks(&conn, day, &blocks).unwrap();
        };
        let tickets = || -> Vec<String> {
            let mut t: Vec<String> = conn
                .prepare("SELECT jira_issue FROM blocks WHERE day = ?1 AND jira_issue IS NOT NULL")
                .unwrap()
                .query_map([day.to_string()], |r| r.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            t.sort();
            t
        };
        // 9:00–9:10 and 10:00–10:10, 50 minutes apart: two blocks.
        for m in (0..=10).step_by(2) {
            add(format!("a{m}"), 9, m);
            add(format!("b{m}"), 10, m);
        }
        rebuild();
        conn.execute(
            "UPDATE blocks SET jira_issue = 'T-1' WHERE started_at LIKE '%T09:00%'",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE blocks SET jira_issue = 'T-2' WHERE started_at LIKE '%T10:00%'",
            [],
        )
        .unwrap();
        assert_eq!(tickets(), ["T-1", "T-2"]);

        // The gap fills in: one stretch of work 9:00–10:10.
        for m in (12..60).step_by(4) {
            add(format!("c{m}"), 9, m);
        }
        rebuild();
        assert_eq!(tickets(), ["T-1", "T-2"], "neither ticket may be lost");
    }

    /// A meeting overlapping two prior ticketed blocks has no folder, so
    /// cutting it left every piece without a project and all of it vanished.
    #[test]
    fn meeting_over_two_tickets_is_not_cut() {
        let meeting = crate::infer::InferEvent {
            ts: at(13, 0),
            source: "gcal".into(),
            duration_seconds: Some(3600),
            jira_issue: None,
            event_id: Some(1),
            project_path: None,
            session_id: None,
            title: None,
            lane_tag: None,
        };
        let block = crate::infer::new_block(&meeting);
        let ticketed = vec![
            (at(12, 50), at(14, 0), "T-1".to_string()),
            (at(13, 31), at(14, 1), "T-2".to_string()),
        ];
        let out = super::cut_at_ticket_edges(&[block], &ticketed);
        assert_eq!(out.len(), 1);
        assert_eq!((out[0].started_at, out[0].ended_at), (at(13, 0), at(14, 0)));
    }

    /// A new meeting overlapping a later synced block must not borrow that
    /// block's ticket and Tempo id while the later block still exists
    /// (exact-start match) — two tickets would share one Tempo worklog.
    #[test]
    fn overlap_fallback_skips_prior_rows_with_exact_start_match() {
        let conn = crate::db::open_memory().unwrap();
        let day = chrono::NaiveDate::from_ymd_opt(2026, 9, 23).unwrap();
        let blk = |source: &str, (h, m): (u32, u32), (eh, em): (u32, u32)| {
            let mut b = crate::infer::new_block(&crate::infer::InferEvent {
                ts: at(h, m),
                source: source.into(),
                duration_seconds: None,
                jira_issue: None,
                event_id: None,
                project_path: None,
                session_id: None,
                title: None,
                lane_tag: None,
            });
            b.ended_at = at(eh, em);
            b.duration_seconds = (b.ended_at - b.started_at).num_seconds();
            b
        };
        let prep = blk("claude", (12, 50), (13, 8));
        let coding = blk("claude", (13, 31), (14, 1));
        crate::infer::persist_blocks(&conn, day, &[prep.clone(), coding.clone()]).unwrap();
        for (hm, ticket, tempo) in [("T12:50", "T-1", "111"), ("T13:31", "T-2", "222")] {
            conn.execute(
                "UPDATE blocks SET jira_issue = ?1, tempo_worklog_id = ?2 WHERE started_at LIKE ?3",
                rusqlite::params![ticket, tempo, format!("%{hm}%")],
            )
            .unwrap();
        }

        let meeting = blk("gcal", (13, 0), (14, 0));
        crate::infer::persist_blocks(&conn, day, &[prep, meeting, coding]).unwrap();

        let stored = crate::repo::list_blocks_for_day(&conn, "2026-09-23").unwrap();
        let get = |hm: &str| stored.iter().find(|b| b.started_at.contains(hm)).unwrap();
        assert_eq!(get("T13:31").tempo_worklog_id.as_deref(), Some("222"));
        assert_eq!(
            get("T13:00").tempo_worklog_id,
            None,
            "meeting must not borrow 222"
        );
        assert_eq!(
            get("T13:00").jira_issue,
            None,
            "meeting must not borrow T-2"
        );
    }

    #[test]
    fn grown_block_drops_description() {
        assert!(!super::keeps_description((0, 30), (0, 182), None));
    }

    #[test]
    fn manual_description_always_kept() {
        assert!(super::keeps_description((0, 30), (0, 182), Some("manual")));
    }

    #[test]
    fn grow_by_exactly_30_drops_description() {
        assert!(!super::keeps_description((0, 60), (0, 90), None));
    }

    #[test]
    fn grow_past_1_5x_under_30_drops_description() {
        assert!(!super::keeps_description((0, 10), (0, 16), None));
    }

    #[test]
    fn shrink_past_1_5x_drops_description() {
        assert!(!super::keeps_description((0, 60), (0, 25), None));
    }

    #[test]
    fn just_under_thresholds_keeps_description() {
        assert!(super::keeps_description((0, 60), (0, 89), None));
    }

    #[test]
    fn unchanged_length_keeps_description() {
        assert!(super::keeps_description((0, 60), (0, 60), None));
    }
}
