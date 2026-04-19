//! Download + extract the `web/` subdirectory of a GitHub archive.
//!
//! This exists because the installer only drops the `worklog` binary —
//! users don't clone the repo. `worklog web up` needs the `web/` tree
//! (Dockerfile + bun sources) to build the container image. Rather than
//! bundling ~5 MB of JS into the binary, we pull it on demand from the
//! GitHub archive that matches the installed binary version.
//!
//! Target URL pattern:
//!
//!   https://github.com/<owner>/<repo>/archive/refs/tags/<tag>.tar.gz
//!
//! or (for dev / pre-release versions):
//!
//!   https://github.com/<owner>/<repo>/archive/refs/heads/main.tar.gz
//!
//! Cache layout:
//!
//!   $paths.data_dir/web/                      ← target of the extract
//!   $paths.data_dir/web/.fetched-version      ← string of the tag that
//!                                                produced this tree;
//!                                                used to skip re-fetch

use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use flate2::read::GzDecoder;
use reqwest::blocking::Client;
use tar::Archive;
use tracing::debug;

use crate::paths::Paths;

/// The GitHub repo we pull from. Overridable via env so tests and forks
/// can point at a local httpmock server.
pub const DEFAULT_REPO: &str = "TomasPalsson/worklog";
pub const ENV_ARCHIVE_URL: &str = "WORKLOG_WEB_ARCHIVE_URL";

/// 50 MB cap on archive size. The repo's archive is ~2 MB; anything near
/// the cap means we're downloading the wrong thing.
pub const MAX_ARCHIVE_BYTES: u64 = 50 * 1024 * 1024;

/// Cached web tree lives under the data dir so it survives upgrades but
/// can be nuked along with the rest of worklog's local state.
pub fn cache_dir(paths: &Paths) -> PathBuf {
    paths.data_dir.join("web")
}

fn fetched_version_file(paths: &Paths) -> PathBuf {
    cache_dir(paths).join(".fetched-version")
}

/// Decide which git ref's archive to pull. Released binaries get a
/// tag-matching archive; dev/rc builds fall back to `main`.
pub fn ref_for_version(version: &str) -> String {
    // `0.3.0` → `refs/tags/v0.3.0`
    // `0.3.0-dev`, `0.3.0-rc.1` → `refs/heads/main`
    if version
        .chars()
        .all(|c| c.is_ascii_digit() || c == '.')
    {
        format!("refs/tags/v{version}")
    } else {
        "refs/heads/main".to_owned()
    }
}

/// Build the archive URL for the current binary.
///
/// Respects `$WORKLOG_WEB_ARCHIVE_URL` (used by tests to swap in an
/// httpmock URL). Otherwise constructs the github.com/<repo>/archive
/// URL for the given git ref.
pub fn archive_url_for(version: &str) -> String {
    if let Ok(url) = std::env::var(ENV_ARCHIVE_URL) {
        return url;
    }
    let git_ref = ref_for_version(version);
    format!("https://github.com/{DEFAULT_REPO}/archive/{git_ref}.tar.gz")
}

