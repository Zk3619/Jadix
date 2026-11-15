//! Isolated layer mixing and Porter–Duff composition of premultiplied pixels.
use crate::Image2D;

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    Difference,
    Exclusion,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

/// Porter–Duff composition after color mixing. Plus modes clamp to LDR;
/// unbounded HDR accumulation is not provided by the current target format.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CompositeMode {
    Clear,
    Copy,
    Destination,
    #[default]
    SourceOver,
    DestinationOver,
    SourceIn,
    DestinationIn,
    SourceOut,
    DestinationOut,
    SourceAtop,
    DestinationAtop,
    Xor,
    Plus,
    PlusLighter,
    /// Multiply the backdrop by premultiplied source luminance and group opacity.
    LuminanceIn,
    /// Multiply the backdrop by one minus premultiplied source luminance.
    LuminanceOut,
}
impl CompositeMode {
    pub(crate) fn preserves_destination_with_empty_source(self) -> bool {
        matches!(
            self,
            Self::Destination
                | Self::SourceOver
                | Self::DestinationOver
                | Self::DestinationOut
                | Self::SourceAtop
                | Self::Xor
                | Self::Plus
                | Self::PlusLighter
                | Self::LuminanceOut
        )
    }
}

/// Effects apply once at group pop. Clip is a device-pixel rectangle; mask is
/// canvas-sized alpha. Both limit FINAL composition, preserving the outside.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerOptions {
    pub opacity: f32,
    pub blend: BlendMode,
    pub compose: CompositeMode,
    pub clip: Option<[f32; 4]>,
    pub mask: Option<Image2D>,
    /// Applied in order before opacity, mixing and final clip/mask. Invalid
    /// filters are ignored. Device-pixel effects are bounded by the canvas.
    pub filters: Vec<crate::LayerFilter>,
}
impl Default for LayerOptions {
    fn default() -> Self {
        Self {
            opacity: 1.,
            blend: BlendMode::Normal,
            compose: CompositeMode::SourceOver,
            clip: None,
            mask: None,
            filters: Vec::new(),
        }
    }
}
impl LayerOptions {
    pub fn new(opacity: f32, blend: BlendMode) -> Self {
        Self {
            opacity,
            blend,
            ..Self::default()
        }
    }
    pub(crate) fn normalized(mut self, width: u32, height: u32) -> Self {
        self.filters.retain(crate::LayerFilter::valid);
        self.opacity = if self.opacity.is_finite() {
            self.opacity.clamp(0., 1.)
        } else {
            0.
        };
        if self
            .clip
            .is_some_and(|c| c.iter().any(|v| !v.is_finite()) || c[2] <= 0. || c[3] <= 0.)
            || self
                .mask
                .as_ref()
                .is_some_and(|m| m.width() != width || m.height() != height)
        {
            self.clip = Some([0.; 4]);
            self.mask = None;
        }
        self
    }
    pub(crate) fn coverage(&self, x: u32, y: u32, width: u32) -> f32 {
        let rect = self.clip.map_or(1., |[cx, cy, cw, ch]| {
            ((x as f32 + 1.).min(cx + cw) - (x as f32).max(cx)).clamp(0., 1.)
                * ((y as f32 + 1.).min(cy + ch) - (y as f32).max(cy)).clamp(0., 1.)
        });
        rect * self.mask.as_ref().map_or(1., |m| {
            m.rgba8()[(y as usize * width as usize + x as usize) * 4 + 3] as f32 / 255.
        })
    }
}
fn lum(c: [f32; 3]) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}
fn sat(c: [f32; 3]) -> f32 {
    c.into_iter().fold(f32::NEG_INFINITY, f32::max) - c.into_iter().fold(f32::INFINITY, f32::min)
}
fn set_lum(mut c: [f32; 3], value: f32) -> [f32; 3] {
    let delta = value - lum(c);
    c = c.map(|v| v + delta);
    let l = lum(c);
    let lo = c.into_iter().fold(f32::INFINITY, f32::min);
    let hi = c.into_iter().fold(f32::NEG_INFINITY, f32::max);
    if lo < 0. {
        c = c.map(|v| l + (v - l) * l / (l - lo).max(1e-12));
    }
    if hi > 1. {
        c = c.map(|v| l + (v - l) * (1. - l) / (hi - l).max(1e-12));
    }
    c.map(|v| v.clamp(0., 1.))
}
fn set_sat(c: [f32; 3], value: f32) -> [f32; 3] {
    let lo = c.into_iter().fold(f32::INFINITY, f32::min);
    let range = sat(c);
    if range > 0. {
        c.map(|v| (v - lo) * value / range)
    } else {
        [0.; 3]
    }
}
fn mix_color(cb: [f32; 3], cs: [f32; 3], mode: BlendMode) -> [f32; 3] {
    match mode {
        BlendMode::Hue => set_lum(set_sat(cs, sat(cb)), lum(cb)),
        BlendMode::Saturation => set_lum(set_sat(cb, sat(cs)), lum(cb)),
        BlendMode::Color => set_lum(cs, lum(cb)),
        BlendMode::Luminosity => set_lum(cb, lum(cs)),
        _ => std::array::from_fn(|i| {
            let (b, s) = (cb[i], cs[i]);
            match mode {
                BlendMode::Normal => s,
                BlendMode::Multiply => s * b,
                BlendMode::Screen => s + b - s * b,
                BlendMode::Overlay => {
                    if b <= 0.5 {
                        2. * s * b
                    } else {
                        1. - 2. * (1. - s) * (1. - b)
                    }
                }
                BlendMode::Darken => s.min(b),
                BlendMode::Lighten => s.max(b),
                BlendMode::Difference => (s - b).abs(),
                BlendMode::Exclusion => s + b - 2. * s * b,
                BlendMode::ColorDodge => {
                    if b == 0. {
                        0.
                    } else if s == 1. {
                        1.
                    } else {
                        (b / (1. - s)).min(1.)
                    }
                }
                BlendMode::ColorBurn => {
                    if b == 1. {
                        1.
                    } else if s == 0. {
                        0.
                    } else {
                        1. - ((1. - b) / s).min(1.)
                    }
                }
                BlendMode::HardLight => {
                    if s <= 0.5 {
                        2. * s * b
                    } else {
                        1. - 2. * (1. - s) * (1. - b)
                    }
                }
                BlendMode::SoftLight => {
                    if s <= 0.5 {
                        b - (1. - 2. * s) * b * (1. - b)
                    } else {
                        let d = if b <= 0.25 {
                            ((16. * b - 12.) * b + 4.) * b
                        } else {
                            b.sqrt()
                        };
                        b + (2. * s - 1.) * (d - b)
                    }
                }
                _ => unreachable!(),
            }
        }),
    }
}
pub(crate) fn compose_pixel(d: [f32; 4], s: [f32; 4], options: &LayerOptions) -> [f32; 4] {
    compose_pixel_extended(d, s, options, false)
}
fn compose_pixel_extended(
    d: [f32; 4],
    s: [f32; 4],
    options: &LayerOptions,
    extended: bool,
) -> [f32; 4] {
    if matches!(
        options.compose,
        CompositeMode::LuminanceIn | CompositeMode::LuminanceOut
    ) {
        let l = ((s[0] * 0.2126 + s[1] * 0.7152 + s[2] * 0.0722) * options.opacity).clamp(0., 1.);
        let factor = if options.compose == CompositeMode::LuminanceOut {
            1. - l
        } else {
            l
        };
        return d.map(|v| v * factor);
    }
    let sa = s[3].clamp(0., 1.) * options.opacity;
    let da = d[3].clamp(0., 1.);
    if options.blend == BlendMode::Normal && options.compose == CompositeMode::SourceOver {
        return std::array::from_fn(|i| s[i] * options.opacity + d[i] * (1. - sa));
    }
    let cs = std::array::from_fn(|i| {
        if s[3] > 0. {
            (s[i] / s[3]).clamp(0., 1.)
        } else {
            0.
        }
    });
    let cb = std::array::from_fn(|i| {
        if da > 0. {
            (d[i] / da).clamp(0., 1.)
        } else {
            0.
        }
    });
    let mixed = mix_color(cb, cs, options.blend);
    let (fa, fb) = match options.compose {
        CompositeMode::Clear => (0., 0.),
        CompositeMode::Copy => (1., 0.),
        CompositeMode::Destination => (0., 1.),
        CompositeMode::SourceOver => (1., 1. - sa),
        CompositeMode::DestinationOver => (1. - da, 1.),
        CompositeMode::SourceIn => (da, 0.),
        CompositeMode::DestinationIn => (0., sa),
        CompositeMode::SourceOut => (1. - da, 0.),
        CompositeMode::DestinationOut => (0., 1. - sa),
        CompositeMode::SourceAtop => (da, 1. - sa),
        CompositeMode::DestinationAtop => (1. - da, sa),
        CompositeMode::Xor => (1. - da, 1. - sa),
        CompositeMode::Plus | CompositeMode::PlusLighter => (1., 1.),
        CompositeMode::LuminanceIn | CompositeMode::LuminanceOut => unreachable!(),
    };
    if extended && options.blend == BlendMode::Normal {
        return [
            s[0] * options.opacity * fa + d[0] * fb,
            s[1] * options.opacity * fa + d[1] * fb,
            s[2] * options.opacity * fa + d[2] * fb,
            (sa * fa + da * fb).clamp(0., 1.),
        ];
    }
    let mut out = [0.; 4];
    for i in 0..3 {
        out[i] = (sa * fa * ((1. - da) * cs[i] + da * mixed[i]) + fb * d[i]).clamp(0., 1.);
    }
    out[3] = (sa * fa + da * fb).clamp(0., 1.);
    out
}
pub(crate) fn composite(dst: &mut [f32], src: &[f32], opacity: f32, mode: BlendMode) {
    let options = LayerOptions::new(opacity, mode).normalized(0, 0);
    for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
        let out = compose_pixel((&*d).try_into().unwrap(), s.try_into().unwrap(), &options);
        d.copy_from_slice(&out);
    }
}
pub(crate) fn composite_layer(
    dst: &mut [f32],
    src: &[f32],
    options: &LayerOptions,
    width: u32,
    extended: bool,
) {
    for (index, (d, s)) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)).enumerate() {
        let coverage = options.coverage(index as u32 % width, index as u32 / width, width);
        if coverage == 0. {
            continue;
        }
        let out = compose_pixel_extended(
            d.try_into().unwrap(),
            s.try_into().unwrap(),
            options,
            extended,
        );
        for i in 0..4 {
            d[i] += coverage * (out[i] - d[i]);
        }
    }
}
