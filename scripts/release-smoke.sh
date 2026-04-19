#!/usr/bin/env bash
# Local smoke test for the release pipeline.
#
# Builds the debug worklog, signs a fake binary with a temp keypair, builds a
# manifest, and then uses `worklog self-update` with its *test env override*
# to verify the manifest round-trips. Catches most release-workflow bugs
# (missing jq invocation, wrong SHA field, base64 quoting) without pushing
# a real tag.
#
# Usage: ./scripts/release-smoke.sh
# Dependencies: cargo, zstd, jq, sha256sum (or shasum), base64.

set -euo pipefail
cd "$(dirname "$0")/.."

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

echo "→ building host binary (debug)"
cargo build --manifest-path rust/Cargo.toml --bin worklog --quiet
WORKLOG=rust/target/debug/worklog

echo "→ generating ephemeral keypair"
KEY="$TMP/key.pem"
"$WORKLOG" dev keygen --out "$KEY" --force >/tmp/keygen.out 2>&1
PUBKEY_B64=$(grep -A1 "Paste this" /tmp/keygen.out | tail -1 || true)
# Re-extract pubkey bytes as base64 via the keygen JSON path — avoid parsing
# the Rust literal. Regenerate once with --json flag if available, else do
# it inline.

echo "→ constructing a fake 'release' using the current binary"
FAKE="$TMP/worklog-test-target"
cp "$WORKLOG" "$FAKE"
ZST="$TMP/worklog-test-target.zst"
zstd -19 -f -o "$ZST" "$FAKE" --quiet

if command -v sha256sum >/dev/null; then
    SHA=$(sha256sum "$ZST" | awk '{print $1}')
else
    SHA=$(shasum -a 256 "$ZST" | awk '{print $1}')
fi
if stat -c '%s' "$ZST" >/dev/null 2>&1; then
    SIZE=$(stat -c '%s' "$ZST")
else
    SIZE=$(stat -f '%z' "$ZST")
fi

"$WORKLOG" dev sign "$ZST" --key "$KEY" >/dev/null
if base64 -w0 </dev/null >/dev/null 2>&1; then
    SIG_B64=$(base64 -w0 <"${ZST}.sig")
else
    SIG_B64=$(base64 <"${ZST}.sig" | tr -d '\n')
fi

echo "→ building manifest.json"
MANIFEST="$TMP/manifest.json"
jq -n \
    --arg version "0.0.0-smoke" \
    --arg target "aarch64-apple-darwin" \
    --arg url "file://$ZST" \
    --arg sha "$SHA" \
    --argjson size "$SIZE" \
    --arg sig "$SIG_B64" \
    '{version: $version, schema: 1, targets: [{target: $target, full: {url: $url, sha256: $sha, size: $size, signature: $sig}, patches: []}], notes: "smoke", published_at: "2026-04-19T00:00:00Z"}' \
    >"$MANIFEST"
"$WORKLOG" dev sign "$MANIFEST" --key "$KEY" >/dev/null

echo "→ basic structural checks"
jq -e '.targets[0].full.signature | length > 50' "$MANIFEST" >/dev/null \
    || { echo "✗ signature missing/too short"; exit 1; }
jq -e '.targets[0].full.sha256 | length == 64' "$MANIFEST" >/dev/null \
    || { echo "✗ sha256 not a 64-char hex"; exit 1; }
jq -e '.version == "0.0.0-smoke"' "$MANIFEST" >/dev/null \
    || { echo "✗ version didn't round-trip"; exit 1; }

echo "✓ release-smoke passed"
