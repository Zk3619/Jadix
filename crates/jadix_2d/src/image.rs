//! Immutable shared bitmap resources. Clones retain identity for GPU upload reuse.
use jadix_theme::color_space::{
    encode_linear_srgb_component as encode_linear_srgb_to_display_srgb,
    linearize_display_srgb_component as linearize_display_srgb,
};
use std::sync::Arc;
#[cfg(feature = "gpu")]
use std::sync::{
    atomic::{AtomicU64, Ordering},
    OnceLock,
};

#[cfg(feature = "gpu")]
static NEXT_IMAGE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum ImageSampling {
    #[default]
    Nearest,
    /// Interpolate premultiplied linear-sRGB texels; transparent RGB cannot bleed.
    Bilinear,
    /// 4 × 4 Mitchell–Netravali reconstruction (B = C = 1/3).
    Bicubic,
    /// 6 × 6 Lanczos windowed sinc, radius 3. Not scale-adaptive minification.
    Lanczos3,
    /// Bilinear samples in two prepared mip levels; footprint chooses LOD.
    Trilinear,
    /// Up to 16 trilinear taps along the footprint's major singular axis.
    Anisotropic,
    /// Elliptical weighted Gaussian reconstruction, bounded to 17x17 taps per
    /// selected mip level. Explicit mips preserve large minification footprints.
    Ewa,
}

#[derive(Debug)]
pub(crate) struct ImageData {
    #[cfg(feature = "gpu")]
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub linear: Option<Vec<f32>>,
    pub(crate) mips: Option<Arc<crate::mipmap::Mipmaps>>,
    #[cfg(feature = "gpu")]
    gpu_rgba: OnceLock<Vec<u8>>,
    #[cfg(feature = "gpu")]
    pub update: Option<(u64, [u32; 4])>,
}

#[derive(Clone, Debug)]
pub struct Image2D(pub(crate) Arc<ImageData>);

