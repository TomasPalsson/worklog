//! One-off upgrade for spec 006: deletes stored personal-owner commit/PR
//! events (FR-02) and re-infers every stored day once under the new
//! attribution rules, carrying `exported_at`, Tempo ids, manual
//! descriptions and tickets (FR-10, D-09).

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{params, Connection};

use crate::{collectors::github, infer, infer_allocations, local_clone};

/// Runs the phase-A upgrade against `conn`. `personal_user` is the
/// configured GitHub login (`None` when unset), used to delete previously
/// stored rows from the owner's personal account (D-06).
pub fn run(conn: &Connection, personal_user: Option<&str>) -> Result<()> {
    run_with(
        conn,
        personal_user,
        local_clone::folder_for_repo,
        |folder, sha| local_clone::sha_is_local(Path::new(folder), sha),
    )
}

/// Test seam: `folder_for_repo`/`sha_is_local` are injected so tests don't
/// depend on `~/Desktop/Work`. `run` wires in the real `local_clone` fns.
fn run_with(
    conn: &Connection,
    personal_user: Option<&str>,
    folder_for_repo: impl Fn(&str) -> Option<String>,
    sha_is_local: impl Fn(&str, &str) -> bool,
) -> Result<()> {
    delete_personal_rows(conn, personal_user)?;
    reresolve_github_events(conn, &folder_for_repo, &sha_is_local)?;
    reinfer_all_days(conn)?;
    Ok(())
}

