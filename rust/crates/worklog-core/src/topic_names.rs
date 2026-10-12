//! AI topic names: cache in `topic_names`, keyword fallback, background naming job.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::topics_contract::{NamedBy, TopicsReport, NAME_REUSE_JACCARD};
use crate::updater::crypto::sha256_hex;

const SCAN_ROWS: i64 = 200;
const FALLBACK_KEYWORDS: usize = 3;
const NAME_MAX_CHARS: usize = 40;
const DESCRIPTION_MAX_CHARS: usize = 200;
/// Topics per model call: a bigger prompt outruns the claude -p timeout.
pub const BATCH: usize = 4;

/// Lets route-level tests answer the model call without a provider.
#[cfg(test)]
pub(crate) static TEST_REPLY: Mutex<Option<Value>> = Mutex::new(None);

static IN_FLIGHT: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

static FAILED: LazyLock<Mutex<HashMap<String, Instant>>> = LazyLock::new(Default::default);

/// One topic as the namer sees it; `members` are its block ids.
#[derive(Debug, Clone)]
pub struct TopicInput {
    pub id: usize,
    pub members: Vec<i64>,
    pub keywords: Vec<String>,
    pub examples: Vec<String>,
    pub blocks: i64,
    pub seconds: i64,
}

/// Topics `resolve` could not name from the cache.
pub struct NamesOutcome {
    pub pending: Vec<TopicInput>,
}

fn sorted(members: &[i64]) -> Vec<i64> {
    let mut m = members.to_vec();
    m.sort_unstable();
    m
}

pub(crate) fn join_ids(members: &[i64]) -> String {
    sorted(members)
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

pub fn member_hash(members: &[i64]) -> String {
    sha256_hex(join_ids(members).as_bytes())
}

fn jaccard(a: &[i64], b: &[i64]) -> f64 {
    let a: HashSet<i64> = a.iter().copied().collect();
    let b: HashSet<i64> = b.iter().copied().collect();
    a.intersection(&b).count() as f64 / a.union(&b).count().max(1) as f64
}

fn parse_ids(joined: &str) -> Vec<i64> {
    joined.split(',').filter_map(|s| s.parse().ok()).collect()
}

fn keyword_name(input: &TopicInput) -> String {
    if input.keywords.is_empty() {
        return format!("Topic {}", input.id + 1);
    }
    input
        .keywords
        .iter()
        .take(FALLBACK_KEYWORDS)
        .cloned()
        .collect::<Vec<_>>()
        .join(" · ")
}

struct CachedName {
    members: Vec<i64>,
    name: String,
    description: String,
    model: String,
}

fn newest_rows(conn: &Connection) -> Result<Vec<CachedName>> {
    conn.prepare(
        "SELECT members, name, description, model FROM topic_names
          ORDER BY created_at DESC, rowid DESC LIMIT ?1",
    )?
    .query_map([SCAN_ROWS], |r| {
        Ok(CachedName {
            members: parse_ids(&r.get::<_, String>(0)?),
            name: r.get(1)?,
            description: r.get(2)?,
            model: r.get(3)?,
        })
    })?
    .collect::<rusqlite::Result<_>>()
    .map_err(Into::into)
}

fn insert_row(
    conn: &Connection,
    input: &TopicInput,
    name: &str,
    description: &str,
    model: &str,
) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO topic_names
             (member_hash, members, name, description, model, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            member_hash(&input.members),
            join_ids(&input.members),
            name,
            description,
            model,
            Utc::now().to_rfc3339()
        ],
    )?;
    Ok(())
}

/// Names each topic from the cache (exact hash, else best Jaccard overlap) or
/// with keywords; the keyword-named ones come back in `pending`.
pub fn resolve(
    conn: &Connection,
    report: &mut TopicsReport,
    inputs: &[TopicInput],
) -> Result<NamesOutcome> {
    let recent = newest_rows(conn)?;
    let mut pending = Vec::new();
    for input in inputs {
        let exact: Option<(String, String)> = conn
            .query_row(
                "SELECT name, description FROM topic_names WHERE member_hash = ?1",
                [member_hash(&input.members)],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let named = match exact {
            Some(hit) => Some(hit),
            None => {
                let best = recent
                    .iter()
                    .map(|r| (jaccard(&input.members, &r.members), r))
                    .filter(|(j, _)| *j >= NAME_REUSE_JACCARD)
                    // Greater on a tie keeps the earlier (newest) row.
                    .max_by(|a, b| a.0.total_cmp(&b.0).then(Ordering::Greater));
                if let Some((_, r)) = best {
                    insert_row(conn, input, &r.name, &r.description, &r.model)?;
                }
                best.map(|(_, r)| (r.name.clone(), r.description.clone()))
            }
        };
        let topic = &mut report.topics[input.id];
        match named {
            Some((name, description)) => {
                topic.name = name;
                topic.description = Some(description);
                topic.named_by = NamedBy::Ai;
            }
            None => {
                topic.name = keyword_name(input);
                topic.description = None;
                topic.named_by = NamedBy::Keywords;
                pending.push(input.clone());
            }
        }
    }
    Ok(NamesOutcome { pending })
}

pub fn store(
    conn: &Connection,
    inputs: &[TopicInput],
    names: &[(usize, String, String)],
    model: &str,
) -> Result<()> {
    for (id, name, description) in names {
        if let Some(input) = inputs.iter().find(|i| i.id == *id) {
            insert_row(conn, input, name, description, model)?;
        }
    }
    Ok(())
}

pub const SYSTEM_PROMPT: &str = "You are naming themes in one software consultant's work log. Each theme is a cluster of work blocks grouped by meaning.
For every theme give:
- \"name\": 2-4 words, Title Case, specific (name the product, customer or system when the evidence names one), not generic (\"Development Work\", \"Infrastructure\" alone are bad). Name what the whole theme has in common, not just its first item.
- \"description\": one plain sentence, max 18 words, saying what the work in this theme is. Use only facts in the evidence; do not guess what a product or customer is.
Each name must be at most 40 characters. Names must be distinct from each other. Write in English unless every work item in that theme is Icelandic. Keep Icelandic proper nouns (customer and product names) as written.
If the items share nothing, name it 'Odds and Ends'.";

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {"topics": {"type": "array", "items": {
            "type": "object",
            "properties": {
                "id": {"type": "integer"},
                "name": {"type": "string", "maxLength": 40},
                "description": {"type": "string", "maxLength": 200}
            },
            "required": ["id", "name", "description"]
        }}},
        "required": ["topics"]
    })
}

