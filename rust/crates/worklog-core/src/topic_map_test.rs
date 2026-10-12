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

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

#[test]
fn two_runs_are_bit_identical() {
    // catches: RNG or hash-order nondeterminism in init, neighbours or symmetrisation.
    let v = grouped(2, 15);
    let a = layout(&v);
    let b = layout(&v);
    assert_eq!(a.len(), 30);
    for (p, q) in a.iter().zip(&b) {
        assert_eq!(p[0].to_bits(), q[0].to_bits());
        assert_eq!(p[1].to_bits(), q[1].to_bits());
    }
}

#[test]
fn separated_groups_stay_apart() {
    // catches: a layout that ignores P (random/PCA-only output) or inverted forces.
    let xy = layout(&grouped(2, 15));
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
    let xy = layout(&grouped(3, 10));
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
    assert!(layout(&v[..MAP_MIN_POINTS - 1]).is_empty());
    assert_eq!(layout(&v[..MAP_MIN_POINTS]).len(), MAP_MIN_POINTS);
    assert!(layout(&[]).is_empty());
}

#[test]
fn identical_vectors_yield_finite_points() {
    // catches: 0/0 in normalisation or PCA scaling when every point coincides.
    let v = vec![vec![1.0, 2.0, 3.0]; MAP_MIN_POINTS + 2];
    let xy = layout(&v);
    assert_eq!(xy.len(), v.len());
    for p in xy {
        assert!(p[0].is_finite() && p[1].is_finite(), "{p:?}");
        assert!((0.0..=1.0).contains(&p[0]) && (0.0..=1.0).contains(&p[1]));
    }
}

#[test]
#[ignore]
fn bench_n2000() {
    let v = grouped(8, 250);
    let t = std::time::Instant::now();
    let xy = layout(&v);
    println!("layout n=2000 elapsed {:?}", t.elapsed());
    assert_eq!(xy.len(), 2000);
}
