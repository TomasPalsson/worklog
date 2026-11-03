use super::*;
use crate::collectors::jira::JiraAuth;
use crate::collectors::tempo::TempoAuth;
use crate::http;
use httpmock::prelude::*;
use serde_json::json;

fn jira(server: &MockServer) -> JiraAuth {
    JiraAuth {
        base_url: server.base_url(),
        email: "x".into(),
        token: "t".into(),
    }
}

fn tempo(server: &MockServer) -> TempoAuth {
    TempoAuth {
        token: "t".into(),
        author: "a".into(),
        base_url: format!("{}/tempo", server.base_url()),
    }
}

fn acct(id: &str, name: &str) -> AllowedAccount {
    AllowedAccount {
        id: id.into(),
        name: name.into(),
    }
}

fn mock_story_type(server: &MockServer) {
    server.mock(|when, then| {
        when.method(GET)
            .path("/rest/api/3/issue/createmeta/GENAI/issuetypes");
        then.status(200)
            .json_body(json!({ "issueTypes": [{"id": "7", "name": "Story"}] }));
    });
}

/// Tempo's Forge "Account" field: createmeta lists the field with an
/// object schema and no `allowedValues` at all.
fn mock_forge_createmeta(server: &MockServer) {
    mock_story_type(server);
    server.mock(|when, then| {
        when.method(GET)
            .path("/rest/api/3/issue/createmeta/GENAI/issuetypes/7");
        then.status(200).json_body(json!({ "fields": [
            {"fieldId": "customfield_11530", "required": true, "name": "Account",
             "schema": {"type": "object",
                        "custom": "ari:cloud:ecosystem::extension/x/y/static/account"}}
        ]}));
    });
}

#[test]
fn forge_field_accounts_come_from_tempo_project_links() {
    let server = MockServer::start();
    mock_forge_createmeta(&server);
    server.mock(|when, then| {
        when.method(GET).path("/rest/api/3/project/GENAI");
        then.status(200)
            .json_body(json!({ "id": "13120", "key": "GENAI" }));
    });
    server.mock(|when, then| {
        when.method(GET).path("/tempo/account-links/project/13120");
        then.status(200).json_body(json!({ "results": [
            {"id": 1, "account": {"self": "https://api.tempo.io/4/accounts/155"}},
            {"id": 2, "account": {"self": "https://api.tempo.io/4/accounts/396"}}
        ]}));
    });
    server.mock(|when, then| {
        when.method(GET)
            .path("/tempo/accounts")
            .query_param("status", "OPEN");
        then.status(200).json_body(json!({ "results": [
            {"id": 155, "key": "VORU", "name": "[P] Vitinn vöruþróun", "global": false},
            {"id": 53, "key": "ANDES", "name": "Andes", "global": false},
            {"id": 9, "key": "GLOBAL", "name": "[O] Global", "global": true}
        ]}));
    });
    let got = allowed_accounts(
        &jira(&server),
        Some(&tempo(&server)),
        "GENAI",
        "Story",
        "customfield_11530",
        &http::client().unwrap(),
    )
    .unwrap();
    // Linked and open (155) plus global (9). Andes is not linked; 396 is
    // linked but not open.
    assert_eq!(
        got,
        vec![acct("155", "[P] Vitinn vöruþróun"), acct("9", "[O] Global")]
    );
}

#[test]
fn forge_field_without_tempo_names_the_missing_token() {
    let server = MockServer::start();
    mock_forge_createmeta(&server);
    let err = allowed_accounts(
        &jira(&server),
        None,
        "GENAI",
        "Story",
        "customfield_11530",
        &http::client().unwrap(),
    )
    .unwrap_err();
    assert!(format!("{err:#}").contains("tempo_api_token"), "{err:#}");
}

#[test]
fn listed_field_still_reads_createmeta_and_never_calls_tempo() {
    let server = MockServer::start();
    mock_story_type(&server);
    server.mock(|when, then| {
        when.method(GET)
            .path("/rest/api/3/issue/createmeta/GENAI/issuetypes/7");
        then.status(200).json_body(json!({ "fields": [
            {"fieldId": "customfield_10100", "allowedValues": [{"id": "42", "name": "Acme"}]}
        ]}));
    });
    let tempo_hit = server.mock(|when, then| {
        when.method(GET).path_matches(regex::Regex::new("^/tempo/").unwrap());
        then.status(500);
    });
    let got = allowed_accounts(
        &jira(&server),
        Some(&tempo(&server)),
        "GENAI",
        "Story",
        "customfield_10100",
        &http::client().unwrap(),
    )
    .unwrap();
    assert_eq!(got, vec![acct("42", "Acme")]);
    tempo_hit.assert_hits(0);
}
