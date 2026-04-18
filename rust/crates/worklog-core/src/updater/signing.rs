//! Release-signing helpers — generate/load the Ed25519 private key,
//! write/read PKCS#8 PEM files, format the pubkey as a Rust array
//! literal. Lives in `worklog-core` so the CLI crate only needs to
//! speak at the "give me a key + file" level.

use std::path::Path;

use anyhow::{Context, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ed25519_dalek::pkcs8::spki::der::pem::LineEnding;
use ed25519_dalek::pkcs8::{DecodePrivateKey, EncodePrivateKey};
use ed25519_dalek::SigningKey;

use super::crypto::PUBLIC_KEY_LEN;

/// Generate a fresh Ed25519 keypair, write the private key as PKCS#8 PEM
/// to `path` (chmod 0600 on unix), and return `(raw_pubkey, rust_literal,
/// base64_pubkey)` — everything a caller needs to report to the user.
pub fn keygen_to_file(path: &Path, overwrite: bool) -> Result<GenResult> {
    if path.exists() && !overwrite {
        anyhow::bail!("{} already exists — pass --force to overwrite", path.display());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let (sk, vk) = super::crypto::generate_keypair();
    let pem = sk
        .to_pkcs8_pem(LineEnding::LF)
        .context("serialise private key to PEM")?;
    std::fs::write(path, pem.as_bytes()).with_context(|| format!("writing {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(path, perms)?;
    }

    let pub_bytes = vk.to_bytes();
    Ok(GenResult {
        public_key: pub_bytes,
        rust_literal: format_byte_array(&pub_bytes),
        base64: STANDARD.encode(pub_bytes),
    })
}

/// Parsed result of `keygen_to_file` — everything the CLI needs to
/// render a user-facing report.
#[derive(Debug)]
pub struct GenResult {
    pub public_key: [u8; PUBLIC_KEY_LEN],
    pub rust_literal: String,
    pub base64: String,
}

/// Load a signing key from a PKCS#8 PEM file.
pub fn load_signing_key(path: &Path) -> Result<SigningKey> {
    let pem = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    SigningKey::from_pkcs8_pem(&pem).context("parsing PKCS#8 PEM key")
}

/// Format a public key as a Rust array literal, formatted across 4 lines
/// to match rustfmt's default output for an 8-column wide byte array.
pub fn format_byte_array(bytes: &[u8]) -> String {
    let mut s = String::from("[\n    ");
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && i % 8 == 0 {
            s.push_str(",\n    ");
        } else if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&format!("0x{b:02x}"));
    }
    s.push_str(",\n]");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn keygen_writes_pkcs8_pem_and_reloads() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("key.pem");
        let gen = keygen_to_file(&path, false).unwrap();
        assert_eq!(gen.public_key.len(), PUBLIC_KEY_LEN);
        assert!(gen.rust_literal.starts_with('['));
        assert!(gen.rust_literal.contains("0x"));

        let pem = std::fs::read_to_string(&path).unwrap();
        assert!(pem.contains("-----BEGIN PRIVATE KEY-----"));

        let sk = load_signing_key(&path).unwrap();
        let reloaded_pub = sk.verifying_key().to_bytes();
        assert_eq!(reloaded_pub, gen.public_key);
    }

    #[test]
    fn keygen_refuses_to_overwrite_without_force() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("key.pem");
        keygen_to_file(&path, false).unwrap();
        let err = keygen_to_file(&path, false).unwrap_err();
        assert!(format!("{err:#}").contains("already exists"));
    }

    #[test]
    fn keygen_overwrites_with_force() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("key.pem");
        let a = keygen_to_file(&path, false).unwrap();
        let b = keygen_to_file(&path, true).unwrap();
        assert_ne!(a.public_key, b.public_key, "second keygen must produce a new key");
    }

    #[test]
    fn format_byte_array_matches_rustfmt_shape() {
        let bytes = [0u8; 32];
        let s = format_byte_array(&bytes);
        // 4 groups of 8, each comma-separated, wrapped with [ and ]
        assert!(s.starts_with("[\n    0x00, 0x00, 0x00, 0x00"));
        assert!(s.ends_with("0x00,\n]"));
        // 31 commas + 1 trailing comma = 32 total
        assert_eq!(s.matches("0x").count(), 32);
    }

    #[test]
    fn keygen_sets_permissions_0600() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("key.pem");
        keygen_to_file(&path, false).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
