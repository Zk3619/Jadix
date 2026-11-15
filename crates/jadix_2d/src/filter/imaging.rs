//! Bounded image effects over the existing isolated group surfaces.
use super::*;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FilterColorSpace {
    #[default]
    LinearSrgb,
    Srgb,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FilterEdge {
    Transparent,
    #[default]
    Duplicate,
    Wrap,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterError {
    Transfer,
    Convolution,
}
impl std::fmt::Display for FilterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid image filter: {self:?}")
    }
}
impl std::error::Error for FilterError {}

/// Functions take straight components in `[0,1]`, and clamp the result to `[0,1]`.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum TransferFunction {
    #[default]
    Identity,
    Linear {
        slope: f32,
        intercept: f32,
    },
    Gamma {
        amplitude: f32,
        exponent: f32,
        offset: f32,
    },
    /// Piecewise linear interpolation, 1..=16 samples including endpoints.
    Table(Arc<[f32]>),
    /// Posterization, 1..=16 equally sized bins (last bin includes 1).
    Discrete(Arc<[f32]>),
}
impl TransferFunction {
    pub fn table(values: &[f32]) -> Result<Self, FilterError> {
        if values.is_empty() || values.len() > 16 {
            return Err(FilterError::Transfer);
        }
        let result = Self::Table(values.into());
        if result.valid() {
            Ok(result)
        } else {
            Err(FilterError::Transfer)
        }
    }
    pub fn discrete(values: &[f32]) -> Result<Self, FilterError> {
        if values.is_empty() || values.len() > 16 {
            return Err(FilterError::Transfer);
        }
        let result = Self::Discrete(values.into());
        if result.valid() {
            Ok(result)
        } else {
            Err(FilterError::Transfer)
        }
    }
    pub(crate) fn valid(&self) -> bool {
        let number = |v: f32| v.is_finite() && v.abs() <= 1e6;
        match self {
            Self::Identity => true,
            Self::Linear { slope, intercept } => number(*slope) && number(*intercept),
            Self::Gamma {
                amplitude,
                exponent,
                offset,
            } => {
                number(*amplitude)
                    && number(*offset)
                    && exponent.is_finite()
                    && (0. ..=128.).contains(exponent)
            }
            Self::Table(t) | Self::Discrete(t) => {
                !t.is_empty() && t.len() <= 16 && t.iter().all(|v| number(*v))
            }
        }
    }
    pub(crate) fn sample(&self, x: f32) -> f32 {
        let x = x.clamp(0., 1.);
        match self {
            Self::Identity => x,
            Self::Linear { slope, intercept } => slope * x + intercept,
            Self::Gamma {
                amplitude,
                exponent,
                offset,
            } => {
                amplitude
                    * if *exponent == 0. {
                        1.
                    } else {
                        x.powf(*exponent)
                    }
                    + offset
            }
            Self::Table(t) => {
                let p = x * (t.len() - 1) as f32;
                let a = (p.floor() as usize).min(t.len() - 1);
                t[a] + (t[(a + 1).min(t.len() - 1)] - t[a]) * (p - a as f32)
            }
            Self::Discrete(t) => t[((x * t.len() as f32) as usize).min(t.len() - 1)],
        }
        .clamp(0., 1.)
    }
    #[cfg(feature = "gpu")]
    pub(crate) fn pack(&self, p: &mut [f32]) {
        match self {
            Self::Identity => {}
            Self::Linear { slope, intercept } => {
                p[0] = 1.;
                p[1] = *slope;
                p[2] = *intercept;
            }
            Self::Gamma {
                amplitude,
                exponent,
                offset,
            } => {
                p[0] = 2.;
                p[1] = *amplitude;
                p[2] = *exponent;
                p[3] = *offset;
            }
            Self::Table(t) | Self::Discrete(t) => {
                p[0] = if matches!(self, Self::Table(_)) {
                    3.
                } else {
                    4.
                };
                p[1] = t.len() as f32;
                p[4..4 + t.len()].copy_from_slice(t);
            }
        }
    }
}

