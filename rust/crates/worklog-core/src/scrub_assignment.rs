//! `NAME=value`-shaped secret patterns, split out of `scrub.rs` for the
//! file-size budget: bare `NAME=value`/`NAME: value`, `--flag value`,
//! `NAME="quoted value"` and JSON-in-prose `"NAME": "value"`. All four
//! share one keyword list (`NAME_FRAGMENT`) and one scrub-or-not
//! decision (`should_scrub_assignment`).

use std::sync::OnceLock;

use regex::{Captures, Regex};

use crate::clues_contract::SECRET_PLACEHOLDER;
use crate::scrub::{is_pure_numeric, looks_credential_shaped};

/// Shared "does this name look like a secret" fragment. The second
/// alternative only fires on a `_key`/`-key` SUFFIX (`OPENAI_KEY`,
/// `SECRET_KEY`) — never on an arbitrary word that merely contains "key"
/// (`monkey`, `keyboard`), since neither has a literal `_`/`-`
/// immediately before "key". A name matching only this suffix
/// alternative additionally requires its value to look secret-ish
/// (`should_scrub_assignment`) — `partition_key: userId` stays.
const NAME_FRAGMENT: &str = r"[\w.-]*(?:token|secret|password|passwd|pwd|api[_-]?key|private_key|credential)[\w.-]*|[\w.-]*(?:_key|-key)";

/// The explicit keyword half of `NAME_FRAGMENT` — true when `name`
/// itself names a secret, as opposed to only matching the generic
/// `_key`/`-key` suffix alternative.
fn name_is_explicit_secret_keyword(name: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)token|secret|password|passwd|pwd|api[_-]?key|private_key|credential")
            .unwrap()
    })
    .is_match(name)
}

/// Value-shape gate for a name that only matched the `_key`/`-key`
/// suffix alternative: `userId` (6 chars) stays, a long/credential-
/// shaped value still gets scrubbed.
fn key_value_looks_secret(v: &str) -> bool {
    let len = v.chars().count();
    if len < 8 {
        return false;
    }
    if len >= 16 {
        return true;
    }
    looks_credential_shaped(v)
}

/// Every assignment-shaped pattern (`NAME=value`, `NAME: "value"`,
/// `"NAME": "value"`) shares this decision: a purely numeric value is
/// never a secret, an explicit keyword name always scrubs, and a
/// suffix-only (`_key`/`-key`) name additionally requires a
/// secret-shaped value.
fn should_scrub_assignment(name: &str, value: &str) -> bool {
    if is_pure_numeric(value) {
        return false;
    }
    name_is_explicit_secret_keyword(name) || key_value_looks_secret(value)
}

/// False right after a path separator or dot — `src/token.rs`'s "token"
/// is part of a filename, not an assignment name. Rust `regex` has no
/// lookbehind, so this is checked in the replacement closure instead.
fn not_path_like(s: &str, match_start: usize) -> bool {
    match_start == 0 || !matches!(s.as_bytes()[match_start - 1], b'/' | b'.')
}

/// `NAME="a whole quoted value"` / `'...'` — the entire quoted value
/// becomes the placeholder (no quotes), so a multi-word secret can't
/// leave a dangling fragment behind for `scrub_bare`'s `\S+` to
/// half-scrub. Must run before `scrub_bare`.
fn quoted_value_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(&format!(
            r#"(?i)({NAME_FRAGMENT})([ \t]*=[ \t]*|[ \t]*:[ \t]+)("[^"]*"|'[^']*')"#
        ))
        .unwrap()
    })
}

pub(crate) fn scrub_quoted(s: &str) -> String {
    quoted_value_re()
        .replace_all(s, |caps: &Captures| {
            let m = caps.get(0).unwrap();
            let name = &caps[1];
            let sep = &caps[2];
            let quoted = &caps[3];
            let inner = &quoted[1..quoted.len() - 1];
            if not_path_like(s, m.start()) && should_scrub_assignment(name, inner) {
                format!("{name}{sep}{SECRET_PLACEHOLDER}")
            } else {
                m.as_str().to_string()
            }
        })
        .into_owned()
}

