use super::*;
use crate::topic_metrics::NOVELTY_WARMUP_BLOCKS;
use crate::topics_contract::{TopicsStatus, MAX_TOPICS, MIN_BLOCKS, ODDBALL_MAX_COS};

const A: [f32; 2] = [1.0, 0.0];
const B: [f32; 2] = [0.0, 1.0];

fn pt(id: i64, day: &str, hm: &str, seconds: i64, ticket: Option<&str>, v: [f32; 2]) -> BlockPoint {
    BlockPoint {
        id,
        day: day.to_string(),
        started_at: format!("{day}T{hm}:00+00:00"),
        seconds,
        ticket: ticket.map(str::to_string),
        description: Some("x".to_string()),
        vec: v.to_vec(),
    }
}

fn input(now: Vec<BlockPoint>) -> TopicsInput {
    TopicsInput {
        from: "2026-10-05".into(),
        to: "2026-10-11".into(),
        now,
        history: vec![],
        ticket_summaries: HashMap::new(),
    }
}

/// n blocks spread over a quarter circle, one per angle, so any k is reachable.
fn fan(n: usize) -> Vec<BlockPoint> {
    (0..n)
        .map(|i| {
            let t = (i as f32) * 1.5 / (n as f32);
            pt(
                i as i64 + 1,
                "2026-10-06",
                "09:00",
                60,
                None,
                [t.cos(), t.sin()],
            )
        })
        .collect()
}

/// Three A blocks (day 06) then three B blocks (day 07).
fn two_groups() -> Vec<BlockPoint> {
    vec![
        pt(1, "2026-10-06", "09:00", 3600, Some("A-1"), A),
        pt(2, "2026-10-06", "10:00", 1800, Some("A-1"), [0.99, 0.01]),
        pt(3, "2026-10-06", "11:00", 600, Some("A-2"), [0.98, 0.02]),
        pt(4, "2026-10-07", "09:00", 900, Some("B-1"), B),
        pt(5, "2026-10-07", "10:00", 2400, None, [0.01, 0.99]),
        pt(6, "2026-10-07", "11:00", 300, None, [0.02, 0.98]),
    ]
}

fn ready(i: &TopicsInput) -> TopicsReport {
    let r = build(i);
    assert_eq!(r.status, TopicsStatus::Ready);
    r
}

#[test]
fn too_few_blocks_just_below_min_and_ready_at_min() {
    // catches: `n <= MIN_BLOCKS` (off by one) and a missing guard.
    let r = build(&input(fan(MIN_BLOCKS - 1)));
    assert_eq!(
        r,
        TopicsReport::empty(TopicsStatus::TooFewBlocks, "2026-10-05", "2026-10-11")
    );
    let r = build(&input(fan(MIN_BLOCKS)));
    assert_eq!(r.status, TopicsStatus::Ready);
    assert_eq!(r.embedded_blocks, MIN_BLOCKS as i64);
}

#[test]
fn topic_count_follows_rounded_sqrt_and_clamps() {
    // catches: floor/ceil instead of round, missing MIN clamp, missing MAX clamp.
    assert_eq!(ready(&input(fan(6))).topics.len(), 2); // sqrt(3)=1.73 -> 2
    assert_eq!(ready(&input(fan(10))).topics.len(), 2); // sqrt(5)=2.24: ceil would give 3
    assert_eq!(ready(&input(fan(14))).topics.len(), 3); // sqrt(7)=2.65: floor would give 2
    assert_eq!(ready(&input(fan(200))).topics.len(), MAX_TOPICS); // sqrt(100)=10 -> 8
}

#[test]
fn clusters_by_direction_ordered_by_seconds_with_totals() {
    // catches: clustering by input order, topics not sorted by seconds, wrong sums.
    let r = ready(&input(two_groups()));
    assert_eq!(r.topics.len(), 2);
    assert_eq!((r.topics[0].id, r.topics[1].id), (0, 1));
    assert_eq!((r.topics[0].seconds, r.topics[0].blocks), (6000, 3));
    assert_eq!((r.topics[1].seconds, r.topics[1].blocks), (3600, 3));
}

