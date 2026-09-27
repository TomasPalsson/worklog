#!/bin/bash
# S1: DB bytes after a fresh ingest of the fixture's days, per binary (hermetic).
#   PERF_DIR=<fixture> bash perf/space.sh <bin> [<bin>...]
set -euo pipefail
P=${PERF_DIR:?set PERF_DIR}
DAYS=${DAYS:-2026-09-24 2026-09-25 2026-09-26 2026-09-27}
for BIN in "$@"; do
  R=$P/runs/space-$(basename "$BIN"); rm -rf "$R"; mkdir -p "$R"; cp -c -R "$P/empty" "$R/data"
  for d in $DAYS; do
    env -i HOME="$P/home" WORKLOG_HOME="$R/data" WORKLOG_SECRETS_FILE="$R/data/secrets.json" \
      WORKLOG_ESTIMATOR_PROVIDER=litellm WORKLOG_PRUNE_ENABLED=false PATH=/usr/bin:/bin \
      "$BIN" day --day "$d" --no-serve >/dev/null 2>&1
  done
  db="$R/data/worklog.db"
  /usr/bin/sqlite3 "$db" "PRAGMA wal_checkpoint(TRUNCATE);" >/dev/null
  live=$(stat -f %z "$db")
  /usr/bin/sqlite3 "$db" "VACUUM;" >/dev/null
  printf '%-16s events=%s  db=%.2f MB  after VACUUM=%.2f MB\n' "$(basename "$BIN")" \
    "$(/usr/bin/sqlite3 "$db" 'SELECT count(*) FROM events;')" \
    "$(echo "$live / 1048576" | bc -l)" "$(echo "$(stat -f %z "$db") / 1048576" | bc -l)"
  rm -rf "$R"
done
