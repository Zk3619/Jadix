mod effects;
mod gradient;
mod motion;
mod interpolation;
pub use effects::*;
pub use gradient::*;
pub use motion::*;
pub use interpolation::*;

/// Visual properties shared by normal nodes and floating-panel transitions.
/// Edge order is top, right, bottom, left; corners are top-left clockwise.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ViewAppearance {
    pub opacity: f32,
    /// Column-major homogeneous transform, including perspective if required.
    pub transform: [f32; 16],
    pub background: Option<[u8; 4]>,
    pub tint: Option<[u8; 4]>,
    pub corners: [f32; 4],
    pub border_widths: [f32; 4],
    pub border_colors: [[u8; 4]; 4],
    pub outline_width: f32,
    pub outline_offset: f32,
    pub outline_color: [u8; 4],
    pub shadows: Vec<ViewShadow>,
    pub clip: bool,
    pub continuous_corners: bool,
    pub border_style: StrokeStyle,
    pub outline_style: StrokeStyle,
    pub origin: TransformOrigin,
    pub backface_hidden: bool,
    pub hidden: bool,
    pub cursor: ViewCursor,
    pub accent: Option<[u8; 4]>,
    pub blend_mode: BlendMode,
    /// First image is painted on top, as in CSS.
    pub backgrounds: Vec<ViewGradient>,
    pub masks: Vec<ViewGradient>,
    pub filters: Vec<ViewFilter>,
    pub backdrop_filters: Vec<ViewFilter>,
    /// Vertical sharp/filtered backdrop mix in normalized local coordinates.
    pub backdrop_progressive: Option<ViewProgressiveBlur>,
    pub backdrop: Option<MacBackdrop>,
    pub glass: GlassEffect,
    pub glass_tint: Option<[u8; 4]>,
    pub control: ControlAppearance,
    pub transition: Option<ViewTransition>,
    pub animation: Option<ViewAnimation>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ViewShadow {
    pub offset: [f32; 2],
    pub blur: f32,
    pub spread: f32,
    pub color: [u8; 4],
    pub inset: bool,
}

impl Default for ViewAppearance {
    fn default() -> Self {
        Self {
            opacity: 1.0,
            transform: [
                1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
            ],
            background: None,
            tint: None,
            corners: [0.; 4],
            border_widths: [0.; 4],
            border_colors: [[0; 4]; 4],
            outline_width: 0.,
            outline_offset: 0.,
            outline_color: [0; 4],
            shadows: Vec::new(),
            clip: false,
            continuous_corners: false,
            border_style: StrokeStyle::Solid,
            outline_style: StrokeStyle::Solid,
            origin: TransformOrigin::default(),
            backface_hidden: false,
            hidden: false,
            cursor: ViewCursor::Auto,
            accent: None,
            blend_mode: BlendMode::Normal,
            backgrounds: Vec::new(),
            masks: Vec::new(),
            filters: Vec::new(),
            backdrop_filters: Vec::new(),
            backdrop_progressive: None,
            backdrop: None,
            glass: GlassEffect::None,
            glass_tint: None,
            control: ControlAppearance::Native,
            transition: None,
            animation: None,
        }
    }
}

impl ViewAppearance {
    pub fn is_valid(&self) -> bool {
        let bounded = |v: f32| v.is_finite() && v.abs() <= 1_000_000.;
        let size = |v: f32| bounded(v) && v >= 0.;
        (0.0..=1.0).contains(&self.opacity)
            && self.backdrop_progressive.as_ref().is_none_or(ViewProgressiveBlur::is_valid)
            && self
                .transition
                .as_ref()
                .is_none_or(ViewTransition::is_valid)
            && self.animation.as_ref().is_none_or(ViewAnimation::is_valid)
            && self.transform.into_iter().all(bounded)
            && self
                .corners
                .into_iter()
                .chain(self.border_widths)
                .chain([self.outline_width])
                .all(size)
            && bounded(self.outline_offset)
            && self
                .origin
                .fraction
                .into_iter()
                .chain(self.origin.offset)
                .chain([self.origin.z])
                .all(bounded)
            && self.backgrounds.len() <= 16
            && self.masks.len() <= 16
            && self
                .backgrounds
                .iter()
                .chain(&self.masks)
                .all(ViewGradient::is_valid)
            && self.filters.len() <= 32
            && self.backdrop_filters.len() <= 32
            && self
                .filters
                .iter()
                .chain(&self.backdrop_filters)
                .all(ViewFilter::is_valid)
            && self.shadows.len() <= 32
            && self
                .shadows
                .iter()
                .all(|s| s.offset.into_iter().all(bounded) && size(s.blur) && bounded(s.spread))
    }
}
