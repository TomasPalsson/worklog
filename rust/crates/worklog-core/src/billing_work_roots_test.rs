//! `~/Work` is the live work root; `~/Desktop/Work` stays recognised so
//! historical events keep billing as before.

use super::*;

fn home() -> String {
    dirs::home_dir().unwrap().to_string_lossy().into_owned()
}

#[test]
fn both_roots_resolve_to_the_project_folder() {
    let h = home();
    for path in [
        format!("{h}/Work/sjukra/app"),
        format!("{h}/Desktop/Work/sjukra/.claude/worktrees/x"),
    ] {
        assert_eq!(work_folder_for_path(&path).as_deref(), Some("sjukra"));
        assert_eq!(billable_work_folder(&path).as_deref(), Some("sjukra"));
    }
}

#[test]
fn work_root_itself_is_not_a_project() {
    let h = home();
    assert_eq!(work_folder_for_path(&format!("{h}/Work")), None);
    assert_eq!(billable_work_folder(&format!("{h}/Work")), None);
}

#[test]
fn sibling_with_work_prefix_is_not_billable() {
    assert_eq!(
        billable_work_folder(&format!("{}/Workshop/foo", home())),
        None
    );
}

#[test]
fn live_root_is_first() {
    assert_eq!(work_prefix(), Some(format!("{}/Work", home()).as_str()));
}
