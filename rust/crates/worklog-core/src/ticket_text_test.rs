use super::*;
use serde_json::json;

fn doc(content: serde_json::Value) -> serde_json::Value {
    json!({ "type": "doc", "version": 1, "content": content })
}

fn text(value: &str) -> serde_json::Value {
    json!({ "type": "text", "text": value })
}

#[test]
fn paragraphs_split_on_blank_lines() {
    assert_eq!(
        markdown_to_adf("one\n\ntwo"),
        doc(json!([
            { "type": "paragraph", "content": [text("one")] },
            { "type": "paragraph", "content": [text("two")] },
        ]))
    );
}

#[test]
fn heading_level_follows_hash_count() {
    assert_eq!(
        markdown_to_adf("## Scope"),
        doc(json!([
            { "type": "heading", "attrs": { "level": 2 }, "content": [text("Scope")] },
        ]))
    );
}

#[test]
fn bullets_become_one_bullet_list() {
    assert_eq!(
        markdown_to_adf("- a\n- b"),
        doc(json!([{ "type": "bulletList", "content": [
            { "type": "listItem", "content": [{ "type": "paragraph", "content": [text("a")] }] },
            { "type": "listItem", "content": [{ "type": "paragraph", "content": [text("b")] }] },
        ]}]))
    );
}

#[test]
fn task_list_carries_state_and_unique_ids() {
    let adf = markdown_to_adf("- [ ] open\n- [x] done");
    let list = &adf["content"][0];
    assert_eq!(list["type"], "taskList");
    let items = list["content"].as_array().unwrap();
    assert_eq!(items[0]["type"], "taskItem");
    assert_eq!(items[0]["attrs"]["state"], "TODO");
    assert_eq!(items[0]["content"], json!([text("open")]));
    assert_eq!(items[1]["attrs"]["state"], "DONE");
    assert_eq!(items[1]["content"], json!([text("done")]));
    assert_ne!(items[0]["attrs"]["localId"], items[1]["attrs"]["localId"]);
    assert!(list["attrs"]["localId"].is_string());
}

#[test]
fn bold_and_link_become_marks() {
    assert_eq!(
        markdown_to_adf("a **b** [c](https://x.test/y) d"),
        doc(json!([{ "type": "paragraph", "content": [
            text("a "),
            { "type": "text", "text": "b", "marks": [{ "type": "strong" }] },
            text(" "),
            { "type": "text", "text": "c",
              "marks": [{ "type": "link", "attrs": { "href": "https://x.test/y" } }] },
            text(" d"),
        ]}]))
    );
}

#[test]
fn unmatched_markers_stay_literal() {
    assert_eq!(
        markdown_to_adf("2 ** 3 [x]"),
        doc(json!([{ "type": "paragraph", "content": [text("2 ** 3 [x]")] }]))
    );
}

#[test]
fn empty_input_is_an_empty_doc() {
    assert_eq!(markdown_to_adf("  \n\n"), doc(json!([])));
}

#[test]
fn emoji_detection() {
    assert!(has_emoji("ship it \u{1F680}"));
    assert!(has_emoji("done \u{2705}"));
    assert!(has_emoji("warn \u{26A0}\u{FE0F}"));
    assert!(!has_emoji("plain text, Þórður — 100% (ok) → fine"));
}
