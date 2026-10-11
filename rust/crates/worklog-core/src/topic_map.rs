//! Exact t-SNE layout of centered block vectors for the semantic map.

use crate::topic_metrics::cosine;
use crate::topics_contract::MAP_MIN_POINTS;

const MAX_PERPLEXITY: f64 = 30.0;
const ITERS: usize = 750;
const EXAGGERATION_ITERS: usize = 250;
const EXAGGERATION: f64 = 12.0;
const INIT_STD: f64 = 1e-4;
const POWER_ITERS: usize = 100;

/// Sparse symmetrised affinities: `(i, j, p_ij)` sorted by `(i, j)`, both directions present.
fn affinities(v: &[Vec<f32>], perplexity: f64) -> Vec<(usize, usize, f64)> {
    let n = v.len();
    let k = ((3.0 * perplexity).ceil() as usize).min(n - 1);
    let target = perplexity.ln();
    let mut edges = Vec::with_capacity(2 * n * k);
    for i in 0..n {
        let mut near: Vec<(usize, f64)> = (0..n)
            .filter(|&j| j != i)
            .map(|j| (j, 1.0 - cosine(&v[i], &v[j])))
            .collect();
        near.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        near.truncate(k);
        // Shift by the nearest distance so exp() cannot underflow for every neighbour.
        let d0 = near[0].1;
        let weights = |beta: f64| -> Vec<f64> {
            near.iter()
                .map(|&(_, d)| (-beta * (d - d0)).exp())
                .collect()
        };
        let entropy = |beta: f64| -> f64 {
            let w = weights(beta);
            let z: f64 = w.iter().sum();
            let dw: f64 = w.iter().zip(&near).map(|(w, &(_, d))| w * (d - d0)).sum();
            z.ln() + beta * dw / z
        };
        let (mut lo, mut hi, mut beta) = (0.0_f64, f64::INFINITY, 1.0_f64);
        for _ in 0..64 {
            let h = entropy(beta);
            if (h - target).abs() < 1e-5 {
                break;
            }
            if h > target {
                lo = beta;
                beta = if hi.is_infinite() {
                    beta * 2.0
                } else {
                    (beta + hi) / 2.0
                };
            } else {
                hi = beta;
                beta = (beta + lo) / 2.0;
            }
        }
        let w = weights(beta);
        let z: f64 = w.iter().sum();
        for (&(j, _), w) in near.iter().zip(&w) {
            let p = w / z / (2.0 * n as f64);
            edges.push((i, j, p));
            edges.push((j, i, p));
        }
    }
    edges.sort_by_key(|a| (a.0, a.1));
    let mut out: Vec<(usize, usize, f64)> = Vec::with_capacity(edges.len());
    for e in edges {
        match out.last_mut() {
            Some(l) if (l.0, l.1) == (e.0, e.1) => l.2 += e.2,
            _ => out.push(e),
        }
    }
    out
}

/// Top-2 principal components by power iteration from a fixed start vector, scaled so the
/// first axis has std `INIT_STD`.
fn pca_init(v: &[Vec<f32>]) -> Vec<[f64; 2]> {
    let n = v.len();
    let d = v[0].len();
    let mut means = vec![0.0; d];
    for row in v {
        for (m, &x) in means.iter_mut().zip(row) {
            *m += x as f64 / n as f64;
        }
    }
    let x: Vec<Vec<f64>> = v
        .iter()
        .map(|r| r.iter().zip(&means).map(|(&a, m)| a as f64 - m).collect())
        .collect();
    let dot = |a: &[f64], b: &[f64]| -> f64 { a.iter().zip(b).map(|(a, b)| a * b).sum() };
    let mut comps: Vec<Vec<f64>> = Vec::new();
    for c in 0..2 {
        let mut u: Vec<f64> = (0..d)
            .map(|k| 1.0 + ((k * 7 + 3 + c * 5) % 11) as f64)
            .collect();
        for _ in 0..POWER_ITERS {
            for prev in &comps {
                let p = dot(&u, prev);
                u.iter_mut().zip(prev).for_each(|(a, b)| *a -= p * b);
            }
            let mut next = vec![0.0; d];
            for row in &x {
                let s = dot(row, &u);
                next.iter_mut().zip(row).for_each(|(a, b)| *a += s * b);
            }
            let norm = dot(&next, &next).sqrt();
            if norm < 1e-12 {
                break;
            }
            u = next.iter().map(|a| a / norm).collect();
        }
        comps.push(u);
    }
    let mut y: Vec<[f64; 2]> = x
        .iter()
        .map(|r| [dot(r, &comps[0]), dot(r, &comps[1])])
        .collect();
    let var = y.iter().map(|p| p[0] * p[0]).sum::<f64>() / n as f64;
    let scale = if var > 1e-24 {
        INIT_STD / var.sqrt()
    } else {
        0.0
    };
    y.iter_mut().for_each(|p| *p = [p[0] * scale, p[1] * scale]);
    y
}

