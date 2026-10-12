use super::*;
use crate::topics_contract::{MIRROR_MAX_DAYS, RIVER_WEEK_AFTER_DAYS};

fn block(id: i64, day: &str, hm: &str, seconds: i64, vec: [f32; 2]) -> BlockPoint {
    BlockPoint {
        id,
        day: day.to_string(),
        started_at: format!("{day}T{hm}:00+00:00"),
        seconds,
        ticket: None,
        description: None,
        vec: vec.to_vec(),
    }
}

fn refs(v: &[BlockPoint]) -> Vec<&BlockPoint> {
    v.iter().collect()
}

fn vecs(v: &[BlockPoint]) -> Vec<Vec<f32>> {
    v.iter().map(|p| p.vec.clone()).collect()
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "{a} != {b}");
}

#[test]
fn center_subtracts_the_mean_and_normalises() {
    // catches: no mean subtraction, no normalisation.
    let c = center(&[vec![2.0, 0.0], vec![0.0, 2.0]]);
    let r = std::f64::consts::FRAC_1_SQRT_2;
    close(f64::from(c[0][0]), r);
    close(f64::from(c[0][1]), -r);
    close(f64::from(c[1][0]), -r);
    close(f64::from(c[1][1]), r);
}

#[test]
fn center_keeps_a_row_that_equals_the_mean() {
    // catches: dividing by a zero norm (NaN) instead of keeping the row.
    let c = center(&[vec![1.0, 1.0], vec![1.0, 1.0]]);
    assert_eq!(c, vec![vec![1.0, 1.0], vec![1.0, 1.0]]);
}

#[test]
fn center_separates_groups_hidden_by_a_shared_component() {
    // catches: skipping centering (raw cross-group cosine stays ~0.98).
    let raw = vec![vec![10.0, 1.0], vec![10.0, -1.0]];
    assert!(cosine(&raw[0], &raw[1]) > 0.95);
    let c = center(&raw);
    close(cosine(&c[0], &c[1]), -1.0);
}

#[test]
fn novelty_of_the_first_block_ever_is_one() {
    // catches: 0.0 or NaN when no earlier block exists.
    let now = vec![block(1, "2026-10-06", "09:00", 60, [1.0, 0.0])];
    assert_eq!(novelty(&now, &vecs(&now), &[], &[]), vec![1.0]);
}

#[test]
fn novelty_counts_only_strictly_earlier_blocks() {
    // catches: `<=` (same start time counted as earlier), comparing against later blocks.
    let now = vec![
        block(1, "2026-10-06", "09:00", 60, [1.0, 0.0]),
        block(2, "2026-10-06", "09:00", 60, [1.0, 0.0]),
        block(3, "2026-10-06", "10:00", 60, [1.0, 0.0]),
    ];
    let got = novelty(&now, &vecs(&now), &[], &[]);
    assert_eq!(got, vec![1.0, 1.0, 0.0]);
}

#[test]
fn novelty_looks_into_history_and_ignores_later_history() {
    // catches: range-only novelty, history from the future counted.
    let now = vec![block(1, "2026-10-06", "09:00", 60, [1.0, 0.0])];
    let past = vec![block(9, "2026-09-01", "09:00", 60, [0.6, 0.8])];
    let future = vec![block(8, "2026-10-07", "09:00", 60, [1.0, 0.0])];
    close(novelty(&now, &vecs(&now), &past, &vecs(&past))[0], 0.4);
    assert_eq!(
        novelty(&now, &vecs(&now), &future, &vecs(&future)),
        vec![1.0]
    );
}

#[test]
fn novelty_uses_the_best_match_not_the_latest() {
    // catches: comparing only the previous block.
    let now = vec![
        block(1, "2026-10-06", "09:00", 60, [1.0, 0.0]),
        block(2, "2026-10-06", "10:00", 60, [0.0, 1.0]),
        block(3, "2026-10-06", "11:00", 60, [1.0, 0.0]),
    ];
    assert_eq!(novelty(&now, &vecs(&now), &[], &[])[2], 0.0);
}

#[test]
fn nearest_cos_excludes_the_block_itself() {
    // catches: including self (always 1.0), taking the mean instead of the max.
    let c = vec![vec![1.0, 0.0], vec![0.6, 0.8], vec![0.0, 1.0]];
    let got = nearest_cos(&c);
    close(got[0], 0.6);
    close(got[1], 0.8);
    close(got[2], 0.8);
}

