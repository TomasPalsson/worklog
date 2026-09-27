//! Tests for `clues_work_items::group` — grouping by task, not by time.

use super::*;

fn clue(minutes: i64, ticket: Option<&str>, branch: Option<&str>) -> BlockClue {
    BlockClue {
        minutes,
        ticket: ticket.map(str::to_string),
        branches: branch.map(|b| vec![b.to_string()]).unwrap_or_default(),
        change_titles: Vec::new(),
        file_basenames: Vec::new(),
        description: None,
    }
}

#[test]
fn groups_two_blocks_sharing_a_branch_and_leaves_the_other_alone() {
    let blocks = vec![
        clue(30, None, Some("fix-login")),
        clue(90, None, Some("other-branch")),
        clue(45, None, Some("fix-login")),
    ];
    let items = group(blocks);
    assert_eq!(
        items.len(),
        2,
        "{items:?}",
        items = items.iter().map(|i| &i.title).collect::<Vec<_>>()
    );
    // Biggest by minutes first: the other-branch block's 90 alone beats
    // the fix-login group's summed 30+45=75.
    assert_eq!(items[0].minutes, 90);
    assert_eq!(items[1].minutes, 75);
    assert_eq!(items[1].branches, vec!["fix-login".to_string()]);
}

#[test]
fn shared_jira_key_groups_before_branch() {
    let a = clue(20, Some("ABC-1"), Some("branch-a"));
    let b = clue(10, Some("ABC-1"), Some("branch-b"));
    let items = group(vec![a, b]);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].minutes, 30);
    assert_eq!(items[0].ticket, Some("ABC-1".to_string()));
    assert_eq!(
        items[0].branches,
        vec!["branch-a".to_string(), "branch-b".to_string()]
    );
}

#[test]
fn same_description_case_insensitive_groups_when_no_key_or_branch() {
    let a = BlockClue {
        minutes: 15,
        ticket: None,
        branches: Vec::new(),
        change_titles: Vec::new(),
        file_basenames: Vec::new(),
        description: Some("Fixed the deploy script".to_string()),
    };
    let b = BlockClue {
        minutes: 15,
        ticket: None,
        branches: Vec::new(),
        change_titles: Vec::new(),
        file_basenames: Vec::new(),
        description: Some("fixed the deploy script".to_string()),
    };
    let items = group(vec![a, b]);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].minutes, 30);
}

#[test]
fn no_shared_key_branch_or_description_stays_separate() {
    let a = clue(10, None, None);
    let b = clue(10, None, None);
    let items = group(vec![a, b]);
    assert_eq!(items.len(), 2);
}

#[test]
fn title_falls_back_through_ticket_branch_change_title_file_description() {
    let mut item = clue(10, None, None);
    item.description = Some("did a thing".to_string());
    let items = group(vec![item]);
    assert_eq!(items[0].title, "did a thing");

    let with_ticket = clue(10, Some("ABC-9"), Some("some-branch"));
    let items = group(vec![with_ticket]);
    assert_eq!(items[0].title, "ABC-9");
}
