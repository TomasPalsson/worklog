//! Secret scrubbing — replaces detected tokens, keys, passwords and other
//! secrets with `clues_contract::SECRET_PLACEHOLDER` before a value is
//! stored or sent off-machine (spec 006, D-03). Populated by T007:
//! `scrub_secrets`.
//!
//! Precision matters as much as recall here: `scrub_secrets` now runs on
//! every stored title/details (repo::upsert_event), so over-scrubbing
//! permanently destroys ordinary commit/PR prose. Every pattern below
//! that could plausibly collide with prose, a file path, or a numeric
//! setting is gated — either by requiring a credential-shaped value (a
//! digit, a base64/token symbol, or mixed case not at the very start),
//! by rejecting a purely-numeric value, or by requiring the surrounding
//! command context (mysql/curl/wget/...). The `NAME=value`-shaped family
//! (bare/flagged/quoted/JSON) lives in `scrub_assignment.rs`.

use std::sync::OnceLock;

use regex::{Captures, Regex};

use crate::clues_contract::SECRET_PLACEHOLDER;
use crate::scrub_assignment;

/// Whole-match patterns: the entire match becomes the placeholder. `(?s)`
/// only affects the PEM alternative — every other alternative's `.` is
/// escaped (`\.`), so it stays a literal dot regardless of the flag.
fn whole_match_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(concat!(
            r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?-----END [A-Z ]*PRIVATE KEY-----",
            r"|\bgh[oprsu]_[A-Za-z0-9]{36}\b",
            r"|\bgithub_pat_[A-Za-z0-9_]{20,}\b",
            r"|\bglpat-[A-Za-z0-9_-]{20,}\b",
            r"|\bnpm_[A-Za-z0-9]{36}\b",
            r"|\b(?:AKIA|ASIA)[A-Z0-9]{16}\b",
            r"|\bxapp-\d+-[A-Za-z0-9-]{8,}\b",
            r"|\bxoxe\.xoxp-[A-Za-z0-9-]{8,}\b",
            r"|\bxox[abeprs]-[A-Za-z0-9-]{8,}\b",
            r"|\bsk-[A-Za-z0-9_-]{20,}\b",
            r"|\b(?:sk|rk)_live_[A-Za-z0-9]{10,}\b",
            r"|\bAIza[A-Za-z0-9_-]{35}\b",
            r"|\beyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\b",
        ))
        .unwrap()
    })
}

/// A PEM block whose `-----BEGIN ... PRIVATE KEY-----` marker is never
/// followed by a matching END — `whole_match_re` only pairs the two, so
/// a truncated/streamed key would otherwise leak everything after BEGIN.
/// Scrubs from the marker to the end of the string.
fn unterminated_pem_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*\z").unwrap())
}

// ───────────────────────── shared value-shape guards ─────────────────────────
// (used by both this file's Bearer/Basic checks and scrub_assignment.rs)

/// True when `v` looks like a real credential rather than an ordinary
/// English word of the same length: contains a digit, one of the
/// URL-safe base64/token symbols, or an uppercase letter NOT at the very
/// start (real base64/tokens mix case mid-word; English prose is either
/// all-lowercase or Titlecase, never `dXNl`-style).
pub(crate) fn looks_credential_shaped(v: &str) -> bool {
    v.chars().any(|c| c.is_ascii_digit())
        || v.chars().any(|c| "+/=._~-".contains(c))
        || v.chars().skip(1).any(|c| c.is_ascii_uppercase())
}

pub(crate) fn is_pure_numeric(v: &str) -> bool {
    !v.is_empty() && v.chars().all(|c| c.is_ascii_digit())
}

/// True when the text immediately before `match_start` is an
/// `Authorization:` header label (with optional trailing whitespace) —
/// a bare `Bearer <opaque token>` with no other credential-shaped
/// signal is still a secret when it's genuinely a header value.
fn preceded_by_authorization_header(s: &str, match_start: usize) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)authorization\s*:\s*$").unwrap())
        .is_match(&s[..match_start])
}

// ───────────────────────── Authorization headers ─────────────────────────

