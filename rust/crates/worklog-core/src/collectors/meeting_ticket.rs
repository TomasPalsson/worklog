//! Internal-meeting ticket: calendar events whose guests are all from the
//! owner's company land on a configured Jira ticket. Off unless both
//! `meeting_ticket` and `company_domain` are set in `personal.toml`.

use serde::Deserialize;

use crate::personal::ConfigFile;

#[derive(Debug, Clone, Default)]
pub struct MeetingConfig {
    pub ticket: Option<String>,
    pub domain: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Attendee {
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub resource: bool,
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

    /// The meeting ticket for an event with these attendees, if on and internal.
    pub fn ticket_for(&self, attendees: &[Attendee]) -> Option<String> {
        let (t, d) = (self.ticket.as_ref()?, self.domain.as_ref()?);
        is_internal(attendees, d).then(|| t.clone())
    }
}

/// True when every real attendee email ends with `@<domain>`. Rooms and
/// groups are ignored; no attendees counts as internal.
pub fn is_internal(attendees: &[Attendee], domain: &str) -> bool {
    let suffix = format!("@{}", domain.to_lowercase());
    attendees
        .iter()
        .filter(|a| {
            let e = a.email.to_lowercase();
            !a.resource
                && !e.ends_with("@resource.calendar.google.com")
                && !e.ends_with("@group.calendar.google.com")
        })
        .all(|a| a.email.to_lowercase().ends_with(&suffix))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(email: &str, resource: bool) -> Attendee {
        Attendee {
            email: email.into(),
            resource,
        }
    }

    #[test]
    fn all_internal() {
        assert!(is_internal(
            &[a("x@apro.is", false), a("y@apro.is", false)],
            "apro.is"
        ));
    }

    #[test]
    fn one_external_guest() {
        assert!(!is_internal(
            &[a("x@apro.is", false), a("z@other.com", false)],
            "apro.is"
        ));
    }

    #[test]
    fn lookalike_domain_is_external() {
        assert!(!is_internal(&[a("x@notapro.is", false)], "apro.is"));
    }

    #[test]
    fn rooms_and_groups_ignored() {
        let at = [
            a("x@apro.is", false),
            a("room@weird.com", true),
            a("c_1@resource.calendar.google.com", false),
            a("g@group.calendar.google.com", false),
        ];
        assert!(is_internal(&at, "apro.is"));
    }

    #[test]
    fn no_attendees_is_internal() {
        assert!(is_internal(&[], "apro.is"));
    }

    #[test]
    fn case_insensitive() {
        assert!(is_internal(&[a("Tomas@APRO.is", false)], "Apro.IS"));
    }

    #[test]
    fn off_unless_both_keys() {
        let only_ticket = MeetingConfig {
            ticket: Some("APRO-7".into()),
            domain: None,
        };
        assert_eq!(only_ticket.ticket_for(&[]), None);
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
        assert_eq!(c.ticket_for(&[]).as_deref(), Some("APRO-7"));
    }
}
