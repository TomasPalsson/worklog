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
//!   $paths.data_dir/web/.fetched-version      ← two lines: the version
//!                                                that produced this
//!                                                tree, then an RFC-3339
//!                                                UTC fetch timestamp.
//!                                                Read by
//!                                                `cache_is_current` to
//!                                                decide whether to
//!                                                re-fetch.
//!   $paths.data_dir/web.incoming/             ← transient staging dir
//!   $paths.data_dir/web.previous/             ← transient, old tree
//!                                                during the swap
//!   $paths.data_dir/web.lock                  ← flock, serialises fetches
//!
//! The stamp is what stops a populated cache from being trusted
//! forever: `resolve_web_context` only serves the cache when the stamp
//! matches the running binary, so an upgrade no longer leaves the user
//! staring at the previous release's UI.

use std::fs::{File, OpenOptions};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use flate2::read::GzDecoder;
use fs4::fs_std::FileExt;
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
    if version.chars().all(|c| c.is_ascii_digit() || c == '.') {
        format!("refs/tags/v{version}")
    } else {
        "refs/heads/main".to_owned()
    }
}

/// True iff `ref_for_version` resolves to a branch (mutable) rather than
/// a tag (cut once, immutable). Dev/rc builds track `main`, so a
/// version-string match alone doesn't prove the cache still matches
/// what's on GitHub — see the TTL applied in `cache_is_current`.
pub fn ref_is_mutable(version: &str) -> bool {
    ref_for_version(version).starts_with("refs/heads/")
}

/// How long a mutable-ref (dev/rc) cache is trusted without a re-fetch.
/// Tagged releases are immutable so they have no TTL.
pub const MUTABLE_CACHE_TTL_HOURS: i64 = 24;

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
/// Extraction happens in a staging dir that's a sibling of `dest` (same
/// parent, hence guaranteed same filesystem, so the final swap is a
/// pair of atomic renames: old tree aside, staged tree in). `dest` is
/// only ever touched after the staged tree has passed the Dockerfile
/// sanity check below — a mid-stream failure (flaky wifi, disk-full,
/// corrupt archive) can then never destroy a previously-working cache,
/// which is what lets `resolve_web_context` fall back to a stale cache
/// when the network is down. Returns the canonical path on success.
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

    let file_name = dest
        .file_name()
        .context("dest has no file name; can't derive a staging path")?;
    let staging = dest.with_file_name(format!("{}.incoming", file_name.to_string_lossy()));

    // Leftover from a previous crash mid-extraction — clear it before we
    // reuse the name.
    if staging.exists() {
        std::fs::remove_dir_all(&staging)
            .with_context(|| format!("wiping stale staging dir {}", staging.display()))?;
    }
    std::fs::create_dir_all(&staging).with_context(|| format!("creating {}", staging.display()))?;

    // Every fallible step between here and the atomic rename funnels
    // through this closure: on Err we remove the staging dir and
    // propagate, leaving `dest` byte-for-byte untouched.
    let extracted: Result<()> = (|| {
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
        // into `staging` directly so the final layout is `staging/Dockerfile`.
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
                if matches!(
                    comp,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                ) {
                    return Err(anyhow!(
                        "refusing to extract archive entry with unsafe path: {}",
                        rest.display()
                    ));
                }
            }

            let target = staging.join(&rest);
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
        let dockerfile = staging.join("Dockerfile");
        if !dockerfile.is_file() {
            return Err(anyhow!(
                "archive extracted but {} is missing — archive layout may have changed",
                dockerfile.display()
            ));
        }
        Ok(())
    })();

    if let Err(e) = extracted {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }

    // Staged tree passed the sanity check — swap it into place. Every
    // path here shares `dest`'s parent (see the staging path derivation
    // above), so each rename is atomic and same-filesystem.
    //
    // The old tree is moved aside rather than deleted first: a
    // `remove_dir_all` that fails partway (say one unreadable file)
    // would leave `dest` half-deleted with no way back, and
    // `resolve_web_context`'s stale-cache fallback would then serve a
    // corrupted tree while reporting success. A rename either works or
    // leaves `dest` untouched, so there is no half-state to serve.
    let previous = dest.with_file_name(format!("{}.previous", file_name.to_string_lossy()));
    let _ = std::fs::remove_dir_all(&previous);
    let had_previous = if dest.exists() {
        if let Err(e) = std::fs::rename(dest, &previous)
            .with_context(|| format!("moving {} aside to {}", dest.display(), previous.display()))
        {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e);
        }
        true
    } else {
        false
    };
    if let Err(e) = std::fs::rename(&staging, dest)
        .with_context(|| format!("renaming {} to {}", staging.display(), dest.display()))
    {
        // Put the old tree back so the caller still has a usable cache.
        if had_previous {
            let _ = std::fs::rename(&previous, dest);
        }
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }
    // New tree is live — the old one is now just garbage.
    if had_previous {
        let _ = std::fs::remove_dir_all(&previous);
    }

    std::fs::canonicalize(dest).with_context(|| format!("canonicalising {}", dest.display()))
}