/// `Authorization: Bearer <token>` / bare `Bearer <token>`, case
/// insensitive. Requires the token look credential-shaped (a digit or a
/// token/base64 symbol) — a bare `Bearer token refresh` in prose never
/// matches at all (candidate length floor); a real header value with no
/// such signal (`Bearer zzzzzzzz...`) still scrubs when genuinely
/// preceded by `Authorization:`.
fn bearer_candidate_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\b(bearer\s+)([A-Za-z0-9\-._~+/=]{16,})").unwrap())
}

fn scrub_bearer(s: &str) -> String {
    bearer_candidate_re()
        .replace_all(s, |caps: &Captures| {
            let m = caps.get(0).unwrap();
            let word = &caps[1];
            let token = &caps[2];
            if looks_credential_shaped(token) || preceded_by_authorization_header(s, m.start()) {
                format!("{word}{SECRET_PLACEHOLDER}")
            } else {
                m.as_str().to_string()
            }
        })
        .into_owned()
}

/// `Authorization: Basic <base64>`, case insensitive. Requires a
/// base64-shaped run of >=12 chars — `basic validation`/`basic auth
/// support` (ordinary words) never even reach the length floor, and a
/// genuine word-shaped run needs a digit/symbol/mixed-case signal too.
fn basic_candidate_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\b(basic\s+)([A-Za-z0-9+/]{12,}=*)").unwrap())
}

fn scrub_basic(s: &str) -> String {
    basic_candidate_re()
        .replace_all(s, |caps: &Captures| {
            let m = caps.get(0).unwrap();
            let word = &caps[1];
            let token = &caps[2];
            if looks_credential_shaped(token) || preceded_by_authorization_header(s, m.start()) {
                format!("{word}{SECRET_PLACEHOLDER}")
            } else {
                m.as_str().to_string()
            }
        })
        .into_owned()
}

// ───────────────────────── URL credentials ─────────────────────────

/// `scheme://user:pass@host` → `scheme://[secret]@host`, for any URI
/// scheme (`postgres`, `mongodb+srv`, `redis`, ...) and an optionally
/// empty user (`redis://:pass@host`). The password half excludes `/` so
/// a bare port number before the first path segment
/// (`https://host:8080/users/foo@bar.com`) can never be mistaken for
/// userinfo — real userinfo always ends at the first `/`.
fn url_creds_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"([A-Za-z][A-Za-z0-9+.-]*://)[^\s:/@]*:[^\s@/]+@").unwrap())
}

// ───────────────────────── shell credential flags ─────────────────────────

/// `curl -u user:pass` / `curl --user user:pass` → `-u [secret]` — only
/// within a curl/wget/http(ie) command (`docker run -u 1000:1000` is a
/// UID:GID pair, not credentials), and never when both sides are plain
/// digits either.
fn curl_user_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?:-u|--user)\s+(\S+):(\S+)").unwrap())
}

fn scrub_curl_user(s: &str) -> String {
    static PROG: OnceLock<Regex> = OnceLock::new();
    let prog = PROG.get_or_init(|| Regex::new(r"\b(?:curl|wget|https?|httpie)\b").unwrap());
    if !prog.is_match(s) {
        return s.to_string();
    }
    curl_user_re()
        .replace_all(s, |caps: &Captures| {
            let user = &caps[1];
            let pass = &caps[2];
            if is_pure_numeric(user) && is_pure_numeric(pass) {
                caps.get(0).unwrap().as_str().to_string()
            } else {
                format!("-u {SECRET_PLACEHOLDER}")
            }
        })
        .into_owned()
}

/// `sshpass -p X` (attached or spaced) — `sshpass` IS the password
/// prompt bypass, so its `-p` is unconditionally a secret.
fn sshpass_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(sshpass\s+-p)(\s*)\S+").unwrap())
}

/// mysql/mysqldump/mariadb's `-p` password flag, spaced or attached —
/// gated on one of those program names appearing in the SAME command
/// segment (split on `;`, `&&`, `||`, `|`, newline) as the `-p`, since
/// `-p` alone is far too generic to redact unconditionally (`mkdir -p`)
/// and a later unrelated command in the same line must stay untouched
/// (`mysql migration; mkdir -p build` keeps "build").
fn command_separator_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r";|&&|\|\||\||\n").unwrap())
}

