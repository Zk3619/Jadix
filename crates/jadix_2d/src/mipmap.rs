//! Explicit, bounded, immutable prefiltering in premultiplied linear light.
use crate::{ExtendMode, Image2D, ImageSampling};
use std::sync::Arc;
#[cfg(feature = "gpu")]
use std::sync::OnceLock;
#[derive(Clone, Copy, Debug)]
pub struct MipmapLimits {
    /// Includes copied base storage and retained f32 mip pixels, not GPU/staging.
    pub max_bytes: usize,
    /// Includes level zero. A shorter chain is a deliberate LOD ceiling.
    pub max_levels: u32,
}
impl Default for MipmapLimits {
    fn default() -> Self {
        Self {
            max_bytes: 256 * 1024 * 1024,
            max_levels: 32,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MipmapError {
    InvalidLimits,
    InvalidRegion,
    ByteBudget,
}
impl std::fmt::Display for MipmapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mipmaps: {self:?}")
    }
}
impl std::error::Error for MipmapError {}
#[derive(Debug)]
pub(crate) struct MipLevel {
    pub size: [u32; 2],
    pub pixels: Vec<f32>,
    #[cfg(feature = "gpu")]
    pub bytes: OnceLock<Vec<u8>>,
}
#[derive(Debug)]
pub(crate) struct Mipmaps {
    pub levels: Vec<MipLevel>,
}
impl Image2D {
    /// Prepare a private complete/truncated mip chain. Level zero retains its
    /// pixels; every other level integrates exact NPOT rectangular source areas.
    /// Clones share the result. No lazy unbounded global crop cache is created.
    pub fn with_mipmaps(&self, limits: MipmapLimits) -> Result<Self, MipmapError> {
        if limits.max_levels == 0
            || limits.max_levels > 32
            || limits.max_bytes == 0
            || limits.max_bytes > 1024 * 1024 * 1024
        {
            return Err(MipmapError::InvalidLimits);
        }
        let mut size = [self.width(), self.height()];
        let base = self.0.rgba.len() + self.0.linear.as_ref().map_or(0, |v| v.len() * 4);
        let mut bytes = base;
        let mut sizes = Vec::new();
        while size != [1, 1] && sizes.len() + 1 < limits.max_levels as usize {
            size = size.map(|n| (n / 2).max(1));
            bytes = bytes
                .checked_add(size[0] as usize * size[1] as usize * 16)
                .ok_or(MipmapError::ByteBudget)?;
            if bytes > limits.max_bytes {
                return Err(MipmapError::ByteBudget);
            }
            sizes.push(size);
        }
        if bytes > limits.max_bytes {
            return Err(MipmapError::ByteBudget);
        }
        let mut levels: Vec<MipLevel> = Vec::with_capacity(sizes.len());
        for size in sizes {
            let previous = levels.last();
            let source_size = previous.map_or([self.width(), self.height()], |p| p.size);
            let texel = |x: u32, y: u32| {
                previous.map_or_else(
                    || self.linear_texel(x as usize, y as usize),
                    |p| {
                        p.pixels[((y * p.size[0] + x) * 4) as usize..][..4]
                            .try_into()
                            .unwrap()
                    },
                )
            };
            let mut pixels = Vec::with_capacity(size[0] as usize * size[1] as usize * 4);
            for y in 0..size[1] {
                for x in 0..size[0] {
                    let lo = [
                        x as f64 * source_size[0] as f64 / size[0] as f64,
                        y as f64 * source_size[1] as f64 / size[1] as f64,
                    ];
                    let hi = [
                        (x + 1) as f64 * source_size[0] as f64 / size[0] as f64,
                        (y + 1) as f64 * source_size[1] as f64 / size[1] as f64,
                    ];
                    let mut sum = [0f64; 4];
                    let area = (hi[0] - lo[0]) * (hi[1] - lo[1]);
                    for yy in lo[1].floor() as u32..(hi[1].ceil() as u32).min(source_size[1]) {
                        for xx in lo[0].floor() as u32..(hi[0].ceil() as u32).min(source_size[0]) {
                            let weight = (hi[0].min(xx as f64 + 1.) - lo[0].max(xx as f64))
                                * (hi[1].min(yy as f64 + 1.) - lo[1].max(yy as f64))
                                / area;
                            let p = texel(xx, yy);
                            for k in 0..4 {
                                sum[k] += p[k] as f64 * weight;
                            }
                        }
                    }
                    pixels.extend(sum.map(|v| v as f32));
                }
            }
            levels.push(MipLevel {
                size,
                pixels,
                #[cfg(feature = "gpu")]
                bytes: OnceLock::new(),
            });
        }
        let mut image = if let Some(p) = self.linear_rgba32f() {
            Self::from_linear_rgba32f(self.width(), self.height(), p)
        } else {
            Self::from_rgba8(self.width(), self.height(), self.rgba8())
        }
        .unwrap();
        Arc::get_mut(&mut image.0).unwrap().mips = Some(Arc::new(Mipmaps { levels }));
        Ok(image)
    }
    /// Copy a crop before prefiltering so neighboring atlas entries never enter
    /// coarse mip levels. Draw the returned image as a whole image/brush.
    pub fn mipmapped_region(
        &self,
        rect: [u32; 4],
        limits: MipmapLimits,
    ) -> Result<Self, MipmapError> {
        if limits.max_levels == 0
            || limits.max_levels > 32
            || limits.max_bytes == 0
            || limits.max_bytes > 1024 * 1024 * 1024
        {
            return Err(MipmapError::InvalidLimits);
        }
        let [x, y, w, h] = rect;
        if w == 0
            || h == 0
            || x.checked_add(w).is_none_or(|v| v > self.width())
            || y.checked_add(h).is_none_or(|v| v > self.height())
        {
            return Err(MipmapError::InvalidRegion);
        }
        let stride = if self.linear_rgba32f().is_some() {
            20
        } else {
            4
        };
        if (w as usize)
            .checked_mul(h as usize)
            .and_then(|n| n.checked_mul(stride))
            .is_none_or(|n| n > limits.max_bytes)
        {
            return Err(MipmapError::ByteBudget);
        }
        let image = if let Some(p) = self.linear_rgba32f() {
            let pixels = (y..y + h)
                .flat_map(|yy| {
                    let at = ((yy * self.width() + x) * 4) as usize;
                    p[at..at + w as usize * 4].iter().copied()
                })
                .collect::<Vec<_>>();
            Self::from_linear_rgba32f(w, h, &pixels)
        } else {
            let p = self.rgba8();
            let pixels = (y..y + h)
                .flat_map(|yy| {
                    let at = ((yy * self.width() + x) * 4) as usize;
                    p[at..at + w as usize * 4].iter().copied()
                })
                .collect::<Vec<_>>();
            Self::from_rgba8(w, h, &pixels)
        }
        .unwrap();
        image.with_mipmaps(limits)
    }
    pub fn mip_level_count(&self) -> u32 {
        1 + self.0.mips.as_ref().map_or(0, |m| m.levels.len() as u32)
    }
    pub fn mip_level_size(&self, level: u32) -> Option<[u32; 2]> {
        if level == 0 {
            return Some([self.width(), self.height()]);
        }
        self.0
            .mips
            .as_ref()?
            .levels
            .get(level as usize - 1)
            .map(|p| p.size)
    }
    /// Nonzero levels contain premultiplied linear RGBA; level zero is available
    /// through rgba8()/linear_rgba32f(), whose alpha contract remains straight.
    pub fn mip_level_premultiplied(&self, level: u32) -> Option<&[f32]> {
        self.0
            .mips
            .as_ref()?
            .levels
            .get(level.checked_sub(1)? as usize)
            .map(|v| v.pixels.as_slice())
    }
    #[cfg(feature = "gpu")]
    pub(crate) fn mip_gpu_bytes(&self, level: u32) -> &[u8] {
        let p = &self.0.mips.as_ref().unwrap().levels[level as usize - 1];
        p.bytes.get_or_init(|| {
            p.pixels
                .iter()
                .flat_map(|v| half::f16::from_f32(*v).to_le_bytes())
                .collect()
        })
    }
    /// Explicit texture-space derivatives for one output pixel. Missing/truncated
    /// chains clamp to their available levels, like a one-mip GPU texture.
    pub fn sample_linear_with_footprint(
        &self,
        point: [f32; 2],
        dx: [f32; 2],
        dy: [f32; 2],
        sampling: ImageSampling,
        extend_x: ExtendMode,
        extend_y: ExtendMode,
    ) -> [f32; 4] {
        if (sampling as u8) < 4 {
            return self.sample_linear(point, sampling, extend_x, extend_y);
        }
        if point
            .iter()
            .chain(dx.iter())
            .chain(dy.iter())
            .any(|v| !v.is_finite())
        {
            return [0.; 4];
        }
        let plan = footprint_plan(dx, dy, sampling);
        if sampling == ImageSampling::Ewa {
            let lod = plan[2].min((self.mip_level_count() - 1) as f32);
            let lo = lod.floor() as u32;
            let hi = lod.ceil() as u32;
            let t = lod.fract();
            let a = self.ewa_sample(point, plan, lo, extend_x, extend_y);
            if lo == hi {
                return a;
            }
            let b = self.ewa_sample(point, plan, hi, extend_x, extend_y);
            return std::array::from_fn(|i| a[i] * (1. - t) + b[i] * t);
        }
        let mut out = [0.; 4];
        let n = plan[3] as u32;
        for i in 0..n {
            let t = i as f32 + 0.5 - n as f32 * 0.5;
            let point = [point[0] + plan[0] * t, point[1] + plan[1] * t];
            let lod = plan[2].min((self.mip_level_count() - 1) as f32);
            let a = lod.floor() as u32;
            let b = (a + 1).min(self.mip_level_count() - 1);
            let f = lod - a as f32;
            let ca = self.mip_sample(point, a, extend_x, extend_y);
            let cb = self.mip_sample(point, b, extend_x, extend_y);
            for k in 0..4 {
                out[k] += (ca[k] * (1. - f) + cb[k] * f) / n as f32;
            }
        }
        out
    }
    fn ewa_sample(
        &self,
        point: [f32; 2],
        plan: [f32; 4],
        level: u32,
        x: ExtendMode,
        y: ExtendMode,
    ) -> [f32; 4] {
        let size = self.mip_level_size(level).unwrap();
        let ratio = [
            size[0] as f32 / self.width() as f32,
            size[1] as f32 / self.height() as f32,
        ];
        if plan[0].hypot(plan[1]) <= 1.000001 && plan[3] <= 1.000001 {
            return self.mip_sample(point, level, x, y);
        }
        let (a, b) = ellipse_axes(plan, ratio);
        let xx = a[0] * a[0] + b[0] * b[0];
        let xy = a[0] * a[1] + b[0] * b[1];
        let yy = a[1] * a[1] + b[1] * b[1];
        let det = xx * yy - xy * xy;
        if det <= 1e-12 {
            return self.mip_sample(point, level, x, y);
        }
        let center = [point[0] * ratio[0] - 0.5, point[1] * ratio[1] - 0.5];
        let origin = center.map(f32::floor);
        let mut out = [0.; 4];
        let mut total = 0.;
        for iy in -8..=8 {
            for ix in -8..=8 {
                let delta = [
                    origin[0] + ix as f32 - center[0],
                    origin[1] + iy as f32 - center[1],
                ];
                let r2 = (yy * delta[0] * delta[0] - 2. * xy * delta[0] * delta[1]
                    + xx * delta[1] * delta[1])
                    / det;
                if !(0. ..1.).contains(&r2) {
                    continue;
                }
                let weight = (-2. * r2).exp() - (-2f32).exp();
                total += weight;
                let (Some(tx), Some(ty)) = (
                    crate::brush::texel_index(origin[0] + ix as f32, size[0], x),
                    crate::brush::texel_index(origin[1] + iy as f32, size[1], y),
                ) else {
                    continue;
                };
                let value = if level == 0 {
                    self.linear_texel(tx, ty)
                } else {
                    self.mip_level_premultiplied(level).unwrap()[(ty * size[0] as usize + tx) * 4..]
                        [..4]
                        .try_into()
                        .unwrap()
                };
                for k in 0..4 {
                    out[k] += value[k] * weight;
                }
            }
        }
        if total > 0. {
            out.map(|v| v / total)
        } else {
            self.mip_sample(point, level, x, y)
        }
    }
    fn mip_sample(&self, point: [f32; 2], level: u32, x: ExtendMode, y: ExtendMode) -> [f32; 4] {
        if level == 0 {
            return self.sample_linear(point, ImageSampling::Bilinear, x, y);
        }
        let p = &self.0.mips.as_ref().unwrap().levels[level as usize - 1];
        crate::image::filtered_sample(
            (
                point[0] * p.size[0] as f32 / self.width() as f32,
                point[1] * p.size[1] as f32 / self.height() as f32,
            ),
            ImageSampling::Bilinear,
            |xx, yy| {
                let (Some(xx), Some(yy)) = (
                    crate::brush::texel_index(xx, p.size[0], x),
                    crate::brush::texel_index(yy, p.size[1], y),
                ) else {
                    return [0.; 4];
                };
                p.pixels[(yy * p.size[0] as usize + xx) * 4..][..4]
                    .try_into()
                    .unwrap()
            },
        )
    }
}
// Anisotropic plan = tap-step vector, explicit LOD, tap count (at most 16).
// EWA plan = base-texel major-axis vector, explicit LOD, minor-axis length.
// SVD avoids requiring GPU derivatives/extensions for either representation.
pub(crate) fn footprint_plan(dx: [f32; 2], dy: [f32; 2], sampling: ImageSampling) -> [f32; 4] {
    if dx.iter().chain(&dy).any(|v| !v.is_finite()) {
        return [0., 0., 0., 1.];
    }
    let scale = dx.iter().chain(&dy).map(|v| v.abs()).fold(0f32, f32::max);
    if scale > 1e12 {
        return if sampling == ImageSampling::Ewa {
            [1e12, 0., 40., 1e12]
        } else {
            [0., 0., 40., 1.]
        };
    }
    if scale == 0. {
        return [0., 0., 0., 1.];
    }
    let x = dx.map(|v| v / scale);
    let y = dy.map(|v| v / scale);
    let a = x[0] * x[0] + y[0] * y[0];
    let b = x[0] * x[1] + y[0] * y[1];
    let c = x[1] * x[1] + y[1] * y[1];
    let root = ((a - c) * (a - c) + 4. * b * b).sqrt();
    let major = ((a + c + root) * 0.5).sqrt() * scale;
    let minor = ((a + c - root).max(0.) * 0.5).sqrt() * scale;
    if sampling == ImageSampling::Trilinear {
        return [0., 0., major.max(1.).log2(), 1.];
    }
    let raw_minor = minor;
    let minor = minor.max(major / 16.).max(1.);
    let taps = (major / minor).ceil().clamp(1., 16.);
    let v = if b.abs() > 1e-8 {
        [(a + c + root) * 0.5 - c, b]
    } else if a >= c {
        [1., 0.]
    } else {
        [0., 1.]
    };
    let len = v[0].hypot(v[1]);
    if sampling == ImageSampling::Ewa {
        let major = major.max(1.);
        let minor = raw_minor.max(1.);
        return [
            v[0] / len * major,
            v[1] / len * major,
            (major / 4.).max(minor / 2.).max(1.).log2(),
            minor,
        ];
    }
    [
        v[0] / len * major / taps,
        v[1] / len * major / taps,
        minor.log2(),
        taps,
    ]
}

// Convert base-space ellipse axes into the actual NPOT mip grid. A fixed 8-texel
// radius bounds work even for deliberately truncated or missing mip chains.
fn ellipse_axes(plan: [f32; 4], ratio: [f32; 2]) -> ([f32; 2], [f32; 2]) {
    let len = plan[0].hypot(plan[1]);
    let direction = if len > 0. {
        [plan[0] / len, plan[1] / len]
    } else {
        [1., 0.]
    };
    let mut a = [plan[0] * ratio[0], plan[1] * ratio[1]];
    let mut b = [
        -direction[1] * plan[3] * ratio[0],
        direction[0] * plan[3] * ratio[1],
    ];
    // Clamp together so the ellipse orientation/eccentricity remain unchanged.
    let radius = (a[0] * a[0] + b[0] * b[0])
        .sqrt()
        .max((a[1] * a[1] + b[1] * b[1]).sqrt());
    let scale = 8. / radius.max(8.);
    a = a.map(|v| v * scale);
    b = b.map(|v| v * scale);
    (a, b)
}
