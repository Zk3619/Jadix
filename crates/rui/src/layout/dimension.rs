use std::sync::atomic::{AtomicU32, Ordering};

use taffy::prelude::{CompactLength, Dimension, LengthPercentage, LengthPercentageAuto};

/// Default CSS root font-size (1rem) when unset. Live value is [`css_root_font_size_px`].
pub const CSS_ROOT_FONT_SIZE_PX: f32 = 16.0;

/// 0 = uninitialized (lazy-read `RUI_FONT_SIZE` / default 16). Synced from [`crate::utils::font::set_text_defaults`].
static ROOT_FONT_SIZE_BITS: AtomicU32 = AtomicU32::new(0);

/// Current CSS root font-size in logical px (`1rem`). Same slot as [`crate::utils::font::TextDefaults::size_px`].
#[inline]
pub fn css_root_font_size_px() -> f32 {
    let bits = ROOT_FONT_SIZE_BITS.load(Ordering::Relaxed);
    if bits != 0 {
        let v = f32::from_bits(bits);
        if v.is_finite() && v > 0.0 {
            return v;
        }
    }
    let v = std::env::var("RUI_FONT_SIZE")
        .ok()
        .and_then(|s| s.trim().parse::<f32>().ok())
        .filter(|x| x.is_finite() && *x > 0.0)
        .unwrap_or(CSS_ROOT_FONT_SIZE_PX)
        .max(1.0);
    ROOT_FONT_SIZE_BITS.store(v.to_bits(), Ordering::Relaxed);
    v
}

/// Keep rem math in lockstep with [`crate::utils::font::set_text_defaults`].
pub(crate) fn sync_root_font_size_px(px: f32) {
    ROOT_FONT_SIZE_BITS.store(px.max(1.0).to_bits(), Ordering::Relaxed);
}

/// Builder length: logical pixels (same as a raw `f32` / `i32` on builders).
#[inline]
pub fn px(v: f32) -> CssDim {
    CssDim::Px(v)
}

/// Builder length: CSS `rem` (scales with [`css_root_font_size_px`]).
#[inline]
pub fn rem(v: f32) -> CssDim {
    CssDim::Rem(v)
}

/// Builder length: CSS `em` (element font-size at layout; preview uses root).
#[inline]
pub fn em(v: f32) -> CssDim {
    CssDim::Em(v)
}

/// Taffy 0.13 stores length/percent/auto as a tagged pointer, not an enum.
/// Classify so call sites can still `match` without poking `CompactLength` tags.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DimKind {
    Length(f32),
    Percent(f32),
    Auto,
}

#[inline]
pub fn dim_kind(d: Dimension) -> DimKind {
    match d.tag() {
        CompactLength::LENGTH_TAG => DimKind::Length(d.value()),
        CompactLength::PERCENT_TAG => DimKind::Percent(d.value()),
        _ => DimKind::Auto,
    }
}

#[inline]
pub fn dim_length_px(d: Dimension) -> Option<f32> {
    (d.tag() == CompactLength::LENGTH_TAG).then(|| d.value())
}

#[inline]
pub fn lp_kind(lp: LengthPercentage) -> DimKind {
    let raw = lp.into_raw();
    match raw.tag() {
        CompactLength::PERCENT_TAG => DimKind::Percent(raw.value()),
        _ => DimKind::Length(raw.value()),
    }
}

#[inline]
pub fn lp_length_px(lp: LengthPercentage) -> Option<f32> {
    let raw = lp.into_raw();
    (raw.tag() == CompactLength::LENGTH_TAG).then(|| raw.value())
}

#[inline]
pub fn lpa_kind(lpa: LengthPercentageAuto) -> DimKind {
    let raw = lpa.into_raw();
    match raw.tag() {
        CompactLength::LENGTH_TAG => DimKind::Length(raw.value()),
        CompactLength::PERCENT_TAG => DimKind::Percent(raw.value()),
        _ => DimKind::Auto,
    }
}

#[inline]
pub fn lpa_length_px(lpa: LengthPercentageAuto) -> Option<f32> {
    let raw = lpa.into_raw();
    (raw.tag() == CompactLength::LENGTH_TAG).then(|| raw.value())
}

#[inline]
pub fn dimension_to_lp(d: Dimension) -> LengthPercentage {
    match dim_kind(d) {
        DimKind::Length(l) => LengthPercentage::length(l),
        DimKind::Percent(p) => LengthPercentage::percent(p),
        DimKind::Auto => LengthPercentage::length(0.0),
    }
}

#[inline]
pub fn dimension_to_lpa(d: Dimension) -> LengthPercentageAuto {
    match dim_kind(d) {
        DimKind::Length(l) => LengthPercentageAuto::length(l),
        DimKind::Percent(p) => LengthPercentageAuto::percent(p),
        DimKind::Auto => LengthPercentageAuto::auto(),
    }
}

pub(crate) fn parse_css_number(s: &str) -> Option<f32> {
    s.trim().parse::<f32>().ok()
}

// ---------------------------------------------------------------------------
// CSS angle type
// ---------------------------------------------------------------------------

/// CSS `<angle>` value: `deg | rad | grad | turn`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CssAngle {
    Deg(f32),
    Rad(f32),
    Grad(f32),
    Turn(f32),
}

impl Default for CssAngle {
    fn default() -> Self {
        Self::Deg(0.0)
    }
}

impl CssAngle {
    pub fn to_rad(self) -> f32 {
        match self {
            Self::Deg(d) => d * std::f32::consts::PI / 180.0,
            Self::Rad(r) => r,
            Self::Grad(g) => g * std::f32::consts::PI / 200.0,
            Self::Turn(t) => t * 2.0 * std::f32::consts::PI,
        }
    }

    pub fn to_deg(self) -> f32 {
        match self {
            Self::Deg(d) => d,
            Self::Rad(r) => r * 180.0 / std::f32::consts::PI,
            Self::Grad(g) => g * 0.9,
            Self::Turn(t) => t * 360.0,
        }
    }
}

/// Parse a CSS angle string like `"45deg"`, `"1.5rad"`, `"100grad"`, `"0.5turn"`.
/// Also accepts bare `"0"` as zero degrees.
pub fn parse_css_angle(s: &str) -> Option<CssAngle> {
    let s = s.trim();
    if s == "0" {
        return Some(CssAngle::Deg(0.0));
    }
    if let Some(raw) = s.strip_suffix("deg") {
        return parse_css_number(raw).map(CssAngle::Deg);
    }
    if let Some(raw) = s.strip_suffix("grad") {
        return parse_css_number(raw).map(CssAngle::Grad);
    }
    if let Some(raw) = s.strip_suffix("rad") {
        return parse_css_number(raw).map(CssAngle::Rad);
    }
    if let Some(raw) = s.strip_suffix("turn") {
        return parse_css_number(raw).map(CssAngle::Turn);
    }
    None
}