impl PartialEq for Image2D {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Image2D {
    /// Copy an exact, nonempty straight-alpha sRGB bitmap. Invalid dimensions/length fail.
    pub fn from_rgba8(width: u32, height: u32, rgba: &[u8]) -> Option<Self> {
        let len = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        if width == 0 || height == 0 || rgba.len() != len {
            return None;
        }
        Some(Self(Arc::new(ImageData {
            #[cfg(feature = "gpu")]
            id: NEXT_IMAGE.fetch_add(1, Ordering::Relaxed),
            width,
            height,
            rgba: rgba.to_vec(),
            linear: None,
            mips: None,
            #[cfg(feature = "gpu")]
            gpu_rgba: OnceLock::new(),
            #[cfg(feature = "gpu")]
            update: None,
        })))
    }
    /// Finite straight linear-sRGB RGBA, retaining negative wide-gamut and HDR
    /// components up to 16384. rgba8() is an explicitly clipped sRGB preview.
    pub fn from_linear_rgba32f(width: u32, height: u32, rgba: &[f32]) -> Option<Self> {
        let len = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        if width == 0
            || height == 0
            || rgba.len() != len
            || rgba.chunks_exact(4).any(|p| {
                p.iter().any(|v| !v.is_finite() || v.abs() > 16384.) || !(0. ..=1.).contains(&p[3])
            })
        {
            return None;
        }
        let preview = rgba
            .chunks_exact(4)
            .flat_map(|p| {
                [
                    (encode_linear_srgb_to_display_srgb(p[0]) * 255.).round() as u8,
                    (encode_linear_srgb_to_display_srgb(p[1]) * 255.).round() as u8,
                    (encode_linear_srgb_to_display_srgb(p[2]) * 255.).round() as u8,
                    (p[3] * 255.).round() as u8,
                ]
            })
            .collect();
        Some(Self(Arc::new(ImageData {
            #[cfg(feature = "gpu")]
            id: NEXT_IMAGE.fetch_add(1, Ordering::Relaxed),
            width,
            height,
            rgba: preview,
            linear: Some(rgba.to_vec()),
            mips: None,
            #[cfg(feature = "gpu")]
            gpu_rgba: OnceLock::new(),
            #[cfg(feature = "gpu")]
            update: None,
        })))
    }
    pub fn linear_rgba32f(&self) -> Option<&[f32]> {
        self.0.linear.as_deref()
    }
    /// Convert explicitly tagged CSS/predefined-space pixels before rendering.
    pub fn from_color_space(
        width: u32,
        height: u32,
        rgba: &[f32],
        space: crate::ColorSpaceTag,
    ) -> Option<Self> {
        let len = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        if width == 0 || height == 0 || rgba.len() != len {
            return None;
        }
        let converted = rgba
            .chunks_exact(4)
            .map(|p| crate::Color2D::new(space, p.try_into().unwrap()).map(|c| c.linear_rgba()))
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        Self::from_linear_rgba32f(width, height, &converted)
    }
    pub(crate) fn linear_texel(&self, x: usize, y: usize) -> [f32; 4] {
        let at = (y * self.width() as usize + x) * 4;
        if let Some(p) = &self.0.linear {
            let p = &p[at..at + 4];
            return [p[0] * p[3], p[1] * p[3], p[2] * p[3], p[3]];
        }
        let p = &self.0.rgba[at..at + 4];
        let a = p[3] as f32 / 255.;
        [
            linearize_display_srgb(p[0] as f32 / 255.) * a,
            linearize_display_srgb(p[1] as f32 / 255.) * a,
            linearize_display_srgb(p[2] as f32 / 255.) * a,
            a,
        ]
    }
    /// Sample in texel coordinates (centers at n + 0.5), returning premultiplied
    /// linear-sRGB. High-quality kernels clamp ringing to the nontransparent tap
    /// color envelope and alpha `[0,1]`, preserving signed/HDR source RGB.
    pub fn sample_linear(
        &self,
        point: [f32; 2],
        sampling: ImageSampling,
        extend_x: crate::ExtendMode,
        extend_y: crate::ExtendMode,
    ) -> [f32; 4] {
        crate::brush::sample_image(self, (point[0], point[1]), extend_x, extend_y, sampling)
    }
    /// Sample a validated crop with Pad extension, in crop-local texel coordinates.
    /// Every tap is clamped to the crop, so atlas neighbors cannot bleed.
    pub fn sample_region_linear(
        &self,
        region: [u32; 4],
        point: [f32; 2],
        sampling: ImageSampling,
    ) -> Option<[f32; 4]> {
        let [x, y, w, h] = region;
        if w == 0
            || h == 0
            || x.checked_add(w)? > self.width()
            || y.checked_add(h)? > self.height()
            || point.iter().any(|v| !v.is_finite())
        {
            return None;
        }
        Some(filtered_sample((point[0], point[1]), sampling, |px, py| {
            self.linear_texel(
                x as usize + px.clamp(0., w as f32 - 1.) as usize,
                y as usize + py.clamp(0., h as f32 - 1.) as usize,
            )
        }))
    }
    /// CPU-owned pixel storage, excluding lazily prepared GPU upload bytes.
    pub fn byte_len(&self) -> usize {
        self.0.rgba.len()
            + self.0.linear.as_ref().map_or(0, |v| v.len() * 4)
            + self.0.mips.as_ref().map_or(0, |m| {
                m.levels.iter().map(|v| v.pixels.len() * 4).sum::<usize>()
            })
    }
    /// Immutable update. Existing snapshots remain valid; the GPU may upload only
    /// this rectangle when the predecessor is resident and no longer CPU-owned.
    pub fn updated_rgba8(&self, rect: [u32; 4], rgba: &[u8]) -> Option<Self> {
        let [x, y, w, h] = rect;
        if self.0.linear.is_some()
            || w == 0
            || h == 0
            || x.checked_add(w)? > self.width()
            || y.checked_add(h)? > self.height()
            || rgba.len() != (w as usize).checked_mul(h as usize)?.checked_mul(4)?
        {
            return None;
        }
        let mut pixels = self.0.rgba.clone();
        for row in 0..h as usize {
            let at = ((y as usize + row) * self.width() as usize + x as usize) * 4;
            pixels[at..at + w as usize * 4]
                .copy_from_slice(&rgba[row * w as usize * 4..(row + 1) * w as usize * 4]);
        }
        #[allow(unused_mut)]
        let mut result = Self::from_rgba8(self.width(), self.height(), &pixels)?;
        #[cfg(feature = "gpu")]
        {
            Arc::get_mut(&mut result.0)?.update = Some((self.0.id, rect));
        }
        Some(result)
    }
    pub fn width(&self) -> u32 {
        self.0.width
    }
    pub fn height(&self) -> u32 {
        self.0.height
    }
    pub fn rgba8(&self) -> &[u8] {
        &self.0.rgba
    }
    // sRGB texture decoding produces linear premultiplied RGB before hardware filtering.
    #[cfg(feature = "gpu")]
    pub(crate) fn gpu_rgba(&self) -> &[u8] {
        self.0.gpu_rgba.get_or_init(|| {
            if self.mip_level_count() > 1 && self.0.linear.is_none() {
                return (0..self.height())
                    .flat_map(|y| {
                        (0..self.width()).flat_map(move |x| {
                            self.linear_texel(x as usize, y as usize)
                                .into_iter()
                                .flat_map(|v| half::f16::from_f32(v).to_le_bytes())
                        })
                    })
                    .collect();
            }
            if let Some(pixels) = &self.0.linear {
                return pixels
                    .chunks_exact(4)
                    .flat_map(|p| [p[0] * p[3], p[1] * p[3], p[2] * p[3], p[3]])
                    .flat_map(|v| half::f16::from_f32(v).to_bits().to_le_bytes())
                    .collect();
            }
            let mut pixels = self.0.rgba.clone();
            for p in pixels.chunks_exact_mut(4) {
                let a = p[3] as f32 / 255.0;
                for c in &mut p[..3] {
                    *c = (encode_linear_srgb_to_display_srgb(
                        linearize_display_srgb(*c as f32 / 255.0) * a,
                    ) * 255.0)
                        .round() as u8;
                }
            }
            pixels
        })
    }
}

// Separable reconstruction shared by raw image and brush/crop sampling.
// Bilinear retains the original arithmetic; negative-lobe kernels use a local
// straight-color envelope instead of assuming SDR RGB <= alpha.
pub(crate) fn filtered_sample(
    point: (f32, f32),
    sampling: ImageSampling,
    texel: impl Fn(f32, f32) -> [f32; 4],
) -> [f32; 4] {
    if !point.0.is_finite() || !point.1.is_finite() {
        return [0.; 4];
    }
    if sampling == ImageSampling::Nearest {
        return texel(point.0.floor(), point.1.floor());
    }
    let x = point.0 - 0.5;
    let y = point.1 - 0.5;
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = (x - ix, y - iy);
    if sampling == ImageSampling::Bilinear || sampling as u8 >= 4 {
        let (a, b, c, d) = (
            texel(ix, iy),
            texel(ix + 1., iy),
            texel(ix, iy + 1.),
            texel(ix + 1., iy + 1.),
        );
        return std::array::from_fn(|i| {
            (a[i] * (1. - fx) + b[i] * fx) * (1. - fy) + (c[i] * (1. - fx) + d[i] * fx) * fy
        });
    }
    let (first, last) = if sampling == ImageSampling::Bicubic {
        (-1, 2)
    } else {
        (-2, 3)
    };
    let mut sum = [0.; 4];
    let mut weight = 0.;
    let mut low = [f32::INFINITY; 3];
    let mut high = [f32::NEG_INFINITY; 3];
    for j in first..=last {
        for i in first..=last {
            let w = reconstruction_weight(i as f32 - fx, sampling)
                * reconstruction_weight(j as f32 - fy, sampling);
            if w == 0. {
                continue;
            }
            let c = texel(ix + i as f32, iy + j as f32);
            for k in 0..4 {
                sum[k] += c[k] * w;
            }
            weight += w;
            if c[3] > 0. {
                for k in 0..3 {
                    let v = c[k] / c[3];
                    low[k] = low[k].min(v);
                    high[k] = high[k].max(v);
                }
            }
        }
    }
    if sum[3] <= 0. || weight <= 0. {
        return [0.; 4];
    }
    let alpha = (sum[3] / weight).clamp(0., 1.);
    let mut result = [0.; 4];
    result[3] = alpha;
    for k in 0..3 {
        result[k] = (sum[k] / sum[3]).clamp(low[k], high[k]) * alpha;
    }
    result
}
fn reconstruction_weight(x: f32, sampling: ImageSampling) -> f32 {
    let x = x.abs();
    if sampling == ImageSampling::Bicubic {
        if x < 1. {
            return (7. * x * x * x - 12. * x * x + 16. / 3.) / 6.;
        }
        if x < 2. {
            return (-7. / 3. * x * x * x + 12. * x * x - 20. * x + 32. / 3.) / 6.;
        }
        return 0.;
    }
    if x < 1e-6 {
        return 1.;
    }
    if x >= 3. {
        return 0.;
    }
    let p = std::f32::consts::PI * x;
    p.sin() * (p / 3.).sin() / (p * p / 3.)
}
/// Sample in premultiplied linear light, then return straight display RGB + alpha.
pub(crate) fn sample(
    rgba: &[u8],
    width: u32,
    height: u32,
    u: f32,
    v: f32,
    sampling: ImageSampling,
) -> [f32; 4] {
    sample_region(rgba, width, height, [0, 0, width, height], u, v, sampling)
}
pub(crate) fn sample_region(
    rgba: &[u8],
    width: u32,
    height: u32,
    region: [u32; 4],
    u: f32,
    v: f32,
    sampling: ImageSampling,
) -> [f32; 4] {
    let [rx, ry, rw, rh] = region;
    let at = |x: f32, y: f32| {
        let x = x.clamp(rx as f32, (rx + rw - 1) as f32) as usize;
        let y = y.clamp(ry as f32, (ry + rh - 1) as f32) as usize;
        &rgba[(y * width as usize + x) * 4..][..4]
    };
    if sampling == ImageSampling::Nearest {
        let p = at((u * width as f32).floor(), (v * height as f32).floor());
        return std::array::from_fn(|i| p[i] as f32 / 255.);
    }
    let mut color = filtered_sample((u * width as f32, v * height as f32), sampling, |x, y| {
        let p = at(x, y);
        let a = p[3] as f32 / 255.;
        [
            linearize_display_srgb(p[0] as f32 / 255.) * a,
            linearize_display_srgb(p[1] as f32 / 255.) * a,
            linearize_display_srgb(p[2] as f32 / 255.) * a,
            a,
        ]
    });
    if color[3] > 1e-6 {
        for i in 0..3 {
            color[i] = encode_linear_srgb_to_display_srgb(color[i] / color[3]);
        }
    }
    color
}
