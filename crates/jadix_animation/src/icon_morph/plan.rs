//! Correspondence and 2D Procrustes alignment.

use super::types::{morph_err, MorphError, Sampled};

const LEN_WEIGHT: f64 = 0.35;
const LAMBDA: f64 = 0.05;
const GLOBAL_EPS: f64 = 5e-3;
const PERM_MAX: usize = 8;
const SURJ_MAX: f64 = 1e5;

#[derive(Clone, Copy, Debug)]
pub struct Similarity {
    pub theta: f64,
    pub sigma: f64,
    pub res: f64,
}

#[derive(Clone, Debug)]
pub struct PlanItem {
    pub a: Vec<f64>,
    pub a_c: Vec<f64>,
    pub b_t: Vec<f64>,
    pub b_o: Vec<f64>,
    pub ca: [f64; 2],
    pub cb: [f64; 2],
    pub theta: f64,
    pub ln_sigma: f64,
    pub res: f64,
    pub closed: bool,
    pub block: Option<BlockTransport>,
}

#[derive(Clone, Copy, Debug)]
pub struct BlockTransport {
    pub off: [f64; 2],
    pub drift: [f64; 2],
}

#[derive(Clone, Debug)]
pub struct MorphPlan {
    pub items: Vec<PlanItem>,
    pub n: usize,
}

pub fn centroid(p: &[f64]) -> [f64; 2] {
    let n = p.len() / 2;
    let mut cx = 0.0;
    let mut cy = 0.0;
    for i in 0..n {
        cx += p[2 * i];
        cy += p[2 * i + 1];
    }
    [cx / n as f64, cy / n as f64]
}

pub fn poly_len(p: &[f64]) -> f64 {
    let n = p.len() / 2;
    let mut l = 0.0;
    for i in 1..n {
        l += (p[2 * i] - p[2 * i - 2]).hypot(p[2 * i + 1] - p[2 * i - 1]);
    }
    l
}

fn reverse_pts(p: &[f64]) -> Vec<f64> {
    let n = p.len() / 2;
    let mut out = vec![0.0; 2 * n];
    for i in 0..n {
        out[2 * i] = p[2 * (n - 1 - i)];
        out[2 * i + 1] = p[2 * (n - 1 - i) + 1];
    }
    out
}

fn rotate_pts(p: &[f64], off: usize) -> Vec<f64> {
    let n = p.len() / 2;
    let mut out = vec![0.0; 2 * n];
    for i in 0..n {
        let j = (i + off) % n;
        out[2 * i] = p[2 * j];
        out[2 * i + 1] = p[2 * j + 1];
    }
    out
}

pub fn procrustes(a: &[f64], b: &[f64], ca: [f64; 2], cb: [f64; 2]) -> Similarity {
    let n = a.len() / 2;
    let mut sxx = 0.0;
    let mut sxy = 0.0;
    let mut syx = 0.0;
    let mut syy = 0.0;
    let mut na = 0.0;
    let mut nb = 0.0;
    for i in 0..n {
        let ax = a[2 * i] - ca[0];
        let ay = a[2 * i + 1] - ca[1];
        let bx = b[2 * i] - cb[0];
        let by = b[2 * i + 1] - cb[1];
        sxx += ax * bx;
        syy += ay * by;
        sxy += ax * by;
        syx += ay * bx;
        na += ax * ax + ay * ay;
        nb += bx * bx + by * by;
    }
    let theta = (sxy - syx).atan2(sxx + syy);
    let num = theta.cos() * (sxx + syy) + theta.sin() * (sxy - syx);
    let mut sigma = if na > 1e-12 { num / na } else { 1.0 };
    if sigma.partial_cmp(&1e-6) != Some(std::cmp::Ordering::Greater) {
        sigma = 1e-6;
    }
    let res2 = (sigma * sigma * na - 2.0 * sigma * num + nb).max(0.0);
    let res = if nb > 1e-12 { (res2 / nb).sqrt() } else { 0.0 };
    Similarity { theta, sigma, res }
}

struct Alignment {
    ca: [f64; 2],
    cb: [f64; 2],
    a: Vec<f64>,
    b: Vec<f64>,
    sim: Similarity,
}

fn align_pair(a_pts: &[f64], b_pts: &[f64], a_closed: bool, b_closed: bool) -> Alignment {
    let ca = centroid(a_pts);
    let cb = centroid(b_pts);
    let vary_a = a_closed && !b_closed;
    let base: &[f64] = if vary_a { a_pts } else { b_pts };
    let offs = if a_closed || b_closed {
        base.len() / 2
    } else {
        1
    };
    let mut best_score = f64::INFINITY;
    let mut best = base.to_vec();
    let mut sim = Similarity {
        theta: 0.0,
        sigma: 1.0,
        res: 0.0,
    };
    for dir in 0..2 {
        let walk = if dir == 1 {
            reverse_pts(base)
        } else {
            base.to_vec()
        };
        for off in 0..offs {
            let cand = if off > 0 {
                rotate_pts(&walk, off)
            } else {
                walk.clone()
            };
            let s = if vary_a {
                procrustes(&cand, b_pts, ca, cb)
            } else {
                procrustes(a_pts, &cand, ca, cb)
            };
            let score = s.res + (LAMBDA * s.theta.abs()) / std::f64::consts::PI;
            if score < best_score {
                best_score = score;
                best = cand;
                sim = s;
            }
        }
    }
    if vary_a {
        Alignment {
            ca,
            cb,
            a: best,
            b: b_pts.to_vec(),
            sim,
        }
    } else {
        Alignment {
            ca,
            cb,
            a: a_pts.to_vec(),
            b: best,
            sim,
        }
    }
}

