//! Shared HTTP client for collectors.
//!
//! Stage 2 uses blocking reqwest so collectors can stay synchronous — they
//! are one-shot CLI invocations, not long-running services. Stage 3 will
//! introduce an axum IPC server that needs async; we'll split the client
//! out then. Every collector gets the same user-agent, connect timeout,
//! and rustls TLS so the binary stays hermetic (no system openssl).

use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::blocking::{Client, RequestBuilder, Response};

/// Single shared user-agent string. Bumped manually on major changes.
pub const USER_AGENT: &str = concat!("worklog/", env!("CARGO_PKG_VERSION"));

/// Construct a pre-configured blocking client. Cheap; callers may cache it
/// per collector invocation.
pub fn client() -> Result<Client> {
    Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .context("building reqwest blocking client")
}

/// Basic-auth header for `user:token` (used by Atlassian Cloud APIs). The
/// encoded value is memo-able but we rebuild per-call since tokens change
/// between calls in the test harness.
pub fn basic_auth_header(user: &str, token: &str) -> String {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    format!(
        "Basic {}",
        STANDARD.encode(format!("{user}:{token}"))
    )
}

/// Small helper to turn a non-2xx response into an anyhow error that
/// carries the body for debugging.
pub fn check_status(resp: Response) -> Result<Response> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status();
    let url = resp.url().clone();
    let body = resp.text().unwrap_or_else(|_| "<unreadable body>".into());
    anyhow::bail!("HTTP {status} for {url}\n{body}")
}

/// Extension trait so collector code reads linearly.
pub trait RequestBuilderExt {
    fn send_ok(self) -> Result<Response>;
    fn json_ok<T: serde::de::DeserializeOwned>(self) -> Result<T>;
}

impl RequestBuilderExt for RequestBuilder {
    fn send_ok(self) -> Result<Response> {
        let resp = self.send().context("sending request")?;
        check_status(resp)
    }

    fn json_ok<T: serde::de::DeserializeOwned>(self) -> Result<T> {
        let resp = self.send_ok()?;
        resp.json::<T>().context("decoding JSON response")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_auth_header_encodes_correctly() {
        // "tomas@p5.is:secret" → base64 = dG9tYXNAcDUuaXM6c2VjcmV0
        let h = basic_auth_header("tomas@p5.is", "secret");
        assert_eq!(h, "Basic dG9tYXNAcDUuaXM6c2VjcmV0");
    }

    #[test]
    fn client_builds_successfully() {
        let _c = client().unwrap();
    }
}