#[test]
fn topic_days_and_tickets() {
    // catches: ranking tickets by block count, wrong first/last day, wrong days_active,
    // and fields T005 owns being filled early.
    let r = ready(&input(two_groups()));
    let got: Vec<_> = r.topics[0]
        .top_tickets
        .iter()
        .map(|t| (t.label.as_str(), t.value))
        .collect();
    assert_eq!(got, vec![("A-1", 5400), ("A-2", 600)]);
    let t = &r.topics[1];
    assert_eq!(
        (t.days_active, t.first_day.as_str(), t.last_day.as_str()),
        (1, "2026-10-07", "2026-10-07")
    );
    assert_eq!(t.name, "");
    assert!(t.keywords.is_empty() && r.points.is_empty());
}

#[test]
fn topic_days_active_counts_distinct_days() {
    // catches: counting blocks as days, first/last taken from input order.
    let now = vec![
        pt(1, "2026-10-08", "09:00", 60, None, A),
        pt(2, "2026-10-08", "10:00", 60, None, A),
        pt(3, "2026-10-06", "09:00", 60, None, A),
        pt(4, "2026-10-07", "09:00", 60, None, B),
        pt(5, "2026-10-07", "10:00", 60, None, B),
        pt(6, "2026-10-09", "10:00", 60, None, B),
    ];
    let r = ready(&input(now));
    let a = r
        .topics
        .iter()
        .find(|t| t.first_day == "2026-10-06")
        .unwrap();
    assert_eq!((a.days_active, a.last_day.as_str()), (2, "2026-10-08"));
}

/// Two groups sharing a big component: raw cosines are ~0.98 everywhere, the
/// group is only visible after centering. `ticket` marks the group.
fn anisotropic() -> Vec<BlockPoint> {
    let mut out = vec![];
    for i in 0..12i64 {
        let group = i % 2;
        let jitter = (i / 2) as f32 * 0.02;
        let sign = if group == 0 { 1.0 } else { -1.0 };
        out.push(pt(
            i + 1,
            "2026-10-06",
            "09:00",
            60,
            Some(["A", "B"][group as usize]),
            [10.0 + jitter, sign],
        ));
    }
    out
}

#[test]
fn cluster_splits_anisotropic_groups_after_centering() {
    // catches: clustering raw vectors, skipping centering, mixing the groups.
    let pts = anisotropic();
    let vectors: Vec<Vec<f32>> = pts.iter().map(|p| p.vec.clone()).collect();
    let labels = cluster(&crate::topic_metrics::center(&vectors), 2);
    let agree = pts
        .iter()
        .zip(&labels)
        .filter(|(p, l)| (p.ticket.as_deref() == Some("A")) == (**l == labels[0]))
        .count();
    assert!(
        agree * 10 >= pts.len() * 9,
        "group purity {agree}/{}",
        pts.len()
    );
    let r = ready(&input(pts));
    assert_eq!((r.topics[0].blocks, r.topics[1].blocks), (6, 6));
    assert_eq!(r.topics[0].top_tickets.len(), 1);
}

#[test]
fn first_block_has_novelty_one_and_repeats_have_none() {
    // catches: novelty of the first block 0, history ignored in build.
    let now = vec![
        pt(1, "2026-10-05", "09:00", 600, None, A),
        pt(2, "2026-10-06", "09:00", 600, None, B),
        pt(3, "2026-10-06", "10:00", 600, None, A),
        pt(4, "2026-10-07", "09:00", 600, None, B),
        pt(5, "2026-10-07", "10:00", 600, None, A),
        pt(6, "2026-10-07", "11:00", 600, None, B),
    ];
    let mut i = input(now);
    i.history = (0..NOVELTY_WARMUP_BLOCKS as i64)
        .map(|k| pt(100 + k, "2026-09-01", &format!("09:{k:02}"), 600, None, B))
        .collect();
    let r = ready(&i);
    assert!(r.days[0].novelty > 0.5); // block 1 is new against a history of B
    assert!(r.days[2].novelty.abs() < 1e-6); // every block that day repeats an earlier one
    assert!(r.first_times.iter().any(|h| h.block_id == 1));
}

