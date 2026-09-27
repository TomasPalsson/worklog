use std::path::Path;
use std::process::Command;

use chrono::{TimeZone, Utc};

use super::*;
use crate::billing_registry::{Customer, FolderMap};
use crate::db;

fn home() -> String {
    dirs::home_dir().unwrap().to_string_lossy().into_owned()
}

fn work(sub: &str) -> String {
    format!("{}/Desktop/Work/{sub}", home())
}

fn registry() -> Registry {
    Registry {
        customers: vec![
            Customer {
                id: None,
                name: "Sjúkra".into(),
                aliases: vec!["Sjukratryggingar".into()],
            },
            Customer {
                id: None,
                name: "APRÓ".into(),
                aliases: vec![],
            },
        ],
        folders: vec![],
    }
}

fn at(h: u32, m: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 27, h, m, 0).unwrap()
}

#[test]
fn pin_stores_a_resolved_customer() {
    let conn = db::open_memory().unwrap();
    let reg = registry();
    let cwd = work("vitinn-infra/tenants/sjukra");

    let stored = pin(
        &conn,
        &reg,
        "sess-1",
        Path::new(&cwd),
        "sjúkra",
        at(9, 0),
        Some("feat/x"),
    )
    .unwrap();

    assert_eq!(stored.session_id, "sess-1");
    assert_eq!(stored.customer, "Sjúkra");
    assert_eq!(stored.from_at, at(9, 0));
    assert_eq!(stored.folder, "vitinn-infra");
    assert_eq!(stored.branch.as_deref(), Some("feat/x"));
    assert_eq!(stored.source, "claude");

    let rows = pins_for_sessions(&conn, &["sess-1".to_string()]).unwrap();
    assert_eq!(rows, vec![stored]);
}

#[test]
fn pin_matches_a_customer_alias_case_insensitively() {
    let conn = db::open_memory().unwrap();
    let reg = registry();
    let cwd = work("vitinn-infra");

    let stored = pin(
        &conn,
        &reg,
        "sess-1",
        Path::new(&cwd),
        "SJUKRATRYGGINGAR",
        at(9, 0),
        None,
    )
    .unwrap();

    assert_eq!(stored.customer, "Sjúkra");
}

#[test]
fn pin_refuses_an_unknown_customer_and_stores_nothing() {
    let conn = db::open_memory().unwrap();
    let reg = registry();
    let cwd = work("vitinn-infra");

    let err = pin(
        &conn,
        &reg,
        "sess-1",
        Path::new(&cwd),
        "Sjukra tryggingar",
        at(9, 0),
        None,
    )
    .expect_err("unknown customer must be refused");

    match err {
        PinError::UnknownCustomer { known } => {
            assert_eq!(known, vec!["Sjúkra".to_string(), "APRÓ".to_string()]);
        }
        PinError::Other(e) => panic!("expected UnknownCustomer, got {e:#}"),
    }

    let rows = pins_for_sessions(&conn, &["sess-1".to_string()]).unwrap();
    assert!(rows.is_empty(), "a refused pin must store nothing");
}

#[test]
fn pin_fails_when_cwd_has_no_work_folder() {
    let conn = db::open_memory().unwrap();
    let reg = registry();

    let err = pin(
        &conn,
        &reg,
        "sess-1",
        Path::new(""),
        "Sjúkra",
        at(9, 0),
        None,
    )
    .expect_err("an unusable cwd must not be pinned");
    assert!(matches!(err, PinError::Other(_)));
}

#[test]
fn re_pinning_at_the_same_time_overwrites_the_row() {
    let conn = db::open_memory().unwrap();
    let reg = registry();
    let cwd = work("vitinn-infra");

    pin(
        &conn,
        &reg,
        "sess-1",
        Path::new(&cwd),
        "Sjúkra",
        at(9, 0),
        None,
    )
    .unwrap();
    pin(
        &conn,
        &reg,
        "sess-1",
        Path::new(&cwd),
        "APRÓ",
        at(9, 0),
        None,
    )
    .unwrap();

    let rows = pins_for_sessions(&conn, &["sess-1".to_string()]).unwrap();
    assert_eq!(rows.len(), 1, "same (session, from_at) is one row");
    assert_eq!(rows[0].customer, "APRÓ");
}

