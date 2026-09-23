//! Client for the optional Laya classifier helper.
//!
//! Laya runs as a local `uv run --with laya` process on `LAYA_ADDR`; a
//! connection failure is treated as "no guess available", not an error.
//! See spec 003 T006.

use std::time::Duration;

use anyhow::Result;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::routing_contract::{Classifier, Guess, LAYA_ADDR};

/// Embedded helper script for `worklog laya serve`.
pub const SERVER_SCRIPT: &str = include_str!("../templates/laya_server.py");

/// HTTP client for the Laya helper. Production points at `LAYA_ADDR`;
/// tests point `base_url` at an httpmock server via [`Self::with_client`].
pub struct LayaClassifier {
    client: Client,
    base_url: String,
}

impl LayaClassifier {
    /// Production client: `http://LAYA_ADDR`, 10 s timeout.
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self::with_client(client, format!("http://{LAYA_ADDR}"))
    }

    pub fn with_client(client: Client, base_url: String) -> Self {
        Self { client, base_url }
    }
}

impl Default for LayaClassifier {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize)]
struct ClassifyRequest<'a> {
    state: &'a Value,
    options: &'a [String],
}

#[derive(Deserialize)]
struct ClassifyResponse {
    choice: String,
    confidence: f64,
}

impl Classifier for LayaClassifier {
    fn classify(&self, state: &Value, options: &[String]) -> Result<Option<Guess>> {
        let sent = self
            .client
            .post(format!("{}/classify", self.base_url))
            .json(&ClassifyRequest { state, options })
            .send();
        let resp = match sent {
            Ok(r) => r,
            // Helper not running / connect timeout — leave the event
            // unsorted rather than failing the whole day's routing.
            Err(_) => return Ok(None),
        };
        if !resp.status().is_success() {
            return Ok(None);
        }
        match resp.json::<ClassifyResponse>() {
            Ok(body) => Ok(Some(Guess {
                folder: body.choice,
                confidence: body.confidence,
            })),
            Err(_) => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use serde_json::json;

    #[test]
    fn unreachable_is_none() {
        // Bind then drop to obtain a port nothing is listening on.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let classifier = LayaClassifier::with_client(
            Client::builder()
                .timeout(Duration::from_secs(1))
                .build()
                .unwrap(),
            format!("http://127.0.0.1:{port}"),
        );
        let state = json!({"title": "aws console"});
        let options = vec!["aws-cert".to_string()];
        assert_eq!(classifier.classify(&state, &options).unwrap(), None);
    }

    #[test]
    fn classify_returns_guess_on_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/classify").json_body(
                json!({"state": {"title": "aws console"}, "options": ["aws-cert", "worklog"]}),
            );
            then.status(200)
                .json_body(json!({"choice": "aws-cert", "confidence": 0.97}));
        });

        let classifier =
            LayaClassifier::with_client(Client::builder().build().unwrap(), server.base_url());
        let state = json!({"title": "aws console"});
        let options = vec!["aws-cert".to_string(), "worklog".to_string()];
        let guess = classifier.classify(&state, &options).unwrap().unwrap();
        assert_eq!(guess.folder, "aws-cert");
        assert_eq!(guess.confidence, 0.97);
    }

    #[test]
    fn classify_returns_none_on_error_status() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/classify");
            then.status(500);
        });

        let classifier =
            LayaClassifier::with_client(Client::builder().build().unwrap(), server.base_url());
        let state = json!({"title": "aws console"});
        let options = vec!["aws-cert".to_string()];
        assert_eq!(classifier.classify(&state, &options).unwrap(), None);
    }
}