fn parse_css_length_px(s: &str, em_base_px: f32) -> Option<f32> {
    let dim = parse_css_dim(s)?;
    let ctx = UnitCtx::new(0.0, 0.0, em_base_px);
    dim.resolve_px(&ctx, PercentAxis::Inline)
        .or_else(|| dim.preview_dimension().and_then(dim_length_px))
}

/// Convert builder / RSX size tokens into a Taffy [`Dimension`].
///
/// # Percentage (`"50%"`, `"100%"`) and fractions (`"1/2"`, `"3/4"`)
/// Resolves against the **containing block's content box** (CSS): the parent's
/// content width/height after subtracting that parent's padding and border.
/// It is **not** a fraction of this element's inner content. Same for `min-width` /
/// `max-width` / `min-height` / `max-height`. `"1/2"` ≡ `"50%"` of the parent.
///
/// # Unsuffixed numbers (`"100"`, `"12"`)
/// Remain **rem** ([`css_root_font_size_px`]). `"100"` is **not** `100%` — write `"100%"` for
/// a parent-relative fraction.
///
/// # Content keywords (`"fit"`, `"fit-content"`, `"min-content"`, `"max-content"`)
/// Parsed on the [`CssDim`] / [`IntoCssDim`] path. Taffy 0.13 flex `Dimension`
/// cannot carry those tags (they panic in `maybe_resolve`), so they map to
/// [`Dimension::Auto`] plus a stretch-hug policy in layout.
///
/// # Viewport units (`vw` / `vh` / `dvw` / …)
/// Immediate [`IntoDimension`] maps these to [`Dimension::Percent`] (bind fallback).
/// Builder / RSX paths use [`CssDim`] and resolve against the layout viewport in
/// [`crate::style::Style::to_taffy_style_with`].
pub trait IntoDimension {
    fn into_dimension(self) -> Dimension;
}

impl IntoDimension for f32 {
    fn into_dimension(self) -> Dimension {
        Dimension::length(self)
    }
}

impl IntoDimension for Dimension {
    fn into_dimension(self) -> Dimension {
        self
    }
}

impl IntoDimension for i32 {
    fn into_dimension(self) -> Dimension {
        Dimension::length(self as f32)
    }
}

impl IntoDimension for &str {
    fn into_dimension(self) -> Dimension {
        parse_css_dim(self)
            .and_then(|d| d.preview_dimension())
            .unwrap_or(Dimension::auto())
    }
}

impl IntoDimension for CssDim {
    fn into_dimension(self) -> Dimension {
        self.preview_dimension().unwrap_or(Dimension::auto())
    }
}

/// `vw` / `vh` / `dvw` / `dvh` / `svw` / `svh` / `lvw` / `lvh` → numeric prefix.
fn strip_viewport_unit(s: &str) -> Option<&str> {
    const SUFFIXES: &[&str] = &["dvw", "dvh", "svw", "svh", "lvw", "lvh", "vw", "vh"];
    for suffix in SUFFIXES {
        if let Some(rest) = s.strip_suffix(suffix) {
            // Reject bare unit or junk like "85dvhpx".
            if !rest.is_empty()
                && rest
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == '.' || c == '-')
            {
                return Some(rest);
            }
        }
    }
    None
}

impl IntoDimension for String {
    fn into_dimension(self) -> Dimension {
        self.as_str().into_dimension()
    }
}

impl IntoDimension for &String {
    fn into_dimension(self) -> Dimension {
        self.as_str().into_dimension()
    }
}

/// Which containing-block axis `%` (inside `calc`) resolves against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PercentAxis {
    /// Width, horizontal padding/margin/gap, left/right inset. Padding `%` always uses this (CSS).
    Inline,
    /// Height, row-gap, top/bottom inset.
    Block,
}

/// Layout-time unit context: viewport = current [`crate::layout::LayoutTree`] available size.
#[derive(Clone, Copy, Debug)]
pub struct UnitCtx {
    pub viewport_w: f32,
    pub viewport_h: f32,
    /// Used for `em` (CSS: relative to this element's font-size).
    pub font_size: f32,
    pub root_font_size: f32,
    /// Containing-block **content** width (parent content box). Needed for `%` inside `calc`.
    pub cb_w: Option<f32>,
    /// Containing-block **content** height.
    pub cb_h: Option<f32>,
    /// CSS `calc-size()` `size` keyword — used px of the basis after layout.
    pub size_px: Option<f32>,
}

impl UnitCtx {
    pub fn new(viewport_w: f32, viewport_h: f32, font_size: f32) -> Self {
        Self {
            viewport_w: viewport_w.max(0.0),
            viewport_h: viewport_h.max(0.0),
            font_size: if font_size.is_finite() && font_size > 0.0 {
                font_size
            } else {
                css_root_font_size_px()
            },
            root_font_size: css_root_font_size_px(),
            cb_w: None,
            cb_h: None,
            size_px: None,
        }
    }

    pub fn with_containing_block(mut self, w: Option<f32>, h: Option<f32>) -> Self {
        self.cb_w = w.filter(|v| v.is_finite() && *v >= 0.0);
        self.cb_h = h.filter(|v| v.is_finite() && *v >= 0.0);
        self
    }
}

/// Parsed CSS length for box-model fields. Viewport / `em` / `calc` resolve in [`Style::to_taffy_style`].
#[derive(Clone, Debug, PartialEq)]
pub enum CssDim {
    Auto,
    Px(f32),
    /// 0..=1 fraction of the **parent containing-block content box** (CSS `%`).
    Percent(f32),
    Em(f32),
    Rem(f32),
    /// 0..=1 fraction of layout viewport width (`vw` / `dvw` / `svw` / `lvw`).
    Vw(f32),
    /// 0..=1 fraction of layout viewport height (`vh` / `dvh` / `svh` / `lvh`).
    Vh(f32),
    /// CSS `width: fit-content` / Tailwind `w-fit`. Taffy flex maps this to Auto.
    FitContent,
    /// CSS `min-content` / Tailwind `w-min`.
    MinContent,
    /// CSS `max-content` / Tailwind `w-max`.
    MaxContent,
    /// Unitless number inside `calc()` / `calc-size()` (multipliers).
    Number(f32),
    CalcAdd(Box<CssDim>, Box<CssDim>),
    CalcSub(Box<CssDim>, Box<CssDim>),
    CalcMul(Box<CssDim>, Box<CssDim>),
    CalcDiv(Box<CssDim>, Box<CssDim>),
    /// CSS `calc-size()` `size` keyword (only valid inside a calc-size calculation).
    Size,
    /// CSS `calc-size(basis, calc)` — `size` in `calc` is the used px of `basis`.
    CalcSize {
        basis: Box<CssDim>,
        calc: Box<CssDim>,
    },
}

impl CssDim {
    pub fn parse(s: &str) -> Option<Self> {
        parse_css_dim(s.trim())
    }

