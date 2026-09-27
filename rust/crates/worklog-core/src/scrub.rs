//! Secret scrubbing — replaces detected tokens, keys, passwords and other
//! secrets with `clues_contract::SECRET_PLACEHOLDER` before a value is
//! stored or sent off-machine (spec 006, D-03). Populated by T007:
//! `scrub_secrets`.

use std::sync::OnceLock;

use regex::Regex;

use crate::clues_contract::SECRET_PLACEHOLDER;

/// Shared "does this name look like a secret" fragment, reused by
/// `assignment_re`, `flag_assignment_re` and `quoted_value_re` so the
/// keyword list lives in one place. The second alternative only fires on
/// a `_key`/`-key` SUFFIX (`OPENAI_KEY`, `SECRET_KEY`) — never on an
/// arbitrary word that merely contains "key" (`monkey`, `keyboard`),
/// since neither has a literal `_`/`-` immediately before "key".
const NAME_FRAGMENT: &str = r"[\w.-]*(?:token|secret|password|passwd|pwd|api[_-]?key|private_key|credential)[\w.-]*|[\w.-]*(?:_key|-key)";

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

/// `Authorization: Bearer <token>` / bare `Bearer <token>`, case
/// insensitive (`bearer`, `BEARER`) — the matched word is kept verbatim,
/// only the token is scrubbed.
fn bearer_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\b(bearer\s+)[A-Za-z0-9\-._~+/=]+").unwrap())
}

/// `Authorization: Basic <base64>`, case insensitive.
fn basic_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\b(basic\s+)[A-Za-z0-9+/=]+").unwrap())
}

/// `scheme://user:pass@host` → `scheme://[secret]@host`, for any URI
/// scheme (`postgres`, `mongodb+srv`, `redis`, ...) and an optionally
/// empty user (`redis://:pass@host`).
fn url_creds_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"([A-Za-z][A-Za-z0-9+.-]*://)[^\s:/@]*:[^\s@]+@").unwrap())
}

/// `NAME="a whole quoted value"` / `'...'` — the entire quoted value
/// becomes the placeholder (no quotes), so a multi-word secret can't
/// leave a dangling fragment behind for `assignment_re`'s `\S+` to
/// half-scrub. Must run before `assignment_re`.
fn quoted_value_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(&format!(
            r#"(?i)({NAME_FRAGMENT})([:=]\s*)("[^"]*"|'[^']*')"#
        ))
        .unwrap()
    })
}

/// `NAME=value` / `NAME: value` (also matches after `export ` or `--`,
/// since neither is part of the match) where NAME contains one of the
/// secret-ish keywords. The name is kept, the value becomes the
/// placeholder.
fn assignment_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(&format!(r"(?i)(--)?\b({NAME_FRAGMENT})(\s*[:=]\s*)\S+")).unwrap())
}

/// `--password value` — a CLI flag and its value separated by whitespace
/// instead of `=`. Requires the leading `--` so ordinary prose ("fix
/// token refresh") never matches.
fn flag_assignment_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(&format!(r"(?i)(--(?:{NAME_FRAGMENT}))(\s+)\S+")).unwrap())
}

/// `curl -u user:pass` / `curl --user user:pass` → `-u [secret]` —
/// normalizes both flag spellings and scrubs the whole `user:pass` pair.
fn curl_user_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?:-u|--user)\s+\S+:\S+").unwrap())
}

/// `sshpass -p X` (attached or spaced) — `sshpass` IS the password
/// prompt bypass, so its `-p` is unconditionally a secret.
fn sshpass_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(sshpass\s+-p)(\s*)\S+").unwrap())
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
    let s = bearer_re()
        .replace_all(&s, format!("${{1}}{SECRET_PLACEHOLDER}").as_str())
        .into_owned();
    let s = basic_re()
        .replace_all(&s, format!("${{1}}{SECRET_PLACEHOLDER}").as_str())
        .into_owned();
    let s = url_creds_re()
        .replace_all(&s, format!("${{1}}{SECRET_PLACEHOLDER}@").as_str())
        .into_owned();
    let s = quoted_value_re()
        .replace_all(&s, format!("${{1}}${{2}}{SECRET_PLACEHOLDER}").as_str())
        .into_owned();
    let s = assignment_re()
        .replace_all(
            &s,
            format!("${{1}}${{2}}${{3}}{SECRET_PLACEHOLDER}").as_str(),
        )
        .into_owned();
    let s = flag_assignment_re()
        .replace_all(&s, format!("${{1}}${{2}}{SECRET_PLACEHOLDER}").as_str())
        .into_owned();
    let s = curl_user_re()
        .replace_all(&s, format!("-u {SECRET_PLACEHOLDER}").as_str())
        .into_owned();
    let s = sshpass_re()
        .replace_all(&s, format!("${{1}}${{2}}{SECRET_PLACEHOLDER}").as_str())
        .into_owned();
    scrub_mysql_password(&s)
}

/// mysql/mysqldump/mariadb's `-p` password flag, spaced or attached —
/// gated on one of those program names appearing anywhere in `s`, since
/// `-p` alone is far too generic to redact unconditionally (`mkdir -p`).
fn scrub_mysql_password(s: &str) -> String {
    static PROG: OnceLock<Regex> = OnceLock::new();
    let prog = PROG.get_or_init(|| Regex::new(r"\b(?:mysql|mysqldump|mariadb)\b").unwrap());
    if !prog.is_match(s) {
        return s.to_string();
    }
    static FLAG: OnceLock<Regex> = OnceLock::new();
    let flag = FLAG.get_or_init(|| Regex::new(r"(^|\s)(-p)(\s*)(\S+)").unwrap());
    flag.replace_all(
        s,
        format!("${{1}}${{2}}${{3}}{SECRET_PLACEHOLDER}").as_str(),
    )
    .into_owned()
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
