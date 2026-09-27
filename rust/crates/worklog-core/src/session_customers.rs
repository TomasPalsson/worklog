//! Splits a shared repo folder's lane by session customer.
//!
//! A folder like `vitinn-infra` can host two different customers' Claude
//! sessions back to back. When a folder's sessions name at least two
//! distinct customers, each resolved session's events get `lane_tag` set
//! so `infer_lanes::lane_key` gives it its own lane; a folder that never
//! shows more than one named customer stays a single lane (R1 unchanged).
//!
//! A session pin always wins over the text guess: an event takes the pin
//! whose `from_at` is the latest one at or before its own timestamp, so a
//! mid-session pin switch moves only the events from that time on; events
//! before the earliest applicable pin (or in an unpinned session) fall
//! back to the session's text guess.

use std::collections::{BTreeMap, BTreeSet};

use crate::billing_registry::Registry;
use crate::infer::InferEvent;
use crate::infer_lanes::lane_folder;
use crate::session_pins::SessionPin;

/// Per-event resolved customer: the pin covering the event's timestamp, or
/// the session's text guess when no pin covers it yet.
fn resolve_events(
    events: &[InferEvent],
    registry: &Registry,
    sessions: &BTreeMap<(Option<String>, String), Vec<usize>>,
    pins: &[SessionPin],
) -> BTreeMap<usize, String> {
    let mut pins_by_session: BTreeMap<&str, Vec<&SessionPin>> = BTreeMap::new();
    for p in pins {
        pins_by_session
            .entry(p.session_id.as_str())
            .or_default()
            .push(p);
    }

    let mut resolved: BTreeMap<usize, String> = BTreeMap::new();
    for (key, idxs) in sessions {
        let (_, session_id) = key;
        let text = idxs
            .iter()
            .flat_map(|&i| [events[i].title.as_deref(), events[i].jira_issue.as_deref()])
            .flatten()
            .collect::<Vec<_>>()
            .join("\n");
        let text_guess = registry.customer_in_text(&text);
        let session_pins = pins_by_session.get(session_id.as_str());
        for &i in idxs {
            let pinned = session_pins.and_then(|ps| {
                ps.iter()
                    .filter(|p| p.from_at <= events[i].ts)
                    .max_by_key(|p| p.from_at)
                    .map(|p| p.customer.clone())
            });
            if let Some(customer) = pinned.or_else(|| text_guess.clone()) {
                resolved.insert(i, customer);
            }
        }
    }
    resolved
}

pub(crate) fn tag_sessions(events: &mut [InferEvent], registry: &Registry, pins: &[SessionPin]) {
    let mut sessions: BTreeMap<(Option<String>, String), Vec<usize>> = BTreeMap::new();
    for (i, e) in events.iter().enumerate() {
        if let Some(sid) = e.session_id.clone() {
            sessions.entry((lane_folder(e), sid)).or_default().push(i);
        }
    }

    let resolved = resolve_events(events, registry, &sessions, pins);

    let mut customers_by_folder: BTreeMap<Option<String>, BTreeSet<String>> = BTreeMap::new();
    for (key, idxs) in &sessions {
        let (folder, _) = key;
        for &i in idxs {
            if let Some(customer) = resolved.get(&i) {
                customers_by_folder
                    .entry(folder.clone())
                    .or_default()
                    .insert(customer.clone());
            }
        }
    }

    for (key, idxs) in &sessions {
        let (folder, _) = key;
        if customers_by_folder.get(folder).map_or(0, BTreeSet::len) < 2 {
            continue;
        }
        for &i in idxs {
            if let Some(customer) = resolved.get(&i) {
                events[i].lane_tag = Some(customer.clone());
            }
        }
    }
}

#[cfg(test)]
#[path = "session_customers_test.rs"]
mod tests;
