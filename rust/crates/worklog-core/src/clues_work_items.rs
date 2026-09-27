//! Groups a billing line's blocks into per-task [`WorkItem`]s so the
//! line-text model can tell which clue belongs to which piece of work,
//! instead of one flat bag every block's clues get glued into.
//!
//! Grouping is BY TASK, never by time: two blocks land in the same item
//! when they share a Jira key, else a branch, else the same description
//! (case-insensitive). Everything else is its own item.

use crate::clues_contract::WorkItem;

/// One block's already-scrubbed/deduped/capped clues — the only input
/// [`group`] ever sees. Built by `clues_send::build_line_input` from a
/// block's own [`crate::clues_contract::DescriptionInput`].
pub struct BlockClue {
    pub minutes: i64,
    pub ticket: Option<String>,
    pub branches: Vec<String>,
    pub change_titles: Vec<String>,
    pub file_basenames: Vec<String>,
    pub description: Option<String>,
}

/// A merged-list field is capped here too — a group folding in many
/// blocks could otherwise exceed the per-block cap. Mirrors
/// `clues_send::MAX_LIST_ENTRIES`.
const MAX_MERGED_ENTRIES: usize = 30;

/// Groups `blocks` into work items, ordered by minutes descending.
pub fn group(blocks: Vec<BlockClue>) -> Vec<WorkItem> {
    let mut groups: Vec<BlockClue> = Vec::new();
    for block in blocks {
        match groups.iter().position(|g| same_task(g, &block)) {
            Some(idx) => absorb(&mut groups[idx], block),
            None => groups.push(block),
        }
    }
    let mut items: Vec<WorkItem> = groups.into_iter().map(finish).collect();
    items.sort_by_key(|i| std::cmp::Reverse(i.minutes));
    items
}

/// Same task iff: shared non-empty Jira key, else a shared branch, else
/// the same (case-insensitive) description.
fn same_task(g: &BlockClue, b: &BlockClue) -> bool {
    if let (Some(a), Some(bk)) = (&g.ticket, &b.ticket) {
        return a == bk;
    }
    if !g.branches.is_empty() && b.branches.iter().any(|x| g.branches.contains(x)) {
        return true;
    }
    if let (Some(a), Some(bk)) = (&g.description, &b.description) {
        return a.to_lowercase() == bk.to_lowercase();
    }
    false
}

fn absorb(g: &mut BlockClue, b: BlockClue) {
    g.minutes += b.minutes;
    g.ticket = g.ticket.take().or(b.ticket);
    merge_capped(&mut g.branches, b.branches);
    merge_capped(&mut g.change_titles, b.change_titles);
    merge_capped(&mut g.file_basenames, b.file_basenames);
    g.description = g.description.take().or(b.description);
}

fn merge_capped(into: &mut Vec<String>, from: Vec<String>) {
    for item in from {
        if into.len() == MAX_MERGED_ENTRIES {
            break;
        }
        if !into.contains(&item) {
            into.push(item);
        }
    }
}

fn finish(g: BlockClue) -> WorkItem {
    let title = title_for(
        &g.ticket,
        &g.branches,
        &g.change_titles,
        &g.file_basenames,
        &g.description,
    );
    WorkItem {
        title,
        minutes: g.minutes,
        ticket: g.ticket,
        branches: g.branches,
        change_titles: g.change_titles,
        file_basenames: g.file_basenames,
        description: g.description,
    }
}

/// Best available identifying label, in order of confidence. Always
/// built from already-scrubbed fields — never a fresh raw string.
fn title_for(
    ticket: &Option<String>,
    branches: &[String],
    change_titles: &[String],
    file_basenames: &[String],
    description: &Option<String>,
) -> String {
    ticket
        .clone()
        .or_else(|| branches.first().cloned())
        .or_else(|| change_titles.first().cloned())
        .or_else(|| file_basenames.first().cloned())
        .or_else(|| description.clone())
        .unwrap_or_else(|| "verkefni".to_string())
}

#[path = "clues_work_items_test.rs"]
#[cfg(test)]
mod tests;
