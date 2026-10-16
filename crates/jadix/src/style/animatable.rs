//! Single animatable-property table shared by CSS `transition` and `@keyframes` (ANIM-1).
//!
//! New interpolable fields go here — do **not** add more `KeyframeStop` struct fields.

use super::clip_path::ClipPath;
use super::element_style::{AnimationComposition, Style, TransitionProperty, UnitSlot};
use super::filter::{interpolate_filter_lists, FilterList};
use super::text_style::TextStyle;
use super::used_layout::UsedLayout;
use crate::layout::dimension::CssDim;
use crate::layout::{
    dim_kind, first_track_fr, lp_kind, lp_length_px, lpa_kind, lpa_length_px, set_first_track_fr,
    set_tracks_fr_list, tracks_fr_list, DimKind,
};

/// Layout / paint impact of interpolating a property (ANIM-3 damage grading).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutImpact {
    None,
    Paint,
    Layout,
}

/// CSS property id for the animation table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum PropertyId {
    Opacity = 0,
    Translate = 1,
    Scale = 2,
    Rotate = 3,
    Transform = 4,
    Filter = 5,
    BackdropFilter = 6,
    Zoom = 7,
    BackgroundColor = 8,
    BorderColor = 9,
    OutlineColor = 10,
    OutlineWidth = 11,
    OutlineOffset = 12,
    BoxShadow = 13,
    BorderRadius = 14,
    BorderTopLeftRadius = 15,
    BorderTopRightRadius = 16,
    BorderBottomRightRadius = 17,
    BorderBottomLeftRadius = 18,
    Width = 19,
    Height = 20,
    MinWidth = 21,
    MinHeight = 22,
    MaxWidth = 23,
    MaxHeight = 24,
    Padding = 25,
    PaddingTop = 26,
    PaddingRight = 27,
    PaddingBottom = 28,
    PaddingLeft = 29,
    Margin = 30,
    MarginTop = 31,
    MarginRight = 32,
    MarginBottom = 33,
    MarginLeft = 34,
    Inset = 35,
    Top = 36,
    Right = 37,
    Bottom = 38,
    Left = 39,
    Gap = 40,
    RowGap = 41,
    ColumnGap = 42,
    BorderWidth = 43,
    BorderTopWidth = 44,
    BorderRightWidth = 45,
    BorderBottomWidth = 46,
    BorderLeftWidth = 47,
    FlexGrow = 48,
    FlexShrink = 49,
    FlexBasis = 50,
    Color = 51,
    FontSize = 52,
    LineHeight = 53,
    LetterSpacing = 54,
    TextDecorationColor = 55,
    /// `grid-template-rows` `fr` list (equal-length lerp; 1-track stored as Scalar).
    GridTemplateRows = 56,
    /// `grid-template-columns` `fr` list (same interpolator as rows).
    GridTemplateColumns = 57,
    /// CSS `background-position` as `[x, y]` fractions of the padding box (0–1).
    BackgroundPosition = 58,
    /// CSS `clip-path` (inset / polygon / circle / ellipse / path / url).
    ClipPath = 59,
    /// CSS `transform-origin` as `[x, y]` fractions of the border box (0–1).
    TransformOrigin = 60,
    /// CSS `offset-distance` (0–1 along [`Style::offset_path`]).
    OffsetDistance = 61,
    /// CSS `offset-rotate` in degrees (auto tangent stored as the extra angle).
    OffsetRotate = 62,
    /// CSS `translate` as a fraction of the border box (`translate: 100%`).
    TranslatePct = 63,
}