pub fn prompt(inputs: &[TopicInput]) -> String {
    let mut out = String::new();
    for t in inputs {
        out.push_str(&format!(
            "Theme {} ({} blocks, {:.1}h)\n  distinctive terms: {}\n  representative work items:\n",
            t.id,
            t.blocks,
            t.seconds as f64 / 3600.0,
            t.keywords.join(", ")
        ));
        for e in &t.examples {
            out.push_str(&format!("  - {e}\n"));
        }
        out.push('\n');
    }
    out.push_str(
        "Reply with ONLY a JSON array, no prose, no code fence: [{\"id\": <theme id>, \"name\": \"...\", \"description\": \"...\"}, ...]",
    );
    out
}

fn strip_fence(s: &str) -> &str {
    let s = s.trim();
    let Some(rest) = s.strip_prefix("```") else {
        return s;
    };
    let rest = rest.strip_prefix("json").unwrap_or(rest);
    rest.strip_suffix("```").unwrap_or(rest).trim()
}

/// At most `DESCRIPTION_MAX_CHARS` chars, cut at a word boundary with "…".
fn clip_description(d: &str) -> String {
    if d.chars().count() <= DESCRIPTION_MAX_CHARS {
        return d.to_string();
    }
    let head: String = d.chars().take(DESCRIPTION_MAX_CHARS - 1).collect();
    let whole_word = d
        .chars()
        .nth(DESCRIPTION_MAX_CHARS - 1)
        .is_some_and(char::is_whitespace);
    let cut = if whole_word {
        &head
    } else {
        head.rfind(char::is_whitespace)
            .map_or(head.as_str(), |i| &head[..i])
    };
    format!("{}…", cut.trim_end())
}

/// `(id, name, description)` for each requested id the reply names with a
/// name of at most `NAME_MAX_CHARS`; errors when none.
pub fn parse_reply(reply: &Value, wanted: &[usize]) -> Result<Vec<(usize, String, String)>> {
    let parsed;
    let reply = match reply {
        Value::String(s) => {
            parsed = serde_json::from_str::<Value>(strip_fence(s))
                .map_err(|e| anyhow!("naming reply is not JSON: {e}"))?;
            &parsed
        }
        other => other,
    };
    let items = reply
        .get("topics")
        .unwrap_or(reply)
        .as_array()
        .ok_or_else(|| anyhow!("naming reply has no topic list"))?;
    let named: Vec<(usize, String, String)> = items
        .iter()
        .filter_map(|v| {
            let id = usize::try_from(v.get("id")?.as_u64()?).ok()?;
            let name = v.get("name")?.as_str()?.trim();
            let description = v.get("description")?.as_str()?.trim();
            (wanted.contains(&id) && !name.is_empty() && name.chars().count() <= NAME_MAX_CHARS)
                .then(|| (id, name.to_string(), clip_description(description)))
        })
        .collect();
    if named.is_empty() {
        return Err(anyhow!("naming reply named none of the requested topics"));
    }
    Ok(named)
}

/// Marks the topics no other job is naming; returns just those.
pub fn claim(inputs: &[TopicInput]) -> Vec<TopicInput> {
    let mut set = IN_FLIGHT.lock().unwrap();
    inputs
        .iter()
        .filter(|i| set.insert(member_hash(&i.members)))
        .cloned()
        .collect()
}

pub fn release(inputs: &[TopicInput]) {
    let mut set = IN_FLIGHT.lock().unwrap();
    for i in inputs {
        set.remove(&member_hash(&i.members));
    }
}

pub fn record_failure(inputs: &[TopicInput]) {
    let mut failed = FAILED.lock().unwrap();
    for i in inputs {
        failed.insert(member_hash(&i.members), Instant::now());
    }
}

/// True when naming this topic failed less than `window` ago.
pub fn backed_off(input: &TopicInput, window: Duration) -> bool {
    FAILED
        .lock()
        .unwrap()
        .get(&member_hash(&input.members))
        .is_some_and(|at| at.elapsed() < window)
}

#[cfg(test)]
#[path = "topic_names_test.rs"]
mod tests;
