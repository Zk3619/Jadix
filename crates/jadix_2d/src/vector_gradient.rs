//! Explicit vector gradient geometry and immutable exact stop tables.
use crate::{ExtendMode, GpuFillGrad};
use jadix_theme::color_space::linearize_display_srgb_component as linearize;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GradientAlpha {
    /// Premultiply endpoints in the selected interpolation space (no transparent fringe).
    #[default]
    Premultiplied,
    /// Interpolate straight colors in the selected space and alpha independently.
    Straight,
}
/// Interpolation space; output remains linear-sRGB premultiplied LDR.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GradientSpace {
    #[default]
    LinearSrgb,
    Srgb,
    Oklab,
    Oklch,
    Hsl,
}
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HueDirection {
    #[default]
    Shorter,
    Longer,
    Increasing,
    Decreasing,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientError {
    Stops,
    Geometry,
}
impl std::fmt::Display for GradientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid vector gradient: {self:?}")
    }
}
impl std::error::Error for GradientError {}

/// 1..=4096 ordered stops, including equal offsets for exact hard transitions.
/// Offsets are in `[0,1]`, colors are finite straight display-sRGB, clamped to LDR.
/// Clones share prepared linear colors; geometry/extend/alpha do not copy stops.
#[derive(Clone, Debug, PartialEq)]
pub struct GradientStops(Arc<Vec<(f32, [f32; 4], f32)>>);
impl GradientStops {
    /// Extended straight linear-sRGB stops. Negative wide-gamut components and
    /// HDR values are retained; use a floating-point render target to preserve them.
    pub fn new_linear(stops: &[(f32, [f32; 4])]) -> Result<Self, GradientError> {
        if stops.is_empty()
            || stops.len() > 4096
            || stops.iter().any(|(t, c)| {
                !t.is_finite()
                    || !(0. ..=1.).contains(t)
                    || c.iter().any(|v| !v.is_finite() || v.abs() > 16384.)
                    || !(0. ..=1.).contains(&c[3])
            })
            || stops.windows(2).any(|w| w[0].0 > w[1].0)
        {
            return Err(GradientError::Stops);
        }
        Ok(Self(Arc::new(
            stops.iter().map(|(t, c)| (*t, *c, 1.)).collect(),
        )))
    }
    pub fn new(stops: &[(f32, [f32; 4])]) -> Result<Self, GradientError> {
        if stops.is_empty()
            || stops.len() > 4096
            || stops.iter().any(|(t, c)| {
                !t.is_finite() || !(0. ..=1.).contains(t) || c.iter().any(|v| !v.is_finite())
            })
            || stops.windows(2).any(|w| w[0].0 > w[1].0)
        {
            return Err(GradientError::Stops);
        }
        Ok(Self(Arc::new(
            stops
                .iter()
                .map(|(t, c)| {
                    let c = c.map(|v| v.clamp(0., 1.));
                    (
                        *t,
                        [linearize(c[0]), linearize(c[1]), linearize(c[2]), c[3]],
                        1.,
                    )
                })
                .collect(),
        )))
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Per-interval interpolation midpoint, as a fraction strictly between 0
    /// and 1. None is linear. A midpoint maps to exactly half of the color mix.
    /// Stop geometry/alpha interpolation are retained; invalid input is atomic.
    pub fn with_midpoints(mut self, midpoints: &[Option<f32>]) -> Result<Self, GradientError> {
        if midpoints.len() != self.len() - 1
            || midpoints
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || *v <= 0. || *v >= 1.)
        {
            return Err(GradientError::Stops);
        }
        let exponents: Vec<f32> = midpoints
            .iter()
            .map(|v| v.map_or(1., |v| 0.5f32.ln() / v.ln()))
            .collect();
        if exponents.iter().any(|v| !v.is_finite()) {
            return Err(GradientError::Stops);
        }
        for (stop, exponent) in Arc::make_mut(&mut self.0).iter_mut().zip(exponents) {
            stop.2 = exponent;
        }
        Ok(self)
    }
    #[cfg(feature = "gpu")]
    pub(crate) fn identity(&self) -> usize {
        Arc::as_ptr(&self.0) as usize
    }
    #[cfg(feature = "gpu")]
    pub(crate) fn data(&self) -> &[(f32, [f32; 4], f32)] {
        &self.0
    }
    fn sample(
        &self,
        t: f32,
        alpha: GradientAlpha,
        space: GradientSpace,
        hue: HueDirection,
    ) -> [f32; 4] {
        let upper = self.0.partition_point(|s| s.0 <= t);
        let (a, b, f) = if upper == 0 {
            (&self.0[0], &self.0[0], 0.)
        } else if upper == self.len() {
            (&self.0[upper - 1], &self.0[upper - 1], 0.)
        } else {
            let a = &self.0[upper - 1];
            let b = &self.0[upper];
            (a, b, ((t - a.0) / (b.0 - a.0)).powf(a.2))
        };
        if matches!(space, GradientSpace::Oklch | GradientSpace::Hsl) {
            return polar_mix(a.1, b.1, f, alpha, space, hue);
        }
        let prep = |mut c: [f32; 4]| {
            let rgb = to_space([c[0], c[1], c[2]], space);
            c[..3].copy_from_slice(&rgb);
            match alpha {
                GradientAlpha::Premultiplied => [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]],
                GradientAlpha::Straight => c,
            }
        };
        let (a, b) = (prep(a.1), prep(b.1));
        let mut c: [f32; 4] = std::array::from_fn(|i| a[i] + (b[i] - a[i]) * f);
        if space != GradientSpace::LinearSrgb {
            let rgb = if alpha == GradientAlpha::Premultiplied && c[3] > 0. {
                [c[0] / c[3], c[1] / c[3], c[2] / c[3]]
            } else {
                [c[0], c[1], c[2]]
            };
            let rgb = from_space(rgb, space);
            for i in 0..3 {
                c[i] = rgb[i].clamp(0., 1.) * c[3];
            }
        } else if alpha == GradientAlpha::Straight {
            for i in 0..3 {
                c[i] *= c[3];
            }
        }
        c
    }
}

