//! OS-keychain secret storage.
//!
//! Wraps the `keyring` crate with a fixed `"worklog"` service prefix so all
//! secrets show up grouped in Keychain Access / secret-service / Credential
//! Manager. In `cfg(test)` builds we swap the backend for an in-process
//! HashMap so unit tests never touch the developer's real keychain, and so
//! CI runners with no keyring daemon still pass.

use anyhow::Result;

/// Service name registered with the OS keychain. Scopes all worklog secrets.
pub const SERVICE: &str = "worklog";

/// Secrets the app knows about by name. The setup wizard walks this list.
pub const KNOWN_KEYS: &[&str] = &[
    "jira_email",
    "jira_api_token",
    "jira_base_url",
    "github_token",
    "github_user",
    "tempo_api_token",
    "google_client_id",
    "google_client_secret",
    "google_refresh_token",
    "anthropic_api_key",
];

/// Map each known key to the Python-era `.env` variable name so Rust
/// collectors can read tokens the user entered via the old
/// `worklog setup` wizard without forcing a migration.
fn env_var_for(key: &str) -> Option<&'static str> {
    Some(match key {
        "jira_email" => "WORKLOG_JIRA_EMAIL",
        "jira_api_token" => "WORKLOG_JIRA_TOKEN",
        "jira_base_url" => "WORKLOG_JIRA_BASE_URL",
        "github_token" => "WORKLOG_GITHUB_TOKEN",
        "github_user" => "WORKLOG_GITHUB_USER",
        "tempo_api_token" => "WORKLOG_TEMPO_TOKEN",
        "google_client_id" => "WORKLOG_GOOGLE_CLIENT_ID",
        "google_client_secret" => "WORKLOG_GOOGLE_CLIENT_SECRET",
        "google_refresh_token" => "WORKLOG_GOOGLE_REFRESH_TOKEN",
        "anthropic_api_key" => "ANTHROPIC_API_KEY",
        _ => return None,
    })
}

/// Override for the `.env` fallback path. Primarily for tests.
#[cfg(not(test))]
const ENV_FILE_PATH_OVERRIDE: &str = "WORKLOG_ENV_FILE";

/// Read a value from the XDG `.env` file that the Python CLI writes to
/// `~/.config/worklog/.env`. Returns `None` if the file or key is missing.
///
/// Never used under `cfg(test)` — test builds use the in-memory HashMap.
#[cfg(not(test))]
fn read_env_file(key: &str) -> Option<String> {
    let env_name = env_var_for(key)?;
    let path = if let Some(p) = std::env::var_os(ENV_FILE_PATH_OVERRIDE) {
        std::path::PathBuf::from(p)
    } else {
        let home = dirs::home_dir()?;
        home.join(".config/worklog/.env")
    };
    let bytes = std::fs::read(&path).ok()?;
    let text = std::str::from_utf8(&bytes).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (k, v) = line.split_once('=')?;
        if k.trim() != env_name {
            continue;
        }
        return Some(strip_quotes(v.trim()).to_owned());
    }
    None
}

