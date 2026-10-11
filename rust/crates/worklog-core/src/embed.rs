//! Block text vectors: the /embed client and the lazily-filled
//! `block_embeddings` store. Contract: `topics_contract`.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use regex::Regex;
use reqwest::blocking::Client;
use rusqlite::{params, Connection, OptionalExtension};

use crate::routing_contract::CLASSIFIER_ADDR;
use crate::topics_contract::{
    EmbedRequest, EmbedResponse, EMBED_BATCH, EMBED_DIM, EMBED_MODEL_REVISION, PROMPTS_PER_BLOCK,
    PROMPT_CHARS,
};
use crate::updater::crypto::sha256_hex;

pub(crate) const EMBED_TIMEOUT: Duration = Duration::from_secs(30);

pub struct EmbedClient {
    client: Client,
    base_url: String,
}

impl EmbedClient {
    pub fn from_env() -> Self {
        let client = Client::builder()
            .timeout(EMBED_TIMEOUT)
            .build()
            .unwrap_or_default();
        Self::with_client(client, format!("http://{CLASSIFIER_ADDR}"))
    }

    pub fn with_client(client: Client, base_url: String) -> Self {
        Self { client, base_url }
    }

    /// One unit-length EMBED_DIM vector per text, same order.
    pub fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let resp: EmbedResponse = self
            .client
            .post(format!("{}/embed", self.base_url))
            .json(&EmbedRequest {
                texts: texts.to_vec(),
            })
            .send()
            .context("calling /embed")?
            .error_for_status()
            .context("/embed status")?
            .json()
            .context("decoding /embed reply")?;
        if resp.vectors.len() != texts.len() {
            bail!(
                "/embed returned {} vectors for {} texts",
                resp.vectors.len(),
                texts.len()
            );
        }
        if resp.vectors.iter().any(|v| v.len() != EMBED_DIM) {
            bail!("/embed returned a vector that is not {EMBED_DIM} long");
        }
        Ok(resp.vectors)
    }
}

fn strip_keys(s: &str) -> String {
    static KEY: OnceLock<Regex> = OnceLock::new();
    KEY.get_or_init(|| Regex::new(r"\b[A-Z][A-Z0-9]+-\d+\b").expect("static regex"))
        .replace_all(s, "")
        .into_owned()
}

/// What a block says: its description plus its first prompts, Jira keys removed.
pub fn block_text(conn: &Connection, block_id: i64) -> Result<Option<String>> {
    let Some(description) = conn
        .query_row(
            "SELECT description FROM blocks WHERE id = ?1",
            [block_id],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()?
    else {
        return Ok(None);
    };
    let mut parts = vec![strip_keys(&description.unwrap_or_default())];
    let mut stmt = conn.prepare(
        "SELECT e.details FROM block_events be JOIN events e ON e.id = be.event_id
         WHERE be.block_id = ?1 AND e.source = 'claude'
           AND e.title LIKE 'UserPromptSubmit%' AND length(e.details) > 20
         ORDER BY e.started_at, e.id LIMIT ?2",
    )?;
    let prompts = stmt.query_map(params![block_id, PROMPTS_PER_BLOCK as i64], |r| {
        r.get::<_, String>(0)
    })?;
    for p in prompts {
        parts.push(strip_keys(&p?).chars().take(PROMPT_CHARS).collect());
    }
    let text = parts.join(" ").trim().to_string();
    Ok((!text.is_empty()).then_some(text))
}

fn to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn from_blob(b: &[u8]) -> Option<Vec<f32>> {
    (b.len() == EMBED_DIM * 4).then(|| {
        b.chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    })
}

/// Vectors for every id that has text, embedding only what is missing or stale.
pub fn ensure_vectors(
    conn: &Connection,
    client: &EmbedClient,
    block_ids: &[i64],
) -> Result<HashMap<i64, Vec<f32>>> {
    let mut out = HashMap::new();
    let mut stale: Vec<(i64, String, String)> = Vec::new();
    for &id in block_ids {
        let Some(text) = block_text(conn, id)? else {
            continue;
        };
        let hash = sha256_hex(text.as_bytes());
        let stored: Option<(String, String, Vec<u8>)> = conn
            .query_row(
                "SELECT model, text_hash, vec FROM block_embeddings WHERE block_id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        match stored {
            Some((model, h, blob)) if model == EMBED_MODEL_REVISION && h == hash => {
                match from_blob(&blob) {
                    Some(v) => {
                        out.insert(id, v);
                    }
                    None => stale.push((id, text, hash)),
                }
            }
            _ => stale.push((id, text, hash)),
        }
    }
    for chunk in stale.chunks(EMBED_BATCH) {
        let texts: Vec<String> = chunk.iter().map(|(_, t, _)| t.clone()).collect();
        let vectors = client.embed(&texts)?;
        for ((id, _, hash), v) in chunk.iter().zip(vectors) {
            conn.execute(
                "INSERT INTO block_embeddings (block_id, model, text_hash, vec)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(block_id) DO UPDATE SET
                   model = excluded.model, text_hash = excluded.text_hash, vec = excluded.vec",
                params![id, EMBED_MODEL_REVISION, hash, to_blob(&v)],
            )?;
            out.insert(*id, v);
        }
    }
    Ok(out)
}

#[cfg(test)]
#[path = "embed_test.rs"]
mod tests;
