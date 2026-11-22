//! Platform-independent visual effect values. Angles are radians; distances are points.
/// Interpolates a backdrop's sharp and filtered captures vertically. Reversing
/// the edges reverses the ramp; coincident edges produce a hard boundary.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewProgressiveBlur {
    pub start: f32,
    pub end: f32,
    pub min_mix: f32,
}
impl ViewProgressiveBlur {
    pub fn is_valid(&self) -> bool {
        [self.start, self.end, self.min_mix].into_iter().all(|v| (0.0..=1.0).contains(&v))
    }
    /// Piecewise-linear masks for nine actual filter strengths. Their weights
    /// partition unity, so premultiplied additive composition preserves alpha.
    /// The repeated position for a hard boundary deliberately preserves the step.
    pub fn sample_masks(&self) -> Option<Vec<(f32, Vec<[f32; 2]>)>> {
        if !self.is_valid() { return None; }
        let mut positions = vec![0., 1., self.start, self.end];
        if self.start != self.end && self.min_mix < 1. {
            for i in 1..8 {
                let progress = (i as f32 / 8. - self.min_mix) / (1. - self.min_mix);
                if (0. ..=1.).contains(&progress) {
                    positions.push(self.start + progress * (self.end - self.start));
                }
            }
        }
        positions.sort_by(f32::total_cmp);
        positions.dedup();
        if self.start == self.end {
            let index = positions.iter().position(|p| *p == self.start).unwrap();
            positions.insert(index, self.start);
        }
        Some((0..=8).filter_map(|i| {
            let strength = i as f32 / 8.;
            let stops: Vec<_> = positions.iter().enumerate().map(|(n, &y)| {
                let mix = if self.start == self.end && y == self.start
                    && positions.get(n + 1) == Some(&y) { self.min_mix } else { self.mix(y) };
                [y, (1. - (mix * 8. - i as f32).abs()).max(0.)]
            }).collect();
            stops.iter().any(|p| p[1] > 0.).then_some((strength, stops))
        }).collect())
    }
    pub fn mix(&self, y: f32) -> f32 {
        let progress = if self.start == self.end { if y < self.start { 0. } else { 1. } }
            else { ((y - self.start) / (self.end - self.start)).clamp(0., 1.) };
        self.min_mix + (1. - self.min_mix) * progress
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GradientStop {
    pub color: [u8; 4],
    pub position: Option<GradientPosition>,
}
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GradientPosition {
    Fraction(f32),
    Points(f32),
}
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GradientSize {
    ClosestSide,
    ClosestCorner,
    FarthestSide,
    #[default]
    FarthestCorner,
}
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GradientKind {
    Linear {
        angle: f32,
    },
    LinearCorner {
        right: bool,
        bottom: bool,
    },
    Radial {
        circle: bool,
        size: GradientSize,
        center: [f32; 2],
    },
    Conic {
        angle: f32,
        center: [f32; 2],
    },
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewGradient {
    pub geometry: GradientKind,
    pub stops: Vec<GradientStop>,
    #[serde(default)]
    pub repeating: bool,
}
impl ViewGradient {
    pub fn is_valid(&self) -> bool {
        let finite = |x: f32| x.is_finite() && x.abs() <= 1_000_000.;
        let geometry = match self.geometry {
            GradientKind::Linear { angle } => finite(angle),
            GradientKind::LinearCorner { .. } => true,
            GradientKind::Radial { center, .. } => center.into_iter().all(finite),
            GradientKind::Conic { angle, center } => {
                finite(angle) && center.into_iter().all(finite)
            }
        };
        geometry
            && (2..=256).contains(&self.stops.len())
            && self.stops.iter().all(|s| match s.position {
                Some(GradientPosition::Fraction(x) | GradientPosition::Points(x)) => finite(x),
                None => true,
            })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ViewFilter {
    Blur(f32),
    Brightness(f32),
    Contrast(f32),
    Grayscale(f32),
    HueRotate(f32),
    Invert(f32),
    Opacity(f32),
    Saturate(f32),
    Sepia(f32),
    DropShadow(super::ViewShadow),
}
impl ViewFilter {
    /// RGBA color matrix in row-major order. The fifth column uses normalized
    /// channel offsets; renderers using byte channels multiply it by 255.
    /// Spatial filters return `None` and retain their position in the filter list.
    pub fn color_matrix(self) -> Option<[f32; 20]> {
        let mut m = [1.,0.,0.,0.,0., 0.,1.,0.,0.,0., 0.,0.,1.,0.,0., 0.,0.,0.,1.,0.];
        match self {
            Self::Brightness(v) => { m[0] = v; m[6] = v; m[12] = v; }
            Self::Contrast(v) => { m[0] = v; m[6] = v; m[12] = v; m[4] = (1.-v)/2.; m[9] = m[4]; m[14] = m[4]; }
            Self::Opacity(v) => m[18] = v,
            Self::Invert(v) => { m[0] = 1.-2.*v; m[6] = m[0]; m[12] = m[0]; m[4] = v; m[9] = v; m[14] = v; }
            Self::Grayscale(v) | Self::Saturate(v) => {
                let s = if matches!(self, Self::Grayscale(_)) { 1.-v } else { v };
                for row in 0..3 { for (col, luma) in [0.2126,0.7152,0.0722].into_iter().enumerate() {
                    m[row*5+col] = luma*(1.-s) + if row == col { s } else { 0. };
                }}
            }
            Self::Sepia(v) => {
                for (row, values) in [[0.393,0.769,0.189],[0.349,0.686,0.168],[0.272,0.534,0.131]].into_iter().enumerate() {
                    for col in 0..3 { m[row*5+col] = values[col]*v + if row == col { 1.-v } else { 0. }; }
                }
            }
            Self::HueRotate(angle) => {
                let (s,c) = angle.sin_cos();
                let rows = [
                    [0.213+0.787*c-0.213*s,0.715-0.715*c-0.715*s,0.072-0.072*c+0.928*s],
                    [0.213-0.213*c+0.143*s,0.715+0.285*c+0.140*s,0.072-0.072*c-0.283*s],
                    [0.213-0.213*c-0.787*s,0.715-0.715*c+0.715*s,0.072+0.928*c+0.072*s],
                ];
                for row in 0..3 { m[row*5..row*5+3].copy_from_slice(&rows[row]); }
            }
            Self::Blur(_) | Self::DropShadow(_) => return None,
        }
        Some(m)
    }

    /// Interpolates from the identity filter without changing chain order.
    pub fn at_strength(self, strength: f32) -> Self {
        let t = strength.clamp(0., 1.);
        match self {
            Self::Blur(v) => Self::Blur(v * t),
            Self::Brightness(v) => Self::Brightness(1. + (v - 1.) * t),
            Self::Contrast(v) => Self::Contrast(1. + (v - 1.) * t),
            Self::Saturate(v) => Self::Saturate(1. + (v - 1.) * t),
            Self::Opacity(v) => Self::Opacity(1. + (v - 1.) * t),
            Self::Grayscale(v) => Self::Grayscale(v * t),
            Self::Invert(v) => Self::Invert(v * t),
            Self::Sepia(v) => Self::Sepia(v * t),
            Self::HueRotate(v) => Self::HueRotate(v * t),
            Self::DropShadow(mut shadow) => {
                shadow.blur *= t;
                shadow.offset = shadow.offset.map(|v| v * t);
                shadow.color[3] = (shadow.color[3] as f32 * t).round() as u8;
                Self::DropShadow(shadow)
            }
        }
    }

    pub fn is_valid(&self) -> bool {
        match *self {
            Self::HueRotate(x) => x.is_finite() && x.abs() <= 1_000_000.,
            Self::Grayscale(x) | Self::Invert(x) | Self::Opacity(x) | Self::Sepia(x) => {
                (0.0..=1.).contains(&x)
            }
            Self::DropShadow(s) => {
                s.offset
                    .into_iter()
                    .chain([s.spread])
                    .all(|x| x.is_finite() && x.abs() <= 1_000_000.)
                    && s.blur.is_finite()
                    && (0.0..=1_000_000.).contains(&s.blur)
                    && !s.inset
            }
            Self::Blur(x) | Self::Brightness(x) | Self::Contrast(x) | Self::Saturate(x) => {
                x.is_finite() && (0.0..=1_000_000.).contains(&x)
            }
        }
    }
}
macro_rules! values {
    ($name:ident, $default:ident, $($variant:ident),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { #[default] $default, $($variant),+ }
    };
}
values!(StrokeStyle, Solid, Dashed, Dotted);
values!(
    ViewCursor,
    Auto,
    Default,
    Pointer,
    Text,
    Crosshair,
    Grab,
    Grabbing,
    Wait,
    Progress,
    Help,
    Move,
    ZoomIn,
    ZoomOut,
    NotAllowed,
    ResizeHorizontal,
    ResizeVertical,
    ResizeNorthEastSouthWest,
    ResizeNorthWestSouthEast,
    None
);
values!(
    BlendMode,
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
    PlusDarker,
    PlusLighter
);
values!(GlassEffect, None, Regular, Clear);
values!(ControlAppearance, Native, Consistent, Plain);
values!(
    BackdropMaterial,
    Popover,
    Titlebar,
    Selection,
    Menu,
    Sidebar,
    HeaderView,
    Sheet,
    WindowBackground,
    HudWindow,
    FullScreenUi,
    ToolTip,
    ContentBackground,
    UnderWindowBackground,
    UnderPageBackground
);
values!(BackdropBlending, WithinWindow, BehindWindow);
values!(BackdropState, Active, FollowsWindowActiveState, Inactive);
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MacBackdrop {
    pub material: BackdropMaterial,
    pub blending: BackdropBlending,
    pub state: BackdropState,
}
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TransformOrigin {
    pub fraction: [f32; 2],
    pub offset: [f32; 2],
    pub z: f32,
}
impl Default for TransformOrigin {
    fn default() -> Self {
        Self {
            fraction: [0.5; 2],
            offset: [0.; 2],
            z: 0.,
        }
    }
}