/// `"NAME": "value"` / `'NAME': 'value'` — a JSON object fragment pasted
/// into free text (D-03: title/details are prose, not just shell/env
/// text). Rust's `regex` crate has no backreferences, so the name's and
/// the value's quote type are each their own alternation branch instead
/// of one quote-char group referenced twice.
fn json_kv_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(&format!(
            r#"(?i)(?:"({NAME_FRAGMENT})"|'({NAME_FRAGMENT})')([ \t]*:[ \t]*)(?:"([^"]*)"|'([^']*)')"#
        ))
        .unwrap()
    })
}

pub(crate) fn scrub_json_kv(s: &str) -> String {
    json_kv_re()
        .replace_all(s, |caps: &Captures| {
            let (name, name_q) = match (caps.get(1), caps.get(2)) {
                (Some(m), _) => (m.as_str(), '"'),
                (_, Some(m)) => (m.as_str(), '\''),
                _ => unreachable!("one of the two quote alternatives always matches"),
            };
            let sep = &caps[3];
            let (value, value_q) = match (caps.get(4), caps.get(5)) {
                (Some(m), _) => (m.as_str(), '"'),
                (_, Some(m)) => (m.as_str(), '\''),
                _ => unreachable!("one of the two quote alternatives always matches"),
            };
            if should_scrub_assignment(name, value) {
                format!("{name_q}{name}{name_q}{sep}{value_q}{SECRET_PLACEHOLDER}{value_q}")
            } else {
                caps.get(0).unwrap().as_str().to_string()
            }
        })
        .into_owned()
}

/// `NAME=value` / `NAME: value` (also matches after `export ` or `--`,
/// since neither is part of the match). `=` allows tight packing
/// (`GITHUB_TOKEN=x`); `:` requires at least one space/tab after it, so
/// `src/token.rs:42` (no space) and a path/line-number never match, and
/// neither separator crosses a newline (`[ \t]`, not `\s`) so "The
/// token:\nrotate every hour" keeps "rotate".
fn assignment_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(&format!(
            r"(?i)(--)?\b({NAME_FRAGMENT})([ \t]*=[ \t]*|[ \t]*:[ \t]+)(\S+)"
        ))
        .unwrap()
    })
}

pub(crate) fn scrub_bare(s: &str) -> String {
    assignment_re()
        .replace_all(s, |caps: &Captures| {
            let m = caps.get(0).unwrap();
            let prefix = caps.get(1).map_or("", |g| g.as_str());
            let name = &caps[2];
            let sep = &caps[3];
            let value = &caps[4];
            if not_path_like(s, m.start()) && should_scrub_assignment(name, value) {
                format!("{prefix}{name}{sep}{SECRET_PLACEHOLDER}")
            } else {
                m.as_str().to_string()
            }
        })
        .into_owned()
}

/// `--password value` — a CLI flag and its value separated by whitespace
/// instead of `=`. Requires the leading `--` so ordinary prose ("fix
/// token refresh") never matches.
fn flag_assignment_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(&format!(r"(?i)(--(?:{NAME_FRAGMENT}))([ \t]+)(\S+)")).unwrap())
}

pub(crate) fn scrub_flagged(s: &str) -> String {
    flag_assignment_re()
        .replace_all(s, |caps: &Captures| {
            let flag = &caps[1];
            let sep = &caps[2];
            let value = &caps[3];
            let name = flag.trim_start_matches("--");
            if should_scrub_assignment(name, value) {
                format!("{flag}{sep}{SECRET_PLACEHOLDER}")
            } else {
                caps.get(0).unwrap().as_str().to_string()
            }
        })
        .into_owned()
}
