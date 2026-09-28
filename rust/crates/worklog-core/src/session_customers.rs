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

use chrono::{DateTime, Utc};

use crate::billing_registry::Registry;
use crate::infer::InferEvent;
use crate::infer_lanes::lane_folder;
use crate::session_pins::{resolve_event_customer, SessionPin};

/// `(lane_folder, session_id)` — the key `group_by_session` and
/// [`session_contexts`] both group on.
type SessionKey = (Option<String>, String);

/// `(session_start, text_guess)` — the two per-session inputs
/// [`resolve_event_customer`] needs beyond the pins themselves.
type SessionContext = (DateTime<Utc>, Option<String>);

/// Groups `events` by `(lane_folder, session_id)`, same key
/// [`tag_sessions`] and [`session_contexts`] both build on — a session
/// spans several blocks whenever there's an idle gap, so this is meant
/// to run over a whole day's events, never a single block's.
fn group_by_session(events: &[InferEvent]) -> BTreeMap<SessionKey, Vec<usize>> {
    let mut sessions: BTreeMap<SessionKey, Vec<usize>> = BTreeMap::new();
    for (i, e) in events.iter().enumerate() {
        if let Some(sid) = e.session_id.clone() {
            sessions.entry((lane_folder(e), sid)).or_default().push(i);
        }
    }
    sessions
}

/// A session's `(session_start, text_guess)` from its events at `idxs` —
/// the session's first event timestamp (anchoring
/// `session_pins::pin_covering`'s setup-race reach-back) and the
/// customer named in its joined title/jira text, if any. The ONE formula
/// both [`resolve_events`] and [`session_contexts`] use, so a session's
/// context can never drift between the two.
fn session_start_and_text_guess(
    idxs: &[usize],
    events: &[InferEvent],
    registry: &Registry,
) -> (DateTime<Utc>, Option<String>) {
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
    (session_start, text_guess)
}

/// Every session's `(session_start, text_guess)` in `events`, keyed the
/// same way [`tag_sessions`] groups sessions — exposed so
/// `tenant_split::pinned_customer_for_block` can feed
/// [`resolve_event_customer`] exactly the inputs the lanes use. A
/// session spans several blocks whenever there's an idle gap between
/// them, so `events` must be a whole day's set (or wider), never a
/// single block's — a block-local recomputation would give a blinkered
/// session_start/text_guess that disagrees with the lanes.
pub(crate) fn session_contexts(
    events: &[InferEvent],
    registry: &Registry,
) -> BTreeMap<SessionKey, SessionContext> {
    group_by_session(events)
        .iter()
        .map(|(key, idxs)| {
            (
                key.clone(),
                session_start_and_text_guess(idxs, events, registry),
            )
        })
        .collect()
}

/// Per-event resolved customer, via the shared
/// [`resolve_event_customer`] rule (pin beats text guess; reach-back
/// loses to an existing text guess) — kept in one place with
/// `tenant_split::pinned_customer_for_block` so lanes and the Pinned
/// block slice can't disagree about the same event.
fn resolve_events(
    events: &[InferEvent],
    registry: &Registry,
    sessions: &BTreeMap<SessionKey, Vec<usize>>,
    pins: &[SessionPin],
) -> BTreeMap<usize, String> {
    let mut resolved: BTreeMap<usize, String> = BTreeMap::new();
    for (key, idxs) in sessions {
        let (_, session_id) = key;
        let (session_start, text_guess) = session_start_and_text_guess(idxs, events, registry);
        for &i in idxs {
            let ts = events[i].ts;
            let (customer, _from_pin) =
                resolve_event_customer(pins, session_id, ts, session_start, text_guess.as_deref());
            if let Some(customer) = customer {
                resolved.insert(i, customer);
            }
        }
    }
    resolved
}

pub(crate) fn tag_sessions(events: &mut [InferEvent], registry: &Registry, pins: &[SessionPin]) {
    let sessions = group_by_session(events);

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
