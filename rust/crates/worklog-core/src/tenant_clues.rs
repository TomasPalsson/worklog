//! Timestamped customer clue extraction for multi-tenant infra folders (spec 005).

use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::Connection;

use crate::billing;
use crate::billing_registry::Registry;
use crate::repo;
use crate::tenant_contract::{Clue, ClueStrength};
use crate::tenants;

/// Timestamped clues for one block's events, scoped to `folder` (an event
/// whose `project_path` resolves to a different work folder is ignored).
///
/// Two kinds of clue:
/// * **Tenant path** — an edited file under `<tenant root>/<tenant>/`; the
///   tenant must already be a known, resolved tenant of `folder` (`tenants`
///   — an `Ignored` tenant gives no clue, per FR-05).
/// * **Branch** — a git branch (`claude_work` details, `git_reflog`
///   `checkout <branch>` titles) or a worktree name
///   (`.claude/worktrees/<name>`); matched against customer names/aliases
///   directly, since branches aren't tracked as tenants.
pub fn clues_for_block(
    conn: &Connection,
    block_id: i64,
    folder: &str,
    tenant_customers: &HashMap<(String, String), String>,
    registry: &Registry,
) -> Result<Vec<Clue>> {
    let roots: Vec<String> = tenants::list_roots(conn)?
        .into_iter()
        .filter(|r| r.folder == folder)
        .map(|r| r.root)
        .collect();

    let mut clues = Vec::new();
    for event in repo::list_events_for_block(conn, block_id)? {
        let event_folder = event
            .project_path
            .as_deref()
            .and_then(billing::work_folder_for_path);
        if event_folder.as_deref() != Some(folder) {
            continue;
        }
        let Ok(at) = DateTime::parse_from_rfc3339(&event.started_at) else {
            continue;
        };
        let at = at.with_timezone(&Utc);

        if let Some(details) = &event.details {
            for path in edited_paths(details) {
                if let Some(tenant) = tenant_segment(&roots, &path) {
                    if let Some(customer) =
                        tenant_customers.get(&(folder.to_owned(), tenant.to_owned()))
                    {
                        clues.push(Clue {
                            at,
                            customer: customer.clone(),
                            strength: ClueStrength::TenantPath,
                        });
                    }
                }
            }
            if let Some(branch) = branch_after(details, "branch ") {
                if let Some(customer) = registry.customer_in_text(branch) {
                    clues.push(Clue {
                        at,
                        customer,
                        strength: ClueStrength::Branch,
                    });
                }
            }
        }
        if let Some(branch) = branch_after(&event.title, "checkout ") {
            if let Some(customer) = registry.customer_in_text(branch) {
                clues.push(Clue {
                    at,
                    customer,
                    strength: ClueStrength::Branch,
                });
            }
        }
        if let Some(worktree) = worktree_name(event.project_path.as_deref()) {
            if let Some(customer) = registry.customer_in_text(worktree) {
                clues.push(Clue {
                    at,
                    customer,
                    strength: ClueStrength::Branch,
                });
            }
        }
    }
    Ok(clues)
}

/// The tenant directory name in `path` when it falls under one of `roots`
/// (`<root>/<tenant>/...`, where a `*` root segment matches any one
/// directory name — see `tenant_contract::TenantRoot`).
fn tenant_segment<'a>(roots: &[String], path: &'a str) -> Option<&'a str> {
    let segments: Vec<&str> = path.split('/').collect();
    for root in roots {
        let root_segments: Vec<&str> = root.split('/').collect();
        if segments.len() <= root_segments.len() {
            continue;
        }
        let matches = root_segments
            .iter()
            .zip(segments.iter())
            .all(|(r, s)| *r == "*" || r == s);
        if matches {
            return Some(segments[root_segments.len()]);
        }
    }
    None
}

/// The value right after `prefix` in `text`, up to the next " · " separator
/// (see `claude_transcripts::WorkMinute::summary` and
/// `reflog::title_for_reflog_message`, the two producers of this shape).
fn branch_after<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(prefix)?;
    let branch = rest.split(" · ").next().unwrap_or(rest).trim();
    (!branch.is_empty()).then_some(branch)
}

/// File paths out of a `claude_work` details string's `"edited a, b, c +2"`
/// segment (see `claude_transcripts::WorkMinute::summary`).
fn edited_paths(details: &str) -> Vec<String> {
    for part in details.split(" · ") {
        let Some(rest) = part.strip_prefix("edited ") else {
            continue;
        };
        let files_part = match rest.rfind(" +") {
            Some(idx) if rest[idx + 2..].chars().all(|c| c.is_ascii_digit()) => &rest[..idx],
            _ => rest,
        };
        return files_part
            .split(", ")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
    }
    Vec::new()
}

/// The worktree name out of `.claude/worktrees/<name>` in `project_path`
/// (FR: worktree names count as a Branch clue).
fn worktree_name(project_path: Option<&str>) -> Option<&str> {
    let path = project_path?;
    let marker = "/.claude/worktrees/";
    let rest = &path[path.find(marker)? + marker.len()..];
    let name = rest.split('/').next().unwrap_or(rest);
    (!name.is_empty()).then_some(name)
}