fn to_space([r, g, b]: [f32; 3], space: GradientSpace) -> [f32; 3] {
    match space {
        GradientSpace::LinearSrgb => [r, g, b],
        GradientSpace::Srgb => {
            [r, g, b].map(jadix_theme::color_space::encode_linear_srgb_component)
        }
        GradientSpace::Oklch => {
            let [l, a, b] = to_space([r, g, b], GradientSpace::Oklab);
            [l, a.hypot(b), b.atan2(a).to_degrees().rem_euclid(360.)]
        }
        GradientSpace::Hsl => {
            let [r, g, b] = to_space([r, g, b], GradientSpace::Srgb);
            let max = r.max(g).max(b);
            let min = r.min(g).min(b);
            let d = max - min;
            let l = (max + min) * 0.5;
            if d <= 1e-7 {
                return [l, 0., 0.];
            }
            let hue = if max == r {
                (g - b) / d
            } else if max == g {
                (b - r) / d + 2.
            } else {
                (r - g) / d + 4.
            };
            [
                l,
                d / (1. - (2. * l - 1.).abs()),
                (hue * 60.).rem_euclid(360.),
            ]
        }
        GradientSpace::Oklab => {
            let l = (0.41222146 * r + 0.53633255 * g + 0.051445995 * b).cbrt();
            let m = (0.2119035 * r + 0.6806995 * g + 0.10739696 * b).cbrt();
            let s = (0.08830246 * r + 0.28171885 * g + 0.6299787 * b).cbrt();
            [
                0.21045426 * l + 0.7936178 * m - 0.004072047 * s,
                1.9779985 * l - 2.4285922 * m + 0.4505937 * s,
                0.025904037 * l + 0.78277177 * m - 0.80867577 * s,
            ]
        }
    }
}
fn from_space([l, a, b]: [f32; 3], space: GradientSpace) -> [f32; 3] {
    match space {
        GradientSpace::LinearSrgb => [l, a, b],
        GradientSpace::Srgb => [l, a, b].map(linearize),
        GradientSpace::Oklch => from_space(
            [l, a * b.to_radians().cos(), a * b.to_radians().sin()],
            GradientSpace::Oklab,
        ),
        GradientSpace::Hsl => {
            let c = (1. - (2. * l - 1.).abs()) * a;
            let h = b.rem_euclid(360.) / 60.;
            let x = c * (1. - (h.rem_euclid(2.) - 1.).abs());
            let rgb = if h < 1. {
                [c, x, 0.]
            } else if h < 2. {
                [x, c, 0.]
            } else if h < 3. {
                [0., c, x]
            } else if h < 4. {
                [0., x, c]
            } else if h < 5. {
                [x, 0., c]
            } else {
                [c, 0., x]
            };
            rgb.map(|v| linearize((v + l - c * 0.5).clamp(0., 1.)))
        }
        GradientSpace::Oklab => {
            let ll = (l + 0.39633778 * a + 0.21580376 * b).powi(3);
            let m = (l - 0.105561346 * a - 0.06385417 * b).powi(3);
            let s = (l - 0.08948418 * a - 1.2914855 * b).powi(3);
            [
                4.0767417 * ll - 3.3077116 * m + 0.23096994 * s,
                -1.268438 * ll + 2.6097574 * m - 0.34131938 * s,
                -0.0041960863 * ll - 0.7034186 * m + 1.7076147 * s,
            ]
        }
    }
}

