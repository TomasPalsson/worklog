use super::*;
use crate::topics_contract::{TopicsStatus, MAX_TOPICS, MIN_BLOCKS};

const A: [f32; 2] = [1.0, 0.0];
const B: [f32; 2] = [0.0, 1.0];

fn pt(
    id: i64,
    day: &str,
    hm: &str,
    seconds: i64,
    ticket: Option<&str>,
    desc: Option<&str>,
    v: [f32; 2],
) -> BlockPoint {
    BlockPoint {
        id,
        day: day.to_string(),
        started_at: format!("{day}T{hm}:00+00:00"),
        seconds,
        ticket: ticket.map(str::to_string),
        description: desc.map(str::to_string),
        vec: v.to_vec(),
    }
}

fn input(now: Vec<BlockPoint>) -> TopicsInput {
    TopicsInput {
        from: "2026-10-05".into(),
        to: "2026-10-11".into(),
        now,
        before: vec![],
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
                Some("x"),
                [t.cos(), t.sin()],
            )
        })
        .collect()
}

/// Three A blocks (day 06) then three B blocks (day 07).
fn two_groups() -> Vec<BlockPoint> {
    vec![
        pt(
            1,
            "2026-10-06",
            "09:00",
            3600,
            Some("A-1"),
            Some("api work"),
            A,
        ),
        pt(
            2,
            "2026-10-06",
            "10:00",
            1800,
            Some("A-1"),
            Some("api more"),
            [0.99, 0.01],
        ),
        pt(
            3,
            "2026-10-06",
            "11:00",
            600,
            Some("A-2"),
            Some("api small"),
            [0.98, 0.02],
        ),
        pt(
            4,
            "2026-10-07",
            "09:00",
            900,
            Some("B-1"),
            Some("fix ci"),
            B,
        ),
        pt(
            5,
            "2026-10-07",
            "10:00",
            2400,
            None,
            Some("deploy pipeline"),
            [0.01, 0.99],
        ),
        pt(
            6,
            "2026-10-07",
            "11:00",
            300,
            None,
            Some("tiny"),
            [0.02, 0.98],
        ),
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
fn topic_name_is_top_ticket_summary_else_longest_block_description() {
    // catches: naming by the first ticket seen, ignoring seconds, ignoring a
    // missing summary, naming by the first block instead of the longest.
    let mut i = input(two_groups());
    i.ticket_summaries
        .insert("A-1".into(), "API rewrite".into());
    i.ticket_summaries
        .insert("A-2".into(), "Small thing".into());
    let r = ready(&i);
    assert_eq!(r.topics[0].name, "API rewrite");
    assert_eq!(r.topics[0].name_ticket.as_deref(), Some("A-1"));
    // B-1 is the top ticket of topic 1 but has no summary -> longest block's description.
    assert_eq!(r.topics[1].name, "deploy pipeline");
    assert_eq!(r.topics[1].name_ticket, None);
}

#[test]
fn topic_top_tickets_ranked_by_seconds() {
    // catches: ranking by block count.
    let r = ready(&input(two_groups()));
    let got: Vec<_> = r.topics[0]
        .top_tickets
        .iter()
        .map(|t| (t.label.as_str(), t.value))
        .collect();
    assert_eq!(got, vec![("A-1", 5400), ("A-2", 600)]);
}

#[test]
fn switches_count_topic_changes_in_started_order_and_longest_run_is_wall_clock() {
    // catches: using input order, counting blocks instead of changes, a run
    // measured as summed seconds, days not oldest first.
    let now = vec![
        pt(3, "2026-10-06", "11:00", 600, None, Some("a"), A),
        pt(1, "2026-10-06", "09:00", 1800, None, Some("a"), A),
        pt(2, "2026-10-06", "10:00", 1800, None, Some("a"), A),
        pt(4, "2026-10-06", "12:00", 600, None, Some("b"), B),
        pt(5, "2026-10-06", "13:00", 600, None, Some("a"), A),
        pt(6, "2026-10-05", "09:00", 600, None, Some("b"), B),
    ];
    let r = ready(&input(now));
    assert_eq!(r.switches.len(), 2);
    assert_eq!(r.switches[0].day, "2026-10-05");
    assert_eq!(r.switches[0].switches, 0);
    let d = &r.switches[1];
    assert_eq!(d.day, "2026-10-06");
    assert_eq!(d.switches, 2); // a a a | b | a
    assert_eq!(d.work_seconds, 1800 + 1800 + 600 + 600 + 600);
    // 09:00 .. 11:10 = 7800s wall clock; summed seconds would be 4200.
    assert_eq!(d.longest_run_seconds, 7800);
}

#[test]
fn drift_assigns_before_blocks_to_nearest_topic() {
    // catches: assigning by index, losing seconds_now.
    let mut i = input(two_groups());
    i.before = vec![
        pt(101, "2026-09-29", "09:00", 100, None, None, [0.97, 0.03]),
        pt(102, "2026-09-29", "10:00", 200, None, None, [0.9, 0.1]),
        pt(103, "2026-09-30", "09:00", 50, None, None, [0.2, 0.8]),
    ];
    let r = ready(&i);
    assert_eq!(r.drift.len(), r.topics.len());
    let d0 = &r.drift[0];
    assert_eq!(
        (d0.topic_id, d0.seconds_now, d0.seconds_before),
        (0, 6000, 300)
    );
    let d1 = &r.drift[1];
    assert_eq!(
        (d1.topic_id, d1.seconds_now, d1.seconds_before),
        (1, 3600, 50)
    );
    assert_eq!(d0.name, r.topics[0].name);
}

#[test]
fn drift_is_zero_when_there_is_no_previous_range() {
    // catches: panic or non-zero on empty `before`.
    let r = ready(&input(two_groups()));
    assert!(r.drift.iter().all(|d| d.seconds_before == 0));
}

/// (ticket, count, vec for even blocks, vec for odd blocks); ids start at `base`.
fn tickets(base: i64, spec: &[(&str, usize, [f32; 2], [f32; 2])]) -> Vec<BlockPoint> {
    let mut id = base;
    let mut out = vec![];
    for (t, n, even, odd) in spec {
        for k in 0..*n {
            id += 1;
            let v = if k % 2 == 0 { *even } else { *odd };
            out.push(pt(id, "2026-10-06", "09:00", 60, Some(t), Some("x"), v));
        }
    }
    out
}

#[test]
fn sprawl_needs_min_blocks_and_ranks_widest_first() {
    // catches: `>` for `>=` on SPRAWL_MIN_BLOCKS, ranking narrowest first,
    // a spread that is not 1 - mean pairwise cosine.
    let mid = [std::f32::consts::FRAC_1_SQRT_2; 2];
    let now = tickets(
        0,
        &[
            ("SAME-1", 3, A, A),
            ("WIDE-1", 3, A, B),
            ("TWO-1", 2, A, B),
            ("MID-1", 3, A, mid),
        ],
    );
    let mut i = input(now);
    i.ticket_summaries
        .insert("WIDE-1".into(), "Wide one".into());
    let r = ready(&i);
    let order: Vec<_> = r.sprawl.iter().map(|s| s.ticket.as_str()).collect();
    assert_eq!(order, vec!["WIDE-1", "MID-1", "SAME-1"]); // TWO-1: 2 blocks, excluded
                                                          // WIDE: A,B,A -> pair sims 0,1,0 -> mean 1/3 -> spread 2/3.
    assert!((r.sprawl[0].spread - 2.0 / 3.0).abs() < 1e-6);
    assert_eq!(r.sprawl[0].blocks, 3);
    assert_eq!(r.sprawl[0].summary.as_deref(), Some("Wide one"));
    assert_eq!(r.sprawl[2].summary, None);
    // MID: A,mid,A -> sims 0.7071,1,0.7071 -> mean 0.80474 -> spread 0.19526.
    assert!((r.sprawl[1].spread - 0.19526).abs() < 1e-4);
    assert!(r.sprawl[2].spread.abs() < 1e-6);
}

#[test]
fn sprawl_is_capped_at_top_three() {
    // catches: no truncation to SPRAWL_TOP.
    let now = tickets(
        0,
        &[
            ("T-1", 3, A, B),
            ("T-2", 3, A, B),
            ("T-3", 3, A, B),
            ("T-4", 3, A, B),
        ],
    );
    assert_eq!(ready(&input(now)).sprawl.len(), 3);
}

fn history_days() -> Vec<BlockPoint> {
    vec![
        pt(201, "2026-10-01", "09:00", 3600, Some("A-1"), None, A),
        pt(202, "2026-10-02", "09:00", 3600, Some("B-1"), None, B),
        pt(
            203,
            "2026-10-03",
            "09:00",
            3600,
            Some("A-9"),
            None,
            [0.9, 0.1],
        ),
    ]
}

fn today_like_a() -> Vec<BlockPoint> {
    let mut now = two_groups();
    now.push(pt(
        7,
        "2026-10-11",
        "09:00",
        3600,
        Some("A-1"),
        Some("api"),
        A,
    ));
    now
}

#[test]
fn similar_days_rank_twin_first_and_exclude_the_target_day() {
    // catches: ranking worst first, comparing the target to itself.
    let mut i = input(today_like_a());
    i.history = history_days();
    let r = ready(&i);
    let days: Vec<_> = r.similar_days.iter().map(|d| d.day.as_str()).collect();
    assert_eq!(days.len(), 3);
    assert!(!days.contains(&"2026-10-11"));
    // 10-01 is exactly A; 10-06 is A-leaning; 10-03 is near A; 10-02 and 10-07 lean B.
    assert_eq!(days[0], "2026-10-01");
    assert!((r.similar_days[0].similarity - 1.0).abs() < 1e-6);
    assert_eq!(r.similar_days[0].work_seconds, 3600);
    assert_eq!(r.similar_days[0].top_tickets[0].label, "A-1");
    assert!(r
        .similar_days
        .windows(2)
        .all(|w| w[0].similarity >= w[1].similarity));
    assert!(!days.contains(&"2026-10-02"));
}

#[test]
fn similar_days_fall_back_to_last_day_in_now_when_to_has_no_blocks() {
    // catches: returning nothing when `to` is empty, picking the first day.
    let mut i = input(two_groups()); // last day in range is 2026-10-07 (B blocks)
    i.history = history_days();
    let r = ready(&i);
    assert_eq!(r.similar_days[0].day, "2026-10-02"); // the pure-B history day
}

#[test]
fn similar_days_weight_by_hours_not_block_count() {
    // catches: an unweighted mean of block vectors.
    // Day 01: one long A + three 10 s B. Hours-weighted it is ~A (similarity ~1);
    // unweighted it would be (1,3) -> 0.32.
    let mut now = two_groups();
    now.push(pt(7, "2026-10-11", "09:00", 3600, None, None, A));
    let mut i = input(now);
    i.history = vec![
        pt(301, "2026-10-01", "09:00", 3600, None, None, A),
        pt(302, "2026-10-01", "10:00", 10, None, None, B),
        pt(303, "2026-10-01", "11:00", 10, None, None, B),
        pt(304, "2026-10-01", "12:00", 10, None, None, B),
    ];
    let r = ready(&i);
    let day1 = r
        .similar_days
        .iter()
        .find(|x| x.day == "2026-10-01")
        .unwrap();
    assert!(day1.similarity > 0.99);
}

#[test]
fn similar_days_capped_at_top_three() {
    // catches: no truncation to SIMILAR_DAYS_TOP.
    let mut i = input(today_like_a());
    i.history = (1..=5)
        .map(|d| {
            pt(
                400 + d,
                &format!("2026-09-{d:02}"),
                "09:00",
                600,
                None,
                None,
                A,
            )
        })
        .collect();
    assert_eq!(ready(&i).similar_days.len(), 3);
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

fn tie_group(first_id: i64, v: [f32; 2], name: &str) -> Vec<BlockPoint> {
    (0..2)
        .map(|i| {
            let n = format!("{name}{i}");
            pt(first_id + i, "2026-10-06", "09:00", 60, None, Some(&n), v)
        })
        .collect()
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
    assert_eq!(r.topics[0].name, "a0"); // A+M merged, not M+B
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
    assert_eq!(ready(&input(pts.clone())).topics[0].name, "a0");
    for p in &mut pts {
        p.id = 100 - p.id; // B groups now hold the lowest ids
    }
    assert!(ready(&input(pts)).topics[0].name.starts_with('b'));
}

#[test]
fn drift_tie_goes_to_the_first_topic() {
    // catches: `>=` letting the last equally-near centroid win.
    let mut i = input(two_groups());
    i.before = vec![pt(900, "2026-09-29", "09:00", 500, None, None, M)];
    let r = ready(&i);
    assert_eq!(r.drift[0].seconds_before, 500);
    assert_eq!(r.drift[1].seconds_before, 0);
}

#[test]
fn similar_day_ties_order_by_earlier_day_first() {
    // catches: tie order on equal similarity flipping to latest-day-first.
    let mut i = input(today_like_a());
    i.history = ["2026-09-02", "2026-09-01"]
        .iter()
        .enumerate()
        .map(|(n, d)| pt(500 + n as i64, d, "09:00", 600, None, None, A))
        .collect();
    let days: Vec<_> = ready(&i)
        .similar_days
        .into_iter()
        .map(|d| d.day)
        .filter(|d| d.starts_with("2026-09"))
        .collect();
    assert_eq!(days, ["2026-09-01", "2026-09-02"]);
}
