//! Client for the optional Verdict classifier helper.
//!
//! Verdict runs as a local `worklog verdict serve` process on
//! `CLASSIFIER_ADDR`; a connection failure is treated as "no guess
//! available", not an error. See spec 004.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::routing_contract::{Classifier, CLASSIFIER_ADDR};
use crate::verdict_contract::Ranking;

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
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    examples: &'a BTreeMap<String, Vec<String>>,
}

impl Classifier for VerdictClassifier {
    fn classify(
        &self,
        state: &Value,
        options: &[String],
        examples: &BTreeMap<String, Vec<String>>,
    ) -> Result<Option<Ranking>> {
        let sent = self
            .client
            .post(format!("{}/classify", self.base_url))
            .json(&ClassifyRequest {
                state,
                options,
                examples,
            })
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
        Ok(resp.json::<Ranking>().ok())
    }
}

#[derive(Deserialize)]
struct MatchResponse {
    matches: Vec<bool>,
}

/// Whether each of `texts` matches `query`, via Verdict's `/match`.
/// Unlike `classify`, an unreachable helper is an error: "no match" would
/// read as a failed check.
pub fn match_texts(query: &str, texts: &[String]) -> Result<Vec<bool>> {
    let client = Client::builder().timeout(Duration::from_secs(60)).build()?;
    match_texts_at(&client, &format!("http://{CLASSIFIER_ADDR}"), query, texts)
}

fn match_texts_at(
    client: &Client,
    base_url: &str,
    query: &str,
    texts: &[String],
) -> Result<Vec<bool>> {
    let resp = client
        .post(format!("{base_url}/match"))
        .json(&serde_json::json!({ "query": query, "states": texts }))
        .send()
        .context("verdict not reachable")?
        .error_for_status()?;
    let matches = resp.json::<MatchResponse>()?.matches;
    if matches.len() != texts.len() {
        bail!(
            "verdict answered {} of {} texts",
            matches.len(),
            texts.len()
        );
    }
    Ok(matches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use serde_json::json;

    fn unreachable_classifier() -> VerdictClassifier {
        // Bind then drop to obtain a port nothing is listening on.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        VerdictClassifier::with_client(
            Client::builder()
                .timeout(Duration::from_secs(1))
                .build()
                .unwrap(),
            format!("http://127.0.0.1:{port}"),
        )
    }

    fn at(server: &MockServer) -> VerdictClassifier {
        VerdictClassifier::with_client(Client::builder().build().unwrap(), server.base_url())
    }

    #[test]
    fn unreachable_is_none() {
        let state = json!({"title": "aws console"});
        let options = vec!["aws-cert".to_string()];
        let got = unreachable_classifier().classify(&state, &options, &BTreeMap::new());
        assert_eq!(got.unwrap(), None);
    }

    #[test]
    fn classify_returns_ranking_on_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/classify").json_body(json!({
                "state": {"title": "aws console"},
                "options": ["aws-cert", "worklog"],
                "examples": {"aws-cert": ["IAM policy"]}
            }));
            then.status(200).json_body(json!({
                "ranking": [
                    {"id": "aws-cert", "probability": 0.97},
                    {"id": "worklog", "probability": 0.4}
                ],
                "abstain": 0.1,
                "agreed": true
            }));
        });

        let state = json!({"title": "aws console"});
        let options = vec!["aws-cert".to_string(), "worklog".to_string()];
        let examples = BTreeMap::from([("aws-cert".to_string(), vec!["IAM policy".to_string()])]);
        let got = at(&server)
            .classify(&state, &options, &examples)
            .unwrap()
            .unwrap();
        assert_eq!(got.ranking.len(), 2);
        assert_eq!(got.ranking[0].id, "aws-cert");
        assert_eq!(got.ranking[0].probability, 0.97);
        assert_eq!(got.ranking[1].id, "worklog");
        assert_eq!(got.abstain, 0.1);
        assert!(got.agreed);
    }

    #[test]
    fn classify_keeps_agreed_false() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/classify");
            then.status(200).json_body(json!({
                "ranking": [{"id": "a", "probability": 0.5}],
                "abstain": 0.1,
                "agreed": false
            }));
        });
        let options = vec!["a".to_string()];
        let got = at(&server).classify(&json!({}), &options, &BTreeMap::new());
        assert!(!got.unwrap().unwrap().agreed);
    }

    #[test]
    fn classify_returns_none_on_error_status() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/classify");
            then.status(500);
        });
        let options = vec!["aws-cert".to_string()];
        let got = at(&server).classify(&json!({}), &options, &BTreeMap::new());
        assert_eq!(got.unwrap(), None);
    }

    #[test]
    fn classify_returns_none_on_malformed_body() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/classify");
            then.status(200).json_body(json!({"choice": "a"}));
        });
        let options = vec!["a".to_string()];
        let got = at(&server).classify(&json!({}), &options, &BTreeMap::new());
        assert_eq!(got.unwrap(), None);
    }

    #[test]
    fn match_texts_returns_one_bool_per_text() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/match")
                .json_body(json!({"query": "q", "states": ["one", "two"]}));
            then.status(200)
                .json_body(json!({"matches": [true, false]}));
        });
        let texts = vec!["one".to_string(), "two".to_string()];
        let got = match_texts_at(&Client::new(), &server.base_url(), "q", &texts);
        assert_eq!(got.unwrap(), vec![true, false]);
    }

    #[test]
    fn match_texts_errors_when_unreachable() {
        let c = unreachable_classifier();
        let got = match_texts_at(&c.client, &c.base_url, "q", &["x".to_string()]);
        assert!(
            got.is_err(),
            "an unreachable helper must not read as no-match"
        );
    }

    #[test]
    fn match_texts_errors_on_error_status() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/match");
            then.status(500);
        });
        let got = match_texts_at(&Client::new(), &server.base_url(), "q", &["x".to_string()]);
        assert!(got.is_err());
    }

    #[test]
    fn match_texts_errors_on_short_answer() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/match");
            then.status(200).json_body(json!({"matches": [true]}));
        });
        let texts = vec!["a".to_string(), "b".to_string()];
        let got = match_texts_at(&Client::new(), &server.base_url(), "q", &texts);
        assert!(
            got.is_err(),
            "one answer for two texts must not be zipped short"
        );
    }

    #[test]
    fn match_texts_empty_input_is_empty() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/match");
            then.status(200).json_body(json!({"matches": []}));
        });
        let got = match_texts_at(&Client::new(), &server.base_url(), "q", &[]);
        assert_eq!(got.unwrap(), Vec::<bool>::new());
    }
}
