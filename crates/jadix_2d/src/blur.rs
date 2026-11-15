//! Gaussian convolution of rounded rectangles, in local coordinates.
use crate::{filled_rect, PathCommand};

#[derive(Clone, Copy)]
pub(crate) struct BlurredRect {
    pub rect: [f32; 4],
    pub radius: f32,
    pub sigma: f32,
    pub color: [f32; 4],
    pub corners: [f32; 4],
    pub inset: bool,
}
impl BlurredRect {
    pub fn new(rect: [f32; 4], radius: f32, sigma: f32, color: [f32; 4]) -> Option<Self> {
        if rect.iter().chain(color.iter()).any(|v| !v.is_finite())
            || !radius.is_finite()
            || !sigma.is_finite()
            || sigma < 0.
            || rect[2] <= 0.
            || rect[3] <= 0.
        {
            return None;
        }
        let blur = Self {
            rect,
            radius: radius.clamp(0., rect[2].min(rect[3]) * 0.5),
            sigma: if sigma < 0.001 { 0. } else { sigma },
            color: color.map(|v| v.clamp(0., 1.)),
            corners: [radius.clamp(0., rect[2].min(rect[3]) * 0.5); 4],
            inset: false,
        };
        let b = blur.bounds();
        if b.iter().any(|v| !v.is_finite())
            || !(b[0] + b[2]).is_finite()
            || !(b[1] + b[3]).is_finite()
        {
            return None;
        }
        Some(blur)
    }
    /// Circular corner radii in top-left, top-right, bottom-right, bottom-left
    /// order. A common scale enforces all adjacent sums, like CSS border-radius.
    pub fn shadow(
        rect: [f32; 4],
        corners: [f32; 4],
        sigma: f32,
        color: [f32; 4],
        inset: bool,
    ) -> Option<Self> {
        if corners.iter().any(|v| !v.is_finite()) {
            return None;
        }
        let mut b = Self::new(rect, 0., sigma, color)?;
        if inset && b.sigma == 0. {
            return None;
        }
        let mut r = corners.map(|v| v.max(0.) as f64);
        let scale = [
            rect[2] as f64 / (r[0] + r[1]),
            rect[2] as f64 / (r[3] + r[2]),
            rect[3] as f64 / (r[0] + r[3]),
            rect[3] as f64 / (r[1] + r[2]),
        ]
        .into_iter()
        .fold(1f64, f64::min);
        for v in &mut r {
            *v *= scale;
        }
        b.corners = r.map(|v| v as f32);
        b.radius = b.corners[0];
        b.inset = inset;
        Some(b)
    }
    pub fn extended(&self) -> bool {
        self.inset || self.corners != [self.radius; 4]
    }
    pub fn bounds(&self) -> [f32; 4] {
        let [x, y, w, h] = self.rect;
        let m = if self.inset { 0. } else { 4. * self.sigma };
        [x - m, y - m, w + 2. * m, h + 2. * m]
    }
    pub fn path(&self) -> Vec<PathCommand> {
        let [x, y, w, h] = self.rect;
        if self.sigma > 0. && !self.inset {
            let b = self.bounds();
            return filled_rect(b[0], b[1], b[2], b[3]);
        }
        let [tl, tr, br, bl] = self.corners;
        if self.corners == [0.; 4] {
            return filled_rect(x, y, w, h);
        }
        vec![
            PathCommand::MoveTo((x + tl, y)),
            PathCommand::LineTo((x + w - tr, y)),
            PathCommand::Arc {
                radii: (tr, tr),
                x_rot_deg: 0.,
                large: false,
                sweep: true,
                to: (x + w, y + tr),
            },
            PathCommand::LineTo((x + w, y + h - br)),
            PathCommand::Arc {
                radii: (br, br),
                x_rot_deg: 0.,
                large: false,
                sweep: true,
                to: (x + w - br, y + h),
            },
            PathCommand::LineTo((x + bl, y + h)),
            PathCommand::Arc {
                radii: (bl, bl),
                x_rot_deg: 0.,
                large: false,
                sweep: true,
                to: (x, y + h - bl),
            },
            PathCommand::LineTo((x, y + tl)),
            PathCommand::Arc {
                radii: (tl, tl),
                x_rot_deg: 0.,
                large: false,
                sweep: true,
                to: (x + tl, y),
            },
            PathCommand::Close,
        ]
    }
    pub fn alpha(&self, point: (f32, f32)) -> f32 {
        if self.extended() {
            return self.shadow_alpha(point);
        }
        let [x, y, w, h] = self.rect;
        let r = self.radius;
        let s = self.sigma;
        let (px, py) = (point.0 - x, point.1 - y);
        let horizontal = |left: f32, right: f32| cdf((right - px) / s) - cdf((left - px) / s);
        if r == 0. {
            return horizontal(0., w) * (cdf((h - py) / s) - cdf(-py / s));
        }
        let center = horizontal(0., w) * (cdf((h - r - py) / s) - cdf((r - py) / s));
        let cap = |lo: f32, hi: f32, cy: f32| {
            let lo = lo.max(py - 4. * s);
            let hi = hi.min(py + 4. * s);
            if hi <= lo {
                return 0.;
            }
            let mid = lo + (hi - lo) * 0.5;
            let half = (hi - lo) * 0.5;
            let mut sum = 0.;
            for i in 0..12 {
                let yy = mid + half * NODES[i];
                let d = (yy - cy) / r;
                let inset = r * (1. - (1. - d * d).max(0.).sqrt());
                let q = (yy - py) / s;
                sum += WEIGHTS[i] * (-0.5 * q * q).exp() / s
                    * 0.3989423
                    * horizontal(inset, w - inset);
            }
            half * sum
        };
        (center + cap(0., r, r) + cap(h - r, h, h - r)).clamp(0., 1.)
    }
    fn shadow_alpha(&self, point: (f32, f32)) -> f32 {
        let [x, y, w, h] = self.rect;
        let (px, py) = (point.0 - x, point.1 - y);
        if self.inset && !self.contains((px, py)) {
            return 0.;
        }
        let s = self.sigma;
        let mut a = (cdf((w - px) / s) - cdf(-px / s)) * (cdf((h - py) / s) - cdf(-py / s));
        // Subtract the four disjoint corner cutouts from an analytic rectangle.
        for (i, r) in self.corners.into_iter().enumerate() {
            if r == 0. {
                continue;
            }
            let right = i == 1 || i == 2;
            let bottom = i >= 2;
            let (p, q) = (
                if right { w - px } else { px },
                if bottom { h - py } else { py },
            );
            let lo = 0f32.max(q - 4. * s);
            let hi = r.min(q + 4. * s);
            if hi <= lo {
                continue;
            }
            let half = (hi - lo) * 0.5;
            let mid = lo + half;
            for k in 0..12 {
                let yy = mid + half * NODES[k];
                let d = (yy - r) / r;
                let cut = r * (1. - (1. - d * d).max(0.).sqrt());
                let z = (yy - q) / s;
                a -= half * WEIGHTS[k] * (-0.5 * z * z).exp() / s
                    * 0.3989423
                    * (cdf((cut - p) / s) - cdf(-p / s));
            }
        }
        if self.inset {
            (1. - a).clamp(0., 1.)
        } else {
            a.clamp(0., 1.)
        }
    }
    fn contains(&self, (x, y): (f32, f32)) -> bool {
        let [_, _, w, h] = self.rect;
        if x < 0. || y < 0. || x > w || y > h {
            return false;
        }
        for (i, r) in self.corners.into_iter().enumerate() {
            let xx = if i == 1 || i == 2 { w - x } else { x };
            let yy = if i >= 2 { h - y } else { y };
            if r > 0. && xx < r && yy < r && ((xx - r) / r).powi(2) + ((yy - r) / r).powi(2) > 1. {
                return false;
            }
        }
        true
    }
}
// 12-point Gauss–Legendre integration of the two curved caps. The middle
// rectangle is analytic. This is a convolution, not an SDF falloff approximation.
const NODES: [f32; 12] = [
    -0.98156065,
    -0.9041173,
    -0.76990265,
    -0.58731896,
    -0.3678315,
    -0.12523341,
    0.12523341,
    0.3678315,
    0.58731896,
    0.76990265,
    0.9041173,
    0.98156065,
];
const WEIGHTS: [f32; 12] = [
    0.047175337,
    0.10693932,
    0.16007833,
    0.20316742,
    0.23349254,
    0.24914704,
    0.24914704,
    0.23349254,
    0.20316742,
    0.16007833,
    0.10693932,
    0.047175337,
];
pub(crate) fn cdf(x: f32) -> f32 {
    let x = x.clamp(-12., 12.) * std::f32::consts::FRAC_1_SQRT_2;
    let t = 1. / (1. + 0.3275911 * x.abs());
    let poly =
        (((((1.0614054 * t - 1.4531521) * t) + 1.4214138) * t - 0.28449672) * t + 0.2548296) * t;
    0.5 * (1. + x.signum() * (1. - poly * (-x * x).exp()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_corners_and_inset_match_midpoint_convolution() {
        let mut worst = 0f64;
        for radii in [[0., 8., 3., 5.], [14., 2., 0., 6.], [6., 6., 6., 6.]] {
            for s in [0.4, 2., 8.] {
                let b = BlurredRect::shadow([0., 0., 20., 12.], radii, s, [1.; 4], false).unwrap();
                let r = b.corners.map(f64::from);
                for (px, py) in [
                    (0., 0.),
                    (2., 2.),
                    (10., 6.),
                    (18., 2.),
                    (20., 12.),
                    (-2., 5.),
                ] {
                    // Independently test circle membership, integrate in f64.
                    let inside = |x: f64, y: f64| -> bool {
                        if !(0. ..=20.).contains(&x) || !(0. ..=12.).contains(&y) {
                            return false;
                        }
                        let centers = [
                            (r[0], r[0]),
                            (20. - r[1], r[1]),
                            (20. - r[2], 12. - r[2]),
                            (r[3], 12. - r[3]),
                        ];
                        for i in 0..4 {
                            let xx = if i == 1 || i == 2 { 20. - x } else { x };
                            let yy = if i >= 2 { 12. - y } else { y };
                            if xx < r[i]
                                && yy < r[i]
                                && (x - centers[i].0).hypot(y - centers[i].1) > r[i]
                            {
                                return false;
                            }
                        }
                        true
                    };
                    let s64 = f64::from(s);
                    let n = 360;
                    let (lo_x, hi_x) = (0f64.max(px - 5. * s64), 20f64.min(px + 5. * s64));
                    let (lo_y, hi_y) = (0f64.max(py - 5. * s64), 12f64.min(py + 5. * s64));
                    let (dx, dy) = (
                        (hi_x - lo_x).max(0.) / n as f64,
                        (hi_y - lo_y).max(0.) / n as f64,
                    );
                    let mut reference = 0.;
                    for y in 0..n {
                        for x in 0..n {
                            let (xx, yy) =
                                (lo_x + (x as f64 + 0.5) * dx, lo_y + (y as f64 + 0.5) * dy);
                            if inside(xx, yy) {
                                reference += (-((xx - px).powi(2) + (yy - py).powi(2))
                                    / (2. * s64 * s64))
                                    .exp()
                                    * dx
                                    * dy
                                    / (2. * std::f64::consts::PI * s64 * s64);
                            }
                        }
                    }
                    for inset in [false, true] {
                        let mut test = b;
                        test.inset = inset;
                        let expected = if inset {
                            if inside(px, py) {
                                1. - reference
                            } else {
                                0.
                            }
                        } else {
                            reference
                        };
                        let error = (test.alpha((px as f32, py as f32)) as f64 - expected).abs();
                        worst = worst.max(error);
                        assert!(error < 0.003, "{radii:?}/{s}/{inset} at {px}/{py}: {error}");
                    }
                }
            }
        }
        eprintln!("independent four-corner/inset Gaussian max alpha error: {worst}");
    }
    #[test]
    fn rounded_gaussian_matches_independent_midpoint_convolution() {
        // Independent f64 2D midpoint integral over the rounded shape.
        // No call to the production CDF or Gauss-Legendre kernel.
        let mut worst = 0f64;
        for (w, h, r, s) in [
            (20., 12., 0., 2.),
            (20., 12., 6., 2.),
            (10., 10., 5., 0.5),
            (10., 4., 2., 12.),
            (20., 12., 3., 0.1),
        ] {
            let b = BlurredRect::new([0., 0., w as f32, h as f32], r as f32, s as f32, [1.; 4])
                .unwrap();
            for (x, y) in [
                (-s, 0.),
                (0., 0.),
                (r, r),
                (w * 0.5, h * 0.5),
                (w - 0.3, h * 0.25),
                (w + s, h + s),
            ] {
                let n = 320;
                let x0 = 0f64.max(x - 5. * s);
                let x1 = w.min(x + 5. * s);
                let y0 = 0f64.max(y - 5. * s);
                let y1 = h.min(y + 5. * s);
                let dx = (x1 - x0).max(0.) / n as f64;
                let dy = (y1 - y0).max(0.) / n as f64;
                let mut reference = 0.;
                for iy in 0..n {
                    let yy = y0 + (iy as f64 + 0.5) * dy;
                    let cy = yy.clamp(r, h - r);
                    for ix in 0..n {
                        let xx = x0 + (ix as f64 + 0.5) * dx;
                        let cx = xx.clamp(r, w - r);
                        if (xx - cx).powi(2) + (yy - cy).powi(2) <= r * r {
                            reference += (-((xx - x).powi(2) + (yy - y).powi(2)) / (2. * s * s))
                                .exp()
                                * dx
                                * dy
                                / (2. * std::f64::consts::PI * s * s);
                        }
                    }
                }
                let error = (b.alpha((x as f32, y as f32)) as f64 - reference).abs();
                worst = worst.max(error);
                assert!(error < 0.003, "{w}/{h}/{r}/{s} at {x}/{y}: {error}");
            }
        }
        eprintln!("independent Gaussian convolution max alpha error: {worst}");
    }
}
