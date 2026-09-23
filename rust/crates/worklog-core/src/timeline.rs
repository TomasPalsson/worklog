//! Pure helpers for per-block confidence and day-level gap detection.

use chrono::{DateTime, Duration, Utc};

/// Confidence label for a block based on how many distinct sources fed it.
pub fn block_confidence(distinct_sources: usize) -> &'static str {
    match distinct_sources {
        0 | 1 => "low",
        2 => "medium",
        _ => "high",
    }
}

/// Gaps of at least `min_gap` between consecutive blocks, sorted by start
/// time. Overlapping/adjacent blocks are merged first so overlaps never
/// produce a spurious gap.
pub fn day_gaps(
    blocks: &[(DateTime<Utc>, DateTime<Utc>)],
    min_gap: Duration,
) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    let mut sorted: Vec<(DateTime<Utc>, DateTime<Utc>)> = blocks.to_vec();
    sorted.sort_by_key(|(start, _)| *start);

    let mut merged: Vec<(DateTime<Utc>, DateTime<Utc>)> = Vec::new();
    for (start, end) in sorted {
        match merged.last_mut() {
            Some((_, last_end)) if start <= *last_end => {
                if end > *last_end {
                    *last_end = end;
                }
            }
            _ => merged.push((start, end)),
        }
    }

    merged
        .windows(2)
        .filter_map(|w| {
            let (_, prev_end) = w[0];
            let (next_start, _) = w[1];
            let gap = next_start - prev_end;
            (gap >= min_gap).then_some((prev_end, next_start))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 23, h, m, 0).unwrap()
    }

    #[test]
    fn confidence_thresholds() {
        assert_eq!(block_confidence(0), "low");
        assert_eq!(block_confidence(1), "low");
        assert_eq!(block_confidence(2), "medium");
        assert_eq!(block_confidence(3), "high");
        assert_eq!(block_confidence(5), "high");
    }

    #[test]
    fn gap_of_45_minutes_is_reported() {
        let blocks = vec![(t(9, 0), t(10, 0)), (t(10, 45), t(11, 0))];
        let gaps = day_gaps(&blocks, Duration::minutes(30));
        assert_eq!(gaps, vec![(t(10, 0), t(10, 45))]);
    }

    #[test]
    fn gap_of_10_minutes_not_reported_at_min_30() {
        let blocks = vec![(t(9, 0), t(10, 0)), (t(10, 10), t(11, 0))];
        let gaps = day_gaps(&blocks, Duration::minutes(30));
        assert!(gaps.is_empty());
    }

    #[test]
    fn overlapping_blocks_produce_no_gap() {
        let blocks = vec![
            (t(9, 0), t(10, 0)),
            (t(9, 30), t(10, 30)),
            (t(10, 15), t(11, 0)),
        ];
        let gaps = day_gaps(&blocks, Duration::minutes(5));
        assert!(gaps.is_empty());
    }
}