#[test]
fn median_of_odd_even_and_empty() {
    // catches: upper-middle for even counts, panic on empty.
    close(median(&[3.0, 1.0, 2.0]), 2.0);
    close(median(&[4.0, 1.0, 2.0, 3.0]), 2.5);
    close(median(&[]), 0.0);
}

#[test]
fn day_focus_is_none_for_one_block_and_hours_weighted_otherwise() {
    // catches: Some(0) for a lone block, an unweighted mean (would be 1/3).
    let pts = vec![
        block(1, "2026-10-05", "09:00", 3600, [1.0, 0.0]),
        block(2, "2026-10-06", "09:00", 7200, [1.0, 0.0]),
        block(3, "2026-10-06", "10:00", 3600, [1.0, 0.0]),
        block(4, "2026-10-06", "11:00", 3600, [0.0, 1.0]),
    ];
    let rows = day_rows(
        &refs(&pts),
        &vecs(&pts),
        &[1.0; 4],
        &[false; 4],
        &[0, 0, 0, 0],
    );
    assert_eq!(rows[0].day, "2026-10-05");
    assert_eq!(rows[0].focus, None);
    // pair weights 2h*1h=2 (cos 1), 2h*1h=2 (cos 0), 1h*1h=1 (cos 0) -> 2/5.
    close(rows[1].focus.unwrap(), 0.4);
}

#[test]
fn day_novelty_is_hours_weighted() {
    // catches: unweighted mean (would be 2/3).
    let pts = vec![
        block(1, "2026-10-06", "09:00", 3600, [1.0, 0.0]),
        block(2, "2026-10-06", "10:00", 3600, [1.0, 0.0]),
        block(3, "2026-10-06", "11:00", 7200, [1.0, 0.0]),
    ];
    let rows = day_rows(
        &refs(&pts),
        &vecs(&pts),
        &[1.0, 1.0, 0.0],
        &[false; 3],
        &[0, 0, 0],
    );
    close(rows[0].novelty, 0.5);
}

#[test]
fn day_switches_follow_started_order_and_longest_run_is_wall_clock() {
    // catches: input order, counting blocks instead of changes, a run measured as summed seconds.
    let pts = vec![
        block(3, "2026-10-06", "11:00", 600, [1.0, 0.0]),
        block(1, "2026-10-06", "09:00", 1800, [1.0, 0.0]),
        block(2, "2026-10-06", "10:00", 1800, [1.0, 0.0]),
        block(4, "2026-10-06", "12:00", 600, [0.0, 1.0]),
        block(5, "2026-10-06", "13:00", 600, [1.0, 0.0]),
    ];
    let topic_of = [0, 0, 0, 1, 0];
    let rows = day_rows(&refs(&pts), &vecs(&pts), &[0.0; 5], &[false; 5], &topic_of);
    assert_eq!(rows[0].switches, 2); // 0 0 0 | 1 | 0
    assert_eq!(rows[0].work_seconds, 5400);
    // 09:00 .. 11:10 = 7800s; summed seconds would be 4200.
    assert_eq!(rows[0].longest_run_seconds, 7800);
}

#[test]
fn day_top_topic_is_by_seconds_with_lowest_id_on_ties() {
    // catches: top by block count, a tie going to the later topic, wrong touched count.
    let pts = vec![
        block(1, "2026-10-06", "09:00", 600, [1.0, 0.0]),
        block(2, "2026-10-06", "10:00", 600, [1.0, 0.0]),
        block(3, "2026-10-06", "11:00", 1200, [0.0, 1.0]),
        block(4, "2026-10-07", "09:00", 600, [1.0, 0.0]),
        block(5, "2026-10-07", "10:00", 900, [0.0, 1.0]),
        block(6, "2026-10-07", "11:00", 900, [0.0, 1.0]),
    ];
    let rows = day_rows(
        &refs(&pts),
        &vecs(&pts),
        &[0.0; 6],
        &[false; 6],
        &[0, 0, 1, 0, 1, 1],
    );
    assert_eq!((rows[0].top_topic, rows[0].topics_touched), (0, 2)); // 1200 vs 1200 tie
    assert_eq!((rows[1].top_topic, rows[1].topics_touched), (1, 2));
}

fn one_per_day(days: usize, vec_of: impl Fn(usize) -> [f32; 2]) -> Vec<BlockPoint> {
    (0..days)
        .map(|i| {
            let day = (NaiveDate::from_ymd_opt(2026, 1, 1).unwrap() + Duration::days(i as i64))
                .to_string();
            block(i as i64 + 1, &day, "09:00", 3600, vec_of(i))
        })
        .collect()
}

