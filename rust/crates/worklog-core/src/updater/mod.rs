//! Self-updater — signed delta-patch releases with auto-rollback.
//!
//! The pipeline:
//!
//! ```text
//! current binary
//!   │
//!   ▼  (1) fetch manifest.json + .sig from GitHub Releases
//! verify signature (Ed25519, embedded pubkey)
//!   │
//!   ▼  (2) pick asset for this target + version:
//!         delta patch from current → target (preferred, ~100KB)
//!         or full binary (~15MB fallback)
//!   │
//!   ▼  (3) download to temp dir, verify SHA256 + signature
//!   ▼      apply bspatch if delta
//!   ▼      zstd-decompress
//!   │
//!   ▼  (4) smoke test `<new> --version`
//!   ▼      if OK → atomic rename, keep old as `.previous`
//!   ▼      if bad → drop new binary, keep old in place
//!   │
//!   ▼  (5) post-swap smoke test — if exec fails,
//!         rename `.previous` back
//! ```
//!
//! Cryptography:
//! * Ed25519 signatures over the manifest and every asset (the manifest
//!   itself contains SHA256 hashes of each asset, so signing the
//!   manifest transitively signs everything — we still verify per-asset
//!   signatures as defence in depth).
//! * Pubkey is embedded in the binary at compile time (see
//!   [`pubkey::RELEASE_PUBLIC_KEY`]). A release built with the default
//!   placeholder pubkey will refuse every signature — the dev must run
//!   `worklog dev keygen` to generate a real key and embed its pubkey.
//!
//! Safety:
//! * Every step is idempotent so a partial failure leaves the user in a
//!   valid state (either the old binary works, or rollback restores it).
//! * The updater never mutates the worklog DB — it only touches the
//!   binary at `<paths.bin_dir>/worklog-rs` (or whatever CARGO_BIN_EXE is
//!   at build time).

pub mod crypto;
pub mod delta;
pub mod fetch;
pub mod install;
pub mod manifest;
pub mod pubkey;
pub mod signing;

pub use crypto::{sha256_hex, sign_detached, verify_detached, VerifyError};
pub use manifest::{Asset, Manifest, PatchDescriptor, Target};

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tracing::info;

/// Inputs for a full self-update run.
#[derive(Debug, Clone)]
pub struct UpdateRequest {
    /// URL of the signed manifest (e.g.
    /// https://github.com/.../releases/latest/download/manifest.json).
    pub manifest_url: String,
    /// Path to the current worklog binary — the one to replace.
    pub current_binary: PathBuf,
    /// Current version string, used to pick a delta patch.
    pub current_version: String,
    /// Scratch directory for downloads + staging. Should be on the same
    /// filesystem as `current_binary` so the final rename is atomic.
    pub work_dir: PathBuf,
    /// If true, do all fetches + verifications but skip the swap.
    pub dry_run: bool,
    /// If true, re-install even when manifest.version == current_version.
    pub force: bool,
}

/// What happened. Populated even for dry-runs so the CLI can report.
#[derive(Debug)]
pub struct UpdateReport {
    pub from: String,
    pub to: String,
    pub used_delta: bool,
    pub asset_bytes: u64,
    pub dry_run: bool,
    pub rolled_back: bool,
    pub install: Option<install::InstallOutcome>,
}

