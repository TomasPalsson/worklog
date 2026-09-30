//! The per-block card kept after a block's raw events are deleted.

use anyhow::Result;
use chrono::NaiveDate;
use rusqlite::Connection;

use crate::digest_contract::BlockDigest;
use crate::models::Event;

pub fn horizon(_today: NaiveDate) -> NaiveDate {
    todo!()
}

pub fn eval_evidence(_events: &[Event]) -> (Vec<String>, Vec<String>) {
    todo!()
}

pub fn build_digest(_conn: &Connection, _block_id: i64) -> Result<BlockDigest> {
    todo!()
}

pub fn write_digest(_conn: &Connection, _block_id: i64, _digest: &BlockDigest) -> Result<bool> {
    todo!()
}

pub fn digest_for_block(_conn: &Connection, _block_id: i64) -> Result<Option<BlockDigest>> {
    todo!()
}

pub fn day_is_compressed(_conn: &Connection, _day: &str) -> Result<bool> {
    todo!()
}

#[cfg(test)]
#[path = "block_digest_test.rs"]
mod tests;