    pub fn needs_layout_context(&self) -> bool {
        match self {
            Self::Em(_) | Self::Vw(_) | Self::Vh(_) | Self::Size | Self::CalcSize { .. } => true,
            Self::CalcAdd(a, b)
            | Self::CalcSub(a, b)
            | Self::CalcMul(a, b)
            | Self::CalcDiv(a, b) => {
                a.needs_layout_context() || b.needs_layout_context() || self.has_percent()
            }
            _ => false,
        }
    }

    pub fn has_percent(&self) -> bool {
        match self {
            Self::Percent(_) => true,
            Self::CalcAdd(a, b)
            | Self::CalcSub(a, b)
            | Self::CalcMul(a, b)
            | Self::CalcDiv(a, b) => a.has_percent() || b.has_percent(),
            Self::CalcSize { basis, calc } => basis.has_percent() || calc.has_percent(),
            _ => false,
        }
    }

    pub fn is_calc_size(&self) -> bool {
        matches!(self, Self::CalcSize { .. })
    }

    pub fn contains_size_keyword(&self) -> bool {
        match self {
            Self::Size => true,
            Self::CalcAdd(a, b)
            | Self::CalcSub(a, b)
            | Self::CalcMul(a, b)
            | Self::CalcDiv(a, b) => a.contains_size_keyword() || b.contains_size_keyword(),
            Self::CalcSize { calc, .. } => calc.contains_size_keyword(),
            _ => false,
        }
    }

    /// CSS `fit-content` / `min-content` / `max-content` (and Tailwind `fit` / `min` / `max`).
    pub fn is_content_keyword(&self) -> bool {
        matches!(self, Self::FitContent | Self::MinContent | Self::MaxContent)
    }

    /// Immediate Taffy value when no viewport / element font-size is required.
    pub fn as_immediate_dimension(&self) -> Option<Dimension> {
        match self {
            Self::Auto | Self::FitContent | Self::MinContent | Self::MaxContent => {
                Some(Dimension::auto())
            }
            Self::Px(v) => Some(Dimension::length(*v)),
            Self::Percent(p) => Some(Dimension::percent(*p)),
            Self::Rem(v) => Some(Dimension::length(*v * css_root_font_size_px())),
            _ => None,
        }
    }

    /// Preview stored on `Style` before layout (vw/vh ≈ parent % until resolve).
    pub fn preview_dimension(&self) -> Option<Dimension> {
        if let Some(d) = self.as_immediate_dimension() {
            return Some(d);
        }
        match self {
            Self::Em(v) => Some(Dimension::length(*v * css_root_font_size_px())),
            Self::Vw(p) | Self::Vh(p) => Some(Dimension::percent(*p)),
            Self::CalcSize { .. } | Self::Size => Some(Dimension::auto()),
            _ => None,
        }
    }

    pub fn resolve_px(&self, ctx: &UnitCtx, axis: PercentAxis) -> Option<f32> {
        match self {
            Self::Auto | Self::FitContent | Self::MinContent | Self::MaxContent => None,
            Self::Percent(p) => {
                let cb = match axis {
                    PercentAxis::Inline => ctx.cb_w?,
                    PercentAxis::Block => ctx.cb_h?,
                };
                Some(*p * cb)
            }
            Self::Px(v) => Some(*v),
            Self::Rem(v) => Some(*v * ctx.root_font_size),
            Self::Em(v) => Some(*v * ctx.font_size),
            Self::Vw(p) => Some(*p * ctx.viewport_w),
            Self::Vh(p) => Some(*p * ctx.viewport_h),
            Self::Number(v) => Some(*v),
            Self::CalcAdd(a, b) => Some(a.resolve_px(ctx, axis)? + b.resolve_px(ctx, axis)?),
            Self::CalcSub(a, b) => Some(a.resolve_px(ctx, axis)? - b.resolve_px(ctx, axis)?),
            Self::CalcMul(a, b) => Some(a.resolve_px(ctx, axis)? * b.resolve_px(ctx, axis)?),
            Self::CalcDiv(a, b) => {
                let d = b.resolve_px(ctx, axis)?;
                if d.abs() < 1e-12 {
                    return None;
                }
                Some(a.resolve_px(ctx, axis)? / d)
            }
            Self::Size => ctx.size_px,
            Self::CalcSize { basis, calc } => {
                let basis_px = match basis.as_ref() {
                    Self::Auto | Self::FitContent | Self::MinContent | Self::MaxContent => {
                        ctx.size_px
                    }
                    _ => basis.resolve_px(ctx, axis),
                }?;
                let mut inner = *ctx;
                inner.size_px = Some(basis_px);
                calc.resolve_px(&inner, axis)
            }
        }
    }

    /// Evaluate `calc-size(basis, calc)` with `size` bound to `size_px`.
    pub fn eval_calc_size(&self, size_px: f32, ctx: &UnitCtx, axis: PercentAxis) -> Option<f32> {
        let mut inner = *ctx;
        inner.size_px = Some(size_px.max(0.0));
        match self {
            Self::CalcSize { .. } | Self::Size => self.resolve_px(&inner, axis),
            _ => self.resolve_px(&inner, axis),
        }
    }

    pub fn to_dimension(&self, ctx: &UnitCtx) -> Dimension {
        self.to_dimension_axis(ctx, PercentAxis::Inline)
    }

    pub fn to_dimension_axis(&self, ctx: &UnitCtx, axis: PercentAxis) -> Dimension {
        if let Self::Percent(p) = self {
            return Dimension::percent(*p);
        }
        if matches!(
            self,
            Self::Auto | Self::FitContent | Self::MinContent | Self::MaxContent | Self::Size
        ) {
            return Dimension::auto();
        }
        if let Self::CalcSize { basis, .. } = self {
            if matches!(
                basis.as_ref(),
                Self::Auto | Self::FitContent | Self::MinContent | Self::MaxContent
            ) && ctx.size_px.is_none()
            {
                return Dimension::auto();
            }
        }
        if let Some(px) = self.resolve_px(ctx, axis) {
            return Dimension::length(px);
        }
        self.preview_dimension().unwrap_or(Dimension::auto())
    }

    pub fn to_length_percentage(&self, ctx: &UnitCtx, axis: PercentAxis) -> LengthPercentage {
        dimension_to_lp(self.to_dimension_axis(ctx, axis))
    }

    pub fn to_length_percentage_auto(
        &self,
        ctx: &UnitCtx,
        axis: PercentAxis,
    ) -> LengthPercentageAuto {
        dimension_to_lpa(self.to_dimension_axis(ctx, axis))
    }

    /// CSS Box Model: percentage **padding** (all sides) resolves against the
    /// containing block's **width**. Unknown CB → `0` (cyclic `%`).
    pub fn to_box_edge_length_percentage(&self, ctx: &UnitCtx) -> LengthPercentage {
        match self {
            Self::Percent(p) => match ctx.cb_w {
                Some(w) => LengthPercentage::length(*p * w),
                None => LengthPercentage::length(0.0),
            },
            _ => self.to_length_percentage(ctx, PercentAxis::Inline),
        }
    }

