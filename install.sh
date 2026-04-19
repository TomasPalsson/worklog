#!/usr/bin/env bash
# worklog installer — pulls the latest signed binary from GitHub Releases
# and drops it at ~/.local/bin/worklog.
#
# Trust model:
#   * This script is fetched over HTTPS from github.com — you're implicitly
#     trusting GitHub's TLS.
#   * The binary it downloads is the same artifact the signed self-updater
#     will later verify against the Ed25519 pubkey baked in at compile
#     time. We do NOT re-verify here — `worklog upgrade` does it for every
#     subsequent update, but the *first* install is a trust bootstrap.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/TomasPalsson/worklog/main/install.sh | bash
#
# Flags:
#   --version <tag>   Install a specific tag (e.g. v0.3.1). Default: latest.
#   --prefix <dir>    Override install dir (default: $HOME/.local/bin).
#   --dry-run         Print what would happen without downloading/installing.
#   --force           Overwrite an existing `worklog` binary in prefix.
#   --help            Print this help.

set -euo pipefail

REPO="TomasPalsson/worklog"
VERSION="latest"
PREFIX="${HOME}/.local/bin"
DRY_RUN=0
FORCE=0

# ─────────────────────────── arg parsing ───────────────────────────
while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSION="$2"; shift 2 ;;
    --prefix)  PREFIX="$2";  shift 2 ;;
    --dry-run) DRY_RUN=1;    shift   ;;
    --force)   FORCE=1;      shift   ;;
    -h|--help)
      awk '/^#[^!]/ {sub(/^# ?/, ""); print; next} /^[^#]/ {exit}' "$0"
      exit 0 ;;
    *) echo "unknown flag: $1" >&2; exit 2 ;;
  esac
done

log()  { printf '\033[1;34m▶\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!\033[0m %s\n' "$*" >&2; }
fail() { printf '\033[1;31m✗\033[0m %s\n' "$*" >&2; exit 1; }
ok()   { printf '\033[1;32m✓\033[0m %s\n' "$*"; }

# ─────────────────────────── pre-install checks ───────────────────────────

# Detect + refuse to silently stomp on an existing `uv tool install worklog`.
# The two binaries would both answer to `worklog` on PATH and users would
# get confused about which one is running. Asking them to uninstall first
# is safer than picking for them.
if command -v uv >/dev/null 2>&1; then
  if uv tool list 2>/dev/null | grep -qE '^worklog(\s|$)'; then
    warn "you have 'uv tool install worklog' on this machine."
    warn "run this first, then re-invoke the installer:"
    warn "    uv tool uninstall worklog"
    if [ "$FORCE" -eq 0 ]; then
      fail "refusing to continue without --force"
    fi
    warn "proceeding anyway (--force); new install may shadow the uv one"
  fi
fi

# ─────────────────────────── target detection ───────────────────────────

UNAME_S=$(uname -s)
UNAME_M=$(uname -m)
case "$UNAME_S/$UNAME_M" in
  Darwin/arm64)       TARGET=aarch64-apple-darwin ;;
  Darwin/x86_64)      fail "Intel Mac builds aren't published yet. Build from source: cargo install --path rust/crates/worklog-cli" ;;
  Linux/x86_64)       TARGET=x86_64-unknown-linux-gnu ;;
  Linux/aarch64)      fail "linux-arm64 builds aren't published yet. Build from source: cargo install --path rust/crates/worklog-cli" ;;
  *) fail "unsupported platform: $UNAME_S/$UNAME_M" ;;
esac
log "target detected: $TARGET"

# ─────────────────────────── URL resolution ───────────────────────────

if [ "$VERSION" = "latest" ]; then
  # GitHub's `/releases/latest/download/` redirects to the most recent tag.
  URL="https://github.com/${REPO}/releases/latest/download/worklog-${TARGET}"
else
  URL="https://github.com/${REPO}/releases/download/${VERSION}/worklog-${TARGET}"
fi
log "source: $URL"
log "dest:   $PREFIX/worklog"

# ─────────────────────────── dry-run exit ───────────────────────────

if [ "$DRY_RUN" -eq 1 ]; then
  ok "dry-run: nothing downloaded, nothing installed"
  exit 0
fi

# ─────────────────────────── overwrite check ───────────────────────────

if [ -e "$PREFIX/worklog" ] && [ "$FORCE" -eq 0 ]; then
  warn "$PREFIX/worklog already exists"
  warn "use --force to overwrite, or remove it manually first"
  exit 1
fi

# ─────────────────────────── download ───────────────────────────

mkdir -p "$PREFIX"
TMP=$(mktemp)
trap 'rm -f "$TMP"' EXIT

if ! curl -fsSL "$URL" -o "$TMP"; then
  fail "download failed — check that the release exists at $URL"
fi

if [ ! -s "$TMP" ]; then
  fail "downloaded file is empty — aborting"
fi

chmod +x "$TMP"
mv "$TMP" "$PREFIX/worklog"
trap - EXIT
ok "installed → $PREFIX/worklog"

# ─────────────────────────── post-install checks ───────────────────────────

# Is the prefix on PATH? If not, point the user at the fix — otherwise the
# install "worked" but `worklog` won't be found and they'll think it didn't.
case ":$PATH:" in
  *":$PREFIX:"*) ;;
  *)
    warn "$PREFIX is not on your PATH."
    warn "add this to your shell rc:"
    warn "    export PATH=\"$PREFIX:\$PATH\""
    ;;
esac

# Print version to confirm it runs. Non-fatal — some sandboxed environments
# (docker-in-docker, etc.) refuse to exec the downloaded binary the first
# time around; the install itself succeeded.
if "$PREFIX/worklog" --version >/dev/null 2>&1; then
  VER=$("$PREFIX/worklog" --version 2>/dev/null || echo "?")
  ok "${VER}"
else
  warn "installed, but '$PREFIX/worklog --version' failed — try running it directly"
fi

cat <<'EOF'

Next steps:
  worklog setup              # one-shot onboarding (secrets, db, hook)
  worklog day                # end-of-day one-shot pipeline
  worklog upgrade            # signed self-update (when a new release drops)

EOF
