use taffy::prelude::{CompactLength, Dimension, LengthPercentage, LengthPercentageAuto};

pub const CSS_ROOT_FONT_SIZE_PX: f32 = 16.0;

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
    let trimmed = s.trim();
    if let Some(raw) = trimmed.strip_suffix("px") {
        return parse_css_number(raw);
    }
    if let Some(raw) = trimmed.strip_suffix("rem") {
        return parse_css_number(raw).map(|v| v * CSS_ROOT_FONT_SIZE_PX);
    }
    if let Some(raw) = trimmed.strip_suffix("em") {
        return parse_css_number(raw).map(|v| v * em_base_px);
    }
    // Unsuffixed numeric values default to rem (ShadCN-friendly semantics).
    parse_css_number(trimmed).map(|v| v * CSS_ROOT_FONT_SIZE_PX)
}

/// Convert builder / RSX size tokens into a Taffy [`Dimension`].
///
/// # Percentage (`"50%"`, `"100%"`)
/// Resolves against the **containing block's content box** (CSS): the parent's
/// content width/height after subtracting that parent's padding and border.
/// It does **not** include the parent's margin.
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
        let s = self.trim();
        if let Some(rest) = s.strip_suffix('%') {
            if let Ok(val) = rest.parse::<f32>() {
                return Dimension::percent(val / 100.0);
            }
        } else if let Some(rest) = strip_viewport_unit(s) {
            // Taffy Dimension has no viewport units; map to percent of the containing
            // block (portal body mount ≈ viewport for Dialog / floating shells).
            if let Ok(val) = rest.parse::<f32>() {
                return Dimension::percent(val / 100.0);
            }
        } else if let Some(val) = parse_css_length_px(s, CSS_ROOT_FONT_SIZE_PX) {
            return Dimension::length(val);
        }
        Dimension::auto()
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
}

