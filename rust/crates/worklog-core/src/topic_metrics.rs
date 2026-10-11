//! Centering, per-block novelty / oddball and per-day metrics for the Topic atlas.
//! Contract: `topics_contract`.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, Duration, NaiveDate};

use crate::stats_topics::BlockPoint;
use crate::topics_contract::{Bin, DayRow, RiverBin, MIRROR_MAX_DAYS, RIVER_WEEK_AFTER_DAYS};

const MIN_NORM: f64 = 1e-9;

pub(crate) fn cosine<A: Copy + Into<f64>, B: Copy + Into<f64>>(a: &[A], b: &[B]) -> f64 {
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

pub(crate) fn started_ts(p: &BlockPoint) -> i64 {
    DateTime::parse_from_rfc3339(&p.started_at)
        .map(|t| t.timestamp())
        .unwrap_or(0)
}

/// Column-wise mean of `v`.
pub fn mean(v: &[Vec<f32>]) -> Vec<f32> {
    let mut acc = vec![0.0f64; v.first().map_or(0, Vec::len)];
    for row in v {
        for (a, x) in acc.iter_mut().zip(row) {
            *a += f64::from(*x);
        }
    }
    acc.iter().map(|a| (a / v.len() as f64) as f32).collect()
}

/// `(v - mean) / ||v - mean||` per row; a row that is (almost) the mean is kept as is.
pub fn center_with(v: &[Vec<f32>], mean: &[f32]) -> Vec<Vec<f32>> {
    v.iter()
        .map(|row| {
            let shifted: Vec<f32> = row.iter().zip(mean).map(|(x, m)| x - m).collect();
            let norm = shifted
                .iter()
                .map(|x| f64::from(*x).powi(2))
                .sum::<f64>()
                .sqrt();
            if norm < MIN_NORM {
                row.clone()
            } else {
                shifted
                    .iter()
                    .map(|x| (f64::from(*x) / norm) as f32)
                    .collect()
            }
        })
        .collect()
}

pub fn center(v: &[Vec<f32>]) -> Vec<Vec<f32>> {
    center_with(v, &mean(v))
}

/// 1 - max cosine to any block (history or range) that started strictly earlier;
/// 1.0 for the first block ever. Vectors must already share one centering.
pub fn novelty(
    now: &[BlockPoint],
    now_centered: &[Vec<f32>],
    history: &[BlockPoint],
    history_centered: &[Vec<f32>],
) -> Vec<f64> {
    let others: Vec<(i64, &Vec<f32>)> = history
        .iter()
        .zip(history_centered)
        .chain(now.iter().zip(now_centered))
        .map(|(p, v)| (started_ts(p), v))
        .collect();
    now.iter()
        .zip(now_centered)
        .map(|(p, v)| {
            let at = started_ts(p);
            let best = others
                .iter()
                .filter(|(t, _)| *t < at)
                .map(|(_, other)| cosine(v, other))
                .fold(f64::NEG_INFINITY, f64::max);
            if best == f64::NEG_INFINITY {
                1.0
            } else {
                1.0 - best
            }
        })
        .collect()
}

/// Per block, the highest cosine to any other block in `centered`.
pub fn nearest_cos(centered: &[Vec<f32>]) -> Vec<f64> {
    (0..centered.len())
        .map(|i| {
            (0..centered.len())
                .filter(|&j| j != i)
                .map(|j| cosine(&centered[i], &centered[j]))
                .fold(f64::NEG_INFINITY, f64::max)
        })
        .collect()
}

pub fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    match sorted.len() {
        0 => 0.0,
        n if n % 2 == 1 => sorted[n / 2],
        n => (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0,
    }
}

/// Indices into `pts` per day, oldest day first.
fn by_day(pts: &[&BlockPoint]) -> BTreeMap<String, Vec<usize>> {
    let mut days: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, p) in pts.iter().enumerate() {
        days.entry(p.day.clone()).or_default().push(i);
    }
    days
}

/// Mean pairwise cosine, each pair weighted by the product of its blocks' seconds.
fn focus(members: &[usize], pts: &[&BlockPoint], centered: &[Vec<f32>]) -> Option<f64> {
    let (mut sum, mut weight) = (0.0, 0.0);
    for (a, &i) in members.iter().enumerate() {
        for &j in &members[a + 1..] {
            let w = (pts[i].seconds * pts[j].seconds) as f64;
            sum += w * cosine(&centered[i], &centered[j]);
            weight += w;
        }
    }
    (weight > 0.0).then(|| sum / weight)
}

/// Topic changes between consecutive blocks (by start time) and the longest
/// wall-clock stretch inside one topic.
fn switches(members: &[usize], pts: &[&BlockPoint], topic_of: &[usize]) -> (i64, i64) {
    let mut order = members.to_vec();
    order.sort_by_key(|&i| (started_ts(pts[i]), pts[i].id));
    let (mut changes, mut longest) = (0, 0);
    let mut run: Option<(usize, i64, i64)> = None; // topic, start, end
    for &i in &order {
        let (start, end) = (started_ts(pts[i]), started_ts(pts[i]) + pts[i].seconds);
        run = match run {
            Some((t, s, e)) if t == topic_of[i] => Some((t, s, e.max(end))),
            prev => {
                changes += i64::from(prev.is_some());
                Some((topic_of[i], start, end))
            }
        };
        if let Some((_, s, e)) = run {
            longest = longest.max(e - s);
        }
    }
    (changes, longest)
}

