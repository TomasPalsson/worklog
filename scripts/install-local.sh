#!/usr/bin/env bash
# Install origin/main on this Mac in one go:
#   [merge PR] → build → swap binary → restart daemon → refresh hook + skill
#   → deploy web/ to the cache and restart the UI → health check.
#
#   scripts/install-local.sh          # install current origin/main
#   scripts/install-local.sh 124      # squash-merge PR #124 first
#
# Builds in a reusable detached checkout inside .git, so it works from any
# worktree and never touches a dirty checkout. No backups: main is in git.
set -euo pipefail

pr="${1:-}"
common="$(git rev-parse --path-format=absolute --git-common-dir)"
repo="$(dirname "$common")"
src="$common/install-tree"
target="$repo/rust/target"
data="${WORKLOG_HOME:-$HOME/.local/share/worklog}"
bin="$HOME/.local/bin/worklog"

step() { printf '\033[1;34m▶\033[0m %s\n' "$*"; }

if [[ -n "$pr" ]]; then
  step "merge PR #$pr"
  if [[ "$(gh pr view "$pr" --json isDraft -q .isDraft)" == true ]]; then
    gh pr ready "$pr"
  fi
  gh pr merge "$pr" --squash
fi

step "checkout origin/main"
git -C "$repo" fetch -q origin main
[[ -d "$src" ]] || git -C "$repo" worktree add -q --detach "$src" origin/main
git -C "$src" checkout -q --detach origin/main

step "build"
CARGO_TARGET_DIR="$target" cargo build -q --release \
  --manifest-path "$src/rust/Cargo.toml" -p worklog-cli

step "install binary + restart daemon"
# cp + mv is an atomic swap: the running daemon never sees a half-written file.
cp "$target/release/worklog" "$bin.new" && mv "$bin.new" "$bin"
launchctl kickstart -k "gui/$(id -u)/is.p5.worklog.daemon"
for _ in $(seq 20); do
  curl -fs -o /dev/null http://127.0.0.1:9323/health && break
  sleep 0.5
done
curl -fsS -o /dev/null http://127.0.0.1:9323/health
"$bin" hook install >/dev/null
"$bin" skill install >/dev/null

step "deploy web UI"
ver="$("$bin" --version | awk '{print $2}')"
rsync -a --delete --exclude node_modules --exclude .next \
  --exclude .fetched-version --exclude tsconfig.tsbuildinfo \
  "$src/web/" "$data/web/"
# Stamp the cache as current for this binary so a later `worklog web up`
# serves it instead of auto-fetching the tagged tree from GitHub.
printf '%s\n%s\n' "$ver" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >"$data/web/.fetched-version"
"$bin" web down >/dev/null 2>&1
# A next-server that outlived its pid file would keep serving the old build.
lsof -ti tcp:3333 -sTCP:LISTEN | xargs kill 2>/dev/null || true
WORKLOG_WEB_DIR="$data/web" "$bin" web up --no-open --no-daemon >/dev/null
curl -fsS -o /dev/null http://127.0.0.1:3333/

printf '\033[1;32m✓\033[0m worklog %s installed from main @ %s\n' \
  "$ver" "$(git -C "$src" rev-parse --short HEAD)"
