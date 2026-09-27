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
    /// The folder holds many customers' tenants (spec 005) — the export
    /// splits each block between customers instead of billing this pin.
    #[serde(default)]
    pub multi_tenant: bool,
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
pub(crate) fn alias_matches(haystack: &str, alias: &str) -> bool {
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

    /// Find the customer whose name or one of its aliases equals `name`
    /// exactly (case-insensitively) — used by the pin command, where the
    /// model names a customer outright rather than mentioning it in
    /// running text (see [`Self::customer_in_text`]).
    pub fn customer_named(&self, name: &str) -> Option<String> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        let needle = name.to_lowercase();
        self.customers.iter().find_map(|c| {
            let hit = c.name.to_lowercase() == needle
                || c.aliases.iter().any(|a| a.to_lowercase() == needle);
            hit.then(|| c.name.clone())
        })
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
        "SELECT id, folder, customer, verkefni, billable, multi_tenant
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
                multi_tenant: r.get::<_, i64>(5)? != 0,
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
        "INSERT INTO billing_folder_map (folder, customer, verkefni, billable, multi_tenant)
              VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(folder) DO UPDATE SET
              customer = excluded.customer,
              verkefni = excluded.verkefni,
              billable = excluded.billable,
              multi_tenant = excluded.multi_tenant",
        params![
            folder,
            blank_to_none(&f.customer),
            blank_to_none(&f.verkefni),
            i64::from(f.billable),
            i64::from(f.multi_tenant),
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
                multi_tenant: false,
            },
        )?;
    }
    Ok(true)
}

/// Seed the tenant roots for the two known multi-tenant infra folders
/// (`vitinn-infra`, `genai-infra`), so multi-tenant billing works without a
/// setup step. Guarded on `billing_tenant_roots` being empty — like
/// `seed_if_empty`, it never re-adds a root the user deleted. A missing
/// folder row is inserted pinned to the house customer; an existing folder
/// row only gets `multi_tenant` flipped on — its pin is never touched.
pub fn seed_tenant_roots_if_empty(conn: &Connection) -> Result<bool> {
    let roots: i64 = conn.query_row("SELECT COUNT(*) FROM billing_tenant_roots", [], |r| {
        r.get(0)
    })?;
    if roots > 0 {
        return Ok(false);
    }

    const SEED_ROOTS: &[(&str, &str)] = &[
        ("vitinn-infra", "tenants"),
        ("genai-infra", "terraform/workspaces/*"),
    ];
    for (folder, root) in SEED_ROOTS {
        conn.execute(
            "INSERT INTO billing_tenant_roots (folder, root) VALUES (?1, ?2)
             ON CONFLICT(folder, root) DO NOTHING",
            params![folder, root],
        )
        .context("seed billing_tenant_roots")?;

        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM billing_folder_map WHERE folder = ?1",
            params![folder],
            |r| r.get(0),
        )?;
        if exists == 0 {
            upsert_folder(
                conn,
                &FolderMap {
                    id: None,
                    folder: (*folder).to_owned(),
                    customer: Some(crate::tenant_contract::HOUSE_CUSTOMER.to_owned()),
                    verkefni: None,
                    billable: true,
                    multi_tenant: true,
                },
            )?;
        } else {
            conn.execute(
                "UPDATE billing_folder_map SET multi_tenant = 1 WHERE folder = ?1",
                params![folder],
            )
            .context("flip multi_tenant on an existing folder pin")?;
        }
    }
    Ok(true)
}

#[cfg(test)]
#[path = "billing_registry_test.rs"]
mod tests;
