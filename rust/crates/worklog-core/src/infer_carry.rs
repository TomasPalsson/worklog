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
        if hits.len() < 2 {
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
            out.extend(crate::infer_allocations::span_block(
                evs, w[0], w[1], &b.events,
            ));
        }
    }
    out
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
}
