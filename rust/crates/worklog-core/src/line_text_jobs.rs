//! In-flight billing-line regeneration job tracking (spec change set:
//! background regenerate) — the daemon's `AppState` holds one
//! [`JobTracker`] so a second regenerate for a key already running is
//! rejected instead of racing the first, and `GET
//! /billing/lines/status` can answer without touching sqlite.

use crate::clues_contract::BillingLineKey;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobState {
    Running,
    Done,
    Failed(String),
}

/// ponytail: a single mutex over one `HashMap` — fine at this daemon's
/// single-user, handful-of-regenerates-a-day scale; shard by key if
/// contention ever shows up.
pub struct JobTracker<K = BillingLineKey>(Mutex<HashMap<K, JobState>>);

impl<K> Default for JobTracker<K> {
    fn default() -> Self {
        Self(Mutex::default())
    }
}

impl<K: Eq + Hash + Clone> JobTracker<K> {
    /// Marks `key` as `Running` and returns `true`, unless a job for it
    /// is already running — then returns `false` without changing
    /// anything. The check-and-set happens under one lock so two
    /// concurrent regenerate calls for the same key can't both start.
    pub fn try_start(&self, key: K) -> bool {
        let mut map = self.0.lock().unwrap();
        if matches!(map.get(&key), Some(JobState::Running)) {
            return false;
        }
        map.insert(key, JobState::Running);
        true
    }

    /// Records a finished job's outcome — called once the background
    /// task's prepare/invoke/commit pass returns.
    pub fn finish(&self, key: K, result: std::result::Result<(), String>) {
        let mut map = self.0.lock().unwrap();
        let state = match result {
            Ok(()) => JobState::Done,
            Err(reason) => JobState::Failed(reason),
        };
        map.insert(key, state);
    }

    /// `None` when nothing has ever been tracked for `key` (the poll
    /// route reports this as `"idle"`).
    pub fn state(&self, key: &K) -> Option<JobState> {
        self.0.lock().unwrap().get(key).cloned()
    }
}

#[path = "line_text_jobs_test.rs"]
#[cfg(test)]
mod tests;
