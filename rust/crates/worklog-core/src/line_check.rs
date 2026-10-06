//! Verdict's two yes/no checks on a generated Tempo line, and the single
//! regenerate when one fails (spec 017 FR-19..FR-21).

use anyhow::Result;

use crate::verdict_contract::LineCheck;

const SPECIFIC_WORK: &str =
    "a specific piece of work such as a feature, fix, file or meeting topic";

/// `None` means not checked (Verdict unreachable or no ticket summary): the
/// line is neither passed nor flagged. `matcher` is `verdict::match_texts`.
pub fn check<F>(matcher: F, line_text: &str, ticket_summary: &str) -> Result<Option<LineCheck>>
where
    F: Fn(&str, &[String]) -> Result<Vec<bool>>,
{
    let summary = ticket_summary.trim();
    if summary.is_empty() {
        return Ok(None);
    }
    let texts = [line_text.to_string()];
    for query in [summary, SPECIFIC_WORK] {
        match matcher(query, &texts) {
            Ok(answers) if answers == [true] => {}
            Ok(_) => return Ok(Some(LineCheck::NeedsLook)),
            Err(_) => return Ok(None),
        }
    }
    Ok(Some(LineCheck::Passed))
}

/// Checks `text`; on a fail asks `regenerate` for one new text and checks that.
/// Returns the text to keep and its status. A failed regenerate keeps the
/// original text, flagged.
pub fn check_with_regenerate<F, R>(
    matcher: F,
    text: String,
    ticket_summary: &str,
    regenerate: R,
) -> Result<(String, Option<LineCheck>)>
where
    F: Fn(&str, &[String]) -> Result<Vec<bool>>,
    R: FnOnce() -> Result<String>,
{
    let first = check(&matcher, &text, ticket_summary)?;
    if first != Some(LineCheck::NeedsLook) {
        return Ok((text, first));
    }
    match regenerate() {
        Ok(again) => {
            let second = check(&matcher, &again, ticket_summary)?;
            Ok((again, second))
        }
        Err(_) => Ok((text, first)),
    }
}

#[cfg(test)]
#[path = "line_check_test.rs"]
mod tests;