impl PropertyId {
    pub const ALL: &'static [PropertyId] = &[
        PropertyId::Opacity,
        PropertyId::Translate,
        PropertyId::Scale,
        PropertyId::Rotate,
        PropertyId::Transform,
        PropertyId::Filter,
        PropertyId::BackdropFilter,
        PropertyId::Zoom,
        PropertyId::BackgroundColor,
        PropertyId::BorderColor,
        PropertyId::OutlineColor,
        PropertyId::OutlineWidth,
        PropertyId::OutlineOffset,
        PropertyId::BoxShadow,
        PropertyId::BorderRadius,
        PropertyId::BorderTopLeftRadius,
        PropertyId::BorderTopRightRadius,
        PropertyId::BorderBottomRightRadius,
        PropertyId::BorderBottomLeftRadius,
        PropertyId::Width,
        PropertyId::Height,
        PropertyId::MinWidth,
        PropertyId::MinHeight,
        PropertyId::MaxWidth,
        PropertyId::MaxHeight,
        PropertyId::Padding,
        PropertyId::PaddingTop,
        PropertyId::PaddingRight,
        PropertyId::PaddingBottom,
        PropertyId::PaddingLeft,
        PropertyId::Margin,
        PropertyId::MarginTop,
        PropertyId::MarginRight,
        PropertyId::MarginBottom,
        PropertyId::MarginLeft,
        PropertyId::Inset,
        PropertyId::Top,
        PropertyId::Right,
        PropertyId::Bottom,
        PropertyId::Left,
        PropertyId::Gap,
        PropertyId::RowGap,
        PropertyId::ColumnGap,
        PropertyId::BorderWidth,
        PropertyId::BorderTopWidth,
        PropertyId::BorderRightWidth,
        PropertyId::BorderBottomWidth,
        PropertyId::BorderLeftWidth,
        PropertyId::FlexGrow,
        PropertyId::FlexShrink,
        PropertyId::FlexBasis,
        PropertyId::Color,
        PropertyId::FontSize,
        PropertyId::LineHeight,
        PropertyId::LetterSpacing,
        PropertyId::TextDecorationColor,
        PropertyId::GridTemplateRows,
        PropertyId::GridTemplateColumns,
        PropertyId::BackgroundPosition,
        PropertyId::ClipPath,
        PropertyId::TransformOrigin,
        PropertyId::OffsetDistance,
        PropertyId::OffsetRotate,
        PropertyId::TranslatePct,
    ];

    pub fn bit(self) -> u64 {
        1u64 << (self as u8)
    }

    pub fn layout_impact(self) -> LayoutImpact {
        match self {
            PropertyId::Opacity
            | PropertyId::Translate
            | PropertyId::Scale
            | PropertyId::Rotate
            | PropertyId::Transform
            | PropertyId::Filter
            | PropertyId::BackdropFilter
            | PropertyId::OffsetDistance
            | PropertyId::OffsetRotate
            | PropertyId::TranslatePct => LayoutImpact::None,
            PropertyId::BackgroundColor
            | PropertyId::BorderColor
            | PropertyId::OutlineColor
            | PropertyId::OutlineWidth
            | PropertyId::OutlineOffset
            | PropertyId::BoxShadow
            | PropertyId::BorderRadius
            | PropertyId::BorderTopLeftRadius
            | PropertyId::BorderTopRightRadius
            | PropertyId::BorderBottomRightRadius
            | PropertyId::BorderBottomLeftRadius
            | PropertyId::Color
            | PropertyId::TextDecorationColor
            | PropertyId::BackgroundPosition
            | PropertyId::ClipPath
            | PropertyId::TransformOrigin => LayoutImpact::Paint,
            _ => LayoutImpact::Layout,
        }
    }

    pub fn from_css(name: &str) -> Option<Self> {
        Some(match name.trim().to_ascii_lowercase().as_str() {
            "opacity" => Self::Opacity,
            "translate" => Self::Translate,
            "scale" => Self::Scale,
            "rotate" => Self::Rotate,
            "transform" => Self::Transform,
            "filter" => Self::Filter,
            "backdrop-filter" => Self::BackdropFilter,
            "zoom" => Self::Zoom,
            "background-color" | "background" => Self::BackgroundColor,
            "border-color" => Self::BorderColor,
            "outline-color" => Self::OutlineColor,
            "outline-width" => Self::OutlineWidth,
            "outline-offset" => Self::OutlineOffset,
            "box-shadow" => Self::BoxShadow,
            "border-radius" => Self::BorderRadius,
            "border-top-left-radius" => Self::BorderTopLeftRadius,
            "border-top-right-radius" => Self::BorderTopRightRadius,
            "border-bottom-right-radius" => Self::BorderBottomRightRadius,
            "border-bottom-left-radius" => Self::BorderBottomLeftRadius,
            "width" => Self::Width,
            "height" => Self::Height,
            "min-width" => Self::MinWidth,
            "min-height" => Self::MinHeight,
            "max-width" => Self::MaxWidth,
            "max-height" => Self::MaxHeight,
            "padding" => Self::Padding,
            "padding-top" => Self::PaddingTop,
            "padding-right" => Self::PaddingRight,
            "padding-bottom" => Self::PaddingBottom,
            "padding-left" => Self::PaddingLeft,
            "margin" => Self::Margin,
            "margin-top" => Self::MarginTop,
            "margin-right" => Self::MarginRight,
            "margin-bottom" => Self::MarginBottom,
            "margin-left" => Self::MarginLeft,
            "inset" => Self::Inset,
            "top" => Self::Top,
            "right" => Self::Right,
            "bottom" => Self::Bottom,
            "left" => Self::Left,
            "gap" => Self::Gap,
            "row-gap" => Self::RowGap,
            "column-gap" => Self::ColumnGap,
            "border-width" => Self::BorderWidth,
            "border-top-width" => Self::BorderTopWidth,
            "border-right-width" => Self::BorderRightWidth,
            "border-bottom-width" => Self::BorderBottomWidth,
            "border-left-width" => Self::BorderLeftWidth,
            "flex-grow" => Self::FlexGrow,
            "flex-shrink" => Self::FlexShrink,
            "flex-basis" => Self::FlexBasis,
            "color" | "-webkit-text-fill-color" => Self::Color,
            "font-size" => Self::FontSize,
            "line-height" => Self::LineHeight,
            "letter-spacing" => Self::LetterSpacing,
            "text-decoration-color" => Self::TextDecorationColor,
            "grid-template-rows" => Self::GridTemplateRows,
            "grid-template-columns" => Self::GridTemplateColumns,
            "background-position" => Self::BackgroundPosition,
            "clip-path" => Self::ClipPath,
            "transform-origin" => Self::TransformOrigin,
            "offset-distance" => Self::OffsetDistance,
            "offset-rotate" => Self::OffsetRotate,
            _ => return None,
        })
    }

    pub fn from_transition(prop: TransitionProperty) -> Option<Self> {
        Some(match prop {
            TransitionProperty::Opacity => Self::Opacity,
            TransitionProperty::Translate => Self::Translate,
            TransitionProperty::Scale => Self::Scale,
            TransitionProperty::Rotate => Self::Rotate,
            TransitionProperty::Transform => Self::Transform,
            TransitionProperty::Filter => Self::Filter,
            TransitionProperty::BackdropFilter => Self::BackdropFilter,
            TransitionProperty::Zoom => Self::Zoom,
            TransitionProperty::BackgroundColor => Self::BackgroundColor,
            TransitionProperty::BorderColor => Self::BorderColor,
            TransitionProperty::OutlineColor => Self::OutlineColor,
            TransitionProperty::OutlineWidth => Self::OutlineWidth,
            TransitionProperty::OutlineOffset => Self::OutlineOffset,
            TransitionProperty::BoxShadow => Self::BoxShadow,
            TransitionProperty::BorderRadius => Self::BorderRadius,
            TransitionProperty::BorderTopLeftRadius => Self::BorderTopLeftRadius,
            TransitionProperty::BorderTopRightRadius => Self::BorderTopRightRadius,
            TransitionProperty::BorderBottomRightRadius => Self::BorderBottomRightRadius,
            TransitionProperty::BorderBottomLeftRadius => Self::BorderBottomLeftRadius,
            TransitionProperty::Width => Self::Width,
            TransitionProperty::Height => Self::Height,
            TransitionProperty::MinWidth => Self::MinWidth,
            TransitionProperty::MinHeight => Self::MinHeight,
            TransitionProperty::MaxWidth => Self::MaxWidth,
            TransitionProperty::MaxHeight => Self::MaxHeight,
            TransitionProperty::Padding => Self::Padding,
            TransitionProperty::PaddingTop => Self::PaddingTop,
            TransitionProperty::PaddingRight => Self::PaddingRight,
            TransitionProperty::PaddingBottom => Self::PaddingBottom,
            TransitionProperty::PaddingLeft => Self::PaddingLeft,
            TransitionProperty::Margin => Self::Margin,
            TransitionProperty::MarginTop => Self::MarginTop,
            TransitionProperty::MarginRight => Self::MarginRight,
            TransitionProperty::MarginBottom => Self::MarginBottom,
            TransitionProperty::MarginLeft => Self::MarginLeft,
            TransitionProperty::Inset => Self::Inset,
            TransitionProperty::Top => Self::Top,
            TransitionProperty::Right => Self::Right,
            TransitionProperty::Bottom => Self::Bottom,
            TransitionProperty::Left => Self::Left,
            TransitionProperty::Gap => Self::Gap,
            TransitionProperty::RowGap => Self::RowGap,
            TransitionProperty::ColumnGap => Self::ColumnGap,
            TransitionProperty::BorderWidth => Self::BorderWidth,
            TransitionProperty::BorderTopWidth => Self::BorderTopWidth,
            TransitionProperty::BorderRightWidth => Self::BorderRightWidth,
            TransitionProperty::BorderBottomWidth => Self::BorderBottomWidth,
            TransitionProperty::BorderLeftWidth => Self::BorderLeftWidth,
            TransitionProperty::FlexGrow => Self::FlexGrow,
            TransitionProperty::FlexShrink => Self::FlexShrink,
            TransitionProperty::FlexBasis => Self::FlexBasis,
            TransitionProperty::Color => Self::Color,
            TransitionProperty::FontSize => Self::FontSize,
            TransitionProperty::LineHeight => Self::LineHeight,
            TransitionProperty::LetterSpacing => Self::LetterSpacing,
            TransitionProperty::TextDecorationColor => Self::TextDecorationColor,
            TransitionProperty::GridTemplateRows => Self::GridTemplateRows,
            TransitionProperty::GridTemplateColumns => Self::GridTemplateColumns,
            TransitionProperty::BackgroundPosition => Self::BackgroundPosition,
            TransitionProperty::ClipPath => Self::ClipPath,
            TransitionProperty::TransformOrigin => Self::TransformOrigin,
            TransitionProperty::OffsetDistance => Self::OffsetDistance,
            TransitionProperty::OffsetRotate => Self::OffsetRotate,
            TransitionProperty::All | TransitionProperty::Unsupported => return None,
        })
    }
}

