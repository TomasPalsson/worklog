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

# S1: repo::upsert_event may store raw_json as a deflate BLOB when that's
# smaller than the original TEXT (raw_json.rs). Decode it here so growth
# and secret checks measure the ORIGINAL JSON, never compressed bytes —
# otherwise this gate goes silently blind to secrets inside a BLOB.
day_bytes() {
  python3 - "$WORK/worklog.db" "$DAY%" <<'PY'
import sqlite3, sys, zlib
conn = sqlite3.connect(sys.argv[1])
total = 0
for source, source_id, started_at, title, details, project_path, session_id, raw_json in conn.execute(
    "SELECT source, source_id, started_at, title, details, project_path, "
    "session_id, raw_json FROM events WHERE started_at LIKE ?",
    (sys.argv[2],),
):
    total += len(source) + len(source_id) + len(started_at) + len(title)
    total += len(details or "") + len(project_path or "") + len(session_id or "")
    if raw_json is not None:
        if isinstance(raw_json, bytes):
            raw_json = zlib.decompress(raw_json, -15).decode("utf-8")
        total += len(raw_json)
print(total)
PY
}

before=$(day_bytes)
days=$(( ( $(date -u +%s) - $(date -u -j -f %Y-%m-%d "$DAY" +%s 2>/dev/null || date -u -d "$DAY" +%s) ) / 86400 + 1 ))
for target in shell reflog transcripts; do
  "$BIN" collect "$target" --days "$days" >/dev/null
done
after=$(day_bytes)
growth=$((after - before))

pattern='gh[oprsu]_[A-Za-z0-9]{36}|github_pat_[A-Za-z0-9_]{20,}|A(KIA|SIA)[0-9A-Z]{16}|xox[abprs]-[A-Za-z0-9-]{10,}|sk-ant-[A-Za-z0-9_-]{20,}|-----BEGIN [A-Z ]*PRIVATE KEY-----|AIza[0-9A-Za-z_-]{35}|(sk|rk)_live_[A-Za-z0-9]{10,}'
hits=$(python3 - "$WORK/worklog.db" <<'PY' | grep -Ec "$pattern" || true
import sqlite3, sys, zlib
conn = sqlite3.connect(sys.argv[1])
for raw_json, title, details in conn.execute(
    "SELECT raw_json, title, details FROM events WHERE raw_json IS NOT NULL"
):
    if isinstance(raw_json, bytes):
        try:
            raw_json = zlib.decompress(raw_json, -15).decode("utf-8")
        except Exception:
            raw_json = ""
    print((raw_json or "") + " " + title + " " + (details or ""))
PY
)

rows=$(sqlite3 "$WORK/worklog.db" "SELECT source || ' ' || COUNT(*) FROM events WHERE started_at LIKE '$DAY%' AND raw_json IS NOT NULL GROUP BY source")
echo "day: $DAY"
echo "rows with raw_json on $DAY:"
echo "$rows" | sed 's/^/  /'
echo "stored bytes for $DAY: before=$before after=$after growth=$growth (limit $LIMIT_BYTES)"
echo "scrubber-pattern hits in stored events: $hits"

[ "$growth" -le "$LIMIT_BYTES" ] || { echo "FAIL: growth over 5 MB"; exit 1; }
[ "$hits" -eq 0 ] || { echo "FAIL: secrets left in worklog.db"; exit 1; }
echo "PASS"
