//! Tenant roots, discovery, and links for multi-tenant infra folders (spec 005).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Result;
use rusqlite::{params, Connection};

use crate::billing_registry::Registry;
use crate::tenant_contract::{Tenant, TenantLink, TenantOrigin, TenantRoot};

pub fn list_roots(conn: &Connection) -> Result<Vec<TenantRoot>> {
    let mut stmt = conn.prepare(
        "SELECT folder, root FROM billing_tenant_roots ORDER BY folder COLLATE NOCASE, root",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(TenantRoot {
                folder: r.get(0)?,
                root: r.get(1)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// The directories whose immediate children are tenants for one root,
/// expanding a single `*` segment to every directory it matches
/// (`tenant_contract::TenantRoot`).
fn root_dirs(base: &Path, root: &str) -> Vec<PathBuf> {
    let mut dirs = vec![base.to_path_buf()];
    for segment in root.split('/') {
        let mut next = Vec::new();
        for dir in &dirs {
            if segment == "*" {
                let Ok(entries) = std::fs::read_dir(dir) else {
                    continue;
                };
                next.extend(
                    entries
                        .flatten()
                        .map(|e| e.path())
                        .filter(|p| p.is_dir()),
                );
            } else {
                let candidate = dir.join(segment);
                if candidate.is_dir() {
                    next.push(candidate);
                }
            }
        }
        dirs = next;
    }
    dirs
}

/// Tenant directory names found under one folder's tenant root. A missing
/// root or folder on disk yields zero tenants, never an error.
fn tenant_names(folder_base: &Path, root: &str) -> Vec<String> {
    let mut names: Vec<String> = root_dirs(folder_base, root)
        .into_iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    names.dedup();
    names
}

fn tenant_links(conn: &Connection) -> Result<HashMap<(String, String), TenantLink>> {
    let mut stmt =
        conn.prepare("SELECT folder, tenant, customer, ignored FROM billing_tenant_links")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(TenantLink {
                folder: r.get(0)?,
                tenant: r.get(1)?,
                customer: r.get(2)?,
                ignored: r.get::<_, i64>(3)? != 0,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .map(|l| ((l.folder.clone(), l.tenant.clone()), l))
        .collect())
}

/// A link beats an alias; Ignored beats both.
fn resolve_origin(
    links: &HashMap<(String, String), TenantLink>,
    registry: &Registry,
    folder: &str,
    name: &str,
) -> (Option<String>, TenantOrigin) {
    if let Some(link) = links.get(&(folder.to_owned(), name.to_owned())) {
        if link.ignored {
            return (None, TenantOrigin::Ignored);
        }
        if let Some(customer) = &link.customer {
            return (Some(customer.clone()), TenantOrigin::Link);
        }
    }
    match registry.customer_in_text(name) {
        Some(customer) => (Some(customer), TenantOrigin::Alias),
        None => (None, TenantOrigin::Unmatched),
    }
}

pub fn list_tenants(conn: &Connection) -> Result<Vec<Tenant>> {
    match crate::billing::work_prefix() {
        Some(prefix) => list_tenants_under(conn, Path::new(prefix)),
        None => Ok(Vec::new()),
    }
}

fn list_tenants_under(conn: &Connection, base: &Path) -> Result<Vec<Tenant>> {
    let roots = list_roots(conn)?;
    let registry = Registry::load(conn)?;
    let links = tenant_links(conn)?;

    let mut seen = HashSet::new();
    let mut tenants = Vec::new();
    for root in &roots {
        let folder_base = base.join(&root.folder);
        for name in tenant_names(&folder_base, &root.root) {
            if !seen.insert((root.folder.clone(), name.clone())) {
                continue;
            }
            let (customer, origin) = resolve_origin(&links, &registry, &root.folder, &name);
            tenants.push(Tenant {
                folder: root.folder.clone(),
                name,
                customer,
                origin,
            });
        }
    }
    tenants.sort_by(|a, b| {
        (a.folder.as_str(), a.name.as_str()).cmp(&(b.folder.as_str(), b.name.as_str()))
    });
    Ok(tenants)
}

fn customer_map_from(tenants: Vec<Tenant>) -> HashMap<(String, String), String> {
    let mut map = HashMap::new();
    for t in tenants {
        match t.origin {
            TenantOrigin::Alias | TenantOrigin::Link => {
                if let Some(customer) = t.customer {
                    map.insert((t.folder, t.name), customer);
                }
            }
            TenantOrigin::Unmatched | TenantOrigin::Ignored => {}
        }
    }
    map
}

pub fn tenant_customer_map(conn: &Connection) -> Result<HashMap<(String, String), String>> {
    Ok(customer_map_from(list_tenants(conn)?))
}

/// `customer: None` + `ignored: false` removes the link (back to alias
/// matching).
pub fn link_tenant(conn: &Connection, link: &TenantLink) -> Result<()> {
    let multi_tenant: i64 = conn.query_row(
        "SELECT COUNT(*) FROM billing_folder_map WHERE folder = ?1 AND multi_tenant = 1",
        params![link.folder],
        |r| r.get(0),
    )?;
    if multi_tenant == 0 {
        anyhow::bail!("Folder is not multi-tenant");
    }
    if let Some(customer) = &link.customer {
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM billing_customers WHERE name = ?1",
            params![customer],
            |r| r.get(0),
        )?;
        if exists == 0 {
            anyhow::bail!("Customer no longer exists");
        }
    }
    if link.customer.is_none() && !link.ignored {
        conn.execute(
            "DELETE FROM billing_tenant_links WHERE folder = ?1 AND tenant = ?2",
            params![link.folder, link.tenant],
        )?;
        return Ok(());
    }
    conn.execute(
        "INSERT INTO billing_tenant_links (folder, tenant, customer, ignored)
              VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(folder, tenant) DO UPDATE SET
              customer = excluded.customer,
              ignored = excluded.ignored",
        params![
            link.folder,
            link.tenant,
            link.customer,
            i64::from(link.ignored)
        ],
    )?;
    Ok(())
}

// Tests live in tenants_test.rs (same module, split file for line budget).
#[cfg(test)]
#[path = "tenants_test.rs"]
mod tests;