/// Interpolable (or discrete) value for one [`PropertyId`].
#[derive(Clone, Debug, PartialEq)]
pub enum AnimatableValue {
    Scalar(f32),
    Vec2([f32; 2]),
    Vec4([f32; 4]),
    Transform([f32; 5]),
    Shadow([f32; 8]),
    Color([f32; 4]),
    Filter(Option<FilterList>),
    /// Equal-length `fr` track list (`grid-template-rows` / `columns`, 2+ tracks).
    FrList(Vec<f32>),
    /// CSS `clip-path` geometry.
    Clip(ClipPath),
    /// Unevaluated `calc-size()` / keyword size (resolved via [`resolve_used`]).
    Dim(CssDim),
    /// `auto` / `%` / inherit — not continuously interpolable.
    Auto,
}

pub fn values_close(a: &AnimatableValue, b: &AnimatableValue) -> bool {
    match (a, b) {
        (AnimatableValue::Scalar(x), AnimatableValue::Scalar(y)) => (x - y).abs() <= 1e-4,
        (AnimatableValue::Vec2(x), AnimatableValue::Vec2(y)) => {
            (x[0] - y[0]).abs() <= 1e-4 && (x[1] - y[1]).abs() <= 1e-4
        }
        (AnimatableValue::Vec4(x), AnimatableValue::Vec4(y))
        | (AnimatableValue::Color(x), AnimatableValue::Color(y)) => {
            (0..4).all(|i| (x[i] - y[i]).abs() <= 1e-4)
        }
        (AnimatableValue::Transform(x), AnimatableValue::Transform(y)) => {
            (0..5).all(|i| (x[i] - y[i]).abs() <= 1e-4)
        }
        (AnimatableValue::Shadow(x), AnimatableValue::Shadow(y)) => {
            (0..8).all(|i| (x[i] - y[i]).abs() <= 1e-4)
        }
        (AnimatableValue::Filter(x), AnimatableValue::Filter(y)) => x == y,
        (AnimatableValue::FrList(x), AnimatableValue::FrList(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(a, b)| (a - b).abs() <= 1e-4)
        }
        (AnimatableValue::Clip(a), AnimatableValue::Clip(b)) => a == b,
        (AnimatableValue::Dim(a), AnimatableValue::Dim(b)) => a == b,
        (AnimatableValue::Auto, AnimatableValue::Auto) => true,
        _ => false,
    }
}

pub fn interpolate(a: &AnimatableValue, b: &AnimatableValue, t: f32) -> Option<AnimatableValue> {
    let t = t.clamp(0.0, 1.0);
    let lerp = |x: f32, y: f32| x + (y - x) * t;
    match (a, b) {
        (AnimatableValue::Scalar(x), AnimatableValue::Scalar(y)) => {
            Some(AnimatableValue::Scalar(lerp(*x, *y)))
        }
        (AnimatableValue::Vec2(x), AnimatableValue::Vec2(y)) => {
            Some(AnimatableValue::Vec2([lerp(x[0], y[0]), lerp(x[1], y[1])]))
        }
        (AnimatableValue::Vec4(x), AnimatableValue::Vec4(y)) => Some(AnimatableValue::Vec4([
            lerp(x[0], y[0]),
            lerp(x[1], y[1]),
            lerp(x[2], y[2]),
            lerp(x[3], y[3]),
        ])),
        (AnimatableValue::Color(x), AnimatableValue::Color(y)) => {
            // Linear sRGB mix via existing helper.
            let ua = f32_color_to_u8(*x);
            let ub = f32_color_to_u8(*y);
            let m = super::color_space::lerp_text_rgba(ua, ub, t);
            Some(AnimatableValue::Color(u8_color_to_f32(m)))
        }
        (AnimatableValue::Transform(x), AnimatableValue::Transform(y)) => {
            let mut o = [0.0; 5];
            for i in 0..5 {
                o[i] = lerp(x[i], y[i]);
            }
            Some(AnimatableValue::Transform(o))
        }
        (AnimatableValue::Shadow(x), AnimatableValue::Shadow(y)) => {
            let mut o = [0.0; 8];
            for i in 0..8 {
                o[i] = lerp(x[i], y[i]);
            }
            Some(AnimatableValue::Shadow(o))
        }
        (AnimatableValue::Filter(x), AnimatableValue::Filter(y)) => {
            match interpolate_filter_lists(x.as_ref(), y.as_ref(), t) {
                Some(list) => Some(AnimatableValue::Filter(if list.is_empty() {
                    None
                } else {
                    Some(list)
                })),
                None => {
                    if t >= 1.0 - 1e-6 {
                        Some(AnimatableValue::Filter(y.clone()))
                    } else {
                        Some(AnimatableValue::Filter(x.clone()))
                    }
                }
            }
        }
        (AnimatableValue::FrList(x), AnimatableValue::FrList(y)) if x.len() == y.len() => Some(
            AnimatableValue::FrList(x.iter().zip(y.iter()).map(|(a, b)| lerp(*a, *b)).collect()),
        ),
        (AnimatableValue::FrList(x), AnimatableValue::FrList(y)) => {
            if t >= 1.0 - 1e-6 {
                Some(AnimatableValue::FrList(y.clone()))
            } else {
                Some(AnimatableValue::FrList(x.clone()))
            }
        }
        (AnimatableValue::Scalar(s), AnimatableValue::FrList(y)) if y.len() == 1 => {
            Some(AnimatableValue::Scalar(lerp(*s, y[0])))
        }
        (AnimatableValue::FrList(x), AnimatableValue::Scalar(s)) if x.len() == 1 => {
            Some(AnimatableValue::Scalar(lerp(x[0], *s)))
        }
        (AnimatableValue::Auto, AnimatableValue::Scalar(y)) => {
            Some(AnimatableValue::Scalar(lerp(0.0, *y)))
        }
        (AnimatableValue::Scalar(x), AnimatableValue::Auto) => {
            Some(AnimatableValue::Scalar(lerp(*x, 0.0)))
        }
        (AnimatableValue::Clip(a), AnimatableValue::Clip(b)) => {
            Some(AnimatableValue::Clip(a.interpolate(b, t)))
        }
        (AnimatableValue::Vec4(inset), AnimatableValue::Clip(b)) => {
            let a = ClipPath::Inset {
                trbl: [
                    super::clip_path::ClipCoord::Px(inset[0]),
                    super::clip_path::ClipCoord::Px(inset[1]),
                    super::clip_path::ClipCoord::Px(inset[2]),
                    super::clip_path::ClipCoord::Px(inset[3]),
                ],
            };
            Some(AnimatableValue::Clip(a.interpolate(b, t)))
        }
        (AnimatableValue::Clip(a), AnimatableValue::Vec4(inset)) => {
            let b = ClipPath::Inset {
                trbl: [
                    super::clip_path::ClipCoord::Px(inset[0]),
                    super::clip_path::ClipCoord::Px(inset[1]),
                    super::clip_path::ClipCoord::Px(inset[2]),
                    super::clip_path::ClipCoord::Px(inset[3]),
                ],
            };
            Some(AnimatableValue::Clip(a.interpolate(&b, t)))
        }
        _ => None,
    }
}

