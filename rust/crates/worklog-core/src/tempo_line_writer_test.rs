use super::*;
use crate::db::open_memory;
use crate::estimate::FixedInvoker;
use rusqlite::params;
use std::sync::Mutex;

fn key() -> TempoLineKey {
    TempoLineKey {
        day: "2026-10-05".into(),
        jira_issue: "APRO-1".into(),
    }
}

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
        write("{}", &key(), &inv, "m").unwrap(),
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
        write("{}", &key(), &inv, "m").unwrap(),
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
    let err = write("{}", &key(), &inv, "m").unwrap_err();
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

fn block(conn: &Connection, day: &str, issue: &str, personal: i32) {
    conn.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds, is_personal)
         VALUES (?1, ?2, ?1 || 'T09:00:00+00:00', ?1 || 'T10:00:00+00:00', 3600, ?3)",
        params![day, issue, personal],
    )
    .unwrap();
}

#[test]
fn prepare_lists_the_days_other_tickets_with_summaries() {
    let conn = open_memory().unwrap();
    let key = key();
    block(&conn, "2026-10-05", "APRO-1", 0);
    block(&conn, "2026-10-05", "APRO-2", 0);
    block(&conn, "2026-10-05", "APRO-4", 1);
    block(&conn, "2026-10-06", "APRO-5", 0);
    conn.execute(
        "INSERT INTO tempo_line_texts (day, jira_issue, updated_at)
         VALUES ('2026-10-05', 'APRO-3', 'x'), ('2026-10-06', 'APRO-6', 'x')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO jira_tickets (key, summary) VALUES ('APRO-2', 'Second'), ('APRO-1', 'Own')",
        [],
    )
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&prepare(&conn, &key).unwrap()).unwrap();
    assert_eq!(
        v["other_tickets_today"],
        serde_json::json!(["APRO-2: Second", "APRO-3"])
    );
}

#[test]
fn prepare_omits_other_tickets_when_there_are_none() {
    let conn = open_memory().unwrap();
    block(&conn, "2026-10-05", "APRO-1", 0);
    let v: serde_json::Value = serde_json::from_str(&prepare(&conn, &key()).unwrap()).unwrap();
    assert!(v.get("other_tickets_today").is_none());
}

#[test]
fn reask_names_the_new_rules() {
    let inv = Scripted(
        Mutex::new(vec![
            "Mældu biðtíma. Sendi tillögur í Slack.",
            "Mældi biðtíma. Sendi tillögur.",
        ]),
        Mutex::new(Vec::new()),
    );
    write("{}", &key(), &inv, "m").unwrap();
    let seen = inv.1.lock().unwrap();
    for rule in ["vísbendingar", "tölustöfum", "Slack", "eintölu"] {
        assert!(seen[1].contains(rule), "{rule}: {}", seen[1]);
    }
}

#[test]
fn prompt_carries_every_new_rule() {
    for phrase in [
        "Lýstu aldrei vísbendingunum sjálfum",
        "Nefndu aldrei samstarfsfólk með nafni, aldrei Slack",
        "útgáfunúmer með tölustöfum",
        "Lýstu aðeins vinnu fyrir viðskiptavin þessa máls",
    ] {
        assert!(SYSTEM_PROMPT_IS.contains(phrase), "{phrase}");
    }
}

fn customer(conn: &Connection, name: &str, aliases: &[&str]) {
    crate::billing_registry::upsert_customer(
        conn,
        &crate::billing_registry::Customer {
            id: None,
            name: name.into(),
            aliases: aliases.iter().map(|a| a.to_string()).collect(),
        },
    )
    .unwrap();
}

fn prepared(conn: &Connection) -> serde_json::Value {
    serde_json::from_str(&prepare(conn, &key()).unwrap()).unwrap()
}

