//! Internal-meeting ticket: a calendar event organised by someone at the
//! owner's company whose title names none of the owner's billing
//! customers lands on a configured Jira ticket. Off unless both
//! `meeting_ticket` and `company_domain` are set in `personal.toml`.
//!
//! Guests don't decide it: an internal talk with one outside speaker is
//! still internal, and a customer sync often has no guests on the
//! owner's calendar at all — the customer's name in the title does.

use serde::Deserialize;

use crate::billing_registry::{alias_matches, Customer};
use crate::personal::ConfigFile;

#[derive(Debug, Clone, Default)]
pub struct MeetingConfig {
    pub ticket: Option<String>,
    pub domain: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Organizer {
    #[serde(default)]
    pub email: String,
}

impl MeetingConfig {
    /// Missing file, bad TOML or unset keys all mean "feature off".
    pub fn load() -> Self {
        crate::paths::Paths::resolve()
            .ok()
            .map(|p| Self::load_from(&p.config_dir.join("personal.toml")))
            .unwrap_or_default()
    }

    pub fn load_from(path: &std::path::Path) -> Self {
        let file: ConfigFile = std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| toml::from_str(&raw).ok())
            .unwrap_or_default();
        let clean = |v: Option<String>| v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        Self {
            ticket: clean(file.meeting_ticket),
            domain: clean(file.company_domain),
        }
    }

    /// The meeting ticket for this event, if the feature is on and the
    /// event is internal (see the module doc).
    pub fn ticket_for(
        &self,
        organizer: &str,
        title: &str,
        customers: &[Customer],
    ) -> Option<String> {
        let (t, d) = (self.ticket.as_ref()?, self.domain.as_ref()?);
        is_internal(organizer, title, d, customers).then(|| t.clone())
    }
}

/// Organised from `@<domain>` and no customer named in the title. The
/// owner's own company (a customer whose name or alias is the domain's
/// first label, e.g. "Apro" for apro.is) doesn't count as a customer.
pub fn is_internal(organizer: &str, title: &str, domain: &str, customers: &[Customer]) -> bool {
    let domain = domain.to_lowercase();
    if !organizer.to_lowercase().ends_with(&format!("@{domain}")) {
        return false;
    }
    let own = domain.split('.').next().unwrap_or_default();
    !customers.iter().any(|c| {
        let mut names = std::iter::once(&c.name).chain(&c.aliases);
        let is_own = names.clone().any(|n| n.trim().to_lowercase() == own);
        !is_own && names.any(|n| alias_matches(title, n))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cust(name: &str, aliases: &[&str]) -> Customer {
        Customer {
            id: None,
            name: name.into(),
            aliases: aliases.iter().map(|a| a.to_string()).collect(),
        }
    }

    fn customers() -> Vec<Customer> {
        vec![
            cust("APRÓ", &["Apro", "APRO"]),
            cust("RL", &[]),
            cust("Coripharma", &[]),
        ]
    }

    fn internal(organizer: &str, title: &str) -> bool {
        is_internal(organizer, title, "apro.is", &customers())
    }

    #[test]
    fn this_weeks_calendar() {
        // Regression (2026-10-09): Öryggishugvekja had an outside speaker
        // and was skipped; RL sync named a customer and was not.
        assert!(internal("erlasylvia@apro.is", "Öryggishugvekja"));
        assert!(internal("erlasylvia@apro.is", "APRÓfest"));
        assert!(internal("levy@apro.is", "Argus daily"));
        assert!(!internal("elin@apro.is", "RL sync"));
        assert!(!internal(
            "tomas@apro.is",
            "Coripharma - Sync regarding code interpreter"
        ));
    }

    #[test]
    fn outside_organizer_is_not_internal() {
        assert!(!internal("someone@client.com", "Weekly"));
        assert!(!internal("x@notapro.is", "Weekly"));
        assert!(!internal("", "Weekly"));
    }

    #[test]
    fn customer_name_only_matches_whole_words() {
        // "RL" must not fire inside "URL" or "Herlev".
        assert!(internal("a@apro.is", "URL cleanup"));
    }

    #[test]
    fn case_insensitive() {
        assert!(internal("Tomas@APRO.IS", "Team lunch"));
        assert!(!internal("a@apro.is", "coripharma demo"));
    }

    #[test]
    fn off_unless_both_keys() {
        let only_ticket = MeetingConfig {
            ticket: Some("APRO-7".into()),
            domain: None,
        };
        assert_eq!(only_ticket.ticket_for("a@apro.is", "x", &[]), None);
    }

    #[test]
    fn load_from_reads_keys_and_tolerates_missing() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("personal.toml");
        assert!(MeetingConfig::load_from(&p).ticket.is_none());
        std::fs::write(
            &p,
            "meeting_ticket = \"APRO-7\"\ncompany_domain = \"apro.is\"\n",
        )
        .unwrap();
        let c = MeetingConfig::load_from(&p);
        assert_eq!(
            c.ticket_for("a@apro.is", "Standup", &[]).as_deref(),
            Some("APRO-7")
        );
    }
}