/// CSS `animation-composition`: combine the keyframe effect with the current underlying value.
///
/// `add` / `accumulate` share the same MVP: scalars/colors/vectors add; transform
/// translates and rotation add while scale multiplies. Filter lists stay replace-only.
pub fn compose(
    mode: AnimationComposition,
    underlying: &AnimatableValue,
    effect: &AnimatableValue,
) -> AnimatableValue {
    match mode {
        AnimationComposition::Replace => effect.clone(),
        AnimationComposition::Add | AnimationComposition::Accumulate => {
            add_values(underlying, effect).unwrap_or_else(|| effect.clone())
        }
    }
}

fn add_values(a: &AnimatableValue, b: &AnimatableValue) -> Option<AnimatableValue> {
    match (a, b) {
        (AnimatableValue::Scalar(x), AnimatableValue::Scalar(y)) => {
            Some(AnimatableValue::Scalar(x + y))
        }
        (AnimatableValue::Vec2(x), AnimatableValue::Vec2(y)) => {
            Some(AnimatableValue::Vec2([x[0] + y[0], x[1] + y[1]]))
        }
        (AnimatableValue::Vec4(x), AnimatableValue::Vec4(y)) => Some(AnimatableValue::Vec4([
            x[0] + y[0],
            x[1] + y[1],
            x[2] + y[2],
            x[3] + y[3],
        ])),
        (AnimatableValue::Color(x), AnimatableValue::Color(y)) => Some(AnimatableValue::Color([
            x[0] + y[0],
            x[1] + y[1],
            x[2] + y[2],
            x[3] + y[3],
        ])),
        (AnimatableValue::Transform(u), AnimatableValue::Transform(e)) => {
            Some(AnimatableValue::Transform([
                u[0] + e[0],
                u[1] + e[1],
                u[2] * e[2],
                u[3] * e[3],
                u[4] + e[4],
            ]))
        }
        (AnimatableValue::Shadow(x), AnimatableValue::Shadow(y)) => {
            let mut o = [0.0; 8];
            for i in 0..8 {
                o[i] = x[i] + y[i];
            }
            Some(AnimatableValue::Shadow(o))
        }
        (AnimatableValue::FrList(x), AnimatableValue::FrList(y)) if x.len() == y.len() => Some(
            AnimatableValue::FrList(x.iter().zip(y.iter()).map(|(a, b)| a + b).collect()),
        ),
        (AnimatableValue::Scalar(s), AnimatableValue::FrList(y)) if y.len() == 1 => {
            Some(AnimatableValue::Scalar(s + y[0]))
        }
        (AnimatableValue::FrList(x), AnimatableValue::Scalar(s)) if x.len() == 1 => {
            Some(AnimatableValue::Scalar(x[0] + s))
        }
        _ => None,
    }
}

fn f32_color_to_u8(c: [f32; 4]) -> [u8; 4] {
    [
        (c[0] * 255.0).round().clamp(0.0, 255.0) as u8,
        (c[1] * 255.0).round().clamp(0.0, 255.0) as u8,
        (c[2] * 255.0).round().clamp(0.0, 255.0) as u8,
        (c[3] * 255.0).round().clamp(0.0, 255.0) as u8,
    ]
}

fn u8_color_to_f32(c: [u8; 4]) -> [f32; 4] {
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
        c[3] as f32 / 255.0,
    ]
}

/// Replace `auto` size keywords with used-layout px for implicit keyframe ends.
pub fn resolve_used(id: PropertyId, v: AnimatableValue, used: &UsedLayout) -> AnimatableValue {
    if let AnimatableValue::Dim(d) = &v {
        if d.is_calc_size() || d.contains_size_keyword() || matches!(d, CssDim::Auto) {
            let size = match id {
                PropertyId::MaxHeight | PropertyId::Height | PropertyId::MinHeight => {
                    used.anim_height()
                }
                PropertyId::MaxWidth | PropertyId::Width | PropertyId::MinWidth => {
                    used.anim_width()
                }
                _ => 0.0,
            };
            let axis = match id {
                PropertyId::MaxHeight | PropertyId::Height | PropertyId::MinHeight => {
                    crate::layout::dimension::PercentAxis::Block
                }
                _ => crate::layout::dimension::PercentAxis::Inline,
            };
            let ctx = crate::layout::dimension::UnitCtx::new(0.0, 0.0, 16.0);
            if let Some(px) = d.eval_calc_size(size, &ctx, axis) {
                return AnimatableValue::Scalar(px);
            }
        }
    }
    if !matches!(v, AnimatableValue::Auto) {
        return v;
    }
    match id {
        PropertyId::MaxHeight | PropertyId::Height | PropertyId::MinHeight => {
            AnimatableValue::Scalar(used.anim_height())
        }
        PropertyId::MaxWidth | PropertyId::Width | PropertyId::MinWidth => {
            AnimatableValue::Scalar(used.anim_width())
        }
        _ => v,
    }
}

fn dim_value(d: taffy::style::Dimension) -> AnimatableValue {
    match dim_kind(d) {
        DimKind::Length(px) => AnimatableValue::Scalar(px),
        _ => AnimatableValue::Auto,
    }
}

fn lp_value(d: taffy::style::LengthPercentage) -> AnimatableValue {
    match lp_kind(d) {
        DimKind::Length(px) => AnimatableValue::Scalar(px),
        _ => AnimatableValue::Auto,
    }
}

fn lpa_value(d: taffy::style::LengthPercentageAuto) -> AnimatableValue {
    match lpa_kind(d) {
        DimKind::Length(px) => AnimatableValue::Scalar(px),
        _ => AnimatableValue::Auto,
    }
}