fn normalise(y: &[[f64; 2]]) -> Vec<[f32; 2]> {
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in y {
        for a in 0..2 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
    }
    let span = (hi[0] - lo[0]).max(hi[1] - lo[1]);
    let at = |p: &[f64; 2], a: usize| -> f32 {
        if span <= 0.0 {
            return 0.5;
        }
        let pad = (span - (hi[a] - lo[a])) / 2.0;
        (((p[a] - lo[a]) + pad) / span).clamp(0.0, 1.0) as f32
    };
    y.iter().map(|p| [at(p, 0), at(p, 1)]).collect()
}

/// 2-D t-SNE coordinates in [0,1]², empty below `MAP_MIN_POINTS`.
// ponytail: O(n²) per iteration, ~seconds at n=2000; Barnes-Hut if ranges >3k blocks get slow
pub fn layout(v: &[Vec<f32>]) -> Vec<[f32; 2]> {
    let n = v.len();
    if n < MAP_MIN_POINTS {
        return Vec::new();
    }
    let perplexity = MAX_PERPLEXITY.min((n - 1) as f64 / 3.0);
    let p = affinities(v, perplexity);
    let mut y = pca_init(v);
    let lr = (n as f64 / 12.0).max(50.0);
    let mut vel = vec![[0.0; 2]; n];
    let mut gains = vec![[1.0_f64; 2]; n];
    let mut rep = vec![[0.0; 2]; n];
    for it in 0..ITERS {
        let exag = if it < EXAGGERATION_ITERS {
            EXAGGERATION
        } else {
            1.0
        };
        let momentum = if it < EXAGGERATION_ITERS { 0.5 } else { 0.8 };
        rep.iter_mut().for_each(|r| *r = [0.0; 2]);
        let mut z = 0.0;
        for i in 0..n {
            for j in (i + 1)..n {
                let (dx, dy) = (y[i][0] - y[j][0], y[i][1] - y[j][1]);
                let w = 1.0 / (1.0 + dx * dx + dy * dy);
                z += 2.0 * w;
                let w2 = w * w;
                rep[i][0] += w2 * dx;
                rep[i][1] += w2 * dy;
                rep[j][0] -= w2 * dx;
                rep[j][1] -= w2 * dy;
            }
        }
        let mut grad: Vec<[f64; 2]> = rep.iter().map(|r| [-r[0] / z, -r[1] / z]).collect();
        for &(i, j, pij) in &p {
            let (dx, dy) = (y[i][0] - y[j][0], y[i][1] - y[j][1]);
            let w = 1.0 / (1.0 + dx * dx + dy * dy);
            grad[i][0] += exag * pij * w * dx;
            grad[i][1] += exag * pij * w * dy;
        }
        for i in 0..n {
            for a in 0..2 {
                let g = 4.0 * grad[i][a];
                gains[i][a] = if (g > 0.0) == (vel[i][a] > 0.0) {
                    (gains[i][a] * 0.8).max(0.01)
                } else {
                    gains[i][a] + 0.2
                };
                vel[i][a] = momentum * vel[i][a] - lr * gains[i][a] * g;
                y[i][a] += vel[i][a];
            }
        }
    }
    normalise(&y)
}

#[cfg(test)]
#[path = "topic_map_test.rs"]
mod tests;
