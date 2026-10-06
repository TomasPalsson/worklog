//! Find today's Daily thread and reply to it (spec 018, FR-21..23).

use crate::daily_helpers_contract::{PostOutcome, DAILY_THREAD_PREFIX};
use anyhow::Result;
use chrono::NaiveDate;
use reqwest::blocking::{Client, RequestBuilder};
use serde_json::{json, Value};

pub(crate) fn parse_ok(body: &str) -> Result<Value, String> {
    let v: Value = serde_json::from_str(body).map_err(|e| format!("bad response: {e}"))?;
    if v["ok"].as_bool() == Some(true) {
        return Ok(v);
    }
    Err(v["error"]
        .as_str()
        .filter(|e| !e.is_empty())
        .unwrap_or("unknown_error")
        .to_string())
}

fn send(req: RequestBuilder, token: &str) -> Result<Result<Value, String>> {
    let resp = req.bearer_auth(token).send()?;
    let status = resp.status();
    if !status.is_success() {
        return Ok(Err(format!("http {}", status.as_u16())));
    }
    Ok(parse_ok(&resp.text()?))
}

pub fn post_to_daily(
    client: &Client,
    base_url: &str,
    token: &str,
    channel: Option<&str>,
    today: NaiveDate,
    text: &str,
) -> Result<PostOutcome> {
    let channel = match channel.map(|c| c.trim().trim_start_matches('#')) {
        Some(c) if !c.is_empty() => c,
        _ => return Ok(PostOutcome::NoChannel),
    };
    let refused = |error| Ok(PostOutcome::SlackRefused { error });
    let query = format!("in:#{channel} on:{today} \"{DAILY_THREAD_PREFIX}\"");
    let found = send(
        client
            .get(format!("{base_url}/search.messages"))
            .query(&[("query", query.as_str())]),
        token,
    )?;
    let found = match found {
        Ok(v) => v,
        Err(error) => return refused(error),
    };
    let root = found["messages"]["matches"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|m| {
            m["text"]
                .as_str()
                .is_some_and(|t| t.starts_with(DAILY_THREAD_PREFIX))
                && m["thread_ts"]
                    .as_str()
                    .is_none_or(|t| Some(t) == m["ts"].as_str())
        });
    let Some(root) = root else {
        return Ok(PostOutcome::NoThread {
            channel: channel.to_string(),
        });
    };
    let (channel_id, root_ts) = (
        root["channel"]["id"].as_str().unwrap_or_default(),
        root["ts"].as_str().unwrap_or_default(),
    );
    let posted = send(
        client
            .post(format!("{base_url}/chat.postMessage"))
            .json(&json!({"channel": channel_id, "thread_ts": root_ts, "text": text})),
        token,
    )?;
    let posted = match posted {
        Ok(v) => v,
        Err(error) => return refused(error),
    };
    // The reply is already live; a failed lookup falls back to the thread's own link.
    let reply_link = send(
        client.get(format!("{base_url}/chat.getPermalink")).query(&[
            ("channel", channel_id),
            ("message_ts", posted["ts"].as_str().unwrap_or_default()),
        ]),
        token,
    )
    .ok()
    .and_then(|r| r.ok())
    .and_then(|v| v["permalink"].as_str().map(str::to_string));
    let permalink = reply_link
        .or_else(|| root["permalink"].as_str().map(str::to_string))
        .unwrap_or_default();
    Ok(PostOutcome::Posted { permalink })
}

#[cfg(test)]
#[path = "slack_post_test.rs"]
mod tests;
