//! Display-only topic stats over block vectors (GET /stats/topics).
//! Contract: `topics_contract::TopicsReport`. Writes nothing.

use std::collections::{BTreeMap, HashMap};

use chrono::DateTime;

use crate::stats::{bump, ranked};
use crate::topics_contract::{
    DaySwitches, SimilarDay, TicketSprawl, Topic, TopicDrift, TopicsReport, TopicsStatus,
    MAX_TOPICS, MIN_BLOCKS, MIN_TOPICS, SIMILAR_DAYS_TOP, SPRAWL_MIN_BLOCKS, SPRAWL_TOP,
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
    pub before: Vec<BlockPoint>,
    pub history: Vec<BlockPoint>,
    pub ticket_summaries: HashMap<String, String>,
}

fn cosine<A: Copy + Into<f64>, B: Copy + Into<f64>>(a: &[A], b: &[B]) -> f64 {
    let (mut dot, mut na, mut nb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        let (x, y): (f64, f64) = ((*x).into(), (*y).into());
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na.sqrt() * nb.sqrt())
    }
}

fn add_weighted(acc: &mut Vec<f64>, v: &[f32], weight: f64) {
    if acc.is_empty() {
        acc.resize(v.len(), 0.0);
    }
    for (a, x) in acc.iter_mut().zip(v) {
        *a += weight * f64::from(*x);
    }
}

fn started_ts(p: &BlockPoint) -> i64 {
    DateTime::parse_from_rfc3339(&p.started_at)
        .map(|t| t.timestamp())
        .unwrap_or(0)
}

