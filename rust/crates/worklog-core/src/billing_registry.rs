//! The billing registry — what turns a work folder into a customer and
//! an accounting key.
//!
//! Lives in SQLite rather than a config file so it is editable entirely
//! from the review UI (Settings → Billing). Two tables:
//!
//! * `billing_customers` — the customers time can be billed to, each
//!   with aliases matched against a block's Jira ticket summary and
//!   description. This is what lets a *shared* folder like `genai-infra`
//!   (infra worked on for many customers) still resolve per line.
//! * `billing_folder_map` — per-work-folder defaults. A folder pinned to
//!   a customer resolves without looking at any text; a folder with
//!   `customer = NULL` is explicitly "shared, resolve from text".
//!
//! Deliberately **not** an LLM: the accounting key (`verkefni`) is only
//! ever filled from an explicit registry pin, never guessed. Anything
//! unresolved comes out as `None` and the user fills it in.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// A customer, with the aliases used to spot it in free text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Customer {
    #[serde(default)]
    pub id: Option<i64>,
    pub name: String,
    /// Alternate spellings matched (case-insensitively, on word
    /// boundaries) against ticket summaries and block descriptions.
    /// The `name` itself is always matched too, so listing it here is
    /// unnecessary.
    #[serde(default)]
    pub aliases: Vec<String>,
}

/// Per-work-folder billing defaults.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FolderMap {
    #[serde(default)]
    pub id: Option<i64>,
    /// Work-folder name — the project root, e.g. `sjukra`. Worktrees and
    /// sub-directories collapse into it (see `billing::work_folder_for_path`).
    pub folder: String,
    /// `None` = shared folder; resolve the customer from text instead.
    #[serde(default)]
    pub customer: Option<String>,
    /// `None` = leave the accounting key blank for the user to pick.
    #[serde(default)]
    pub verkefni: Option<String>,
    /// `false` → Óreikningshæft.
    #[serde(default = "default_billable")]
    pub billable: bool,
}

fn default_billable() -> bool {
    true
}

/// The whole registry, loaded once per export so a day's rows don't
/// re-query per block.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Registry {
    pub customers: Vec<Customer>,
    pub folders: Vec<FolderMap>,
}

/// What the registry could work out for one line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// `None` when undetectable — the user fills it in.
    pub customer: Option<String>,
    /// `None` unless a folder pin supplied it.
    pub verkefni: Option<String>,
    pub billable: bool,
}

impl Default for Resolved {
    fn default() -> Self {
        Self {
            customer: None,
            verkefni: None,
            billable: true,
        }
    }
}

