use super::*;

#[test]
fn scrub_json_scrubs_nested_strings_and_secret_keys() {
    let token = fake_ghp();
    let input = serde_json::json!({
        "command": format!("curl -H 'Authorization: Bearer {token}' x"),
        "nested": [{"api_token": "plain-looking-value"}],
        "count": 3,
        "path": "/Users/me/src/main.rs"
    });
    let out = scrub_json(&input);
    assert!(!out.to_string().contains(&token));
    assert_eq!(out["nested"][0]["api_token"], "[secret]");
    assert_eq!(out["count"], 3);
    assert_eq!(out["path"], "/Users/me/src/main.rs");
}

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
    let token = format!(
        "xoxb-{}-{}-{}",
        "1".repeat(11),
        "2".repeat(12),
        "c".repeat(24)
    );
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
    let body = "QUJDREVGRw==\n".repeat(3);
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
    assert_eq!(scrub_secrets(&input), "mysql --password [secret] -u root");
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
    assert_eq!(scrub_identifiers(input), "sent to [secret] yesterday");
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

// ───────────────────── URL credentials, any scheme ─────────────────────

#[test]
fn scrub_url_credentials_postgres() {
    let input = "connect to postgres://alice:hunter2@db.example.com/app";
    assert_eq!(
        scrub_secrets(input),
        "connect to postgres://[secret]@db.example.com/app"
    );
}

#[test]
fn scrub_url_credentials_mongodb_srv() {
    let input = "uri mongodb+srv://alice:hunter2@cluster0.example.mongodb.net/db";
    assert_eq!(
        scrub_secrets(input),
        "uri mongodb+srv://[secret]@cluster0.example.mongodb.net/db"
    );
}

#[test]
fn scrub_url_credentials_redis_empty_user() {
    let input = "url redis://:hunter2@cache.example.com:6379";
    assert_eq!(
        scrub_secrets(input),
        "url redis://[secret]@cache.example.com:6379"
    );
}

// ───────────────────── Authorization: Basic / case-insensitive bearer ─────────────────────

#[test]
fn scrub_authorization_basic_header() {
    let b64 = "dXNlcjpwYXNz";
    let input = format!("Authorization: Basic {b64}");
    assert_eq!(scrub_secrets(&input), "Authorization: Basic [secret]");
}

#[test]
fn scrub_bearer_is_case_insensitive() {
    let tok = "z".repeat(40);
    let input = format!("authorization: bearer {tok}");
    assert_eq!(scrub_secrets(&input), "authorization: bearer [secret]");
}

// ───────────────────── extended name keywords ─────────────────────

#[test]
fn scrub_pwd_assignment() {
    let value = "s".repeat(16);
    let input = format!("pwd={value}");
    assert_eq!(scrub_secrets(&input), "pwd=[secret]");
}

#[test]
fn scrub_apikey_assignment() {
    let value = "s".repeat(20);
    let input = format!("apikey={value}");
    assert_eq!(scrub_secrets(&input), "apikey=[secret]");
}

#[test]
fn scrub_x_api_key_header() {
    let value = "s".repeat(20);
    let input = format!("x-api-key: {value}");
    assert_eq!(scrub_secrets(&input), "x-api-key: [secret]");
}

#[test]
fn scrub_access_key_assignment() {
    let value = "s".repeat(20);
    let input = format!("access_key={value}");
    assert_eq!(scrub_secrets(&input), "access_key=[secret]");
}

#[test]
fn scrub_credential_assignment() {
    let value = "s".repeat(20);
    let input = format!("credential={value}");
    assert_eq!(scrub_secrets(&input), "credential=[secret]");
}

#[test]
fn scrub_name_ending_in_key_suffix() {
    let value = "s".repeat(20);
    let input = format!("OPENAI_KEY={value}");
    assert_eq!(scrub_secrets(&input), "OPENAI_KEY=[secret]");
}

#[test]
fn scrub_does_not_alter_monkey_assignment() {
    let input = "monkey=1";
    assert_eq!(scrub_secrets(input), input);
}

#[test]
fn scrub_does_not_alter_keyboard_word() {
    let input = "keyboard: mechanical";
    assert_eq!(scrub_secrets(input), input);
}

// ───────────────────── quoted assignment values ─────────────────────

#[test]
fn scrub_quoted_password_value_double_quotes() {
    let input = r#"PASSWORD="correct horse battery staple""#;
    assert_eq!(scrub_secrets(input), "PASSWORD=[secret]");
}

