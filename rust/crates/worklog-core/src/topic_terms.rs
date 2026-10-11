//! c-TF-IDF keywords and MMR representative examples per topic.

use std::collections::{BTreeMap, HashSet};
use std::sync::OnceLock;

use regex::Regex;

use crate::topic_metrics::{cosine, mean};

const MMR_LAMBDA: f64 = 0.5;

const STOPWORDS: &[&str] = &[
    "a", "an", "the", "to", "of", "in", "on", "for", "and", "or", "with", "by", "at", "is", "are",
    "was", "be", "from", "as", "it", "this", "that", "i", "my", "me", "we", "our", "og", "í", "á",
    "að", "við", "fyrir", "með", "um", "sem", "er", "var", "til", "af", "work", "worked",
    "working", "task", "tasks", "update", "updated", "fix", "fixed", "add", "added", "review",
    "check", "make", "made", "using", "use", "new", "into", "over", "also", "och",
];

pub fn tokenize(text: &str) -> Vec<String> {
    static JIRA: OnceLock<Regex> = OnceLock::new();
    let jira = JIRA.get_or_init(|| Regex::new(r"\b[A-Z][A-Z0-9]+-\d+\b").expect("static regex"));
    jira.replace_all(text, " ")
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| {
            t.chars().count() >= 3
                && !t.chars().all(|c| c.is_ascii_digit())
                && !STOPWORDS.contains(t)
        })
        .map(str::to_string)
        .collect()
}

/// c-TF-IDF: `tf(t,c)/len(c) · ln(1 + A / f(t))`, best first, ties by term.
pub fn keywords(docs_per_topic: &[Vec<String>], top: usize) -> Vec<Vec<String>> {
    let total: usize = docs_per_topic.iter().map(Vec::len).sum();
    let avg = total as f64 / docs_per_topic.len().max(1) as f64;
    let mut freq: BTreeMap<&str, usize> = BTreeMap::new();
    for t in docs_per_topic.iter().flatten() {
        *freq.entry(t).or_default() += 1;
    }
    docs_per_topic
        .iter()
        .map(|doc| {
            let mut tf: BTreeMap<&str, usize> = BTreeMap::new();
            for t in doc {
                *tf.entry(t).or_default() += 1;
            }
            let mut scored: Vec<(&str, f64)> = tf
                .into_iter()
                .map(|(t, n)| {
                    let score = n as f64 / doc.len() as f64 * (1.0 + avg / freq[t] as f64).ln();
                    (t, score)
                })
                .collect();
            // BTreeMap order + stable sort keeps term-asc on equal scores.
            scored.sort_by(|a, b| b.1.total_cmp(&a.1));
            scored
                .into_iter()
                .take(top)
                .map(|(t, _)| t.to_string())
                .collect()
        })
        .collect()
}

/// MMR over cosine to the topic centroid; equal descriptions are never both picked.
pub fn representatives(vecs: &[Vec<f32>], descriptions: &[String], n: usize) -> Vec<usize> {
    let centroid = mean(vecs);
    let rel: Vec<f64> = vecs.iter().map(|v| cosine(v, &centroid)).collect();
    let key = |i: usize| descriptions.get(i).map(|d| d.trim().to_lowercase());
    let mut picked: Vec<usize> = Vec::new();
    let mut seen: HashSet<Option<String>> = HashSet::new();
    while picked.len() < n {
        let mut best: Option<(usize, f64)> = None;
        for i in (0..vecs.len()).filter(|i| !picked.contains(i) && !seen.contains(&key(*i))) {
            let redundancy = picked
                .iter()
                .map(|&p| cosine(&vecs[i], &vecs[p]))
                .max_by(f64::total_cmp)
                .unwrap_or(0.0);
            let score = rel[i] - MMR_LAMBDA * redundancy;
            if best.is_none_or(|(_, b)| score > b) {
                best = Some((i, score));
            }
        }
        let Some((i, _)) = best else { break };
        seen.insert(key(i));
        picked.push(i);
    }
    picked
}

#[cfg(test)]
#[path = "topic_terms_test.rs"]
mod tests;