/// Split the stored newline/comma-separated alias blob into trimmed,
/// non-empty aliases.
fn parse_aliases(raw: &str) -> Vec<String> {
    raw.split(['\n', ','])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn join_aliases(aliases: &[String]) -> String {
    aliases
        .iter()
        .map(|a| a.trim())
        .filter(|a| !a.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Case-insensitive, word-boundary-aware substring match.
///
/// Word boundaries matter because real aliases are short: `RU` must not
/// fire on "t**ru**e" or "**RU**N", and `HÍ` must not fire on
/// "**hí**býli". A boundary is the start/end of the haystack or any
/// non-alphanumeric character.
fn alias_matches(haystack: &str, alias: &str) -> bool {
    let alias = alias.trim();
    if alias.is_empty() {
        return false;
    }
    let hay: Vec<char> = haystack.to_lowercase().chars().collect();
    let needle: Vec<char> = alias.to_lowercase().chars().collect();
    if needle.len() > hay.len() {
        return false;
    }
    for start in 0..=(hay.len() - needle.len()) {
        if hay[start..start + needle.len()] != needle[..] {
            continue;
        }
        let before_ok = start == 0 || !hay[start - 1].is_alphanumeric();
        let after_idx = start + needle.len();
        let after_ok = after_idx == hay.len() || !hay[after_idx].is_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

impl Registry {
    /// Load the full registry.
    pub fn load(conn: &Connection) -> Result<Self> {
        Ok(Self {
            customers: list_customers(conn)?,
            folders: list_folders(conn)?,
        })
    }

    fn folder_entry(&self, folder: &str) -> Option<&FolderMap> {
        self.folders.iter().find(|f| f.folder == folder)
    }

    /// Find the single customer whose name or aliases appear in `text`.
    ///
    /// Returns `None` when nothing matches **or** when two different
    /// customers match — an ambiguous line is left blank rather than
    /// billed to a coin-flip.
    pub fn customer_in_text(&self, text: &str) -> Option<String> {
        let mut hits: Vec<&str> = Vec::new();
        for c in &self.customers {
            let matched =
                alias_matches(text, &c.name) || c.aliases.iter().any(|a| alias_matches(text, a));
            if matched && !hits.contains(&c.name.as_str()) {
                hits.push(&c.name);
            }
        }
        match hits.as_slice() {
            [only] => Some((*only).to_owned()),
            _ => None,
        }
    }

    /// Resolve one line's billing fields.
    ///
    /// Ladder: a folder pinned to a customer wins outright; otherwise the
    /// customer is matched out of `text` (the block's Jira ticket summary
    /// plus its description); otherwise `None`. `verkefni` only ever
    /// comes from a folder pin.
    pub fn resolve(&self, folder: &str, text: &str) -> Resolved {
        let entry = self.folder_entry(folder);
        let verkefni = entry.and_then(|e| e.verkefni.clone());
        let billable = entry.map(|e| e.billable).unwrap_or(true);

        let customer = match entry.and_then(|e| e.customer.clone()) {
            Some(pinned) => Some(pinned),
            // Shared folder (or no entry at all) → ask the text.
            None => self.customer_in_text(text),
        };

        Resolved {
            customer,
            verkefni,
            billable,
        }
    }
}

// ───────────────────────────── customers ─────────────────────────────

pub fn list_customers(conn: &Connection) -> Result<Vec<Customer>> {
    let mut stmt = conn
        .prepare("SELECT id, name, aliases FROM billing_customers ORDER BY name COLLATE NOCASE")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Customer {
                id: Some(r.get(0)?),
                name: r.get(1)?,
                aliases: parse_aliases(&r.get::<_, String>(2)?),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Insert or update a customer by name. Returns its row id.
pub fn upsert_customer(conn: &Connection, c: &Customer) -> Result<i64> {
    let name = c.name.trim();
    if name.is_empty() {
        anyhow::bail!("customer name must not be empty");
    }
    conn.execute(
        "INSERT INTO billing_customers (name, aliases) VALUES (?1, ?2)
         ON CONFLICT(name) DO UPDATE SET aliases = excluded.aliases",
        params![name, join_aliases(&c.aliases)],
    )
    .context("upsert_customer")?;
    let id = conn.query_row(
        "SELECT id FROM billing_customers WHERE name = ?1",
        params![name],
        |r| r.get(0),
    )?;
    Ok(id)
}

/// Delete a customer by id. Returns whether a row was removed.
pub fn delete_customer(conn: &Connection, id: i64) -> Result<bool> {
    let n = conn
        .execute("DELETE FROM billing_customers WHERE id = ?1", params![id])
        .context("delete_customer")?;
    Ok(n > 0)
}

// ─────────────────────────── folder mappings ───────────────────────────

pub fn list_folders(conn: &Connection) -> Result<Vec<FolderMap>> {
    let mut stmt = conn.prepare(
        "SELECT id, folder, customer, verkefni, billable
           FROM billing_folder_map ORDER BY folder COLLATE NOCASE",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(FolderMap {
                id: Some(r.get(0)?),
                folder: r.get(1)?,
                customer: r.get(2)?,
                verkefni: r.get(3)?,
                billable: r.get::<_, i64>(4)? != 0,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Insert or update a folder mapping by folder name. Empty-string
/// customer/verkefni are normalised to `NULL` so the UI can clear a
/// field by blanking it.
pub fn upsert_folder(conn: &Connection, f: &FolderMap) -> Result<i64> {
    let folder = f.folder.trim();
    if folder.is_empty() {
        anyhow::bail!("folder must not be empty");
    }
    let blank_to_none = |v: &Option<String>| -> Option<String> {
        v.as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    conn.execute(
        "INSERT INTO billing_folder_map (folder, customer, verkefni, billable)
              VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(folder) DO UPDATE SET
              customer = excluded.customer,
              verkefni = excluded.verkefni,
              billable = excluded.billable",
        params![
            folder,
            blank_to_none(&f.customer),
            blank_to_none(&f.verkefni),
            i64::from(f.billable),
        ],
    )
    .context("upsert_folder")?;
    let id = conn.query_row(
        "SELECT id FROM billing_folder_map WHERE folder = ?1",
        params![folder],
        |r| r.get(0),
    )?;
    Ok(id)
}

pub fn delete_folder(conn: &Connection, id: i64) -> Result<bool> {
    let n = conn
        .execute("DELETE FROM billing_folder_map WHERE id = ?1", params![id])
        .context("delete_folder")?;
    Ok(n > 0)
}

/// Work folders seen in the last `days` of events that have no mapping
/// yet — the Settings → Billing "unmapped folders" list, so the user
/// maps real folders by clicking instead of typing names from memory.
///
/// Returns `(folder, event_count)` most-active first.
pub fn unmapped_folders(conn: &Connection, days: i64) -> Result<Vec<(String, i64)>> {
    // GROUP BY in SQL, not in Rust. A busy month is hundreds of thousands of
    // event rows but only a few hundred distinct `project_path` values, and
    // the per-row version timed the daemon out at 10s on a real database —
    // it materialised every row into a String and ran the path
    // normalisation on each one.
    let mut stmt = conn.prepare(
        "SELECT project_path, COUNT(*) FROM events
          WHERE project_path IS NOT NULL
            AND started_at >= date('now', ?1)
          GROUP BY project_path",
    )?;
    let paths = stmt
        .query_map([format!("-{days} days")], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?
        .collect::<std::result::Result<Vec<(String, i64)>, _>>()?;

    let mut counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for (p, n) in paths {
        // Only folders genuinely under the work prefix are billable — the
        // lenient attribution fallback would otherwise offer `~/dotfiles`
        // and `~/Desktop/Projects/*` as things to map.
        if let Some(folder) = crate::billing::billable_work_folder(&p) {
            *counts.entry(folder).or_insert(0) += n;
        }
    }
    for mapped in list_folders(conn)? {
        counts.remove(&mapped.folder);
    }
    let mut out: Vec<(String, i64)> = counts.into_iter().collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Ok(out)
}

/// Seed the registry the first time it is used, so the user starts by
/// correcting rather than typing everything from scratch.
///
/// Only runs when **both** tables are empty — it never overwrites user
/// edits, and never re-adds something the user deleted. Customers are
/// seeded from the names that appear in the user's own Jira summaries;
/// folder pins are seeded only where the mapping is unambiguous from the
/// folder name itself. `genai-infra` is deliberately seeded with a NULL
/// customer to mark it shared (its customer comes from ticket text).
pub fn seed_if_empty(conn: &Connection) -> Result<bool> {
    let customers: i64 =
        conn.query_row("SELECT COUNT(*) FROM billing_customers", [], |r| r.get(0))?;
    let folders: i64 =
        conn.query_row("SELECT COUNT(*) FROM billing_folder_map", [], |r| r.get(0))?;
    if customers > 0 || folders > 0 {
        return Ok(false);
    }

    const SEED_CUSTOMERS: &[(&str, &str)] = &[
        ("APRÓ", "Apró\nApro\nAPRO"),
        ("Sjúkra", "Sjukra\nSjúkratryggingar\nSjukratryggingar"),
        ("MMS", "Miðstöð menntunar og skólaþjónustu\nefnisveita"),
        ("Sensa", ""),
        ("VÍS", "VIS"),
        ("ÍAV", "IAV"),
        ("RL", ""),
        ("RU", "ru.is\nReykjavík University\nReykjavíkurháskóli"),
        ("HÍ", "HI\nHáskóli Íslands"),
        ("Lyfjastofnun", ""),
    ];
    for (name, aliases) in SEED_CUSTOMERS {
        upsert_customer(
            conn,
            &Customer {
                id: None,
                name: (*name).to_owned(),
                aliases: parse_aliases(aliases),
            },
        )?;
    }

    // (folder, customer, verkefni, billable)
    const SEED_FOLDERS: &[(&str, Option<&str>, Option<&str>)] = &[
        (
            "apro-website",
            Some("APRÓ"),
            Some("Vefsíður APRÓ og dótturfélaga"),
        ),
        ("apro-hubspot", Some("APRÓ"), None),
        ("sjukra", Some("Sjúkra"), None),
        ("lyfjastofnun", Some("Lyfjastofnun"), None),
        // Shared infra — serves many customers, so no pin: the customer
        // is resolved from each line's ticket/description text.
        ("genai-infra", None, None),
    ];
    for (folder, customer, verkefni) in SEED_FOLDERS {
        upsert_folder(
            conn,
            &FolderMap {
                id: None,
                folder: (*folder).to_owned(),
                customer: customer.map(str::to_owned),
                verkefni: verkefni.map(str::to_owned),
                billable: true,
            },
        )?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn conn() -> Connection {
        db::open_memory().unwrap()
    }

    #[test]
    fn alias_matching_respects_word_boundaries() {
        assert!(alias_matches("Villur hjá VÍS", "VÍS"));
        assert!(alias_matches("Agent fyrir RL - Atlassian", "RL"));
        // Short aliases must not fire inside longer words.
        assert!(!alias_matches("this is true", "RU"));
        assert!(!alias_matches("RUNNING the agent", "RU"));
        // Boundary characters other than space still count.
        assert!(alias_matches("Spjallbúbbla inn á ru.is", "ru.is"));
        assert!(alias_matches("MMS - lagfæringar", "MMS"));
    }

    #[test]
    fn alias_matching_is_case_insensitive() {
        assert!(alias_matches("document analyzer fyrir sjúkra", "Sjúkra"));
        assert!(alias_matches("Tempo MCP fyrir Apró", "APRÓ"));
    }

    #[test]
    fn customer_in_text_needs_an_unambiguous_hit() {
        let reg = Registry {
            customers: vec![
                Customer {
                    id: None,
                    name: "Sjúkra".into(),
                    aliases: vec![],
                },
                Customer {
                    id: None,
                    name: "MMS".into(),
                    aliases: vec![],
                },
            ],
            folders: vec![],
        };
        assert_eq!(
            reg.customer_in_text("Document analyzer fyrir Sjúkra"),
            Some("Sjúkra".into())
        );
        // Two customers named → ambiguous → left blank, never guessed.
        assert_eq!(reg.customer_in_text("Sjúkra and MMS sync"), None);
        assert_eq!(reg.customer_in_text("no customer here"), None);
    }

    #[test]
    fn resolve_prefers_a_folder_pin_over_text() {
        let reg = Registry {
            customers: vec![Customer {
                id: None,
                name: "MMS".into(),
                aliases: vec![],
            }],
            folders: vec![FolderMap {
                id: None,
                folder: "sjukra".into(),
                customer: Some("Sjúkra".into()),
                verkefni: Some("[P] Vöktun".into()),
                billable: true,
            }],
        };
        // Text mentions MMS but the folder is pinned to Sjúkra.
        let r = reg.resolve("sjukra", "MMS schema sync");
        assert_eq!(r.customer, Some("Sjúkra".into()));
        assert_eq!(r.verkefni, Some("[P] Vöktun".into()));
        assert!(r.billable);
    }

    #[test]
    fn resolve_falls_back_to_text_for_a_shared_folder() {
        let reg = Registry {
            customers: vec![Customer {
                id: None,
                name: "Sensa".into(),
                aliases: vec![],
            }],
            folders: vec![FolderMap {
                id: None,
                folder: "genai-infra".into(),
                customer: None, // shared
                verkefni: None,
                billable: true,
            }],
        };
        let r = reg.resolve("genai-infra", "Sensa - Deploy Jira MCP í Vitinn-umhverfi");
        assert_eq!(r.customer, Some("Sensa".into()));
        // verkefni is never guessed.
        assert_eq!(r.verkefni, None);
    }

    #[test]
    fn resolve_returns_none_when_undetectable() {
        let reg = Registry::default();
        let r = reg.resolve("mystery-folder", "some work");
        assert_eq!(r.customer, None);
        assert_eq!(r.verkefni, None);
        assert!(r.billable, "unmapped folders default to billable");
    }

    #[test]
    fn customer_crud_round_trips() {
        let c = conn();
        let id = upsert_customer(
            &c,
            &Customer {
                id: None,
                name: "Sensa".into(),
                aliases: vec!["sensa.is".into()],
            },
        )
        .unwrap();
        let all = list_customers(&c).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "Sensa");
        assert_eq!(all[0].aliases, vec!["sensa.is".to_string()]);

        // Upsert by name updates rather than duplicating.
        upsert_customer(
            &c,
            &Customer {
                id: None,
                name: "Sensa".into(),
                aliases: vec!["sensa.is".into(), "Sensa hf".into()],
            },
        )
        .unwrap();
        let all = list_customers(&c).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].aliases.len(), 2);

        assert!(delete_customer(&c, id).unwrap());
        assert!(list_customers(&c).unwrap().is_empty());
    }

    #[test]
    fn folder_crud_blanks_normalise_to_null() {
        let c = conn();
        upsert_folder(
            &c,
            &FolderMap {
                id: None,
                folder: "genai-infra".into(),
                customer: Some("   ".into()),
                verkefni: Some("".into()),
                billable: false,
            },
        )
        .unwrap();
        let all = list_folders(&c).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].customer, None, "whitespace-only clears to NULL");
        assert_eq!(all[0].verkefni, None);
        assert!(!all[0].billable);
    }

    #[test]
    fn seed_runs_once_and_never_overwrites() {
        let c = conn();
        assert!(seed_if_empty(&c).unwrap(), "first call seeds");
        let customers = list_customers(&c).unwrap();
        assert!(customers.iter().any(|x| x.name == "Sjúkra"));
        let folders = list_folders(&c).unwrap();
        let website = folders.iter().find(|f| f.folder == "apro-website").unwrap();
        assert_eq!(website.customer, Some("APRÓ".into()));
        assert_eq!(
            website.verkefni,
            Some("Vefsíður APRÓ og dótturfélaga".into())
        );
        // genai-infra is seeded as shared (no pinned customer).
        let shared = folders.iter().find(|f| f.folder == "genai-infra").unwrap();
        assert_eq!(shared.customer, None);

        // A second call is a no-op even after the user edits.
        upsert_folder(
            &c,
            &FolderMap {
                id: None,
                folder: "apro-website".into(),
                customer: Some("Edited".into()),
                verkefni: None,
                billable: true,
            },
        )
        .unwrap();
        assert!(!seed_if_empty(&c).unwrap(), "second call does nothing");
        let folders = list_folders(&c).unwrap();
        let website = folders.iter().find(|f| f.folder == "apro-website").unwrap();
        assert_eq!(
            website.customer,
            Some("Edited".into()),
            "seed must not clobber a user edit"
        );
    }
}
