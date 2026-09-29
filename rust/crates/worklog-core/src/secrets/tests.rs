use super::*;
use std::sync::Mutex;

// Test backend is a process-global HashMap; serialise tests so they don't
// stomp on each other.
static LOCK: Mutex<()> = Mutex::new(());

fn clean() {
    for k in KNOWN_KEYS {
        let _ = delete(k);
    }
    let _ = delete("test_roundtrip");
}

#[test]
fn round_trip_secret() {
    let _g = LOCK.lock().unwrap();
    clean();
    assert!(get("test_roundtrip").unwrap().is_none());
    set("test_roundtrip", "s3cret").unwrap();
    assert_eq!(get("test_roundtrip").unwrap().as_deref(), Some("s3cret"));
    assert!(delete("test_roundtrip").unwrap());
    assert!(get("test_roundtrip").unwrap().is_none());
}

#[test]
fn audit_reports_missing_keys() {
    let _g = LOCK.lock().unwrap();
    clean();
    let rows = audit();
    assert_eq!(rows.len(), KNOWN_KEYS.len());
    assert!(rows.iter().all(|r| !r.present));
}

#[test]
fn audit_reports_present_keys() {
    let _g = LOCK.lock().unwrap();
    clean();
    set("jira_email", "tomas@p5.is").unwrap();
    let rows = audit();
    let jira = rows.iter().find(|r| r.key == "jira_email").unwrap();
    assert!(jira.present);
}

#[test]
fn delete_returns_false_when_absent() {
    let _g = LOCK.lock().unwrap();
    clean();
    assert!(!delete("nonexistent_key").unwrap());
}

#[test]
fn env_var_mapping_covers_every_known_key() {
    for k in KNOWN_KEYS {
        assert!(
            env_var_for(k).is_some(),
            "known key {k} has no WORKLOG_* mapping — collectors reading from .env will miss it"
        );
    }
}

#[test]
fn require_errors_when_absent_with_actionable_message() {
    let _g = LOCK.lock().unwrap();
    clean();
    let err = require("jira_api_token").unwrap_err().to_string();
    assert!(err.contains("missing secret"), "err = {err}");
    assert!(err.contains("worklog secret set"), "err = {err}");
}

#[test]
fn require_returns_stored_value() {
    let _g = LOCK.lock().unwrap();
    clean();
    set("jira_email", "t@p5.is").unwrap();
    assert_eq!(require("jira_email").unwrap(), "t@p5.is");
}

// ───────────────────────── LiteLLM / provider keys (v0.7) ─────────────────────────

/// `worklog setup` and `worklog doctor` walk `KNOWN_KEYS` to drive their
/// UI — every new credential the estimator can read MUST be listed here
/// or the wizard silently skips prompting for it and the doctor report
/// hides it. These assertions pin that contract.
#[test]
fn known_keys_include_litellm_provider_settings() {
    for k in &[
        "litellm_base_url",
        "litellm_api_key",
        "litellm_model",
        "worklog_estimator_provider",
    ] {
        assert!(
            KNOWN_KEYS.contains(k),
            "KNOWN_KEYS should include `{k}` so the wizard prompts for it"
        );
    }
}

/// The `.env` fallback exists so Python-era and CI users can drop a
/// flat file instead of touching the keychain. If we add a key but
/// forget the env mapping, those users lose a setting. Mirror the
/// screaming-snake `WORKLOG_*` convention already in use.
#[test]
fn env_var_mapping_for_litellm_keys() {
    assert_eq!(
        env_var_for("litellm_base_url"),
        Some("WORKLOG_LITELLM_BASE_URL")
    );
    assert_eq!(
        env_var_for("litellm_api_key"),
        Some("WORKLOG_LITELLM_API_KEY")
    );
    assert_eq!(env_var_for("litellm_model"), Some("WORKLOG_LITELLM_MODEL"));
    assert_eq!(
        env_var_for("worklog_estimator_provider"),
        Some("WORKLOG_ESTIMATOR_PROVIDER"),
    );
}