/// Run the whole update flow: fetch manifest → verify → pick asset →
/// download → apply delta (if any) → decompress → swap with rollback.
pub fn run_update(req: &UpdateRequest) -> Result<UpdateReport> {
    if pubkey::is_placeholder() {
        anyhow::bail!(
            "the release public key is the all-zero placeholder — \
             self-update is disabled until a real key is embedded. \
             Run `worklog dev keygen` on your release-signing machine, \
             paste the printed constant into worklog-core/src/updater/pubkey.rs, \
             and rebuild."
        );
    }

    let pk = pubkey::resolve();
    let http = fetch::client()?;

    let manifest = fetch::fetch_manifest(&http, &req.manifest_url, &pk)?;
    if manifest.schema > manifest::CURRENT_SCHEMA {
        anyhow::bail!(
            "manifest schema {} is newer than this binary understands ({}). \
             Please upgrade manually — `uv tool install git+ssh://…`",
            manifest.schema,
            manifest::CURRENT_SCHEMA,
        );
    }

    if !req.force && manifest.version == req.current_version {
        return Ok(UpdateReport {
            from: req.current_version.clone(),
            to: manifest.version,
            used_delta: false,
            asset_bytes: 0,
            dry_run: req.dry_run,
            rolled_back: false,
            install: None,
        });
    }

    let choice = manifest.pick_asset(&req.current_version).with_context(|| {
        format!(
            "manifest has no asset for target {}",
            Target::current()
                .map(|t| t.triple())
                .unwrap_or("(unsupported)")
        )
    })?;

    std::fs::create_dir_all(&req.work_dir)
        .with_context(|| format!("creating work dir {}", req.work_dir.display()))?;

    let (staged_compressed, used_delta, bytes) = match &choice {
        manifest::Choice::Full(a) => {
            let dst = req.work_dir.join("full.zst");
            fetch::fetch_and_verify_asset(&http, a, &pk, &dst)?;
            (dst, false, a.size)
        }
        manifest::Choice::Delta(pd) => {
            let patch_path = req.work_dir.join("delta.bin");
            fetch::fetch_and_verify_asset(&http, &pd.asset, &pk, &patch_path)?;
            // Apply patch: old binary → new compressed bytes. The patch
            // itself is (old-compressed, new-compressed) so the result
            // is still a zstd-compressed new binary — same as `Full`.
            let old_bytes = std::fs::read(&req.current_binary)
                .with_context(|| format!("reading current binary {}", req.current_binary.display()))?;
            let old_zstd = zstd::encode_all(std::io::Cursor::new(&old_bytes), 19)
                .context("recompress current binary for patch base")?;
            let patch_bytes = std::fs::read(&patch_path)?;
            let new_zstd = delta::apply_patch(&old_zstd, &patch_bytes)?;
            let dst = req.work_dir.join("full.zst");
            std::fs::write(&dst, &new_zstd)?;
            (dst, true, pd.asset.size)
        }
    };

    // Decompress the staged zstd blob → raw binary, ready to smoke-test.
    let staged = req.work_dir.join("worklog.new");
    decompress_zstd(&staged_compressed, &staged)?;

    if req.dry_run {
        info!("dry-run: staged binary at {}", staged.display());
        return Ok(UpdateReport {
            from: req.current_version.clone(),
            to: manifest.version,
            used_delta,
            asset_bytes: bytes,
            dry_run: true,
            rolled_back: false,
            install: None,
        });
    }

    let outcome = install::swap_with_rollback(&staged, &req.current_binary)?;
    let rolled_back = outcome.rolled_back;
    Ok(UpdateReport {
        from: req.current_version.clone(),
        to: manifest.version,
        used_delta,
        asset_bytes: bytes,
        dry_run: false,
        rolled_back,
        install: Some(outcome),
    })
}