/// Download + extract the web/ subdirectory of the archive into `dest`.
///
/// `dest` is wiped and recreated so a fresh fetch can't leave stale
/// files from a prior version. Returns the canonical path on success.
pub fn fetch_and_extract(url: &str, dest: &Path, client: &Client) -> Result<PathBuf> {
    debug!(%url, dest = %dest.display(), "gcal: downloading web archive");

    let resp = client
        .get(url)
        .send()
        .with_context(|| format!("GET {url}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        return Err(anyhow!(
            "web-archive download failed ({status}) — URL was {url}"
        ));
    }

    // Wipe + recreate dest so prior fetches don't pollute.
    if dest.exists() {
        std::fs::remove_dir_all(dest)
            .with_context(|| format!("wiping {}", dest.display()))?;
    }
    std::fs::create_dir_all(dest).with_context(|| format!("creating {}", dest.display()))?;

    // Streams through: reqwest -> gzip -> tar, capped at MAX_ARCHIVE_BYTES.
    // The cap protects against a compressed-bomb attack (a malicious
    // archive whose gunzip output balloons to GB) — we count bytes read
    // from the gunzip stream, not the compressed bytes.
    let reader = BufReader::new(resp);
    let gz = GzDecoder::new(reader);
    let capped = gz.take(MAX_ARCHIVE_BYTES);
    let mut archive = Archive::new(capped);

    // tar archives from GitHub have a single top-level directory:
    //   <repo>-<sha-or-tag>/web/Dockerfile
    //   <repo>-<sha-or-tag>/web/package.json
    //   ...
    // We strip that prefix and filter to just the web/ subtree, writing
    // into `dest` directly so the final layout is `dest/Dockerfile`.
    for entry in archive.entries().context("reading tar entries")? {
        let mut entry = entry.context("bad tar entry")?;
        let path = entry.path().context("entry path")?.into_owned();

        // First component is the archive's root dir; second must be "web".
        let mut comps = path.components();
        let _root = match comps.next() {
            Some(c) => c,
            None => continue,
        };
        let second = match comps.next() {
            Some(c) => c,
            None => continue,
        };
        if second.as_os_str() != "web" {
            continue;
        }
        let rest: PathBuf = comps.collect();
        if rest.as_os_str().is_empty() {
            // The web/ directory entry itself; skip.
            continue;
        }

        // Reject anything with absolute / parent components — tar allows
        // these and a malicious archive could overwrite arbitrary paths.
        for comp in rest.components() {
            use std::path::Component;
            if matches!(comp, Component::ParentDir | Component::RootDir | Component::Prefix(_)) {
                return Err(anyhow!(
                    "refusing to extract archive entry with unsafe path: {}",
                    rest.display()
                ));
            }
        }

        let target = dest.join(&rest);
        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&target)
                .with_context(|| format!("mkdir {}", target.display()))?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("mkdir {}", parent.display()))?;
            }
            entry
                .unpack(&target)
                .with_context(|| format!("unpack {}", target.display()))?;
        }
    }

    // Sanity-check the minimum required file is present. If the archive
    // layout ever changes we'd rather fail here than spend ten minutes
    // wondering why docker-compose can't find the Dockerfile.
    let dockerfile = dest.join("Dockerfile");
    if !dockerfile.is_file() {
        return Err(anyhow!(
            "archive extracted but {} is missing — archive layout may have changed",
            dockerfile.display()
        ));
    }

    std::fs::canonicalize(dest).with_context(|| format!("canonicalising {}", dest.display()))
}

/// One-shot fetch into `paths.data_dir/web`, stamping a `.fetched-version`
/// file so subsequent `worklog web up` runs can skip the download when
/// the binary version hasn't changed.
pub fn fetch_to_cache(paths: &Paths, version: &str) -> Result<PathBuf> {
    let dest = cache_dir(paths);
    let url = archive_url_for(version);
    let client = crate::http::client()?;
    let out = fetch_and_extract(&url, &dest, &client)?;
    std::fs::write(fetched_version_file(paths), version)
        .with_context(|| "writing .fetched-version")?;
    Ok(out)
}