#[test]
fn prepare_carries_the_ticket_customer_and_every_known_customer() {
    let conn = open_memory().unwrap();
    block(&conn, "2026-10-05", "APRO-1", 0);
    customer(&conn, "Sjúkra", &[]);
    customer(&conn, "VÍS", &["VIS"]);
    conn.execute(
        "INSERT INTO jira_tickets (key, summary) VALUES ('APRO-1', 'Data retention fyrir Sjúkra')",
        [],
    )
    .unwrap();
    let v = prepared(&conn);
    assert_eq!(v["ticket_customer"], "Sjúkra");
    assert_eq!(
        v["known_customers"],
        serde_json::json!([
            {"name": "Sjúkra", "aliases": []},
            {"name": "VÍS", "aliases": ["VIS"]},
        ])
    );
}

#[test]
fn prepare_falls_back_to_the_parent_summary_and_omits_an_ambiguous_customer() {
    let conn = open_memory().unwrap();
    block(&conn, "2026-10-05", "APRO-1", 0);
    customer(&conn, "Sjúkra", &[]);
    customer(&conn, "VÍS", &["VIS"]);
    conn.execute(
        "INSERT INTO jira_tickets (key, summary, parent_summary) VALUES ('APRO-1', 'Lagfæra villu', 'Epic Sjúkra')",
        [],
    )
    .unwrap();
    assert_eq!(prepared(&conn)["ticket_customer"], "Sjúkra");
    conn.execute(
        "UPDATE jira_tickets SET summary = 'Sjúkra og VIS' WHERE key = 'APRO-1'",
        [],
    )
    .unwrap();
    assert_eq!(prepared(&conn)["ticket_customer"], "Sjúkra");
    conn.execute(
        "UPDATE jira_tickets SET parent_summary = NULL WHERE key = 'APRO-1'",
        [],
    )
    .unwrap();
    assert!(prepared(&conn).get("ticket_customer").is_none());
}

const FOREIGN_REPLY: &str =
    "Rannsakaði 503-villur hjá Sjúkra. Reiknaði kostnað út frá raungögnum frá Vis, Byko og Sjúkra.";
const CLEAN_REPLY: &str = "Rannsakaði 503-villur hjá Sjúkra. Reiknaði kostnað út frá raungögnum.";

fn customer_msg() -> String {
    serde_json::json!({
        "ticket_customer": "Sjúkra",
        "known_customers": [
            {"name": "Sjúkra", "aliases": []},
            {"name": "VÍS", "aliases": ["VIS"]},
            {"name": "BYKO", "aliases": []},
        ],
    })
    .to_string()
}

#[test]
fn foreign_customer_reply_is_reasked_with_the_names() {
    let inv = Scripted(
        Mutex::new(vec![FOREIGN_REPLY, CLEAN_REPLY]),
        Mutex::new(Vec::new()),
    );
    assert_eq!(
        write(&customer_msg(), &key(), &inv, "m").unwrap(),
        CLEAN_REPLY
    );
    let seen = inv.1.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert!(
        seen[1].contains("nefnir annan viðskiptavin: BYKO, VÍS"),
        "{}",
        seen[1]
    );
}

#[test]
fn own_customer_reply_passes_first_time() {
    let inv = Scripted(Mutex::new(vec![CLEAN_REPLY]), Mutex::new(Vec::new()));
    write(&customer_msg(), &key(), &inv, "m").unwrap();
    assert_eq!(inv.1.lock().unwrap().len(), 1);
}

#[test]
fn customer_rule_is_skipped_without_usable_customer_data() {
    for msg in [
        "{}".to_string(),
        r#"{"ticket_customer":"Sjúkra","known_customers":"oops"}"#.to_string(),
        "not json".to_string(),
    ] {
        let inv = Scripted(Mutex::new(vec![FOREIGN_REPLY]), Mutex::new(Vec::new()));
        assert_eq!(
            write(&msg, &key(), &inv, "m").unwrap(),
            FOREIGN_REPLY,
            "{msg}"
        );
    }
}
