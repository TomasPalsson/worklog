//! Release manifest — a JSON document describing what's available for
//! a given version + target, signed as a whole by the release author.
//!
//! One manifest per release, hosted at a stable URL like
//! `https://github.com/<user>/worklog/releases/latest/download/manifest.json`.
//! The client fetches + verifies the signature, matches its current
//! state against the manifest, and decides whether to apply a delta patch
//! or download the full binary.

use std::fmt;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// The top-level release descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Semver-ish version string — matches the tag, e.g. `"0.3.1"`.
    pub version: String,

    /// Target triple for the assets listed here. A single release ships
    /// one manifest per target (or merges them, but each `Target` is
    /// listed separately in `targets[]`).
    pub targets: Vec<TargetManifest>,

    /// Release notes, in plain text. Shown to the user before applying.
    #[serde(default)]
    pub notes: String,

    /// When the manifest was written (ISO-8601). Used in logs/output.
    #[serde(default)]
    pub published_at: String,

    /// Schema version — bump when we make incompatible changes so older
    /// binaries can print a clear "please upgrade manually" error.
    #[serde(default = "default_schema")]
    pub schema: u32,
}

fn default_schema() -> u32 {
    1
}

pub const CURRENT_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetManifest {
    pub target: Target,
    /// Full standalone binary tarball (zstd-compressed binary). Always
    /// present — delta patches are optional.
    pub full: Asset,
    /// Delta patches indexed by the `from` version. Clients look up
    /// their current version; miss → fall back to `full`.
    #[serde(default)]
    pub patches: Vec<PatchDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    /// HTTPS URL the client will `GET`.
    pub url: String,
    /// SHA256 of the fetched bytes (lowercase hex, no prefix).
    pub sha256: String,
    /// Bytes on the wire — for the UI progress bar.
    pub size: u64,
    /// Detached Ed25519 signature, base64-encoded.
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchDescriptor {
    /// Version this patch starts from.
    pub from: String,
    /// SHA256 of the RECONSTRUCTED binary (i.e. `bspatch(old, patch)`).
    /// Verified after applying the patch. This is the load-bearing
    /// integrity check for the delta path — `asset.sha256` only covers
    /// the patch bytes themselves, not the reconstructed output.
    ///
    /// Defaults to empty string for forward compatibility with older
    /// manifests (pre-qa-phase-2) that didn't include this field. An
    /// empty value causes `run_update` to reject the delta and fall
    /// back to the full-binary asset — safer than silently skipping
    /// the check.
    #[serde(default)]
    pub result_sha256: String,
    #[serde(flatten)]
    pub asset: Asset,
}

/// The target triples we publish. Keep this small — one per platform
/// the author actually uses. Others fall back to full-binary downloads
/// from the GitHub release page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Target {
    #[serde(rename = "aarch64-apple-darwin")]
    AArch64AppleDarwin,
    #[serde(rename = "x86_64-apple-darwin")]
    X86_64AppleDarwin,
    #[serde(rename = "aarch64-unknown-linux-gnu")]
    AArch64UnknownLinuxGnu,
    #[serde(rename = "x86_64-unknown-linux-gnu")]
    X86_64UnknownLinuxGnu,
}

/// The build target isn't one of the published ones. Carries the
/// compile-time arch + os so the error message names them.
#[derive(Debug, thiserror::Error)]
#[error("unsupported target: {arch}-{os} — no signed release published for it. \
         Either build + install from source, or ask the maintainer to add \
         this target to the release matrix.")]
pub struct UnsupportedTarget {
    pub arch: &'static str,
    pub os: &'static str,
}

impl Target {
    /// The target triple of the running binary, as published in
    /// manifests. Returns an error (naming arch + os) on an unsupported
    /// build target — the type system prevents callers from accidentally
    /// defaulting to a specific target, which would silently install
    /// the wrong binary.
    pub const fn current() -> Result<Self, UnsupportedTarget> {
        #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
        {
            return Ok(Target::AArch64AppleDarwin);
        }
        #[cfg(all(target_arch = "x86_64", target_os = "macos"))]
        {
            return Ok(Target::X86_64AppleDarwin);
        }
        #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
        {
            return Ok(Target::AArch64UnknownLinuxGnu);
        }
        #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
        {
            return Ok(Target::X86_64UnknownLinuxGnu);
        }
        #[allow(unreachable_code)]
        Err(UnsupportedTarget {
            arch: std::env::consts::ARCH,
            os: std::env::consts::OS,
        })
    }

    pub const fn triple(self) -> &'static str {
        match self {
            Target::AArch64AppleDarwin => "aarch64-apple-darwin",
            Target::X86_64AppleDarwin => "x86_64-apple-darwin",
            Target::AArch64UnknownLinuxGnu => "aarch64-unknown-linux-gnu",
            Target::X86_64UnknownLinuxGnu => "x86_64-unknown-linux-gnu",
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.triple())
    }
}

impl Manifest {
    /// Find the target-specific section for the running binary. Returns
    /// `None` if this binary was built for a target we don't publish
    /// assets for OR the manifest simply doesn't include a section for
    /// this target.
    pub fn for_current_target(&self) -> Option<&TargetManifest> {
        let t = Target::current().ok()?;
        self.targets.iter().find(|tm| tm.target == t)
    }

