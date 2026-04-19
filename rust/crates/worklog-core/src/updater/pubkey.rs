//! Embedded release public key.
//!
//! This is the Ed25519 public key whose matching private key is held by
//! the release author. Every release manifest + asset must be signed
//! with that private key; the updater refuses anything else.
//!
//! **How to set your key:**
//! 1. Run `worklog dev keygen` on the release-signing machine.
//! 2. Copy the `pub const RELEASE_PUBLIC_KEY: [u8; 32] = …` line it
//!    prints into this file, replacing the placeholder below.
//! 3. Commit the change; distribute the new binary.
//!
//! The placeholder starts as all zeros so the updater will reject every
//! real signature until a genuine key is embedded — a fail-closed
//! default that keeps a freshly-built binary from accepting unverified
//! downloads.
//!
//! **Test-only override:** in `#[cfg(test)]` builds, setting
//! `$WORKLOG_RELEASE_PUBKEY_BASE64` overrides the embedded const so
//! integration tests can inject a fresh keypair. The override is
//! compiled out of release binaries entirely — a production process
//! cannot swap the pubkey via an env var, closing a supply-chain gap
//! where a post-install hook or misconfigured shell could substitute
//! an attacker-controlled key.

use super::crypto::PUBLIC_KEY_LEN;

/// Embedded public key. Replace this with your own by running
/// `worklog dev keygen` and pasting the printed constant.
pub const RELEASE_PUBLIC_KEY: [u8; PUBLIC_KEY_LEN] = [0u8; PUBLIC_KEY_LEN];

/// Resolve the pubkey at runtime. In release builds this always returns
/// the compiled-in constant. In test builds the
/// `$WORKLOG_RELEASE_PUBKEY_BASE64` env var can override the const so
/// integration tests can verify signatures with a freshly-generated
/// keypair without mutating source.
pub fn resolve() -> [u8; PUBLIC_KEY_LEN] {
    #[cfg(test)]
    {
        use base64::{engine::general_purpose::STANDARD, Engine};
        if let Ok(b64) = std::env::var("WORKLOG_RELEASE_PUBKEY_BASE64") {
            if let Ok(bytes) = STANDARD.decode(b64.trim()) {
                if let Ok(arr) = <[u8; PUBLIC_KEY_LEN]>::try_from(bytes) {
                    return arr;
                }
            }
        }
    }
    RELEASE_PUBLIC_KEY
}

/// True iff the resolved pubkey is the all-zero placeholder. The updater
/// refuses to run against it — verification against an all-zero key
/// accepts degenerate signatures and rolling out an updater that
/// "verifies" by coincidence would be worse than useless.
pub fn is_placeholder() -> bool {
    resolve() == [0u8; PUBLIC_KEY_LEN]
}

/// Crate-internal mutex so tests that touch $WORKLOG_RELEASE_PUBKEY_BASE64
/// serialise — otherwise parallel tests leak each other's env state.
/// Uses a poison-tolerant lock so a panicked earlier test doesn't take
/// down every test that follows.
#[cfg(test)]
pub(crate) fn test_env_lock() -> std::sync::MutexGuard<'static, ()> {
    use std::sync::{Mutex, OnceLock};
    static L: OnceLock<Mutex<()>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    #[test]
    fn embedded_pubkey_is_real_not_placeholder() {
        // Invariant flip: once a real keypair is generated and embedded,
        // the all-zero placeholder must never ship again. If this test
        // regresses, someone committed the placeholder — that's a
        // supply-chain tripwire, not a nuisance.
        let _g = test_env_lock();
        std::env::remove_var("WORKLOG_RELEASE_PUBKEY_BASE64");
        assert!(
            !is_placeholder(),
            "pubkey.rs still has the all-zero placeholder — \
             generate a release keypair and embed its RELEASE_PUBLIC_KEY const"
        );
    }

    #[test]
    fn env_override_takes_precedence() {
        let _g = test_env_lock();
        let key = [7u8; PUBLIC_KEY_LEN];
        std::env::set_var("WORKLOG_RELEASE_PUBKEY_BASE64", STANDARD.encode(key));
        assert_eq!(resolve(), key);
        assert!(!is_placeholder());
        std::env::remove_var("WORKLOG_RELEASE_PUBKEY_BASE64");
    }

    #[test]
    fn bad_env_falls_back_to_embedded() {
        let _g = test_env_lock();
        std::env::set_var("WORKLOG_RELEASE_PUBKEY_BASE64", "not base64!!!");
        assert_eq!(resolve(), RELEASE_PUBLIC_KEY);
        std::env::remove_var("WORKLOG_RELEASE_PUBKEY_BASE64");
    }
}