pub fn read_value(style: &Style, text: &TextStyle, id: PropertyId) -> AnimatableValue {
    match id {
        PropertyId::Opacity => AnimatableValue::Scalar(style.opacity),
        PropertyId::Translate => AnimatableValue::Vec2(style.translate),
        PropertyId::Scale => AnimatableValue::Vec2(style.scale),
        PropertyId::Rotate => AnimatableValue::Scalar(style.rotate_deg),
        PropertyId::Transform => AnimatableValue::Transform([
            style.translate[0],
            style.translate[1],
            style.scale[0],
            style.scale[1],
            style.rotate_deg,
        ]),
        PropertyId::Filter => AnimatableValue::Filter(style.filter.clone()),
        PropertyId::BackdropFilter => AnimatableValue::Scalar(style.backdrop_blur),
        PropertyId::Zoom => AnimatableValue::Scalar(style.zoom),
        PropertyId::BackgroundColor => AnimatableValue::Color(style.bg_color),
        PropertyId::BorderColor => AnimatableValue::Color(style.border_color),
        PropertyId::OutlineColor => AnimatableValue::Color(style.outline_color),
        PropertyId::OutlineWidth => AnimatableValue::Scalar(style.outline_width),
        PropertyId::OutlineOffset => AnimatableValue::Scalar(style.outline_offset),
        PropertyId::BoxShadow => AnimatableValue::Shadow([
            style.shadow_color[0],
            style.shadow_color[1],
            style.shadow_color[2],
            style.shadow_color[3],
            style.shadow_offset[0],
            style.shadow_offset[1],
            style.shadow_blur,
            style.shadow_spread,
        ]),
        PropertyId::BorderRadius => AnimatableValue::Vec4(style.radii),
        PropertyId::BorderTopLeftRadius => AnimatableValue::Scalar(style.radii[0]),
        PropertyId::BorderTopRightRadius => AnimatableValue::Scalar(style.radii[1]),
        PropertyId::BorderBottomRightRadius => AnimatableValue::Scalar(style.radii[2]),
        PropertyId::BorderBottomLeftRadius => AnimatableValue::Scalar(style.radii[3]),
        PropertyId::Width => dim_value(style.size.width),
        PropertyId::Height => dim_value(style.size.height),
        PropertyId::MinWidth => dim_value(style.min_size.width),
        PropertyId::MinHeight => dim_value(style.min_size.height),
        PropertyId::MaxWidth => dim_value(style.max_size.width),
        PropertyId::MaxHeight => dim_value(style.max_size.height),
        PropertyId::Padding => AnimatableValue::Vec4([
            lp_length_px(style.padding.top).unwrap_or(0.0),
            lp_length_px(style.padding.right).unwrap_or(0.0),
            lp_length_px(style.padding.bottom).unwrap_or(0.0),
            lp_length_px(style.padding.left).unwrap_or(0.0),
        ]),
        PropertyId::PaddingTop => lp_value(style.padding.top),
        PropertyId::PaddingRight => lp_value(style.padding.right),
        PropertyId::PaddingBottom => lp_value(style.padding.bottom),
        PropertyId::PaddingLeft => lp_value(style.padding.left),
        PropertyId::Margin => AnimatableValue::Vec4([
            lpa_length_px(style.margin.top).unwrap_or(0.0),
            lpa_length_px(style.margin.right).unwrap_or(0.0),
            lpa_length_px(style.margin.bottom).unwrap_or(0.0),
            lpa_length_px(style.margin.left).unwrap_or(0.0),
        ]),
        PropertyId::MarginTop => lpa_value(style.margin.top),
        PropertyId::MarginRight => lpa_value(style.margin.right),
        PropertyId::MarginBottom => lpa_value(style.margin.bottom),
        PropertyId::MarginLeft => lpa_value(style.margin.left),
        PropertyId::Inset => AnimatableValue::Vec4([
            lpa_length_px(style.inset.top).unwrap_or(0.0),
            lpa_length_px(style.inset.right).unwrap_or(0.0),
            lpa_length_px(style.inset.bottom).unwrap_or(0.0),
            lpa_length_px(style.inset.left).unwrap_or(0.0),
        ]),
        PropertyId::Top => lpa_value(style.inset.top),
        PropertyId::Right => lpa_value(style.inset.right),
        PropertyId::Bottom => lpa_value(style.inset.bottom),
        PropertyId::Left => lpa_value(style.inset.left),
        PropertyId::Gap => AnimatableValue::Vec2([
            lp_length_px(style.gap.width).unwrap_or(0.0),
            lp_length_px(style.gap.height).unwrap_or(0.0),
        ]),
        PropertyId::ColumnGap => lp_value(style.gap.width),
        PropertyId::RowGap => lp_value(style.gap.height),
        PropertyId::BorderWidth => {
            let w = style.border_width_sides.unwrap_or([style.border_width; 4]);
            AnimatableValue::Vec4(w)
        }
        PropertyId::BorderTopWidth => {
            AnimatableValue::Scalar(style.border_width_sides.unwrap_or([style.border_width; 4])[0])
        }
        PropertyId::BorderRightWidth => {
            AnimatableValue::Scalar(style.border_width_sides.unwrap_or([style.border_width; 4])[1])
        }
        PropertyId::BorderBottomWidth => {
            AnimatableValue::Scalar(style.border_width_sides.unwrap_or([style.border_width; 4])[2])
        }
        PropertyId::BorderLeftWidth => {
            AnimatableValue::Scalar(style.border_width_sides.unwrap_or([style.border_width; 4])[3])
        }
        PropertyId::FlexGrow => AnimatableValue::Scalar(style.flex_grow),
        PropertyId::FlexShrink => AnimatableValue::Scalar(style.flex_shrink),
        PropertyId::FlexBasis => dim_value(style.flex_basis),
        PropertyId::Color => match text.color {
            Some(c) => AnimatableValue::Color(u8_color_to_f32(c)),
            None => AnimatableValue::Auto,
        },
        PropertyId::FontSize => match text.size {
            Some(s) => AnimatableValue::Scalar(s),
            None => AnimatableValue::Auto,
        },
        PropertyId::LineHeight => match text.line_height_px {
            Some(h) => AnimatableValue::Scalar(h),
            None => AnimatableValue::Auto,
        },
        PropertyId::LetterSpacing => AnimatableValue::Scalar(text.letter_spacing),
        PropertyId::TextDecorationColor => match text.decoration_color {
            Some(c) => AnimatableValue::Color(u8_color_to_f32(c)),
            None => AnimatableValue::Auto,
        },
        PropertyId::GridTemplateRows => grid_tracks_value(&style.grid_template_rows),
        PropertyId::GridTemplateColumns => grid_tracks_value(&style.grid_template_columns),
        PropertyId::BackgroundPosition => AnimatableValue::Vec2(style.bg_position),
        PropertyId::ClipPath => match &style.clip_path {
            ClipPath::None => AnimatableValue::Auto,
            clip => AnimatableValue::Clip(clip.clone()),
        },
        PropertyId::TransformOrigin => {
            AnimatableValue::Vec2(super::transform::origin_frac(&style.transform_origin))
        }
        PropertyId::OffsetDistance => AnimatableValue::Scalar(style.offset_distance),
        PropertyId::OffsetRotate => AnimatableValue::Scalar(style.offset_rotate_deg.unwrap_or(0.0)),
        PropertyId::TranslatePct => AnimatableValue::Vec2(style.translate_pct),
    }
}

fn grid_tracks_value(tracks: &[crate::layout::GridTrack]) -> AnimatableValue {
    match tracks_fr_list(tracks) {
        Some(list) if list.len() == 1 => AnimatableValue::Scalar(list[0]),
        Some(list) => AnimatableValue::FrList(list),
        None => match first_track_fr(tracks) {
            Some(fr) => AnimatableValue::Scalar(fr),
            None => AnimatableValue::Auto,
        },
    }
}

