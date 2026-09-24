use super::*;
use crate::db::open_memory;

fn write_reflog(repo_dir: &Path, lines: &str) {
    let git_dir = repo_dir.join(".git/logs");
    std::fs::create_dir_all(&git_dir).unwrap();
    std::fs::write(git_dir.join("HEAD"), lines).unwrap();
}

#[test]
fn parses_two_repos_and_filters_by_time_range() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Work");
    std::fs::create_dir_all(&root).unwrap();

    let repo_a = root.join("repo-a");
    std::fs::create_dir_all(&repo_a).unwrap();
    write_reflog(
        &repo_a,
        "0000000000000000000000000000000000000000 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa Tomas Palsson <t@example.com> 1700000000 +0000\tcommit (initial): first\n",
    );

    let repo_b = root.join("repo-b");
    std::fs::create_dir_all(&repo_b).unwrap();
    write_reflog(
        &repo_b,
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb cccccccccccccccccccccccccccccccccccccccc Tomas Palsson <t@example.com> 1800000000 +0000\tcommit: later\n",
    );

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2023, 12, 1).unwrap();
    let report = collect_from_roots(&conn, &[root], since, until).unwrap();
    assert_eq!(report.events_written, 1);
    let events = repo::load_day_events(&conn, "2023-11-14").unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].title, "commit");
    assert_eq!(
        events[0].project_path.as_deref(),
        Some(repo_a.to_string_lossy().as_ref())
    );
}

#[test]
fn titles_carry_no_commit_message_text() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Work");
    std::fs::create_dir_all(&root).unwrap();
    let repo_dir = root.join("repo-a");
    std::fs::create_dir_all(&repo_dir).unwrap();
    write_reflog(
        &repo_dir,
        "0000000000000000000000000000000000000000 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa Tomas Palsson <t@example.com> 1700000000 +0000\tcheckout: moving from main to secret-project-x\n",
    );

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2023, 12, 1).unwrap();
    collect_from_roots(&conn, &[root], since, until).unwrap();
    let events = repo::load_day_events(&conn, "2023-11-14").unwrap();
    assert_eq!(events[0].title, "checkout secret-project-x");
    assert_eq!(events[0].details, None);
}

#[test]
fn re_run_inserts_no_new_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Work");
    std::fs::create_dir_all(&root).unwrap();
    let repo_dir = root.join("repo-a");
    std::fs::create_dir_all(&repo_dir).unwrap();
    write_reflog(
        &repo_dir,
        "0000000000000000000000000000000000000000 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa Tomas Palsson <t@example.com> 1700000000 +0000\tcommit: x\n",
    );

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2023, 12, 1).unwrap();
    collect_from_roots(&conn, std::slice::from_ref(&root), since, until).unwrap();
    let report = collect_from_roots(&conn, &[root], since, until).unwrap();
    assert_eq!(report.events_written, 1, "upsert still counts as written");
    let events = repo::load_day_events(&conn, "2023-11-14").unwrap();
    assert_eq!(events.len(), 1, "dedupe on (source, source_id)");
}

#[test]
fn worktree_reflog_is_collected_and_attributed_to_the_main_repo() {
    // vitinn-infra's `sandbox-runner` worktree keeps its own reflog at
    // `.git/worktrees/sandbox-runner/logs/HEAD` — a sibling of the main
    // `.git/logs/HEAD`, not a subdirectory of it. Every entry there
    // must still land under the main repo's project_path, so the
    // afternoon's commits aren't invisible to billing.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Work");
    std::fs::create_dir_all(&root).unwrap();
    let repo_dir = root.join("vitinn-infra");
    write_reflog(
        &repo_dir,
        "0000000000000000000000000000000000000000 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa Tomas Palsson <t@example.com> 1700000000 +0000\tcommit: main log\n",
    );
    let worktree_log = repo_dir.join(".git/worktrees/sandbox-runner/logs");
    std::fs::create_dir_all(&worktree_log).unwrap();
    std::fs::write(
        worktree_log.join("HEAD"),
        "1111111111111111111111111111111111111111 2222222222222222222222222222222222222222 Tomas Palsson <t@example.com> 1700000100 +0000\tcommit: worktree log\n",
    )
    .unwrap();

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2023, 12, 1).unwrap();
    let report = collect_from_roots(&conn, &[root], since, until).unwrap();
    assert_eq!(report.events_written, 2);
    let events = repo::load_day_events(&conn, "2023-11-14").unwrap();
    assert_eq!(events.len(), 2);
    assert!(events
        .iter()
        .all(|e| e.project_path.as_deref() == Some(repo_dir.to_string_lossy().as_ref())));
    // source_ids must differ even though the epoch/sha pair for the
    // worktree entry is unrelated to the main log's — this asserts the
    // suffix that keeps them from ever colliding.
    let ids: Vec<&str> = events.iter().map(|e| e.source_id.as_str()).collect();
    assert!(ids.iter().any(|id| id.contains(":wt-sandbox-runner:")));
}

#[test]
fn submodule_reflog_is_collected_and_attributed_to_the_main_repo() {
    // The code-interpreter submodule's reflog lives at
    // `.git/modules/tools/code-interpreter/logs/HEAD` under the main
    // repo's `.git`, keyed by its in-repo path, not its own directory.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Work");
    std::fs::create_dir_all(&root).unwrap();
    let repo_dir = root.join("vitinn-infra");
    write_reflog(
        &repo_dir,
        "0000000000000000000000000000000000000000 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa Tomas Palsson <t@example.com> 1700000000 +0000\tcommit: main log\n",
    );
    let submodule_log = repo_dir.join(".git/modules/tools/code-interpreter/logs");
    std::fs::create_dir_all(&submodule_log).unwrap();
    std::fs::write(
        submodule_log.join("HEAD"),
        "3333333333333333333333333333333333333333 4444444444444444444444444444444444444444 Tomas Palsson <t@example.com> 1700000200 +0000\tcommit: submodule log\n",
    )
    .unwrap();

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2023, 12, 1).unwrap();
    let report = collect_from_roots(&conn, &[root], since, until).unwrap();
    assert_eq!(report.events_written, 2);
    let events = repo::load_day_events(&conn, "2023-11-14").unwrap();
    assert_eq!(events.len(), 2);
    assert!(events
        .iter()
        .all(|e| e.project_path.as_deref() == Some(repo_dir.to_string_lossy().as_ref())));
    let ids: Vec<&str> = events.iter().map(|e| e.source_id.as_str()).collect();
    assert!(ids
        .iter()
        .any(|id| id.contains(":sm-tools/code-interpreter:")));
}

#[test]
fn a_worktree_entry_under_root_skips_its_own_file_git() {
    // If a worktree checkout is itself listed directly under a root
    // (its `.git` is a file, not a directory), collect_repo must not
    // try to read logs from it — that log belongs to the main repo and
    // is already collected from there.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Work");
    std::fs::create_dir_all(&root).unwrap();
    let worktree_dir = root.join("some-worktree");
    std::fs::create_dir_all(&worktree_dir).unwrap();
    std::fs::write(
        worktree_dir.join(".git"),
        "gitdir: /elsewhere/.git/worktrees/some-worktree\n",
    )
    .unwrap();

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2023, 12, 1).unwrap();
    let report = collect_from_roots(&conn, &[root], since, until).unwrap();
    assert_eq!(report.events_written, 0);
}
