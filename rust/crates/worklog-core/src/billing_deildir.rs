//! The deildir registry — a per-customer list of deildir (Verkefni names)
//! with keywords, plus keyword matching against a block's ticket summary
//! and description (spec 006). Types live in `deild_contract`; see
//! `deild_contract::Deild`. Populated by T002: `list_deildir`,
//! `upsert_deild`, `delete_deild`, `deild_in_text`.

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use crate::deild_contract::Deild;

/// Split the stored newline/comma-separated keyword blob into trimmed,
/// non-empty keywords. Mirrors `billing_registry::parse_aliases` — deild
/// keywords are matched the same way as customer aliases (FR-03).
fn parse_keywords(raw: &str) -> Vec<String> {
    raw.split(['\n', ','])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn join_keywords(keywords: &[String]) -> String {
    keywords
        .iter()
        .map(|k| k.trim())
        .filter(|k| !k.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn list_deildir(conn: &Connection) -> Result<Vec<Deild>> {
    let mut stmt = conn.prepare(
        "SELECT id, customer, name, keywords FROM billing_deildir
          ORDER BY customer COLLATE NOCASE, name COLLATE NOCASE",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Deild {
                id: Some(r.get(0)?),
                customer: r.get(1)?,
                name: r.get(2)?,
                keywords: parse_keywords(&r.get::<_, String>(3)?),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Insert a new deild (`d.id` is `None`) or update one in place by id.
/// Bails when the name is empty, or when the resulting (customer, name)
/// would collide with a *different* row (FR-02: unique per customer).
pub fn upsert_deild(conn: &Connection, d: &Deild) -> Result<i64> {
    let customer = d.customer.trim();
    let name = d.name.trim();
    if name.is_empty() {
        anyhow::bail!("deild name must not be empty");
    }
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM billing_deildir WHERE customer = ?1 AND name = ?2",
            params![customer, name],
            |r| r.get(0),
        )
        .optional()
        .context("checking for a duplicate deild")?;
    if let Some(existing_id) = existing {
        if d.id != Some(existing_id) {
            anyhow::bail!("deild '{name}' already exists for customer '{customer}'");
        }
    }
    let keywords = join_keywords(&d.keywords);
    match d.id {
        Some(id) => {
            conn.execute(
                "UPDATE billing_deildir SET customer = ?1, name = ?2, keywords = ?3 WHERE id = ?4",
                params![customer, name, keywords, id],
            )
            .context("update deild")?;
            Ok(id)
        }
        None => {
            conn.execute(
                "INSERT INTO billing_deildir (customer, name, keywords) VALUES (?1, ?2, ?3)",
                params![customer, name, keywords],
            )
            .context("insert deild")?;
            Ok(conn.last_insert_rowid())
        }
    }
}

pub fn delete_deild(conn: &Connection, id: i64) -> Result<bool> {
    let n = conn
        .execute("DELETE FROM billing_deildir WHERE id = ?1", params![id])
        .context("delete_deild")?;
    Ok(n > 0)
}

/// Find the single deild of `customer` whose keywords appear in `text`.
/// `None` when no deild of that customer matches, or when more than one
/// does — an ambiguous line is left for the Owner rather than guessed
/// (mirrors `billing_registry::Registry::customer_in_text`).
pub fn deild_in_text(deildir: &[Deild], customer: &str, text: &str) -> Option<String> {
    let mut hits: Vec<&str> = Vec::new();
    for d in deildir.iter().filter(|d| d.customer == customer) {
        let matched = d
            .keywords
            .iter()
            .any(|k| crate::billing_registry::alias_matches(text, k));
        if matched && !hits.contains(&d.name.as_str()) {
            hits.push(&d.name);
        }
    }
    match hits.as_slice() {
        [only] => Some((*only).to_owned()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::deild_contract::Deild;

    fn conn() -> rusqlite::Connection {
        db::open_memory().unwrap()
    }

    #[test]
    fn deild_crud_round_trips() {
        let c = conn();
        let id = upsert_deild(
            &c,
            &Deild {
                id: None,
                customer: "Sjúkra".into(),
                name: "Rekstur".into(),
                keywords: vec!["ops".into()],
            },
        )
        .unwrap();
        let all = list_deildir(&c).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].customer, "Sjúkra");
        assert_eq!(all[0].name, "Rekstur");
        assert_eq!(all[0].keywords, vec!["ops".to_string()]);

        // Editing by id updates in place rather than duplicating.
        upsert_deild(
            &c,
            &Deild {
                id: Some(id),
                customer: "Sjúkra".into(),
                name: "Rekstur".into(),
                keywords: vec!["ops".into(), "vöktun".into()],
            },
        )
        .unwrap();
        let all = list_deildir(&c).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].keywords.len(), 2);

        assert!(delete_deild(&c, id).unwrap());
        assert!(list_deildir(&c).unwrap().is_empty());
    }

    #[test]
    fn upsert_deild_rejects_empty_name() {
        let c = conn();
        let err = upsert_deild(
            &c,
            &Deild {
                id: None,
                customer: "Sjúkra".into(),
                name: "   ".into(),
                keywords: vec![],
            },
        )
        .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("name"));
    }

    #[test]
    fn upsert_deild_rejects_duplicate_customer_name() {
        let c = conn();
        upsert_deild(
            &c,
            &Deild {
                id: None,
                customer: "Sjúkra".into(),
                name: "Rekstur".into(),
                keywords: vec![],
            },
        )
        .unwrap();
        let err = upsert_deild(
            &c,
            &Deild {
                id: None,
                customer: "Sjúkra".into(),
                name: "Rekstur".into(),
                keywords: vec!["ops".into()],
            },
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("Sjúkra"), "message must name the customer");
        assert!(err.contains("Rekstur"), "message must name the deild");
    }

    #[test]
    fn deild_in_text_needs_an_unambiguous_hit() {
        let deildir = vec![
            Deild {
                id: Some(1),
                customer: "Sjúkra".into(),
                name: "Rekstur".into(),
                keywords: vec!["ops".into()],
            },
            Deild {
                id: Some(2),
                customer: "Sjúkra".into(),
                name: "Vöktun".into(),
                keywords: vec!["monitoring".into()],
            },
            Deild {
                id: Some(3),
                customer: "MMS".into(),
                name: "Ops".into(),
                keywords: vec!["ops".into()],
            },
        ];
        assert_eq!(
            deild_in_text(&deildir, "Sjúkra", "ops ticket for sjúkra"),
            Some("Rekstur".into())
        );
        // Other customers' deildir never count, even on the same keyword.
        assert_eq!(
            deild_in_text(&deildir, "MMS", "ops ticket for sjúkra"),
            Some("Ops".into())
        );
        assert_eq!(
            deild_in_text(&deildir, "Sjúkra", "nothing relevant here"),
            None
        );
    }

    #[test]
    fn deild_in_text_is_ambiguous_when_two_deildir_of_the_customer_match() {
        let deildir = vec![
            Deild {
                id: Some(1),
                customer: "Sjúkra".into(),
                name: "Rekstur".into(),
                keywords: vec!["ops".into()],
            },
            Deild {
                id: Some(2),
                customer: "Sjúkra".into(),
                name: "Vöktun".into(),
                keywords: vec!["ops".into()],
            },
        ];
        assert_eq!(
            deild_in_text(&deildir, "Sjúkra", "ops ticket"),
            None,
            "two matching deildir under the same customer is ambiguous"
        );
    }
}
