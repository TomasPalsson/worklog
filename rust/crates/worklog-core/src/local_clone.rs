//! Repo → local folder and sha-presence checks for org commits/PRs (spec
//! 006, FR-03/FR-04, Glossary: "Submodule owner"). Reuses
//! `billing::work_prefix` and the billing submodule map rather than
//! re-deriving folder mapping — see `billing::submodule_repo_map`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::billing;

/// True when `sha` is a commit reachable from `folder`'s own git history,
/// or from the checkout of any submodule it declares. Any error (not a
/// repo, git missing, unknown sha) is "not local" — D-07, FR-04.
pub fn sha_is_local(folder: &Path, sha: &str) -> bool {
    if cat_file_has_commit(folder, sha) {
        return true;
    }
    submodule_paths(folder)
        .into_iter()
        .any(|path| cat_file_has_commit(&folder.join(path), sha))
}

fn cat_file_has_commit(folder: &Path, sha: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(folder)
        .args(["cat-file", "-e", &format!("{sha}^{{commit}}")])
        .output()
        .is_ok_and(|out| out.status.success())
}

/// `submodule.<name>.path` values declared in `folder`'s `.gitmodules`,
/// read through `git config` rather than parsing the file by hand.
fn submodule_paths(folder: &Path) -> Vec<PathBuf> {
    let Ok(output) = Command::new("git")
        .arg("-C")
        .arg(folder)
        .args([
            "config",
            "-f",
            ".gitmodules",
            "--get-regexp",
            r"^submodule\..*\.path$",
        ])
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once(' '))
        .map(|(_, path)| PathBuf::from(path))
        .collect()
}

/// The local work-folder that owns `repo` (`"org/repo"`), if any: either a
/// direct clone at `<work prefix>/<repo basename>`, or the top-level
/// folder whose `.gitmodules` vendors it as a submodule (Glossary:
/// "Submodule owner"). `None` when neither exists — FR-04, the commit is
/// "done elsewhere".
pub fn folder_for_repo(repo: &str) -> Option<String> {
    let root = billing::work_prefix()?;
    resolve_folder_for_repo(Path::new(root), repo, billing::submodule_repo_map())
}

#[cfg(test)]
fn folder_for_repo_under(root: &Path, repo: &str) -> Option<String> {
    let map = billing::submodule_repo_map_under(root);
    resolve_folder_for_repo(root, repo, &map)
}

fn resolve_folder_for_repo(
    root: &Path,
    repo: &str,
    submodule_map: &HashMap<String, String>,
) -> Option<String> {
    let basename = repo.rsplit('/').next().filter(|s| !s.is_empty())?;
    let direct = root.join(basename);
    if direct.join(".git").exists() {
        return Some(direct.to_string_lossy().into_owned());
    }
    submodule_map
        .get(basename)
        .map(|owner| root.join(owner).to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_repo_with_commit(path: &Path) -> String {
        std::fs::create_dir_all(path).unwrap();
        run(path, &["init", "-q", "-b", "main"]);
        run(path, &["config", "user.email", "test@example.com"]);
        run(path, &["config", "user.name", "Tester"]);
        run(path, &["config", "commit.gpgsign", "false"]);
        std::fs::write(path.join("f.txt"), "hello\n").unwrap();
        run(path, &["add", "f.txt"]);
        run(path, &["commit", "-q", "-m", "first"]);
        let out = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }

    fn run(cwd: &Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn sha_present_sets_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_dir = tmp.path().join("myrepo");
        let sha = init_repo_with_commit(&repo_dir);

        let folder = folder_for_repo_under(tmp.path(), "org/myrepo");
        assert_eq!(folder.as_deref(), Some(repo_dir.to_string_lossy().as_ref()));
        assert!(sha_is_local(&repo_dir, &sha));
    }

    #[test]
    fn sha_absent_is_elsewhere() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_dir = tmp.path().join("myrepo");
        init_repo_with_commit(&repo_dir);

        let bogus_sha = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
        assert!(!sha_is_local(&repo_dir, bogus_sha));
    }

    #[test]
    fn submodule_repo_maps_to_owner_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let owner_dir = tmp.path().join("owner-folder");
        std::fs::create_dir_all(&owner_dir).unwrap();
        std::fs::write(
            owner_dir.join(".gitmodules"),
            "[submodule \"vendor/sub-repo\"]\n\
             \tpath = vendor/sub-repo\n\
             \turl = git@github.com:org/sub-repo.git\n",
        )
        .unwrap();
        let sha = init_repo_with_commit(&owner_dir.join("vendor/sub-repo"));

        let folder = folder_for_repo_under(tmp.path(), "org/sub-repo");
        assert_eq!(
            folder.as_deref(),
            Some(owner_dir.to_string_lossy().as_ref())
        );
        assert!(
            sha_is_local(&owner_dir, &sha),
            "sha must be found inside the submodule's own checkout"
        );
    }

    #[test]
    fn unknown_repo_resolves_to_none() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("some-other-folder")).unwrap();

        assert_eq!(folder_for_repo_under(tmp.path(), "org/nope"), None);
    }
}