/// FR-02: delete stored `github_commit`/`github_pr` rows whose repo owner
/// is the configured personal account, same rule as collection time
/// (`github::is_personal_owner`). `block_events` rows for the deleted
/// events cascade via the FK; the blocks and org rows are untouched.
pub(crate) fn delete_personal_rows(conn: &Connection, personal_user: Option<&str>) -> Result<()> {
    let Some(user) = personal_user else {
        return Ok(());
    };
    let mut stmt =
        conn.prepare("SELECT id, repo FROM events WHERE source IN ('github_commit', 'github_pr')")?;
    let rows: Vec<(i64, Option<String>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    drop(stmt);
    for (id, repo) in rows {
        if repo
            .as_deref()
            .is_some_and(|r| github::is_personal_owner(r, user))
        {
            conn.execute("DELETE FROM events WHERE id = ?1", params![id])?;
        }
    }
    Ok(())
}

/// Re-resolve every remaining `github_commit`/`github_pr` row's
/// `project_path`/`elsewhere`, mirroring what `collectors::github` does
/// at collection time: a commit is local when its sha is reachable from
/// the repo's clone; a PR is local when the clone simply exists.
fn reresolve_github_events(
    conn: &Connection,
    folder_for_repo: &impl Fn(&str) -> Option<String>,
    sha_is_local: &impl Fn(&str, &str) -> bool,
) -> Result<()> {
    // FR-06: an owner-moved row (`elsewhere = 2`) is never re-resolved —
    // it must keep its manual placement across this upgrade too.
    let mut stmt = conn.prepare(
        "SELECT id, source, source_id, repo FROM events
          WHERE source IN ('github_commit', 'github_pr') AND elsewhere != 2",
    )?;
    let rows: Vec<(i64, String, String, Option<String>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<std::result::Result<_, _>>()?;
    drop(stmt);
    for (id, source, source_id, repo) in rows {
        let Some(repo) = repo else { continue };
        let folder = folder_for_repo(&repo);
        let is_local = if source == "github_commit" {
            folder
                .as_deref()
                .is_some_and(|f| sha_is_local(f, &source_id))
        } else {
            folder.is_some()
        };
        if is_local {
            conn.execute(
                "UPDATE events SET project_path = ?1, elsewhere = 0 WHERE id = ?2",
                params![folder, id],
            )?;
        } else {
            // Match collection time (`collectors::github`): a row that's
            // no longer local carries no project_path either.
            conn.execute(
                "UPDATE events SET project_path = NULL, elsewhere = 1 WHERE id = ?1",
                params![id],
            )?;
        }
    }
    Ok(())
}

/// D-09: rebuild every day that has a stored block or event, so the new
/// attribution rules and the row deletion above are reflected everywhere.
fn reinfer_all_days(conn: &Connection) -> Result<()> {
    let mut days: BTreeSet<NaiveDate> = BTreeSet::new();
    {
        let mut stmt = conn.prepare("SELECT DISTINCT day FROM blocks")?;
        for row in stmt.query_map([], |r| r.get::<_, String>(0))? {
            if let Ok(d) = NaiveDate::parse_from_str(&row?, "%Y-%m-%d") {
                days.insert(d);
            }
        }
    }
    {
        let mut stmt = conn.prepare("SELECT started_at FROM events")?;
        for row in stmt.query_map([], |r| r.get::<_, String>(0))? {
            if let Ok(dt) = DateTime::parse_from_rfc3339(&row?) {
                days.insert(crate::tz::local_date(dt.with_timezone(&Utc)));
            }
        }
    }
    for day in days {
        let blocks = infer_allocations::build_day_blocks(conn, day)?;
        infer::persist_blocks(conn, day, &blocks)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::models::Event;
    use crate::repo;
    use rusqlite::params;

    fn insert_event(conn: &Connection, source: &str, source_id: &str, repo: &str, ts: &str) -> i64 {
        let mut ev = Event::minimal(source, source_id, ts, format!("{source} {source_id}"));
        ev.repo = Some(repo.to_owned());
        // 30 minutes: long enough to clear MIN_BLOCK_MINUTES on its own,
        // so the day always rebuilds into exactly one block.
        ev.duration_seconds = Some(1800);
        repo::upsert_event(conn, &ev).unwrap()
    }

    fn resolver_always_local(
        folder: &'static str,
    ) -> (impl Fn(&str) -> Option<String>, impl Fn(&str, &str) -> bool) {
        (
            move |_repo: &str| Some(folder.to_owned()),
            |_f: &str, _sha: &str| true,
        )
    }

    fn resolver_never_local() -> (impl Fn(&str) -> Option<String>, impl Fn(&str, &str) -> bool) {
        (|_repo: &str| None, |_f: &str, _sha: &str| false)
    }

    struct CarriedFields {
        jira_issue: Option<String>,
        description: Option<String>,
        estimated_by: Option<String>,
        tempo_worklog_id: Option<String>,
        exported_at: Option<String>,
    }

    #[test]
    fn carries_all_fields() {
        // Fixture: a block with tempo_worklog_id, exported_at,
        // estimated_by='manual' + description, and a manual jira_issue,
        // linked to org commit events. After run(), a rebuilt block for
        // the same day must still carry all four. Two commits ten
        // minutes apart so the rebuilt span clears MIN_BLOCK_MINUTES —
        // a lone point event's 2-minute CREDIT would not.
        let conn = db::open_memory().unwrap();
        let event_id = insert_event(
            &conn,
            "github_commit",
            "sha1",
            "aproorg/worklog",
            "2026-04-18T09:00:00+00:00",
        );
        insert_event(
            &conn,
            "github_commit",
            "sha2",
            "aproorg/worklog",
            "2026-04-18T09:10:00+00:00",
        );
        conn.execute(
            "INSERT INTO blocks (
                day, jira_issue, started_at, ended_at, duration_seconds,
                description, estimated_by, tempo_worklog_id, exported_at
             ) VALUES (
                '2026-04-18', 'ABC-1', '2026-04-18T09:00:00+00:00', '2026-04-18T09:30:00+00:00', 1800,
                'manual write-up', 'manual', '999', '2026-04-19T00:00:00+00:00'
             )",
            [],
        )
        .unwrap();
        let block_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
            params![block_id, event_id],
        )
        .unwrap();

        let (folder_for_repo, sha_is_local) = resolver_always_local("/work/worklog");
        run_with(&conn, None, folder_for_repo, sha_is_local).unwrap();

        let carried = conn
            .query_row(
                "SELECT jira_issue, description, estimated_by, tempo_worklog_id, exported_at
                   FROM blocks WHERE day = '2026-04-18'",
                [],
                |r| {
                    Ok(CarriedFields {
                        jira_issue: r.get(0)?,
                        description: r.get(1)?,
                        estimated_by: r.get(2)?,
                        tempo_worklog_id: r.get(3)?,
                        exported_at: r.get(4)?,
                    })
                },
            )
            .unwrap();
        assert_eq!(carried.jira_issue.as_deref(), Some("ABC-1"));
        assert_eq!(carried.description.as_deref(), Some("manual write-up"));
        assert_eq!(carried.estimated_by.as_deref(), Some("manual"));
        assert_eq!(carried.tempo_worklog_id.as_deref(), Some("999"));
        assert_eq!(
            carried.exported_at.as_deref(),
            Some("2026-04-19T00:00:00+00:00"),
            "A15: exported_at must survive the carry, or a rebuild un-marks billed work"
        );
    }

    #[test]
    fn personal_rows_deleted_org_rows_kept() {
        let conn = db::open_memory().unwrap();
        insert_event(
            &conn,
            "github_commit",
            "personal-sha",
            "TomasPalsson/dotfiles",
            "2026-04-18T09:00:00+00:00",
        );
        insert_event(
            &conn,
            "github_commit",
            "org-sha",
            "aproorg/worklog",
            "2026-04-18T10:00:00+00:00",
        );

        let (folder_for_repo, sha_is_local) = resolver_always_local("/work/worklog");
        run_with(&conn, Some("TomasPalsson"), folder_for_repo, sha_is_local).unwrap();

        let remaining: Vec<String> = conn
            .prepare("SELECT source_id FROM events ORDER BY source_id")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        assert_eq!(
            remaining,
            vec!["org-sha".to_string()],
            "personal-owner row must be gone, org row must survive"
        );
    }

    #[test]
    fn absent_sha_marked_elsewhere_and_out_of_blocks() {
        let conn = db::open_memory().unwrap();
        insert_event(
            &conn,
            "github_commit",
            "nowhere-sha",
            "aproorg/worklog",
            "2026-04-18T09:00:00+00:00",
        );

        let (folder_for_repo, sha_is_local) = resolver_never_local();
        run_with(&conn, None, folder_for_repo, sha_is_local).unwrap();

        let elsewhere: i64 = conn
            .query_row(
                "SELECT elsewhere FROM events WHERE source_id = 'nowhere-sha'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(elsewhere, 1);

        let block_events: i64 = conn
            .query_row("SELECT COUNT(*) FROM block_events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            block_events, 0,
            "an elsewhere event must never end up in a block"
        );
    }

    #[test]
    fn reresolve_clears_project_path_when_no_longer_local() {
        // A row previously resolved local (project_path set by a stale
        // collection) whose clone/sha is gone by the time the upgrade
        // re-resolves it must end up exactly as collection time would
        // leave a never-local row: project_path cleared too, not just
        // elsewhere flipped.
        let conn = db::open_memory().unwrap();
        let event_id = insert_event(
            &conn,
            "github_commit",
            "flip-sha",
            "aproorg/worklog",
            "2026-04-18T09:00:00+00:00",
        );
        conn.execute(
            "UPDATE events SET project_path = '/work/worklog' WHERE id = ?1",
            params![event_id],
        )
        .unwrap();

        let (folder_for_repo, sha_is_local) = resolver_never_local();
        run_with(&conn, None, folder_for_repo, sha_is_local).unwrap();

        let project_path: Option<String> = conn
            .query_row(
                "SELECT project_path FROM events WHERE id = ?1",
                params![event_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            project_path, None,
            "a row that flips to not-local must have its project_path cleared"
        );
    }

    #[test]
    fn runs_once_via_migrate() {
        let conn = db::open_memory().unwrap();
        conn.pragma_update(None, "user_version", 14).unwrap();
        crate::secrets::set("github_user", "TomasPalsson").unwrap();
        insert_event(
            &conn,
            "github_commit",
            "first-personal-sha",
            "TomasPalsson/dotfiles",
            "2026-04-18T09:00:00+00:00",
        );

        db::migrate(&conn).unwrap();
        assert_eq!(db::current_version(&conn).unwrap(), db::SCHEMA_VERSION);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "the first migrate must run the upgrade once");

        // A second migrate() at the now-current version must not re-run
        // the upgrade: a personal row inserted afterwards survives it.
        insert_event(
            &conn,
            "github_commit",
            "second-personal-sha",
            "TomasPalsson/dotfiles",
            "2026-04-20T09:00:00+00:00",
        );
        db::migrate(&conn).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            count, 1,
            "a second migrate() must not re-run the personal-row deletion"
        );
        let _ = crate::secrets::delete("github_user");
    }
}
