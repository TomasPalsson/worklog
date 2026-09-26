//! `TEXT` column round-trip for the change-log's contract enums — split
//! out of `change_log.rs` to keep that file within the size guard.

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;

/// Round-trips an enum through its own `serde(rename_all = "snake_case")`
/// so the `TEXT` column and any JSON wire form can never drift apart.
pub(crate) fn to_col<T: Serialize>(value: T) -> String {
    match serde_json::to_value(value).expect("contract enums always serialize") {
        serde_json::Value::String(s) => s,
        _ => unreachable!("contract enums serialize to a JSON string"),
    }
}

pub(crate) fn from_col<T: DeserializeOwned>(raw: &str) -> Result<T> {
    serde_json::from_value(serde_json::Value::String(raw.to_string()))
        .with_context(|| format!("unknown change-log column value '{raw}'"))
}

/// A malformed `field`/`source` column fails the whole row the same way a
/// SQLite type mismatch would — both mean the row can't be trusted.
pub(crate) fn column_error(e: anyhow::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
}
