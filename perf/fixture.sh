#!/bin/bash
# Build a frozen, hermetic benchmark fixture from this Mac's real data.
# APFS clones (cp -c) so it costs ~0 extra disk. Never writes to the sources.
#
#   PERF_DIR      where the fixture goes        (default: $TMPDIR/worklog-perf)
#   PERF_SRC_DB   the DB to snapshot            (default: ~/.local/share/worklog/worklog.db)
#   PERF_SINCE    transcripts touched since     (default: 2026-09-24 00:00)
set -euo pipefail
PERF_DIR=${PERF_DIR:-${TMPDIR:-/tmp}/worklog-perf}
SRC_DB=${PERF_SRC_DB:-$HOME/.local/share/worklog/worklog.db}
SINCE=${PERF_SINCE:-2026-09-24 00:00:00}
SQ=/usr/bin/sqlite3

rm -rf "$PERF_DIR"
mkdir -p "$PERF_DIR/data" "$PERF_DIR/empty" "$PERF_DIR/home/Desktop/Projects" "$PERF_DIR/home/.local/share/fish"

# DB: copy file + WAL, then fold the WAL into the copy only.
cp -c "$SRC_DB" "$PERF_DIR/data/worklog.db"
[ -f "$SRC_DB-wal" ] && cp -c "$SRC_DB-wal" "$PERF_DIR/data/worklog.db-wal"
$SQ "$PERF_DIR/data/worklog.db" "PRAGMA wal_checkpoint(TRUNCATE);" >/dev/null
[ "$($SQ "$PERF_DIR/data/worklog.db" 'PRAGMA integrity_check;')" = ok ]
rm -f "$PERF_DIR/data/worklog.db-wal" "$PERF_DIR/data/worklog.db-shm"
echo '{}' > "$PERF_DIR/data/secrets.json"
echo '{}' > "$PERF_DIR/empty/secrets.json"

# Transcripts (projects + jobs), keeping relative paths and mtimes.
for sub in projects jobs; do
  src="$HOME/.claude/$sub"
  [ -d "$src" ] || continue
  (cd "$src" && find . -name '*.jsonl' -newermt "$SINCE" -print0) | while IFS= read -r -d '' f; do
    mkdir -p "$PERF_DIR/home/.claude/$sub/$(dirname "$f")"
    cp -c -p "$src/$f" "$PERF_DIR/home/.claude/$sub/$f"
  done
done

# Fish history (real commands; local only) and the work-folder names (empty dirs:
# routing options come from the names, reflog finds no repos).
[ -f "$HOME/.local/share/fish/fish_history" ] && cp -c -p "$HOME/.local/share/fish/fish_history" "$PERF_DIR/home/.local/share/fish/"
for d in "$HOME"/Desktop/Work/*/; do mkdir -p "$PERF_DIR/home/Desktop/Work/$(basename "$d")"; done

echo "fixture: $PERF_DIR"
echo "  db:          $(du -h "$PERF_DIR/data/worklog.db" | cut -f1) (schema v$($SQ "$PERF_DIR/data/worklog.db" 'PRAGMA user_version;'))"
echo "  transcripts: $(find "$PERF_DIR/home/.claude" -name '*.jsonl' | wc -l | tr -d ' ') files, $(du -sh "$PERF_DIR/home/.claude" | cut -f1)"
echo "  work dirs:   $(ls "$PERF_DIR/home/Desktop/Work" | wc -l | tr -d ' ')"