/// RAII handle for the web-cache lockfile.
///
/// We explicitly `unlock` in Drop instead of relying on close-releases-flock
/// semantics. On macOS the BSD `flock(2)` release on close is observable in
/// the kernel asynchronously, so a re-`open + flock` immediately after drop
/// can transiently return `EWOULDBLOCK`. An explicit unlock makes the
/// release deterministic.
#[derive(Debug)]
pub struct CacheLock {
    file: File,
}

impl Drop for CacheLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

/// Acquire an exclusive, non-blocking advisory lock on
/// `<paths.data_dir>/web.lock`. Five call sites can trigger a re-fetch
/// (`web up` / `web build` / `serve` / `day --serve` / `web fetch`), so
/// without this two concurrent invocations could race to wipe/refill the
/// cache. Returns the guard on success; on `WouldBlock` the caller should
/// fall back to whatever stale cache is already on disk.
pub fn acquire_cache_lock(paths: &Paths) -> Result<CacheLock> {
    std::fs::create_dir_all(&paths.data_dir)
        .with_context(|| format!("creating {}", paths.data_dir.display()))?;
    let lock_path = paths.data_dir.join("web.lock");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("opening lock file {}", lock_path.display()))?;
    match file.try_lock_exclusive() {
        Ok(()) => Ok(CacheLock { file }),
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => anyhow::bail!(
            "another worklog web fetch is already running \
             (lock held on {}). Wait for it to finish and retry.",
            lock_path.display()
        ),
        Err(e) => Err(e).with_context(|| format!("locking {}", lock_path.display())),
    }
}

/// Process-wide lock guarding tests that mutate `$WORKLOG_WEB_ARCHIVE_URL`,
/// `$WORKLOG_WEB_DIR`, or the current directory — all process-global state
/// that `cargo test`'s default multi-threaded runner can race on. Shared
/// across the `fetch` and `web` test modules so a set_var in one can never
/// leak into another's assertion.
#[cfg(test)]
pub(crate) fn env_test_lock() -> &'static std::sync::Mutex<()> {
    use std::sync::{Mutex, OnceLock};
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// One-shot fetch into `paths.data_dir/web`, stamping a `.fetched-version`
/// file so subsequent `worklog web up` runs can skip the download when
/// the binary version hasn't changed. Serialises against concurrent
/// fetches via `acquire_cache_lock`.
pub fn fetch_to_cache(paths: &Paths, version: &str) -> Result<PathBuf> {
    let _lock = acquire_cache_lock(paths)?;
    let dest = cache_dir(paths);
    let url = archive_url_for(version);
    let client = crate::http::client()?;
    let out = fetch_and_extract(&url, &dest, &client)?;
    let stamp = format!("{version}\n{}\n", chrono::Utc::now().to_rfc3339());
    std::fs::write(fetched_version_file(paths), stamp)
        .with_context(|| "writing .fetched-version")?;
    Ok(out)
}