#[test]
fn history_lowers_novelty_of_a_repeated_theme() {
    // catches: build ignoring `history`.
    let warmed = |v: [f32; 2]| -> Vec<BlockPoint> {
        (0..NOVELTY_WARMUP_BLOCKS as i64)
            .map(|k| pt(100 + k, "2026-09-01", &format!("09:{k:02}"), 600, None, v))
            .collect()
    };
    let mut i = input(two_groups());
    i.history = warmed(B);
    let alone = ready(&i).days[0].novelty;
    i.history = warmed(A);
    assert!(ready(&i).days[0].novelty < alone);
}

fn outlier_set() -> Vec<BlockPoint> {
    let mut v: Vec<BlockPoint> = (0..7)
        .map(|i| {
            pt(
                i + 1,
                "2026-10-06",
                "09:00",
                600,
                None,
                [1.0, 0.01 * i as f32],
            )
        })
        .collect();
    v.push(pt(8, "2026-10-07", "09:00", 600, None, B));
    v
}

#[test]
fn oddball_is_the_block_far_from_all_others() {
    // catches: flagging cluster members, missing the outlier, wrong score, wrong median.
    let r = ready(&input(outlier_set()));
    assert_eq!(r.oddballs.len(), 1);
    assert_eq!(r.oddballs[0].block_id, 8);
    assert!(r.oddballs[0].score < ODDBALL_MAX_COS);
    assert!(r.oddball_median > 0.9);
}

#[test]
fn oddball_threshold_is_strictly_below() {
    // catches: `<=` (a block exactly at the threshold flagged), `>`.
    assert!(is_oddball(ODDBALL_MAX_COS - 1e-9));
    assert!(!is_oddball(ODDBALL_MAX_COS));
    assert!(!is_oddball(ODDBALL_MAX_COS + 1e-9));
}

#[test]
fn highlights_are_capped_and_ordered() {
    // catches: no cap, wrong order for first_times (novelty desc) and oddballs (cos asc).
    let r = ready(&input(fan(40)));
    assert!(r.first_times.len() <= HIGHLIGHTS_TOP);
    assert!(r.first_times.windows(2).all(|w| w[0].score >= w[1].score));
    assert!(r.oddballs.windows(2).all(|w| w[0].score <= w[1].score));
}

#[test]
fn report_has_days_river_and_similarity() {
    // catches: days/river/day_similarity not wired into build.
    let r = ready(&input(two_groups()));
    assert_eq!(r.days.len(), 2);
    assert_eq!(r.day_similarity.len(), 2);
    assert_eq!(r.river.len(), 7); // 2026-10-05 ..= 2026-10-11
    let total: i64 = r.river.iter().flat_map(|b| &b.seconds).sum();
    assert_eq!(total, 9600);
    assert_eq!(r.river[0].seconds.len(), r.topics.len());
}

#[test]
fn build_is_deterministic_under_input_order() {
    // catches: input order leaking into the output.
    let a = ready(&input(two_groups()));
    let mut rev = two_groups();
    rev.reverse();
    assert_eq!(a, ready(&input(rev)));
}

const M: [f32; 2] = [1.0, 1.0];

fn tie_group(first_id: i64, v: [f32; 2], ticket: &str) -> Vec<BlockPoint> {
    (0..2)
        .map(|i| pt(first_id + i, "2026-10-06", "09:00", 60, Some(ticket), v))
        .collect()
}

fn lead_ticket(r: &TopicsReport) -> &str {
    &r.topics[0].top_tickets[0].label
}

#[test]
fn equidistant_cluster_pairs_merge_lower_id_first() {
    // catches: last-pair-wins on an exact distance tie (M is equidistant from A and B).
    let mut pts = tie_group(1, A, "a");
    pts.extend(tie_group(3, M, "m"));
    pts.extend(tie_group(5, B, "b"));
    let r = ready(&input(pts));
    assert_eq!(r.topics.len(), 2);
    assert_eq!(r.topics[0].blocks, 4);
    assert_eq!(lead_ticket(&r), "a"); // A+M merged, not M+B
}

