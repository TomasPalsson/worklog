//! Crypto primitives — Ed25519 signing + verification and SHA256 hashing.
//!
//! We keep the public surface tiny: sign/verify a detached signature over
//! arbitrary bytes, and compute a SHA256 hash as lowercase hex. Both are
//! wrappers over battle-tested crates (`ed25519-dalek` + `sha2`) — there's
//! no bespoke crypto here.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use anyhow::{Context, Result};
use ed25519_dalek::{Signature, SigningKey, Verifier, VerifyingKey, SIGNATURE_LENGTH};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Length of an Ed25519 public key in bytes. Matches
/// `ed25519_dalek::PUBLIC_KEY_LENGTH`.
pub const PUBLIC_KEY_LEN: usize = 32;

/// A detached Ed25519 signature as raw bytes.
pub type DetachedSignature = [u8; SIGNATURE_LENGTH];

#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("signature length {0} does not match expected {SIGNATURE_LENGTH}")]
    BadSignatureLength(usize),
    #[error("public key length {0} does not match expected {PUBLIC_KEY_LEN}")]
    BadPubkeyLength(usize),
    #[error("signature verification failed — file may be tampered or signed with a different key")]
    Mismatch,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Sign `message` with a raw 32-byte Ed25519 signing key seed. The
/// returned signature is 64 bytes.
pub fn sign_detached(seed: &[u8; 32], message: &[u8]) -> DetachedSignature {
    let sk = SigningKey::from_bytes(seed);
    use ed25519_dalek::Signer;
    sk.sign(message).to_bytes()
}

/// Verify a detached signature. Returns `Ok(())` on success.
pub fn verify_detached(
    pubkey: &[u8; PUBLIC_KEY_LEN],
    message: &[u8],
    signature: &[u8],
) -> std::result::Result<(), VerifyError> {
    if signature.len() != SIGNATURE_LENGTH {
        return Err(VerifyError::BadSignatureLength(signature.len()));
    }
    let vk = VerifyingKey::from_bytes(pubkey).map_err(|_| VerifyError::Mismatch)?;
    let sig_bytes: [u8; SIGNATURE_LENGTH] = signature
        .try_into()
        .map_err(|_| VerifyError::BadSignatureLength(signature.len()))?;
    let sig = Signature::from_bytes(&sig_bytes);
    vk.verify(message, &sig).map_err(|_| VerifyError::Mismatch)
}

/// Convenience: verify a file against a detached signature file.
pub fn verify_file(
    pubkey: &[u8; PUBLIC_KEY_LEN],
    file: &Path,
    sig: &Path,
) -> std::result::Result<(), VerifyError> {
    let msg = std::fs::read(file)?;
    let signature = std::fs::read(sig)?;
    verify_detached(pubkey, &msg, &signature)
}

/// Generate a fresh Ed25519 keypair. Returns `(seed, pubkey)` as raw
/// bytes suitable for PEM serialisation via `ed25519_dalek::pkcs8`.
pub fn generate_keypair() -> (SigningKey, VerifyingKey) {
    use rand::rngs::OsRng;
    let sk = SigningKey::generate(&mut OsRng);
    let vk = sk.verifying_key();
    (sk, vk)
}

/// SHA256 a byte buffer, return lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex_encode(&h.finalize())
}

/// Stream SHA256 over a file (constant memory). Returns lowercase hex.
pub fn sha256_file(path: &Path) -> Result<String> {
    let f = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut reader = BufReader::new(f);
    let mut h = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex_encode(&h.finalize()))
}

/// Lowercase hex without allocation overhead — simple byte-by-byte.
fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_then_verify_roundtrip() {
        let (sk, vk) = generate_keypair();
        let msg = b"release manifest body";
        let sig = sign_detached(&sk.to_bytes(), msg);
        assert!(verify_detached(&vk.to_bytes(), msg, &sig).is_ok());
    }

    #[test]
    fn tampered_message_fails_verification() {
        let (sk, vk) = generate_keypair();
        let sig = sign_detached(&sk.to_bytes(), b"hello");
        let err = verify_detached(&vk.to_bytes(), b"world", &sig).unwrap_err();
        assert!(matches!(err, VerifyError::Mismatch));
    }

    #[test]
    fn different_key_rejected() {
        let (sk1, _) = generate_keypair();
        let (_, vk2) = generate_keypair();
        let sig = sign_detached(&sk1.to_bytes(), b"hi");
        let err = verify_detached(&vk2.to_bytes(), b"hi", &sig).unwrap_err();
        assert!(matches!(err, VerifyError::Mismatch));
    }

    #[test]
    fn sha256_hex_matches_known_vector() {
        // sha256("") known vector
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sha256_file_streams_identically_to_in_memory() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"hello world").unwrap();
        assert_eq!(sha256_file(tmp.path()).unwrap(), sha256_hex(b"hello world"));
    }

    #[test]
    fn verify_file_against_sig_file() {
        let (sk, vk) = generate_keypair();
        let tmp_msg = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp_msg.path(), b"asset bytes").unwrap();
        let sig = sign_detached(&sk.to_bytes(), b"asset bytes");
        let tmp_sig = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp_sig.path(), sig).unwrap();
        assert!(verify_file(&vk.to_bytes(), tmp_msg.path(), tmp_sig.path()).is_ok());
    }

    #[test]
    fn bad_sig_length_is_explicit_error() {
        let (_, vk) = generate_keypair();
        let err = verify_detached(&vk.to_bytes(), b"x", &[0u8; 10]).unwrap_err();
        assert!(matches!(err, VerifyError::BadSignatureLength(10)));
    }
}