/// True iff `cache_dir` already has a Dockerfile that was fetched for
/// the current binary version. Lets `resolve_web_context` skip a
/// network call on the warm path.
///
/// The `.fetched-version` stamp is two lines: the version, and the UTC
/// RFC3339 timestamp of the fetch. Tagged (immutable) versions only need
/// the version to match — the referenced tag can never move. Mutable
/// (dev/rc, tracking `refs/heads/main`) versions additionally need the
/// timestamp to be within `MUTABLE_CACHE_TTL_HOURS`, since main can have
/// moved since the fetch even though the version string is unchanged.
/// A legacy single-line stamp (no timestamp) still validates for a
/// tagged version, for back-compat with caches written by older
/// releases.
pub fn cache_is_current(paths: &Paths, version: &str) -> bool {
    let cache = cache_dir(paths);
    if !cache.join("Dockerfile").is_file() {
        return false;
    }
    let stamp = match std::fs::read_to_string(fetched_version_file(paths)) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let mut lines = stamp.lines();
    let stamped_version = match lines.next() {
        Some(v) => v.trim(),
        None => return false,
    };
    if stamped_version != version {
        return false;
    }
    if !ref_is_mutable(version) {
        // Tagged release: the ref is immutable, so the version match
        // alone proves the cache is current. No TTL needed.
        return true;
    }
    let fetched_at = match lines.next() {
        Some(l) => l.trim(),
        None => return false,
    };
    let fetched_at = match chrono::DateTime::parse_from_rfc3339(fetched_at) {
        Ok(t) => t.with_timezone(&chrono::Utc),
        Err(_) => return false,
    };
    let age = chrono::Utc::now().signed_duration_since(fetched_at);
    let ttl = chrono::Duration::hours(MUTABLE_CACHE_TTL_HOURS);
    age < ttl && age > -ttl
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
        let _guard = env_test_lock().lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var(ENV_ARCHIVE_URL);
        let default = archive_url_for("0.3.0");
        assert!(default.starts_with("https://github.com/"), "got: {default}");
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

    #[test]
    fn cache_is_current_two_line_stamp_tagged_version() {
        let tmp = tempdir().unwrap();
        let paths = Paths::from_root(tmp.path());
        paths.ensure().unwrap();
        std::fs::create_dir_all(cache_dir(&paths)).unwrap();
        std::fs::write(cache_dir(&paths).join("Dockerfile"), b"FROM bun").unwrap();
        let stamp = format!("0.3.0\n{}\n", chrono::Utc::now().to_rfc3339());
        std::fs::write(fetched_version_file(&paths), stamp).unwrap();
        assert!(cache_is_current(&paths, "0.3.0"));
    }

    #[test]
    fn cache_is_current_legacy_one_line_stamp_tagged_version_back_compat() {
        // A stamp written by a previous release (before the TTL line was
        // added) must still validate for a tagged version — the immutable
        // path never looks at line 2.
        let tmp = tempdir().unwrap();
        let paths = Paths::from_root(tmp.path());
        paths.ensure().unwrap();
        std::fs::create_dir_all(cache_dir(&paths)).unwrap();
        std::fs::write(cache_dir(&paths).join("Dockerfile"), b"FROM bun").unwrap();
        std::fs::write(fetched_version_file(&paths), "0.3.0\n").unwrap();
        assert!(cache_is_current(&paths, "0.3.0"));
    }

    #[test]
    fn cache_is_current_mutable_version_respects_ttl() {
        let tmp = tempdir().unwrap();
        let paths = Paths::from_root(tmp.path());
        paths.ensure().unwrap();
        std::fs::create_dir_all(cache_dir(&paths)).unwrap();
        std::fs::write(cache_dir(&paths).join("Dockerfile"), b"FROM bun").unwrap();

        // Fresh timestamp — current.
        let fresh = format!("0.12.0-dev\n{}\n", chrono::Utc::now().to_rfc3339());
        std::fs::write(fetched_version_file(&paths), fresh).unwrap();
        assert!(cache_is_current(&paths, "0.12.0-dev"));

        // 25h old timestamp — stale, main may have moved since.
        let stale_ts = chrono::Utc::now() - chrono::Duration::hours(25);
        let stale = format!("0.12.0-dev\n{}\n", stale_ts.to_rfc3339());
        std::fs::write(fetched_version_file(&paths), stale).unwrap();
        assert!(!cache_is_current(&paths, "0.12.0-dev"));

        // No timestamp line at all — stale, we can't prove it's fresh.
        std::fs::write(fetched_version_file(&paths), "0.12.0-dev\n").unwrap();
        assert!(!cache_is_current(&paths, "0.12.0-dev"));
    }

    #[test]
    fn fetch_and_extract_is_atomic_on_mid_stream_failure() {
        // Pre-populate `dest` with a working cache, then simulate a
        // failed re-fetch (corrupt/truncated body). The pre-existing
        // cache must survive byte-for-byte and no staging dir must leak.
        use httpmock::prelude::*;
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/broken.tar.gz");
            then.status(200)
                .header("content-type", "application/gzip")
                .body(b"this is not a valid gzip stream at all".as_slice());
        });

        let tmp = tempdir().unwrap();
        let dest = tmp.path().join("web");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("Dockerfile"), b"FROM bun:1 # original\n").unwrap();

        let client = crate::http::client().unwrap();
        let err = fetch_and_extract(
            &format!("{}/broken.tar.gz", server.base_url()),
            &dest,
            &client,
        )
        .unwrap_err();
        drop(err);

        // Original cache untouched.
        let contents = std::fs::read_to_string(dest.join("Dockerfile")).unwrap();
        assert_eq!(contents, "FROM bun:1 # original\n");

        // No leftover staging dir.
        let staging = dest.with_file_name("web.incoming");
        assert!(
            !staging.exists(),
            "staging dir leaked after failed fetch: {}",
            staging.display()
        );
    }

    #[test]
    fn fetch_and_extract_replaces_existing_cache_and_leaves_no_scratch_dirs() {
        // The success counterpart to the atomicity test above: a re-fetch
        // over a populated cache must fully replace it (no stale files
        // surviving the swap) and must not leave the `.incoming` or
        // `.previous` scratch dirs behind for the next run to trip over.
        use httpmock::prelude::*;
        let server = MockServer::start();
        let archive = build_fake_archive();
        server.mock(|when, then| {
            when.method(GET).path("/fresh.tar.gz");
            then.status(200)
                .header("content-type", "application/gzip")
                .body(archive);
        });

        let tmp = tempdir().unwrap();
        let dest = tmp.path().join("web");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("Dockerfile"), b"FROM bun:1 # old\n").unwrap();
        // A file that exists only in the old tree — it must not survive.
        std::fs::write(dest.join("removed-upstream.txt"), b"stale\n").unwrap();

        let client = crate::http::client().unwrap();
        let out = fetch_and_extract(
            &format!("{}/fresh.tar.gz", server.base_url()),
            &dest,
            &client,
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(out.join("Dockerfile")).unwrap(),
            "FROM bun:1\n",
            "new tree should have replaced the old Dockerfile"
        );
        assert!(
            !out.join("removed-upstream.txt").exists(),
            "a file dropped upstream survived the swap"
        );
        for scratch in ["web.incoming", "web.previous"] {
            let p = dest.with_file_name(scratch);
            assert!(!p.exists(), "scratch dir leaked: {}", p.display());
        }
    }
}