#[cfg(not(test))]
fn strip_quotes(s: &str) -> &str {
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// Environment variable that, if set to a path, forces secrets into a JSON
/// file rather than the OS keychain. Exclusively for tests and CI — never
/// advertised in the user docs. The setup wizard ignores this path.
#[cfg(not(test))]
const ENV_FILE_BACKEND: &str = "WORKLOG_SECRETS_FILE";

#[cfg(not(test))]
mod backend {
    use super::*;
    use anyhow::Context;
    use keyring::Entry;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn file_backend_path() -> Option<PathBuf> {
        std::env::var_os(ENV_FILE_BACKEND)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    }

    fn read_file_store(p: &std::path::Path) -> Result<HashMap<String, String>> {
        if !p.exists() {
            return Ok(HashMap::new());
        }
        let bytes = std::fs::read(p).with_context(|| format!("reading {}", p.display()))?;
        if bytes.is_empty() {
            return Ok(HashMap::new());
        }
        let store: HashMap<String, String> = serde_json::from_slice(&bytes)
            .with_context(|| format!("parsing {} as JSON secret store", p.display()))?;
        Ok(store)
    }

    fn write_file_store(p: &std::path::Path, store: &HashMap<String, String>) -> Result<()> {
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("mkdir {}", parent.display()))?;
        }
        let bytes = serde_json::to_vec_pretty(store)?;
        std::fs::write(p, bytes).with_context(|| format!("writing {}", p.display()))?;
        Ok(())
    }

    fn entry(key: &str) -> Result<Entry> {
        Entry::new(SERVICE, key).with_context(|| format!("opening keyring entry for {key}"))
    }

    pub fn set(key: &str, value: &str) -> Result<()> {
        if let Some(path) = file_backend_path() {
            let mut store = read_file_store(&path)?;
            store.insert(key.to_owned(), value.to_owned());
            return write_file_store(&path, &store);
        }
        entry(key)?
            .set_password(value)
            .with_context(|| format!("writing secret {key}"))
    }

    pub fn get(key: &str) -> Result<Option<String>> {
        let primary = if let Some(path) = file_backend_path() {
            read_file_store(&path)?.get(key).cloned()
        } else {
            match entry(key)?.get_password() {
                Ok(v) => Some(v),
                Err(keyring::Error::NoEntry) => None,
                Err(e) => return Err(e).with_context(|| format!("reading secret {key}")),
            }
        };
        if primary.is_some() {
            return Ok(primary);
        }
        // Fall back to the Python-era .env file so existing installs keep
        // working without a migration step. Applies to both the real
        // keychain backend and the file-backed shim used by integration
        // tests — otherwise tests can't verify the fallback end-to-end.
        Ok(super::read_env_file(key))
    }

    pub fn delete(key: &str) -> Result<bool> {
        if let Some(path) = file_backend_path() {
            let mut store = read_file_store(&path)?;
            let existed = store.remove(key).is_some();
            if existed {
                write_file_store(&path, &store)?;
            }
            return Ok(existed);
        }
        match entry(key)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(e).with_context(|| format!("deleting secret {key}")),
        }
    }
}

#[cfg(test)]
mod backend {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    fn store() -> &'static Mutex<HashMap<String, String>> {
        use std::sync::OnceLock;
        static STORE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
        STORE.get_or_init(|| Mutex::new(HashMap::new()))
    }

    pub fn set(key: &str, value: &str) -> Result<()> {
        store()
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }

    pub fn get(key: &str) -> Result<Option<String>> {
        Ok(store().lock().unwrap().get(key).cloned())
    }

    pub fn delete(key: &str) -> Result<bool> {
        Ok(store().lock().unwrap().remove(key).is_some())
    }
}

pub fn set(key: &str, value: &str) -> Result<()> {
    backend::set(key, value)
}
pub fn get(key: &str) -> Result<Option<String>> {
    backend::get(key)
}
pub fn delete(key: &str) -> Result<bool> {
    backend::delete(key)
}

/// Fetch a required secret or return a helpful error pointing at the
/// commands that would set it.
pub fn require(key: &str) -> Result<String> {
    match get(key)? {
        Some(v) if !v.is_empty() => Ok(v),
        _ => anyhow::bail!(
            "missing secret `{key}`. Set it with `worklog secret set {key}` or run `worklog setup`."
        ),
    }
}

/// Status of a single known secret, for `worklog doctor` and the wizard.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SecretStatus {
    pub key: &'static str,
    pub present: bool,
}

pub fn audit() -> Vec<SecretStatus> {
    KNOWN_KEYS
        .iter()
        .map(|&k| SecretStatus {
            key: k,
            present: matches!(get(k), Ok(Some(_))),
        })
        .collect()
}

#[cfg(test)]
mod tests {
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
}
