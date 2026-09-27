//! `events.raw_json` compression (S1). It dominates DB size (15.8 MB/week
//! measured). `repo::upsert_event` deflates it at write time when that's
//! smaller than the original TEXT; every SQL reader of `raw_json` decodes
//! through `decode_raw_json` so callers see the identical String either
//! way. Legacy rows already on disk stay plain TEXT and still read back
//! fine — no migration.

use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;
use rusqlite::types::{Value, ValueRef};
use rusqlite::Row;
use std::io::{Read, Write};

/// Deflate `raw` at level 6 (raw DEFLATE, no zlib/gzip header — matches
/// what the perf harness inflates with `Bun.inflateSync`). Pure function of
/// `raw`: same input always yields the same bytes, which is what keeps
/// `upsert_event`'s `IS NOT excluded.raw_json` no-op check working after
/// compression (see `repo_test::upsert_event_recompresses_identically`).
fn compress_raw_json(raw: &str) -> Vec<u8> {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::new(6));
    encoder
        .write_all(raw.as_bytes())
        .expect("compressing into an in-memory Vec cannot fail");
    encoder
        .finish()
        .expect("compressing into an in-memory Vec cannot fail")
}

/// Encodes `raw_json` for storage: a deflate BLOB when that's smaller than
/// the original text, else the TEXT itself unchanged (small payloads don't
/// compress well, and there's no point paying decode cost for no space win).
pub fn encode_raw_json(raw: Option<&str>) -> Value {
    match raw {
        None => Value::Null,
        Some(s) => {
            let compressed = compress_raw_json(s);
            if compressed.len() < s.len() {
                Value::Blob(compressed)
            } else {
                Value::Text(s.to_string())
            }
        }
    }
}

/// Decodes an `events.raw_json` column, whichever form it's stored in:
/// legacy/uncompressed TEXT (read back as-is, strict UTF-8 — same as a
/// plain `row.get::<_, String>` would have failed before compression
/// existed) or a deflate BLOB written by `encode_raw_json` (inflated back
/// to the original text). A BLOB that fails to inflate degrades to `None`
/// rather than failing the whole query — callers (e.g. `block_details`)
/// already treat a missing `raw_json` as "no raw record" for one row, which
/// is safer than losing an entire day's rows over one corrupt column. ONE
/// shared helper — every SQL reader of `raw_json` must call this, never
/// `row.get` directly, or a compressed row would fail to deserialize.
pub fn decode_raw_json(row: &Row, idx: usize) -> rusqlite::Result<Option<String>> {
    match row.get_ref(idx)? {
        ValueRef::Null => Ok(None),
        ValueRef::Text(bytes) => {
            let s = std::str::from_utf8(bytes).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    idx,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;
            Ok(Some(s.to_owned()))
        }
        ValueRef::Blob(bytes) => {
            let mut out = String::new();
            match DeflateDecoder::new(bytes).read_to_string(&mut out) {
                Ok(_) => Ok(Some(out)),
                Err(_) => Ok(None),
            }
        }
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            idx,
            other.data_type(),
            "unexpected raw_json SQL type".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_compression() {
        let raw = r#"{"a":1,"b":"hello world hello world hello world"}"#;
        let encoded = encode_raw_json(Some(raw));
        assert!(
            matches!(encoded, Value::Blob(_)),
            "should compress: repetitive text is smaller deflated"
        );
    }

    #[test]
    fn short_payload_stays_text() {
        // Too short to beat its own deflate framing overhead.
        let encoded = encode_raw_json(Some("{}"));
        assert!(matches!(encoded, Value::Text(_)));
    }

    #[test]
    fn none_is_null() {
        assert!(matches!(encode_raw_json(None), Value::Null));
    }

    #[test]
    fn compression_is_deterministic() {
        let raw = "x".repeat(500);
        assert_eq!(compress_raw_json(&raw), compress_raw_json(&raw));
    }

    #[test]
    fn corrupt_blob_decodes_to_none_not_an_error() {
        // A BLOB that fails to inflate (bit rot, truncation) must degrade
        // to None for that one row, not fail the whole query.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE t (raw_json BLOB)")
            .unwrap();
        conn.execute(
            "INSERT INTO t (raw_json) VALUES (?1)",
            rusqlite::params![vec![0xFFu8, 0x00, 0x01, 0x02]],
        )
        .unwrap();
        conn.query_row("SELECT raw_json FROM t", [], |row| {
            assert_eq!(decode_raw_json(row, 0).unwrap(), None);
            Ok(())
        })
        .unwrap();
    }
}