fn similarity_of(pts: &[BlockPoint]) -> Vec<Vec<f64>> {
    let rows = day_rows(
        &refs(pts),
        &vecs(pts),
        &vec![0.0; pts.len()],
        &vec![false; pts.len()],
        &vec![0; pts.len()],
    );
    day_similarity(&refs(pts), &vecs(pts), &rows)
}

#[test]
fn day_similarity_is_symmetric_with_unit_diagonal_and_rounded() {
    // catches: asymmetric matrix, diagonal != 1, no rounding (0.4472136 -> 0.447).
    let pts = one_per_day(3, |i| [[1.0, 0.0], [1.0, 2.0], [0.0, 1.0]][i]);
    let m = similarity_of(&pts);
    assert_eq!(m.len(), 3);
    for (i, row) in m.iter().enumerate() {
        assert_eq!(row[i], 1.0);
        for (j, x) in row.iter().enumerate() {
            assert_eq!(*x, m[j][i]);
        }
    }
    assert_eq!(m[0][1], 0.447);
    assert_eq!(m[0][2], 0.0);
}

#[test]
fn day_similarity_weights_by_hours() {
    // catches: unweighted centroid (would put the centroid at 45 degrees -> 0.707).
    let pts = vec![
        block(1, "2026-10-06", "09:00", 3600, [1.0, 0.0]),
        block(2, "2026-10-06", "10:00", 7200, [0.0, 1.0]),
        block(3, "2026-10-07", "09:00", 3600, [0.0, 1.0]),
    ];
    let m = similarity_of(&pts);
    assert_eq!(m[0][1], 0.894); // cos of (1,2) vs (0,1) = 2/sqrt(5)
}

#[test]
fn day_similarity_keeps_only_the_last_mirror_days() {
    // catches: no cap, keeping the oldest days (day 1 is the odd one out and must be dropped).
    let pts = one_per_day(MIRROR_MAX_DAYS + 1, |i| {
        if i == 0 {
            [0.0, 1.0]
        } else {
            [1.0, 0.0]
        }
    });
    let m = similarity_of(&pts);
    assert_eq!(m.len(), MIRROR_MAX_DAYS);
    assert!(m.iter().flatten().all(|&x| x == 1.0));
    let exact = one_per_day(MIRROR_MAX_DAYS, |_| [1.0, 0.0]);
    assert_eq!(similarity_of(&exact).len(), MIRROR_MAX_DAYS);
}

#[test]
fn river_day_bins_cover_the_range_with_zeros_and_sum_to_topic_seconds() {
    // catches: bins only for days with work, a topic's seconds lost or double counted.
    let pts = vec![
        block(1, "2026-10-06", "09:00", 100, [1.0, 0.0]),
        block(2, "2026-10-06", "10:00", 50, [1.0, 0.0]),
        block(3, "2026-10-08", "09:00", 70, [0.0, 1.0]),
    ];
    let (bin, bins) = river(&refs(&pts), &[0, 1, 1], 2, "2026-10-05", "2026-10-09");
    assert_eq!(bin, Bin::Day);
    let starts: Vec<_> = bins.iter().map(|b| b.start.as_str()).collect();
    assert_eq!(
        starts,
        [
            "2026-10-05",
            "2026-10-06",
            "2026-10-07",
            "2026-10-08",
            "2026-10-09"
        ]
    );
    assert_eq!(bins[1].seconds, vec![100, 50]);
    assert_eq!(bins[3].seconds, vec![0, 70]);
    let totals: Vec<i64> = (0..2)
        .map(|t| bins.iter().map(|b| b.seconds[t]).sum())
        .collect();
    assert_eq!(totals, vec![100, 120]);
}

#[test]
fn river_switches_to_weeks_only_above_the_threshold() {
    // catches: `>=` instead of `>` (90 inclusive days stay daily), a threshold of 91 days.
    assert_eq!(RIVER_WEEK_AFTER_DAYS, 90);
    let (bin, bins) = river(&[], &[], 1, "2026-07-14", "2026-10-11"); // 90 days inclusive
    assert_eq!((bin, bins.len()), (Bin::Day, 90));
    let (bin, bins) = river(&[], &[], 1, "2026-07-13", "2026-10-11"); // 91 days inclusive
    assert_eq!((bin, bins.len()), (Bin::Week, 13));
}

