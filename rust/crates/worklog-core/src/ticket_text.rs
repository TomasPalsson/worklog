//! Markdown → Atlassian Document Format for ticket text (spec 013).

use serde_json::{json, Value};

pub fn markdown_to_adf(md: &str) -> Value {
    let mut blocks: Vec<Value> = Vec::new();
    let mut paragraph: Vec<&str> = Vec::new();
    let mut list: Option<(ListKind, Vec<Value>)> = None;
    let mut task_counter = 0usize;

    for line in md.lines().map(str::trim_end).chain(std::iter::once("")) {
        let item = list_item(line);
        let continues = matches!((&list, &item), (Some((kind, _)), Some((item_kind, _))) if std::mem::discriminant(kind) == std::mem::discriminant(item_kind));
        if !continues {
            flush_list(&mut blocks, &mut list);
        }
        if item.is_none() && heading(line).is_none() && !line.trim().is_empty() {
            paragraph.push(line.trim());
            continue;
        }
        flush_paragraph(&mut blocks, &mut paragraph);

        if let Some((kind, body)) = item {
            task_counter += 1;
            let entry = list.get_or_insert_with(|| (kind, Vec::new()));
            entry.1.push(match kind {
                ListKind::Bullet => json!({
                    "type": "listItem",
                    "content": [{ "type": "paragraph", "content": inline(body) }],
                }),
                ListKind::Task(done) => json!({
                    "type": "taskItem",
                    "attrs": { "localId": format!("task-{task_counter}"), "state": if done { "DONE" } else { "TODO" } },
                    "content": inline(body),
                }),
            });
        } else if let Some((level, title)) = heading(line) {
            blocks.push(json!({
                "type": "heading",
                "attrs": { "level": level },
                "content": inline(title),
            }));
        }
    }

    json!({ "type": "doc", "version": 1, "content": blocks })
}

pub fn has_emoji(s: &str) -> bool {
    s.chars().any(|c| {
        matches!(
            c as u32,
            0x1F000..=0x1FAFF | 0x2600..=0x26FF | 0x2702 | 0x2705 | 0x2708..=0x270D | 0x270F | 0x2712 | 0x2714 | 0x2716 | 0x271D | 0x2721 | 0x2728 | 0x2733 | 0x2734 | 0x2744 | 0x2747 | 0x274C | 0x274E | 0x2753..=0x2755 | 0x2757 | 0x2763 | 0x2764 | 0x2795..=0x2797 | 0x27A1 | 0x27B0 | 0x27BF | 0x203C | 0x2049 | 0x2934 | 0x2935 | 0x2B05..=0x2B07 | 0x2B1B | 0x2B1C | 0x2B50 | 0x2B55 | 0x3030 | 0x303D | 0x3297 | 0x3299 | 0x231A..=0x231B | 0x23E9..=0x23FA
        )
    })
}

#[derive(Clone, Copy)]
enum ListKind {
    Bullet,
    Task(bool),
}

fn list_item(line: &str) -> Option<(ListKind, &str)> {
    let rest = line
        .trim_start()
        .strip_prefix("- ")
        .or_else(|| line.trim_start().strip_prefix("* "))?;
    for (marker, done) in [("[ ] ", false), ("[x] ", true), ("[X] ", true)] {
        if let Some(body) = rest.strip_prefix(marker) {
            return Some((ListKind::Task(done), body));
        }
    }
    Some((ListKind::Bullet, rest))
}

fn heading(line: &str) -> Option<(usize, &str)> {
    let hashes = line.chars().take_while(|&c| c == '#').count();
    let title = line[hashes..].strip_prefix(' ')?;
    (1..=6).contains(&hashes).then_some((hashes, title.trim()))
}

fn flush_paragraph(blocks: &mut Vec<Value>, lines: &mut Vec<&str>) {
    if !lines.is_empty() {
        blocks.push(json!({ "type": "paragraph", "content": inline(&lines.join(" ")) }));
        lines.clear();
    }
}

fn flush_list(blocks: &mut Vec<Value>, list: &mut Option<(ListKind, Vec<Value>)>) {
    match list.take() {
        Some((ListKind::Bullet, items)) => {
            blocks.push(json!({ "type": "bulletList", "content": items }));
        }
        Some((ListKind::Task(_), items)) => {
            let local_id = items[0]["attrs"]["localId"].as_str().unwrap_or_default();
            blocks.push(json!({
                "type": "taskList",
                "attrs": { "localId": format!("list-{local_id}") },
                "content": items,
            }));
        }
        None => {}
    }
}

fn inline(text: &str) -> Vec<Value> {
    let mut nodes = Vec::new();
    let mut plain = String::new();
    let mut rest = text;

    while !rest.is_empty() {
        if let Some((node, tail)) = bold(rest).or_else(|| link(rest)) {
            if !plain.is_empty() {
                nodes.push(json!({ "type": "text", "text": std::mem::take(&mut plain) }));
            }
            nodes.push(node);
            rest = tail;
        } else {
            let ch = rest.chars().next().unwrap_or_default();
            plain.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }
    if !plain.is_empty() {
        nodes.push(json!({ "type": "text", "text": plain }));
    }
    nodes
}

fn bold(text: &str) -> Option<(Value, &str)> {
    let inner = text.strip_prefix("**")?;
    let end = inner.find("**").filter(|&end| end > 0)?;
    let node = json!({ "type": "text", "text": &inner[..end], "marks": [{ "type": "strong" }] });
    Some((node, &inner[end + 2..]))
}

fn link(text: &str) -> Option<(Value, &str)> {
    let inner = text.strip_prefix('[')?;
    let label_end = inner.find(']')?;
    let after = inner[label_end..].strip_prefix("](")?;
    let url_end = after.find(')')?;
    if label_end == 0 || url_end == 0 {
        return None;
    }
    let node = json!({
        "type": "text",
        "text": &inner[..label_end],
        "marks": [{ "type": "link", "attrs": { "href": &after[..url_end] } }],
    });
    Some((node, &after[url_end + 1..]))
}

#[cfg(test)]
#[path = "ticket_text_test.rs"]
mod tests;
