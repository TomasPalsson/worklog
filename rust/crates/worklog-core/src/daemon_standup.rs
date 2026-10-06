//! Daemon route handlers for the standup (spec 018, FR-17, FR-20, FR-22..24).
//! Routes are registered in `daemon.rs`. Drafting never posts; posting
//! sends exactly the text the Owner submits.

use anyhow::Context;
use axum::extract::State;
use axum::Json;
use chrono::{NaiveDate, Utc};
use serde::Deserialize;

use crate::collectors::slack::SLACK_API;
use crate::daemon::{ApiError, Shared};
use crate::daily_helpers_contract::{PostOutcome, StandupDraft, SLACK_DAILY_CHANNEL_KEY};
use crate::estimate::{self, ModelInvoker};
use crate::routing_contract::SLACK_TOKEN_KEY;
use crate::{envfile, http, secrets, slack_post, standup, tz};

#[derive(Deserialize)]
pub struct DraftBody {
    /// Present on a regenerate (FR-24): the draft to reword.
    pub previous: Option<StandupDraft>,
}

#[derive(Deserialize)]
pub struct PostBody {
    pub text: String,
}

pub async fn draft(
    State(state): State<Shared>,
    Json(body): Json<DraftBody>,
) -> Result<Json<StandupDraft>, ApiError> {
    let today = tz::local_date(Utc::now());
    Ok(Json(
        draft_with(state, today, body.previous, estimate::build_invoker).await?,
    ))
}

pub async fn post(Json(body): Json<PostBody>) -> Result<Json<PostOutcome>, ApiError> {
    let today = tz::local_date(Utc::now());
    let token = tokio::task::spawn_blocking(|| secrets::get(SLACK_TOKEN_KEY).ok().flatten())
        .await
        .context("spawn_blocking")?;
    let channel = envfile::read(SLACK_DAILY_CHANNEL_KEY);
    Ok(Json(
        post_with(SLACK_API, token, channel, today, body.text).await?,
    ))
}

async fn draft_with<F>(
    state: Shared,
    today: NaiveDate,
    previous: Option<StandupDraft>,
    make_invoker: F,
) -> Result<StandupDraft, ApiError>
where
    F: FnOnce() -> anyhow::Result<Box<dyn ModelInvoker>> + Send + 'static,
{
    let draft = tokio::task::spawn_blocking(move || {
        let model = make_invoker()?;
        let conn = state.conn.blocking_lock();
        match previous {
            Some(previous) => standup::regenerate(&conn, today, model.as_ref(), &previous),
            None => standup::draft(&conn, today, model.as_ref()),
        }
    })
    .await
    .context("spawn_blocking")??;
    Ok(draft)
}

