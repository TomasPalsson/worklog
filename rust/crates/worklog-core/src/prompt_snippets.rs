//! Prompt snippets for the owner's own review, read on demand.
//!
//! PRIVACY: prompt text is never stored in the worklog database. When the
//! owner opens a block's events, the daemon reads the first line of each
//! prompt straight from the local Claude transcript
//! (`~/.claude/projects/*/<session>.jsonl`) and hands it to the local UI.
//! Nothing is written, logged, or sent anywhere else.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use crate::models::Event;

/// Longest snippet, in characters.
const SNIPPET_CHARS: usize = 160;

/// `event id → snippet` for every `claude_turn` in `events` whose line is
/// still in a transcript under the default root.
pub fn for_events(events: &[Event]) -> HashMap<i64, String> {
    match dirs::home_dir() {
        Some(home) => from_root(&home.join(".claude/projects"), events),
        None => HashMap::new(),
    }
}

pub fn from_root(root: &Path, events: &[Event]) -> HashMap<i64, String> {
    // uuid → event id, grouped by session so each transcript is read once.
    let mut by_session: HashMap<&str, HashMap<&str, i64>> = HashMap::new();
    for e in events.iter().filter(|e| e.source == "claude_turn") {
        let (Some(id), Some((session, uuid))) = (e.id, e.source_id.split_once(':')) else {
            continue;
        };
        by_session.entry(session).or_default().insert(uuid, id);
    }
    let mut out = HashMap::new();
    let Ok(dirs) = std::fs::read_dir(root) else {
        return out;
    };
    let dirs: Vec<_> = dirs.flatten().map(|d| d.path()).collect();
    for (session, wanted) in by_session {
        let Some(file) = dirs
            .iter()
            .map(|d| d.join(format!("{session}.jsonl")))
            .find(|f| f.is_file())
        else {
            continue;
        };
        let Ok(content) = std::fs::read_to_string(file) else {
            continue;
        };
        for line in content.lines() {
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let Some(id) = v
                .get("uuid")
                .and_then(Value::as_str)
                .and_then(|u| wanted.get(u))
            else {
                continue;
            };
            if let Some(s) = snippet(&v) {
                out.insert(*id, s);
            }
        }
    }
    out
}

/// The typed text of a prompt line, whitespace-collapsed and cut short.
fn snippet(line: &Value) -> Option<String> {
    let text = match line.pointer("/message/content")? {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter(|i| i.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|i| i.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(" "),
        _ => return None,
    };
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() {
        return None;
    }
    let mut chars = flat.chars();
    let cut: String = chars.by_ref().take(SNIPPET_CHARS).collect();
    Some(if chars.next().is_some() {
        format!("{cut}…")
    } else {
        cut
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt_event(id: i64, session: &str, uuid: &str) -> Event {
        let mut e = Event::minimal(
            "claude_turn",
            format!("{session}:{uuid}"),
            "2026-09-23T09:00:00Z",
            "prompt",
        );
        e.id = Some(id);
        e
    }

    #[test]
    fn reads_the_prompt_from_the_transcript_cut_short() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("-Users-x-Desktop-Work-widget");
        std::fs::create_dir_all(&dir).unwrap();
        let long = "fix   the login\nbug ".repeat(20);
        let lines = [
            format!(r#"{{"type":"user","uuid":"u1","message":{{"role":"user","content":{}}}}}"#, serde_json::to_string(&long).unwrap()),
            r#"{"type":"user","uuid":"u2","message":{"role":"user","content":[{"type":"text","text":"deploy to staging"}]}}"#.to_string(),
        ];
        std::fs::write(dir.join("s1.jsonl"), lines.join("\n")).unwrap();

        let got = from_root(
            tmp.path(),
            &[prompt_event(1, "s1", "u1"), prompt_event(2, "s1", "u2")],
        );
        let one = &got[&1];
        assert!(one.starts_with("fix the login bug fix"), "{one}");
        assert!(
            one.ends_with('…') && one.chars().count() == SNIPPET_CHARS + 1,
            "{one}"
        );
        assert_eq!(got[&2], "deploy to staging");
    }

    #[test]
    fn missing_transcript_gives_no_snippet() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(from_root(tmp.path(), &[prompt_event(1, "gone", "u1")]).is_empty());
    }
}