fn scrub_mysql_password(s: &str) -> String {
    let sep = command_separator_re();
    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    for m in sep.find_iter(s) {
        out.push_str(&scrub_mysql_segment(&s[last..m.start()]));
        out.push_str(m.as_str());
        last = m.end();
    }
    out.push_str(&scrub_mysql_segment(&s[last..]));
    out
}

fn scrub_mysql_segment(seg: &str) -> String {
    static PROG: OnceLock<Regex> = OnceLock::new();
    let prog = PROG.get_or_init(|| Regex::new(r"\b(?:mysql|mysqldump|mariadb)\b").unwrap());
    if !prog.is_match(seg) {
        return seg.to_string();
    }
    static FLAG: OnceLock<Regex> = OnceLock::new();
    let flag = FLAG.get_or_init(|| Regex::new(r"(^|\s)(-p)(\s*)(\S+)").unwrap());
    flag.replace_all(
        seg,
        format!("${{1}}${{2}}${{3}}{SECRET_PLACEHOLDER}").as_str(),
    )
    .into_owned()
}

/// Redact tokens, keys, passwords and private keys only. Emails and IPs
/// stay raw — this is the storage-time scrub (D-03).
pub fn scrub_secrets(s: &str) -> String {
    let s = whole_match_re()
        .replace_all(s, SECRET_PLACEHOLDER)
        .into_owned();
    let s = unterminated_pem_re()
        .replace_all(&s, SECRET_PLACEHOLDER)
        .into_owned();
    let s = scrub_bearer(&s);
    let s = scrub_basic(&s);
    let s = url_creds_re()
        .replace_all(&s, format!("${{1}}{SECRET_PLACEHOLDER}@").as_str())
        .into_owned();
    let s = scrub_assignment::scrub_json_kv(&s);
    let s = scrub_assignment::scrub_quoted(&s);
    let s = scrub_assignment::scrub_bare(&s);
    let s = scrub_assignment::scrub_flagged(&s);
    let s = scrub_curl_user(&s);
    let s = sshpass_re()
        .replace_all(&s, format!("${{1}}${{2}}{SECRET_PLACEHOLDER}").as_str())
        .into_owned();
    scrub_mysql_password(&s)
}

fn email_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b").unwrap())
}

fn ipv4_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"\b(?:(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)\.){3}(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)\b",
        )
        .unwrap()
    })
}

fn account_id_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b\d{12}\b").unwrap())
}

/// `scrub_secrets` plus emails, IPv4 addresses and 12-digit AWS account
/// ids. Used for anything leaving the machine (D-02).
pub fn scrub_identifiers(s: &str) -> String {
    let s = scrub_secrets(s);
    let s = email_re().replace_all(&s, SECRET_PLACEHOLDER).into_owned();
    let s = ipv4_re().replace_all(&s, SECRET_PLACEHOLDER).into_owned();
    account_id_re()
        .replace_all(&s, SECRET_PLACEHOLDER)
        .into_owned()
}

/// `scrub_secrets` over every string in a JSON value. A string value whose
/// object key names a secret (`"token": "…"`) is replaced whole, since the
/// key never reaches the text-level assignment pattern.
pub fn scrub_json(value: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::String(s) => Value::String(scrub_secrets(s)),
        Value::Array(items) => Value::Array(items.iter().map(scrub_json).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| {
                    let v = match v {
                        Value::String(_) if names_a_secret(k) => {
                            Value::String(SECRET_PLACEHOLDER.to_string())
                        }
                        _ => scrub_json(v),
                    };
                    (k.clone(), v)
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

fn names_a_secret(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "token",
        "secret",
        "password",
        "passwd",
        "api_key",
        "private_key",
    ]
    .iter()
    .any(|word| key.contains(word))
}

// Tests live in scrub_test.rs (same module, split file for line budget).
#[cfg(test)]
#[path = "scrub_test.rs"]
mod tests;