/// A 1..=7 by 1..=7 convolution in device pixels. Kernel is rotated 180 degrees
/// as in SVG feConvolveMatrix. Unit lengths select bilinearly sampled offsets;
/// this is not an intermediate-image rescaling implementation.
#[derive(Clone, Debug, PartialEq)]
pub struct Convolution {
    pub order: [u32; 2],
    pub kernel: Arc<[f32]>,
    pub target: [u32; 2],
    /// Zero uses sum(kernel), or 1 when the sum is zero.
    pub divisor: f32,
    pub bias: f32,
    pub unit_length: [f32; 2],
    pub edge: FilterEdge,
    pub preserve_alpha: bool,
    pub color_space: FilterColorSpace,
}
impl Convolution {
    pub fn new(order: [u32; 2], kernel: &[f32]) -> Result<Self, FilterError> {
        if !order.iter().all(|v| (1..=7).contains(v))
            || kernel.len() != order[0] as usize * order[1] as usize
        {
            return Err(FilterError::Convolution);
        }
        let result = Self {
            order,
            kernel: kernel.into(),
            target: [order[0] / 2, order[1] / 2],
            divisor: 0.,
            bias: 0.,
            unit_length: [1.; 2],
            edge: FilterEdge::Duplicate,
            preserve_alpha: false,
            color_space: FilterColorSpace::LinearSrgb,
        };
        if result.valid() {
            Ok(result)
        } else {
            Err(FilterError::Convolution)
        }
    }
    pub(crate) fn divisor(&self) -> f32 {
        if self.divisor != 0. {
            return self.divisor;
        }
        let sum = self.kernel.iter().map(|v| *v as f64).sum::<f64>() as f32;
        if sum == 0. {
            1.
        } else {
            sum
        }
    }
    pub(crate) fn valid(&self) -> bool {
        self.order.iter().all(|v| (1..=7).contains(v))
            && self.kernel.len() == self.order[0] as usize * self.order[1] as usize
            && self.kernel.iter().all(|v| v.is_finite() && v.abs() <= 1e6)
            && self.target.iter().zip(self.order).all(|(t, n)| *t < n)
            && self.divisor.is_finite()
            && self.divisor.abs() <= 1e6
            && self.divisor().is_finite()
            && self.divisor().abs() >= 1e-6
            && self.bias.is_finite()
            && self.bias.abs() <= 1e6
            && self
                .unit_length
                .iter()
                .all(|v| v.is_finite() && *v > 0. && *v <= 1e6)
    }
}
fn convert_rgb(rgb: &mut [f32], from_linear: bool, to_linear: bool) {
    if from_linear == to_linear {
        return;
    }
    for c in rgb {
        *c = if to_linear {
            linearize(*c)
        } else {
            jadix_theme::color_space::encode_linear_srgb_component(*c)
        };
    }
}
pub(super) fn transfer(
    pixels: &mut [f32],
    channels: &[TransferFunction; 4],
    space: FilterColorSpace,
    linear: bool,
) {
    let domain_linear = space == FilterColorSpace::LinearSrgb;
    for p in pixels.chunks_exact_mut(4) {
        let mut straight = if p[3] > 0. {
            [p[0] / p[3], p[1] / p[3], p[2] / p[3], p[3]]
        } else {
            [0.; 4]
        };
        convert_rgb(&mut straight[..3], linear, domain_linear);
        for i in 0..4 {
            straight[i] = channels[i].sample(straight[i]);
        }
        convert_rgb(&mut straight[..3], domain_linear, linear);
        for i in 0..3 {
            p[i] = straight[i] * straight[3];
        }
        p[3] = straight[3];
    }
}
pub(super) fn convolve(pixels: &mut [f32], size: [u32; 2], c: &Convolution, linear: bool) {
    let mut input = pixels.to_vec();
    let domain_linear = c.color_space == FilterColorSpace::LinearSrgb;
    for p in input.chunks_exact_mut(4) {
        if p[3] > 0. {
            for k in 0..3 {
                p[k] /= p[3];
            }
        } else {
            p[..3].fill(0.);
        }
        convert_rgb(&mut p[..3], linear, domain_linear);
        if !c.preserve_alpha {
            for k in 0..3 {
                p[k] *= p[3];
            }
        }
    }
    let load = |x: i64, y: i64| -> [f32; 4] {
        let map = |v: i64, n: u32| -> Option<u32> {
            match c.edge {
                FilterEdge::Transparent => {
                    if v < 0 || v >= n as i64 {
                        None
                    } else {
                        Some(v as u32)
                    }
                }
                FilterEdge::Duplicate => Some(v.clamp(0, n as i64 - 1) as u32),
                FilterEdge::Wrap => Some(v.rem_euclid(n as i64) as u32),
            }
        };
        match (map(x, size[0]), map(y, size[1])) {
            (Some(x), Some(y)) => input[((y * size[0] + x) * 4) as usize..][..4]
                .try_into()
                .unwrap(),
            _ => [0.; 4],
        }
    };
    for y in 0..size[1] {
        for x in 0..size[0] {
            let mut sum = [0f64; 4];
            for i in 0..c.order[1] {
                for j in 0..c.order[0] {
                    let xx = x as f32 + (j as f32 - c.target[0] as f32) * c.unit_length[0];
                    let yy = y as f32 + (i as f32 - c.target[1] as f32) * c.unit_length[1];
                    let ix = xx.floor() as i64;
                    let iy = yy.floor() as i64;
                    let fx = xx - xx.floor();
                    let fy = yy - yy.floor();
                    let weight =
                        c.kernel[c.kernel.len() - 1 - (i * c.order[0] + j) as usize] as f64;
                    for (dy, wy) in [(0, 1. - fy), (1, fy)] {
                        for (dx, wx) in [(0, 1. - fx), (1, fx)] {
                            let v = load(ix + dx, iy + dy);
                            for k in 0..4 {
                                sum[k] += v[k] as f64 * wx as f64 * wy as f64 * weight;
                            }
                        }
                    }
                }
            }
            let at = ((y * size[0] + x) * 4) as usize;
            let alpha = if c.preserve_alpha {
                pixels[at + 3]
            } else {
                (sum[3] / c.divisor() as f64 + c.bias as f64).clamp(0., 1.) as f32
            };
            let mut rgb = std::array::from_fn::<_, 3, _>(|k| {
                let value = sum[k] / c.divisor() as f64
                    + c.bias as f64 * if c.preserve_alpha { 1. } else { alpha as f64 };
                if c.preserve_alpha {
                    value.clamp(0., 1.) as f32
                } else if alpha > 0. {
                    value.clamp(0., alpha as f64) as f32 / alpha
                } else {
                    0.
                }
            });
            convert_rgb(&mut rgb, domain_linear, linear);
            for k in 0..3 {
                pixels[at + k] = rgb[k] * alpha;
            }
            pixels[at + 3] = alpha;
        }
    }
}
