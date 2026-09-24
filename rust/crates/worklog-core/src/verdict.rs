//! Client for the optional Verdict classifier helper.
//!
//! Verdict runs as a local `worklog verdict serve` process on
//! `CLASSIFIER_ADDR`; a connection failure is treated as "no guess
//! available", not an error. See spec 004.

use std::time::Duration;

use anyhow::Result;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::routing_contract::{Classifier, Guess, CLASSIFIER_ADDR};

/// Embedded helper script for `worklog verdict serve`.
pub const SERVER_SCRIPT: &str = include_str!("../templates/verdict_server.py");

/// HTTP client for the Verdict helper. Production points at
/// `CLASSIFIER_ADDR`; tests point `base_url` at an httpmock server via
/// [`Self::with_client`].
pub struct VerdictClassifier {
    client: Client,
    base_url: String,
}

impl VerdictClassifier {
    /// Production client: `http://CLASSIFIER_ADDR`, 10 s timeout.
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self::with_client(client, format!("http://{CLASSIFIER_ADDR}"))
    }

    pub fn with_client(client: Client, base_url: String) -> Self {
        Self { client, base_url }
    }
}

impl Default for VerdictClassifier {
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
    probability: f64,
    runner_up: f64,
    abstain: f64,
}

impl Classifier for VerdictClassifier {
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
                confidence: body.probability,
                runner_up: body.runner_up,
                abstain: body.abstain,
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

        let classifier = VerdictClassifier::with_client(
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
            then.status(200).json_body(json!({
                "choice": "aws-cert",
                "probability": 0.97,
                "runner_up": 0.4,
                "abstain": 0.1
            }));
        });

        let classifier =
            VerdictClassifier::with_client(Client::builder().build().unwrap(), server.base_url());
        let state = json!({"title": "aws console"});
        let options = vec!["aws-cert".to_string(), "worklog".to_string()];
        let guess = classifier.classify(&state, &options).unwrap().unwrap();
        assert_eq!(guess.folder, "aws-cert");
        assert_eq!(guess.confidence, 0.97);
        assert_eq!(guess.runner_up, 0.4);
        assert_eq!(guess.abstain, 0.1);
    }

    #[test]
    fn classify_returns_none_on_error_status() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/classify");
            then.status(500);
        });

        let classifier =
            VerdictClassifier::with_client(Client::builder().build().unwrap(), server.base_url());
        let state = json!({"title": "aws console"});
        let options = vec!["aws-cert".to_string()];
        assert_eq!(classifier.classify(&state, &options).unwrap(), None);
    }
}
