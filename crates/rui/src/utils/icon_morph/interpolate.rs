//! Polar interpolation of a morph plan.

use super::plan::MorphPlan;

pub fn alloc_outputs(plan: &MorphPlan) -> Vec<Vec<f64>> {
    plan.items.iter().map(|_| vec![0.0; 2 * plan.n]).collect()
}

pub fn interp_polar(plan: &MorphPlan, t: f64, out: &mut [Vec<f64>]) {
    for (k, it) in plan.items.iter().enumerate() {
        let o = &mut out[k];
        let n = plan.n;
        let s = (it.ln_sigma * t).exp();
        let ang = it.theta * t;
        let cos = ang.cos() * s;
        let sin = ang.sin() * s;
        let (cx, cy) = if let Some(block) = it.block {
            let [ox, oy] = block.off;
            let [dx, dy] = block.drift;
            (
                it.ca[0] + dx * t + (ox * cos - oy * sin - ox),
                it.ca[1] + dy * t + (ox * sin + oy * cos - oy),
            )
        } else {
            (
                it.ca[0] + (it.cb[0] - it.ca[0]) * t,
                it.ca[1] + (it.cb[1] - it.ca[1]) * t,
            )
        };
        for i in 0..n {
            let px = it.a_c[2 * i] + (it.b_t[2 * i] - it.a_c[2 * i]) * t;
            let py = it.a_c[2 * i + 1] + (it.b_t[2 * i + 1] - it.a_c[2 * i + 1]) * t;
            o[2 * i] = cx + px * cos - py * sin;
            o[2 * i + 1] = cy + px * sin + py * cos;
        }
    }
}

/// Raw coordinate lerp (same correspondence, no decomposition).
pub fn interp_linear(plan: &MorphPlan, t: f64, out: &mut [Vec<f64>]) {
    for (k, it) in plan.items.iter().enumerate() {
        let o = &mut out[k];
        let n = plan.n;
        for i in 0..n {
            o[2 * i] = it.a[2 * i] + (it.b_o[2 * i] - it.a[2 * i]) * t;
            o[2 * i + 1] = it.a[2 * i + 1] + (it.b_o[2 * i + 1] - it.a[2 * i + 1]) * t;
        }
    }
}
