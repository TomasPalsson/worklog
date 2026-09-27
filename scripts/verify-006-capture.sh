#!/usr/bin/env bash
# Spec 006 §5 on real data: phase-B capture must grow worklog.db by at most
# 5 MB per active day, and no stored value may still match a scrubber pattern.
#
# Works on a backup copy of the live DB — the live DB is never written.
# Re-collects the local sources (shell, reflog, Claude transcripts) for the
# last few days with this branch's binary, then measures DAY (default
# 2026-09-25).
set -euo pipefail

DAY="${DAY:-2026-09-25}"
LIMIT_BYTES=$((5 * 1024 * 1024))
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LIVE_DB="${LIVE_DB:-$HOME/.local/share/worklog/worklog.db}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

cargo build --quiet --manifest-path "$ROOT/rust/Cargo.toml" -p worklog-cli
BIN="$ROOT/rust/target/debug/worklog"

sqlite3 "$LIVE_DB" ".backup '$WORK/worklog.db'"
export WORKLOG_HOME="$WORK"
"$BIN" db migrate >/dev/null

day_bytes() {
  sqlite3 "$WORK/worklog.db" "SELECT COALESCE(SUM(
      LENGTH(source) + LENGTH(source_id) + LENGTH(started_at) + LENGTH(title)
      + COALESCE(LENGTH(details), 0) + COALESCE(LENGTH(project_path), 0)
      + COALESCE(LENGTH(session_id), 0) + COALESCE(LENGTH(raw_json), 0)), 0)
    FROM events WHERE started_at LIKE '$DAY%'"
}

before=$(day_bytes)
days=$(( ( $(date -u +%s) - $(date -u -j -f %Y-%m-%d "$DAY" +%s 2>/dev/null || date -u -d "$DAY" +%s) ) / 86400 + 1 ))
for target in shell reflog transcripts; do
  "$BIN" collect "$target" --days "$days" >/dev/null
done
after=$(day_bytes)
growth=$((after - before))

pattern='gh[oprsu]_[A-Za-z0-9]{36}|github_pat_[A-Za-z0-9_]{20,}|A(KIA|SIA)[0-9A-Z]{16}|xox[abprs]-[A-Za-z0-9-]{10,}|sk-ant-[A-Za-z0-9_-]{20,}|-----BEGIN [A-Z ]*PRIVATE KEY-----|AIza[0-9A-Za-z_-]{35}|(sk|rk)_live_[A-Za-z0-9]{10,}'
hits=$(sqlite3 "$WORK/worklog.db" \
  "SELECT COALESCE(raw_json, '') || ' ' || title || ' ' || COALESCE(details, '') FROM events WHERE raw_json IS NOT NULL" \
  | grep -Ec "$pattern" || true)

rows=$(sqlite3 "$WORK/worklog.db" "SELECT source || ' ' || COUNT(*) FROM events WHERE started_at LIKE '$DAY%' AND raw_json IS NOT NULL GROUP BY source")
echo "day: $DAY"
echo "rows with raw_json on $DAY:"
echo "$rows" | sed 's/^/  /'
echo "stored bytes for $DAY: before=$before after=$after growth=$growth (limit $LIMIT_BYTES)"
echo "scrubber-pattern hits in stored events: $hits"

[ "$growth" -le "$LIMIT_BYTES" ] || { echo "FAIL: growth over 5 MB"; exit 1; }
[ "$hits" -eq 0 ] || { echo "FAIL: secrets left in worklog.db"; exit 1; }
echo "PASS"
