//! Session Jira tickets — "session S is on ticket K". Stored in
//! `session_tickets`; set by `worklog ticket use`, read by the hook
//! (`hook_run::handle`) and the SessionStart hint.

use anyhow::{bail, Result};
use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension};

/// Record (or replace) the ticket a session is on.
pub fn set(conn: &Connection, session_id: &str, key: &str) -> Result<()> {
    let full = Regex::new(&format!("^{}$", crate::hook_run::jira_re().as_str()))?;
    if !full.is_match(key) {
        bail!("invalid Jira key {key:?}: expected e.g. GENAI-123");
    }
    if session_id.is_empty() {
        bail!("session id must not be empty");
    }
    conn.execute(
        "INSERT INTO session_tickets (session_id, jira_issue) VALUES (?1, ?2)
         ON CONFLICT(session_id) DO UPDATE SET
             jira_issue = excluded.jira_issue,
             set_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        params![session_id, key],
    )?;
    Ok(())
}

/// The ticket recorded for a session, if any.
pub fn get(conn: &Connection, session_id: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT jira_issue FROM session_tickets WHERE session_id = ?1",
            params![session_id],
            |r| r.get(0),
        )
        .optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;

    #[test]
    fn set_then_get() {
        let c = open_memory().unwrap();
        set(&c, "s1", "GENAI-9").unwrap();
        assert_eq!(get(&c, "s1").unwrap().as_deref(), Some("GENAI-9"));
    }

    #[test]
    fn second_set_replaces() {
        let c = open_memory().unwrap();
        set(&c, "s1", "GENAI-9").unwrap();
        set(&c, "s1", "PROJ-2").unwrap();
        assert_eq!(get(&c, "s1").unwrap().as_deref(), Some("PROJ-2"));
    }

    #[test]
    fn missing_is_none() {
        let c = open_memory().unwrap();
        assert_eq!(get(&c, "nope").unwrap(), None);
    }

    #[test]
    fn bad_keys_rejected() {
        let c = open_memory().unwrap();
        for k in ["genai-9", "GENAI", "GENAI-9 x", "x GENAI-9", "", "G-1"] {
            assert!(set(&c, "s1", k).is_err(), "{k}");
        }
        assert!(set(&c, "", "GENAI-9").is_err());
        assert_eq!(get(&c, "s1").unwrap(), None);
    }
}
