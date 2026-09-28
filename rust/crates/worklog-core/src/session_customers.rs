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
use crate::session_pins::{pin_covering, SessionPin};

/// Per-event resolved customer: the pin covering the event's timestamp, or
/// the session's text guess when no pin covers it yet.
///
/// A pin genuinely covering the event (`from_at <= ts`, [`pin_covering`]'s
/// ordinary rule) always wins over the text guess, same as before. A pin
/// that only reaches the event via [`pin_covering`]'s setup-race
/// reach-back (its `from_at` is after the event's own timestamp) instead
/// LOSES to an available text guess — the reach-back exists to cover
/// setup-minute events that would otherwise have no signal at all, not to
/// override a session's own established text guess for a genuine
/// mid-session re-pin (see `pin_beats_text_guess`).
fn resolve_events(
    events: &[InferEvent],
    registry: &Registry,
    sessions: &BTreeMap<(Option<String>, String), Vec<usize>>,
    pins: &[SessionPin],
) -> BTreeMap<usize, String> {
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
        let session_start = idxs
            .iter()
            .map(|&i| events[i].ts)
            .min()
            .expect("idxs is never empty — sessions only holds non-empty groups");
        for &i in idxs {
            let ts = events[i].ts;
            let customer = match pin_covering(pins, session_id, ts, session_start) {
                Some(pin) if pin.from_at <= ts => Some(pin.customer.clone()),
                Some(pin) => text_guess.clone().or_else(|| Some(pin.customer.clone())),
                None => text_guess.clone(),
            };
            if let Some(customer) = customer {
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
