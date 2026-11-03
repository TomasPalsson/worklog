//! Pure helpers: accent-blind account name matching for
//! `worklog account allowed [QUERY]` and `ticket create --account`.

use anyhow::{bail, Result};
use worklog_core::jira_assist_contract::AllowedAccount;

/// Lowercase and strip Icelandic accents so "voruthroun" finds "vöruþróun".
pub fn fold(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' => "a".into(),
            'é' => "e".into(),
            'í' => "i".into(),
            'ó' | 'ö' => "o".into(),
            'ú' => "u".into(),
            'ý' => "y".into(),
            'æ' => "ae".into(),
            'þ' => "th".into(),
            'ð' => "d".into(),
            c => c.to_string(),
        })
        .collect()
}

/// Accounts whose folded name contains every word of the folded query.
pub fn matching<'a>(accounts: &'a [AllowedAccount], query: &str) -> Vec<&'a AllowedAccount> {
    let q = fold(query);
    accounts
        .iter()
        .filter(|a| {
            let name = fold(&a.name);
            q.split_whitespace().all(|w| name.contains(w))
        })
        .collect()
}

/// A non-empty, all-digit value is an account id, never a name.
pub fn is_account_id(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())
}

/// Ids pass through; anything else must pick exactly one account by name.
/// A blank value is an error: it would match every account.
pub fn resolve_account(accounts: &[AllowedAccount], value: &str) -> Result<String> {
    if is_account_id(value) {
        return Ok(value.to_string());
    }
    if value.trim().is_empty() {
        bail!("--account is empty; pass an account id or words from its name");
    }
    match matching(accounts, value).as_slice() {
        [] => bail!("no account matches {value}; run worklog account allowed <word>"),
        [one] => Ok(one.id.clone()),
        many => {
            let lines: Vec<String> = many
                .iter()
                .take(10)
                .map(|a| format!("{}  {}", a.id, a.name))
                .collect();
            bail!(
                "{} accounts match {value}; pass the id with --account:\n{}",
                many.len(),
                lines.join("\n")
            )
        }
    }
}

#[cfg(test)]
#[path = "account_match_test.rs"]
mod tests;
