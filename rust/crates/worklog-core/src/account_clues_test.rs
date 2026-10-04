use super::*;
use crate::db;

fn conn() -> Connection {
    db::open_memory().unwrap()
}

fn acct(id: &str, name: &str) -> AllowedAccount {
    AllowedAccount {
        id: id.into(),
        name: name.into(),
    }
}

fn seed(c: &Connection, id: &str, clue: &str, hits: i64, wrong: i64) {
    c.execute(
        "INSERT INTO account_clues (account_id, account_name, clue, hits, wrong)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, format!("acct {id}"), clue, hits, wrong],
    )
    .unwrap();
}

fn seed_count(c: &Connection, id: &str, tickets: i64) {
    c.execute(
        "INSERT INTO account_ticket_counts (account_id, account_name, tickets)
         VALUES (?1, ?2, ?3)",
        rusqlite::params![id, format!("acct {id}"), tickets],
    )
    .unwrap();
}

#[test]
fn relearn_rebuilds() {
    let c = conn();
    seed(&c, "9", "stale", 4, 0);
    let a = acct("1", "Innnes");
    let b = acct("2", "VIS");
    let tickets = vec![
        ("Innnes - SSO login with Okta".to_string(), a.clone()),
        ("Innnes - billing export".to_string(), a.clone()),
        ("Villur hjá VÍS".to_string(), b),
    ];
    let report = relearn(&c, &tickets).unwrap();
    assert_eq!(
        report,
        RelearnReport {
            tickets_read: 3,
            accounts: 2,
            clues: 6
        }
    );
    let hits = |id: &str, clue: &str| -> Option<i64> {
        c.query_row(
            "SELECT hits FROM account_clues WHERE account_id=?1 AND clue=?2",
            [id, clue],
            |r| r.get(0),
        )
        .ok()
    };
    assert_eq!(hits("1", "innnes"), Some(2));
    assert_eq!(hits("1", "okta"), Some(1));
    assert_eq!(hits("1", "with"), None);
    assert_eq!(hits("2", "villur"), Some(1));
    assert_eq!(hits("9", "stale"), None);
    let tickets_on_1: i64 = c
        .query_row(
            "SELECT tickets FROM account_ticket_counts WHERE account_id='1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tickets_on_1, 2);
}

#[test]
fn suggest_ranks() {
    let c = conn();
    seed(&c, "1", "innnes", 3, 0);
    seed(&c, "1", "sso", 1, 0);
    seed(&c, "2", "innnes", 1, 0);
    seed(&c, "3", "vis", 5, 0);
    seed(&c, "4", "innnes", 100, 0);
    seed_count(&c, "1", 7);
    let allowed = vec![acct("1", "A"), acct("2", "B"), acct("3", "C")];
    let got = suggest(&c, "Innnes SSO fix", &allowed).unwrap();
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].account.id, "1");
    assert_eq!(got[0].matched_clues, vec!["innnes", "sso"]);
    assert_eq!(got[0].past_tickets, 7);
    assert_eq!(got[0].score, 4.0);
    assert_eq!(got[1].account.id, "2");
    assert_eq!(got[1].past_tickets, 0);
    assert_eq!(got[1].score, 1.0);
}

#[test]
fn suggest_returns_at_most_three() {
    let c = conn();
    let mut allowed = Vec::new();
    for i in 1..=5 {
        seed(&c, &i.to_string(), "innnes", i, 0);
        allowed.push(acct(&i.to_string(), "x"));
    }
    let got = suggest(&c, "innnes", &allowed).unwrap();
    let ids: Vec<&str> = got.iter().map(|s| s.account.id.as_str()).collect();
    assert_eq!(ids, vec!["5", "4", "3"]);
}

#[test]
fn wrong_twice_drops() {
    let c = conn();
    seed(&c, "1", "innnes", 3, 0);
    let (a, b) = (acct("1", "A"), acct("2", "B"));
    let allowed = vec![a, b.clone()];
    let clues = vec!["innnes".to_string()];

    record_decision(&c, "Innnes thing", &b, Some("1"), &clues).unwrap();
    let after_one = suggest(&c, "innnes", &allowed).unwrap();
    assert_eq!(after_one[0].account.id, "1");

    record_decision(&c, "Innnes again", &b, Some("1"), &clues).unwrap();
    let after_two = suggest(&c, "innnes", &allowed).unwrap();
    assert_eq!(after_two.len(), 1);
    assert_eq!(after_two[0].account.id, "2");

    let rows: Vec<(String, String, i64, String)> = c
        .prepare("SELECT picked_id, guessed_id, correct, clues FROM account_decisions ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0],
        ("2".into(), "1".into(), 0, r#"["innnes"]"#.to_string())
    );
}

#[test]
fn decision_marks_correct_guess_and_counts_ticket() {
    let c = conn();
    let a = acct("1", "A");
    record_decision(&c, "Innnes", &a, Some("1"), &["innnes".to_string()]).unwrap();
    record_decision(&c, "Named", &a, None, &[]).unwrap();
    let correct: Vec<i64> = c
        .prepare("SELECT correct FROM account_decisions ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(correct, vec![1, 0]);
    let tickets: i64 = c
        .query_row(
            "SELECT tickets FROM account_ticket_counts WHERE account_id='1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tickets, 2);
    let hits: i64 = c
        .query_row(
            "SELECT hits FROM account_clues WHERE account_id='1' AND clue='innnes'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(hits, 1);
}

#[test]
fn mixed_case_clues_count_against_lowercase_rows() {
    let c = conn();
    seed(&c, "1", "innnes", 3, 0);
    let (a, b) = (acct("1", "A"), acct("2", "B"));
    let allowed = vec![a, b.clone()];
    let clues = vec!["  Innnes ".to_string()];
    record_decision(&c, "Innnes thing", &b, Some("1"), &clues).unwrap();
    record_decision(&c, "Innnes again", &b, Some("1"), &clues).unwrap();
    let got = suggest(&c, "innnes", &allowed).unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].account.id, "2");
    assert_eq!(got[0].matched_clues, vec!["innnes"]);
}

#[test]
fn picking_a_dropped_clue_revives_it() {
    let c = conn();
    seed(&c, "1", "acme", 3, 2);
    let a = acct("1", "A");
    record_decision(&c, "Acme job", &a, Some("1"), &["acme".to_string()]).unwrap();
    let got = suggest(&c, "acme", &[a]).unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].account.id, "1");
}

#[test]
fn relearn_keeps_recorded_decisions() {
    let c = conn();
    let x = acct("7", "X");
    seed_count(&c, "7", 1);
    record_decision(&c, "Zebra rollout", &x, None, &["zebra".to_string()]).unwrap();
    let report = relearn(&c, &[]).unwrap();
    assert_eq!(report.accounts, 1);
    let got = suggest(&c, "zebra", &[x]).unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].past_tickets, 1);
}

#[test]
fn suggest_surfaces_db_errors() {
    let c = conn();
    seed(&c, "1", "acme", 3, 0);
    c.execute("DROP TABLE account_ticket_counts", []).unwrap();
    assert!(suggest(&c, "acme", &[acct("1", "A")]).is_err());
}