async fn post_with(
    base_url: &str,
    token: Option<String>,
    channel: Option<String>,
    today: NaiveDate,
    text: String,
) -> Result<PostOutcome, ApiError> {
    if text.trim().is_empty() {
        return Err(ApiError::bad_request(anyhow::anyhow!(
            "standup text is empty"
        )));
    }
    let Some(token) = token.filter(|t| !t.is_empty()) else {
        return Ok(PostOutcome::SlackRefused {
            error: format!("{SLACK_TOKEN_KEY} is not set"),
        });
    };
    let base_url = base_url.to_string();
    let outcome = tokio::task::spawn_blocking(move || {
        slack_post::post_to_daily(
            &http::client()?,
            &base_url,
            &token,
            channel.as_deref(),
            today,
            &text,
        )
    })
    .await
    .context("spawn_blocking")??;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::state_from_conn;
    use crate::db::open_memory;
    use crate::estimate::FixedInvoker;
    use httpmock::prelude::*;
    use serde_json::{json, Value};
    use std::sync::{Arc, Mutex};

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 6).unwrap()
    }

    fn reply() -> Value {
        json!({"today": ["a"], "next": ["b"], "blockers": ["c"]})
    }

    struct Spy(Arc<Mutex<Vec<String>>>);

    impl ModelInvoker for Spy {
        fn invoke(&self, _s: &str, user: &str, _sc: &Value, _m: &str) -> anyhow::Result<Value> {
            self.0.lock().unwrap().push(user.to_string());
            Ok(reply())
        }
    }

    struct Down;

    impl ModelInvoker for Down {
        fn invoke(&self, _s: &str, _u: &str, _sc: &Value, _m: &str) -> anyhow::Result<Value> {
            Err(anyhow::anyhow!("model down"))
        }
    }

    fn state() -> Shared {
        state_from_conn(open_memory().unwrap())
    }

    fn slack_thread(server: &MockServer) -> (httpmock::Mock<'_>, httpmock::Mock<'_>) {
        let search = server.mock(|when, then| {
            when.method(GET).path("/search.messages");
            then.status(200)
                .json_body(json!({"ok": true, "messages": {"matches": [{
                "text": "Daily:thread 6 Oct", "ts": "111.1", "permalink": "https://x/p",
                "channel": {"id": "C1"}}]}}));
        });
        let post = server.mock(|when, then| {
            when.method(POST)
                .path("/chat.postMessage")
                // wrong impl: posts the generated draft, not the Owner's edit (FR-20)
                .json_body(
                    json!({"channel": "C1", "thread_ts": "111.1", "text": "my edited text"}),
                );
            then.status(200)
                .json_body(json!({"ok": true, "ts": "222.2"}));
        });
        (search, post)
    }

    async fn post_to(
        server: &MockServer,
        token: Option<&str>,
        channel: Option<&str>,
        text: &str,
    ) -> Result<PostOutcome, ApiError> {
        post_with(
            &server.base_url(),
            token.map(str::to_string),
            channel.map(str::to_string),
            today(),
            text.to_string(),
        )
        .await
    }

    #[tokio::test(flavor = "current_thread")]
    async fn draft_returns_the_models_answers() {
        let got = draft_with(state(), today(), None, || {
            Ok(Box::new(FixedInvoker(reply())))
        })
        .await
        .unwrap_or_else(|_| panic!("draft should succeed"));
        assert_eq!(got.today, ["a"]);
        assert_eq!(got.blockers, ["c"]);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn model_failure_is_an_error_not_an_empty_draft() {
        // wrong impl: swallows the error and returns StandupDraft::default()
        let got = draft_with(state(), today(), None, || Ok(Box::new(Down))).await;
        assert!(got.is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn regenerate_sends_previous_draft_to_the_model_and_plain_draft_does_not() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let previous = StandupDraft {
            today: vec!["PREVIOUS-MARKER".into()],
            ..Default::default()
        };
        for prev in [Some(previous), None] {
            let spy = Spy(seen.clone());
            draft_with(state(), today(), prev, move || Ok(Box::new(spy)))
                .await
                .unwrap_or_else(|_| panic!("draft should succeed"));
        }
        let inputs = seen.lock().unwrap();
        // wrong impl: ignores `previous` and always calls standup::draft
        assert!(inputs[0].contains("PREVIOUS-MARKER"));
        assert!(!inputs[1].contains("PREVIOUS-MARKER"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn post_sends_the_submitted_text_to_the_thread() {
        let server = MockServer::start();
        let (search, post) = slack_thread(&server);
        let out = post_to(&server, Some("xoxp-t"), Some("daily"), "my edited text")
            .await
            .unwrap_or_else(|_| panic!("post should succeed"));
        search.assert();
        post.assert();
        assert!(matches!(out, PostOutcome::Posted { .. }));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn blank_text_is_a_bad_request_and_calls_slack_never() {
        let server = MockServer::start();
        let (search, post) = slack_thread(&server);
        // wrong impl: posts whitespace, or checks only text.is_empty()
        let out = post_to(&server, Some("xoxp-t"), Some("daily"), " \n ").await;
        assert!(matches!(out, Err(ApiError::BadRequest(_))));
        assert_eq!((search.hits(), post.hits()), (0, 0));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn missing_or_empty_token_is_refused_with_a_cause_and_calls_slack_never() {
        let server = MockServer::start();
        let (search, _post) = slack_thread(&server);
        for token in [None, Some("")] {
            // wrong impl: sends "Bearer " and lets Slack answer, or panics on None
            let out = post_to(&server, token, Some("daily"), "my edited text")
                .await
                .unwrap_or_else(|_| panic!("refusal is an outcome"));
            assert!(
                matches!(out, PostOutcome::SlackRefused { error } if error.contains("not set"))
            );
        }
        assert_eq!(search.hits(), 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn unset_channel_posts_nothing() {
        let server = MockServer::start();
        let (search, post) = slack_thread(&server);
        let out = post_to(&server, Some("xoxp-t"), None, "my edited text")
            .await
            .unwrap_or_else(|_| panic!("no channel is an outcome"));
        assert_eq!(out, PostOutcome::NoChannel);
        assert_eq!((search.hits(), post.hits()), (0, 0));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn missing_thread_and_slack_refusal_pass_through() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/search.messages");
            then.status(200)
                .json_body(json!({"ok": true, "messages": {"matches": []}}));
        });
        let out = post_to(&server, Some("xoxp-t"), Some("daily"), "x")
            .await
            .ok();
        assert_eq!(
            out,
            Some(PostOutcome::NoThread {
                channel: "daily".into()
            })
        );

        let refusing = MockServer::start();
        refusing.mock(|when, then| {
            when.method(GET).path("/search.messages");
            then.status(200)
                .json_body(json!({"ok": false, "error": "missing_scope"}));
        });
        let out = post_to(&refusing, Some("xoxp-t"), Some("daily"), "x")
            .await
            .ok();
        assert_eq!(
            out,
            Some(PostOutcome::SlackRefused {
                error: "missing_scope".into()
            })
        );
    }
}