fn cost_matrix(a: &[Vec<f64>], b: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let cbs: Vec<[f64; 2]> = b.iter().map(|p| centroid(p)).collect();
    let lbs: Vec<f64> = b.iter().map(|p| poly_len(p)).collect();
    a.iter()
        .map(|pa| {
            let ca = centroid(pa);
            let la = poly_len(pa);
            cbs.iter()
                .enumerate()
                .map(|(j, cb)| {
                    (ca[0] - cb[0]).hypot(ca[1] - cb[1]) + LEN_WEIGHT * (la - lbs[j]).abs()
                })
                .collect()
        })
        .collect()
}

fn best_permutation(c: &[Vec<f64>]) -> Vec<usize> {
    let n = c.len();
    if n > PERM_MAX {
        let mut pairs = Vec::new();
        for (source_index, row) in c.iter().enumerate() {
            for (target_index, &cost) in row.iter().enumerate().take(n) {
                pairs.push((cost, source_index, target_index));
            }
        }
        pairs.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut out = vec![usize::MAX; n];
        let mut used = vec![false; n];
        for (_, i, j) in pairs {
            if out[i] == usize::MAX && !used[j] {
                out[i] = j;
                used[j] = true;
            }
        }
        return out;
    }
    let mut idx: Vec<usize> = (0..n).collect();
    let mut best = idx.clone();
    let mut bc = f64::INFINITY;
    fn perm(
        arr: &mut [usize],
        k: usize,
        acc: f64,
        n: usize,
        c: &[Vec<f64>],
        best: &mut Vec<usize>,
        bc: &mut f64,
    ) {
        if acc >= *bc {
            return;
        }
        if k == n {
            *bc = acc;
            best.copy_from_slice(arr);
            return;
        }
        for i in k..n {
            arr.swap(k, i);
            perm(arr, k + 1, acc + c[k][arr[k]], n, c, best, bc);
            arr.swap(k, i);
        }
    }
    perm(&mut idx, 0, 0.0, n, c, &mut best, &mut bc);
    best
}

fn best_surjection(c: &[Vec<f64>]) -> Result<Vec<usize>, MorphError> {
    let b = c.len();
    let s = c[0].len();
    if (s as f64).powi(b as i32) > SURJ_MAX {
        let mut f: Vec<usize> = c
            .iter()
            .map(|row| {
                let mut m = 0;
                for j in 1..row.len() {
                    if row[j] < row[m] {
                        m = j;
                    }
                }
                m
            })
            .collect();
        let mut mult = vec![0usize; s];
        for &si in &f {
            mult[si] += 1;
        }
        for si in 0..s {
            if mult[si] > 0 {
                continue;
            }
            let mut bi = usize::MAX;
            let mut bc = f64::INFINITY;
            for i in 0..b {
                if mult[f[i]] < 2 {
                    continue;
                }
                let extra = c[i][si] - c[i][f[i]];
                if extra < bc {
                    bc = extra;
                    bi = i;
                }
            }
            if bi != usize::MAX {
                mult[f[bi]] -= 1;
                f[bi] = si;
                mult[si] += 1;
            }
        }
        return Ok(f);
    }
    let mut best: Option<Vec<usize>> = None;
    let mut bc = f64::INFINITY;
    let mut f = vec![0usize; b];
    let mut mult = vec![0usize; s];
    #[expect(
        clippy::too_many_arguments,
        reason = "The bounded surjection search passes its recursion state without allocating per step"
    )]
    fn rec(
        i: usize,
        acc: f64,
        covered: usize,
        b: usize,
        s: usize,
        c: &[Vec<f64>],
        f: &mut [usize],
        mult: &mut [usize],
        best: &mut Option<Vec<usize>>,
        bc: &mut f64,
    ) {
        if acc >= *bc || s - covered > b - i {
            return;
        }
        if i == b {
            *bc = acc;
            *best = Some(f.to_vec());
            return;
        }
        for si in 0..s {
            f[i] = si;
            mult[si] += 1;
            rec(
                i + 1,
                acc + c[i][si],
                covered + if mult[si] == 1 { 1 } else { 0 },
                b,
                s,
                c,
                f,
                mult,
                best,
                bc,
            );
            mult[si] -= 1;
        }
    }
    rec(0, 0.0, 0, b, s, c, &mut f, &mut mult, &mut best, &mut bc);
    best.ok_or_else(|| morph_err("no valid surjection (B < S)"))
}

