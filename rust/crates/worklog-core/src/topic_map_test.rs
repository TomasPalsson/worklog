use super::*;
use crate::topics_contract::MAP_MIN_POINTS;

fn lcg(state: &mut u64) -> f32 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    ((*state >> 33) as f32 / (1u64 << 31) as f32) - 0.5
}

/// `groups` clusters of `per` vectors in 8 dims, each around its own axis.
fn grouped(groups: usize, per: usize) -> Vec<Vec<f32>> {
    let mut s = 7u64;
    let mut out = Vec::new();
    for g in 0..groups {
        for _ in 0..per {
            let mut v: Vec<f32> = (0..8).map(|_| lcg(&mut s) * 0.2).collect();
            v[g] += 1.0;
            out.push(v);
        }
    }
    out
}

fn blob_labels(groups: usize, per: usize) -> Vec<usize> {
    (0..groups * per).map(|i| i / per).collect()
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

#[test]
fn two_runs_are_bit_identical() {
    // catches: RNG or hash-order nondeterminism in init, neighbours or symmetrisation.
    let v = grouped(2, 15);
    let a = layout(&v, &blob_labels(2, 15));
    let b = layout(&v, &blob_labels(2, 15));
    assert_eq!(a.len(), 30);
    for (p, q) in a.iter().zip(&b) {
        assert_eq!(p[0].to_bits(), q[0].to_bits());
        assert_eq!(p[1].to_bits(), q[1].to_bits());
    }
}

#[test]
fn separated_groups_stay_apart() {
    // catches: a layout that ignores P (random/PCA-only output) or inverted forces.
    let xy = layout(&grouped(2, 15), &blob_labels(2, 15));
    let (mut intra, mut ni, mut inter, mut nx) = (0.0, 0, 0.0, 0);
    for i in 0..30 {
        for j in (i + 1)..30 {
            if (i < 15) == (j < 15) {
                intra += dist(xy[i], xy[j]);
                ni += 1;
            } else {
                inter += dist(xy[i], xy[j]);
                nx += 1;
            }
        }
    }
    let (intra, inter) = (intra / ni as f32, inter / nx as f32);
    assert!(intra < 0.3 * inter, "intra {intra} inter {inter}");
}

#[test]
fn coords_in_unit_square_with_aspect_preserved() {
    // catches: normalising each axis independently (stretch), or not centring the shorter axis.
    let xy = layout(&grouped(3, 10), &blob_labels(3, 10));
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for p in &xy {
        for a in 0..2 {
            assert!(p[a].is_finite() && (0.0..=1.0).contains(&p[a]), "{p:?}");
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
    }
    let ext = [hi[0] - lo[0], hi[1] - lo[1]];
    let long = if ext[0] >= ext[1] { 0 } else { 1 };
    let short = 1 - long;
    assert!(
        (ext[long] - 1.0).abs() < 1e-5,
        "long axis spans {}",
        ext[long]
    );
    assert!(
        ext[short] < 1.0 - 1e-4,
        "both axes span 1: independent stretch"
    );
    assert!(
        ((lo[short] + hi[short]) / 2.0 - 0.5).abs() < 1e-5,
        "shorter axis not centred"
    );
}

#[test]
fn below_min_points_is_empty_and_at_min_is_not() {
    // catches: > vs >= on MAP_MIN_POINTS in either direction.
    let v = grouped(2, MAP_MIN_POINTS);
    let l = blob_labels(2, MAP_MIN_POINTS);
    assert!(layout(&v[..MAP_MIN_POINTS - 1], &l[..MAP_MIN_POINTS - 1]).is_empty());
    assert_eq!(
        layout(&v[..MAP_MIN_POINTS], &l[..MAP_MIN_POINTS]).len(),
        MAP_MIN_POINTS
    );
    assert!(layout(&[], &[]).is_empty());
    assert!(layout3(&v[..MAP_MIN_POINTS - 1], &l[..MAP_MIN_POINTS - 1]).is_empty());
    assert_eq!(
        layout3(&v[..MAP_MIN_POINTS], &l[..MAP_MIN_POINTS]).len(),
        MAP_MIN_POINTS
    );
}

#[test]
fn identical_vectors_yield_finite_points() {
    // catches: 0/0 in normalisation or PCA scaling when every point coincides.
    let v = vec![vec![1.0, 2.0, 3.0]; MAP_MIN_POINTS + 2];
    let xy = layout(&v, &vec![0; v.len()]);
    assert_eq!(xy.len(), v.len());
    for p in xy {
        assert!(p[0].is_finite() && p[1].is_finite(), "{p:?}");
        assert!((0.0..=1.0).contains(&p[0]) && (0.0..=1.0).contains(&p[1]));
    }
}

const TOPICS: usize = 6;
const SUB: usize = 12;

/// Each topic is two orthogonal sub-blobs; the labels are the only thing joining them.
fn split_fixture() -> (Vec<Vec<f32>>, Vec<usize>) {
    let mut s = 11u64;
    let (mut v, mut labels) = (Vec::new(), Vec::new());
    for blob in 0..2 * TOPICS {
        for _ in 0..SUB {
            let mut p: Vec<f32> = (0..2 * TOPICS).map(|_| lcg(&mut s) * 0.2).collect();
            p[blob] += 1.0;
            v.push(p);
            labels.push(blob % TOPICS);
        }
    }
    (v, labels)
}

fn d2<const D: usize>(a: &[f32; D], b: &[f32; D]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

/// Single-link components of `pts` when points closer than `gap` are joined.
fn components<const D: usize>(pts: &[[f32; D]], gap: f32) -> usize {
    let mut comp: Vec<usize> = (0..pts.len()).collect();
    for i in 0..pts.len() {
        for j in 0..i {
            if d2(&pts[i], &pts[j]) <= gap {
                let (from, to) = (comp[i], comp[j]);
                comp.iter_mut()
                    .filter(|c| **c == from)
                    .for_each(|c| *c = to);
            }
        }
    }
    comp.sort_unstable();
    comp.dedup();
    comp.len()
}

fn assert_topics_whole<const D: usize>(xy: &[[f32; D]], labels: &[usize]) {
    for t in 0..TOPICS {
        let mine: Vec<[f32; D]> = (0..xy.len())
            .filter(|&i| labels[i] == t)
            .map(|i| xy[i])
            .collect();
        // catches: gravity absent or wrong-signed, leaving a topic as two far-apart islands.
        assert_eq!(components(&mine, 0.15), 1, "topic {t} split in {D}-D");
    }
    let same = (0..xy.len())
        .filter(|&i| {
            let nearest = (0..xy.len())
                .filter(|&j| j != i)
                .min_by(|&a, &b| d2(&xy[i], &xy[a]).total_cmp(&d2(&xy[i], &xy[b])))
                .unwrap();
            labels[nearest] == labels[i]
        })
        .count();
    // catches: gravity so strong that topics overlap, or too weak to hold them.
    assert!(
        same as f32 >= 0.95 * xy.len() as f32,
        "{same}/{} nearest-same",
        xy.len()
    );
}

#[test]
fn split_topics_stay_whole_in_2d_and_3d() {
    let (v, labels) = split_fixture();
    assert_topics_whole(&layout(&v, &labels), &labels);
    assert_topics_whole(&layout3(&v, &labels), &labels);
}

fn separation<const D: usize>(xy: &[[f32; D]], labels: &[usize]) -> (f32, f32) {
    let mut cen = [[0.0_f32; D]; TOPICS];
    let per = xy.len() as f32 / TOPICS as f32;
    for (p, &t) in xy.iter().zip(labels) {
        (0..D).for_each(|a| cen[t][a] += p[a] / per);
    }
    let within = xy
        .iter()
        .zip(labels)
        .map(|(p, &t)| d2(p, &cen[t]))
        .sum::<f32>()
        / xy.len() as f32;
    let mut between = 0.0;
    let mut n = 0.0;
    for i in 0..TOPICS {
        for j in 0..i {
            between += d2(&cen[i], &cen[j]);
            n += 1.0;
        }
    }
    (between / n, within)
}

#[test]
fn natural_blobs_still_separate_by_topic() {
    // catches: gravity collapsing everything, or label use that ignores vector structure.
    let v = grouped(6, 20);
    let l = blob_labels(6, 20);
    let (b, w) = separation(&layout(&v, &l), &l);
    assert!(b > 2.0 * w, "2-D between {b} within {w}");
    let (b, w) = separation(&layout3(&v, &l), &l);
    assert!(b > 2.0 * w, "3-D between {b} within {w}");
}

#[test]
fn natural_topics_keep_visible_spread() {
    // catches: gravity so strong each topic collapses to a point (unpickable stars).
    let v = grouped(6, 20);
    let l = blob_labels(6, 20);
    let (_, w) = separation(&layout(&v, &l), &l);
    assert!(w > 0.005, "2-D within {w}");
    let (_, w) = separation(&layout3(&v, &l), &l);
    assert!(w > 0.005, "3-D within {w}");
}

#[test]
fn z_axis_is_not_flat() {
    // catches: a 3-D layout whose third component stays at the PCA init plane.
    let (v, labels) = split_fixture();
    let z: Vec<f32> = layout3(&v, &labels).iter().map(|p| p[2]).collect();
    let mean = z.iter().sum::<f32>() / z.len() as f32;
    let std = (z.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / z.len() as f32).sqrt();
    assert!(std > 0.05, "z std {std}");
}

#[test]
fn deterministic_and_unit_cube_in_3d() {
    // catches: nondeterminism in the 3-D path; out-of-range or NaN coordinates.
    let (v, labels) = split_fixture();
    let a = layout3(&v, &labels);
    let b = layout3(&v, &labels);
    assert_eq!(a.len(), v.len());
    for (p, q) in a.iter().zip(&b) {
        for k in 0..3 {
            assert_eq!(p[k].to_bits(), q[k].to_bits());
            assert!(p[k].is_finite() && (0.0..=1.0).contains(&p[k]), "{p:?}");
        }
    }
}

#[test]
fn identical_vectors_finite_in_3d() {
    // catches: 0/0 in the generic normalisation when every point coincides.
    let v = vec![vec![1.0, 2.0, 3.0]; MAP_MIN_POINTS + 2];
    for p in layout3(&v, &vec![0; v.len()]) {
        assert!(
            p.iter().all(|x| x.is_finite() && (0.0..=1.0).contains(x)),
            "{p:?}"
        );
    }
}

#[test]
#[ignore]
fn bench_n2000() {
    let v = grouped(8, 250);
    let l = blob_labels(8, 250);
    let t = std::time::Instant::now();
    let xy = layout(&v, &l);
    println!("layout n=2000 elapsed {:?}", t.elapsed());
    assert_eq!(xy.len(), 2000);
    let t = std::time::Instant::now();
    let xyz = layout3(&v, &l);
    println!("layout3 n=2000 elapsed {:?}", t.elapsed());
    assert_eq!(xyz.len(), 2000);
}
