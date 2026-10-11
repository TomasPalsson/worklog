//! Display-only topic stats over block vectors (GET /stats/topics).
//! Contract: `topics_contract::TopicsReport`. Writes nothing.

use std::collections::{BTreeSet, HashMap};

use crate::stats::{bump, ranked};
use crate::topic_metrics::{
    center_with, cosine, day_rows, day_similarity, mean, median, nearest_cos, novelty, river,
    warmup,
};
use crate::topics_contract::{
    Highlight, NamedBy, Naming, Topic, TopicsReport, TopicsStatus, HIGHLIGHTS_TOP, MAX_TOPICS,
    MIN_BLOCKS, MIN_TOPICS, ODDBALL_MAX_COS,
};

#[derive(Debug, Clone)]
pub struct BlockPoint {
    pub id: i64,
    pub day: String,
    pub started_at: String,
    pub seconds: i64,
    pub ticket: Option<String>,
    pub description: Option<String>,
    pub vec: Vec<f32>,
}

#[derive(Debug, Clone)]
pub struct TopicsInput {
    pub from: String,
    pub to: String,
    pub now: Vec<BlockPoint>,
    pub history: Vec<BlockPoint>,
    pub ticket_summaries: HashMap<String, String>,
}

/// Average-linkage agglomerative clustering on cosine distance; returns each
/// vector's cluster label. Labels are numbered by lowest member, and callers pass
/// vectors sorted by id, so exact ties merge the lower-id pair first.
pub fn cluster(vectors: &[Vec<f32>], k: usize) -> Vec<usize> {
    let n = vectors.len();
    let mut dist = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in i + 1..n {
            dist[i][j] = 1.0 - cosine(&vectors[i], &vectors[j]);
            dist[j][i] = dist[i][j];
        }
    }
    let mut cl: Vec<Vec<usize>> = (0..n).map(|i| vec![i]).collect();
    // `slot[i]` is the row of `dist` holding cluster i; rows are updated in
    // place with the average-linkage (UPGMA) recurrence, so a step is O(m^2).
    let mut slot: Vec<usize> = (0..n).collect();
    while cl.len() > k {
        let (mut bi, mut bj, mut best) = (0, 1, f64::INFINITY);
        for i in 0..cl.len() {
            for j in i + 1..cl.len() {
                let avg = dist[slot[i]][slot[j]];
                // 1e-12: float noise must not break a tie toward the later pair.
                if avg < best - 1e-12 {
                    (bi, bj, best) = (i, j, avg);
                }
            }
        }
        let (a, b) = (slot[bi], slot[bj]);
        let (na, nb) = (cl[bi].len() as f64, cl[bj].len() as f64);
        for &c in &slot {
            let d = (na * dist[a][c] + nb * dist[b][c]) / (na + nb);
            dist[a][c] = d;
            dist[c][a] = d;
        }
        slot.remove(bj);
        let merged = cl.remove(bj);
        cl[bi].extend(merged);
    }
    let mut labels = vec![0; n];
    for (label, members) in cl.iter().enumerate() {
        for &i in members {
            labels[i] = label;
        }
    }
    labels
}

pub(crate) fn is_oddball(nearest_cos: f64) -> bool {
    nearest_cos < ODDBALL_MAX_COS
}

/// Topics ordered by seconds (ties: lowest member), plus every point's topic id.
fn topics_of(pts: &[&BlockPoint], labels: &[usize]) -> (Vec<Topic>, Vec<usize>) {
    let count = labels.iter().max().map_or(0, |m| m + 1);
    let mut members: Vec<Vec<usize>> = vec![vec![]; count];
    for (i, &l) in labels.iter().enumerate() {
        members[l].push(i);
    }
    let seconds_of = |m: &Vec<usize>| m.iter().map(|&i| pts[i].seconds).sum::<i64>();
    // Stable sort: equal seconds keep lowest-member-first order.
    members.sort_by_key(|m| -seconds_of(m));
    let mut topic_of = vec![0; pts.len()];
    let topics = members
        .iter()
        .enumerate()
        .map(|(id, m)| {
            let mut tickets = HashMap::new();
            let mut days = BTreeSet::new();
            for &i in m {
                topic_of[i] = id;
                days.insert(pts[i].day.as_str());
                if let Some(t) = &pts[i].ticket {
                    bump(&mut tickets, t.as_str(), pts[i].seconds);
                }
            }
            Topic {
                id,
                name: String::new(),
                description: None,
                named_by: NamedBy::Keywords,
                keywords: vec![],
                seconds: seconds_of(m),
                blocks: m.len() as i64,
                days_active: days.len() as i64,
                first_day: days.first().map(|d| d.to_string()).unwrap_or_default(),
                last_day: days.last().map(|d| d.to_string()).unwrap_or_default(),
                top_tickets: ranked(tickets, 3),
            }
        })
        .collect();
    (topics, topic_of)
}