    /// Choose the best asset for `(from_version → self.version)` on the
    /// current target. Prefers a delta patch starting from `from_version`
    /// over the full binary if one exists and the patch is smaller.
    pub fn pick_asset(&self, from_version: &str) -> Option<Choice<'_>> {
        let tm = self.for_current_target()?;
        if let Some(p) = tm.patches.iter().find(|p| p.from == from_version) {
            if p.asset.size < tm.full.size {
                return Some(Choice::Delta(p));
            }
        }
        Some(Choice::Full(&tm.full))
    }

    /// Canonical bytes over which the manifest signature is computed.
    /// We pretty-print with sorted keys so independent implementations
    /// (e.g. a CI signing tool) produce the same bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        // serde_json doesn't sort keys by default; use a BTreeMap round-trip.
        let v = serde_json::to_value(self)?;
        let sorted = sort_keys(v);
        Ok(serde_json::to_vec_pretty(&sorted)?)
    }
}

fn sort_keys(v: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match v {
        Value::Object(m) => {
            let btm: std::collections::BTreeMap<_, _> =
                m.into_iter().map(|(k, v)| (k, sort_keys(v))).collect();
            serde_json::to_value(btm).unwrap_or(Value::Null)
        }
        Value::Array(a) => Value::Array(a.into_iter().map(sort_keys).collect()),
        other => other,
    }
}

/// Which kind of asset the updater is about to apply.
#[derive(Debug)]
pub enum Choice<'a> {
    Full(&'a Asset),
    Delta(&'a PatchDescriptor),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_asset(size: u64) -> Asset {
        Asset {
            url: "https://example.com/a".into(),
            sha256: "0".repeat(64),
            size,
            signature: "sig".into(),
        }
    }

    fn sample_manifest(from_versions: &[&str]) -> Manifest {
        Manifest {
            version: "0.3.1".into(),
            targets: vec![TargetManifest {
                target: Target::current().unwrap_or(Target::AArch64AppleDarwin), // test-only default
                full: sample_asset(15_000_000),
                patches: from_versions
                    .iter()
                    .map(|v| PatchDescriptor {
                        from: (*v).to_string(),
                        result_sha256: "0".repeat(64),
                        asset: sample_asset(120_000),
                    })
                    .collect(),
            }],
            notes: "release notes".into(),
            published_at: "2026-04-18T00:00:00Z".into(),
            schema: 1,
        }
    }

    #[test]
    fn patch_descriptor_parses_old_manifest_without_result_sha256() {
        // Forward compatibility: manifests signed before the
        // result_sha256 field was added must still parse. The default
        // is empty string, which `run_update` treats as "refuse this
        // delta and let the user retry with --force for full".
        let raw = r#"{
            "from": "0.3.0",
            "url": "http://example.com/patch.bin",
            "sha256": "aa",
            "size": 100,
            "signature": "sig"
        }"#;
        let pd: PatchDescriptor = serde_json::from_str(raw).unwrap();
        assert_eq!(pd.from, "0.3.0");
        assert_eq!(pd.result_sha256, "");
    }

    #[test]
    fn round_trips_as_json() {
        let m = sample_manifest(&["0.3.0"]);
        let bytes = serde_json::to_vec(&m).unwrap();
        let back: Manifest = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back.version, "0.3.1");
        assert_eq!(back.targets.len(), 1);
        assert_eq!(back.targets[0].patches[0].from, "0.3.0");
    }

    #[test]
    fn pick_asset_prefers_delta_when_smaller() {
        let m = sample_manifest(&["0.3.0"]);
        if Target::current().is_err() {
            return; // Skip on unsupported build targets.
        }
        match m.pick_asset("0.3.0").unwrap() {
            Choice::Delta(p) => assert_eq!(p.from, "0.3.0"),
            Choice::Full(_) => panic!("should have picked delta"),
        }
    }

    #[test]
    fn pick_asset_falls_back_to_full_when_no_matching_patch() {
        let m = sample_manifest(&["0.3.0"]);
        if Target::current().is_err() {
            return;
        }
        assert!(matches!(m.pick_asset("99.0.0"), Some(Choice::Full(_))));
    }

    #[test]
    fn pick_asset_falls_back_to_full_when_delta_bigger() {
        // Construct a manifest where the 'delta' is somehow bigger than
        // the full binary. Not realistic, but we want the client to
        // always pick the cheaper transfer.
        let mut m = sample_manifest(&["0.3.0"]);
        m.targets[0].patches[0].asset.size = 999_999_999;
        m.targets[0].full.size = 1_000;
        if Target::current().is_err() {
            return;
        }
        assert!(matches!(m.pick_asset("0.3.0"), Some(Choice::Full(_))));
    }

    #[test]
    fn canonical_bytes_are_stable() {
        let m = sample_manifest(&["0.3.0"]);
        let a = m.canonical_bytes().unwrap();
        let b = m.canonical_bytes().unwrap();
        assert_eq!(a, b);
        // Sanity check: sorted output starts with `{` and contains the
        // version field.
        let s = String::from_utf8(a).unwrap();
        assert!(s.starts_with('{'));
        assert!(s.contains("\"version\": \"0.3.1\""));
    }

    #[test]
    fn target_triples_stable() {
        assert_eq!(Target::AArch64AppleDarwin.triple(), "aarch64-apple-darwin");
        assert_eq!(
            Target::X86_64UnknownLinuxGnu.triple(),
            "x86_64-unknown-linux-gnu"
        );
    }

    #[test]
    fn schema_has_a_default_version() {
        // An old-format manifest without `schema` should still parse.
        let raw = r#"{
            "version": "0.1.0",
            "targets": [],
            "notes": "hi"
        }"#;
        let m: Manifest = serde_json::from_str(raw).unwrap();
        assert_eq!(m.schema, 1);
    }
}
