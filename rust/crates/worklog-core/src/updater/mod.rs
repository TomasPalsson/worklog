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

    // Serialize concurrent `self-update` invocations. Two processes
    // writing to the same `worklog.new` staged path + both renaming
    // onto `dest` is a race: whichever rename won second wins the
    // install, the other reports "rolled back" for no reason, and the
    // user ends up on an unpredictable version. A filesystem lock on
    // a well-known path in the work dir lets the second invocation
    // bail cleanly instead.
    std::fs::create_dir_all(&req.work_dir)
        .with_context(|| format!("creating work dir {}", req.work_dir.display()))?;
    let _lock = install::acquire_update_lock(&req.work_dir)?;

    let pk = pubkey::resolve();
    let http = fetch::client()?;

    let manifest = fetch::fetch_manifest(&http, &req.manifest_url, &pk)?;
    if manifest.schema > manifest::CURRENT_SCHEMA {
        anyhow::bail!(
            "manifest schema {} is newer than this binary understands ({}). \
             Re-run the installer to pick up a fresh release: \
             curl -fsSL https://raw.githubusercontent.com/TomasPalsson/worklog/main/install.sh | bash",
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

    // If this binary isn't built for a target we publish, say so up front
    // rather than bubbling a vague "no asset" error.
    let current_target = Target::current().context("self-update target check")?;
    let choice = manifest
        .pick_asset(&req.current_version)
        .with_context(|| format!("manifest has no asset for target {current_target}"))?;

    // work_dir already created + lock held above.

    // Staged path — the final raw binary that `swap_with_rollback` will
    // install. How we get bytes into it depends on the asset type:
    //   Full:  download zstd blob → decompress → staged
    //   Delta: download raw bsdiff patch → apply to current binary
    //          (raw-to-raw; produced by `worklog dev make-patch`) →
    //          verify SHA256 of the reconstructed bytes → staged
    let staged = req.work_dir.join("worklog.new");
    let (used_delta, bytes) = match &choice {
        manifest::Choice::Full(a) => {
            let dst = req.work_dir.join("full.zst");
            fetch::fetch_and_verify_asset(&http, a, &pk, &dst)?;
            decompress_zstd(&dst, &staged)?;
            (false, a.size)
        }
        manifest::Choice::Delta(pd) => {
            // pick_asset already refuses deltas without result_sha256,
            // but keep a defense-in-depth guard in case the selection
            // logic changes in the future.
            debug_assert!(
                !pd.result_sha256.is_empty(),
                "pick_asset should have fallen through to Full for empty result_sha256"
            );
            let patch_path = req.work_dir.join("delta.bin");
            fetch::fetch_and_verify_asset(&http, &pd.asset, &pk, &patch_path)?;
            let old_bytes = std::fs::read(&req.current_binary).with_context(|| {
                format!("reading current binary {}", req.current_binary.display())
            })?;
            let patch_bytes = std::fs::read(&patch_path)?;
            let new_bytes = delta::apply_patch(&old_bytes, &patch_bytes)?;
            // Load-bearing integrity check: bspatch is content-blind and
            // will happily produce garbage if the local `old` doesn't
            // match the `old` the patch was built against (different
            // rustc, sideloaded build, etc.). This is the only line
            // standing between the user and a silently-broken binary.
            let got = crypto::sha256_hex(&new_bytes);
            if got != pd.result_sha256 {
                anyhow::bail!(
                    "delta applied cleanly but the reconstructed binary's SHA256 \
                     ({got}) doesn't match the manifest's expected \
                     result_sha256 ({}). Your current binary may not have been \
                     built from an official release — try `--force` with the \
                     full asset, or re-install manually.",
                    pd.result_sha256
                );
            }
            std::fs::write(&staged, &new_bytes)?;
            (true, pd.asset.size)
        }
    };

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

/// Hard cap on decompressed binary size. Rust worklog binaries compile
/// to ~15MB at release; 200MB is generous headroom while still catching
/// a zstd decompression bomb (a crafted patch that expands to GBs and
/// fills the disk before we notice).
pub const MAX_DECOMPRESSED_BYTES: u64 = 200 * 1024 * 1024;

fn decompress_zstd(src: &Path, dst: &Path) -> Result<()> {
    use std::io::Read;
    let f = std::fs::File::open(src).with_context(|| format!("open {}", src.display()))?;
    let decoder = zstd::Decoder::new(f).context("init zstd decoder")?;
    // `Take` enforces MAX_DECOMPRESSED_BYTES bytes and then yields EOF.
    // If the real output exceeds the cap, the next read returns 0 and we
    // stop writing — then we check whether we stopped at the cap (bomb)
    // or at genuine EOF. This keeps constant memory.
    let mut limited = decoder.take(MAX_DECOMPRESSED_BYTES + 1);
    let mut out = std::fs::File::create(dst).with_context(|| format!("create {}", dst.display()))?;
    let written = std::io::copy(&mut limited, &mut out).context("zstd decompress")?;
    if written > MAX_DECOMPRESSED_BYTES {
        // Purge the partially-written output so the caller never sees it.
        let _ = std::fs::remove_file(dst);
        anyhow::bail!(
            "decompressed output exceeds {}MB cap — refusing (possible decompression bomb)",
            MAX_DECOMPRESSED_BYTES / 1024 / 1024
        );
    }
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

    #[test]
    fn run_update_applies_delta_end_to_end() {
        // Previously the delta path was never end-to-end tested — only
        // make_patch/apply_patch unit-tested. A wiring bug (raw vs zstd
        // mismatch, missing result_sha256, wrong apply target) would
        // have shipped. This test exercises the full delta flow.
        let _g = pubkey::test_env_lock();
        with_test_key(|sk| {
            // "Current" binary that prints old version + has some bulk
            // bytes so the delta actually shrinks.
            let old_script = {
                let mut v = b"#!/bin/sh\necho worklog 1.0.0\n".to_vec();
                v.extend_from_slice(&vec![b'#'; 8000]);
                v
            };
            let new_script = {
                let mut v = b"#!/bin/sh\necho worklog 1.1.0\n".to_vec();
                v.extend_from_slice(&vec![b'#'; 8000]);
                v
            };
            let patch = delta::make_patch(&old_script, &new_script).unwrap();
            let result_sha = sha(&new_script);
            let patch_sig = sk.sign(&patch).to_bytes();
            let patch_sha = sha(&patch);

            let target = Target::current().expect("running on supported target");
            let mut manifest = Manifest {
                version: "1.1.0".into(),
                targets: vec![TargetManifest {
                    target,
                    full: Asset {
                        url: "http://unused/full".into(),
                        sha256: "0".repeat(64),
                        size: 999_999_999, // force delta pick
                        signature: STANDARD.encode([0u8; 64]),
                    },
                    patches: vec![PatchDescriptor {
                        from: "1.0.0".into(),
                        result_sha256: result_sha,
                        asset: Asset {
                            url: "PLACEHOLDER".into(),
                            sha256: patch_sha,
                            size: patch.len() as u64,
                            signature: STANDARD.encode(patch_sig),
                        },
                    }],
                }],
                notes: "".into(),
                published_at: "".into(),
                schema: 1,
            };

            let server = MockServer::start();
            manifest.targets[0].patches[0].asset.url = server.url("/delta.bin");
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
                when.method(GET).path("/delta.bin");
                then.status(200).body(patch);
            });

            let tmp = TempDir::new().unwrap();
            let dest = tmp.path().join("worklog");
            std::fs::write(&dest, &old_script).unwrap();
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&dest).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&dest, perms).unwrap();

            let req = UpdateRequest {
                manifest_url: server.url("/manifest.json"),
                current_binary: dest.clone(),
                current_version: "1.0.0".into(),
                work_dir: tmp.path().join("stage"),
                dry_run: false,
                force: false,
            };
            let report = run_update(&req).unwrap();
            assert!(report.used_delta, "delta asset must have been picked");
            assert!(!report.rolled_back);
            let out = std::process::Command::new(&dest)
                .arg("--version")
                .output()
                .unwrap();
            assert!(
                String::from_utf8_lossy(&out.stdout).contains("1.1.0"),
                "delta-applied binary should now print 1.1.0"
            );
        });
    }

    #[test]
    fn run_update_rejects_delta_with_mismatched_result_sha() {
        // Invariant: if the reconstructed binary's SHA doesn't match the
        // manifest's expected result_sha256, refuse. This catches
        // sideloaded builds + corrupted patches + wrong zstd levels.
        let _g = pubkey::test_env_lock();
        with_test_key(|sk| {
            let old = b"#!/bin/sh\necho 1.0.0\n".to_vec();
            let new = b"#!/bin/sh\necho 1.1.0\n".to_vec();
            let patch = delta::make_patch(&old, &new).unwrap();
            let patch_sig = sk.sign(&patch).to_bytes();
            let patch_sha = sha(&patch);

            let target = Target::current().unwrap();
            let mut manifest = Manifest {
                version: "1.1.0".into(),
                targets: vec![TargetManifest {
                    target,
                    full: Asset {
                        url: "http://unused".into(),
                        sha256: "0".repeat(64),
                        size: 999_999_999,
                        signature: STANDARD.encode([0u8; 64]),
                    },
                    patches: vec![PatchDescriptor {
                        from: "1.0.0".into(),
                        result_sha256: "0".repeat(64), // wrong on purpose
                        asset: Asset {
                            url: "PLACEHOLDER".into(),
                            sha256: patch_sha,
                            size: patch.len() as u64,
                            signature: STANDARD.encode(patch_sig),
                        },
                    }],
                }],
                notes: "".into(),
                published_at: "".into(),
                schema: 1,
            };
            let server = MockServer::start();
            manifest.targets[0].patches[0].asset.url = server.url("/d.bin");
            let mj = serde_json::to_vec(&manifest).unwrap();
            let ms = sk.sign(&mj).to_bytes();
            server.mock(|when, then| {
                when.method(GET).path("/m.json");
                then.status(200).body(mj);
            });
            server.mock(|when, then| {
                when.method(GET).path("/m.json.sig");
                then.status(200).body(ms.as_slice());
            });
            server.mock(|when, then| {
                when.method(GET).path("/d.bin");
                then.status(200).body(patch);
            });

            let tmp = TempDir::new().unwrap();
            let dest = tmp.path().join("worklog");
            std::fs::write(&dest, &old).unwrap();
            let req = UpdateRequest {
                manifest_url: server.url("/m.json"),
                current_binary: dest.clone(),
                current_version: "1.0.0".into(),
                work_dir: tmp.path().join("stage"),
                dry_run: false,
                force: false,
            };
            let err = run_update(&req).unwrap_err();
            assert!(
                format!("{err:#}").contains("result_sha256"),
                "expected result_sha256 mismatch error, got: {err:#}"
            );
            // Current binary must be untouched.
            assert_eq!(std::fs::read(&dest).unwrap(), old);
        });
    }

    #[test]
    fn decompress_zstd_rejects_decompression_bomb() {
        // Craft a small zstd-compressed payload that expands to more
        // than MAX_DECOMPRESSED_BYTES. Highly-compressible inputs like
        // a stream of zeros shrink ~1000x — so a ~256MB zero buffer
        // compresses to a few KB.
        let tmp = TempDir::new().unwrap();
        let over_cap = (MAX_DECOMPRESSED_BYTES as usize) + 4096;
        let zeros = vec![0u8; over_cap];
        let compressed = zstd::encode_all(std::io::Cursor::new(&zeros), 3).unwrap();
        assert!(
            compressed.len() < (over_cap / 100),
            "sanity: compressed should be <<1% of raw for a zero-filled input"
        );
        let src = tmp.path().join("bomb.zst");
        std::fs::write(&src, &compressed).unwrap();
        let dst = tmp.path().join("out.bin");

        let err = decompress_zstd(&src, &dst).unwrap_err();
        assert!(
            format!("{err:#}").contains("cap"),
            "expected cap error, got: {err:#}"
        );
        assert!(!dst.exists(), "partial output must be cleaned up");
    }

    #[test]
    fn decompress_zstd_accepts_normal_payload() {
        // Sanity check: the cap doesn't block legitimate payloads.
        let tmp = TempDir::new().unwrap();
        let payload = vec![42u8; 512 * 1024]; // 512KB
        let compressed = zstd::encode_all(std::io::Cursor::new(&payload), 3).unwrap();
        let src = tmp.path().join("ok.zst");
        std::fs::write(&src, &compressed).unwrap();
        let dst = tmp.path().join("out.bin");
        decompress_zstd(&src, &dst).unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), payload);
    }

    #[test]
    fn run_update_refuses_placeholder_pubkey() {
        // Fail-closed guard — force the placeholder via the test env
        // override and assert run_update refuses BEFORE any network
        // call. Without this, a binary built pre-keygen would accept
        // any forged signature. Real releases embed a non-placeholder
        // pubkey so the override is the only way to exercise this path.
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;
        let _g = pubkey::test_env_lock();
        let zeros = [0u8; crate::updater::crypto::PUBLIC_KEY_LEN];
        std::env::set_var("WORKLOG_RELEASE_PUBKEY_BASE64", STANDARD.encode(zeros));

        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("worklog");
        let req = UpdateRequest {
            // Unreachable URL — the point is we shouldn't even try.
            manifest_url: "http://127.0.0.1:1/never".into(),
            current_binary: dest,
            current_version: "0.1.0".into(),
            work_dir: tmp.path().join("stage"),
            dry_run: false,
            force: false,
        };
        let err = run_update(&req).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("placeholder"),
            "placeholder refusal message expected, got: {msg}"
        );
        std::env::remove_var("WORKLOG_RELEASE_PUBKEY_BASE64");
    }
}