#[test]
fn river_weeks_start_on_monday_and_sum_blocks_per_week() {
    // catches: weeks starting on `from`, Sunday landing in the next week, a bin dropped.
    let pts = vec![
        block(1, "2026-07-12", "09:00", 10, [1.0, 0.0]), // Sunday, before the first Monday
        block(2, "2026-07-19", "09:00", 20, [1.0, 0.0]), // Sunday of week 07-13
        block(3, "2026-07-20", "09:00", 40, [1.0, 0.0]), // Monday of the next week
    ];
    let (bin, bins) = river(&refs(&pts), &[0, 0, 0], 1, "2026-07-12", "2026-10-11");
    assert_eq!(bin, Bin::Week);
    assert_eq!(bins[0].start, "2026-07-06");
    assert_eq!(bins.last().unwrap().start, "2026-10-05");
    assert_eq!(bins[0].seconds, vec![10]);
    assert_eq!(bins[1].seconds, vec![20]);
    assert_eq!(bins[2].seconds, vec![40]);
    let total: i64 = bins.iter().map(|b| b.seconds[0]).sum();
    assert_eq!(total, 70);
}

/// n blocks one minute apart starting at 09:00 on 2026-10-05, ids from `first_id`.
fn minutes(first_id: i64, n: usize, day: &str) -> Vec<BlockPoint> {
    (0..n)
        .map(|i| {
            block(
                first_id + i as i64,
                day,
                &format!("09:{i:02}"),
                60,
                [1.0, 0.0],
            )
        })
        .collect()
}

#[test]
fn warmup_lasts_until_twenty_blocks_precede() {
    // catches: `<=` for `<` (block 21 skipped), a warm-up of 19 (block 20 judged).
    let now = minutes(1, 22, "2026-10-05");
    let w = warmup(&now, &[]);
    assert!(w[..20].iter().all(|&x| x), "first 20 are warm: {w:?}");
    assert!(!w[20] && !w[21], "block 21 onwards is judged: {w:?}");
}

#[test]
fn warmup_counts_history_before_the_range() {
    // catches: counting only the range, counting later history.
    let one = minutes(100, 1, "2026-10-06");
    let mut past = minutes(1, 19, "2026-10-05");
    assert_eq!(warmup(&one, &past), vec![true]); // 19 precede
    past.extend(minutes(50, 1, "2026-10-05"));
    past[19].started_at = "2026-10-05T12:00:00+00:00".into();
    assert_eq!(warmup(&one, &past), vec![false]); // 20 precede
    let future = minutes(1, 25, "2026-10-07");
    assert_eq!(warmup(&one, &future), vec![true]); // later blocks never count
}

#[test]
fn warmup_ignores_blocks_that_start_at_the_same_time() {
    // catches: `<=` in the comparison (same-instant blocks counted as earlier).
    let mut now = minutes(1, 21, "2026-10-05");
    for p in &mut now {
        p.started_at = "2026-10-05T09:00:00+00:00".into();
    }
    assert!(warmup(&now, &[]).iter().all(|&x| x));
}

#[test]
fn day_of_only_warmup_blocks_gets_the_median_of_the_other_days() {
    // catches: 0.0 or the raw novelty for an all-warm day; warm blocks leaking into a mixed day.
    let pts = vec![
        block(1, "2026-10-05", "09:00", 3600, [1.0, 0.0]),
        block(2, "2026-10-06", "09:00", 3600, [1.0, 0.0]),
        block(3, "2026-10-07", "09:00", 3600, [1.0, 0.0]),
        block(4, "2026-10-08", "09:00", 3600, [1.0, 0.0]),
        block(5, "2026-10-08", "10:00", 3600, [1.0, 0.0]),
    ];
    let nov = [0.9, 0.2, 0.6, 0.4, 0.99];
    let warm = [true, false, false, false, true];
    let rows = day_rows(&refs(&pts), &vecs(&pts), &nov, &warm, &[0; 5]);
    close(rows[1].novelty, 0.2);
    close(rows[2].novelty, 0.6);
    close(rows[3].novelty, 0.4); // the warm 0.99 block is not averaged in
    close(rows[0].novelty, 0.4); // median of 0.2, 0.6, 0.4
}

#[test]
fn every_day_warm_leaves_novelty_zero() {
    // catches: panic on an empty median.
    let pts = vec![block(1, "2026-10-05", "09:00", 60, [1.0, 0.0])];
    let rows = day_rows(&refs(&pts), &vecs(&pts), &[1.0], &[true], &[0]);
    close(rows[0].novelty, 0.0);
}
