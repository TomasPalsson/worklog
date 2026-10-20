//! Customer-safe text rules for generated lines: no evidence talk, no
//! version numbers spelled out, no Slack, no plural first verb. Split out
//! of `line_text.rs` to keep that file under the repo's size guard.

use regex::Regex;
use std::sync::OnceLock;

const EVIDENCE_PHRASES: &[&str] = &[
    "liggja fyrir",
    "liggur fyrir",
    "vísbending",
    "staðfesta ekki",
    "staðfestir ekki",
    "ekki staðfest",
    "óljóst hvort",
    "ekki ljóst hvort",
];

const MIN_VERB_LETTERS: usize = 3;

/// The first customer-safe rule `text` breaks, as an Icelandic reason
/// naming the rule; `None` when the text is clean.
pub fn unsafe_reason(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    if let Some(phrase) = EVIDENCE_PHRASES.iter().find(|p| lower.contains(*p)) {
        return Some(format!(
            "talar um vísbendingar eða hvað er óstaðfest (\"{phrase}\") — lýstu aðeins því sem var gert"
        ));
    }
    if spelled_number_re().is_match(text) {
        return Some(
            "útgáfunúmer skrifað með orðum — skrifaðu það með tölustöfum, t.d. 5.5".into(),
        );
    }
    if slack_re().is_match(text) {
        return Some("nefnir Slack — nefndu aldrei Slack eða innri rásir".into());
    }
    if starts_with_plural_verb(text) {
        return Some(
            "byrjar á sögn í fleirtölu — byrjaðu á sögn í eintölu, t.d. \"Lagaði\"".into(),
        );
    }
    None
}

fn spelled_number_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        let english = "zero|one|two|three|four|five|six|seven|eight|nine|ten";
        let icelandic = "núll|einn|tveir|þrír|fjórir|fimm|sex|sjö|átta|níu";
        Regex::new(&format!(
            r"(?i)\b(?:(?:{english})\s+point|point\s+(?:{english})|(?:{icelandic})\s+komma|komma\s+(?:{icelandic}))\b"
        ))
        .unwrap()
    })
}

fn slack_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\bslack\b").unwrap())
}

fn starts_with_plural_verb(text: &str) -> bool {
    let word = text
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_end_matches([',', '.', ';', ':', '!', '?']);
    word.chars().count() >= MIN_VERB_LETTERS
        && word.chars().all(char::is_alphabetic)
        && word.ends_with('u')
}
