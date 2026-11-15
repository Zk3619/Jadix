//! Explicit color conversion at input/output boundaries. Rendering works in
//! extended linear sRGB; display mapping is an export operation, not a draw clamp.
mod css;
pub use ::color::{
    ColorSpaceTag, DynamicColor, Flags, HueDirection as ColorHueDirection, ParseError,
};
pub use css::{ColorContext2D, CssColorError};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color2D(pub DynamicColor);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GamutMapping {
    #[default]
    Clip,
    /// Reduce Oklch chroma with the CSS local-MINDE threshold (deltaE OK 0.02).
    OklchChroma,
}

impl Color2D {
    /// CSS Color 4 literals: named/hex, rgb/hsl/hwb, Lab/Lch/Oklab/Oklch and
    /// color() predefined spaces, including missing `none` components. CSS
    /// expressions (var/calc/relative colors/color-mix) need a style resolver.
    pub fn parse(css: &str) -> Result<Self, ParseError> {
        ::color::parse_color(css).map(Self)
    }
    pub fn new(space: ColorSpaceTag, components: [f32; 4]) -> Option<Self> {
        components
            .iter()
            .all(|v| v.is_finite())
            .then_some(Self(DynamicColor {
                cs: space,
                flags: Flags::default(),
                components,
            }))
    }
    pub fn linear_rgba(self) -> [f32; 4] {
        let mut c = self.0.convert(ColorSpaceTag::LinearSrgb).components;
        c[3] = c[3].clamp(0., 1.);
        c
    }
    /// Explicit output mapping to a bounded RGB display gamut. Invalid/non-RGB
    /// target spaces return None; HDR rendering itself does not call this.
    pub fn map_to(self, target: ColorSpaceTag, mapping: GamutMapping) -> Option<Self> {
        if !matches!(
            target,
            ColorSpaceTag::Srgb
                | ColorSpaceTag::LinearSrgb
                | ColorSpaceTag::DisplayP3
                | ColorSpaceTag::A98Rgb
                | ColorSpaceTag::ProphotoRgb
                | ColorSpaceTag::Rec2020
        ) {
            return None;
        }
        let converted = self.0.convert(target);
        if !converted.components.iter().all(|v| v.is_finite()) {
            return None;
        }
        let inside = |c: DynamicColor| c.components[..3].iter().all(|v| (0. ..=1.).contains(v));
        if mapping == GamutMapping::Clip || inside(converted) {
            return Some(Self(converted.clip()));
        }
        let mut origin = self.0.convert(ColorSpaceTag::Oklch);
        let lightness = origin.components[0];
        if lightness >= 1. || lightness <= 0. {
            return Self::new(target, [if lightness >= 1. { 1. } else { 0. }; 4]).map(|mut c| {
                c.0.components[3] = converted.components[3].clamp(0., 1.);
                c
            });
        }
        let mut min = 0.;
        let mut max = origin.components[1];
        let mut min_in_gamut = true;
        let mut clipped = converted.clip();
        while max - min > 0.0001 {
            let chroma = (min + max) * 0.5;
            origin.components[1] = chroma;
            let candidate = origin.convert(target);
            if min_in_gamut && inside(candidate) {
                min = chroma;
                continue;
            }
            clipped = candidate.clip();
            let a = clipped.convert(ColorSpaceTag::Oklab).components;
            let b = origin.convert(ColorSpaceTag::Oklab).components;
            let delta =
                ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
            if delta < 0.02 {
                if 0.02 - delta < 0.0001 {
                    break;
                }
                min_in_gamut = false;
                min = chroma;
            } else {
                max = chroma;
            }
        }
        Some(Self(clipped))
    }
    pub fn brush(self) -> Option<crate::Brush2D> {
        let stops = crate::GradientStops::new_linear(&[(0., self.linear_rgba())]).ok()?;
        Some(crate::Brush2D::vector(
            crate::VectorGradient::linear([0., 0.], [1., 0.], stops).ok()?,
        ))
    }
}

/// A retained ICC transform for RGB/RGBA data. Profiles and transform creation
/// are explicit, never inferred from an untagged bitmap or the host display.
#[cfg(feature = "icc")]
pub struct IccTransform(std::sync::Arc<moxcms::TransformF32Executor>);
#[cfg(feature = "icc")]
impl IccTransform {
    /// Parse bounded source/destination profiles; output uses destination encoding.
    pub fn new(
        source: &[u8],
        destination: &[u8],
        intent: moxcms::RenderingIntent,
    ) -> Result<Self, moxcms::CmsError> {
        let options = moxcms::ParsingOptions {
            max_profile_size: 4 * 1024 * 1024,
            max_allowed_clut_size: 4 * 1024 * 1024,
            max_allowed_trc_size: 65536,
        };
        let source = moxcms::ColorProfile::new_from_slice_with_options(source, options)?;
        let destination = moxcms::ColorProfile::new_from_slice_with_options(destination, options)?;
        source
            .create_transform_f32(
                moxcms::Layout::Rgba,
                &destination,
                moxcms::Layout::Rgba,
                moxcms::TransformOptions {
                    rendering_intent: intent,
                    prefer_fixed_point: false,
                    allow_extended_range_rgb_xyz: true,
                    allow_use_cicp_transfer: false,
                    ..Default::default()
                },
            )
            .map(Self)
    }
    /// Bounded ICC RGB source to extended linear-sRGB for direct float image input.
    pub fn to_linear_srgb(
        source: &[u8],
        intent: moxcms::RenderingIntent,
    ) -> Result<Self, moxcms::CmsError> {
        Self::new(source, &Self::linear_profile()?, intent)
    }
    pub fn from_linear_srgb(
        destination: &[u8],
        intent: moxcms::RenderingIntent,
    ) -> Result<Self, moxcms::CmsError> {
        Self::new(&Self::linear_profile()?, destination, intent)
    }
    fn linear_profile() -> Result<Vec<u8>, moxcms::CmsError> {
        let mut profile = moxcms::ColorProfile::new_srgb();
        profile.cicp = None;
        profile.red_trc = Some(moxcms::ToneReprCurve::Lut(Vec::new()));
        profile.green_trc = profile.red_trc.clone();
        profile.blue_trc = profile.red_trc.clone();
        profile.encode()
    }
    pub fn transform(&self, rgba: &[f32]) -> Result<Vec<f32>, moxcms::CmsError> {
        if !rgba.len().is_multiple_of(4) {
            return Err(moxcms::CmsError::LaneMultipleOfChannels);
        }
        if rgba.len() > 64 * 1024 * 1024
            || rgba.iter().any(|v| !v.is_finite() || v.abs() > 16384.)
            || rgba.chunks_exact(4).any(|p| !(0. ..=1.).contains(&p[3]))
        {
            return Err(moxcms::CmsError::OverflowingError);
        }
        let mut result = vec![0.; rgba.len()];
        self.0.transform(rgba, &mut result)?;
        Ok(result)
    }
}
#[cfg(feature = "icc")]
pub use moxcms;
