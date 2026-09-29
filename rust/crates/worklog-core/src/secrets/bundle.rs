//! All secrets live in ONE keychain item (account [`ACCOUNT`]) as
//! a JSON map. macOS asks "Always Allow" per item *and* per binary hash;
//! worklog is ad-hoc signed, so every rebuild/upgrade is a new binary.
//! One item = one prompt per upgrade instead of one per secret.

use anyhow::{Context, Result};
use std::collections::HashMap;

pub const ACCOUNT: &str = "secrets";

/// Raw per-account keychain access. Keychain in prod, HashMap in tests.
pub trait Raw {
    fn get(&self, account: &str) -> Result<Option<String>>;
    fn set(&self, account: &str, value: &str) -> Result<()>;
    fn delete(&self, account: &str) -> Result<bool>;
}

/// Load the bundle. First run: fold the pre-bundle per-key items into
/// it (one last prompt each), save it, and drop the old items.
pub fn load(raw: &dyn Raw) -> Result<HashMap<String, String>> {
    if let Some(json) = raw.get(ACCOUNT)? {
        return serde_json::from_str(&json).context("parsing keychain secret bundle");
    }
    let mut map = HashMap::new();
    for &key in super::KNOWN_KEYS {
        if let Some(v) = raw.get(key)? {
            map.insert(key.to_owned(), v);
        }
    }
    save(raw, &map)?;
    for key in map.keys() {
        let _ = raw.delete(key);
    }
    Ok(map)
}

pub fn save(raw: &dyn Raw, map: &HashMap<String, String>) -> Result<()> {
    raw.set(ACCOUNT, &serde_json::to_string(map)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    struct Fake {
        items: RefCell<HashMap<String, String>>,
        reads: Cell<usize>,
    }
    impl Raw for Fake {
        fn get(&self, a: &str) -> Result<Option<String>> {
            self.reads.set(self.reads.get() + 1);
            Ok(self.items.borrow().get(a).cloned())
        }
        fn set(&self, a: &str, v: &str) -> Result<()> {
            self.items.borrow_mut().insert(a.into(), v.into());
            Ok(())
        }
        fn delete(&self, a: &str) -> Result<bool> {
            Ok(self.items.borrow_mut().remove(a).is_some())
        }
    }

    #[test]
    fn migrates_legacy_items_then_reads_one_item() {
        let raw = Fake::default();
        raw.set("jira_account_id", "abc").unwrap();
        raw.set("slack_user_token", "xoxp").unwrap();

        let map = load(&raw).unwrap();
        assert_eq!(map.get("jira_account_id").map(String::as_str), Some("abc"));
        assert_eq!(
            map.get("slack_user_token").map(String::as_str),
            Some("xoxp")
        );
        // Legacy items gone; only the bundle remains.
        assert_eq!(raw.items.borrow().len(), 1);
        assert!(raw.items.borrow().contains_key(ACCOUNT));

        // After migration: exactly ONE keychain read (= one prompt max).
        raw.reads.set(0);
        let again = load(&raw).unwrap();
        assert_eq!(again, map);
        assert_eq!(raw.reads.get(), 1);
    }

    #[test]
    fn empty_keychain_creates_empty_bundle() {
        let raw = Fake::default();
        assert!(load(&raw).unwrap().is_empty());
        raw.reads.set(0);
        load(&raw).unwrap();
        assert_eq!(raw.reads.get(), 1);
    }
}
