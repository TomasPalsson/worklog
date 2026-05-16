//! Thin blocking HTTP client for the local worklog daemon.
//!
//! Before this module existed, every single-block mutation (assign a
//! ticket, fix a duration, delete, …) had to be done with a hand-written
//! `curl` against `127.0.0.1:9323`. That is the friction `worklog block`
//! and `worklog summary` remove — they speak to the daemon through here.
//!
//! The daemon answers errors as `{"error": "<message>"}` with a non-2xx
//! status. [`decode`] unwraps that shape so the CLI surfaces the daemon's
//! own message (e.g. "block 7 not found") instead of a raw HTTP dump.

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde_json::Value;
use worklog_core::http;

/// Where the daemon's TCP listener lives. Matches the `--tcp` default in
/// `worklog daemon` and the address `ensure_daemon_running` probes.
pub const BASE: &str = "http://127.0.0.1:9323";

/// GET `path` and decode the JSON body into `T`.
pub fn get<T: DeserializeOwned>(path: &str) -> Result<T> {
    let client = http::client()?;
    let resp = client
        .get(format!("{BASE}{path}"))
        .send()
        .with_context(|| format!("GET {path} — is the worklog daemon running?"))?;
    decode(resp)
}

/// POST `body` as JSON to `path` and decode the JSON response into `T`.
pub fn post<T: DeserializeOwned>(path: &str, body: &Value) -> Result<T> {
    let client = http::client()?;
    let resp = client
        .post(format!("{BASE}{path}"))
        .json(body)
        .send()
        .with_context(|| format!("POST {path} — is the worklog daemon running?"))?;
    decode(resp)
}

/// Turn a daemon response into `T`, or an `anyhow` error carrying the
/// daemon's own `{"error": ...}` message when the status is non-2xx.
fn decode<T: DeserializeOwned>(resp: reqwest::blocking::Response) -> Result<T> {
    let status = resp.status();
    let text = resp.text().context("reading daemon response body")?;
    if !status.is_success() {
        if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&text) {
            if let Some(msg) = map.get("error").and_then(Value::as_str) {
                anyhow::bail!("{msg}");
            }
        }
        anyhow::bail!("daemon returned HTTP {status}: {text}");
    }
    serde_json::from_str(&text).context("decoding daemon JSON response")
}