fn write_grid_tracks(tracks: &mut Vec<crate::layout::GridTrack>, v: &AnimatableValue) {
    match v {
        AnimatableValue::Scalar(x) => set_first_track_fr(tracks, x.max(0.0)),
        AnimatableValue::FrList(list) => set_tracks_fr_list(tracks, list),
        AnimatableValue::Vec2(xy) => set_tracks_fr_list(tracks, &[xy[0].max(0.0), xy[1].max(0.0)]),
        AnimatableValue::Vec4(a) => set_tracks_fr_list(
            tracks,
            &[a[0].max(0.0), a[1].max(0.0), a[2].max(0.0), a[3].max(0.0)],
        ),
        _ => {}
    }
}

fn write_dim(style: &mut Style, slot: UnitSlot, px: f32, nonneg: bool) {
    let v = if nonneg { px.max(0.0) } else { px };
    let slot = super::element_style::physical_unit_slot(slot, style.writing_mode, style.direction);
    style.unit_specs.retain(|(s, _)| *s != slot);
    match slot {
        UnitSlot::Width => style.size.width = taffy::style::Dimension::length(v),
        UnitSlot::Height => style.size.height = taffy::style::Dimension::length(v),
        UnitSlot::MinWidth => style.min_size.width = taffy::style::Dimension::length(v),
        UnitSlot::MinHeight => style.min_size.height = taffy::style::Dimension::length(v),
        UnitSlot::MaxWidth => style.max_size.width = taffy::style::Dimension::length(v),
        UnitSlot::MaxHeight => style.max_size.height = taffy::style::Dimension::length(v),
        UnitSlot::FlexBasis => style.flex_basis = taffy::style::Dimension::length(v),
        UnitSlot::PadT => style.padding.top = taffy::style::LengthPercentage::length(v.max(0.0)),
        UnitSlot::PadR => style.padding.right = taffy::style::LengthPercentage::length(v.max(0.0)),
        UnitSlot::PadB => style.padding.bottom = taffy::style::LengthPercentage::length(v.max(0.0)),
        UnitSlot::PadL => style.padding.left = taffy::style::LengthPercentage::length(v.max(0.0)),
        UnitSlot::MarginT => style.margin.top = taffy::style::LengthPercentageAuto::length(v),
        UnitSlot::MarginR => style.margin.right = taffy::style::LengthPercentageAuto::length(v),
        UnitSlot::MarginB => style.margin.bottom = taffy::style::LengthPercentageAuto::length(v),
        UnitSlot::MarginL => style.margin.left = taffy::style::LengthPercentageAuto::length(v),
        UnitSlot::GapX => style.gap.width = taffy::style::LengthPercentage::length(v.max(0.0)),
        UnitSlot::GapY => style.gap.height = taffy::style::LengthPercentage::length(v.max(0.0)),
        UnitSlot::InsetT => style.inset.top = taffy::style::LengthPercentageAuto::length(v),
        UnitSlot::InsetR => style.inset.right = taffy::style::LengthPercentageAuto::length(v),
        UnitSlot::InsetB => style.inset.bottom = taffy::style::LengthPercentageAuto::length(v),
        UnitSlot::InsetL => style.inset.left = taffy::style::LengthPercentageAuto::length(v),
        _ => {}
    }
}

