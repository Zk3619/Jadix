//! Ordered isolated-group effects, evaluated in device pixels before composition.
mod imaging;
pub use imaging::{Convolution, FilterColorSpace, FilterEdge, FilterError, TransferFunction};

use jadix_theme::color_space::linearize_display_srgb_component as linearize;

/// Filters operate on premultiplied pixels in the canvas working color space.
/// Gaussian samples outside the canvas are transparent. Matrix rows transform
/// straight RGBA, with the fifth column a bias; results clamp to LDR.
#[derive(Clone, Debug, PartialEq)]
pub enum LayerFilter {
    ComponentTransfer {
        channels: [TransferFunction; 4],
        color_space: FilterColorSpace,
    },
    ConvolveMatrix(Convolution),
    /// Fractional translation, premultiplied bilinear sampling, transparent outside.
    Offset {
        offset: [f32; 2],
    },
    /// Rectangular per-channel morphology on premultiplied RGBA; integer pixel radii.
    Morphology {
        radius: [u32; 2],
        operator: Morphology,
    },
    /// Shadow without the original content, useful for independent shadow mixing.
    DropShadowOnly {
        sigma: [f32; 2],
        offset: [f32; 2],
        color: [f32; 4],
    },
    Gaussian {
        sigma: [f32; 2],
    },
    /// Original content over a colored, blurred, fractionally translated alpha.
    /// Color is straight display-sRGB. Offset and sigma are device pixels.
    DropShadow {
        sigma: [f32; 2],
        offset: [f32; 2],
        color: [f32; 4],
    },
    /// Coefficients must be finite with magnitude at most 1e6, so portable f32
    /// evaluation cannot overflow. Invalid filters are ignored as a whole.
    ColorMatrix([f32; 20]),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Morphology {
    Dilate,
    Erode,
}

impl LayerFilter {
    pub(crate) fn valid(&self) -> bool {
        match self {
            Self::ComponentTransfer { channels, .. } => {
                channels.iter().all(TransferFunction::valid)
            }
            Self::ConvolveMatrix(c) => c.valid(),
            Self::Offset { offset } => offset.iter().all(|v| v.is_finite()),
            Self::Morphology { .. } => true,
            Self::Gaussian { sigma } => valid_sigma(*sigma),
            Self::DropShadow {
                sigma,
                offset,
                color,
            }
            | Self::DropShadowOnly {
                sigma,
                offset,
                color,
            } => valid_sigma(*sigma) && offset.iter().chain(color).all(|v| v.is_finite()),
            Self::ColorMatrix(m) => m.iter().all(|v| v.is_finite() && v.abs() <= 1e6),
        }
    }
    pub(crate) fn creates_content(&self) -> bool {
        self.valid()
            && match self {
                Self::ColorMatrix(m) => m[19] > 0.,
                Self::ComponentTransfer { channels, .. } => channels[3].sample(0.) > 0.,
                Self::ConvolveMatrix(c) => !c.preserve_alpha && c.bias > 0.,
                _ => false,
            }
    }
}
fn valid_sigma(sigma: [f32; 2]) -> bool {
    sigma.iter().all(|v| v.is_finite() && *v >= 0.)
}

pub(crate) fn shadow_color(color: [f32; 4], linear: bool) -> [f32; 4] {
    let mut c = color.map(|v| v.clamp(0., 1.));
    if linear {
        for v in &mut c[..3] {
            *v = linearize(*v);
        }
    }
    c
}

// Integrated Gaussian mass per pixel, rather than point samples, remains
// continuous as sigma approaches zero. O(canvas extent) radius even for huge sigma.
pub(crate) fn kernel(sigma: f32, extent: u32) -> Vec<f32> {
    if sigma < 0.001 {
        return vec![1.];
    }
    let radius = (4. * sigma).ceil().min(extent.saturating_sub(1) as f32) as usize;
    (0..=radius)
        .map(|i| {
            crate::blur::cdf((i as f32 + 0.5) / sigma) - crate::blur::cdf((i as f32 - 0.5) / sigma)
        })
        .collect()
}
fn gaussian(pixels: &mut Vec<f32>, size: [u32; 2], sigma: [f32; 2]) {
    let [w, h] = size;
    let mut scratch = vec![0.; pixels.len()];
    for axis in 0..2 {
        if sigma[axis] < 0.001 {
            continue;
        }
        let weights = kernel(sigma[axis], size[axis]);
        for y in 0..h {
            for x in 0..w {
                let at = ((y * w + x) * 4) as usize;
                for c in 0..4 {
                    let mut sum = pixels[at + c] * weights[0];
                    for (d, weight) in weights.iter().enumerate().skip(1) {
                        for sign in [-1i64, 1] {
                            let xx = x as i64 + if axis == 0 { sign * d as i64 } else { 0 };
                            let yy = y as i64 + if axis == 1 { sign * d as i64 } else { 0 };
                            if xx >= 0 && yy >= 0 && xx < w as i64 && yy < h as i64 {
                                sum +=
                                    pixels[((yy as u32 * w + xx as u32) * 4) as usize + c] * weight;
                            }
                        }
                    }
                    scratch[at + c] = sum;
                }
            }
        }
        std::mem::swap(pixels, &mut scratch);
    }
}
fn morphology(pixels: &mut Vec<f32>, size: [u32; 2], radius: [u32; 2], operator: Morphology) {
    // A monotone deque visits each sample at most twice, independent of radius.
    // The independent 2D window oracle covers both axes and transparent borders.
    let mut output = vec![0.; pixels.len()];
    let mut window = std::collections::VecDeque::<(usize, f32)>::new();
    for axis in 0..2 {
        let extent = size[axis] as usize;
        let r = radius[axis].min(size[axis]) as usize;
        if r == 0 || extent == 0 {
            continue;
        }
        let lines = size[1 - axis] as usize;
        for line in 0..lines {
            for channel in 0..4 {
                window.clear();
                let mut next = 0;
                let index = |at: usize| {
                    if axis == 0 {
                        (line * size[0] as usize + at) * 4 + channel
                    } else {
                        (at * size[0] as usize + line) * 4 + channel
                    }
                };
                for at in 0..extent {
                    let right = at.saturating_add(r).min(extent - 1);
                    while next <= right {
                        let value = pixels[index(next)];
                        while window.back().is_some_and(|(_, v)| {
                            if operator == Morphology::Dilate {
                                *v <= value
                            } else {
                                *v >= value
                            }
                        }) {
                            window.pop_back();
                        }
                        window.push_back((next, value));
                        next += 1;
                    }
                    let left = at.saturating_sub(r);
                    while window.front().is_some_and(|(i, _)| *i < left) {
                        window.pop_front();
                    }
                    output[index(at)] = if operator == Morphology::Erode
                        && (at < r || at.saturating_add(r) >= extent)
                    {
                        0.
                    } else {
                        window.front().unwrap().1
                    };
                }
            }
        }
        std::mem::swap(pixels, &mut output);
    }
}
fn sample_at(pixels: &[f32], size: [u32; 2], x: f32, y: f32) -> [f32; 4] {
    // Test float bounds before conversion: large finite offsets cannot overflow.
    if x < -1. || y < -1. || x >= size[0] as f32 || y >= size[1] as f32 {
        return [0.; 4];
    }
    let (ix, iy) = (x.floor() as i64, y.floor() as i64);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let mut a = [0.; 4];
    for (dy, wy) in [(0, 1. - fy), (1, fy)] {
        for (dx, wx) in [(0, 1. - fx), (1, fx)] {
            let (xx, yy) = (ix + dx, iy + dy);
            if xx >= 0 && yy >= 0 && xx < size[0] as i64 && yy < size[1] as i64 {
                for c in 0..4 {
                    a[c] += pixels[((yy as u32 * size[0] + xx as u32) * 4) as usize + c] * wx * wy;
                }
            }
        }
    }
    a
}
pub(crate) fn apply(pixels: &mut Vec<f32>, size: [u32; 2], filters: &[LayerFilter], linear: bool) {
    for filter in filters.iter().filter(|f| f.valid()) {
        match filter {
            LayerFilter::ComponentTransfer {
                channels,
                color_space,
            } => imaging::transfer(pixels, channels, *color_space, linear),
            LayerFilter::ConvolveMatrix(c) => imaging::convolve(pixels, size, c, linear),
            LayerFilter::Offset { offset } => {
                let input = pixels.clone();
                for y in 0..size[1] {
                    for x in 0..size[0] {
                        let at = ((y * size[0] + x) * 4) as usize;
                        pixels[at..at + 4].copy_from_slice(&sample_at(
                            &input,
                            size,
                            x as f32 - offset[0],
                            y as f32 - offset[1],
                        ));
                    }
                }
            }
            LayerFilter::Morphology { radius, operator } => {
                morphology(pixels, size, *radius, *operator)
            }
            LayerFilter::Gaussian { sigma } => gaussian(pixels, size, *sigma),
            LayerFilter::DropShadow {
                sigma,
                offset,
                color,
            }
            | LayerFilter::DropShadowOnly {
                sigma,
                offset,
                color,
            } => {
                let mut blurred = pixels.clone();
                gaussian(&mut blurred, size, *sigma);
                let c = shadow_color(*color, linear);
                for y in 0..size[1] {
                    for x in 0..size[0] {
                        let at = ((y * size[0] + x) * 4) as usize;
                        let a =
                            sample_at(&blurred, size, x as f32 - offset[0], y as f32 - offset[1])
                                [3]
                                * c[3];
                        let only = matches!(filter, LayerFilter::DropShadowOnly { .. });
                        let t = if only { 1. } else { 1. - pixels[at + 3] };
                        if only {
                            pixels[at..at + 4].fill(0.);
                        }
                        for k in 0..3 {
                            pixels[at + k] += c[k] * a * t;
                        }
                        pixels[at + 3] += a * t;
                    }
                }
            }
            LayerFilter::ColorMatrix(m) => {
                for p in pixels.chunks_exact_mut(4) {
                    let straight = if p[3] > 0. {
                        [p[0] / p[3], p[1] / p[3], p[2] / p[3], p[3]]
                    } else {
                        [0.; 4]
                    };
                    let out: [f32; 4] = std::array::from_fn(|row| {
                        // f64 accumulation also handles large finite coefficients.
                        (m[row * 5 + 4] as f64
                            + (0..4)
                                .map(|c| m[row * 5 + c] as f64 * straight[c] as f64)
                                .sum::<f64>())
                        .clamp(0., 1.) as f32
                    });
                    for c in 0..3 {
                        p[c] = out[c] * out[3];
                    }
                    p[3] = out[3];
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gaussian_impulse_matches_independent_integrated_density() {
        // Independent f64 Simpson integration per pixel; no production CDF.
        fn mass(k: i32, s: f64) -> f64 {
            let n = 128;
            let lo = k as f64 - 0.5;
            (0..=n)
                .map(|i| {
                    let x = lo + i as f64 / n as f64;
                    let weight = if i == 0 || i == n {
                        1.
                    } else if i % 2 == 0 {
                        2.
                    } else {
                        4.
                    };
                    weight * (-x * x / (2. * s * s)).exp()
                        / (s * (2. * std::f64::consts::PI).sqrt())
                        / (3. * n as f64)
                })
                .sum()
        }
        let mut worst = 0f64;
        for sigma in [[0.1, 0.5], [0.5, 2.], [2., 3.], [12., 1.]] {
            let mut p = vec![0.; 17 * 17 * 4];
            let center = (8 * 17 + 8) * 4;
            p[center..center + 4].copy_from_slice(&[0.2, 0.4, 0.8, 1.]);
            apply(&mut p, [17, 17], &[LayerFilter::Gaussian { sigma }], true);
            for y in 0..17 {
                for x in 0..17 {
                    let expected = mass(x - 8, sigma[0] as f64) * mass(y - 8, sigma[1] as f64);
                    let at = ((y * 17 + x) * 4) as usize;
                    let error = (p[at + 3] as f64 - expected).abs();
                    worst = worst.max(error);
                    assert!(error < 0.0001, "{sigma:?} {x}/{y}: {error}");
                    assert!((p[at] - p[at + 3] * 0.2).abs() < 0.00001);
                }
            }
        }
        eprintln!("independent integrated Gaussian max alpha error: {worst}");
    }
}