/// True iff `cache_dir` already has a Dockerfile that was fetched for
/// the current binary version. Lets `resolve_web_context` skip a
/// network call on the warm path.
pub fn cache_is_current(paths: &Paths, version: &str) -> bool {
    let cache = cache_dir(paths);
    if !cache.join("Dockerfile").is_file() {
        return false;
    }
    match std::fs::read_to_string(fetched_version_file(paths)) {
        Ok(stamp) => stamp.trim() == version,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Build a minimal tar.gz that mimics GitHub's archive layout:
    ///   worklog-<sha>/web/Dockerfile
    ///   worklog-<sha>/web/package.json
    ///   worklog-<sha>/web/src/app/page.tsx
    /// plus a decoy file outside the web tree to test filtering.
    fn build_fake_archive() -> Vec<u8> {
        use flate2::write::GzEncoder;
        use flate2::Compression;

        let gz_buf = Vec::new();
        let encoder = GzEncoder::new(gz_buf, Compression::default());
        let mut tar = tar::Builder::new(encoder);

        let files = [
            ("worklog-abc123/web/Dockerfile", "FROM bun:1\n"),
            ("worklog-abc123/web/package.json", "{}\n"),
            ("worklog-abc123/web/src/app/page.tsx", "export default fn\n"),
            // Decoy outside the web tree — must be filtered.
            ("worklog-abc123/README.md", "hi\n"),
            ("worklog-abc123/rust/Cargo.toml", "[package]\n"),
        ];
        for (path, body) in files {
            let bytes = body.as_bytes();
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_mtime(1_000_000_000);
            header.set_cksum();
            tar.append_data(&mut header, path, bytes).unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn ref_for_version_stable_semver_gets_tag_ref() {
        assert_eq!(ref_for_version("0.3.0"), "refs/tags/v0.3.0");
        assert_eq!(ref_for_version("1.2.3"), "refs/tags/v1.2.3");
    }

    #[test]
    fn ref_for_version_prerelease_falls_back_to_main() {
        // -dev, -rc.1, -beta, -alpha.7 all fall back to main so a locally
        // built or pre-release binary can still pull a working web tree.
        assert_eq!(ref_for_version("0.3.0-dev"), "refs/heads/main");
        assert_eq!(ref_for_version("0.3.0-rc.1"), "refs/heads/main");
        assert_eq!(ref_for_version("1.0.0-beta.3"), "refs/heads/main");
    }

    // Combined into one test to avoid a race when `cargo test` runs them
    // concurrently: set_var from one would leak into the other's assertion.
    // Serialising within a single `#[test]` body is cheaper than pulling in
    // the serial_test crate just for this.
    #[test]
    fn archive_url_env_override_and_default() {
        std::env::remove_var(ENV_ARCHIVE_URL);
        let default = archive_url_for("0.3.0");
        assert!(
            default.starts_with("https://github.com/"),
            "got: {default}"
        );
        assert!(default.contains("/archive/refs/tags/v0.3.0.tar.gz"));

        std::env::set_var(ENV_ARCHIVE_URL, "http://localhost:9999/fake.tar.gz");
        assert_eq!(
            archive_url_for("0.3.0"),
            "http://localhost:9999/fake.tar.gz"
        );
        std::env::remove_var(ENV_ARCHIVE_URL);
    }

    #[test]
    fn fetch_and_extract_pulls_only_the_web_subtree() {
        use httpmock::prelude::*;
        let server = MockServer::start();
        let archive = build_fake_archive();
        server.mock(|when, then| {
            when.method(GET).path("/fake.tar.gz");
            then.status(200)
                .header("content-type", "application/gzip")
                .body(archive);
        });

        let tmp = tempdir().unwrap();
        let dest = tmp.path().join("web");
        let client = crate::http::client().unwrap();
        let out = fetch_and_extract(
            &format!("{}/fake.tar.gz", server.base_url()),
            &dest,
            &client,
        )
        .unwrap();

        // Dockerfile + package.json + src tree must all land under dest/.
        assert!(out.join("Dockerfile").is_file());
        assert!(out.join("package.json").is_file());
        assert!(out.join("src/app/page.tsx").is_file());
        // Decoy files from outside web/ must be filtered out.
        assert!(!out.join("README.md").exists(), "decoy file leaked");
        assert!(!out.join("Cargo.toml").exists(), "decoy file leaked");
    }

    // NB: we rely on the `tar` crate's own path-traversal check inside
    // `Entry::unpack` as the primary defence (it refuses entries with
    // `..` components). Our `Component::ParentDir` filter is belt-and-
    // braces. A direct test would need to hand-construct raw tar bytes
    // because `tar::Builder::append_data` refuses to *write* such an
    // entry — so the codepath is exercised by the upstream crate's
    // own tests.

    #[test]
    fn fetch_and_extract_errors_when_web_tree_is_missing_dockerfile() {
        // Archive whose web/ subtree doesn't contain a Dockerfile — e.g.
        // because the repo layout changed upstream. Must fail rather than
        // silently return an unusable directory.
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use httpmock::prelude::*;

        let gz_buf = Vec::new();
        let encoder = GzEncoder::new(gz_buf, Compression::default());
        let mut tar = tar::Builder::new(encoder);
        let body = b"{}";
        let mut header = tar::Header::new_gnu();
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(1_000_000_000);
        header.set_cksum();
        tar.append_data(&mut header, "worklog-x/web/package.json", &body[..])
            .unwrap();
        let archive = tar.into_inner().unwrap().finish().unwrap();

        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/partial.tar.gz");
            then.status(200).body(archive);
        });

        let tmp = tempdir().unwrap();
        let dest = tmp.path().join("web");
        let client = crate::http::client().unwrap();
        let err = fetch_and_extract(
            &format!("{}/partial.tar.gz", server.base_url()),
            &dest,
            &client,
        )
        .unwrap_err();
        assert!(
            format!("{err:#}").contains("Dockerfile"),
            "missing-Dockerfile sanity check didn't fire; got: {err:#}"
        );
    }

    #[test]
    fn cache_is_current_detects_stale_stamp() {
        let tmp = tempdir().unwrap();
        let paths = Paths::from_root(tmp.path());
        paths.ensure().unwrap();

        // Fresh install: no Dockerfile.
        assert!(!cache_is_current(&paths, "0.3.0"));

        // Drop a Dockerfile but no stamp — treated as stale.
        std::fs::create_dir_all(cache_dir(&paths)).unwrap();
        std::fs::write(cache_dir(&paths).join("Dockerfile"), b"FROM bun").unwrap();
        assert!(!cache_is_current(&paths, "0.3.0"));

        // Stamp the correct version — now current.
        std::fs::write(fetched_version_file(&paths), "0.3.0\n").unwrap();
        assert!(cache_is_current(&paths, "0.3.0"));

        // Binary upgraded — cache is now stale.
        assert!(!cache_is_current(&paths, "0.3.1"));
    }
}