fn apply_global(items: &mut [PlanItem], n: usize) {
    let t = items.len() * n;
    let mut ga = vec![0.0; 2 * t];
    let mut gb = vec![0.0; 2 * t];
    for (k, it) in items.iter().enumerate() {
        let off = 2 * n * k;
        ga[off..off + 2 * n].copy_from_slice(&it.a);
        gb[off..off + 2 * n].copy_from_slice(&it.b_o);
    }
    let gca = centroid(&ga);
    let g = procrustes(&ga, &gb, gca, centroid(&gb));
    if g.res >= GLOBAL_EPS {
        return;
    }
    let cos = (-g.theta).cos();
    let sin = (-g.theta).sin();
    let rc = g.theta.cos();
    let rs = g.theta.sin();
    for it in items.iter_mut() {
        let mut e2 = 0.0;
        let mut nb = 0.0;
        for i in 0..n {
            let bx = it.b_o[2 * i] - it.cb[0];
            let by = it.b_o[2 * i + 1] - it.cb[1];
            it.b_t[2 * i] = (bx * cos - by * sin) / g.sigma;
            it.b_t[2 * i + 1] = (bx * sin + by * cos) / g.sigma;
            let ex = g.sigma * (rc * it.a_c[2 * i] - rs * it.a_c[2 * i + 1]) - bx;
            let ey = g.sigma * (rs * it.a_c[2 * i] + rc * it.a_c[2 * i + 1]) - by;
            e2 += ex * ex + ey * ey;
            nb += bx * bx + by * by;
        }
        it.theta = g.theta;
        it.ln_sigma = g.sigma.ln();
        it.res = if nb > 1e-12 { (e2 / nb).sqrt() } else { 0.0 };
        let s1 = it.ln_sigma.exp();
        let c1 = it.theta.cos() * s1;
        let n1 = it.theta.sin() * s1;
        let ox = it.ca[0] - gca[0];
        let oy = it.ca[1] - gca[1];
        let rx = ox * c1 - oy * n1 - ox;
        let ry = ox * n1 + oy * c1 - oy;
        it.block = Some(BlockTransport {
            off: [ox, oy],
            drift: [it.cb[0] - it.ca[0] - rx, it.cb[1] - it.ca[1] - ry],
        });
    }
}

/// Builds the morph plan between two lists of sampled subpaths.
pub fn build_plan(src: &[Sampled], dst: &[Sampled]) -> Result<MorphPlan, MorphError> {
    let p = src.len();
    let q = dst.len();
    if p == 0 || q == 0 {
        return Err(morph_err("icon has no subpaths"));
    }
    let a: Vec<Vec<f64>> = src.iter().map(|s| s.pts.clone()).collect();
    let b: Vec<Vec<f64>> = dst.iter().map(|s| s.pts.clone()).collect();
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    if p == q {
        let perm = best_permutation(&cost_matrix(&a, &b));
        for (source_index, target_index) in perm.into_iter().enumerate() {
            pairs.push((source_index, target_index));
        }
    } else if p < q {
        let f = best_surjection(&cost_matrix(&b, &a))?;
        for (target_index, source_index) in f.into_iter().enumerate() {
            pairs.push((source_index, target_index));
        }
    } else {
        let f = best_surjection(&cost_matrix(&a, &b))?;
        for (source_index, target_index) in f.into_iter().enumerate() {
            pairs.push((source_index, target_index));
        }
    }
    let n = a[0].len() / 2;
    let mut items: Vec<PlanItem> = pairs
        .into_iter()
        .map(|(si, di)| {
            let al = align_pair(&a[si], &b[di], src[si].closed, dst[di].closed);
            let mut a_c = vec![0.0; 2 * n];
            let mut b_t = vec![0.0; 2 * n];
            let mut b_o = vec![0.0; 2 * n];
            let cos = (-al.sim.theta).cos();
            let sin = (-al.sim.theta).sin();
            for i in 0..n {
                a_c[2 * i] = al.a[2 * i] - al.ca[0];
                a_c[2 * i + 1] = al.a[2 * i + 1] - al.ca[1];
                let bx = al.b[2 * i] - al.cb[0];
                let by = al.b[2 * i + 1] - al.cb[1];
                b_t[2 * i] = (bx * cos - by * sin) / al.sim.sigma;
                b_t[2 * i + 1] = (bx * sin + by * cos) / al.sim.sigma;
                b_o[2 * i] = al.b[2 * i];
                b_o[2 * i + 1] = al.b[2 * i + 1];
            }
            PlanItem {
                a: al.a,
                a_c,
                b_t,
                b_o,
                ca: al.ca,
                cb: al.cb,
                theta: al.sim.theta,
                ln_sigma: al.sim.sigma.ln(),
                res: al.sim.res,
                closed: src[si].closed && dst[di].closed,
                block: None,
            }
        })
        .collect();
    if items.len() > 1 {
        apply_global(&mut items, n);
    }
    Ok(MorphPlan { items, n })
}