pub fn write_value(style: &mut Style, text: &mut TextStyle, id: PropertyId, v: &AnimatableValue) {
    match (id, v) {
        (PropertyId::Opacity, AnimatableValue::Scalar(x)) => style.opacity = *x,
        (PropertyId::Translate, AnimatableValue::Vec2(xy)) => style.translate = *xy,
        (PropertyId::TranslatePct, AnimatableValue::Vec2(xy)) => style.translate_pct = *xy,
        (PropertyId::Scale, AnimatableValue::Vec2(xy)) => style.scale = *xy,
        (PropertyId::Rotate, AnimatableValue::Scalar(x)) => style.rotate_deg = *x,
        (PropertyId::Transform, AnimatableValue::Transform(t)) => {
            style.translate = [t[0], t[1]];
            style.scale = [t[2], t[3]];
            style.rotate_deg = t[4];
        }
        (PropertyId::Filter, AnimatableValue::Filter(f)) => style.filter = f.clone(),
        (PropertyId::BackdropFilter, AnimatableValue::Scalar(x)) => {
            style.backdrop_blur = x.max(0.0)
        }
        (PropertyId::Zoom, AnimatableValue::Scalar(x)) => {
            style.zoom = if x.is_finite() && *x > 0.0 { *x } else { 1.0 };
        }
        (PropertyId::BackgroundColor, AnimatableValue::Color(c)) => style.bg_color = *c,
        (PropertyId::BorderColor, AnimatableValue::Color(c)) => style.border_color = *c,
        (PropertyId::OutlineColor, AnimatableValue::Color(c)) => style.outline_color = *c,
        (PropertyId::OutlineWidth, AnimatableValue::Scalar(x)) => style.outline_width = *x,
        (PropertyId::OutlineOffset, AnimatableValue::Scalar(x)) => style.outline_offset = *x,
        (PropertyId::BoxShadow, AnimatableValue::Shadow(s)) => {
            style.shadow_color = [s[0], s[1], s[2], s[3]];
            style.shadow_offset = [s[4], s[5]];
            style.shadow_blur = s[6].max(0.0);
            style.shadow_spread = s[7];
        }
        (PropertyId::BorderRadius, AnimatableValue::Vec4(r)) => {
            style.radii = [r[0].max(0.0), r[1].max(0.0), r[2].max(0.0), r[3].max(0.0)];
        }
        (PropertyId::BorderTopLeftRadius, AnimatableValue::Scalar(x)) => {
            style.radii[0] = x.max(0.0)
        }
        (PropertyId::BorderTopRightRadius, AnimatableValue::Scalar(x)) => {
            style.radii[1] = x.max(0.0)
        }
        (PropertyId::BorderBottomRightRadius, AnimatableValue::Scalar(x)) => {
            style.radii[2] = x.max(0.0)
        }
        (PropertyId::BorderBottomLeftRadius, AnimatableValue::Scalar(x)) => {
            style.radii[3] = x.max(0.0)
        }
        (PropertyId::Width, AnimatableValue::Scalar(x)) => {
            write_dim(style, UnitSlot::Width, *x, true)
        }
        (PropertyId::Height, AnimatableValue::Scalar(x)) => {
            write_dim(style, UnitSlot::Height, *x, true)
        }
        (PropertyId::MinWidth, AnimatableValue::Scalar(x)) => {
            write_dim(style, UnitSlot::MinWidth, *x, true)
        }
        (PropertyId::MinHeight, AnimatableValue::Scalar(x)) => {
            write_dim(style, UnitSlot::MinHeight, *x, true)
        }
        (PropertyId::MaxWidth, AnimatableValue::Scalar(x)) => {
            write_dim(style, UnitSlot::MaxWidth, *x, true)
        }
        (PropertyId::MaxHeight, AnimatableValue::Scalar(x)) => {
            write_dim(style, UnitSlot::MaxHeight, *x, true)
        }
        (PropertyId::Padding, AnimatableValue::Vec4(p)) => {
            write_dim(style, UnitSlot::PadT, p[0], true);
            write_dim(style, UnitSlot::PadR, p[1], true);
            write_dim(style, UnitSlot::PadB, p[2], true);
            write_dim(style, UnitSlot::PadL, p[3], true);
        }
        (PropertyId::PaddingTop, AnimatableValue::Scalar(x)) => {
            write_dim(style, UnitSlot::PadT, *x, true)
        }
        (PropertyId::PaddingRight, AnimatableValue::Scalar(x)) => {
            write_dim(style, UnitSlot::PadR, *x, true)
        }
        (PropertyId::PaddingBottom, AnimatableValue::Scalar(x)) => {
            write_dim(style, UnitSlot::PadB, *x, true)
        }
        (PropertyId::PaddingLeft, AnimatableValue::Scalar(x)) => {
            write_dim(style, UnitSlot::PadL, *x, true)
        }
        (PropertyId::Margin, AnimatableValue::Vec4(m)) => {
            write_dim(style, UnitSlot::MarginT, m[0], false);
            write_dim(style, UnitSlot::MarginR, m[1], false);
            write_dim(style, UnitSlot::MarginB, m[2], false);
            write_dim(style, UnitSlot::MarginL, m[3], false);
        }
        (PropertyId::MarginTop, AnimatableValue::Scalar(x)) => {
            write_dim(style, UnitSlot::MarginT, *x, false)
        }
        (PropertyId::MarginRight, AnimatableValue::Scalar(x)) => {
            write_dim(style, UnitSlot::MarginR, *x, false)
        }
        (PropertyId::MarginBottom, AnimatableValue::Scalar(x)) => {
            write_dim(style, UnitSlot::MarginB, *x, false)
        }
        (PropertyId::MarginLeft, AnimatableValue::Scalar(x)) => {
            write_dim(style, UnitSlot::MarginL, *x, false)
        }
        (PropertyId::Inset, AnimatableValue::Vec4(i)) => {
            write_dim(style, UnitSlot::InsetT, i[0], false);
            write_dim(style, UnitSlot::InsetR, i[1], false);
            write_dim(style, UnitSlot::InsetB, i[2], false);
            write_dim(style, UnitSlot::InsetL, i[3], false);
        }
        (PropertyId::Top, AnimatableValue::Scalar(x)) => {
            write_dim(style, UnitSlot::InsetT, *x, false)
        }
        (PropertyId::Right, AnimatableValue::Scalar(x)) => {
            write_dim(style, UnitSlot::InsetR, *x, false)
        }
        (PropertyId::Bottom, AnimatableValue::Scalar(x)) => {
            write_dim(style, UnitSlot::InsetB, *x, false)
        }
        (PropertyId::Left, AnimatableValue::Scalar(x)) => {
            write_dim(style, UnitSlot::InsetL, *x, false)
        }
        (PropertyId::Gap, AnimatableValue::Vec2(g)) => {
            write_dim(style, UnitSlot::GapX, g[0], true);
            write_dim(style, UnitSlot::GapY, g[1], true);
        }
        (PropertyId::ColumnGap, AnimatableValue::Scalar(x)) => {
            write_dim(style, UnitSlot::GapX, *x, true)
        }
        (PropertyId::RowGap, AnimatableValue::Scalar(x)) => {
            write_dim(style, UnitSlot::GapY, *x, true)
        }
        (PropertyId::BorderWidth, AnimatableValue::Vec4(w)) => {
            style.set_border_widths_trbl([w[0], w[1], w[2], w[3]])
        }
        (PropertyId::BorderTopWidth, AnimatableValue::Scalar(x)) => {
            style.set_border_side_width(0, *x)
        }
        (PropertyId::BorderRightWidth, AnimatableValue::Scalar(x)) => {
            style.set_border_side_width(1, *x)
        }
        (PropertyId::BorderBottomWidth, AnimatableValue::Scalar(x)) => {
            style.set_border_side_width(2, *x)
        }
        (PropertyId::BorderLeftWidth, AnimatableValue::Scalar(x)) => {
            style.set_border_side_width(3, *x)
        }
        (PropertyId::FlexGrow, AnimatableValue::Scalar(x)) => style.flex_grow = x.max(0.0),
        (PropertyId::FlexShrink, AnimatableValue::Scalar(x)) => style.flex_shrink = x.max(0.0),
        (PropertyId::FlexBasis, AnimatableValue::Scalar(x)) => {
            write_dim(style, UnitSlot::FlexBasis, *x, true)
        }
        (PropertyId::Color, AnimatableValue::Color(c)) => {
            text.color = Some(f32_color_to_u8(*c));
        }
        (PropertyId::FontSize, AnimatableValue::Scalar(x)) => text.size = Some(x.max(0.0)),
        (PropertyId::LineHeight, AnimatableValue::Scalar(x)) => {
            text.line_height_px = Some(x.max(0.0))
        }
        (PropertyId::LetterSpacing, AnimatableValue::Scalar(x)) => {
            text.letter_spacing = *x;
            text.letter_spacing_specified = true;
        }
        (PropertyId::TextDecorationColor, AnimatableValue::Color(c)) => {
            text.decoration_color = Some(f32_color_to_u8(*c));
            text.decoration_color_specified = true;
        }
        (PropertyId::GridTemplateRows, v) => write_grid_tracks(&mut style.grid_template_rows, v),
        (PropertyId::GridTemplateColumns, v) => {
            write_grid_tracks(&mut style.grid_template_columns, v)
        }
        (PropertyId::BackgroundPosition, AnimatableValue::Vec2(xy)) => style.bg_position = *xy,
        (PropertyId::ClipPath, AnimatableValue::Clip(p)) => style.clip_path = p.clone(),
        (PropertyId::ClipPath, AnimatableValue::Vec4(inset)) => {
            style.clip_path = ClipPath::Inset {
                trbl: [
                    super::clip_path::ClipCoord::Px(inset[0].max(0.0)),
                    super::clip_path::ClipCoord::Px(inset[1].max(0.0)),
                    super::clip_path::ClipCoord::Px(inset[2].max(0.0)),
                    super::clip_path::ClipCoord::Px(inset[3].max(0.0)),
                ],
            };
        }
        (PropertyId::ClipPath, AnimatableValue::Auto) => style.clip_path = ClipPath::None,
        (PropertyId::TransformOrigin, AnimatableValue::Vec2(xy)) => {
            super::transform::set_origin_frac(&mut style.transform_origin, *xy);
        }
        (PropertyId::OffsetDistance, AnimatableValue::Scalar(x)) => {
            style.offset_distance = x.clamp(0.0, 1.0);
        }
        (PropertyId::OffsetRotate, AnimatableValue::Scalar(x)) => {
            style.offset_rotate_deg = Some(*x);
        }
        (PropertyId::Width, AnimatableValue::Dim(d))
        | (PropertyId::Height, AnimatableValue::Dim(d))
        | (PropertyId::MinWidth, AnimatableValue::Dim(d))
        | (PropertyId::MinHeight, AnimatableValue::Dim(d))
        | (PropertyId::MaxWidth, AnimatableValue::Dim(d))
        | (PropertyId::MaxHeight, AnimatableValue::Dim(d)) => {
            let slot = match id {
                PropertyId::Width => UnitSlot::Width,
                PropertyId::Height => UnitSlot::Height,
                PropertyId::MinWidth => UnitSlot::MinWidth,
                PropertyId::MinHeight => UnitSlot::MinHeight,
                PropertyId::MaxWidth => UnitSlot::MaxWidth,
                PropertyId::MaxHeight => UnitSlot::MaxHeight,
                _ => return,
            };
            style.set_css_dim(slot, d.clone());
        }
        _ => {}
    }
}

