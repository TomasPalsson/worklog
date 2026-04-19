#!/usr/bin/env bash
# Smoke tests for install.sh. Runs the script with --dry-run in a few
# scenarios and asserts the output; doesn't hit the network or touch
# $HOME/.local/bin.
#
# Usage: bash tests/install/smoke.sh

set -euo pipefail
cd "$(dirname "$0")/../.."

INSTALLER=install.sh
PASS=0
FAIL=0

assert_contains() {
  local label=$1 needle=$2 haystack=$3
  if printf '%s' "$haystack" | grep -q -F -- "$needle"; then
    printf '  ✓ %s\n' "$label"
    PASS=$((PASS + 1))
  else
    printf '  ✗ %s\n    expected substring: %s\n    got: %s\n' \
      "$label" "$needle" "$haystack"
    FAIL=$((FAIL + 1))
  fi
}

assert_exit() {
  local label=$1 expected=$2 actual=$3
  if [ "$expected" = "$actual" ]; then
    printf '  ✓ %s (exit %s)\n' "$label" "$actual"
    PASS=$((PASS + 1))
  else
    printf '  ✗ %s — expected exit %s, got %s\n' "$label" "$expected" "$actual"
    FAIL=$((FAIL + 1))
  fi
}

echo "─── installer smoke tests ──────────────────────────────────────"

# 1) --help prints usage and exits 0.
OUT=$(bash "$INSTALLER" --help 2>&1) || true
RC=$?
assert_exit "--help exits 0" "0" "$RC"
assert_contains "--help mentions Usage" "Usage:" "$OUT"
assert_contains "--help lists --dry-run" "--dry-run" "$OUT"
assert_contains "--help lists --version" "--version" "$OUT"

# 2) Unknown flag → exit 2.
set +e
OUT=$(bash "$INSTALLER" --nonsense 2>&1)
RC=$?
set -e
assert_exit "unknown flag exits 2" "2" "$RC"
assert_contains "unknown flag names the flag" "unknown flag: --nonsense" "$OUT"

# 3) --dry-run --force produces target + url without touching disk.
OUT=$(bash "$INSTALLER" --dry-run --force 2>&1)
assert_contains "--dry-run reports target detected" "target detected:" "$OUT"
assert_contains "--dry-run prints source URL" "https://github.com/" "$OUT"
assert_contains "--dry-run ends cleanly" "dry-run" "$OUT"

# 4) Custom --version formats the URL correctly.
OUT=$(bash "$INSTALLER" --dry-run --force --version v0.3.1 2>&1)
assert_contains "--version uses the tag in the URL" "releases/download/v0.3.1/" "$OUT"

# 5) Custom --prefix is honored.
OUT=$(bash "$INSTALLER" --dry-run --force --prefix /tmp/wl-test-prefix 2>&1)
assert_contains "--prefix honored in dest" "/tmp/wl-test-prefix/worklog" "$OUT"

echo
if [ "$FAIL" -eq 0 ]; then
  printf '✓ install smoke: %d passed\n' "$PASS"
  exit 0
fi
printf '✗ install smoke: %d passed, %d failed\n' "$PASS" "$FAIL"
exit 1
