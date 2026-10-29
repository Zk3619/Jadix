//! Arc-length resampling with anchored corners.

use super::normalize::icon_to_cubics;
use super::types::{morph_err, CubicPath, IconInput, MorphError, Sampled};

/// Default angular threshold for a segment joint to count as a corner.
pub const CORNER_THRESHOLD: f64 = std::f64::consts::PI / 8.0;
pub const SAMPLE_N: usize = 64;

const GX: [f64; 4] = [
    0.18343464249564978,
    0.525532409916329,
    0.7966664774136267,
    0.9602898564975363,
];
const GW: [f64; 4] = [
    0.362683783378362,
    0.31370664587788727,
    0.22238103445337448,
    0.10122853629037626,
];

fn speed(p: &[f64], k: usize, t: f64) -> f64 {
    let i = 6 * k;
    let u = 1.0 - t;
    let c0 = 3.0 * u * u;
    let c1 = 6.0 * u * t;
    let c2 = 3.0 * t * t;
    let dx = c0 * (p[i + 2] - p[i]) + c1 * (p[i + 4] - p[i + 2]) + c2 * (p[i + 6] - p[i + 4]);
    let dy = c0 * (p[i + 3] - p[i + 1]) + c1 * (p[i + 5] - p[i + 3]) + c2 * (p[i + 7] - p[i + 5]);
    dx.hypot(dy)
}

fn seg_len(p: &[f64], k: usize, t1: f64) -> f64 {
    let half = t1 / 2.0;
    let mut s = 0.0;
    for j in 0..4 {
        s += GW[j] * (speed(p, k, half + half * GX[j]) + speed(p, k, half - half * GX[j]));
    }
    s * half
}

fn point(p: &[f64], k: usize, t: f64, out: &mut [f64], o: usize) {
    let i = 6 * k;
    let u = 1.0 - t;
    let b0 = u * u * u;
    let b1 = 3.0 * u * u * t;
    let b2 = 3.0 * u * t * t;
    let b3 = t * t * t;
    out[o] = b0 * p[i] + b1 * p[i + 2] + b2 * p[i + 4] + b3 * p[i + 6];
    out[o + 1] = b0 * p[i + 1] + b1 * p[i + 3] + b2 * p[i + 5] + b3 * p[i + 7];
}

fn tangent(p: &[f64], k: usize, at_end: bool) -> Option<(f64, f64)> {
    let i = 6 * k;
    let b = if at_end { i + 6 } else { i };
    let s = if at_end { -1.0 } else { 1.0 };
    let js: [usize; 3] = if at_end { [4, 2, 0] } else { [2, 4, 6] };
    for j in js {
        let dx = s * (p[i + j] - p[b]);
        let dy = s * (p[i + j + 1] - p[b + 1]);
        if dx * dx + dy * dy > 1e-18 {
            return Some((dx, dy));
        }
    }
    None
}

pub fn detect_corners(path: &CubicPath, threshold: f64) -> Vec<usize> {
    let p = &path.pts;
    let m = path.seg_count();
    let mut active = Vec::new();
    for k in 0..m {
        if seg_len(p, k, 1.0) > 1e-9 {
            active.push(k);
        }
    }
    if active.is_empty() {
        return Vec::new();
    }
    let mut corners = Vec::new();
    let mut test = |a: usize, b: usize| {
        let Some(u) = tangent(p, a, true) else {
            return;
        };
        let Some(v) = tangent(p, b, false) else {
            return;
        };
        let ang = (u.0 * v.1 - u.1 * v.0).atan2(u.0 * v.0 + u.1 * v.1).abs();
        if ang > threshold {
            corners.push(b);
        }
    };
    for j in 0..active.len().saturating_sub(1) {
        test(active[j], active[j + 1]);
    }
    if path.closed && active.len() > 1 {
        test(active[active.len() - 1], active[0]);
    }
    corners.sort_unstable();
    corners.dedup();
    corners
}

fn invert(p: &[f64], k: usize, s: f64, ls: f64) -> f64 {
    if s <= 0.0 {
        return 0.0;
    }
    if s >= ls {
        return 1.0;
    }
    let mut lo = 0.0;
    let mut hi = 1.0;
    let mut t = s / ls;
    for _ in 0..12 {
        let f = seg_len(p, k, t) - s;
        if f.abs() < 1e-10 * ls + 1e-14 {
            break;
        }
        if f > 0.0 {
            hi = t;
        } else {
            lo = t;
        }
        let sp = speed(p, k, t);
        let mut nt = if sp > 1e-12 {
            t - f / sp
        } else {
            (lo + hi) / 2.0
        };
        if !(nt > lo && nt < hi) {
            nt = (lo + hi) / 2.0;
        }
        t = nt;
    }
    t
}