impl UnitCtx {
    pub fn new(viewport_w: f32, viewport_h: f32, font_size: f32) -> Self {
        Self {
            viewport_w: viewport_w.max(0.0),
            viewport_h: viewport_h.max(0.0),
            font_size: if font_size.is_finite() && font_size > 0.0 {
                font_size
            } else {
                CSS_ROOT_FONT_SIZE_PX
            },
            root_font_size: CSS_ROOT_FONT_SIZE_PX,
            cb_w: None,
            cb_h: None,
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
    /// 0..=1 fraction of containing block (Taffy Percent).
    Percent(f32),
    Em(f32),
    Rem(f32),
    /// 0..=1 fraction of layout viewport width (`vw` / `dvw` / `svw` / `lvw`).
    Vw(f32),
    /// 0..=1 fraction of layout viewport height (`vh` / `dvh` / `svh` / `lvh`).
    Vh(f32),
    CalcAdd(Box<CssDim>, Box<CssDim>),
    CalcSub(Box<CssDim>, Box<CssDim>),
}

impl CssDim {
    pub fn parse(s: &str) -> Option<Self> {
        parse_css_dim(s.trim())
    }

    pub fn needs_layout_context(&self) -> bool {
        match self {
            Self::Em(_) | Self::Vw(_) | Self::Vh(_) => true,
            Self::CalcAdd(a, b) | Self::CalcSub(a, b) => {
                a.needs_layout_context() || b.needs_layout_context() || self.has_percent()
            }
            _ => false,
        }
    }

    pub fn has_percent(&self) -> bool {
        match self {
            Self::Percent(_) => true,
            Self::CalcAdd(a, b) | Self::CalcSub(a, b) => a.has_percent() || b.has_percent(),
            _ => false,
        }
    }

    /// Immediate Taffy value when no viewport / element font-size is required.
    pub fn as_immediate_dimension(&self) -> Option<Dimension> {
        match self {
            Self::Auto => Some(Dimension::auto()),
            Self::Px(v) => Some(Dimension::length(*v)),
            Self::Percent(p) => Some(Dimension::percent(*p)),
            Self::Rem(v) => Some(Dimension::length(*v * CSS_ROOT_FONT_SIZE_PX)),
            _ => None,
        }
    }

    /// Preview stored on `Style` before layout (vw/vh ≈ parent % until resolve).
    pub fn preview_dimension(&self) -> Option<Dimension> {
        if let Some(d) = self.as_immediate_dimension() {
            return Some(d);
        }
        match self {
            Self::Em(v) => Some(Dimension::length(*v * CSS_ROOT_FONT_SIZE_PX)),
            Self::Vw(p) | Self::Vh(p) => Some(Dimension::percent(*p)),
            _ => None,
        }
    }

    pub fn resolve_px(&self, ctx: &UnitCtx, axis: PercentAxis) -> Option<f32> {
        match self {
            Self::Auto => None,
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
            Self::CalcAdd(a, b) => Some(a.resolve_px(ctx, axis)? + b.resolve_px(ctx, axis)?),
            Self::CalcSub(a, b) => Some(a.resolve_px(ctx, axis)? - b.resolve_px(ctx, axis)?),
        }
    }

    pub fn to_dimension(&self, ctx: &UnitCtx) -> Dimension {
        self.to_dimension_axis(ctx, PercentAxis::Inline)
    }

    pub fn to_dimension_axis(&self, ctx: &UnitCtx, axis: PercentAxis) -> Dimension {
        if let Self::Percent(p) = *self {
            return Dimension::percent(p);
        }
        if let Self::Auto = self {
            return Dimension::auto();
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
}

pub fn parse_css_dim(s: &str) -> Option<CssDim> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if s.eq_ignore_ascii_case("auto") {
        return Some(CssDim::Auto);
    }
    if let Some(inner) = s.strip_prefix("calc(").and_then(|t| t.strip_suffix(')')) {
        return parse_calc(inner.trim());
    }
    if let Some(rest) = s.strip_suffix('%') {
        let val: f32 = rest.trim().parse().ok()?;
        return Some(CssDim::Percent(val / 100.0));
    }
    if let Some(rest) = strip_viewport_unit(s) {
        let val: f32 = rest.trim().parse().ok()?;
        let frac = val / 100.0;
        let lower = s.to_ascii_lowercase();
        if lower.contains("vw") {
            return Some(CssDim::Vw(frac));
        }
        return Some(CssDim::Vh(frac));
    }
    if let Some(raw) = s.strip_suffix("rem") {
        return parse_css_number(raw).map(CssDim::Rem);
    }
    if let Some(raw) = s.strip_suffix("em") {
        return parse_css_number(raw).map(CssDim::Em);
    }
    if let Some(raw) = s.strip_suffix("px") {
        return parse_css_number(raw).map(CssDim::Px);
    }
    parse_css_number(s).map(|v| CssDim::Rem(v))
}

fn parse_calc(inner: &str) -> Option<CssDim> {
    let (left, op, right) = split_calc_binop(inner)?;
    let a = parse_css_dim(left)?;
    let b = parse_css_dim(right)?;
    Some(match op {
        '+' => CssDim::CalcAdd(Box::new(a), Box::new(b)),
        '-' => CssDim::CalcSub(Box::new(a), Box::new(b)),
        _ => return None,
    })
}

fn split_calc_binop(s: &str) -> Option<(&str, char, &str)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == '+' || (c == '-' && i > 0) {
            let left = s[..i].trim();
            let right = s[i + 1..].trim();
            if !left.is_empty() && !right.is_empty() {
                return Some((left, c, right));
            }
        }
        i += 1;
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
        // IntoDimension 无节点上下文：em ≡ rem ≡ 根 16px。布局期 CssDim 用本节点 font-size。
        match dim_kind("1em".into_dimension()) {
            DimKind::Length(px) => assert!((px - 16.0).abs() < 1e-6, "1em → {px}"),
            other => panic!("expected length, got {other:?}"),
        }
        match dim_kind("0.5em".into_dimension()) {
            DimKind::Length(px) => assert!((px - 8.0).abs() < 1e-6, "0.5em → {px}"),
            other => panic!("expected length, got {other:?}"),
        }
        assert!((IntoF32::into_f32("1.25em") - 20.0).abs() < 1e-6);
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
                parse_css_length_px(p, CSS_ROOT_FONT_SIZE_PX)
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

pub trait IntoF32 {
    fn into_f32(self) -> f32;
}

impl IntoF32 for f32 {
    fn into_f32(self) -> f32 {
        self
    }
}

impl IntoF32 for &str {
    fn into_f32(self) -> f32 {
        parse_css_length_px(self, CSS_ROOT_FONT_SIZE_PX).unwrap_or(0.0)
    }
}

pub trait IntoF32Raw {
    fn into_f32_raw(self) -> f32;
}

impl IntoF32Raw for f32 {
    fn into_f32_raw(self) -> f32 {
        self
    }
}

impl IntoF32Raw for &str {
    fn into_f32_raw(self) -> f32 {
        self.trim().parse().unwrap_or(0.0)
    }
}
