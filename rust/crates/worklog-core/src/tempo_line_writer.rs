//! Ticket (Tempo) line text written exactly like a billing line's: the same
//! Icelandic system prompt, the rich block input and `line_text::validate`,
//! with up to [`MAX_ATTEMPTS`] tries. Phases are split so the daemon never
//! holds the sqlite connection across a model call.

use crate::clues_send;
use crate::estimate::ModelInvoker;
use crate::line_text::{self, line_text_schema, reply_to_text, SYSTEM_PROMPT_IS};
use crate::tempo_line_contract::TempoLineKey;
use rusqlite::Connection;

const MAX_ATTEMPTS: usize = 3;

/// Phase 1 (under the lock): the ticket's `DescriptionInput` as the user message.
pub fn prepare(conn: &Connection, key: &TempoLineKey) -> Result<String, String> {
    let input = clues_send::build_ticket_line_input(conn, key).map_err(|e| e.to_string())?;
    serde_json::to_string(&input).map_err(|e| e.to_string())
}

/// Phase 2 (no connection): one model round trip, unvalidated.
pub fn invoke(user_msg: &str, invoker: &dyn ModelInvoker, model: &str) -> Result<String, String> {
    reply_to_text(invoker.invoke(SYSTEM_PROMPT_IS, user_msg, &line_text_schema(), model))
}

/// Invokes and validates, re-asking with the rejection reason up to
/// [`MAX_ATTEMPTS`] times in total.
pub fn write(user_msg: &str, invoker: &dyn ModelInvoker, model: &str) -> Result<String, String> {
    let mut msg = user_msg.to_string();
    let mut reason = String::new();
    for _ in 0..MAX_ATTEMPTS {
        reason = match invoke(&msg, invoker, model).and_then(|t| line_text::validate(&t)) {
            Ok(text) => return Ok(text),
            Err(reason) => reason,
        };
        msg = format!(
            "{user_msg}\n\nSíðasta svar var hafnað: {reason}. Skrifaðu það aftur og fylgdu öllum reglunum (engin skráarnöfn, engin verkfæranöfn, 1–3 setningar á íslensku)."
        );
    }
    Err(format!("{reason} (reynt {MAX_ATTEMPTS} sinnum)"))
}

#[path = "tempo_line_writer_test.rs"]
#[cfg(test)]
mod tests;