/// One row per day, oldest first. All slices are aligned with `pts`.
pub fn day_rows(
    pts: &[&BlockPoint],
    centered: &[Vec<f32>],
    novelty: &[f64],
    topic_of: &[usize],
) -> Vec<DayRow> {
    by_day(pts)
        .into_iter()
        .map(|(day, members)| {
            let work_seconds: i64 = members.iter().map(|&i| pts[i].seconds).sum();
            let weighted_novelty: f64 = members
                .iter()
                .map(|&i| novelty[i] * pts[i].seconds as f64)
                .sum();
            let mut per_topic: BTreeMap<usize, i64> = BTreeMap::new();
            for &i in &members {
                *per_topic.entry(topic_of[i]).or_default() += pts[i].seconds;
            }
            // Ascending topic id + strict `>`: a tie keeps the lower id.
            let top_topic = per_topic
                .iter()
                .fold(
                    (0, i64::MIN),
                    |best, (&t, &s)| {
                        if s > best.1 {
                            (t, s)
                        } else {
                            best
                        }
                    },
                )
                .0;
            let (changes, longest_run_seconds) = switches(&members, pts, topic_of);
            DayRow {
                day,
                work_seconds,
                focus: focus(&members, pts, centered),
                novelty: if work_seconds > 0 {
                    weighted_novelty / work_seconds as f64
                } else {
                    0.0
                },
                switches: changes,
                longest_run_seconds,
                top_topic,
                topics_touched: per_topic.len() as i64,
            }
        })
        .collect()
}

/// Cosine between hours-weighted day centroids of the last MIRROR_MAX_DAYS days
/// of `days`; symmetric, diagonal 1, 3 decimals.
pub fn day_similarity(
    pts: &[&BlockPoint],
    centered: &[Vec<f32>],
    days: &[DayRow],
) -> Vec<Vec<f64>> {
    let members = by_day(pts);
    let shown = &days[days.len().saturating_sub(MIRROR_MAX_DAYS)..];
    let centroids: Vec<Vec<f64>> = shown
        .iter()
        .map(|d| {
            let mut acc = vec![0.0; centered.first().map_or(0, Vec::len)];
            for &i in members.get(&d.day).into_iter().flatten() {
                for (a, x) in acc.iter_mut().zip(&centered[i]) {
                    *a += pts[i].seconds as f64 * f64::from(*x);
                }
            }
            acc
        })
        .collect();
    let round = |x: f64| (x * 1000.0).round() / 1000.0;
    (0..shown.len())
        .map(|i| {
            (0..shown.len())
                .map(|j| match i.cmp(&j) {
                    Ordering::Equal => 1.0,
                    // One computation per unordered pair keeps the matrix exactly symmetric.
                    _ => round(cosine(&centroids[i.min(j)], &centroids[i.max(j)])),
                })
                .collect()
        })
        .collect()
}

fn monday(day: NaiveDate) -> NaiveDate {
    day - Duration::days(i64::from(day.weekday().num_days_from_monday()))
}

/// Seconds per topic in every bin from `from` to `to`, zeros included. Bins are
/// days, or ISO weeks (keyed by Monday) when the range spans more than
/// RIVER_WEEK_AFTER_DAYS days, both ends counted.
pub fn river(
    pts: &[&BlockPoint],
    topic_of: &[usize],
    topic_count: usize,
    from: &str,
    to: &str,
) -> (Bin, Vec<RiverBin>) {
    let (Ok(first), Ok(last)) = (
        NaiveDate::parse_from_str(from, "%Y-%m-%d"),
        NaiveDate::parse_from_str(to, "%Y-%m-%d"),
    ) else {
        return (Bin::Day, vec![]);
    };
    let weekly = (last - first).num_days() + 1 > RIVER_WEEK_AFTER_DAYS;
    let (bin, step, start) = if weekly {
        (Bin::Week, 7, monday(first))
    } else {
        (Bin::Day, 1, first)
    };
    let mut bins = vec![];
    let mut at = start;
    while at <= last {
        bins.push(RiverBin {
            start: at.to_string(),
            seconds: vec![0; topic_count],
        });
        at += Duration::days(step);
    }
    for (i, p) in pts.iter().enumerate() {
        let Ok(day) = NaiveDate::parse_from_str(&p.day, "%Y-%m-%d") else {
            continue;
        };
        let day = if weekly { monday(day) } else { day };
        let slot = usize::try_from((day - start).num_days() / step).ok();
        if let Some(b) = slot.and_then(|s| bins.get_mut(s)) {
            b.seconds[topic_of[i]] += p.seconds;
        }
    }
    (bin, bins)
}

#[cfg(test)]
#[path = "topic_metrics_test.rs"]
mod tests;