/// Samples a cubic subpath at N points equidistant by arc length.
pub fn resample_path(
    path: &CubicPath,
    n: usize,
    corner_threshold: f64,
) -> Result<Vec<f64>, MorphError> {
    let p = &path.pts;
    let m = path.seg_count();
    let mut out = vec![0.0; 2 * n];
    let fill = |out: &mut [f64]| {
        for i in 0..n {
            out[2 * i] = p[0];
            out[2 * i + 1] = p[1];
        }
    };
    if m < 1 {
        fill(&mut out);
        return Ok(out);
    }
    let mut lens = vec![0.0; m];
    let mut l_tot = 0.0;
    for (segment_index, length) in lens.iter_mut().enumerate() {
        *length = seg_len(p, segment_index, 1.0);
        l_tot += *length;
    }
    if l_tot < 1e-12 {
        fill(&mut out);
        return Ok(out);
    }

    let cs = detect_corners(path, corner_threshold);
    let anchors: Vec<usize> = if path.closed {
        if cs.is_empty() {
            vec![0]
        } else {
            cs
        }
    } else {
        let mut a = vec![0];
        a.extend_from_slice(&cs);
        a.push(m);
        a.sort_unstable();
        a.dedup();
        a
    };
    let mut runs: Vec<(usize, usize)> = Vec::new();
    if path.closed {
        for j in 0..anchors.len() {
            let a = anchors[j];
            let b = if j + 1 < anchors.len() {
                anchors[j + 1]
            } else {
                anchors[0] + m
            };
            runs.push((a, b));
        }
    } else {
        for j in 0..anchors.len().saturating_sub(1) {
            runs.push((anchors[j], anchors[j + 1]));
        }
    }
    let rl: Vec<f64> = runs
        .iter()
        .map(|&(a, b)| {
            let mut s = 0.0;
            for k in a..b {
                s += lens[k % m];
            }
            s
        })
        .collect();
    let intervals = if path.closed { n } else { n - 1 };
    if runs.len() > intervals {
        return Err(morph_err(format!("N={n} too small ({} runs)", runs.len())));
    }
    let total = rl.iter().sum::<f64>().max(1.0);
    let ideal: Vec<f64> = rl.iter().map(|l| (intervals as f64 * l) / total).collect();
    let mut counts: Vec<usize> = ideal
        .iter()
        .map(|q| 1usize.max(q.floor() as usize))
        .collect();
    let mut r = intervals as i32 - counts.iter().sum::<usize>() as i32;
    if r > 0 {
        let mut order: Vec<(i64, usize)> = ideal
            .iter()
            .enumerate()
            .map(|(idx, q)| (((q - q.floor()) * 1e9).round() as i64, idx))
            .collect();
        order.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let n_counts = counts.len();
        for j in 0..r as usize {
            counts[order[j % n_counts].1] += 1;
        }
    }
    while r < 0 {
        let mut bi = 0;
        for idx in 1..counts.len() {
            if counts[idx] > counts[bi] {
                bi = idx;
            }
        }
        if counts[bi] <= 1 {
            break;
        }
        counts[bi] -= 1;
        r += 1;
    }

    let mut w = 0usize;
    for (r_i, &(k0, k1)) in runs.iter().enumerate() {
        let cnt = counts[r_i];
        let lr = rl[r_i];
        let vi = 6 * (k0 % m);
        out[2 * w] = p[vi];
        out[2 * w + 1] = p[vi + 1];
        w += 1;
        let mut seg = k0;
        let mut acc = 0.0;
        for j in 1..cnt {
            let target = (lr * j as f64) / cnt as f64;
            while seg < k1 - 1 && acc + lens[seg % m] < target {
                acc += lens[seg % m];
                seg += 1;
            }
            let k = seg % m;
            let ls = lens[k];
            let t = if ls > 1e-12 {
                invert(p, k, target - acc, ls)
            } else {
                0.0
            };
            point(p, k, t, &mut out, 2 * w);
            w += 1;
        }
    }
    if !path.closed {
        let vi = 6 * m;
        out[2 * w] = p[vi];
        out[2 * w + 1] = p[vi + 1];
    }
    Ok(out)
}

/// Full input pipeline: icon → cubics → sampled subpaths.
pub fn resample_icon(input: &IconInput, n: usize) -> Result<Vec<Sampled>, MorphError> {
    let mut out = Vec::new();
    for path in icon_to_cubics(input)? {
        out.push(Sampled {
            pts: resample_path(&path, n, CORNER_THRESHOLD)?,
            closed: path.closed,
        });
    }
    Ok(out)
}