    /// Same containing-block rule as [`Self::to_box_edge_length_percentage`] for margin.
    pub fn to_box_edge_length_percentage_auto(&self, ctx: &UnitCtx) -> LengthPercentageAuto {
        match self {
            Self::Percent(p) => match ctx.cb_w {
                Some(w) => LengthPercentageAuto::length(*p * w),
                None => LengthPercentageAuto::length(0.0),
            },
            _ => self.to_length_percentage_auto(ctx, PercentAxis::Inline),
        }
    }

    /// Unary minus for length-like dims (used by [`crate::core::tree::Element::bleed_x`]).
    pub fn negated(self) -> Self {
        match self {
            Self::Auto => Self::Auto,
            Self::Px(v) => Self::Px(-v),
            Self::Percent(p) => Self::Percent(-p),
            Self::Em(v) => Self::Em(-v),
            Self::Rem(v) => Self::Rem(-v),
            Self::Vw(p) => Self::Vw(-p),
            Self::Vh(p) => Self::Vh(-p),
            Self::Number(v) => Self::Number(-v),
            // -(a + b) = (-a) + (-b); -(a - b) = (-a) + b
            Self::CalcAdd(a, b) => Self::CalcAdd(Box::new(a.negated()), Box::new(b.negated())),
            Self::CalcSub(a, b) => Self::CalcAdd(Box::new(a.negated()), b),
            Self::CalcMul(a, b) => Self::CalcMul(Box::new(a.negated()), b),
            Self::CalcDiv(a, b) => Self::CalcDiv(Box::new(a.negated()), b),
            Self::FitContent | Self::MinContent | Self::MaxContent | Self::Size => self,
            Self::CalcSize { basis, calc } => Self::CalcSize {
                basis,
                calc: Box::new(calc.negated()),
            },
        }
    }
}

pub fn parse_css_dim(s: &str) -> Option<CssDim> {
    parse_css_dim_ex(s, false)
}

fn parse_css_dim_ex(s: &str, allow_size: bool) -> Option<CssDim> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let lower = s.to_ascii_lowercase();
    match lower.as_str() {
        "auto" => return Some(CssDim::Auto),
        "fit" | "fit-content" => return Some(CssDim::FitContent),
        "min-content" | "min_content" | "min" => return Some(CssDim::MinContent),
        "max-content" | "max_content" | "max" => return Some(CssDim::MaxContent),
        "size" if allow_size => return Some(CssDim::Size),
        _ => {}
    }
    if let Some(inner) = strip_fn_call(s, "calc-size") {
        return parse_calc_size(inner);
    }
    if let Some(inner) = s.strip_prefix("calc(").and_then(|t| t.strip_suffix(')')) {
        return parse_calc_expr(inner.trim(), allow_size);
    }
    if allow_size {
        if let Some(d) = parse_calc_expr(s, true) {
            return Some(d);
        }
    }
    if let Some(rest) = s.strip_suffix('%') {
        let val: f32 = rest.trim().parse().ok()?;
        return Some(CssDim::Percent(val / 100.0));
    }
    if let Some(rest) = strip_viewport_unit(s) {
        let val: f32 = rest.trim().parse().ok()?;
        let frac = val / 100.0;
        if lower.contains("vw") {
            return Some(CssDim::Vw(frac));
        }
        return Some(CssDim::Vh(frac));
    }
    if let Some(raw) = lower.strip_suffix("rem") {
        if let Some(v) = parse_css_number(raw) {
            return Some(CssDim::Rem(v));
        }
    }
    if let Some(raw) = lower.strip_suffix("em") {
        if let Some(v) = parse_css_number(raw) {
            return Some(CssDim::Em(v));
        }
    }
    if let Some(raw) = lower.strip_suffix("px") {
        if let Some(v) = parse_css_number(raw) {
            return Some(CssDim::Px(v));
        }
    }
    if allow_size {
        if let Some(n) = parse_css_number(s) {
            return Some(CssDim::Number(n));
        }
    }
    // Tailwind-style `a/b` → parent containing-block fraction (`1/2` ≡ 50%).
    if let Some(dim) = parse_css_fraction_percent(s) {
        return Some(dim);
    }
    // Unsuffixed number = rem (live root). `"100"` is 100rem, not 100% of the parent.
    parse_css_number(s).map(CssDim::Rem)
}

fn strip_fn_call<'a>(s: &'a str, name: &str) -> Option<&'a str> {
    let t = s.trim();
    let rest = t.strip_prefix(name)?;
    let rest = rest.trim_start();
    rest.strip_prefix('(')
        .and_then(|r| r.strip_suffix(')'))
        .map(str::trim)
}

fn parse_calc_size(inner: &str) -> Option<CssDim> {
    let (basis_s, calc_s) = split_top_comma(inner)?;
    let basis = parse_css_dim_ex(basis_s, false)?;
    let calc = parse_css_dim_ex(calc_s, true)?;
    Some(CssDim::CalcSize {
        basis: Box::new(basis),
        calc: Box::new(calc),
    })
}

fn split_top_comma(s: &str) -> Option<(&str, &str)> {
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                let left = s[..i].trim();
                let right = s[i + 1..].trim();
                if !left.is_empty() && !right.is_empty() {
                    return Some((left, right));
                }
            }
            _ => {}
        }
    }
    None
}

/// `"1/2"` / `"3/4"` → [`CssDim::Percent`] of the **parent** content box (not inner content).
fn parse_css_fraction_percent(s: &str) -> Option<CssDim> {
    let (num_s, den_s) = s.split_once('/')?;
    if num_s.trim().is_empty() || den_s.trim().is_empty() {
        return None;
    }
    let num: f32 = num_s.trim().parse().ok()?;
    let den: f32 = den_s.trim().parse().ok()?;
    if !num.is_finite() || !den.is_finite() || den <= 0.0 || num < 0.0 || num > den {
        return None;
    }
    Some(CssDim::Percent(num / den))
}

fn parse_calc_expr(inner: &str, allow_size: bool) -> Option<CssDim> {
    let (left, op, right) = split_calc_binop(inner)?;
    let a = parse_css_dim_ex(left, allow_size)?;
    let b = parse_css_dim_ex(right, allow_size)?;
    Some(match op {
        '+' => CssDim::CalcAdd(Box::new(a), Box::new(b)),
        '-' => CssDim::CalcSub(Box::new(a), Box::new(b)),
        '*' => CssDim::CalcMul(Box::new(a), Box::new(b)),
        '/' => CssDim::CalcDiv(Box::new(a), Box::new(b)),
        _ => return None,
    })
}

fn split_calc_binop(s: &str) -> Option<(&str, char, &str)> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    // Lowest precedence first so `size + 20px` splits on `+`.
    for &(ops, skip_leading_minus) in &[("+-", true), ("*/", false)] {
        let mut i = 0;
        while i < bytes.len() {
            let c = bytes[i] as char;
            if c == '(' {
                depth += 1;
            } else if c == ')' {
                depth -= 1;
            } else if depth == 0 && ops.contains(c) {
                if skip_leading_minus && c == '-' && i == 0 {
                    i += 1;
                    continue;
                }
                let left = s[..i].trim();
                let right = s[i + 1..].trim();
                if !left.is_empty() && !right.is_empty() {
                    return Some((left, c, right));
                }
            }
            i += 1;
        }
    }
    None
}