#[test]
fn equal_seconds_topics_order_by_lowest_member_id() {
    // catches: unstable or reversed order among topics with equal seconds.
    let mut pts = tie_group(1, A, "a");
    pts.extend(tie_group(3, B, "b"));
    pts.extend(tie_group(5, A, "a"));
    pts.extend(tie_group(7, B, "b"));
    pts.extend(tie_group(9, A, "a"));
    pts.extend(tie_group(11, B, "b"));
    assert_eq!(lead_ticket(&ready(&input(pts.clone()))), "a");
    for p in &mut pts {
        p.id = 100 - p.id; // B groups now hold the lowest ids
    }
    assert_eq!(lead_ticket(&ready(&input(pts))), "b");
}

#[test]
fn assignments_match_the_reports_topics_block_for_block() {
    let i = input(two_groups());
    let (r, a) = build_with_assignments(&i);

    // catches: assignments from a second clustering that disagrees with `topics`.
    assert_eq!(a.block_ids, vec![1, 2, 3, 4, 5, 6]);
    assert_eq!(a.topic_of.len(), 6);
    for t in &r.topics {
        let members: Vec<usize> = (0..6).filter(|&k| a.topic_of[k] == t.id).collect();
        assert_eq!(members.len() as i64, t.blocks, "topic {}", t.id);
        let seconds: i64 = members.iter().map(|&k| i.now[k].seconds).sum();
        assert_eq!(seconds, t.seconds, "topic {}", t.id);
    }
    // A (6000 s) outranks B (3600 s): ids 1-3 are topic 0.
    assert_eq!(a.topic_of, vec![0, 0, 0, 1, 1, 1]);
}

#[test]
fn assignments_carry_novelty_and_oddball_per_block_in_id_order() {
    let mut now = two_groups();
    now.reverse();
    let i = input(now);
    let (r, a) = build_with_assignments(&i);

    // catches: input order leaking through instead of id order.
    assert_eq!(a.block_ids, vec![1, 2, 3, 4, 5, 6]);
    assert_eq!(a.novelty.len(), 6);
    assert_eq!(a.oddball.len(), 6);
    assert_eq!(a.centered.len(), 6);
    // catches: oddball flags that disagree with the report's own oddball list.
    let flagged: Vec<i64> = (0..6)
        .filter(|&k| a.oddball[k])
        .map(|k| a.block_ids[k])
        .collect();
    let mut listed: Vec<i64> = r.oddballs.iter().map(|h| h.block_id).collect();
    listed.sort();
    assert_eq!(flagged, listed);
}

#[test]
fn assignments_are_empty_when_too_few_blocks() {
    // catches: panicking or stale data below the minimum.
    let (_, a) = build_with_assignments(&input(fan(MIN_BLOCKS - 1)));
    assert!(a.block_ids.is_empty() && a.topic_of.is_empty());
}

/// 25 blocks a minute apart: ids 1..=25, all A except block 21 which is B.
fn warmup_set() -> Vec<BlockPoint> {
    (0..25)
        .map(|k| {
            let day = if k < 20 { "2026-10-05" } else { "2026-10-06" };
            let v = if k == 20 { B } else { A };
            pt(k + 1, day, &format!("09:{k:02}"), 600, None, v)
        })
        .collect()
}

#[test]
fn first_times_skip_the_warmup_blocks_but_not_block_21() {
    // catches: no warm-up mask (block 1, novelty 1.0, would lead), `<=` off by one (block 21 lost).
    let r = ready(&input(warmup_set()));
    let ids: Vec<i64> = r.first_times.iter().map(|h| h.block_id).collect();
    assert!(ids.iter().all(|&id| id > 20), "{ids:?}");
    assert_eq!(ids[0], 21);
}

#[test]
fn warmup_blocks_have_zero_assignment_novelty_and_their_day_takes_the_median() {
    // catches: raw novelty leaking to map points; an all-warm day plotting 0 or 1.
    let (r, a) = build_with_assignments(&input(warmup_set()));
    assert!(a.novelty[..20].iter().all(|&v| v == 0.0));
    assert!(a.novelty[20] > 0.5);
    assert_eq!(r.days[0].novelty, r.days[1].novelty);
}
