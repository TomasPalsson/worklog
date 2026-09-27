#!/usr/bin/env bash
# Real-data check for the block-building rules (R1-R8, spec
# block-clues-and-detail-panel).
#
# Copies the LIVE worklog.db (read-only, via `sqlite3 .backup`) into a
# scratch WORKLOG_HOME, rebuilds 2026-09-21..2026-09-27 against that COPY
# with the release binary, and prints per-day block/hour stats plus the
# 2026-09-25 vitinn-infra (Sjukra) vs everything-else-work (APRO) split.
# Never touches the live database.
#
# Asserts:
#   * 2026-09-25 work hours in [8.35, 8.65] (owner: ~8.5 h)
#   * 2026-09-25 vitinn-infra hours in [1.9, 2.2]
#   * at most 2 blocks < 10 min on 2026-09-25
#   * no block overlaps 2026-09-23 03:00-04:30
#   * no block overlaps 2026-09-26 22:00-23:10
#   * the block covering 2026-09-27 03:03-08:11 is personal
#
# Usage: bash scripts/verify-inference.sh
# Dependencies: cargo, sqlite3, python3.

set -euo pipefail
cd "$(dirname "$0")/.."

SRC_DB="$HOME/.local/share/worklog/worklog.db"
if [ ! -f "$SRC_DB" ]; then
    echo "no live db at $SRC_DB — nothing to verify against" >&2
    exit 1
fi

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
sqlite3 "$SRC_DB" ".backup '$TMP/worklog.db'"
export WORKLOG_HOME="$TMP"

echo "→ building release binary"
cargo build --manifest-path rust/Cargo.toml --release --bin worklog --quiet
WORKLOG=rust/target/release/worklog

echo "→ migrating the copy"
"$WORKLOG" db migrate >/dev/null

DAYS="2026-09-21 2026-09-22 2026-09-23 2026-09-24 2026-09-25 2026-09-26 2026-09-27"
for day in $DAYS; do
    echo "→ inferring $day"
    "$WORKLOG" infer --day "$day" >/dev/null
done

python3 "$(dirname "$0")/verify_inference_report.py" "$TMP/worklog.db" $DAYS