/// Bitset of [`PropertyId`] (fits in one `u64`; keep `PropertyId` discriminants < 64).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PropertySet {
    bits: u64,
}

impl PropertySet {
    pub fn insert(&mut self, id: PropertyId) {
        self.bits |= id.bit();
    }

    pub fn contains(self, id: PropertyId) -> bool {
        self.bits & id.bit() != 0
    }

    pub fn is_empty(self) -> bool {
        self.bits == 0
    }

    pub fn union_with(&mut self, other: PropertySet) {
        self.bits |= other.bits;
    }

    pub fn iter(self) -> impl Iterator<Item = PropertyId> {
        PropertyId::ALL
            .iter()
            .copied()
            .filter(move |id| self.contains(*id))
    }
}

pub fn parse_css_length_px(s: &str) -> Option<f32> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("0") {
        return Some(0.0);
    }
    if let Some(n) = s.strip_suffix("px") {
        return n.trim().parse().ok();
    }
    if let Some(n) = s.strip_suffix("rem") {
        return n.trim().parse::<f32>().ok().map(|v| v * 16.0);
    }
    s.parse().ok()
}

/// Parse `1fr`, `0fr 1fr`, … into Scalar (1 track) or FrList (2+).
pub fn parse_fr_tracks(raw: &str) -> Option<AnimatableValue> {
    let mut parts = Vec::new();
    for tok in raw.split_whitespace() {
        parts.push(parse_one_fr(tok)?);
    }
    match parts.len() {
        0 => None,
        1 => Some(AnimatableValue::Scalar(parts[0])),
        _ => Some(AnimatableValue::FrList(parts)),
    }
}

fn parse_one_fr(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(n) = s.strip_suffix("fr") {
        return n.trim().parse::<f32>().ok().map(|v| v.max(0.0));
    }
    None
}

/// CSS `background-position` → `[x, y]` in 0–1 (keywords / percentages).
pub fn parse_background_position(raw: &str) -> Option<[f32; 2]> {
    let s = raw.trim().to_ascii_lowercase();
    if s.is_empty() {
        return None;
    }
    if s == "center" {
        return Some([0.5, 0.5]);
    }
    let toks: Vec<&str> = s.split_whitespace().collect();
    match toks.as_slice() {
        [one] => {
            if let Some(x) = parse_bg_pos_axis(one, true) {
                Some([x, 0.5])
            } else {
                match *one {
                    "top" => Some([0.5, 0.0]),
                    "bottom" => Some([0.5, 1.0]),
                    "left" => Some([0.0, 0.5]),
                    "right" => Some([1.0, 0.5]),
                    _ => None,
                }
            }
        }
        [a, b] => Some([parse_bg_pos_axis(a, true)?, parse_bg_pos_axis(b, false)?]),
        _ => None,
    }
}

fn parse_bg_pos_axis(s: &str, horizontal: bool) -> Option<f32> {
    match s {
        "left" if horizontal => Some(0.0),
        "right" if horizontal => Some(1.0),
        "top" if !horizontal => Some(0.0),
        "bottom" if !horizontal => Some(1.0),
        "center" => Some(0.5),
        other => {
            if let Some(n) = other.strip_suffix('%') {
                n.trim()
                    .parse::<f32>()
                    .ok()
                    .map(|v| (v / 100.0).clamp(0.0, 1.0))
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::track_fr;

    #[test]
    fn grid_template_rows_fr_lerps() {
        let mut style = Style::default();
        style.grid_template_rows = vec![track_fr(0.0)];
        let text = TextStyle::default();
        let a = read_value(&style, &text, PropertyId::GridTemplateRows);
        style.grid_template_rows = vec![track_fr(1.0)];
        let b = read_value(&style, &text, PropertyId::GridTemplateRows);
        let mid = interpolate(&a, &b, 0.5).expect("fr tracks interpolate");
        match mid {
            AnimatableValue::Scalar(x) => assert!((x - 0.5).abs() < 1e-4, "got {x}"),
            other => panic!("expected scalar, got {other:?}"),
        }
        let mut out = Style::default();
        let mut t = TextStyle::default();
        write_value(&mut out, &mut t, PropertyId::GridTemplateRows, &mid);
        assert!((first_track_fr(&out.grid_template_rows).unwrap() - 0.5).abs() < 1e-4);
    }

    #[test]
    fn grid_template_equal_length_fr_list_lerps() {
        let mut style = Style::default();
        style.grid_template_rows = vec![track_fr(0.0), track_fr(0.0)];
        let text = TextStyle::default();
        let a = read_value(&style, &text, PropertyId::GridTemplateRows);
        style.grid_template_rows = vec![track_fr(1.0), track_fr(1.0)];
        let b = read_value(&style, &text, PropertyId::GridTemplateRows);
        let mid = interpolate(&a, &b, 0.5).expect("equal-length fr lists interpolate");
        match mid {
            AnimatableValue::FrList(ref v) => {
                assert_eq!(v.len(), 2);
                assert!((v[0] - 0.5).abs() < 1e-4 && (v[1] - 0.5).abs() < 1e-4);
            }
            other => panic!("expected FrList, got {other:?}"),
        }
        let mut out = Style::default();
        let mut t = TextStyle::default();
        write_value(&mut out, &mut t, PropertyId::GridTemplateRows, &mid);
        assert_eq!(
            tracks_fr_list(&out.grid_template_rows).as_deref(),
            Some(&[0.5, 0.5][..])
        );
    }

    #[test]
    fn compose_add_opacity_and_transform_scale_multiplies() {
        let o = compose(
            AnimationComposition::Add,
            &AnimatableValue::Scalar(0.3),
            &AnimatableValue::Scalar(0.2),
        );
        match o {
            AnimatableValue::Scalar(x) => assert!((x - 0.5).abs() < 1e-4),
            other => panic!("{other:?}"),
        }
        let t = compose(
            AnimationComposition::Accumulate,
            &AnimatableValue::Transform([10.0, 0.0, 2.0, 2.0, 15.0]),
            &AnimatableValue::Transform([5.0, 1.0, 0.5, 0.5, 5.0]),
        );
        match t {
            AnimatableValue::Transform(v) => {
                assert!((v[0] - 15.0).abs() < 1e-4);
                assert!((v[2] - 1.0).abs() < 1e-4);
                assert!((v[4] - 20.0).abs() < 1e-4);
            }
            other => panic!("{other:?}"),
        }
    }
}