fn polar_mix(
    a: [f32; 4],
    b: [f32; 4],
    f: f32,
    alpha: GradientAlpha,
    space: GradientSpace,
    hue: HueDirection,
) -> [f32; 4] {
    let mut ca = to_space([a[0], a[1], a[2]], space);
    let mut cb = to_space([b[0], b[1], b[2]], space);
    let epsilon = if space == GradientSpace::Oklch {
        4e-6
    } else {
        1e-7
    };
    // Hue is powerless at zero chroma/saturation; borrow the other endpoint's.
    if ca[1] <= epsilon {
        ca[2] = cb[2];
    }
    if cb[1] <= epsilon {
        cb[2] = ca[2];
    }
    let mut delta = cb[2] - ca[2];
    match hue {
        HueDirection::Shorter => {
            if delta > 180. {
                delta -= 360.;
            } else if delta < -180. {
                delta += 360.;
            }
        }
        HueDirection::Longer => {
            if delta > 0. && delta < 180. {
                delta -= 360.;
            } else if delta <= 0. && delta > -180. {
                delta += 360.;
            }
        }
        HueDirection::Increasing => {
            if delta < 0. {
                delta += 360.;
            }
        }
        HueDirection::Decreasing => {
            if delta > 0. {
                delta -= 360.;
            }
        }
    }
    let out_alpha = a[3] + (b[3] - a[3]) * f;
    for i in 0..2 {
        if alpha == GradientAlpha::Premultiplied {
            ca[i] *= a[3];
            cb[i] *= b[3];
        }
        ca[i] += (cb[i] - ca[i]) * f;
        if alpha == GradientAlpha::Premultiplied && out_alpha > 0. {
            ca[i] /= out_alpha;
        }
    }
    ca[2] += delta * f;
    let rgb = from_space(ca, space).map(|v| v.clamp(0., 1.) * out_alpha);
    [rgb[0], rgb[1], rgb[2], out_alpha]
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Geometry {
    Linear {
        start: [f32; 2],
        end: [f32; 2],
    },
    Radial {
        start: [f32; 2],
        r0: f32,
        end: [f32; 2],
        r1: f32,
    },
    Sweep {
        center: [f32; 2],
        start: f32,
        span: f32,
        absolute: bool,
    },
}
/// User-space vector gradient. Radial solves the moving-circle quadratic and
/// chooses the greatest parameter with nonnegative radius; no solution is transparent.
/// Sweep angles are clockwise degrees from +X; one signed turn relative to start.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorGradient {
    geometry: Geometry,
    pub(crate) stops: GradientStops,
    extend: ExtendMode,
    alpha: GradientAlpha,
    space: GradientSpace,
    hue: HueDirection,
}
fn coordinates(v: &[f32]) -> bool {
    v.iter().all(|v| v.is_finite() && v.abs() <= 1e6)
}
impl VectorGradient {
    fn new(geometry: Geometry, stops: GradientStops) -> Self {
        Self {
            geometry,
            stops,
            extend: ExtendMode::Pad,
            alpha: GradientAlpha::Premultiplied,
            space: GradientSpace::LinearSrgb,
            hue: HueDirection::Shorter,
        }
    }
    pub fn linear(
        start: [f32; 2],
        end: [f32; 2],
        stops: GradientStops,
    ) -> Result<Self, GradientError> {
        if !coordinates(&[start[0], start[1], end[0], end[1]]) || start == end {
            return Err(GradientError::Geometry);
        }
        Ok(Self::new(Geometry::Linear { start, end }, stops))
    }
    pub fn radial(
        start: [f32; 2],
        r0: f32,
        end: [f32; 2],
        r1: f32,
        stops: GradientStops,
    ) -> Result<Self, GradientError> {
        if !coordinates(&[start[0], start[1], end[0], end[1], r0, r1])
            || r0 < 0.
            || r1 < 0.
            || (start == end && r0 == r1)
        {
            return Err(GradientError::Geometry);
        }
        Ok(Self::new(Geometry::Radial { start, r0, end, r1 }, stops))
    }
    pub fn sweep(
        center: [f32; 2],
        start_deg: f32,
        end_deg: f32,
        stops: GradientStops,
    ) -> Result<Self, GradientError> {
        let span = end_deg - start_deg;
        if !coordinates(&[center[0], center[1], start_deg, end_deg])
            || span == 0.
            || span.abs() > 360.
        {
            return Err(GradientError::Geometry);
        }
        Ok(Self::new(
            Geometry::Sweep {
                center,
                start: start_deg.rem_euclid(360.).to_radians(),
                span: span.to_radians(),
                absolute: false,
            },
            stops,
        ))
    }
    /// Absolute angular domain with a fixed +X branch cut. Ranges may extend
    /// beyond a turn; angles are not wrapped relative to start.
    pub fn sweep_range(
        center: [f32; 2],
        start_deg: f32,
        end_deg: f32,
        stops: GradientStops,
    ) -> Result<Self, GradientError> {
        if !coordinates(&[center[0], center[1], start_deg, end_deg]) || start_deg == end_deg {
            return Err(GradientError::Geometry);
        }
        Ok(Self::new(
            Geometry::Sweep {
                center,
                start: start_deg.to_radians(),
                span: (end_deg - start_deg).to_radians(),
                absolute: true,
            },
            stops,
        ))
    }
    /// Signed circle anchors preserve the original color domain; only roots
    /// with nonnegative interpolated radius paint.
    pub fn radial_signed(
        start: [f32; 2],
        r0: f32,
        end: [f32; 2],
        r1: f32,
        stops: GradientStops,
    ) -> Result<Self, GradientError> {
        if !coordinates(&[start[0], start[1], end[0], end[1], r0, r1]) || (start == end && r0 == r1)
        {
            return Err(GradientError::Geometry);
        }
        Ok(Self::new(Geometry::Radial { start, r0, end, r1 }, stops))
    }
    pub fn with_extend(mut self, extend: ExtendMode) -> Self {
        self.extend = extend;
        self
    }
    pub fn with_alpha_mode(mut self, alpha: GradientAlpha) -> Self {
        self.alpha = alpha;
        self
    }
    /// Polar-space hue interpolation; rectangular spaces ignore this setting.
    pub fn with_hue_direction(mut self, direction: HueDirection) -> Self {
        self.hue = direction;
        self
    }
    pub fn with_color_space(mut self, space: GradientSpace) -> Self {
        self.space = space;
        self
    }
    pub fn stops(&self) -> &GradientStops {
        &self.stops
    }
    /// Linear-light premultiplied RGBA at a brush-local point.
    pub fn sample(&self, point: [f32; 2]) -> [f32; 4] {
        let Some(mut t) = self.parameter(point) else {
            return [0.; 4];
        };
        match self.extend {
            ExtendMode::Pad => t = t.clamp(0., 1.),
            ExtendMode::Repeat => t = t.rem_euclid(1.),
            ExtendMode::Reflect => {
                let p = t.rem_euclid(2.);
                t = if p <= 1. { p } else { 2. - p };
            }
            ExtendMode::Decal => {
                if !(0. ..=1.).contains(&t) {
                    return [0.; 4];
                }
            }
        }
        self.stops.sample(t, self.alpha, self.space, self.hue)
    }
    fn parameter(&self, p: [f32; 2]) -> Option<f32> {
        if p.iter().any(|v| !v.is_finite()) {
            return None;
        }
        // f64 algebra is the CPU reference. GPU uses normalized f32 coefficients.
        let p = p.map(f64::from);
        let t = match self.geometry {
            Geometry::Linear { start, end } => {
                let d = [
                    f64::from(end[0]) - f64::from(start[0]),
                    f64::from(end[1]) - f64::from(start[1]),
                ];
                ((p[0] - f64::from(start[0])) * d[0] + (p[1] - f64::from(start[1])) * d[1])
                    / (d[0] * d[0] + d[1] * d[1])
            }
            Geometry::Sweep {
                center,
                start,
                span,
                absolute,
            } => {
                let delta = [p[0] - f64::from(center[0]), p[1] - f64::from(center[1])];
                let a = if delta == [0.; 2] {
                    0.
                } else {
                    delta[1].atan2(delta[0])
                };
                if absolute {
                    (a.rem_euclid(std::f64::consts::TAU) - f64::from(start)) / f64::from(span)
                } else if span > 0. {
                    (a - f64::from(start)).rem_euclid(std::f64::consts::TAU) / f64::from(span)
                } else {
                    -(f64::from(start) - a).rem_euclid(std::f64::consts::TAU) / f64::from(span)
                }
            }
            Geometry::Radial { start, r0, end, r1 } => {
                let d = [
                    f64::from(end[0]) - f64::from(start[0]),
                    f64::from(end[1]) - f64::from(start[1]),
                ];
                let q = [p[0] - f64::from(start[0]), p[1] - f64::from(start[1])];
                let (r0, dr) = (f64::from(r0), f64::from(r1) - f64::from(r0));
                let a = d[0] * d[0] + d[1] * d[1] - dr * dr;
                let b = -2. * (q[0] * d[0] + q[1] * d[1] + r0 * dr);
                let c = q[0] * q[0] + q[1] * q[1] - r0 * r0;
                let scale = (d[0] * d[0] + d[1] * d[1] + dr * dr).max(1e-30);
                if a.abs() <= 1e-7 * scale {
                    if b == 0. {
                        return None;
                    }
                    let t = -c / b;
                    if r0 + t * dr < 0. {
                        return None;
                    }
                    t
                } else {
                    let disc = b * b - 4. * a * c;
                    if disc < 0. {
                        return None;
                    }
                    let root = disc.sqrt();
                    let k = -0.5 * (b + if b < 0. { -root } else { root });
                    let roots = if k == 0. {
                        [-b / (2. * a); 2]
                    } else {
                        [k / a, c / k]
                    };
                    roots
                        .into_iter()
                        .filter(|t| t.is_finite() && r0 + t * dr >= 0.)
                        .reduce(f64::max)?
                }
            }
        };
        if t.is_finite() && t.abs() <= f32::MAX as f64 {
            Some(t as f32)
        } else {
            None
        }
    }
    pub(crate) fn payload(&self, inverse: [f32; 6]) -> GpuFillGrad {
        let mut g = GpuFillGrad {
            device_to_brush: inverse,
            kind: 10,
            angle: self.extend as u8 as f32,
            bbox: [0.; 4],
            center: [0.; 2],
            radii: [0.; 2],
            stop_n: self.stops.len() as u32,
            stop_t: [0.; 8],
            stop_c: [[0.; 4]; 8],
        };
        g.stop_t[1] = if self.alpha == GradientAlpha::Straight {
            1.
        } else {
            0.
        };
        g.stop_t[2] = self.space as u8 as f32;
        g.stop_t[4] = self.hue as u8 as f32;
        match self.geometry {
            Geometry::Linear { start, end } => g.bbox = [start[0], start[1], end[0], end[1]],
            Geometry::Radial { start, r0, end, r1 } => {
                g.kind = 11;
                g.bbox = [start[0], start[1], end[0], end[1]];
                g.center = [r0, r1];
            }
            Geometry::Sweep {
                center,
                start,
                span,
                absolute,
            } => {
                g.kind = 12;
                g.bbox = [center[0], center[1], start, span];
                g.stop_t[3] = if absolute { 1. } else { 0. };
            }
        }
        g
    }
}
