//! Secret scrubbing — replaces detected tokens, keys, passwords and other
//! secrets with `clues_contract::SECRET_PLACEHOLDER` before a value is
//! stored or sent off-machine (spec 006, D-03). Populated by T007:
//! `scrub_secrets`.

use std::sync::OnceLock;

use regex::Regex;

use crate::clues_contract::SECRET_PLACEHOLDER;

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
            r"|\b(?:AKIA|ASIA)[A-Z0-9]{16}\b",
            r"|\bxox[abprs]-[A-Za-z0-9-]{8,}\b",
            r"|\bsk-[A-Za-z0-9_-]{20,}\b",
            r"|\b(?:sk|rk)_live_[A-Za-z0-9]{10,}\b",
            r"|\bAIza[A-Za-z0-9_-]{35}\b",
            r"|\beyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\b",
        ))
        .unwrap()
    })
}

/// `Authorization: Bearer <token>` / bare `Bearer <token>` — the word
/// `Bearer` is kept, only the token is scrubbed.
fn bearer_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(Bearer\s+)[A-Za-z0-9\-._~+/=]+").unwrap())
}

/// `https://user:pass@host` → `https://[secret]@host`.
fn url_creds_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(https?://)[^\s:/@]+:[^\s@]+@").unwrap())
}

/// `NAME=value` / `NAME: value` (also matches after `export ` or `--`,
/// since neither is part of the match) where NAME contains one of the
/// secret-ish keywords. The name is kept, the value becomes the
/// placeholder.
fn assignment_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)(--)?\b([\w.-]*(?:token|secret|password|passwd|api_key|private_key)[\w.-]*)(\s*[:=]\s*)\S+",
        )
        .unwrap()
    })
}

/// `--password value` — a CLI flag and its value separated by whitespace
/// instead of `=`. Requires the leading `--` so ordinary prose ("fix
/// token refresh") never matches.
fn flag_assignment_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)(--[\w.-]*(?:token|secret|password|passwd|api_key|private_key)[\w.-]*)(\s+)\S+",
        )
        .unwrap()
    })
}

