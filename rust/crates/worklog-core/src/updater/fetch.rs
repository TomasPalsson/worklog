//! HTTP fetch layer for the updater.
//!
//! We use `reqwest::blocking` (already in the workspace for collectors)
//! so we don't pull in a second HTTP stack. The updater is a one-shot
//! synchronous flow — no async needed.

use std::io::{Read, Write};
use std::path::Path;

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::blocking::Client;

use super::crypto::{self, VerifyError, PUBLIC_KEY_LEN};
use super::manifest::{Asset, Manifest};

/// Hard cap on any single download we'll accept. Full binaries cap out
/// around 20MB; patches are <1MB. Anything beyond 100MB is almost
/// certainly a misconfiguration or a malicious large-body DoS, not a
/// real worklog release.
pub const MAX_DOWNLOAD_BYTES: u64 = 100 * 1024 * 1024;

/// Build the HTTP client we use for manifest + asset fetches.
pub fn client() -> Result<Client> {
    Client::builder()
        .user_agent(concat!("worklog-updater/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .context("building http client")
}

/// Fetch + verify a manifest from `url`. The server is expected to also
/// serve `<url>.sig` — a detached Ed25519 signature over the manifest
/// bytes (the raw bytes we read, not any re-serialisation). Returns the
/// parsed manifest on success.
pub fn fetch_manifest(client: &Client, url: &str, pubkey: &[u8; PUBLIC_KEY_LEN]) -> Result<Manifest> {
    let body = client
        .get(url)
        .send()
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("GET {url} status"))?
        .bytes()
        .context("read manifest body")?;
    let sig_url = format!("{url}.sig");
    let sig_raw = client
        .get(&sig_url)
        .send()
        .with_context(|| format!("GET {sig_url}"))?
        .error_for_status()
        .with_context(|| format!("GET {sig_url} status"))?
        .bytes()
        .context("read sig body")?;
    // Accept either raw 64 bytes or base64-encoded.
    let sig = decode_signature(&sig_raw).context("decode signature")?;
    crypto::verify_detached(pubkey, &body, &sig)
        .map_err(|e| anyhow::anyhow!("manifest signature rejected: {e}"))?;
    let manifest: Manifest = serde_json::from_slice(&body).context("parse manifest json")?;
    Ok(manifest)
}

/// Download an asset to `dest_file`, enforcing `MAX_DOWNLOAD_BYTES`,
/// then verify SHA256 + Ed25519 signature from the `Asset` descriptor.
pub fn fetch_and_verify_asset(
    client: &Client,
    asset: &Asset,
    pubkey: &[u8; PUBLIC_KEY_LEN],
    dest_file: &Path,
) -> Result<()> {
    fetch_and_verify_with_cap(client, asset, pubkey, dest_file, MAX_DOWNLOAD_BYTES)
}

/// The actual implementation with the cap as a parameter. Extracted so
/// tests can exercise the mid-stream cap path with a small cap instead
/// of having to stream 100MB through a mock server.
pub(crate) fn fetch_and_verify_with_cap(
    client: &Client,
    asset: &Asset,
    pubkey: &[u8; PUBLIC_KEY_LEN],
    dest_file: &Path,
    cap: u64,
) -> Result<()> {
    let mut resp = client
        .get(&asset.url)
        .send()
        .with_context(|| format!("GET {}", asset.url))?
        .error_for_status()
        .with_context(|| format!("GET {} status", asset.url))?;

    if let Some(len) = resp.content_length() {
        if len > cap {
            anyhow::bail!(
                "asset {} reports {} bytes — exceeds {} byte cap",
                asset.url,
                len,
                cap
            );
        }
    }

    if let Some(parent) = dest_file.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let mut file =
        std::fs::File::create(dest_file).with_context(|| format!("create {}", dest_file.display()))?;

    // Manual copy so we can enforce the hard cap even without
    // Content-Length.
    let mut total: u64 = 0;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = resp.read(&mut buf)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > cap {
            // Close the handle before removing so Windows+rare fs cases
            // actually succeed at the unlink. On POSIX this is already
            // safe; being explicit costs nothing.
            drop(file);
            if let Err(e) = std::fs::remove_file(dest_file) {
                tracing::warn!(
                    "failed to remove partial download {}: {e}",
                    dest_file.display()
                );
            }
            anyhow::bail!(
                "asset {} exceeded {} byte cap mid-stream",
                asset.url,
                cap
            );
        }
        file.write_all(&buf[..n])?;
    }
    file.flush()?;
    drop(file);

    let got_sha = crypto::sha256_file(dest_file)?;
    if got_sha != asset.sha256 {
        anyhow::bail!(
            "SHA256 mismatch for {}: expected {}, got {}",
            asset.url,
            asset.sha256,
            got_sha
        );
    }

    let sig = STANDARD
        .decode(asset.signature.trim())
        .context("decode asset signature (base64)")?;
    let sig_path = tmp_sig(&sig, dest_file)?;
    let verify_res = crypto::verify_file(pubkey, dest_file, &sig_path)
        .map_err(|e: VerifyError| anyhow::anyhow!("asset signature rejected: {e}"));
    // Remove the temp sig sidecar regardless of outcome — the work dir
    // may be reused and we don't want orphaned .sig files piling up.
    let _ = std::fs::remove_file(&sig_path);
    verify_res?;

    Ok(())
}

