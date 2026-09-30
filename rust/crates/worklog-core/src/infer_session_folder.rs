//! A Claude session can start in a non-repo folder (e.g. `~/Desktop/Work`)
//! and move into a project later. The hook records NULL for the early
//! events, and folderless events never create time, so the pre-move stretch
//! vanished. This gives each such event its session's folder.

use crate::infer::InferEvent;
use std::collections::HashMap;

/// Fill `project_path` for session events that lack one: the latest known
/// folder of the same session at or before the event, else the first folder
/// that session reaches later. Events without a session are untouched.
/// `events` must be sorted by `ts` ascending.
// ponytail: sees one day's events only; a session that moves folders across
// midnight keeps its NULLs on the earlier day. Load neighbouring days to fix.
pub(crate) fn fill_session_folders(events: &mut [InferEvent]) {
    let mut last: HashMap<String, String> = HashMap::new();
    for e in events.iter_mut() {
        let Some(s) = &e.session_id else { continue };
        match &e.project_path {
            Some(p) => {
                last.insert(s.clone(), p.clone());
            }
            None => e.project_path = last.get(s).cloned(),
        }
    }
    let mut next: HashMap<String, String> = HashMap::new();
    for e in events.iter_mut().rev() {
        let Some(s) = &e.session_id else { continue };
        match &e.project_path {
            Some(p) => {
                next.insert(s.clone(), p.clone());
            }
            None => e.project_path = next.get(s).cloned(),
        }
    }
}

#[cfg(test)]
#[path = "infer_session_folder_test.rs"]
mod tests;