/// One or two CSS lengths (`padding-block: 10px 20px`).
#[derive(Clone, Debug, PartialEq)]
pub enum CssRectPair {
    Both(CssDim),
    StartEnd(CssDim, CssDim),
}

impl CssRectPair {
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split_whitespace().collect();
        match parts.as_slice() {
            [a] => Some(Self::Both(parse_css_dim(a)?)),
            [a, b] => Some(Self::StartEnd(parse_css_dim(a)?, parse_css_dim(b)?)),
            _ => None,
        }
    }
}

pub trait IntoCssDim {
    fn into_css_dim(self) -> CssDim;
}

impl IntoCssDim for CssDim {
    fn into_css_dim(self) -> CssDim {
        self
    }
}

impl IntoCssDim for f32 {
    fn into_css_dim(self) -> CssDim {
        CssDim::Px(self)
    }
}

impl IntoCssDim for i32 {
    fn into_css_dim(self) -> CssDim {
        CssDim::Px(self as f32)
    }
}

impl IntoCssDim for Dimension {
    fn into_css_dim(self) -> CssDim {
        match dim_kind(self) {
            DimKind::Length(v) => CssDim::Px(v),
            DimKind::Percent(p) => CssDim::Percent(p),
            DimKind::Auto => CssDim::Auto,
        }
    }
}

impl IntoCssDim for &str {
    fn into_css_dim(self) -> CssDim {
        parse_css_dim(self).unwrap_or(CssDim::Auto)
    }
}

impl IntoCssDim for String {
    fn into_css_dim(self) -> CssDim {
        self.as_str().into_css_dim()
    }
}

impl IntoCssDim for &String {
    fn into_css_dim(self) -> CssDim {
        self.as_str().into_css_dim()
    }
}

/// One or two CSS lengths for logical `*-block` / `*-inline` shorthands.
pub trait IntoCssRectPair {
    fn into_css_rect_pair(self) -> CssRectPair;
}

impl IntoCssRectPair for CssRectPair {
    fn into_css_rect_pair(self) -> CssRectPair {
        self
    }
}

impl IntoCssRectPair for CssDim {
    fn into_css_rect_pair(self) -> CssRectPair {
        CssRectPair::Both(self)
    }
}

impl IntoCssRectPair for f32 {
    fn into_css_rect_pair(self) -> CssRectPair {
        CssRectPair::Both(CssDim::Px(self))
    }
}

impl IntoCssRectPair for i32 {
    fn into_css_rect_pair(self) -> CssRectPair {
        CssRectPair::Both(CssDim::Px(self as f32))
    }
}

impl IntoCssRectPair for &str {
    fn into_css_rect_pair(self) -> CssRectPair {
        CssRectPair::parse(self).unwrap_or(CssRectPair::Both(CssDim::Auto))
    }
}

impl IntoCssRectPair for String {
    fn into_css_rect_pair(self) -> CssRectPair {
        self.as_str().into_css_rect_pair()
    }
}

