//! Splits a shared repo folder's lane by session customer.
//!
//! A folder like `vitinn-infra` can host two different customers' Claude
//! sessions back to back. When a folder's sessions name at least two
//! distinct customers, each resolved session's events get `lane_tag` set
//! so `infer_lanes::lane_key` gives it its own lane; a folder that never
//! shows more than one named customer stays a single lane (R1 unchanged).

use std::collections::{BTreeMap, BTreeSet};

use crate::billing_registry::Registry;
use crate::infer::InferEvent;
use crate::infer_lanes::lane_folder;

pub(crate) fn tag_sessions(events: &mut [InferEvent], registry: &Registry) {
    let mut sessions: BTreeMap<(Option<String>, String), Vec<usize>> = BTreeMap::new();
    for (i, e) in events.iter().enumerate() {
        if let Some(sid) = e.session_id.clone() {
            sessions.entry((lane_folder(e), sid)).or_default().push(i);
        }
    }

    let mut resolved: BTreeMap<(Option<String>, String), String> = BTreeMap::new();
    for (key, idxs) in &sessions {
        let text = idxs
            .iter()
            .flat_map(|&i| [events[i].title.as_deref(), events[i].jira_issue.as_deref()])
            .flatten()
            .collect::<Vec<_>>()
            .join("\n");
        if let Some(customer) = registry.customer_in_text(&text) {
            resolved.insert(key.clone(), customer);
        }
    }

    let mut customers_by_folder: BTreeMap<Option<String>, BTreeSet<&str>> = BTreeMap::new();
    for ((folder, _), customer) in &resolved {
        customers_by_folder
            .entry(folder.clone())
            .or_default()
            .insert(customer.as_str());
    }

    for (key, customer) in &resolved {
        let (folder, _) = key;
        if customers_by_folder.get(folder).map_or(0, BTreeSet::len) < 2 {
            continue;
        }
        for &i in &sessions[key] {
            events[i].lane_tag = Some(customer.clone());
        }
    }
}

#[cfg(test)]
#[path = "session_customers_test.rs"]
mod tests;