/// Redact tokens, keys, passwords and private keys only. Emails and IPs
/// stay raw — this is the storage-time scrub (D-03).
pub fn scrub_secrets(s: &str) -> String {
    let s = whole_match_re().replace_all(s, SECRET_PLACEHOLDER).into_owned();
    let s = bearer_re()
        .replace_all(&s, format!("${{1}}{SECRET_PLACEHOLDER}").as_str())
        .into_owned();
    let s = url_creds_re()
        .replace_all(&s, format!("${{1}}{SECRET_PLACEHOLDER}@").as_str())
        .into_owned();
    let s = assignment_re()
        .replace_all(&s, format!("${{1}}${{2}}${{3}}{SECRET_PLACEHOLDER}").as_str())
        .into_owned();
    flag_assignment_re()
        .replace_all(&s, format!("${{1}}${{2}}{SECRET_PLACEHOLDER}").as_str())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_ghp() -> String {
        format!("ghp_{}", "a".repeat(36))
    }

    #[test]
    fn scrub_github_token() {
        let token = fake_ghp();
        let input = format!("auth with {token} please");
        assert_eq!(
            scrub_secrets(&input),
            "auth with [secret] please".to_string()
        );
    }

    #[test]
    fn scrub_github_pat() {
        let token = format!("github_pat_{}", "b".repeat(30));
        let input = format!("token: {token}");
        assert_eq!(scrub_secrets(&input), "token: [secret]");
    }

    #[test]
    fn scrub_aws_access_key_id() {
        let key = format!("AKIA{}", "B".repeat(16));
        let input = format!("key id is {key} in the config");
        assert_eq!(scrub_secrets(&input), "key id is [secret] in the config");
    }

    #[test]
    fn scrub_aws_secret_access_key_assignment() {
        let value = "s".repeat(40);
        let input = format!("aws_secret_access_key = {value}");
        assert_eq!(scrub_secrets(&input), "aws_secret_access_key = [secret]");
    }

    #[test]
    fn scrub_slack_token() {
        let token = format!("xoxb-{}-{}-{}", "1".repeat(11), "2".repeat(12), "c".repeat(24));
        let input = format!("slack token {token} leaked");
        assert_eq!(scrub_secrets(&input), "slack token [secret] leaked");
    }

    #[test]
    fn scrub_anthropic_style_api_key() {
        let key = format!("sk-ant-{}", "x".repeat(30));
        let input = format!("export ANTHROPIC_API_KEY={key}");
        // ANTHROPIC_API_KEY contains API_KEY -> name kept, value scrubbed.
        assert_eq!(scrub_secrets(&input), "export ANTHROPIC_API_KEY=[secret]");
    }

    #[test]
    fn scrub_bare_sk_api_key() {
        let key = format!("sk-{}", "y".repeat(30));
        let input = format!("using key {key} now");
        assert_eq!(scrub_secrets(&input), "using key [secret] now");
    }

    #[test]
    fn scrub_stripe_live_key() {
        let key = format!("sk_live_{}", "9".repeat(24));
        let input = format!("stripe key {key}");
        assert_eq!(scrub_secrets(&input), "stripe key [secret]");
    }

    #[test]
    fn scrub_google_api_key() {
        let key = format!("AIza{}", "Q".repeat(35));
        let input = format!("maps key {key}");
        assert_eq!(scrub_secrets(&input), "maps key [secret]");
    }

    #[test]
    fn scrub_jwt() {
        let jwt = format!(
            "eyJ{}.eyJ{}.{}",
            "a".repeat(20),
            "b".repeat(20),
            "c".repeat(20)
        );
        let input = format!("jwt is {jwt} ok");
        assert_eq!(scrub_secrets(&input), "jwt is [secret] ok");
    }

    #[test]
    fn scrub_bearer_header() {
        let tok = "z".repeat(40);
        let input = format!("Authorization: Bearer {tok}");
        assert_eq!(scrub_secrets(&input), "Authorization: Bearer [secret]");
    }

    #[test]
    fn scrub_pem_private_key_block() {
        let body: String = std::iter::repeat("QUJDREVGRw==\n").take(3).collect();
        let input = format!(
            "before\n-----BEGIN RSA PRIVATE KEY-----\n{body}-----END RSA PRIVATE KEY-----\nafter"
        );
        assert_eq!(scrub_secrets(&input), "before\n[secret]\nafter");
    }

    #[test]
    fn scrub_url_credentials() {
        let pass = "p".repeat(12);
        let input = format!("clone https://user:{pass}@host.example.com/repo.git now");
        assert_eq!(
            scrub_secrets(&input),
            "clone https://[secret]@host.example.com/repo.git now"
        );
    }

    #[test]
    fn scrub_name_equals_value_assignment() {
        let value = "t".repeat(30);
        let input = format!("GITHUB_TOKEN={value}");
        assert_eq!(scrub_secrets(&input), "GITHUB_TOKEN=[secret]");
    }

    #[test]
    fn scrub_name_colon_value_assignment() {
        let value = "h".repeat(16);
        let input = format!("password: {value}");
        assert_eq!(scrub_secrets(&input), "password: [secret]");
    }

    #[test]
    fn scrub_cli_flag_password_space() {
        let value = "s".repeat(16);
        let input = format!("mysql --password {value} -u root");
        assert_eq!(
            scrub_secrets(&input),
            "mysql --password [secret] -u root"
        );
    }

    #[test]
    fn scrub_cli_flag_password_equals() {
        let value = "s".repeat(16);
        let input = format!("mysql --password={value} -u root");
        assert_eq!(scrub_secrets(&input), "mysql --password=[secret] -u root");
    }

    #[test]
    fn scrub_does_not_alter_prose() {
        let input = "fix token refresh in auth";
        assert_eq!(scrub_secrets(input), input);
    }

    #[test]
    fn scrub_does_not_alter_file_paths() {
        let input = "see rust/crates/worklog-core/src/scrub.rs for details";
        assert_eq!(scrub_secrets(input), input);
    }

    #[test]
    fn scrub_does_not_alter_git_sha() {
        let input = "commit 1234567890abcdef1234567890abcdef12345678 fixed it";
        assert_eq!(scrub_secrets(input), input);
    }

    #[test]
    fn scrub_does_not_alter_uuid() {
        let input = "id 550e8400-e29b-41d4-a716-446655440000 created";
        assert_eq!(scrub_secrets(input), input);
    }

    #[test]
    fn scrub_does_not_alter_bare_password_word() {
        let input = "please enter your password to continue";
        assert_eq!(scrub_secrets(input), input);
    }

    #[test]
    fn scrub_secrets_leaves_email_intact() {
        let input = "contact tomas.ari.palsson@apro.is for access";
        assert_eq!(scrub_secrets(input), input);
    }

    #[test]
    fn scrub_secrets_is_idempotent() {
        let token = fake_ghp();
        let value = "t".repeat(20);
        let input = format!(
            "token {token}, GITHUB_TOKEN={value}, Bearer {value2}",
            value2 = "q".repeat(30)
        );
        let once = scrub_secrets(&input);
        let twice = scrub_secrets(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn scrub_identifiers_redacts_email() {
        let input = "sent to tomas.ari.palsson@apro.is yesterday";
        assert_eq!(
            scrub_identifiers(input),
            "sent to [secret] yesterday"
        );
    }

    #[test]
    fn scrub_identifiers_redacts_ipv4() {
        let input = "connect to 10.20.30.40 over vpn";
        assert_eq!(scrub_identifiers(input), "connect to [secret] over vpn");
    }

    #[test]
    fn scrub_identifiers_redacts_aws_account_id() {
        let input = format!("account {} is billed", "4".repeat(12));
        assert_eq!(scrub_identifiers(&input), "account [secret] is billed");
    }

    #[test]
    fn scrub_identifiers_also_redacts_secrets() {
        let token = fake_ghp();
        let input = format!("token {token} in use");
        assert_eq!(scrub_identifiers(&input), "token [secret] in use");
    }
}
