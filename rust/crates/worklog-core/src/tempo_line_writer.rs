//! Ticket (Tempo) line text written exactly like a billing line's: the same
//! Icelandic system prompt, the rich block input and `line_text::validate`,
//! with up to [`MAX_ATTEMPTS`] tries. Phases are split so the daemon never
//! holds the sqlite connection across a model call.

use crate::billing_registry::Registry;
use crate::clues_send;
use crate::collectors::jira::{self, JiraAuth};
use crate::estimate::ModelInvoker;
use crate::http;
use crate::line_text::{
    self, foreign_customers, line_text_schema, reply_to_text, KnownCustomer, SYSTEM_PROMPT_IS,
};
use crate::tempo_line_contract::TempoLineKey;
use rusqlite::Connection;

const MAX_ATTEMPTS: usize = 3;
const MAX_TICKET_DESCRIPTION_CHARS: usize = 2000;

/// Phase 1 (under the lock): the ticket's `DescriptionInput` as the user message.
pub fn prepare(conn: &Connection, key: &TempoLineKey) -> Result<String, String> {
    let input = clues_send::build_ticket_line_input(conn, key).map_err(|e| e.to_string())?;
    let mut msg = serde_json::to_value(&input).map_err(|e| e.to_string())?;
    let others = other_tickets(conn, key).map_err(|e| e.to_string())?;
    if !others.is_empty() {
        msg["other_tickets_today"] = others.into();
    }
    // An unreadable registry only drops the customer rule; the line is still written.
    if let Ok(registry) = Registry::load(conn) {
        if let Some(customer) = ticket_customer(conn, key, &registry) {
            msg["ticket_customer"] = customer.into();
        }
        let known: Vec<KnownCustomer> = registry
            .customers
            .iter()
            .map(|c| KnownCustomer {
                name: c.name.clone(),
                aliases: c.aliases.clone(),
            })
            .collect();
        msg["known_customers"] = serde_json::json!(known);
    }
    Ok(msg.to_string())
}

/// The customer the ticket's summary names, else its parent's summary.
fn ticket_customer(conn: &Connection, key: &TempoLineKey, registry: &Registry) -> Option<String> {
    let (summary, parent): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT summary, parent_summary FROM jira_tickets WHERE key = ?1",
            [&key.jira_issue],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok()?;
    [summary, parent]
        .into_iter()
        .flatten()
        .find_map(|text| registry.customer_in_text(&text))
}

/// "KEY: summary" for every other ticket the day has: non-personal blocks
/// or a line-text row. Sorted, deduped.
fn other_tickets(conn: &Connection, key: &TempoLineKey) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT d.k, t.summary FROM (
             SELECT jira_issue AS k FROM blocks
              WHERE day = ?1 AND is_personal = 0 AND ignored_at IS NULL
             UNION SELECT jira_issue FROM tempo_line_texts WHERE day = ?1
         ) d LEFT JOIN jira_tickets t ON t.key = d.k
         WHERE d.k IS NOT NULL AND d.k != '' AND d.k != ?2
         ORDER BY d.k",
    )?;
    let rows = stmt.query_map([&key.day, &key.jira_issue], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
    })?;
    rows.map(|row| {
        row.map(|(k, summary)| match summary.filter(|s| !s.is_empty()) {
            Some(s) => format!("{k}: {s}"),
            None => k,
        })
    })
    .collect()
}

/// Phase 2 (no connection): adds the Jira ticket's title and description to
/// `user_msg` so the model knows what the ticket is for. Best-effort: when
/// Jira can't be reached the message goes out as it was.
fn with_ticket(user_msg: &str, key: &TempoLineKey) -> String {
    let detail = JiraAuth::from_secrets()
        .and_then(|auth| jira::fetch_detail_with(&auth, &key.jira_issue, &http::client()?));
    match detail {
        Ok(d) => add_ticket(user_msg, &d.summary, &d.description),
        Err(e) => {
            tracing::debug!("ticket context for {}: {e:#}", key.jira_issue);
            user_msg.to_string()
        }
    }
}

fn add_ticket(user_msg: &str, summary: &str, description: &str) -> String {
    let Ok(mut input) = serde_json::from_str::<serde_json::Value>(user_msg) else {
        return user_msg.to_string();
    };
    for (field, text, cap) in [
        ("ticket_summary", summary, 200),
        (
            "ticket_description",
            description,
            MAX_TICKET_DESCRIPTION_CHARS,
        ),
    ] {
        let text = clues_send::scrub_capped(text, cap);
        if !text.is_empty() {
            input[field] = text.into();
        }
    }
    input.to_string()
}

/// Phase 2 (no connection): one model round trip, unvalidated.
pub fn invoke(
    user_msg: &str,
    invoker: &dyn ModelInvoker,
    model: &str,
    system: &str,
) -> Result<String, String> {
    reply_to_text(invoker.invoke(system, user_msg, &line_text_schema(), model))
}

/// Adds the ticket context ([`with_ticket`]), then invokes and validates,
/// re-asking with the rejection reason up to [`MAX_ATTEMPTS`] times in total.
pub fn write(
    user_msg: &str,
    key: &TempoLineKey,
    invoker: &dyn ModelInvoker,
    model: &str,
) -> Result<String, String> {
    write_with_system(user_msg, key, invoker, model, SYSTEM_PROMPT_IS)
}

/// [`write`] with a caller-supplied system prompt.
pub fn write_with_system(
    user_msg: &str,
    key: &TempoLineKey,
    invoker: &dyn ModelInvoker,
    model: &str,
    system: &str,
) -> Result<String, String> {
    let customers = customer_data(user_msg);
    let user_msg = &with_ticket(user_msg, key);
    let mut msg = user_msg.to_string();
    let mut reason = String::new();
    for _ in 0..MAX_ATTEMPTS {
        reason = match invoke(&msg, invoker, model, system)
            .and_then(|t| line_text::validate(&t))
            .and_then(|t| check_customers(t, &customers))
        {
            Ok(text) => return Ok(text),
            Err(reason) => reason,
        };
        msg = format!(
            "{user_msg}\n\nSíðasta svar var hafnað: {reason}. Skrifaðu það aftur og fylgdu öllum reglunum (engin skráarnöfn, engin verkfæranöfn, 1–3 setningar á íslensku, ekkert tal um vísbendingar eða hvað er óstaðfest, útgáfunúmer með tölustöfum, ekki nefna Slack, byrja á sögn í eintölu)."
        );
    }
    Err(format!("{reason} (reynt {MAX_ATTEMPTS} sinnum)"))
}

/// The `ticket_customer` and `known_customers` keys `prepare` wrote; `None`
/// when `known_customers` is missing or unreadable.
fn customer_data(user_msg: &str) -> Option<(Option<String>, Vec<KnownCustomer>)> {
    let input: serde_json::Value = serde_json::from_str(user_msg).ok()?;
    let known = serde_json::from_value(input.get("known_customers")?.clone()).ok()?;
    let ticket = input["ticket_customer"].as_str().map(str::to_string);
    Some((ticket, known))
}

fn check_customers(
    text: String,
    customers: &Option<(Option<String>, Vec<KnownCustomer>)>,
) -> Result<String, String> {
    let Some((ticket, known)) = customers else {
        return Ok(text);
    };
    let foreign = foreign_customers(&text, ticket.as_deref(), known);
    if foreign.is_empty() {
        Ok(text)
    } else {
        Err(format!("nefnir annan viðskiptavin: {}", foreign.join(", ")))
    }
}

#[path = "tempo_line_writer_test.rs"]
#[cfg(test)]
mod tests;