#[test]
fn scrub_quoted_password_value_single_quotes() {
    let input = "PASSWORD='correct horse battery staple'";
    assert_eq!(scrub_secrets(input), "PASSWORD=[secret]");
}

// ───────────────────── shell credential flags ─────────────────────

#[test]
fn scrub_curl_dash_u_user_pass() {
    let input = "curl -u alice:hunter2 https://api.example.com";
    assert_eq!(
        scrub_secrets(input),
        "curl -u [secret] https://api.example.com"
    );
}

#[test]
fn scrub_curl_long_flag_user_pass() {
    let input = "curl --user alice:hunter2 https://api.example.com";
    assert_eq!(
        scrub_secrets(input),
        "curl -u [secret] https://api.example.com"
    );
}

#[test]
fn scrub_sshpass_dash_p() {
    let value = "s".repeat(12);
    let input = format!("sshpass -p {value} ssh host");
    assert_eq!(scrub_secrets(&input), "sshpass -p [secret] ssh host");
}

#[test]
fn scrub_mysql_dash_p_attached() {
    let value = "s".repeat(12);
    let input = format!("mysql -u root -p{value} -h db.example.com");
    assert_eq!(
        scrub_secrets(&input),
        "mysql -u root -p[secret] -h db.example.com"
    );
}

#[test]
fn scrub_mysqldump_dash_p_spaced() {
    let value = "s".repeat(12);
    let input = format!("mysqldump -u root -p {value} mydb");
    assert_eq!(scrub_secrets(&input), "mysqldump -u root -p [secret] mydb");
}

#[test]
fn scrub_bare_dash_p_outside_mysql_context_is_untouched() {
    // `-p` alone (no mysql/mysqldump/mariadb keyword anywhere) must never
    // be treated as a password flag — e.g. `mkdir -p some-dir`.
    let input = "mkdir -p some-dir";
    assert_eq!(scrub_secrets(input), input);
}

// ───────────────────── tokens ─────────────────────

#[test]
fn scrub_gitlab_pat() {
    let token = format!("glpat-{}", "a".repeat(20));
    let input = format!("token {token} in use");
    assert_eq!(scrub_secrets(&input), "token [secret] in use");
}

#[test]
fn scrub_npm_token() {
    let token = format!("npm_{}", "a".repeat(36));
    let input = format!("registry token {token}");
    assert_eq!(scrub_secrets(&input), "registry token [secret]");
}

#[test]
fn scrub_slack_app_level_token() {
    let token = format!("xapp-1-A0{}", "1".repeat(20));
    let input = format!("app token {token} configured");
    assert_eq!(scrub_secrets(&input), "app token [secret] configured");
}

#[test]
fn scrub_slack_exchange_token() {
    let token = format!("xoxe.xoxp-{}", "1".repeat(20));
    let input = format!("exchange token {token} here");
    assert_eq!(scrub_secrets(&input), "exchange token [secret] here");
}

#[test]
fn scrub_slack_token_e_variant() {
    let token = format!("xoxe-{}", "1".repeat(20));
    let input = format!("token {token} used");
    assert_eq!(scrub_secrets(&input), "token [secret] used");
}

// ───────────────────── unterminated PEM block ─────────────────────

#[test]
fn scrub_unterminated_pem_block_to_end_of_string() {
    let input =
        "before\n-----BEGIN RSA PRIVATE KEY-----\nMIIBOgIBAAJBAK...\nmore lines with no end marker";
    assert_eq!(scrub_secrets(input), "before\n[secret]");
}

// ───────────────────── negatives still hold ─────────────────────

#[test]
fn scrub_still_does_not_alter_prose_after_new_families() {
    let input = "fix token refresh in auth";
    assert_eq!(scrub_secrets(input), input);
}

#[test]
fn scrub_still_does_not_alter_git_sha_after_new_families() {
    let input = "commit 1234567890abcdef1234567890abcdef12345678 fixed it";
    assert_eq!(scrub_secrets(input), input);
}

#[test]
fn scrub_still_does_not_alter_uuid_after_new_families() {
    let input = "id 550e8400-e29b-41d4-a716-446655440000 created";
    assert_eq!(scrub_secrets(input), input);
}

#[test]
fn scrub_new_families_are_idempotent() {
    let input = format!(
        "postgres://alice:hunter2@db/app, PASSWORD=\"correct horse\", sshpass -p {}",
        "s".repeat(12)
    );
    let once = scrub_secrets(&input);
    let twice = scrub_secrets(&once);
    assert_eq!(once, twice);
}
