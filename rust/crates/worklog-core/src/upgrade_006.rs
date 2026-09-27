//! One-off upgrade for spec 006: deletes stored personal-owner commit/PR
//! events (FR-02) and re-infers every stored day once under the new
//! attribution rules, carrying `exported_at`, Tempo ids, manual
//! descriptions and tickets (FR-10, D-09).

use anyhow::Result;
use rusqlite::Connection;

/// Runs the phase-A upgrade against `conn`. `personal_user` is the
/// configured GitHub login (`None` when unset), used to delete previously
/// stored rows from the owner's personal account (D-06).
pub fn run(_conn: &Connection, _personal_user: Option<&str>) -> Result<()> {
    unimplemented!("T005 RED: upgrade_006::run")
}

/// Test seam: `folder_for_repo`/`sha_is_local` are injected so tests don't
/// depend on `~/Desktop/Work`. `run` wires in the real `local_clone` fns.
fn run_with(
    _conn: &Connection,
    _personal_user: Option<&str>,
    _folder_for_repo: impl Fn(&str) -> Option<String>,
    _sha_is_local: impl Fn(&str, &str) -> bool,
) -> Result<()> {
    unimplemented!("T005 RED: upgrade_006::run_with")
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
        repo::upsert_event(conn, &ev).unwrap()
    }

    fn resolver_always_local(folder: &'static str) -> (impl Fn(&str) -> Option<String>, impl Fn(&str, &str) -> bool)
    {
        (move |_repo: &str| Some(folder.to_owned()), |_f: &str, _sha: &str| true)
    }

    fn resolver_never_local() -> (impl Fn(&str) -> Option<String>, impl Fn(&str, &str) -> bool) {
        (|_repo: &str| None, |_f: &str, _sha: &str| false)
    }

    #[test]
    fn carries_all_fields() {
        // Fixture: a block with tempo_worklog_id, exported_at,
        // estimated_by='manual' + description, and a manual jira_issue,
        // linked to one org commit event. After run(), a rebuilt block
        // for the same day must still carry all four.
        let conn = db::open_memory().unwrap();
        let event_id = insert_event(
            &conn,
            "github_commit",
            "sha1",
            "aproorg/worklog",
            "2026-04-18T09:00:00+00:00",
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

        let (jira_issue, description, estimated_by, tempo_worklog_id, exported_at): (
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        ) = conn
            .query_row(
                "SELECT jira_issue, description, estimated_by, tempo_worklog_id, exported_at
                   FROM blocks WHERE day = '2026-04-18'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!(jira_issue.as_deref(), Some("ABC-1"));
        assert_eq!(description.as_deref(), Some("manual write-up"));
        assert_eq!(estimated_by.as_deref(), Some("manual"));
        assert_eq!(tempo_worklog_id.as_deref(), Some("999"));
        assert_eq!(
            exported_at.as_deref(),
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