#[test]
fn re_pinning_at_a_new_time_adds_a_second_row() {
    let conn = db::open_memory().unwrap();
    let reg = registry();
    let cwd = work("vitinn-infra");

    pin(
        &conn,
        &reg,
        "sess-1",
        Path::new(&cwd),
        "Sjúkra",
        at(9, 0),
        None,
    )
    .unwrap();
    pin(
        &conn,
        &reg,
        "sess-1",
        Path::new(&cwd),
        "APRÓ",
        at(11, 0),
        None,
    )
    .unwrap();

    let rows = pins_for_sessions(&conn, &["sess-1".to_string()]).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].customer, "Sjúkra");
    assert_eq!(rows[0].from_at, at(9, 0));
    assert_eq!(rows[1].customer, "APRÓ");
    assert_eq!(rows[1].from_at, at(11, 0));
}

#[test]
fn pin_for_branch_returns_the_latest_pin() {
    let conn = db::open_memory().unwrap();
    let reg = registry();
    let cwd = work("vitinn-infra");

    pin(
        &conn,
        &reg,
        "sess-1",
        Path::new(&cwd),
        "Sjúkra",
        at(9, 0),
        Some("feat/x"),
    )
    .unwrap();
    pin(
        &conn,
        &reg,
        "sess-2",
        Path::new(&cwd),
        "APRÓ",
        at(11, 0),
        Some("feat/x"),
    )
    .unwrap();

    let latest = pin_for_branch(&conn, "vitinn-infra", "feat/x")
        .unwrap()
        .expect("a pin exists");
    assert_eq!(latest.session_id, "sess-2");
    assert_eq!(latest.customer, "APRÓ");
}

#[test]
fn pin_for_branch_never_inherits_on_a_default_branch() {
    let conn = db::open_memory().unwrap();
    let reg = registry();
    let cwd = work("vitinn-infra");

    pin(
        &conn,
        &reg,
        "sess-1",
        Path::new(&cwd),
        "Sjúkra",
        at(9, 0),
        Some("main"),
    )
    .unwrap();

    assert_eq!(pin_for_branch(&conn, "vitinn-infra", "main").unwrap(), None);
    assert_eq!(
        pin_for_branch(&conn, "vitinn-infra", "master").unwrap(),
        None
    );
}

#[test]
fn pin_for_branch_returns_none_when_nothing_is_pinned() {
    let conn = db::open_memory().unwrap();
    assert_eq!(
        pin_for_branch(&conn, "vitinn-infra", "feat/x").unwrap(),
        None
    );
}

#[test]
fn pins_for_sessions_filters_to_the_given_ids() {
    let conn = db::open_memory().unwrap();
    let reg = registry();
    let cwd = work("vitinn-infra");

    pin(
        &conn,
        &reg,
        "sess-1",
        Path::new(&cwd),
        "Sjúkra",
        at(9, 0),
        None,
    )
    .unwrap();
    pin(
        &conn,
        &reg,
        "sess-2",
        Path::new(&cwd),
        "APRÓ",
        at(9, 0),
        None,
    )
    .unwrap();
    pin(
        &conn,
        &reg,
        "sess-3",
        Path::new(&cwd),
        "APRÓ",
        at(9, 0),
        None,
    )
    .unwrap();

    let rows = pins_for_sessions(&conn, &["sess-1".to_string(), "sess-3".to_string()]).unwrap();
    let ids: Vec<&str> = rows.iter().map(|p| p.session_id.as_str()).collect();
    assert_eq!(ids, vec!["sess-1", "sess-3"]);
}

#[test]
fn pins_for_sessions_returns_empty_for_no_ids() {
    let conn = db::open_memory().unwrap();
    assert_eq!(pins_for_sessions(&conn, &[]).unwrap(), Vec::new());
}

#[test]
fn is_default_branch_matches_main_and_master_only() {
    assert!(is_default_branch("main"));
    assert!(is_default_branch("master"));
    assert!(!is_default_branch("feat/x"));
    assert!(!is_default_branch("Main"));
}

fn multi_tenant_folder(folder: &str) -> FolderMap {
    FolderMap {
        id: None,
        folder: folder.into(),
        customer: None,
        verkefni: None,
        billable: true,
        multi_tenant: true,
    }
}

