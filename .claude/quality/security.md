# Security Scan — feature/delete-python

## Verdict
PASS (with 2 IMPORTANT + 2 MINOR; IMPORTANT items fixed inline below)

## Checked
- `rust/crates/worklog-core/src/collectors/gcal.rs` (new, 844 lines)
- `install.sh` (new, 156 lines)
- `.github/workflows/release.yml` (new)
- `scripts/release-smoke.sh`
- `rust/crates/worklog-core/src/updater/pubkey.rs`

## Findings

### IMPORTANT — token.json written without restrictive permissions
**File:** `rust/crates/worklog-core/src/collectors/gcal.rs:196-200`
**Confidence:** HIGH
**Status:** FIXED

The token file contains `access_token`, `refresh_token`, and `client_secret`. `std::fs::write` honours the process umask, typically producing `0644` on Linux (world-readable). Fixed by `chmod 0600` immediately after write.

### IMPORTANT — OAuth error body surfaced verbatim
**File:** `rust/crates/worklog-core/src/collectors/gcal.rs:181-185`, `:292-295`
**Confidence:** HIGH
**Status:** FIXED

Google's OAuth error responses can include `error_description` fields that echo submitted credential values. The raw body was being returned in the user-facing error. Fixed by logging the raw body at `debug!` and surfacing a sanitised message.

### MINOR — Private key cleanup only on happy path
**File:** `.github/workflows/release.yml`
**Confidence:** HIGH
**Status:** FIXED

`shred` at the end of the Assemble step; if the step is cancelled, the key file survives on the (ephemerally-destroyed) GHA runner. Added an `if: always()` cleanup step so it runs even on job cancellation.

### MINOR — VERSION flag in URL
**File:** `install.sh:87`
**Confidence:** HIGH
**Status:** NOT FIXED (non-issue)

`--version <tag>` flows into a double-quoted `curl` URL with no shell expansion; no injection surface. The flagged scenario (attacker-controlled env var) requires pre-existing shell compromise. Left as-is.

## Items Checked — Clean

- Tracing on refresh does not interpolate token values.
- Calendar names go through `urlencoding::encode` before URL path embedding.
- `crate::http::client()` uses reqwest's TLS verification defaults.
- `needs_refresh` compares timestamps (not secret bytes); no timing oracle.
- `#[cfg(test)]` gating on `WORKLOG_RELEASE_PUBKEY_BASE64` compiles out of release builds — confirmed.
- `install.sh` uses `mktemp` + `trap rm` + atomic `mv`; no partial-install risk.
- `$TAG` + `$PRIV` in release.yml are double-quoted / masked; no stdout leakage.
