//! Secret scrubbing — replaces detected tokens, keys, passwords and other
//! secrets with `clues_contract::SECRET_PLACEHOLDER` before a value is
//! stored or sent off-machine (spec 006, D-03). Populated by T007:
//! `scrub_secrets`.

/// Redact tokens, keys, passwords and private keys only. Emails and IPs
/// stay raw — this is the storage-time scrub (D-03).
pub fn scrub_secrets(_s: &str) -> String {
    unimplemented!("T007")
}

/// `scrub_secrets` plus emails, IPv4 addresses and 12-digit AWS account
/// ids. Used for anything leaving the machine (D-02).
pub fn scrub_identifiers(_s: &str) -> String {
    unimplemented!("T007")
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