impl IntoCssRectPair for &String {
    fn into_css_rect_pair(self) -> CssRectPair {
        self.as_str().into_css_rect_pair()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn em_resolves_against_root_16px_not_element_font_size() {
        // IntoDimension 无节点上下文：em ≡ rem ≡ 活根字号。布局期 CssDim 用本节点 font-size。
        let root = css_root_font_size_px();
        match dim_kind("1em".into_dimension()) {
            DimKind::Length(px) => assert!((px - root).abs() < 1e-6, "1em → {px}"),
            other => panic!("expected length, got {other:?}"),
        }
        match dim_kind("0.5em".into_dimension()) {
            DimKind::Length(px) => assert!((px - root * 0.5).abs() < 1e-6, "0.5em → {px}"),
            other => panic!("expected length, got {other:?}"),
        }
        assert!((IntoF32::into_f32("1.25em") - root * 1.25).abs() < 1e-6);
    }

    #[test]
    fn into_f32_unitless_string_is_rem_like_tailwind_tokens() {
        // `text-sm` ≈ 0.875rem；字符串无单位 = rem（非 spacing scale）。
        let root = css_root_font_size_px();
        assert!((IntoF32::into_f32("0.875") - 0.875 * root).abs() < 1e-6);
        assert!((IntoF32::into_f32(14) - 14.0).abs() < 1e-6); // Rust 数值 = px
    }

    #[test]
    fn into_f32_raw_font_weight_unitless() {
        assert!((IntoF32Raw::into_f32_raw("600") - 600.0).abs() < 1e-6);
        assert!((IntoF32Raw::into_f32_raw(600) - 600.0).abs() < 1e-6);
        // 对比：长度路径会把 "600" 当 600rem
        assert!((IntoF32::into_f32("600") - 600.0 * css_root_font_size_px()).abs() < 1e-6);
    }

    #[test]
    fn into_opacity_fraction_and_percent() {
        assert!((0.4f32.into_opacity() - 0.4).abs() < 1e-6);
        assert!((1i32.into_opacity() - 1.0).abs() < 1e-6);
        assert!((0i32.into_opacity() - 0.0).abs() < 1e-6);
        assert!((40i32.into_opacity() - 0.4).abs() < 1e-6);
        assert!((100u8.into_opacity() - 1.0).abs() < 1e-6);
        assert!(("40%".into_opacity() - 0.4).abs() < 1e-6);
        assert!(("100%".into_opacity() - 1.0).abs() < 1e-6);
        assert!(("0.4".into_opacity() - 0.4).abs() < 1e-6);
        assert!(("1".into_opacity() - 1.0).abs() < 1e-6);
        assert!((parse_opacity("50%") - 0.5).abs() < 1e-6);
    }

    #[test]
    fn into_f32_side_offset_parity_4_eq_4px_not_unitless_1() {
        // Builder / RSX `{4}` = 4px；无单位 `"1"` / `"0.25"` = rem（活根字号）。
        let root = css_root_font_size_px();
        assert!((IntoF32::into_f32(4) - IntoF32::into_f32("4px")).abs() < 1e-6);
        assert!((IntoF32::into_f32("0.25") - 0.25 * root).abs() < 1e-6);
        assert!((IntoF32::into_f32("0.25rem") - 0.25 * root).abs() < 1e-6);
        assert!((IntoF32::into_f32("1") - root).abs() < 1e-6);
        assert!((IntoF32::into_f32(4) - IntoF32::into_f32("1")).abs() > 1.0);
    }

    #[test]
    fn into_border_widths_css_shorthand() {
        assert_eq!(IntoBorderWidths::into_border_widths(2.0_f32), [2.0; 4]);
        assert_eq!(
            IntoBorderWidths::into_border_widths("3px 9px"),
            [3.0, 9.0, 3.0, 9.0]
        );
        assert_eq!(
            IntoBorderWidths::into_border_widths("1px 2px 3px 4px"),
            [1.0, 2.0, 3.0, 4.0]
        );
    }

    #[test]
    fn css_dim_negated() {
        assert!(matches!(CssDim::Rem(0.25).negated(), CssDim::Rem(v) if (v + 0.25).abs() < 1e-6));
        assert!(matches!(CssDim::Px(4.0).negated(), CssDim::Px(v) if (v + 4.0).abs() < 1e-6));
        assert!(matches!(CssDim::Auto.negated(), CssDim::Auto));
        let add = CssDim::CalcAdd(Box::new(CssDim::Px(1.0)), Box::new(CssDim::Px(2.0)));
        match add.negated() {
            CssDim::CalcAdd(a, b) => {
                assert!(matches!(*a, CssDim::Px(v) if (v + 1.0).abs() < 1e-6));
                assert!(matches!(*b, CssDim::Px(v) if (v + 2.0).abs() < 1e-6));
            }
            other => panic!("expected CalcAdd, got {other:?}"),
        }
    }

    #[test]
    fn parses_dvh_dvw_as_percent() {
        match dim_kind("85dvh".into_dimension()) {
            DimKind::Percent(p) => assert!((p - 0.85).abs() < 1e-6),
            other => panic!("expected percent, got {other:?}"),
        }
        match dim_kind("85dvw".into_dimension()) {
            DimKind::Percent(p) => assert!((p - 0.85).abs() < 1e-6),
            other => panic!("expected percent, got {other:?}"),
        }
        match dim_kind("50vh".into_dimension()) {
            DimKind::Percent(p) => assert!((p - 0.5).abs() < 1e-6),
            other => panic!("expected percent, got {other:?}"),
        }
    }

    #[test]
    fn css_dim_vw_vh_em_calc_resolve() {
        let ctx = UnitCtx::new(800.0, 600.0, 20.0);
        assert!(matches!(parse_css_dim("auto"), Some(CssDim::Auto)));
        match dim_kind(parse_css_dim("50vw").unwrap().to_dimension(&ctx)) {
            DimKind::Length(px) => assert!((px - 400.0).abs() < 0.5, "50vw → {px}"),
            other => panic!("{other:?}"),
        }
        match dim_kind(parse_css_dim("100dvh").unwrap().to_dimension(&ctx)) {
            DimKind::Length(px) => assert!((px - 600.0).abs() < 0.5, "100dvh → {px}"),
            other => panic!("{other:?}"),
        }
        match dim_kind(parse_css_dim("2em").unwrap().to_dimension(&ctx)) {
            DimKind::Length(px) => assert!((px - 40.0).abs() < 0.5, "2em@20px → {px}"),
            other => panic!("{other:?}"),
        }
        match dim_kind(parse_css_dim("10rem").unwrap().to_dimension(&ctx)) {
            DimKind::Length(px) => assert!((px - 160.0).abs() < 0.5, "10rem → {px}"),
            other => panic!("{other:?}"),
        }
        match dim_kind(parse_css_dim("50%").unwrap().to_dimension(&ctx)) {
            DimKind::Percent(p) => assert!((p - 0.5).abs() < 1e-6),
            other => panic!("{other:?}"),
        }
        match dim_kind(
            parse_css_dim("calc(100vw - 2rem)")
                .unwrap()
                .to_dimension(&ctx),
        ) {
            DimKind::Length(px) => assert!((px - 768.0).abs() < 0.5, "calc → {px}"),
            other => panic!("{other:?}"),
        }
        match dim_kind(parse_css_dim("calc(1em + 8px)").unwrap().to_dimension(&ctx)) {
            DimKind::Length(px) => assert!((px - 28.0).abs() < 0.5, "calc em+px → {px}"),
            other => panic!("{other:?}"),
        }
        let pct_calc = parse_css_dim("calc(100% - 40px)").unwrap();
        assert!(pct_calc.has_percent());
        assert!(pct_calc.to_dimension(&ctx).is_auto());
        let ctx_cb = ctx.with_containing_block(Some(260.0), Some(180.0));
        match dim_kind(pct_calc.to_dimension(&ctx_cb)) {
            DimKind::Length(px) => {
                assert!((px - 220.0).abs() < 0.5, "calc(100%-40px) @260 → {px}")
            }
            other => panic!("{other:?}"),
        }
        match dim_kind(
            parse_css_dim("calc(50% + 10px)")
                .unwrap()
                .to_dimension_axis(&ctx_cb, PercentAxis::Block),
        ) {
            DimKind::Length(px) => {
                assert!((px - 100.0).abs() < 0.5, "50% of 180 + 10 → {px}")
            }
            other => panic!("{other:?}"),
        }
        let pair = CssRectPair::parse("10px 20px").unwrap();
        assert!(matches!(
            pair,
            CssRectPair::StartEnd(CssDim::Px(10.0), CssDim::Px(20.0))
        ));
    }

    #[test]
    fn unsuffixed_100_is_rem_not_parent_percent() {
        match parse_css_dim("100") {
            Some(CssDim::Rem(v)) => assert!((v - 100.0).abs() < 1e-6, "got Rem({v})"),
            other => panic!("w(\"100\") must stay rem, got {other:?}"),
        }
        match dim_kind("100".into_dimension()) {
            DimKind::Length(px) => assert!(
                (px - 100.0 * css_root_font_size_px()).abs() < 1e-6,
                "100rem → {px}"
            ),
            other => panic!("expected length, got {other:?}"),
        }
        match parse_css_dim("12") {
            Some(CssDim::Rem(v)) => assert!((v - 12.0).abs() < 1e-6),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn percent_token_is_parent_fraction_not_content() {
        match parse_css_dim("100%") {
            Some(CssDim::Percent(p)) => assert!((p - 1.0).abs() < 1e-6),
            other => panic!("100% must be parent fraction, got {other:?}"),
        }
        match parse_css_dim("50%") {
            Some(CssDim::Percent(p)) => assert!((p - 0.5).abs() < 1e-6),
            other => panic!("{other:?}"),
        }
        match dim_kind("100%".into_dimension()) {
            DimKind::Percent(p) => assert!((p - 1.0).abs() < 1e-6),
            other => panic!("{other:?}"),
        }
        let ctx = UnitCtx::new(800.0, 600.0, 16.0).with_containing_block(Some(260.0), Some(180.0));
        match dim_kind(parse_css_dim("100%").unwrap().to_dimension(&ctx)) {
            DimKind::Percent(p) => assert!(
                (p - 1.0).abs() < 1e-6,
                "Taffy keeps % so layout resolves against the parent CB, got {p}"
            ),
            other => panic!("{other:?}"),
        }
        assert!(
            (parse_css_dim("100%")
                .unwrap()
                .resolve_px(&ctx, PercentAxis::Inline)
                .unwrap()
                - 260.0)
                .abs()
                < 0.5,
            "100% of parent content 260"
        );
        assert!(
            (parse_css_dim("100%")
                .unwrap()
                .resolve_px(&ctx, PercentAxis::Block)
                .unwrap()
                - 180.0)
                .abs()
                < 0.5,
            "100% of parent content height 180"
        );
    }

    #[test]
    fn slash_fraction_is_parent_percent_not_content() {
        match parse_css_dim("1/2") {
            Some(CssDim::Percent(p)) => assert!((p - 0.5).abs() < 1e-6),
            other => panic!("1/2 must be 50% of parent, got {other:?}"),
        }
        match parse_css_dim("3/4") {
            Some(CssDim::Percent(p)) => assert!((p - 0.75).abs() < 1e-6),
            other => panic!("{other:?}"),
        }
        match parse_css_dim("11/12") {
            Some(CssDim::Percent(p)) => assert!((p - 11.0 / 12.0).abs() < 1e-6),
            other => panic!("{other:?}"),
        }
        match dim_kind("1/2".into_dimension()) {
            DimKind::Percent(p) => assert!((p - 0.5).abs() < 1e-6),
            other => panic!("{other:?}"),
        }
        assert!(
            parse_css_dim("3/2").is_none(),
            "ratio > 1 is not a size fraction"
        );
        assert!(parse_css_dim("1/0").is_none());
        let ctx = UnitCtx::new(800.0, 600.0, 16.0).with_containing_block(Some(260.0), Some(180.0));
        assert!(
            (parse_css_dim("1/2")
                .unwrap()
                .resolve_px(&ctx, PercentAxis::Inline)
                .unwrap()
                - 130.0)
                .abs()
                < 0.5,
            "1/2 of parent content 260"
        );
        assert!(
            (parse_css_dim("1/2")
                .unwrap()
                .resolve_px(&ctx, PercentAxis::Block)
                .unwrap()
                - 90.0)
                .abs()
                < 0.5,
            "1/2 of parent content height 180"
        );
    }

    #[test]
    fn fit_min_max_content_keywords() {
        assert!(matches!(parse_css_dim("fit"), Some(CssDim::FitContent)));
        assert!(matches!(
            parse_css_dim("fit-content"),
            Some(CssDim::FitContent)
        ));
        assert!(matches!(
            parse_css_dim("FIT-CONTENT"),
            Some(CssDim::FitContent)
        ));
        assert!(matches!(
            parse_css_dim("min-content"),
            Some(CssDim::MinContent)
        ));
        assert!(matches!(parse_css_dim("min"), Some(CssDim::MinContent)));
        assert!(matches!(
            parse_css_dim("max-content"),
            Some(CssDim::MaxContent)
        ));
        assert!(matches!(parse_css_dim("max"), Some(CssDim::MaxContent)));
        assert!(parse_css_dim("fit").unwrap().is_content_keyword());
        match dim_kind("fit-content".into_dimension()) {
            DimKind::Auto => {}
            other => panic!("fit-content maps to Auto for Taffy, got {other:?}"),
        }
        assert!(matches!("fit".into_css_dim(), CssDim::FitContent));
    }

    #[test]
    fn padding_margin_percent_bakes_against_containing_block_width() {
        let ctx = UnitCtx::new(800.0, 100.0, 16.0).with_containing_block(Some(400.0), Some(80.0));
        let pad = CssDim::Percent(0.10).to_box_edge_length_percentage(&ctx);
        assert!(
            (lp_length_px(pad).unwrap() - 40.0).abs() < 0.01,
            "10% of cb width 400 → 40, not 10% of height 80"
        );
        let mar = CssDim::Percent(0.25).to_box_edge_length_percentage_auto(&ctx);
        match lpa_kind(mar) {
            DimKind::Length(px) => assert!((px - 100.0).abs() < 0.01, "25% of 400 → {px}"),
            other => panic!("{other:?}"),
        }
        let unk = UnitCtx::new(800.0, 100.0, 16.0);
        let zero = CssDim::Percent(0.10).to_box_edge_length_percentage(&unk);
        assert!(
            (lp_length_px(zero).unwrap()).abs() < 0.01,
            "cyclic / unknown CB → 0"
        );
    }

    #[test]
    fn calc_size_parses_and_evals_size_plus_px() {
        let dim = parse_css_dim("calc-size(auto, size + 20px)").expect("parse");
        match &dim {
            CssDim::CalcSize { basis, calc } => {
                assert!(matches!(**basis, CssDim::Auto));
                assert!(calc.contains_size_keyword());
            }
            other => panic!("{other:?}"),
        }
        let ctx = UnitCtx::new(800.0, 600.0, 16.0);
        let px = dim
            .eval_calc_size(100.0, &ctx, PercentAxis::Block)
            .expect("eval");
        assert!(
            (px - 120.0).abs() < 1e-3,
            "calc-size(auto, size + 20px) of 100 → {px}"
        );
    }

    #[test]
    fn calc_size_half_max_content() {
        let dim = parse_css_dim("calc-size(max-content, size * 0.5)").expect("parse");
        let ctx = UnitCtx::new(800.0, 600.0, 16.0);
        let px = dim
            .eval_calc_size(80.0, &ctx, PercentAxis::Inline)
            .expect("eval");
        assert!((px - 40.0).abs() < 1e-3, "got {px}");
    }

    #[test]
    fn px_rem_em_helpers_and_live_root() {
        assert!(matches!(px(4.0), CssDim::Px(v) if (v - 4.0).abs() < 1e-6));
        assert!(matches!(rem(0.5), CssDim::Rem(v) if (v - 0.5).abs() < 1e-6));
        assert!(matches!(em(1.25), CssDim::Em(v) if (v - 1.25).abs() < 1e-6));
        assert!((IntoF32::into_f32(px(4.0)) - 4.0).abs() < 1e-6);
        assert_eq!(parse_css_dim("1rem"), Some(rem(1.0)));
        let rem1 = IntoF32::into_f32(rem(1.0));
        let rem1_str = IntoF32::into_f32("1rem");
        assert!(
            (rem1_str - rem1).abs() < 1e-6,
            "rem(1)={rem1} \"1rem\"={rem1_str} root={}",
            css_root_font_size_px()
        );
        assert!(rem1 > 4.0, "1rem must not collapse to 1px, got {rem1}");
        match dim_kind(rem(1.0).into_dimension()) {
            DimKind::Length(px) => assert!((px - rem1).abs() < 1e-6, "1rem → {px}"),
            other => panic!("{other:?}"),
        }
        let ctx = UnitCtx::new(800.0, 600.0, 24.0);
        assert!((ctx.root_font_size - css_root_font_size_px()).abs() < 1e-6);
        match dim_kind(CssDim::Em(2.0).to_dimension(&ctx)) {
            DimKind::Length(px) => assert!((px - 48.0).abs() < 1e-6, "2em@24 → {px}"),
            other => panic!("{other:?}"),
        }
    }
}

pub trait IntoLengthPercentage {
    fn into_length_percentage(self) -> LengthPercentage;
}

impl<T: IntoDimension> IntoLengthPercentage for T {
    fn into_length_percentage(self) -> LengthPercentage {
        dimension_to_lp(self.into_dimension())
    }
}

/// CSS `border-width` 1–4 values → **T R B L** logical px.
pub trait IntoBorderWidths {
    fn into_border_widths(self) -> [f32; 4];
}

impl IntoBorderWidths for f32 {
    fn into_border_widths(self) -> [f32; 4] {
        [self.max(0.0); 4]
    }
}

impl IntoBorderWidths for i32 {
    fn into_border_widths(self) -> [f32; 4] {
        [self.max(0) as f32; 4]
    }
}

impl IntoBorderWidths for [f32; 4] {
    fn into_border_widths(self) -> [f32; 4] {
        self.map(|v| v.max(0.0))
    }
}

impl IntoBorderWidths for &str {
    fn into_border_widths(self) -> [f32; 4] {
        let parts: Vec<f32> = self
            .split_whitespace()
            .map(|p| {
                parse_css_length_px(p, css_root_font_size_px())
                    .unwrap_or(0.0)
                    .max(0.0)
            })
            .collect();
        match parts.as_slice() {
            [a] => [*a; 4],
            [v, h] => [*v, *h, *v, *h],
            [t, h, b] => [*t, *h, *b, *h],
            [t, r, b, l] => [*t, *r, *b, *l],
            _ => [0.0; 4],
        }
    }
}

impl IntoBorderWidths for String {
    fn into_border_widths(self) -> [f32; 4] {
        self.as_str().into_border_widths()
    }
}

impl IntoBorderWidths for &String {
    fn into_border_widths(self) -> [f32; 4] {
        self.as_str().into_border_widths()
    }
}

/// Length-ish `f32` for typography / borders / blur (logical px after resolve).
///
/// Tailwind / ShadCN 对齐（与 [`CssDim`] / utility 一致）：
/// - **`&str`**：`px` / `rem` / `em`；**无单位数字 = rem**（[`css_root_font_size_px`]），如 `"0.875"` → 0.875rem
/// - **Rust 数值**（`f32` / 整数 / `f64`）：**逻辑 px**（同 Radix `sideOffset={4}`、引擎 `.w(100)`）
/// - **[`px`] / [`rem`] / [`em`]**：显式单位（builder 推荐）
/// - **utility class**（`p-4`）：仍走 [`rui_style_tokens`] spacing scale（`n × 0.25rem`），不经本 trait
///
/// 无单位 CSS 数（`font-weight` / `flex-grow` / leading 倍率）请用 [`IntoF32Raw`]。
pub trait IntoF32 {
    fn into_f32(self) -> f32;
}

impl IntoF32 for f32 {
    fn into_f32(self) -> f32 {
        self
    }
}

impl IntoF32 for f64 {
    fn into_f32(self) -> f32 {
        self as f32
    }
}

impl IntoF32 for &str {
    fn into_f32(self) -> f32 {
        parse_css_length_px(self, css_root_font_size_px()).unwrap_or(0.0)
    }
}

impl IntoF32 for CssDim {
    fn into_f32(self) -> f32 {
        let root = css_root_font_size_px();
        let ctx = UnitCtx::new(0.0, 0.0, root);
        self.resolve_px(&ctx, PercentAxis::Inline)
            .or_else(|| self.preview_dimension().and_then(dim_length_px))
            .unwrap_or(0.0)
    }
}

macro_rules! impl_into_f32_int {
    ($($t:ty),+ $(,)?) => {$(
        impl IntoF32 for $t {
            fn into_f32(self) -> f32 {
                self as f32
            }
        }
    )+};
}
impl_into_f32_int!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);

/// Unitless / raw `f32`（**不**做 rem 换算）。
///
/// 对齐 Tailwind / CSS：`font-weight: 600`、`flex-grow: 1`、`opacity`、leading 倍率等。
/// `&str` 仅 `parse` 数字（`"600"` → 600），与 [`IntoF32`] 的长度语义分离。
pub trait IntoF32Raw {
    fn into_f32_raw(self) -> f32;
}

impl IntoF32Raw for f32 {
    fn into_f32_raw(self) -> f32 {
        self
    }
}

impl IntoF32Raw for f64 {
    fn into_f32_raw(self) -> f32 {
        self as f32
    }
}

impl IntoF32Raw for &str {
    fn into_f32_raw(self) -> f32 {
        self.trim().parse().unwrap_or(0.0)
    }
}

macro_rules! impl_into_f32_raw_int {
    ($($t:ty),+ $(,)?) => {$(
        impl IntoF32Raw for $t {
            fn into_f32_raw(self) -> f32 {
                self as f32
            }
        }
    )+};
}
impl_into_f32_raw_int!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);

/// CSS `opacity`: a unitless **0–1** number, or a **percentage** (`"40%"` / `40` → `0.4`).
///
/// - `0`, `0.4`, `1` / `"0.4"` stay in `0..=1`
/// - `"40%"` / `"100%"` divide by 100
/// - integers **greater than 1** (and `<= 100`) are percentages so `opacity={40}` ≡ `40%`
pub fn parse_opacity(value: impl IntoOpacity) -> f32 {
    value.into_opacity()
}

pub trait IntoOpacity {
    fn into_opacity(self) -> f32;
}

fn opacity_from_unitless(v: f32) -> f32 {
    if !v.is_finite() {
        return 0.0;
    }
    if v > 1.0 {
        (v / 100.0).clamp(0.0, 1.0)
    } else {
        v.clamp(0.0, 1.0)
    }
}

impl IntoOpacity for f32 {
    fn into_opacity(self) -> f32 {
        opacity_from_unitless(self)
    }
}

impl IntoOpacity for f64 {
    fn into_opacity(self) -> f32 {
        opacity_from_unitless(self as f32)
    }
}

impl IntoOpacity for &str {
    fn into_opacity(self) -> f32 {
        let t = self.trim();
        if let Some(raw) = t.strip_suffix('%') {
            return raw
                .trim()
                .parse::<f32>()
                .map(|v| (v / 100.0).clamp(0.0, 1.0))
                .unwrap_or(0.0);
        }
        t.parse::<f32>().map(opacity_from_unitless).unwrap_or(0.0)
    }
}

impl IntoOpacity for String {
    fn into_opacity(self) -> f32 {
        self.as_str().into_opacity()
    }
}

impl IntoOpacity for &String {
    fn into_opacity(self) -> f32 {
        self.as_str().into_opacity()
    }
}

macro_rules! impl_into_opacity_int {
    ($($t:ty),+ $(,)?) => {$(
        impl IntoOpacity for $t {
            fn into_opacity(self) -> f32 {
                opacity_from_unitless(self as f32)
            }
        }
    )+};
}
impl_into_opacity_int!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);