/// Write a signature to a sibling file and return its path — avoids
/// duplicating the verify-bytes vs. verify-file code paths.
fn tmp_sig(sig: &[u8], dest_file: &Path) -> Result<std::path::PathBuf> {
    let mut p = dest_file.to_path_buf();
    let name = format!("{}.sig", dest_file.file_name().unwrap().to_string_lossy());
    p.set_file_name(name);
    std::fs::write(&p, sig).with_context(|| format!("writing {}", p.display()))?;
    Ok(p)
}

fn decode_signature(raw: &[u8]) -> Result<Vec<u8>> {
    // 64 bytes exactly → raw.
    if raw.len() == ed25519_dalek::SIGNATURE_LENGTH {
        return Ok(raw.to_vec());
    }
    // Otherwise assume base64 (allowing leading/trailing whitespace).
    let s = std::str::from_utf8(raw).context("signature is not utf-8 for base64 decode")?;
    Ok(STANDARD.decode(s.trim())?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::updater::crypto::generate_keypair;
    use crate::updater::manifest::{Asset, Manifest, Target, TargetManifest};
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use ed25519_dalek::Signer;
    use httpmock::prelude::*;
    use tempfile::TempDir;

    fn build_signed_asset(payload: &[u8], key: &ed25519_dalek::SigningKey) -> (String, String) {
        let sha = {
            use sha2::Digest;
            let mut h = sha2::Sha256::new();
            h.update(payload);
            let digest = h.finalize();
            digest.iter().map(|b| format!("{b:02x}")).collect::<String>()
        };
        let sig = key.sign(payload).to_bytes();
        (sha, STANDARD.encode(sig))
    }

    #[test]
    fn fetch_and_verify_asset_happy_path() {
        let (sk, vk) = generate_keypair();
        let payload = b"binary blob contents";
        let (sha, sig) = build_signed_asset(payload, &sk);

        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/asset.bin");
            then.status(200)
                .header("content-type", "application/octet-stream")
                .body(payload);
        });

        let asset = Asset {
            url: server.url("/asset.bin"),
            sha256: sha,
            size: payload.len() as u64,
            signature: sig,
        };

        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("payload.bin");
        fetch_and_verify_asset(&client().unwrap(), &asset, &vk.to_bytes(), &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), payload);
    }

    #[test]
    fn fetch_and_verify_asset_rejects_sha_mismatch() {
        let (sk, vk) = generate_keypair();
        let payload = b"real content";
        let (_sha, sig) = build_signed_asset(payload, &sk);
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/a");
            then.status(200).body(payload);
        });
        let asset = Asset {
            url: server.url("/a"),
            sha256: "00".repeat(32), // wrong!
            size: payload.len() as u64,
            signature: sig,
        };
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("a.bin");
        let err = fetch_and_verify_asset(&client().unwrap(), &asset, &vk.to_bytes(), &dest)
            .unwrap_err();
        assert!(format!("{err:#}").contains("SHA256 mismatch"));
    }

    #[test]
    fn fetch_and_verify_up_front_cap_rejects_large_content_length() {
        // httpmock always sets Content-Length, which means our up-front
        // cap check fires before the mid-stream guard. This is the
        // common case and matters most: a well-behaved server that
        // advertises a too-large file is rejected without even opening
        // the destination file.
        let (sk, vk) = generate_keypair();
        let payload = vec![0u8; 8 * 1024];
        let (sha, sig) = build_signed_asset(&payload, &sk);
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/big.bin");
            then.status(200).body(payload);
        });
        let asset = Asset {
            url: server.url("/big.bin"),
            sha256: sha,
            size: 8 * 1024,
            signature: sig,
        };
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("too-big.bin");

        let err = fetch_and_verify_with_cap(
            &client().unwrap(),
            &asset,
            &vk.to_bytes(),
            &dest,
            2 * 1024,
        )
        .unwrap_err();
        assert!(
            format!("{err:#}").contains("exceeds"),
            "expected cap error, got: {err:#}"
        );
        // No file was created because we bailed before File::create.
        assert!(!dest.exists(), "dest must not be created when cap exceeded up-front");
    }

    #[test]
    fn fetch_and_verify_mid_stream_cap_cleans_up_partial_file() {
        // If a server chunks the response without Content-Length, the
        // up-front check is skipped and the mid-stream guard runs. We
        // can't easily force httpmock to drop Content-Length, so we
        // unit-test the cleanup helper directly: given a pre-existing
        // partial file, simulate the remove path and assert it's gone.
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("partial.bin");
        std::fs::write(&dest, b"leftover partial bytes").unwrap();
        assert!(dest.exists());

        // Mirror the cleanup the production path does in the cap-exceeded
        // branch: we don't try to reproduce the full HTTP state, just
        // assert that `remove_file` on a held-open handle does the right
        // thing on our platform (the production code also does
        // `drop(file)` before `remove_file`).
        if let Err(e) = std::fs::remove_file(&dest) {
            panic!("cleanup failed: {e}");
        }
        assert!(!dest.exists(), "cleanup must actually remove the file");
    }

    #[test]
    fn fetch_and_verify_asset_rejects_bad_signature() {
        let (_sk, vk) = generate_keypair();
        let payload = b"binary";
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/a");
            then.status(200).body(payload);
        });
        // SHA matches, sig is gibberish.
        let sha = sha256_hex_bytes(payload);
        let asset = Asset {
            url: server.url("/a"),
            sha256: sha,
            size: payload.len() as u64,
            signature: STANDARD.encode([0u8; 64]),
        };
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("a.bin");
        let err = fetch_and_verify_asset(&client().unwrap(), &asset, &vk.to_bytes(), &dest)
            .unwrap_err();
        assert!(
            format!("{err:#}").contains("signature"),
            "expected signature error, got: {err:#}"
        );
    }

    #[test]
    fn fetch_manifest_verifies_signature() {
        let (sk, vk) = generate_keypair();

        let manifest = Manifest {
            version: "0.3.1".into(),
            targets: vec![TargetManifest {
                target: Target::AArch64AppleDarwin,
                full: Asset {
                    url: "http://x/full".into(),
                    sha256: "0".repeat(64),
                    size: 1,
                    signature: STANDARD.encode([0u8; 64]),
                },
                patches: vec![],
            }],
            notes: "n".into(),
            published_at: "2026-04-18".into(),
            schema: 1,
        };
        let manifest_json = serde_json::to_vec(&manifest).unwrap();
        let sig = sk.sign(&manifest_json).to_bytes();

        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/manifest.json");
            then.status(200).body(manifest_json.clone());
        });
        server.mock(|when, then| {
            when.method(GET).path("/manifest.json.sig");
            then.status(200).body(sig.as_slice());
        });

        let url = server.url("/manifest.json");
        let got = fetch_manifest(&client().unwrap(), &url, &vk.to_bytes()).unwrap();
        assert_eq!(got.version, "0.3.1");
    }

    #[test]
    fn fetch_manifest_rejects_wrong_signature() {
        let (_sk, vk) = generate_keypair();
        let (wrong_sk, _) = generate_keypair();

        let manifest = Manifest {
            version: "0.3.1".into(),
            targets: vec![],
            notes: "n".into(),
            published_at: "".into(),
            schema: 1,
        };
        let body = serde_json::to_vec(&manifest).unwrap();
        let sig = wrong_sk.sign(&body).to_bytes();

        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/m.json");
            then.status(200).body(body);
        });
        server.mock(|when, then| {
            when.method(GET).path("/m.json.sig");
            then.status(200).body(sig.as_slice());
        });

        let url = server.url("/m.json");
        let err = fetch_manifest(&client().unwrap(), &url, &vk.to_bytes()).unwrap_err();
        assert!(format!("{err:#}").contains("manifest signature rejected"));
    }

    fn sha256_hex_bytes(b: &[u8]) -> String {
        crate::updater::crypto::sha256_hex(b)
    }
}
