//! Integration test exercising the `.env`-fallback path in
//! `secrets::get`. This file is compiled with `cfg(test)` OFF for the crate
//! under test (cargo integration-test mode), so the production keychain
//! backend is what runs — which is exactly what we want to verify.
//!
//! We point `$WORKLOG_SECRETS_FILE` at a tmpdir so the keychain shim goes
//! through the file-backed store (no real keychain prompts), leave that
//! file empty, and then verify that a key present only in the mock
//! `WORKLOG_ENV_FILE` comes back from `get`.

use std::path::PathBuf;

fn write(path: &std::path::Path, body: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

#[test]
fn get_reads_from_env_file_when_keychain_misses() {
    let tmp = tempfile::tempdir().unwrap();
    // Empty file-backed store → keychain misses.
    let store = tmp.path().join("secrets.json");
    write(&store, "{}");
    std::env::set_var("WORKLOG_SECRETS_FILE", &store);

    let env_file = tmp.path().join(".env");
    write(
        &env_file,
        "# comment\n\
         WORKLOG_JIRA_EMAIL=tomas@p5.is\n\
         WORKLOG_JIRA_TOKEN=\"ATATT_value_with_spaces_ok\"\n\
         WORKLOG_GITHUB_USER=TomasPalsson\n",
    );
    std::env::set_var("WORKLOG_ENV_FILE", &env_file);

    assert_eq!(
        worklog_core::secrets::get("jira_email").unwrap().as_deref(),
        Some("tomas@p5.is")
    );
    assert_eq!(
        worklog_core::secrets::get("jira_api_token")
            .unwrap()
            .as_deref(),
        Some("ATATT_value_with_spaces_ok")
    );
    assert_eq!(
        worklog_core::secrets::get("github_user")
            .unwrap()
            .as_deref(),
        Some("TomasPalsson")
    );

    // Unknown keys must not read .env — the mapping table gates access.
    assert_eq!(
        worklog_core::secrets::get("unknown_random_key").unwrap(),
        None
    );

    std::env::remove_var("WORKLOG_SECRETS_FILE");
    std::env::remove_var("WORKLOG_ENV_FILE");
    let _: PathBuf = tmp.path().to_path_buf();
}