/// Average-linkage agglomerative clustering on cosine distance. `pts` must
/// be sorted by id: clusters then stay ordered by lowest member, so exact
/// ties merge the lower-id pair first.
fn cluster(pts: &[&BlockPoint], k: usize) -> Vec<Vec<usize>> {
    let n = pts.len();
    let mut dist = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in i + 1..n {
            dist[i][j] = 1.0 - cosine(&pts[i].vec, &pts[j].vec);
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
    cl
}

fn topic_name(
    members: &[&BlockPoint],
    tickets: HashMap<String, i64>,
    summaries: &HashMap<String, String>,
) -> (String, Option<String>) {
    if let Some(top) = ranked(tickets, 1).first() {
        if let Some(s) = summaries.get(&top.label) {
            return (s.clone(), Some(top.label.clone()));
        }
    }
    let longest = members
        .iter()
        .min_by_key(|p| (-p.seconds, p.id))
        .and_then(|p| p.description.clone());
    (longest.unwrap_or_else(|| "Untitled".into()), None)
}

fn day_switches(day: &str, pts: &[(&BlockPoint, usize)]) -> DaySwitches {
    let mut sorted = pts.to_vec();
    sorted.sort_by(|a, b| (&a.0.started_at, a.0.id).cmp(&(&b.0.started_at, b.0.id)));
    let (mut switches, mut longest) = (0, 0);
    let mut run: Option<(usize, i64, i64)> = None; // topic, start, end
    for (p, topic) in &sorted {
        let (start, end) = (started_ts(p), started_ts(p) + p.seconds);
        run = match run {
            Some((t, s, e)) if t == *topic => Some((t, s, e.max(end))),
            prev => {
                if prev.is_some() {
                    switches += 1;
                }
                Some((*topic, start, end))
            }
        };
        if let Some((_, s, e)) = run {
            longest = longest.max(e - s);
        }
    }
    DaySwitches {
        day: day.to_string(),
        switches,
        work_seconds: sorted.iter().map(|(p, _)| p.seconds).sum(),
        longest_run_seconds: longest,
    }
}

fn sprawl(now: &[BlockPoint], summaries: &HashMap<String, String>) -> Vec<TicketSprawl> {
    let mut by_ticket: BTreeMap<&str, Vec<&BlockPoint>> = BTreeMap::new();
    for p in now {
        if let Some(t) = &p.ticket {
            by_ticket.entry(t).or_default().push(p);
        }
    }
    let mut out: Vec<TicketSprawl> = by_ticket
        .into_iter()
        .filter(|(_, v)| v.len() >= SPRAWL_MIN_BLOCKS)
        .map(|(t, v)| {
            let mut sum = 0.0;
            let mut pairs = 0;
            for i in 0..v.len() {
                for j in i + 1..v.len() {
                    sum += cosine(&v[i].vec, &v[j].vec);
                    pairs += 1;
                }
            }
            TicketSprawl {
                ticket: t.to_string(),
                summary: summaries.get(t).cloned(),
                blocks: v.len() as i64,
                spread: 1.0 - sum / pairs as f64,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.spread
            .total_cmp(&a.spread)
            .then_with(|| a.ticket.cmp(&b.ticket))
    });
    out.truncate(SPRAWL_TOP);
    out
}

#[derive(Default)]
struct DayAcc {
    centroid: Vec<f64>,
    seconds: i64,
    tickets: HashMap<String, i64>,
}

fn similar_days(input: &TopicsInput) -> Vec<SimilarDay> {
    let mut days: BTreeMap<&str, DayAcc> = BTreeMap::new();
    for p in input.history.iter().chain(&input.now) {
        let d = days.entry(&p.day).or_default();
        add_weighted(&mut d.centroid, &p.vec, p.seconds as f64);
        d.seconds += p.seconds;
        if let Some(t) = &p.ticket {
            bump(&mut d.tickets, t.as_str(), p.seconds);
        }
    }
    let target = if days.contains_key(input.to.as_str()) {
        input.to.as_str()
    } else {
        match input.now.iter().map(|p| p.day.as_str()).max() {
            Some(d) => d,
            None => return vec![],
        }
    };
    let target_centroid = days[target].centroid.clone();
    let mut out: Vec<SimilarDay> = days
        .into_iter()
        .filter(|(day, _)| *day < target)
        .map(|(day, acc)| SimilarDay {
            day: day.to_string(),
            similarity: cosine(&acc.centroid, &target_centroid),
            work_seconds: acc.seconds,
            top_tickets: ranked(acc.tickets, 3),
        })
        .collect();
    out.sort_by(|a, b| {
        b.similarity
            .total_cmp(&a.similarity)
            .then_with(|| a.day.cmp(&b.day))
    });
    out.truncate(SIMILAR_DAYS_TOP);
    out
}

fn drift(before: &[BlockPoint], topics: &[Topic], centroids: &[Vec<f64>]) -> Vec<TopicDrift> {
    let mut secs = vec![0; topics.len()];
    for p in before {
        let mut best = (0, f64::NEG_INFINITY);
        for (id, c) in centroids.iter().enumerate() {
            let sim = cosine(&p.vec, c);
            if sim > best.1 {
                best = (id, sim);
            }
        }
        secs[best.0] += p.seconds;
    }
    topics
        .iter()
        .map(|t| TopicDrift {
            topic_id: t.id,
            name: t.name.clone(),
            seconds_now: t.seconds,
            seconds_before: secs[t.id],
        })
        .collect()
}

/// Topics ordered by seconds, each cluster's centroid, and every point's topic id.
fn topics_of(
    pts: &[&BlockPoint],
    k: usize,
    summaries: &HashMap<String, String>,
) -> (Vec<Topic>, Vec<Vec<f64>>, Vec<usize>) {
    let mut clusters = cluster(pts, k);
    let seconds_of = |c: &Vec<usize>| c.iter().map(|&i| pts[i].seconds).sum::<i64>();
    // Stable sort: equal seconds keep lowest-member-first order.
    clusters.sort_by_key(|c| -seconds_of(c));
    let mut topic_of = vec![0; pts.len()];
    let (mut centroids, mut topics) = (vec![], vec![]);
    for (id, c) in clusters.iter().enumerate() {
        let members: Vec<&BlockPoint> = c.iter().map(|&i| pts[i]).collect();
        let mut centroid = vec![];
        let mut tickets = HashMap::new();
        for (&i, p) in c.iter().zip(&members) {
            topic_of[i] = id;
            add_weighted(&mut centroid, &p.vec, 1.0);
            if let Some(t) = &p.ticket {
                bump(&mut tickets, t.as_str(), p.seconds);
            }
        }
        let top_tickets = ranked(tickets.clone(), 3);
        let (name, name_ticket) = topic_name(&members, tickets, summaries);
        centroids.push(centroid);
        topics.push(Topic {
            id,
            name,
            name_ticket,
            seconds: seconds_of(c),
            blocks: c.len() as i64,
            top_tickets,
        });
    }
    (topics, centroids, topic_of)
}

pub fn build(input: &TopicsInput) -> TopicsReport {
    let n = input.now.len();
    if n < MIN_BLOCKS {
        return TopicsReport::empty(TopicsStatus::TooFewBlocks, &input.from, &input.to);
    }
    let mut pts: Vec<&BlockPoint> = input.now.iter().collect();
    pts.sort_by_key(|p| p.id);
    let k = ((n as f64 / 2.0).sqrt().round() as usize).clamp(MIN_TOPICS, MAX_TOPICS);
    let (topics, centroids, topic_of) = topics_of(&pts, k, &input.ticket_summaries);

    let mut by_day: BTreeMap<&str, Vec<(&BlockPoint, usize)>> = BTreeMap::new();
    for (i, p) in pts.iter().enumerate() {
        by_day.entry(&p.day).or_default().push((p, topic_of[i]));
    }
    let switches = by_day.iter().map(|(d, v)| day_switches(d, v)).collect();

    TopicsReport {
        status: TopicsStatus::Ready,
        from: input.from.clone(),
        to: input.to.clone(),
        drift: drift(&input.before, &topics, &centroids),
        topics,
        switches,
        sprawl: sprawl(&input.now, &input.ticket_summaries),
        similar_days: similar_days(input),
        embedded_blocks: n as i64,
        skipped_blocks: 0,
    }
}

#[cfg(test)]
#[path = "stats_topics_test.rs"]
mod tests;
