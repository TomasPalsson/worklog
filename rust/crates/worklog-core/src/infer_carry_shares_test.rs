use super::*;
use crate::db::open_memory;

const DAY: &str = "2026-09-25";

fn seed_share_row(conn: &Connection, started_at: &str, rows_json: &str) {
    conn.execute(
        "INSERT INTO block_customer_shares (day, started_at, shares, rows_json)
         VALUES (?1, ?2, '{}', ?3)",
        params![DAY, started_at, rows_json],
    )
    .unwrap();
}

fn share_rows_json(conn: &Connection, started_at: &str) -> Option<String> {
    conn.query_row(
        "SELECT rows_json FROM block_customer_shares WHERE day = ?1 AND started_at = ?2",
        params![DAY, started_at],
        |r| r.get(0),
    )
    .ok()
}

fn all_share_starts(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT started_at FROM block_customer_shares WHERE day = ?1")
        .unwrap();
    let mut out: Vec<String> = stmt
        .query_map(params![DAY], |r| r.get(0))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    out.sort();
    out
}

fn span(s: &str, e: &str) -> (String, String) {
    (s.to_string(), e.to_string())
}

const ROWS_JSON: &str = r#"[{"customer":"Acme","deild":null,"fraction":1.0}]"#;

#[test]
fn a_shifted_start_inherits_the_old_rows_json() {
    let c = open_memory().unwrap();
    let old_start = "2026-09-25T08:24:00+00:00";
    let old_end = "2026-09-25T09:52:00+00:00";
    let new_start = "2026-09-25T08:34:00+00:00";
    seed_share_row(&c, old_start, ROWS_JSON);

    let old_spans = vec![span(old_start, old_end)];
    let new_spans = vec![span(new_start, old_end)];
    carry_owner_tables(&c, DAY, &old_spans, &new_spans).unwrap();

    assert_eq!(
        share_rows_json(&c, new_start).as_deref(),
        Some(ROWS_JSON),
        "the rebuilt block must inherit the old block's rows_json"
    );
}

#[test]
fn b_a_split_block_inherits_into_both_pieces() {
    let c = open_memory().unwrap();
    let old_start = "2026-09-25T08:00:00+00:00";
    let old_end = "2026-09-25T10:00:00+00:00";
    let mid = "2026-09-25T09:00:00+00:00";
    seed_share_row(&c, old_start, ROWS_JSON);

    let old_spans = vec![span(old_start, old_end)];
    let new_spans = vec![span(old_start, mid), span(mid, old_end)];
    carry_owner_tables(&c, DAY, &old_spans, &new_spans).unwrap();

    assert_eq!(
        share_rows_json(&c, old_start).as_deref(),
        Some(ROWS_JSON),
        "the piece that kept the old start keeps its own row"
    );
    assert_eq!(
        share_rows_json(&c, mid).as_deref(),
        Some(ROWS_JSON),
        "the second piece must inherit a copy too"
    );
}

#[test]
fn c_an_exact_start_match_is_never_overwritten() {
    let c = open_memory().unwrap();
    let start = "2026-09-25T08:24:00+00:00";
    let end = "2026-09-25T09:52:00+00:00";
    seed_share_row(&c, start, ROWS_JSON);

    let old_spans = vec![span(start, end)];
    let new_spans = vec![span(start, end)];
    carry_owner_tables(&c, DAY, &old_spans, &new_spans).unwrap();

    assert_eq!(share_rows_json(&c, start).as_deref(), Some(ROWS_JSON));
    assert_eq!(all_share_starts(&c), vec![start.to_string()]);
}

#[test]
fn d_no_orphan_rows_remain_after_a_rebuild() {
    let c = open_memory().unwrap();
    let old_start = "2026-09-25T08:24:00+00:00";
    let old_end = "2026-09-25T08:30:00+00:00";
    seed_share_row(&c, old_start, ROWS_JSON);

    // The new day's blocks land nowhere near the old span — it's gone.
    let old_spans = vec![span(old_start, old_end)];
    let new_spans = vec![span(
        "2026-09-25T09:00:00+00:00",
        "2026-09-25T10:00:00+00:00",
    )];
    carry_owner_tables(&c, DAY, &old_spans, &new_spans).unwrap();

    assert!(
        all_share_starts(&c).is_empty(),
        "the orphaned row must be deleted, not left behind"
    );
}

#[test]
fn resolution_snapshots_inherit_on_a_shifted_start() {
    let c = open_memory().unwrap();
    let old_start = "2026-09-25T08:24:00+00:00";
    let old_end = "2026-09-25T09:52:00+00:00";
    let new_start = "2026-09-25T08:34:00+00:00";
    let parts_json = r#"[{"customer":"Acme","deild":null,"fraction":1.0}]"#;
    c.execute(
        "INSERT INTO block_resolution_snapshots (day, started_at, description, parts_json)
         VALUES (?1, ?2, ?3, ?4)",
        params![DAY, old_start, "work", parts_json],
    )
    .unwrap();

    let old_spans = vec![span(old_start, old_end)];
    let new_spans = vec![span(new_start, old_end)];
    carry_owner_tables(&c, DAY, &old_spans, &new_spans).unwrap();

    let (description, parts): (Option<String>, String) = c
        .query_row(
            "SELECT description, parts_json FROM block_resolution_snapshots
              WHERE day = ?1 AND started_at = ?2",
            params![DAY, new_start],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(description.as_deref(), Some("work"));
    assert_eq!(parts, parts_json);

    let remaining: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM block_resolution_snapshots WHERE day = ?1 AND started_at = ?2",
            params![DAY, old_start],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(remaining, 0, "the orphaned old-start row must be gone");
}