fn init_git_repo(path: &Path, branch: &str) {
    let out = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["init", "-q", "-b", branch])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn start_text_inherits_branch_pin() {
    let conn = db::open_memory().unwrap();
    let mut reg = registry();

    // `billing::billable_work_folder` (the strict /Work gate `start_text`
    // now uses) and `git::current_branch` both require a *real* directory,
    // and the folder must genuinely sit under `~/Desktop/Work` for the
    // gate to pass — a tempdir elsewhere no longer qualifies. A uniquely
    // named `TempDir` created directly inside the real work root is the
    // only way to satisfy both without an env-var override (which would
    // race other tests touching the same process-global `OnceLock` in
    // `billing::work_prefix`); the random suffix means the registered
    // folder name can never collide with a real project, and the
    // directory is removed again when `repo_dir` drops.
    let work_root = std::path::Path::new(&home()).join("Desktop/Work");
    std::fs::create_dir_all(&work_root).unwrap();
    let repo_dir = tempfile::Builder::new()
        .prefix("session-pins-test-")
        .tempdir_in(&work_root)
        .unwrap();
    let folder_name = repo_dir
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    reg.folders.push(multi_tenant_folder(&folder_name));
    init_git_repo(repo_dir.path(), "feat/x");

    pin(
        &conn,
        &reg,
        "sess-1",
        repo_dir.path(),
        "Sjúkra",
        at(9, 0),
        Some("feat/x"),
    )
    .unwrap();

    let text = start_text(&conn, &reg, "sess-2", repo_dir.path(), at(10, 0))
        .unwrap()
        .expect("a branch pin must produce inherited start text");

    assert_eq!(
        text,
        "This session is for Sjúkra (pinned from branch feat/x). If you switch customer, run: worklog pin <name> --session sess-2"
    );

    let rows = pins_for_sessions(&conn, &["sess-2".to_string()]).unwrap();
    assert_eq!(rows.len(), 1, "inheritance must store one pin row");
    assert_eq!(rows[0].customer, "Sjúkra");
    assert_eq!(rows[0].source, "inherited");
    assert_eq!(rows[0].branch.as_deref(), Some("feat/x"));
    assert_eq!(rows[0].from_at, at(10, 0));
}

#[test]
fn start_text_prints_the_pin_instruction_in_a_multi_tenant_folder() {
    let conn = db::open_memory().unwrap();
    let mut reg = registry();
    reg.folders.push(multi_tenant_folder("vitinn-infra"));

    // Non-existent path: `git::current_branch` returns `None` for it, so
    // this exercises the "no branch pin" leg of `start_text`, not the
    // inheritance leg covered by `start_text_inherits_branch_pin`.
    let cwd = work("vitinn-infra");

    let text = start_text(&conn, &reg, "sess-9", Path::new(&cwd), at(9, 0))
        .unwrap()
        .expect("a multi-tenant folder must print the pin instruction");

    assert!(
        text.contains("worklog pin <name> --session sess-9"),
        "instruction must carry the exact pin command: {text}"
    );
    assert!(text.contains("Sjúkra"), "must list known customers: {text}");
    assert!(text.contains("APRÓ"), "must list known customers: {text}");
    assert!(
        text.chars().count() <= 600,
        "start text must be at most 600 chars, was {}",
        text.chars().count()
    );

    // No branch pin existed, so nothing gets stored for this session.
    assert!(pins_for_sessions(&conn, &["sess-9".to_string()])
        .unwrap()
        .is_empty());
}

#[test]
fn start_text_is_none_when_the_folder_is_not_multi_tenant() {
    let conn = db::open_memory().unwrap();
    let mut reg = registry();
    reg.folders.push(FolderMap {
        id: None,
        folder: "apro-skills".into(),
        customer: Some("APRÓ".into()),
        verkefni: None,
        billable: true,
        multi_tenant: false,
    });
    let cwd = work("apro-skills");

    assert_eq!(
        start_text(&conn, &reg, "sess-1", Path::new(&cwd), at(9, 0)).unwrap(),
        None
    );
}

#[test]
fn start_text_is_none_outside_a_registered_work_folder() {
    let conn = db::open_memory().unwrap();
    let reg = registry();
    let cwd = Path::new("/tmp/does-not-exist/personal-blog");

    assert_eq!(
        start_text(&conn, &reg, "sess-1", cwd, at(9, 0)).unwrap(),
        None
    );
}

#[test]
fn start_text_is_none_outside_desktop_work_even_when_basename_matches() {
    let conn = db::open_memory().unwrap();
    let mut reg = registry();
    reg.folders.push(multi_tenant_folder("vitinn-infra"));
    // Same basename as a registered multi-tenant folder, but reached from
    // outside `~/Desktop/Work` — `work_folder_for_path`'s lenient fallback
    // must not be enough to fire the instruction here (FR-01, §2.2).
    let cwd = Path::new("/tmp/not-work/vitinn-infra");

    assert_eq!(
        start_text(&conn, &reg, "sess-1", cwd, at(9, 0)).unwrap(),
        None
    );
    assert!(
        pins_for_sessions(&conn, &["sess-1".to_string()])
            .unwrap()
            .is_empty(),
        "a path outside /Work must never write an inherited pin"
    );
}