/// Up to HIGHLIGHTS_TOP of `candidates`, best `order` first (ties: lowest block id).
fn highlights(
    candidates: Vec<usize>,
    pts: &[&BlockPoint],
    topic_of: &[usize],
    score: &[f64],
    higher_first: bool,
) -> Vec<Highlight> {
    let mut sorted = candidates;
    sorted.sort_by(|&a, &b| {
        let by_score = score[a].total_cmp(&score[b]);
        let by_score = if higher_first {
            by_score.reverse()
        } else {
            by_score
        };
        by_score.then_with(|| pts[a].id.cmp(&pts[b].id))
    });
    sorted
        .into_iter()
        .take(HIGHLIGHTS_TOP)
        .map(|i| Highlight {
            block_id: pts[i].id,
            day: pts[i].day.clone(),
            label: pts[i]
                .description
                .as_deref()
                .unwrap_or("")
                .chars()
                .take(90)
                .collect(),
            ticket: pts[i].ticket.clone(),
            topic_id: topic_of[i],
            seconds: pts[i].seconds,
            score: score[i],
        })
        .collect()
}

/// Per-block results of one `build`, parallel to `block_ids` (ascending id).
#[derive(Debug, Default)]
pub struct Assignments {
    pub block_ids: Vec<i64>,
    pub topic_of: Vec<usize>,
    pub centered: Vec<Vec<f32>>,
    pub novelty: Vec<f64>,
    pub oddball: Vec<bool>,
}

pub fn build(input: &TopicsInput) -> TopicsReport {
    build_with_assignments(input).0
}

pub fn build_with_assignments(input: &TopicsInput) -> (TopicsReport, Assignments) {
    let n = input.now.len();
    if n < MIN_BLOCKS {
        return (
            TopicsReport::empty(TopicsStatus::TooFewBlocks, &input.from, &input.to),
            Assignments::default(),
        );
    }
    let mut pts: Vec<&BlockPoint> = input.now.iter().collect();
    pts.sort_by_key(|p| p.id);
    let raw: Vec<Vec<f32>> = pts.iter().map(|p| p.vec.clone()).collect();
    let now_mean = mean(&raw);
    let centered = center_with(&raw, &now_mean);
    let history_raw: Vec<Vec<f32>> = input.history.iter().map(|p| p.vec.clone()).collect();
    let history_centered = center_with(&history_raw, &now_mean);

    let k = ((n as f64 / 2.0).sqrt().round() as usize).clamp(MIN_TOPICS, MAX_TOPICS);
    let (topics, topic_of) = topics_of(&pts, &cluster(&centered, k));

    let now_points: Vec<BlockPoint> = pts.iter().map(|p| (*p).clone()).collect();
    let novelty = novelty(&now_points, &centered, &input.history, &history_centered);
    let warm = warmup(&now_points, &input.history);
    let nearest = nearest_cos(&centered);
    let oddball: Vec<bool> = nearest.iter().map(|&c| is_oddball(c)).collect();
    let oddball_ids = (0..n).filter(|&i| oddball[i]).collect();
    let days = day_rows(&pts, &centered, &novelty, &warm, &topic_of);
    let (bin, river) = river(&pts, &topic_of, topics.len(), &input.from, &input.to);

    let report = TopicsReport {
        status: TopicsStatus::Ready,
        from: input.from.clone(),
        to: input.to.clone(),
        naming: Naming::Off,
        day_similarity: day_similarity(&pts, &centered, &days),
        first_times: highlights(
            (0..n).filter(|&i| !warm[i]).collect(),
            &pts,
            &topic_of,
            &novelty,
            true,
        ),
        oddballs: highlights(oddball_ids, &pts, &topic_of, &nearest, false),
        oddball_median: median(&nearest),
        topics,
        points: vec![],
        bin,
        river,
        days,
        embedded_blocks: n as i64,
        skipped_blocks: 0,
    };
    let assignments = Assignments {
        block_ids: pts.iter().map(|p| p.id).collect(),
        topic_of,
        centered,
        novelty: novelty
            .iter()
            .zip(&warm)
            .map(|(&v, &w)| if w { 0.0 } else { v })
            .collect(),
        oddball,
    };
    (report, assignments)
}

#[cfg(test)]
#[path = "stats_topics_test.rs"]
mod tests;
