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
pub mod install;
pub mod manifest;
pub mod pubkey;

pub use crypto::{sha256_hex, sign_detached, verify_detached, VerifyError};
pub use manifest::{Asset, Manifest, PatchDescriptor, Target};
