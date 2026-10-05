use super::*;
use crate::db::open_memory;
use crate::estimate::FixedInvoker;
use rusqlite::params;
use std::sync::Mutex;

struct Scripted(Mutex<Vec<&'static str>>, Mutex<Vec<String>>);

impl ModelInvoker for Scripted {
    fn invoke(
        &self,
        _system: &str,
        user: &str,
        _schema: &serde_json::Value,
        _model: &str,
    ) -> anyhow::Result<serde_json::Value> {
        self.1.lock().unwrap().push(user.to_string());
        let text = self.0.lock().unwrap().remove(0);
        Ok(serde_json::json!({ "text": text }))
    }
}

#[test]
fn valid_reply_is_returned_after_one_call() {
    let inv = FixedInvoker(
        serde_json::json!({"text": "Lagaði villu í uppsetningu. Prófaði breytinguna."}),
    );
    assert_eq!(
        write("{}", &inv, "m").unwrap(),
        "Lagaði villu í uppsetningu. Prófaði breytinguna."
    );
}

#[test]
fn rejected_reply_is_retried_with_the_reason() {
    let inv = Scripted(
        Mutex::new(vec![
            "Lagaði src/main.rs. Prófaði það.",
            "Lagaði villu. Prófaði það.",
        ]),
        Mutex::new(Vec::new()),
    );
    assert_eq!(
        write("{}", &inv, "m").unwrap(),
        "Lagaði villu. Prófaði það."
    );
    let seen = inv.1.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert!(seen[1].contains("Síðasta svar var hafnað:"), "{}", seen[1]);
}

#[test]
fn three_bad_replies_fail_with_the_attempt_count() {
    let inv = Scripted(
        Mutex::new(vec!["a 1", "b 2", "c 3"]),
        Mutex::new(Vec::new()),
    );
    let err = write("{}", &inv, "m").unwrap_err();
    assert!(err.ends_with("(reynt 3 sinnum)"), "{err}");
    assert_eq!(inv.1.lock().unwrap().len(), 3);
}

#[test]
fn prepare_carries_the_ticket_block_descriptions_and_errors_without_blocks() {
    let conn = open_memory().unwrap();
    let key = TempoLineKey {
        day: "2026-09-30".into(),
        jira_issue: "APRO-1".into(),
    };
    assert!(prepare(&conn, &key).is_err());
    conn.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds, description)
         VALUES ('2026-09-30', 'APRO-1', '2026-09-30T09:00:00+00:00', '2026-09-30T10:00:00+00:00', 3600, 'Alpha work')",
        params![],
    )
    .unwrap();
    assert!(prepare(&conn, &key).unwrap().contains("Alpha work"));
}

#[test]
fn ticket_title_and_description_are_added_to_the_input() {
    let msg = add_ticket(
        r#"{"day":"2026-10-05"}"#,
        "Fix login",
        "Users with jo@x.is can't log in.",
    );
    let v: serde_json::Value = serde_json::from_str(&msg).unwrap();
    assert_eq!(v["day"], "2026-10-05");
    assert_eq!(v["ticket_summary"], "Fix login");
    let desc = v["ticket_description"].as_str().unwrap();
    assert!(
        desc.starts_with("Users with ") && !desc.contains("jo@x.is"),
        "{desc}"
    );
}

#[test]
fn empty_ticket_fields_are_left_out_and_long_descriptions_capped() {
    let msg = add_ticket("{}", "", &"a".repeat(5000));
    let v: serde_json::Value = serde_json::from_str(&msg).unwrap();
    assert!(v.get("ticket_summary").is_none());
    assert_eq!(
        v["ticket_description"].as_str().unwrap().len(),
        MAX_TICKET_DESCRIPTION_CHARS
    );
}
