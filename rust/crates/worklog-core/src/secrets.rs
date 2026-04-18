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
    "tempo_api_token",
    "google_client_id",
    "google_client_secret",
    "google_refresh_token",
    "anthropic_api_key",
];

/// Environment variable that, if set to a path, forces secrets into a JSON
/// file rather than the OS keychain. Exclusively for tests and CI — never
/// advertised in the user docs. The setup wizard ignores this path.
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
            std::fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
        }
        let bytes = serde_json::to_vec_pretty(store)?;
        std::fs::write(p, bytes).with_context(|| format!("writing {}", p.display()))?;
        Ok(())
    }

    fn entry(key: &str) -> Result<Entry> {
        Entry::new(SERVICE, key)
            .with_context(|| format!("opening keyring entry for {key}"))
    }

    pub fn set(key: &str, value: &str) -> Result<()> {
        if let Some(path) = file_backend_path() {
            let mut store = read_file_store(&path)?;
            store.insert(key.to_owned(), value.to_owned());
            return write_file_store(&path, &store);
        }
        entry(key)?.set_password(value)
            .with_context(|| format!("writing secret {key}"))
    }

    pub fn get(key: &str) -> Result<Option<String>> {
        if let Some(path) = file_backend_path() {
            return Ok(read_file_store(&path)?.get(key).cloned());
        }
        match entry(key)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e).with_context(|| format!("reading secret {key}")),
        }
    }

    pub fn delete(key: &str) -> Result<bool> {
        if let Some(path) = file_backend_path() {
            let mut store = read_file_store(&path)?;
            let existed = store.remove(key).is_some();
            if existed { write_file_store(&path, &store)?; }
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
        store().lock().unwrap().insert(key.to_owned(), value.to_owned());
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

/// Status of a single known secret, for `worklog doctor` and the wizard.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SecretStatus {
    pub key:     &'static str,
    pub present: bool,
}

pub fn audit() -> Vec<SecretStatus> {
    KNOWN_KEYS
        .iter()
        .map(|&k| SecretStatus {
            key:     k,
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
}
