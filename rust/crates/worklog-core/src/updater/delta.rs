//! Binary delta patches via qbsdiff/qbspatch.
//!
//! `bsdiff(old, new)` produces a patch — typically a few hundred KB for a
//! 15MB Rust binary since most code is unchanged and the differ is
//! great at spotting reordered/rewritten sections. `bspatch(old, patch)`
//! reconstructs `new` deterministically.
//!
//! This module itself does NOT verify the reconstructed output. bspatch
//! is content-blind — applying a patch to the wrong `old` produces
//! garbage, not an error. The load-bearing integrity check lives in
//! `updater::run_update`, which hashes the output of `apply_patch` and
//! compares against the manifest's `PatchDescriptor::result_sha256`.
//! Without that check, a client whose `old` differs from the one the
//! patch was built against (sideload, different toolchain) would
//! silently install garbage.
//!
//! Patches produced by `worklog dev make-patch` are raw-to-raw (no
//! zstd). `run_update`'s delta branch applies them to the raw current
//! binary; full assets are the only zstd-wrapped artifacts.

use std::io::Cursor;

use anyhow::{Context, Result};
use qbsdiff::{Bsdiff, Bspatch};

/// Produce a delta patch from `old → new`. Used by `worklog dev release`.
pub fn make_patch(old: &[u8], new: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(new.len() / 20);
    Bsdiff::new(old, new)
        .compare(Cursor::new(&mut out))
        .context("bsdiff compare")?;
    Ok(out)
}

/// Upper bound on the reconstructed binary size. Defence-in-depth: the
/// patch is already Ed25519 + SHA256 verified, but if the signing key is
/// ever compromised, an attacker could craft a patch header claiming a
/// multi-GB target size. `Vec::with_capacity` would eagerly allocate
/// that much before any patching begins — an instant OOM primitive.
/// We cap the hint so that pathological headers just start smaller and
/// grow as needed.
pub const MAX_PATCH_TARGET_HINT: usize = 256 * 1024 * 1024;

/// Apply `patch` to `old`, producing the reconstructed bytes. Used by the
/// updater client.
pub fn apply_patch(old: &[u8], patch: &[u8]) -> Result<Vec<u8>> {
    let patcher = Bspatch::new(patch).context("parse bspatch header")?;
    let hint = (patcher.hint_target_size() as usize).min(MAX_PATCH_TARGET_HINT);
    let mut out = Vec::with_capacity(hint);
    patcher
        .apply(old, Cursor::new(&mut out))
        .context("bspatch apply")?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_small_patch() {
        let old = b"version 0.3.0\nhello world\nunchanged tail";
        let new = b"version 0.3.1\nhello world\nunchanged tail";
        let patch = make_patch(old, new).unwrap();
        let reconstructed = apply_patch(old, &patch).unwrap();
        assert_eq!(reconstructed, new);
    }

    #[test]
    fn delta_shrinks_large_mostly_unchanged_input() {
        // A realistic case: mostly-identical 100KB buffers differing in
        // a single version string. The delta should be a small fraction
        // of `new` — this is the whole reason we use bsdiff.
        let mut old = vec![0u8; 100_000];
        for (i, b) in old.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        let mut new = old.clone();
        new[..5].copy_from_slice(b"ver11"); // 5-byte diff
        new[50_000..50_005].copy_from_slice(b"HELLO");

        let patch = make_patch(&old, &new).unwrap();
        assert!(
            patch.len() < new.len() / 10,
            "delta should be <10% of new ({} vs {})",
            patch.len(),
            new.len()
        );
        assert_eq!(apply_patch(&old, &patch).unwrap(), new);
    }

    #[test]
    fn patch_is_binary_safe() {
        let old: Vec<u8> = (0u8..=255).collect();
        let mut new = old.clone();
        new[0] = 99;
        new[100] = 42;
        let patch = make_patch(&old, &new).unwrap();
        assert_eq!(apply_patch(&old, &patch).unwrap(), new);
    }

    #[test]
    fn applying_wrong_old_yields_wrong_output() {
        // bspatch doesn't verify old, so callers must hash the result.
        // We document that here: a mismatched `old` produces garbage, not
        // an error.
        let old = b"version 0.3.0";
        let new = b"version 0.3.1";
        let patch = make_patch(old, new).unwrap();
        let wrong_old = b"completely different starting point";
        // Not panicking is the invariant — silent garbage would then be
        // rejected by the SHA256 check in the updater.
        let _ = apply_patch(wrong_old, &patch);
    }
}
