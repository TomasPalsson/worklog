use super::*;
use crate::billing_registry::{upsert_folder, FolderMap};
use crate::db::open_memory;
use crate::models::Event;
use crate::repo;
use rusqlite::{params, Connection};

fn pin(conn: &Connection, folder: &str, customer: &str, verkefni: Option<&str>) {
    upsert_folder(
        conn,
        &FolderMap {
            id: None,
            folder: folder.into(),
            customer: Some(customer.into()),
            verkefni: verkefni.map(str::to_owned),
            billable: true,
            multi_tenant: false,
        },
    )
    .unwrap();
}

fn block(conn: &Connection, day: &str, folder: &str, secs: i64, text: &str) {
    let start = format!("{day}T09:00:00+00:00");
    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds, description)
         VALUES (?1, ?2, ?2, ?3, ?4)",
        params![day, start, secs, text],
    )
    .unwrap();
    let id = conn.last_insert_rowid();
    let path = format!(
        "{}/Desktop/Work/{folder}",
        dirs::home_dir().unwrap().to_string_lossy()
    );
    let mut ev = Event::minimal("claude", format!("{day}-{folder}-{secs}"), &start, "s");
    ev.project_path = Some(path);
    let eid = repo::upsert_event(conn, &ev).unwrap();
    conn.execute(
        "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
        params![id, eid],
    )
    .unwrap();
}

fn fixture() -> Connection {
    let c = open_memory().unwrap();
    pin(&c, "web", "APRÓ", Some("Vefsíður"));
    pin(&c, "ops", "APRÓ", Some("Rekstur"));
    pin(&c, "loose", "APRÓ", None);
    pin(&c, "other", "Sjúkra", Some("Annað"));
    // July: Vefsíður 1h + 2h = 3.0, Rekstur 0.5, loose 1.0 (no deild)
    block(&c, "2026-07-01", "web", 3600, "Síða eitt");
    block(&c, "2026-07-31", "web", 7200, "Síða tvö");
    block(&c, "2026-07-15", "ops", 1800, "Vakt");
    block(&c, "2026-07-10", "loose", 3600, "Óflokkað");
    // other customer, same month: must not appear
    block(&c, "2026-07-10", "other", 3600, "Annað verk");
    // neighbours of the month: must not count in July
    block(&c, "2026-08-01", "web", 3600, "Ágúst");
    // June: Vefsíður 2.0, Rekstur 1.5, nothing for loose
    block(&c, "2026-06-30", "web", 7200, "Júní");
    block(&c, "2026-06-02", "ops", 5400, "Júní vakt");
    c
}

fn line<'a>(r: &'a MonthlyReport, verkefni: Option<&str>) -> &'a ReportLine {
    r.lines
        .iter()
        .find(|l| l.verkefni.as_deref() == verkefni)
        .unwrap_or_else(|| panic!("no line for {verkefni:?}"))
}

#[test]
fn groups_hours_by_deild_with_texts() {
    // fails a report that sums per customer or per day instead of per deild
    let r = monthly_report(&fixture(), "APRÓ", "2026-07").unwrap();
    assert_eq!(r.lines.len(), 3);
    let web = line(&r, Some("Vefsíður"));
    assert_eq!(web.hours, 3.0);
    assert!(web.texts.contains(&"Síða eitt".to_string()));
    assert!(web.texts.contains(&"Síða tvö".to_string()));
    assert_eq!(line(&r, Some("Rekstur")).hours, 0.5);
}

#[test]
fn excludes_other_customers_and_neighbouring_months() {
    // catches no customer filter; a range that is off by one at either end of July
    let r = monthly_report(&fixture(), "APRÓ", "2026-07").unwrap();
    let total: f64 = r.lines.iter().map(|l| l.hours).sum();
    assert_eq!(total, 4.5, "3.0 + 0.5 + 1.0, no Aug 1, Jun 30, or Sjúkra");
}

#[test]
fn change_from_previous_month_is_signed() {
    // catches an unsigned or swapped (previous - current) delta
    let r = monthly_report(&fixture(), "APRÓ", "2026-07").unwrap();
    assert_eq!(line(&r, Some("Vefsíður")).change(), Some(1.0));
    assert_eq!(line(&r, Some("Rekstur")).change(), Some(-1.0));
    let text = r.to_text();
    assert!(text.contains("+1.0"), "{text}");
    assert!(text.contains("-1.0"), "{text}");
}

#[test]
fn empty_previous_month_is_na_not_zero_delta() {
    // catches treating a missing previous month as 0 hours (+1.0)
    let r = monthly_report(&fixture(), "APRÓ", "2026-07").unwrap();
    let loose = line(&r, None);
    assert_eq!(loose.change(), None);
    assert!(r.to_text().contains("n/a"));
    assert!(r.to_csv().contains(",n/a,"));
}

#[test]
fn previous_month_wraps_the_year() {
    // catches month - 1 = 0 for January
    let c = open_memory().unwrap();
    pin(&c, "web", "APRÓ", Some("Vefsíður"));
    block(&c, "2025-12-31", "web", 3600, "des");
    block(&c, "2026-01-05", "web", 7200, "jan");
    let r = monthly_report(&c, "APRÓ", "2026-01").unwrap();
    assert_eq!(line(&r, Some("Vefsíður")).change(), Some(1.0));
}

#[test]
fn unresolved_deild_stays_empty_in_csv() {
    // catches guessing a deild (or printing the dash) for an unpinned verkefni
    let r = monthly_report(&fixture(), "APRÓ", "2026-07").unwrap();
    assert_eq!(line(&r, None).hours, 1.0);
    let csv = r.to_csv();
    assert!(csv.contains("\n,1.0,n/a,Óflokkað"), "{csv}");
}

#[test]
fn csv_has_header_one_row_per_deild_and_quotes_cells() {
    // catches unquoted commas and an unguarded leading '=' formula
    let c = open_memory().unwrap();
    pin(&c, "web", "APRÓ", Some("Vefsíður"));
    block(&c, "2026-07-01", "web", 3600, "=a, \"b\"");
    let csv = monthly_report(&c, "APRÓ", "2026-07").unwrap().to_csv();
    assert_eq!(
        csv,
        "verkefni,timar,breyting,texti\nVefsíður,1.0,n/a,\"'=a, \"\"b\"\"\""
    );
}

#[test]
fn rejects_a_malformed_month() {
    // catches swallowing the parse error into an empty report
    for m in ["2026-13", "2026", "July", "2026-7-1"] {
        assert!(
            monthly_report(&open_memory().unwrap(), "APRÓ", m).is_err(),
            "{m}"
        );
    }
}

#[test]
fn month_without_hours_prints_an_empty_report() {
    let r = monthly_report(&open_memory().unwrap(), "APRÓ", "2026-07").unwrap();
    assert!(r.lines.is_empty());
}