fn decompress_zstd(src: &Path, dst: &Path) -> Result<()> {
    let f = std::fs::File::open(src).with_context(|| format!("open {}", src.display()))?;
    let mut decoder = zstd::Decoder::new(f).context("init zstd decoder")?;
    let mut out = std::fs::File::create(dst).with_context(|| format!("create {}", dst.display()))?;
    std::io::copy(&mut decoder, &mut out).context("zstd decompress")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::updater::crypto::generate_keypair;
    use crate::updater::manifest::TargetManifest;
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use ed25519_dalek::Signer;
    use httpmock::prelude::*;
    use tempfile::TempDir;

    fn sha(bytes: &[u8]) -> String {
        crypto::sha256_hex(bytes)
    }

    /// Set up a local signing key as if the env override had baked it in.
    fn with_test_key<F: FnOnce(&ed25519_dalek::SigningKey)>(f: F) {
        let (sk, vk) = generate_keypair();
        std::env::set_var(
            "WORKLOG_RELEASE_PUBKEY_BASE64",
            STANDARD.encode(vk.to_bytes()),
        );
        f(&sk);
        std::env::remove_var("WORKLOG_RELEASE_PUBKEY_BASE64");
    }

    #[test]
    fn run_update_downloads_full_binary_end_to_end() {
        let _g = pubkey::test_env_lock();
        with_test_key(|sk| {
            // Simulate a new release. The "full" asset is a zstd-compressed
            // shell script that reports version "1.0.0".
            let new_script = b"#!/bin/sh\necho worklog 1.0.0\n";
            let compressed = zstd::encode_all(std::io::Cursor::new(new_script), 3).unwrap();
            let full_sig = sk.sign(&compressed).to_bytes();
            let full_sha = sha(&compressed);

            let target = Target::current().expect("running on supported target");
            let manifest = Manifest {
                version: "1.0.0".into(),
                targets: vec![TargetManifest {
                    target,
                    full: Asset {
                        url: "PLACEHOLDER".into(),
                        sha256: full_sha,
                        size: compressed.len() as u64,
                        signature: STANDARD.encode(full_sig),
                    },
                    patches: vec![],
                }],
                notes: "".into(),
                published_at: "".into(),
                schema: 1,
            };

            let server = MockServer::start();
            // Fix the placeholder URL to point at the mock server.
            let mut manifest = manifest;
            manifest.targets[0].full.url = server.url("/full.zst");

            let manifest_json = serde_json::to_vec(&manifest).unwrap();
            let manifest_sig = sk.sign(&manifest_json).to_bytes();

            server.mock(|when, then| {
                when.method(GET).path("/manifest.json");
                then.status(200).body(manifest_json);
            });
            server.mock(|when, then| {
                when.method(GET).path("/manifest.json.sig");
                then.status(200).body(manifest_sig.as_slice());
            });
            server.mock(|when, then| {
                when.method(GET).path("/full.zst");
                then.status(200).body(compressed);
            });

            // Seed an "old" binary that's just a script too.
            let tmp = TempDir::new().unwrap();
            let dest = tmp.path().join("worklog");
            std::fs::write(&dest, b"#!/bin/sh\necho worklog 0.1.0\n").unwrap();
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&dest).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&dest, perms).unwrap();

            let req = UpdateRequest {
                manifest_url: server.url("/manifest.json"),
                current_binary: dest.clone(),
                current_version: "0.1.0".into(),
                work_dir: tmp.path().join("stage"),
                dry_run: false,
                force: false,
            };

            let report = run_update(&req).unwrap();
            assert!(!report.rolled_back, "should install cleanly");
            assert!(!report.used_delta);
            assert_eq!(report.to, "1.0.0");
            let out = std::process::Command::new(&dest)
                .arg("--version")
                .output()
                .unwrap();
            assert!(
                String::from_utf8_lossy(&out.stdout).contains("1.0.0"),
                "binary at {} should now print 1.0.0: {:?}",
                dest.display(),
                out
            );
        });
    }

    #[test]
    fn run_update_short_circuits_when_already_latest() {
        let _g = pubkey::test_env_lock();
        with_test_key(|sk| {
            let target = Target::current().expect("running on supported target");
            let manifest = Manifest {
                version: "0.3.1".into(),
                targets: vec![TargetManifest {
                    target,
                    full: Asset {
                        url: "http://unused".into(),
                        sha256: "0".repeat(64),
                        size: 1,
                        signature: STANDARD.encode([0u8; 64]),
                    },
                    patches: vec![],
                }],
                notes: "".into(),
                published_at: "".into(),
                schema: 1,
            };
            let body = serde_json::to_vec(&manifest).unwrap();
            let msig = sk.sign(&body).to_bytes();

            let server = MockServer::start();
            server.mock(|when, then| {
                when.method(GET).path("/m.json");
                then.status(200).body(body);
            });
            server.mock(|when, then| {
                when.method(GET).path("/m.json.sig");
                then.status(200).body(msig.as_slice());
            });

            let tmp = TempDir::new().unwrap();
            let dest = tmp.path().join("worklog");
            std::fs::write(&dest, b"").unwrap();
            let req = UpdateRequest {
                manifest_url: server.url("/m.json"),
                current_binary: dest,
                current_version: "0.3.1".into(), // same as manifest
                work_dir: tmp.path().join("stage"),
                dry_run: false,
                force: false,
            };
            let report = run_update(&req).unwrap();
            assert_eq!(report.from, "0.3.1");
            assert_eq!(report.to, "0.3.1");
            assert_eq!(report.asset_bytes, 0);
            assert!(report.install.is_none());
        });
    }

}
