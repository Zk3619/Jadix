//! Runtime utility-class application — single apply path for static `ui!` classes and
//! dynamic `bind_class` / `bind_style` updates.
//!
//! Token recognition lives in [`jadix_style_tokens`] so the macro can warn on unknown
//! classes against the same catalog.

use jadix_style_tokens::{
    classify_utility, parse_class_list, KnownUtility, ParsedClassList, TwLength, VariantSelector,
};

use std::borrow::Cow;
use std::cell::Cell;
use std::collections::HashSet;

use crate::core::tree::{Element, LayoutStatus, PointerEvents};
use crate::layout::cols_equal_fr;
use crate::primitives::interactive::mark_hover_paint_dirty;
use crate::style::{
    mark_variant_style_override, parse_color, parse_text_color, Style, TextAlign, TextStyle,
    VariantStyleField,
};

thread_local! {
    static FRAME_DT: Cell<f32> = const { Cell::new(1.0 / 60.0) };
}

/// Set the frame delta-time (seconds) for callers that still read [`frame_dt`].
///
/// CSS `transition-*` progress uses a **monotonic** timeline (`Instant`), not this `dt`.
/// Do not floor near-zero values to invent time for other chrome either.
pub fn set_frame_dt(dt: f32) {
    FRAME_DT.with(|c| c.set(dt.max(0.0)));
}

pub(crate) fn frame_dt() -> f32 {
    FRAME_DT.with(|c| c.get())
}

/// Whether `after` differs from `before` in fields that feed Taffy / flex geometry.
///
/// Paint-only transform/opacity/color changes must not call `update_taffy_node_style`
/// (taffy `set_style` marks dirty even when layout props are identical).
fn style_needs_taffy_sync(before: &Style, after: &Style) -> bool {
    let mut a = before.clone();
    let mut b = after.clone();
    // Normalize paint-only fields, then compare the rest.
    for s in [&mut a, &mut b] {
        s.bg_color = [0.0; 4];
        s.border_color = [0.0; 4];
        s.outline_color = [0.0; 4];
        s.outline_width = 0.0;
        s.outline_offset = 0.0;
        s.shadow_color = [0.0; 4];
        s.shadow_offset = [0.0; 2];
        s.shadow_blur = 0.0;
        s.shadow_spread = 0.0;
        s.shadow_inset = false;
        s.extra_shadows.clear();
        s.bg_gradient = None;
        s.bg_position = [0.0; 2];
        s.clip_path = crate::style::ClipPath::None;
        s.scroll_timeline_name = None;
        s.view_timeline_name = None;
        s.interpolate_size = crate::style::InterpolateSize::NumericOnly;
        s.opacity = 1.0;
        s.translate = [0.0; 2];
        s.translate_pct = [0.0; 2];
        s.radii = [0.0; 4];
        s.rotate_deg = 0.0;
        s.scale = [1.0; 2];
        s.scale_z = 1.0;
        s.transform = None;
        s.transform_origin = Default::default();
        s.filter = None;
        s.transition.clear();
        // offset-path motion is compositor/paint (same as translate). Leaving these in the
        // comparison made `gal-offset-run` dirty Taffy every frame on the gallery layout page.
        s.offset_distance = 0.0;
        s.offset_rotate_deg = None;
        s.offset_path = crate::style::OffsetPath::None;
        s.will_change = crate::style::WillChangeHint::Auto;
        s.backdrop_blur = 0.0;
        s.glass = None;
    }
    a != b
}

/// Snapshot + variant rules attached to an [`Element`] after [`apply_class_list`].
#[derive(Clone, Debug, Default)]
pub struct ClassSet {
    pub base: Vec<String>,
    /// Each entry: all selectors must match (AND), then apply the utility.
    pub variants: Vec<(Vec<VariantSelector>, String)>,
    pub base_style: Option<Style>,
    pub base_text_style: Option<TextStyle>,
    /// Last resolved interaction fingerprint; skip `Style` clone when unchanged.
    last_ctx_fp: Option<u64>,
    /// Last `:hover` geometry sample from [`sync_class_variants_subtree`].
    /// CursorMoved uses this via [`class_variant_hover_needs_frame`].
    pub(crate) last_pointer_hover: Option<bool>,
}

impl ClassSet {
    pub fn set_base_dimension(
        &mut self,
        slot: crate::style::UnitSlot,
        dim: crate::layout::dimension::CssDim,
    ) {
        if let Some(base) = self.base_style.as_mut() {
            base.set_css_dim(slot, dim);
            self.last_ctx_fp = None;
        }
    }

    /// `hover:scale-*` / `hover:rotate-*` / `hover:skew-*` (and `group-hover:`).
    /// Idle nodes will apply a linear xform on pointer enter; sticky Bake avoids a
    /// Coverage→Bake pop (Twemoji chips, gallery rotate tiles).
    pub fn has_hover_linear_xform(&self) -> bool {
        self.variants.iter().any(|(sels, util)| {
            let hover = sels.iter().any(|s| {
                matches!(
                    s,
                    VariantSelector::Hover | VariantSelector::GroupHover { .. }
                )
            });
            hover && is_linear_xform_utility(util)
        })
    }

    /// Pointer `:hover` linear xform (not `group-hover:`). Scroll-freeze enter uses this
    /// so a child with `group-hover:scale` does not enter from its own painted AABB.
    pub fn has_pointer_hover_linear_xform(&self) -> bool {
        self.variants.iter().any(|(sels, util)| {
            let hover = sels.iter().any(|s| matches!(s, VariantSelector::Hover));
            hover && is_linear_xform_utility(util)
        })
    }

    /// `hover:bg-*` / `hover:text-*` / `hover:underline` (ShadCN chip / link hover).
    pub fn has_hover_color(&self) -> bool {
        self.variants.iter().any(|(sels, util)| {
            let hover = sels.iter().any(|s| matches!(s, VariantSelector::Hover));
            hover && is_hover_paint_utility(util)
        })
    }

    /// Pointer-dependent selectors (`hover:` / `group-hover:` / `peer-hover:`).
    ///
    /// `sm:` / `dark:` / `data-*` also live in [`Self::variants`], but entering those
    /// boxes must not `request_redraw` — that storms Marker `animate-spin` while the
    /// pointer slides ordinary gallery padding.
    pub fn has_pointer_hover_variants(&self) -> bool {
        self.variants.iter().any(|(sels, _)| {
            sels.iter().any(|s| {
                matches!(
                    s,
                    VariantSelector::Hover
                        | VariantSelector::GroupHover { .. }
                        | VariantSelector::PeerHover { .. }
                )
            })
        })
    }
}

fn is_hover_paint_utility(util: &str) -> bool {
    let u = util.trim();
    u.starts_with("bg-")
        || u.starts_with("text-")
        || u == "underline"
        || u == "no-underline"
        || u.starts_with("underline-offset")
}

fn is_linear_xform_utility(util: &str) -> bool {
    let u = util.trim();
    u.starts_with("scale") || u.starts_with("rotate") || u.starts_with("skew")
}

/// Interaction / theme context used to resolve variant utilities each frame.
#[derive(Clone, Debug)]
pub struct VariantContext<'a> {
    pub hovered: bool,
    pub focused: bool,
    pub disabled: bool,
    pub active: bool,
    pub dark: bool,
    /// Logical viewport width (for `sm:` / `md:` / … breakpoints).
    pub viewport_w: f32,
    /// Ancestor `group` / `group/name` markers that are currently hovered.
    /// `None` name entry means the anonymous `group` class.
    pub group_hovered: &'a [Option<&'a str>],
    /// Previous sibling `peer` / `peer/name` markers that are currently hovered.
    pub peer_hovered: &'a [Option<&'a str>],
    /// Raw element attrs (for `data-[state=…]`).
    pub attrs: Option<&'a std::collections::HashMap<String, String>>,
}

impl Default for VariantContext<'static> {
    fn default() -> Self {
        Self {
            hovered: false,
            focused: false,
            disabled: false,
            active: false,
            dark: false,
            viewport_w: 0.0,
            group_hovered: &[],
            peer_hovered: &[],
            attrs: None,
        }
    }
}

impl VariantContext<'_> {
    fn group_matches(&self, name: &Option<String>) -> bool {
        match name {
            None => self.group_hovered.iter().any(|n| n.is_none()),
            Some(want) => self
                .group_hovered
                .iter()
                .any(|n| n.is_some_and(|have| have == want.as_str())),
        }
    }

    fn peer_matches(&self, name: &Option<String>) -> bool {
        match name {
            None => self.peer_hovered.iter().any(|n| n.is_none()),
            Some(want) => self
                .peer_hovered
                .iter()
                .any(|n| n.is_some_and(|have| have == want.as_str())),
        }
    }

    fn matches_selector(&self, sel: &VariantSelector) -> bool {
        match sel {
            VariantSelector::Hover => self.hovered,
            VariantSelector::Focus | VariantSelector::FocusVisible => self.focused,
            VariantSelector::Active => self.active,
            VariantSelector::Disabled => self.disabled,
            VariantSelector::Dark => self.dark,
            VariantSelector::MotionReduce => crate::style::prefers_reduced_motion(),
            VariantSelector::MotionSafe => !crate::style::prefers_reduced_motion(),
            VariantSelector::Breakpoint { min_width } => self.viewport_w >= *min_width as f32,
            VariantSelector::MaxBreakpoint { max_width } => self.viewport_w < *max_width as f32,
            VariantSelector::GroupHover { name } => self.group_matches(name),
            VariantSelector::PeerHover { name } => self.peer_matches(name),
            VariantSelector::Data { key, value } => {
                let Some(attrs) = self.attrs else {
                    return false;
                };
                let data_key = format!("data-{key}");
                attrs
                    .get(&data_key)
                    .or_else(|| attrs.get(key))
                    .is_some_and(|v| v == value)
            }
        }
    }
}

/// Apply a whitespace-separated utility / variant class list onto `el`.
///
/// Base tokens are applied immediately and snapshotted. Variant tokens are stored on
/// [`Element::class_set`] and re-applied by [`resolve_class_variants`].
pub fn apply_class_list(el: &mut Element, classes: &str) -> LayoutStatus {
    *el.text_style = el.specified_text_style();
    el.inherited_text_style = None;
    let caller_classes = classes;
    let has_defaults = el.attrs.contains_key("jadix_default_class")
        || el.attrs.contains_key("jadix_theme_class")
        || el.attrs.contains_key("jadix_stylesheet_class");
    if has_defaults {
        // A bound caller class can change after a resize. Snapshot authored values,
        // not the last viewport's temporary responsive overlay.
        let inner = &mut *el.inner;
        if let Some(cs) = inner.class_set.as_ref() {
            if let Some(style) = &cs.base_style {
                *inner.style = style.clone();
            }
            if let Some(text_style) = &cs.base_text_style {
                *inner.text_style = text_style.clone();
            }
        }
    }
    let combined;
    let classes = if has_defaults {
        let defaults = el
            .attrs
            .get("jadix_default_class")
            .map(String::as_str)
            .unwrap_or("");
        let theme = el
            .attrs
            .get("jadix_theme_class")
            .map(String::as_str)
            .unwrap_or("");
        let sheet = el
            .attrs
            .get("jadix_stylesheet_class")
            .map(String::as_str)
            .unwrap_or("");
        combined = format!("{defaults} {theme} {sheet} {classes}");
        combined.as_str()
    } else {
        classes
    };
    let parsed = parse_class_list(classes);
    // Sheet matching appends this attribute as caller classes. Keeping defaults in
    // it would apply them a second time *after* the sheet and invert precedence.
    apply_parsed_class_list(
        el,
        parsed,
        if has_defaults {
            caller_classes
        } else {
            classes
        },
    )
}

/// Component defaults survive caller `class` replacement, before sheet and caller utilities.
pub fn with_default_class_list(mut el: Element, classes: &str) -> Element {
    el.set_attr("jadix_default_class", classes);
    apply_class_list(&mut el, "");
    el
}

/// Backward-compatible alias used by [`crate::app::reactive::bind_class`].
#[inline]
pub fn apply_utility_classes(el: &mut Element, classes: &str) -> LayoutStatus {
    apply_class_list(el, classes)
}

/// Builder-friendly: apply classes and return the element.
pub fn with_class_list(mut el: Element, classes: &str) -> Element {
    let _ = apply_class_list(&mut el, classes);
    el
}

/// One fragment for [`classes`] / [`classes!`](crate::classes): a class string, optional, or `(token, on)`.
///
/// Join only — does not resolve utility conflicts. Pass the result to a single
/// [`apply_class_list`] / RSX `class={…}` so later tokens win at apply time.
pub trait ClassPart {
    fn push_to(self, out: &mut String);
}

fn append_class_token(out: &mut String, raw: &str) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return;
    }
    if !out.is_empty() {
        out.push(' ');
    }
    out.push_str(trimmed);
}

impl ClassPart for &str {
    fn push_to(self, out: &mut String) {
        append_class_token(out, self);
    }
}

impl ClassPart for String {
    fn push_to(self, out: &mut String) {
        append_class_token(out, &self);
    }
}

impl ClassPart for &String {
    fn push_to(self, out: &mut String) {
        append_class_token(out, self);
    }
}

impl ClassPart for Cow<'_, str> {
    fn push_to(self, out: &mut String) {
        append_class_token(out, self.as_ref());
    }
}

impl<T: ClassPart> ClassPart for Option<T> {
    fn push_to(self, out: &mut String) {
        if let Some(part) = self {
            part.push_to(out);
        }
    }
}

impl<T: ClassPart> ClassPart for (T, bool) {
    fn push_to(self, out: &mut String) {
        if self.1 {
            self.0.push_to(out);
        }
    }
}

/// Append one [`ClassPart`] onto `out`. Building block for [`classes!`](crate::classes).
#[inline]
pub fn push_class_part(out: &mut String, part: impl ClassPart) {
    part.push_to(out);
}

/// Join class fragments into one whitespace-separated list.
///
/// Skips empty strings and `None`. Tuple `("hidden", on)` is the classList form:
/// the token is included only when `on` is true.
///
/// Mixed types (`&str` + `Option<String>` + `(&str, bool)`) need [`classes!`](crate::classes).
pub fn classes(parts: impl IntoIterator<Item = impl ClassPart>) -> String {
    let mut out = String::new();
    for part in parts {
        part.push_to(&mut out);
    }
    out
}

fn apply_parsed_class_list(
    el: &mut Element,
    parsed: ParsedClassList,
    raw_classes: &str,
) -> LayoutStatus {
    let mut impact = LayoutStatus::Clean;
    for token in &parsed.base {
        impact = impact.merge(apply_known_utility(el, token));
    }

    let class_set = ClassSet {
        base: parsed.base,
        variants: parsed.variants,
        base_style: Some(el.style.as_ref().clone()),
        base_text_style: Some(el.text_style.as_ref().clone()),
        last_ctx_fp: None,
        last_pointer_hover: None,
    };
    el.class_set = Some(class_set);
    el.attrs
        .insert("jadix_utility_class".to_string(), raw_classes.to_string());
    sync_chip_typography(el);
    impact
}

/// Append utilities onto an existing [`ClassSet`] (does not replace user `class`).
///
/// Used by chip chrome to restore ShadCN `hover:` / `data-[state=…]` after
/// [`with_class_list`] replaced the snapshot.
pub fn extend_class_list(el: &mut Element, classes: &str) -> LayoutStatus {
    let extra = classes.trim();
    if extra.is_empty() {
        return LayoutStatus::Clean;
    }
    if el.class_set.is_none() {
        return apply_class_list(el, extra);
    }
    let parsed = parse_class_list(extra);
    if parsed.base.is_empty() && parsed.variants.is_empty() {
        return LayoutStatus::Clean;
    }
    let mut impact = LayoutStatus::Clean;
    for token in &parsed.base {
        impact = impact.merge(apply_known_utility(el, token));
    }
    let style = el.style.as_ref().clone();
    let text_style = el.text_style.as_ref().clone();
    if let Some(cs) = el.class_set.as_mut() {
        cs.base.extend(parsed.base);
        cs.variants.extend(parsed.variants);
        cs.base_style = Some(style);
        cs.base_text_style = Some(text_style);
        cs.last_ctx_fp = None;
    }
    let merged = match el.attrs.get("jadix_utility_class") {
        Some(existing) if !existing.trim().is_empty() => {
            format!("{} {extra}", existing.trim())
        }
        _ => extra.to_string(),
    };
    el.attrs.insert("jadix_utility_class".to_string(), merged);
    sync_chip_typography(el);
    impact
}

/// Ensure `classes` hover/data-state variants exist (idempotent).
pub fn ensure_class_list(el: &mut Element, classes: &str) -> LayoutStatus {
    if el.class_set.as_ref().is_some_and(ClassSet::has_hover_color) {
        return LayoutStatus::Clean;
    }
    extend_class_list(el, classes)
}

/// Write rest/selected paint into the class-variant restore snapshot.
///
/// Chrome runs before [`resolve_class_variants`]; without this, restore would
/// revert to build-time colors and wipe the current selected/pressed state.
pub fn patch_class_set_base_paint(
    el: &mut Element,
    bg: [f32; 4],
    fg: [u8; 4],
    shadow: ([f32; 4], f32),
) {
    let radii = el.style.radii;
    let Some(cs) = el.class_set.as_mut() else {
        return;
    };
    let mut dirty = false;
    if let Some(s) = cs.base_style.as_mut() {
        if s.bg_color != bg {
            s.bg_color = bg;
            dirty = true;
        }
        let (sh, blur) = shadow;
        if s.shadow_color != sh || (s.shadow_blur - blur).abs() > 0.01 {
            s.shadow_color = sh;
            s.shadow_blur = blur;
            s.shadow_offset = [0.0, 0.0];
            s.shadow_spread = 0.0;
            dirty = true;
        }
        if s.radii != radii {
            s.radii = radii;
            dirty = true;
        }
    }
    if let Some(t) = cs.base_text_style.as_mut() {
        if t.color != Some(fg) {
            t.color = Some(fg);
            dirty = true;
        }
    }
    if dirty {
        cs.last_ctx_fp = None;
    }
}

pub fn patch_class_set_base_layout(element: &mut Element) {
    let display = element.style.display;
    let grow = element.style.flex_grow;
    let basis = element.style.flex_basis;
    let wrap = element.style.flex_wrap;
    let min_height = element.style.min_size.height;
    let ratio = element.style.aspect_ratio;
    let Some(classes) = element.class_set.as_mut() else {
        return;
    };
    let Some(base) = classes.base_style.as_mut() else {
        return;
    };
    if base.display == display
        && base.flex_grow == grow
        && base.flex_basis == basis
        && base.flex_wrap == wrap
        && base.min_size.height == min_height
        && base.aspect_ratio == ratio
    {
        return;
    }
    base.display = display;
    base.flex_grow = grow;
    base.flex_basis = basis;
    base.flex_wrap = wrap;
    base.min_size.height = min_height;
    base.aspect_ratio = ratio;
    classes.last_ctx_fp = None;
}

/// Keep `display` in the restore snapshot.
///
/// Chrome [`resolve_class_variants`] clones `base_style` after `reactive_sync`.
/// Questionnaire (and any hover-class node) that toggles `Display::None` would
/// otherwise flash Flex for one frame whenever a `data-*` attr changes the
/// variant fingerprint.
pub fn patch_class_set_base_display(el: &mut Element) {
    let display = el.style.display;
    let Some(cs) = el.class_set.as_mut() else {
        return;
    };
    let Some(s) = cs.base_style.as_mut() else {
        return;
    };
    if s.display == display {
        return;
    }
    s.display = display;
    cs.last_ctx_fp = None;
}

/// After ButtonGroup join rewrites inner corners, keep those radii in the restore snapshot.
pub fn patch_class_set_base_radii(el: &mut Element) {
    let radii = el.style.radii;
    let Some(cs) = el.class_set.as_mut() else {
        return;
    };
    let Some(s) = cs.base_style.as_mut() else {
        return;
    };
    if s.radii == radii {
        return;
    }
    s.radii = radii;
    cs.last_ctx_fp = None;
}

/// Keep box-shadow in the restore snapshot (no `shadow-sm` utility).
pub fn patch_class_set_base_shadow(el: &mut Element, color: [f32; 4], blur: f32) {
    let Some(cs) = el.class_set.as_mut() else {
        return;
    };
    let Some(s) = cs.base_style.as_mut() else {
        return;
    };
    if s.shadow_color == color && (s.shadow_blur - blur).abs() <= 0.01 {
        return;
    }
    s.shadow_color = color;
    s.shadow_blur = blur;
    s.shadow_offset = [0.0, 0.0];
    s.shadow_spread = 0.0;
    cs.last_ctx_fp = None;
}

/// Keep jump-button opacity / transform / shadow / `transition` in the restore snapshot.
///
/// `button()` snapshots class chrome *before* MessageScroller writes hide pose and
/// `transition: opacity, transform`. [`resolve_class_variants`] clones that snapshot
/// over the live style every frame, which dropped the motion entries so CSS tick
/// discarded the tween and Latest popped in at the end pose (dest-reset punching
/// wrapping thread).
pub fn patch_class_set_base_motion(el: &mut Element) {
    let opacity = el.style.opacity;
    let scale = el.style.scale;
    let translate = el.style.translate;
    let translate_pct = el.style.translate_pct;
    let shadow_color = el.style.shadow_color;
    let shadow_offset = el.style.shadow_offset;
    let shadow_blur = el.style.shadow_blur;
    let shadow_spread = el.style.shadow_spread;
    let extra_shadows = el.style.extra_shadows.clone();
    let transition = el.style.transition.clone();
    let Some(cs) = el.class_set.as_mut() else {
        return;
    };
    let Some(s) = cs.base_style.as_mut() else {
        return;
    };
    let same = (s.opacity - opacity).abs() <= 0.001
        && s.scale == scale
        && s.translate == translate
        && s.translate_pct == translate_pct
        && s.shadow_color == shadow_color
        && s.shadow_offset == shadow_offset
        && (s.shadow_blur - shadow_blur).abs() <= 0.01
        && (s.shadow_spread - shadow_spread).abs() <= 0.01
        && s.extra_shadows == extra_shadows
        && s.transition == transition;
    if same {
        return;
    }
    s.opacity = opacity;
    s.scale = scale;
    s.translate = translate;
    s.translate_pct = translate_pct;
    s.shadow_color = shadow_color;
    s.shadow_offset = shadow_offset;
    s.shadow_blur = shadow_blur;
    s.shadow_spread = shadow_spread;
    s.extra_shadows = extra_shadows;
    s.transition = transition;
    cs.last_ctx_fp = None;
}

/// Copy the chip's resolved text color onto descendant glyphs.
pub const ATTR_SYNC_LABEL_FG: &str = "jadix_sync_label_fg";

fn sync_chip_label_fg(el: &mut Element) {
    sync_chip_typography(el);
    let parent = el.text_style.clone();
    let sync_fg = el.attrs.get(ATTR_SYNC_LABEL_FG).map(String::as_str) == Some("1");
    if !sync_fg && parent.underline.is_none() && parent.underline_offset.is_none() {
        return;
    }
    let fg = if sync_fg { parent.color } else { None };
    for child in &mut el.children {
        paint_label_text_tree(child, fg, &parent);
    }
}

fn sync_chip_typography(el: &mut Element) {
    if !matches!(
        el.attrs.get("data-slot").map(String::as_str),
        Some("button" | "badge")
    ) {
        return;
    }
    let parent = el.specified_text_style();
    for child in &mut el.children {
        paint_label_typography(child, &parent);
    }
}

fn paint_label_typography(el: &mut Element, parent: &TextStyle) {
    if el.attrs.contains_key("data-slot") || el.text_style.font_family.as_deref() == Some("lucide")
    {
        return;
    }
    if el.text.is_some() {
        if let Some(size) = parent.size {
            el.text_style.size = Some(size);
        }
        if let Some(weight) = parent.weight {
            el.text_style.weight = Some(weight);
        }
        if parent.font_family.is_some() {
            el.text_style.font_family = parent.font_family.clone();
        }
        if parent.line_height_px.is_some() || parent.line_height_scale.is_some() {
            el.text_style.line_height_px = parent.line_height_px;
            el.text_style.line_height_scale = parent.line_height_scale;
            el.text_style.line_height_specified = parent.line_height_specified;
        }
    }
    for child in &mut el.children {
        paint_label_typography(child, parent);
    }
}

fn paint_label_text_tree(el: &mut Element, fg: Option<[u8; 4]>, parent: &TextStyle) {
    if matches!(
        el.attrs.get("data-slot").map(String::as_str),
        Some("kbd" | "kbd-group")
    ) {
        return;
    }
    let is_lucide = el.text_style.font_family.as_deref() == Some("lucide");
    if el.text.is_some() {
        if let Some(fg) = fg {
            if !crate::style::is_variant_style_overridden(&el.attrs, VariantStyleField::Fg)
                && el.text_style.color != Some(fg)
            {
                el.text_style.color = Some(fg);
            }
        }
        if !is_lucide {
            el.text_style.underline = parent.underline;
            if parent.underline_offset.is_some() {
                el.text_style.underline_offset = parent.underline_offset;
            }
            if !el.text_style.decoration_color_specified {
                el.text_style.decoration_color = parent.decoration_color;
            }
        }
    }
    for child in &mut el.children {
        paint_label_text_tree(child, fg, parent);
    }
}

/// Re-apply color-related base utilities after semantic variant chroming.
///
/// Variant bind can repaint container fields; utilities applied via `class` / `tw` that
/// set `bg-*` or `text-*` must win again when they carry override marks.
pub fn reapply_class_color_overrides(el: &mut Element) -> LayoutStatus {
    let tokens: Vec<String> = el
        .class_set
        .as_ref()
        .map(|cs| {
            cs.base
                .iter()
                .filter(|token| {
                    matches!(
                        classify_utility(token),
                        Some(KnownUtility::BgColor(_) | KnownUtility::TextColor(_))
                    )
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let mut impact = LayoutStatus::Clean;
    for token in tokens {
        impact = impact.merge(apply_known_utility(el, &token));
    }
    impact
}

/// Restore base snapshot and apply currently-matching variant utilities.
pub fn resolve_class_variants(el: &mut Element, ctx: &VariantContext<'_>) -> LayoutStatus {
    // ButtonGroup / joined ToggleGroup rewrite inner corners after build. The class
    // snapshot is often taken *before* that join, so restore would flash full
    // `rounded-md` on shared edges (then chrome repairs next frame). Keep the live
    // join radii across the restore.
    // Also keep the live border width. The class snapshot is taken before join
    // and still has the chip's 1px stroke; restoring it paints a per-item SDF
    // ring on top of the group frame.
    // Modal motion is advanced after class resolution on the live portal tree.
    // Responsive variants must not erase its keyframes/pose and restart enter/exit.
    let shell_motion = crate::utils::modal_motion::modal_shell_defer_css_tick(
        el.template_id.as_deref(),
    )
    .then(|| {
        (
            el.style.animations.clone(),
            el.style.transition.clone(),
            el.style.opacity,
            el.style.scale,
            el.style.translate,
            el.style.translate_pct,
        )
    });
    let join_chrome = el.attrs.contains_key("jadix_button_group_edge").then(|| {
        (
            el.style.radii,
            el.style.translate,
            el.style.z_index,
            el.style.border_width,
            el.style.border_width_sides,
        )
    });

    {
        let inner = &mut *el.inner;
        let Some(cs) = inner.class_set.as_ref() else {
            return LayoutStatus::Clean;
        };
        if cs.variants.is_empty() {
            return LayoutStatus::Clean;
        }

        if let Some(s) = cs.base_style.as_ref() {
            *inner.style = s.clone();
        }
        if let Some(t) = cs.base_text_style.as_ref() {
            let computed_align = if inner.text_style.align == Some(TextAlign::MatchParent) {
                inner.text_style.computed_align
            } else {
                None
            };
            *inner.text_style = t.clone();
            if inner.text_style.align == Some(TextAlign::MatchParent) {
                inner.text_style.computed_align = computed_align;
            }
        }
    }

    let mut impact = LayoutStatus::Clean;
    // Collect matching utilities first to avoid borrow issues.
    let mut apply: Vec<String> = Vec::new();
    if let Some(cs) = el.class_set.as_ref() {
        for (sels, util) in &cs.variants {
            if sels.iter().all(|s| ctx.matches_selector(s)) {
                apply.push(util.clone());
            }
        }
    }
    // Variant values are temporary: dimension builders update authored base
    // values too, so detach the snapshot while applying the variant overlay.
    let class_set = el.class_set.take();
    for util in apply {
        impact = impact.merge(apply_known_utility_ex(el, &util, false));
    }
    el.class_set = class_set;
    if let Some((animations, transition, opacity, scale, translate, translate_pct)) = shell_motion {
        el.style.animations = animations;
        el.style.transition = transition;
        el.style.opacity = opacity;
        el.style.scale = scale;
        el.style.translate = translate;
        el.style.translate_pct = translate_pct;
    }
    if let Some((r, t, z, bw, sides)) = join_chrome {
        if el.style.radii != r {
            el.style.radii = r;
            impact = impact.merge(LayoutStatus::StyleChanged);
        }
        if el.style.translate != t {
            el.style.translate = t;
            impact = impact.merge(LayoutStatus::StyleChanged);
        }
        if el.style.z_index != z {
            el.style.z_index = z;
            impact = impact.merge(LayoutStatus::StyleChanged);
        }
        if el.style.border_width != bw || el.style.border_width_sides != sides {
            el.style.border_width = bw;
            el.style.border_width_sides = sides;
            impact = impact.merge(LayoutStatus::StyleChanged);
        }
    }
    sync_chip_label_fg(el);
    el.transition_snapshot.cascade_assigned();
    if el.text_style.align == Some(TextAlign::MatchParent) && el.text_style.computed_align.is_none()
    {
        impact = impact.merge(LayoutStatus::LayoutChanged);
    }
    impact
}

/// Overlay matching hover paint + linear-xform variants without restoring the snapshot.
///
/// Scroll-freeze must not run a full [`resolve_class_variants`]: restoring the
/// snapshot would write rest `scale` over a mid-tween used value and kill the
/// in-flight transition. Overlay writes the *specified* hover target; tick then
/// retargets from the last used value (`PropertyTween::written`).
fn apply_matching_color_class_variants(el: &mut Element, ctx: &VariantContext<'_>) -> LayoutStatus {
    let before = el.style.clone();
    let before_text = el.text_style.clone();
    let mut apply: Vec<String> = Vec::new();
    if let Some(cs) = el.class_set.as_ref() {
        for (sels, util) in &cs.variants {
            if !sels.iter().all(|s| ctx.matches_selector(s)) {
                continue;
            }
            if is_hover_paint_utility(util)
                || is_linear_xform_utility(util)
                || sels
                    .iter()
                    .any(|selector| matches!(selector, VariantSelector::Hover))
            {
                apply.push(util.clone());
            }
        }
    }
    let mut impact = LayoutStatus::Clean;
    let class_set = el.class_set.take();
    for util in apply {
        impact = impact.merge(apply_known_utility_ex(el, &util, false));
    }
    el.class_set = class_set;
    sync_chip_label_fg(el);
    if el.style == before && el.text_style == before_text {
        LayoutStatus::Clean
    } else {
        impact
    }
}

/// Last matching `bg-*` / `text-*` variant for `ctx` (author class, CSS order).
///
/// Button / table-row hover paint writes variant chrome every frame. Callers overlay
/// this so `class="hover:bg-*"` wins like Tailwind `className` after CVA.
pub fn class_variant_color_overlay(
    el: &Element,
    ctx: &VariantContext<'_>,
) -> (Option<[f32; 4]>, Option<[u8; 4]>) {
    let mut bg = None;
    let mut fg = None;
    let Some(cs) = el.class_set.as_ref() else {
        return (None, None);
    };
    for (sels, util) in &cs.variants {
        if !sels.iter().all(|s| ctx.matches_selector(s)) {
            continue;
        }
        match classify_utility(util) {
            Some(KnownUtility::BgColor(name)) => bg = Some(parse_color(name)),
            Some(KnownUtility::TextColor(name)) => fg = Some(parse_text_color(name)),
            _ => {}
        }
    }
    (bg, fg)
}

fn mix_fp(h: &mut u64, x: u64) {
    *h ^= x;
    *h = h.wrapping_mul(0x0000_0100_0000_01b3);
}

fn str_fp(s: &str) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    for b in s.as_bytes() {
        mix_fp(&mut h, *b as u64);
    }
    h
}

fn variant_ctx_fingerprint(
    hovered: bool,
    focused: bool,
    disabled: bool,
    dark: bool,
    viewport_w: f32,
    group_hovered: &[Option<&str>],
    peer_hovered: &[Option<&str>],
    attrs: &std::collections::HashMap<String, String>,
) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    mix_fp(&mut h, hovered as u64);
    mix_fp(&mut h, focused as u64 + 2);
    mix_fp(&mut h, disabled as u64 + 4);
    mix_fp(&mut h, dark as u64 + 8);
    mix_fp(&mut h, crate::style::prefers_reduced_motion() as u64 + 16);
    mix_fp(&mut h, viewport_w.to_bits() as u64);
    for g in group_hovered {
        mix_fp(&mut h, 0x11);
        if let Some(s) = g {
            mix_fp(&mut h, str_fp(s));
        }
    }
    for p in peer_hovered {
        mix_fp(&mut h, 0x22);
        if let Some(s) = p {
            mix_fp(&mut h, str_fp(s));
        }
    }
    let mut attrs_h = 0u64;
    for (k, v) in attrs {
        if k.starts_with("data-") || k == "disabled" || k == "aria-disabled" {
            attrs_h ^= str_fp(k).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ str_fp(v);
        }
    }
    mix_fp(&mut h, attrs_h);
    if h == 0 {
        1
    } else {
        h
    }
}

/// Walk the tree and resolve class variants (chrome sync).
pub fn sync_class_variants_subtree(
    el: &mut Element,
    mouse_pos: [f32; 2],
    focused_id: &str,
    dark: bool,
    viewport_w: f32,
    viewport_h: f32,
    layout_tree: &mut crate::layout::layout_tree::LayoutTree,
    impact: &mut LayoutStatus,
) {
    super::animation::begin_page_css_frame();
    let hit_nodes =
        crate::app::pointer_events::pointer_hit_node_set(el, mouse_pos[0], mouse_pos[1]);
    sync_class_variants_subtree_inner(
        el,
        mouse_pos,
        focused_id,
        dark,
        viewport_w,
        viewport_h,
        &[],
        &[],
        [0.0_f32, 0.0],
        None,
        &hit_nodes,
        layout_tree,
        impact,
    );
}

fn scroller_timeline_progress(el: &Element) -> Option<f32> {
    super::scroll_timeline::scroller_progress_axis(
        el,
        super::scroll_timeline::ScrollTimelineAxis::Block,
    )
}

fn view_timeline_progress(el: &Element, scroll_acc: [f32; 2], viewport_h: f32) -> f32 {
    super::scroll_timeline::view_timeline_progress(el, scroll_acc, viewport_h)
}

// Sampling and scheduling are separate: an offscreen loop still catches up on the
// next paint, but must not wake the entire window just to rotate invisible pixels.
// Keep finite/native players and geometry-changing rules running (completion,
// presence and motion into the viewport can have observable effects).
fn animation_frame_affects_viewport(
    el: &Element,
    scroll_acc: [f32; 2],
    ancestor_linear: crate::style::Mat2d,
    ancestor_scale: [f32; 2],
    viewport_w: f32,
    viewport_h: f32,
) -> bool {
    use super::animatable::{AnimatableValue, PropertyId};
    if !crate::core::tree::element_is_offscreen(el, scroll_acc, viewport_w, viewport_h) {
        return true;
    }
    let mut max_scale = el.style.scale[0].abs().max(el.style.scale[1].abs());
    let mut travel = el.style.translate[0].hypot(el.style.translate[1]);
    let origin = super::transform::origin_px(
        &el.style.transform_origin,
        el.layout_bounds[2],
        el.layout_bounds[3],
    );
    let mut origin_reach = origin[0].hypot(origin[1]);
    for entry in &el.style.animations {
        if entry.playback_id.is_some()
            || !matches!(
                entry.iteration_count,
                super::AnimationIterationCount::Infinite
            )
            || entry.composition != super::AnimationComposition::Replace
        {
            return true;
        }
        let Some(stops) = super::keyframes::lookup_keyframes(&entry.name) else {
            return true;
        };
        for stop in stops.iter() {
            for (&id, value) in stop.props() {
                match (id, value) {
                    (PropertyId::Translate, AnimatableValue::Vec2([x, y]))
                        if x.is_finite() && y.is_finite() =>
                    {
                        travel = travel.max(x.hypot(*y))
                    }
                    (PropertyId::TranslatePct, AnimatableValue::Vec2([x, y]))
                        if x.is_finite() && y.is_finite() =>
                    {
                        travel += (x * el.layout_bounds[2]).hypot(y * el.layout_bounds[3])
                    }
                    (PropertyId::Scale, AnimatableValue::Vec2([x, y]))
                        if x.is_finite() && y.is_finite() =>
                    {
                        max_scale = max_scale.max(x.abs()).max(y.abs())
                    }
                    (PropertyId::TransformOrigin, AnimatableValue::Vec2([x, y]))
                        if x.is_finite() && y.is_finite() =>
                    {
                        origin_reach = origin_reach
                            .max((x * el.layout_bounds[2]).hypot(y * el.layout_bounds[3]));
                    }
                    (
                        PropertyId::Opacity
                        | PropertyId::Rotate
                        | PropertyId::Color
                        | PropertyId::BackgroundColor
                        | PropertyId::BorderColor
                        | PropertyId::OffsetDistance,
                        _,
                    ) => {}
                    _ => return true,
                }
            }
        }
    }
    let Some(path_travel) =
        super::offset_path::max_translate(&el.style, [el.layout_bounds[2], el.layout_bounds[3]])
    else {
        return true;
    };
    travel += path_travel;
    if el.transition_snapshot.has_active_tweens()
        || el.style.transform_origin.z != 0.0
        || el.style.transform.as_ref().is_some_and(|t| !t.is_empty())
    {
        return true;
    }
    // Include overflow ink (including button shadows) in the rotation envelope.
    // A descendant with an independent transform requires a more general bound.
    let b = el.layout_bounds;
    let center = [b[0] + b[2] * 0.5, b[1] + b[3] * 0.5];
    let mut radius = b[2].hypot(b[3]);
    let mut stack: Vec<&Element> = el.children.iter().collect();
    while let Some(child) = stack.pop() {
        if child.style.position_fixed
            || child.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID)
            || child.style.translate != [0.0, 0.0]
            || child.style.scale != [1.0, 1.0]
            || child.style.rotate_deg != 0.0
            || child
                .style
                .transform
                .as_ref()
                .is_some_and(|t| !t.is_empty())
        {
            return true;
        }
        let ink = crate::core::tree::visual_paint_aabb_with_ancestor(
            child,
            [0.0, 0.0],
            crate::style::MAT2D_IDENTITY,
            [1.0, 1.0],
        );
        if ink.iter().any(|value| !value.is_finite()) {
            return true;
        }
        let reach_x = (ink[0] - center[0])
            .abs()
            .max((ink[0] + ink[2] - center[0]).abs());
        let reach_y = (ink[1] - center[1])
            .abs()
            .max((ink[1] + ink[3] - center[1]).abs());
        radius = radius.max(reach_x.hypot(reach_y));
        stack.extend(child.children.iter());
    }
    let v = crate::core::tree::visual_paint_aabb_with_ancestor(
        el,
        scroll_acc,
        ancestor_linear,
        ancestor_scale,
    );
    // A full rotation stays inside this deliberately conservative envelope.
    // The current AABB alone can underestimate a later angle under an
    // anisotropic ancestor transform. Bound every angle using the matrix norm.
    let linear_norm = ancestor_linear[..4]
        .iter()
        .map(|x| x * x)
        .sum::<f32>()
        .sqrt();
    let radius =
        (radius * max_scale + travel + 2.0 * origin_reach * (max_scale + 1.0)) * linear_norm;
    let pad = radius.max(v[2].abs()).max(v[3].abs()) * 2.0 + 32.0;
    !(v[0] + v[2] < -pad
        || v[1] + v[3] < -pad
        || v[0] > viewport_w + pad
        || v[1] > viewport_h + pad)
}

/// During smooth-scroll / scrollbar-fade frames: advance already-running CSS tweens,
/// leave stale `:hover` when content moves away, and enter geometric hover when
/// content slides under the cursor. Preserve the current tween's style snapshot.
pub fn tick_active_css_transitions_subtree(
    el: &mut Element,
    mouse_pos: [f32; 2],
    focused_id: &str,
    dark: bool,
    viewport_w: f32,
    viewport_h: f32,
    scroll_acc: [f32; 2],
    layout_tree: &mut crate::layout::layout_tree::LayoutTree,
    impact: &mut LayoutStatus,
) {
    super::animation::begin_page_css_frame();
    // Identify active branches once. Static list rows need neither transform
    // geometry nor hover resolution on every scroll frame. Recompute from live
    // styles so a newly bound animation or hover variant is never missed.
    fn active_branches(el: &mut Element) -> bool {
        let mut armed = el.class_set.as_ref().and_then(|cs| cs.last_pointer_hover) == Some(true);
        let mut active = el
            .class_set
            .as_ref()
            .is_some_and(|cs| !cs.variants.is_empty() || cs.last_pointer_hover == Some(true))
            || crate::layout::collect_cache::node_paint_needs_frame(el)
            || !el.style.transition.is_empty();
        for child in &mut el.children {
            active |= active_branches(child);
            armed |= child.css_hover_armed;
        }
        el.css_scroll_active = active;
        el.css_hover_armed = armed;
        active
    }
    // A nonzero paint hash means the last collect summarized the same subtree.
    // Reactive class/style edits zero their ancestor hashes before this pass.
    if el.collect_paint_hash == 0 {
        active_branches(el);
    }
    if !el.css_scroll_active {
        return;
    }
    struct Frame {
        ptr: *mut Element,
        scroll_acc: [f32; 2],
        scroll_progress: f32,
        named: Vec<(String, f32)>,
        ancestor_linear: crate::style::Mat2d,
        ancestor_scale: [f32; 2],
        responsive_width: f32,
    }
    let mut stack = vec![Frame {
        ptr: el as *mut Element,
        scroll_acc,
        scroll_progress: 0.0,
        named: Vec::new(),
        ancestor_linear: crate::style::MAT2D_IDENTITY,
        ancestor_scale: [1.0, 1.0],
        responsive_width: viewport_w,
    }];
    let hit_nodes =
        crate::app::pointer_events::pointer_hit_node_set(el, mouse_pos[0], mouse_pos[1]);
    let mut paint_hash_spines: Vec<taffy::prelude::NodeId> = Vec::new();
    while let Some(frame) = stack.pop() {
        // SAFETY: exclusive subtree walk of the `el` passed to this function.
        let el = unsafe { &mut *frame.ptr };
        if !el.css_scroll_active {
            continue;
        }
        let responsive_width = el
            .attrs
            .get("data-jadix-viewport-width")
            .and_then(|value| value.parse::<f32>().ok())
            .filter(|width| width.is_finite() && *width > 0.0)
            .unwrap_or(frame.responsive_width);
        let is_portal =
            el.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID);
        let scroll_acc = if el.style.position_fixed || is_portal {
            [0.0_f32, 0.0]
        } else {
            frame.scroll_acc
        };

        let offscreen =
            crate::core::tree::element_is_offscreen(el, scroll_acc, viewport_w, viewport_h);

        let has_spin = el.attrs.get("jadix_animate").map(|v| v.as_str()) == Some("spin");
        let has_css_anim = !el.style.animations.is_empty()
            || super::animation::css_animation_needs_frame(&el.style, &el.animation_snapshot);
        let has_active_css = el.transition_snapshot.has_active_tweens();
        let css_needs =
            super::animation::css_transition_needs_frame(&el.style, &el.transition_snapshot);
        let prev_pointer_hover =
            el.class_set.as_ref().and_then(|cs| cs.last_pointer_hover) == Some(true);
        let force_leave = class_hover_should_leave(
            el,
            mouse_pos,
            scroll_acc,
            frame.ancestor_linear,
            frame.ancestor_scale,
            &hit_nodes,
        );
        let force_hover_enter = !force_leave
            && class_hover_should_enter(
                el,
                mouse_pos,
                scroll_acc,
                frame.ancestor_linear,
                frame.ancestor_scale,
                &hit_nodes,
            );
        // Color `:hover` has no tween. Strip blit copies last frame's hover ink, so
        // the first enter / leave must named-pin dest-reset. Still-hovered re-apply
        // after chrome restamp is paint-in-place — the blit already carries hover.
        let hover_edge = force_leave || (force_hover_enter && !prev_pointer_hover);
        // Culled nodes still advance in-flight tweens. Spring has duration_ms=0 so the
        // duration clock cannot "catch up" after a skip — it looks like a forced pause.
        // Settled `:hover` (no active tween) must still leave when the pointer is gone.
        // `collect_skip_live` means a descendant is spin/shimmer/keyframes: do not
        // `continue` the parent or idle chrome never notes `page_css` (maximize /
        // stop-scroll then WaitUntil).
        // A short overflow-visible parent can be layout-offscreen while a hovered
        // child (or `hover:scale` leftover) is still painted in the viewport. Skipping
        // the parent here never runs `force_leave` on that child — hover sticks until
        // settle, when full sync walks the same subtree.
        let hover_walk = force_leave || force_hover_enter || el.css_hover_armed;
        if offscreen
            && !(has_spin
                || has_css_anim
                || has_active_css
                || css_needs
                || hover_walk
                || el.collect_skip_live)
        {
            continue;
        }
        if css_needs
            || (crate::layout::collect_cache::node_paint_needs_frame(el)
                && animation_frame_affects_viewport(
                    el,
                    scroll_acc,
                    frame.ancestor_linear,
                    frame.ancestor_scale,
                    viewport_w,
                    viewport_h,
                ))
        {
            super::animation::note_page_css_needs_frame();
        }
        let defer_shell_css =
            crate::utils::modal_motion::modal_shell_defer_css_tick(el.template_id.as_deref())
                || crate::utils::floating_motion::floating_panel_defer_css_tick(
                    el.template_id.as_deref(),
                );

        let mut named = frame.named.clone();
        super::scroll_timeline::push_named_timelines(el, &mut named, scroll_acc, viewport_h);

        let mut local = LayoutStatus::Clean;
        if force_leave {
            local = resolve_stale_class_hover_leave(el, focused_id, dark, responsive_width);
            // Color `:hover` is geometric. Button `transition-colors` would otherwise
            // write the last used hover fill back after snapshot restore.
            if super::theme_preset() != super::ThemePreset::LiquidGlass {
                let inner = &mut *el.inner;
                super::animation::settle_hover_paint_tweens(
                    &inner.style,
                    &mut inner.text_style,
                    &mut inner.transition_snapshot,
                );
            }
        } else if force_hover_enter {
            // Seed idle transform tween slots from the used (rest) value *before*
            // overlaying `hover:scale`. Otherwise `tick` would `or_insert` with
            // current already at 1.5 and never start a transition.
            if !el.transition_snapshot.has_active_tweens() && !el.style.transition.is_empty() {
                crate::style::ensure_used_layout_from_bounds(el);
                let _ = super::animation::tick_element_transitions_with_text_used(
                    &mut el.inner.style,
                    &mut el.inner.text_style,
                    Some(&el.inner.used_layout),
                    &mut el.inner.transition_snapshot,
                    el.inner.animation_snapshot.lock(),
                    frame_dt(),
                );
            }
            local = resolve_scroll_hover_enter(el, focused_id, dark, responsive_width);
            if super::theme_preset() != super::ThemePreset::LiquidGlass {
                let inner = &mut *el.inner;
                super::animation::settle_hover_paint_tweens(
                    &inner.style,
                    &mut inner.text_style,
                    &mut inner.transition_snapshot,
                );
            }
        }

        if has_spin
            || has_css_anim
            || has_active_css
            || css_needs
            || force_leave
            || force_hover_enter
        {
            let before = el.style.clone();
            let spun = tick_animate_spin(el);
            let css_anim = if !defer_shell_css && (has_css_anim || !el.style.animations.is_empty())
            {
                crate::style::ensure_used_layout_from_bounds(el);
                let progress = super::animation::TimelineProgress {
                    scroll: scroller_timeline_progress(el).unwrap_or(frame.scroll_progress),
                    view: view_timeline_progress(el, scroll_acc, viewport_h),
                };
                let was_active = el.animation_snapshot.has_active(&el.style);
                let active = super::animation::tick_element_animations_ex_named(
                    &mut el.inner.style,
                    &mut el.inner.text_style,
                    &el.inner.used_layout,
                    &mut el.inner.animation_snapshot,
                    frame_dt(),
                    progress,
                    &named,
                );
                el.inner.specified_present = el.inner.animation_snapshot.specified.clone();
                active || was_active
            } else {
                false
            };
            let css_trans = if !defer_shell_css
                && (has_active_css || css_needs || !el.style.transition.is_empty())
            {
                super::animation::tick_element_transitions_with_text_used(
                    &mut el.inner.style,
                    &mut el.inner.text_style,
                    Some(&el.inner.used_layout),
                    &mut el.inner.transition_snapshot,
                    el.inner.animation_snapshot.lock(),
                    frame_dt(),
                )
            } else {
                false
            };
            if spun
                || css_anim
                || css_trans
                || el.style != before
                || local != LayoutStatus::Clean
                || hover_edge
            {
                *impact = impact.merge(local.merge(LayoutStatus::StyleChanged));
                // Spin / keyframes already arm `page_css` + node-present dirty.
                // Hover-dirty is for pointer chrome. CSS ticks must not set it:
                // CursorMoved `take()` anywhere on the page would storm extra paints
                // and freeze Marker `animate-spin`.
                if (local != LayoutStatus::Clean || hover_edge) && !(spun || css_anim || css_trans)
                {
                    mark_hover_paint_dirty();
                }
                if spun || css_anim || css_trans || hover_edge || el.style != before {
                    // Paint-only motion must be a node-present dirty. StyleChanged with an
                    // empty damage list FullClears; a sibling Input PartialLoad would skip
                    // this icon and the retain angle jumps on the next Clear.
                    // Color hover enter/leave has no tween: without a pin, scroll blit
                    // keeps the previous hover/rest ink until settle.
                    // Opacity tween *complete* returns css_trans=false after writing 0;
                    // still dest-reset or Latest hide retains the last faded pill.
                    crate::app::damage::push_frame_paint_element_change(
                        el,
                        scroll_acc,
                        before.opacity,
                    );
                }
                if css_trans || el.style != before || hover_edge {
                    // Hover color leave is paint-only on an idle chip. Passing
                    // `collect_skip_live` skips the bust when a spinner ancestor
                    // marked the leaf live last stamp, and `reuse_idle` then
                    // skip-replays the hovered bake until scroll settles.
                    // Zero that spine only — a global hash bust rehashes the
                    // gallery on every trigger enter/leave.
                    queue_paint_hash_spine(
                        &mut paint_hash_spines,
                        layout_tree,
                        el,
                        el.collect_skip_live && !hover_edge,
                    );
                }
                // Padding/margin/size tweens must request flex recompute. StyleChanged alone
                // is treated as scroll-only paint and leaves geometry stuck at the hover size.
                let need_taffy = matches!(
                    local,
                    LayoutStatus::LayoutChanged | LayoutStatus::StructureChanged
                ) || style_needs_taffy_sync(&before, &el.style);
                if need_taffy {
                    *impact = impact.merge(LayoutStatus::LayoutChanged);
                    if el.layout_node.is_some() {
                        layout_tree.update_taffy_node_style(el);
                    }
                }
            }
        }

        if offscreen && !el.collect_skip_live && !el.css_hover_armed {
            continue;
        }

        let child_progress = scroller_timeline_progress(el).unwrap_or(frame.scroll_progress);
        let child_acc = if is_portal {
            scroll_acc
        } else {
            // Match hit-test: `scroll:` viewports read live offset signals, not
            // a lagging `Element.scroll_offset`.
            crate::app::tree_hit::next_hit_scroll_acc(el, scroll_acc)
        };
        let (child_lin, child_sc) = crate::core::tree::child_paint_ancestor(
            el,
            frame.ancestor_linear,
            frame.ancestor_scale,
        );
        let n = el.children.len();
        for i in (0..n).rev() {
            stack.push(Frame {
                ptr: std::ptr::addr_of_mut!(el.children[i]),
                scroll_acc: child_acc,
                scroll_progress: child_progress,
                named: named.clone(),
                ancestor_linear: child_lin,
                ancestor_scale: child_sc,
                responsive_width,
            });
        }
    }
    apply_paint_hash_spines(el, layout_tree, &paint_hash_spines);
}

/// True when pointer geometry would change any utility-class `:hover` sample.
///
/// Gallery `hover:scale-*` Rects have no `on_hover`, so CursorMoved alone does not
/// `request_redraw`. Without this wake, enter waits for an unrelated frame (scrollbar
/// auto-hide) and leave sticks until another chrome hover forces a redraw.
pub fn class_variant_hover_needs_frame(el: &Element, mouse_pos: [f32; 2]) -> bool {
    let hit_nodes =
        crate::app::pointer_events::pointer_hit_node_set(el, mouse_pos[0], mouse_pos[1]);
    class_variant_hover_needs_frame_inner(el, mouse_pos, [0.0, 0.0], None, &hit_nodes)
}

fn class_variant_hover_needs_frame_inner(
    el: &Element,
    mouse_pos: [f32; 2],
    scroll_acc: [f32; 2],
    clip: Option<[f32; 4]>,
    hit_nodes: &HashSet<*const Element>,
) -> bool {
    struct Frame<'a> {
        el: &'a Element,
        scroll_acc: [f32; 2],
        clip: Option<[f32; 4]>,
        clip_round: OverflowRoundClip,
        ancestor_linear: crate::style::Mat2d,
        ancestor_scale: [f32; 2],
    }
    let mut stack = vec![Frame {
        el,
        scroll_acc,
        clip,
        clip_round: None,
        ancestor_linear: crate::style::MAT2D_IDENTITY,
        ancestor_scale: [1.0, 1.0],
    }];
    while let Some(frame) = stack.pop() {
        let el = frame.el;
        let is_portal =
            el.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID);
        let (scroll_acc, clip, clip_round) = if el.style.position_fixed || is_portal {
            ([0.0_f32, 0.0], None, None)
        } else {
            (frame.scroll_acc, frame.clip, frame.clip_round)
        };

        let b = el.layout_bounds;
        let [lx, ly] = crate::app::tree_hit::visual_xy(el, scroll_acc);
        let layout_box = [lx, ly, b[2], b[3]];
        let vis_box = crate::core::tree::visual_paint_aabb_with_ancestor(
            el,
            scroll_acc,
            frame.ancestor_linear,
            frame.ancestor_scale,
        );
        let inside = class_hover_pointer_active(
            el,
            mouse_pos,
            vis_box,
            clip,
            clip_round,
            scroll_acc,
            frame.ancestor_linear,
            frame.ancestor_scale,
            hit_nodes,
        );

        if let Some(cs) = el.class_set.as_ref() {
            if cs.has_pointer_hover_variants() {
                match cs.last_pointer_hover {
                    Some(prev) if prev == inside => {}
                    Some(_) => return true,
                    None if inside => return true,
                    None => {}
                }
            }
        }

        let child_clip = if is_portal {
            None
        } else if node_clips_pointer(el) {
            intersect_clip(clip, layout_box)
        } else {
            clip
        };
        let child_clip_round = next_overflow_round_clip(el, is_portal, layout_box, clip_round);
        let child_scroll_acc = if is_portal {
            scroll_acc
        } else {
            crate::app::tree_hit::next_hit_scroll_acc(el, scroll_acc)
        };
        let (child_lin, child_sc) = crate::core::tree::child_paint_ancestor(
            el,
            frame.ancestor_linear,
            frame.ancestor_scale,
        );
        for child in el.children.iter().rev() {
            stack.push(Frame {
                el: child,
                scroll_acc: child_scroll_acc,
                clip: child_clip,
                clip_round: child_clip_round,
                ancestor_linear: child_lin,
                ancestor_scale: child_sc,
            });
        }
    }
    false
}

fn pointer_inside_for_class_hover(
    el: &Element,
    mouse_pos: [f32; 2],
    vis_box: [f32; 4],
    clip: Option<[f32; 4]>,
    clip_round: OverflowRoundClip,
    scroll_acc: [f32; 2],
    ancestor_linear: crate::style::Mat2d,
    ancestor_scale: [f32; 2],
) -> bool {
    let in_box = crate::core::tree::point_in_aabb(mouse_pos, vis_box);
    let in_clip = clip
        .is_none_or(|c| c[2] > 0.0 && c[3] > 0.0 && crate::core::tree::point_in_aabb(mouse_pos, c));
    if !(in_box && in_clip) {
        return false;
    }
    if !el.pointer_events_mode().targets_self() {
        return false;
    }
    if let Some((rect, radii)) = clip_round {
        if !crate::style::rounded_rect_contains(rect, radii, mouse_pos) {
            return false;
        }
    }
    if !crate::core::tree::overflow_clip_contains_pointer(
        el,
        mouse_pos,
        scroll_acc,
        ancestor_linear,
        ancestor_scale,
    ) {
        return false;
    }
    if !crate::core::tree::clip_path_contains_pointer(
        el,
        mouse_pos,
        scroll_acc,
        ancestor_linear,
        ancestor_scale,
    ) {
        return false;
    }
    if crate::primitives::path::element_has_path_geometry(&el.attrs) {
        return crate::app::tree_hit::element_self_pointer_hit_with(
            el,
            mouse_pos[0],
            mouse_pos[1],
            scroll_acc,
            ancestor_linear,
            ancestor_scale,
        );
    }
    if el.pointer_events_mode() == PointerEvents::BoundingBox {
        if crate::core::tree::css_spin_locks_pointer_to_layout_box(el) {
            return crate::core::tree::pointer_in_paint_border_box(
                el,
                mouse_pos,
                scroll_acc,
                ancestor_linear,
                ancestor_scale,
            );
        }
        return true;
    }
    crate::core::tree::pointer_in_paint_border_box(
        el,
        mouse_pos,
        scroll_acc,
        ancestor_linear,
        ancestor_scale,
    )
}

/// CSS `:hover` is the exclusive hit chain ([`pointer_hit_node_set`]), not raw AABB.
/// A claiming overlay (ColorPicker popover) must not leave covered chips hovered.
fn class_hover_pointer_active(
    el: &Element,
    mouse_pos: [f32; 2],
    vis_box: [f32; 4],
    clip: Option<[f32; 4]>,
    clip_round: OverflowRoundClip,
    scroll_acc: [f32; 2],
    ancestor_linear: crate::style::Mat2d,
    ancestor_scale: [f32; 2],
    hit_nodes: &HashSet<*const Element>,
) -> bool {
    hit_nodes.contains(&(el as *const Element))
        && pointer_inside_for_class_hover(
            el,
            mouse_pos,
            vis_box,
            clip,
            clip_round,
            scroll_acc,
            ancestor_linear,
            ancestor_scale,
        )
}

/// Pointer is over a chip that has geometric `:hover` paint (`hover:bg/text/underline`)
/// and/or linear transform (`hover:scale/rotate/skew`). CSS `:hover` is geometric:
/// content sliding under a still cursor must enter those variants on the freeze path.
fn class_hover_should_enter(
    el: &Element,
    mouse_pos: [f32; 2],
    scroll_acc: [f32; 2],
    ancestor_linear: crate::style::Mat2d,
    ancestor_scale: [f32; 2],
    hit_nodes: &HashSet<*const Element>,
) -> bool {
    let Some(cs) = el.class_set.as_ref() else {
        return false;
    };
    if !cs.variants.iter().any(|(selectors, _)| {
        selectors
            .iter()
            .any(|selector| matches!(selector, VariantSelector::Hover))
    }) {
        return false;
    }
    let vis_box = crate::core::tree::visual_paint_aabb_with_ancestor(
        el,
        scroll_acc,
        ancestor_linear,
        ancestor_scale,
    );
    class_hover_pointer_active(
        el,
        mouse_pos,
        vis_box,
        None,
        None,
        scroll_acc,
        ancestor_linear,
        ancestor_scale,
        hit_nodes,
    )
}

fn resolve_scroll_hover_enter(
    el: &mut Element,
    focused_id: &str,
    dark: bool,
    viewport_w: f32,
) -> LayoutStatus {
    let focused = el
        .focus_id
        .as_deref()
        .is_some_and(|id| !id.is_empty() && id == focused_id);
    let disabled = el.attrs.get("disabled").is_some_and(|v| v != "false")
        || el.attrs.get("aria-disabled").is_some_and(|v| v == "true");
    let attrs_snapshot = el.attrs.clone();
    let ctx = VariantContext {
        hovered: true,
        focused,
        disabled,
        active: false,
        dark,
        viewport_w,
        group_hovered: &[],
        peer_hovered: &[],
        attrs: Some(&attrs_snapshot),
    };
    let local = apply_matching_color_class_variants(el, &ctx);
    if let Some(cs) = el.class_set.as_mut() {
        cs.last_pointer_hover = Some(true);
        // Full sync after scroll must still re-fingerprint remaining variants.
        cs.last_ctx_fp = None;
    }
    local
}

/// Already-hovered class variants whose paint AABB no longer contains the pointer.
/// Used on the scroll-freeze path so springs / padding / scale can leave.
/// Ancestor cascade matches the live hover walks (a parent rotate this frame must leave/stay
/// against the painted box, not last-collect stamps).
fn class_hover_should_leave(
    el: &Element,
    mouse_pos: [f32; 2],
    scroll_acc: [f32; 2],
    ancestor_linear: crate::style::Mat2d,
    ancestor_scale: [f32; 2],
    hit_nodes: &HashSet<*const Element>,
) -> bool {
    let Some(cs) = el.class_set.as_ref() else {
        return false;
    };
    if cs.last_pointer_hover != Some(true) || cs.variants.is_empty() {
        return false;
    }
    let vis_box = crate::core::tree::visual_paint_aabb_with_ancestor(
        el,
        scroll_acc,
        ancestor_linear,
        ancestor_scale,
    );
    !class_hover_pointer_active(
        el,
        mouse_pos,
        vis_box,
        None,
        None,
        scroll_acc,
        ancestor_linear,
        ancestor_scale,
        hit_nodes,
    )
}

fn resolve_stale_class_hover_leave(
    el: &mut Element,
    focused_id: &str,
    dark: bool,
    viewport_w: f32,
) -> LayoutStatus {
    let focused = el
        .focus_id
        .as_deref()
        .is_some_and(|id| !id.is_empty() && id == focused_id);
    let disabled = el.attrs.get("disabled").is_some_and(|v| v != "false")
        || el.attrs.get("aria-disabled").is_some_and(|v| v == "true");
    let attrs_snapshot = el.attrs.clone();
    let ctx = VariantContext {
        hovered: false,
        focused,
        disabled,
        active: false,
        dark,
        viewport_w,
        group_hovered: &[],
        peer_hovered: &[],
        attrs: Some(&attrs_snapshot),
    };
    let local = resolve_class_variants(el, &ctx);
    if let Some(cs) = el.class_set.as_mut() {
        cs.last_pointer_hover = Some(false);
        cs.last_ctx_fp = None;
    }
    local
}

fn intersect_clip(clip: Option<[f32; 4]>, rect: [f32; 4]) -> Option<[f32; 4]> {
    let c = match clip {
        Some(c) => c,
        None => return Some(rect),
    };
    let x1 = c[0].max(rect[0]);
    let y1 = c[1].max(rect[1]);
    let x2 = (c[0] + c[2]).min(rect[0] + rect[2]);
    let y2 = (c[1] + c[3]).min(rect[1] + rect[3]);
    Some([x1, y1, (x2 - x1).max(0.0), (y2 - y1).max(0.0)])
}

/// Overflow `border-radius` clip inherited by descendants (nested ≈ inner radii only).
type OverflowRoundClip = Option<([f32; 4], [f32; 4])>;

fn next_overflow_round_clip(
    el: &Element,
    is_portal: bool,
    layout_box: [f32; 4],
    inherited: OverflowRoundClip,
) -> OverflowRoundClip {
    if is_portal {
        return None;
    }
    if crate::core::tree::element_clips_overflow(el) {
        if crate::style::radii_affect_clip(el.style.radii) {
            Some((layout_box, el.style.radii))
        } else {
            None
        }
    } else {
        inherited
    }
}

fn node_clips_pointer(el: &Element) -> bool {
    crate::core::tree::element_clips_overflow(el) || !el.style.clip_path.is_none()
}

fn record_peer_hover(
    prev_peer_owned: &mut Vec<Option<String>>,
    child: &Element,
    mouse_pos: [f32; 2],
    child_scroll_acc: [f32; 2],
    child_clip: Option<[f32; 4]>,
    child_clip_round: OverflowRoundClip,
    ancestor_linear: crate::style::Mat2d,
    ancestor_scale: [f32; 2],
    hit_nodes: &HashSet<*const Element>,
) {
    prev_peer_owned.clear();
    if !hit_nodes.contains(&(child as *const Element)) {
        return;
    }
    let child_paint = crate::core::tree::visual_paint_aabb_with_ancestor(
        child,
        child_scroll_acc,
        ancestor_linear,
        ancestor_scale,
    );
    let child_hovered = crate::core::tree::point_in_aabb(mouse_pos, child_paint)
        && child_clip.is_none_or(|c| {
            c[2] > 0.0 && c[3] > 0.0 && crate::core::tree::point_in_aabb(mouse_pos, c)
        })
        && child_clip_round.is_none_or(|(rect, radii)| {
            crate::style::rounded_rect_contains(rect, radii, mouse_pos)
        });
    if !child_hovered {
        return;
    }
    if let Some(name) = child.attrs.get("jadix_peer") {
        if name == "1" {
            prev_peer_owned.push(None);
        } else if !name.is_empty() {
            prev_peer_owned.push(Some(name.clone()));
        }
    }
}

fn sync_class_variants_subtree_inner(
    el: &mut Element,
    mouse_pos: [f32; 2],
    focused_id: &str,
    dark: bool,
    viewport_w: f32,
    viewport_h: f32,
    ancestor_group_hovered: &[Option<&str>],
    peer_hovered: &[Option<&str>],
    scroll_acc: [f32; 2],
    clip: Option<[f32; 4]>,
    hit_nodes: &HashSet<*const Element>,
    layout_tree: &mut crate::layout::layout_tree::LayoutTree,
    impact: &mut LayoutStatus,
) {
    struct Frame {
        ptr: *mut Element,
        responsive_width: f32,
        scroll_acc: [f32; 2],
        clip: Option<[f32; 4]>,
        clip_round: OverflowRoundClip,
        ancestor_linear: crate::style::Mat2d,
        ancestor_scale: [f32; 2],
        ancestor_groups: Vec<Option<String>>,
        peer: Vec<Option<String>>,
        next_child: usize,
        started: bool,
        child_scroll_acc: [f32; 2],
        child_clip: Option<[f32; 4]>,
        child_clip_round: OverflowRoundClip,
        child_ancestor_linear: crate::style::Mat2d,
        child_ancestor_scale: [f32; 2],
        group_for_children: Vec<Option<String>>,
        prev_peer_owned: Vec<Option<String>>,
        named: Vec<(String, f32)>,
        scroll_progress: f32,
        child_named: Vec<(String, f32)>,
        child_scroll_progress: f32,
    }
    let mut stack = vec![Frame {
        ptr: el as *mut Element,
        responsive_width: viewport_w,
        scroll_acc,
        clip,
        clip_round: None,
        ancestor_linear: crate::style::MAT2D_IDENTITY,
        ancestor_scale: [1.0, 1.0],
        ancestor_groups: ancestor_group_hovered
            .iter()
            .map(|n| n.map(str::to_string))
            .collect(),
        peer: peer_hovered.iter().map(|n| n.map(str::to_string)).collect(),
        next_child: 0,
        started: false,
        child_scroll_acc: [0.0, 0.0],
        child_clip: None,
        child_clip_round: None,
        child_ancestor_linear: crate::style::MAT2D_IDENTITY,
        child_ancestor_scale: [1.0, 1.0],
        group_for_children: Vec::new(),
        prev_peer_owned: Vec::new(),
        named: Vec::new(),
        scroll_progress: 0.0,
        child_named: Vec::new(),
        child_scroll_progress: 0.0,
    }];
    let mut paint_hash_spines: Vec<taffy::prelude::NodeId> = Vec::new();
    while let Some(mut frame) = stack.pop() {
        let ptr = frame.ptr;
        if !frame.started {
            // SAFETY: exclusive subtree walk of the `el` passed to this function.
            let el = unsafe { &mut *ptr };
            if let Some(width) = el
                .attrs
                .get("data-jadix-viewport-width")
                .and_then(|value| value.parse::<f32>().ok())
                .filter(|width| width.is_finite() && *width > 0.0)
            {
                frame.responsive_width = width;
            }
            // Clone names so later `frame.*` writes do not fight borrows into the Frame.
            let ancestor_owned = frame.ancestor_groups.clone();
            let peer_owned = frame.peer.clone();
            let ancestor_group_hovered: Vec<Option<&str>> =
                ancestor_owned.iter().map(|n| n.as_deref()).collect();
            let peer_hovered: Vec<Option<&str>> = peer_owned.iter().map(|n| n.as_deref()).collect();
            // Reset accumulated scroll for fixed-position or portal-layer elements,
            // mirroring the behaviour in pick_hover_target / hit_path.
            let is_portal =
                el.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID);
            let (scroll_acc, clip, clip_round) = if el.style.position_fixed || is_portal {
                ([0.0_f32, 0.0], None, None)
            } else {
                (frame.scroll_acc, frame.clip, frame.clip_round)
            };

            let has_spin = el.attrs.get("jadix_animate").map(|v| v.as_str()) == Some("spin");
            // Only wake the per-frame resolve/tick path while a tween is actually running,
            // variants may match, or spin / CSS animation needs a tick.
            // Idle `transition-transform` tiles used to force resolve+taffy every frame on the
            // gallery layout page — enough work to jitter hit-test and flash sidebar hover.
            let has_css_anim = !el.style.animations.is_empty()
                || super::animation::css_animation_needs_frame(&el.style, &el.animation_snapshot);
            let has_active_css = el.transition_snapshot.has_active_tweens();
            let css_needs =
                super::animation::css_transition_needs_frame(&el.style, &el.transition_snapshot);
            // Resolve conditions before paint even for offscreen content. The scroll
            // path only advances active effects, so deferring dark/responsive/data
            // variants until reveal would paint the base snapshot for one frame.
            // Unchanged contexts still use the fingerprint fast path below.
            if css_needs
                || (crate::layout::collect_cache::node_paint_needs_frame(el)
                    && animation_frame_affects_viewport(
                        el,
                        scroll_acc,
                        frame.ancestor_linear,
                        frame.ancestor_scale,
                        viewport_w,
                        viewport_h,
                    ))
            {
                super::animation::note_page_css_needs_frame();
            }

            let b = el.layout_bounds;
            let [vx, vy] = crate::app::tree_hit::visual_xy(el, scroll_acc);
            let layout_box = [vx, vy, b[2], b[3]];
            let vis_box = crate::core::tree::visual_paint_aabb_with_ancestor(
                el,
                scroll_acc,
                frame.ancestor_linear,
                frame.ancestor_scale,
            );
            // Clip `:hover` to ancestor overflow / scroll viewports (layout border box).
            // The node's own hover uses the painted AABB so `hover:scale-*` stays hovered
            // while the pointer is on the scaled ink (not the unscaled Taffy box).
            // SVG path leaves further refine via `pointer-events` fill/stroke geometry.
            // Exclusive hit-path membership occludes covered siblings (portal popover).
            let inside = class_hover_pointer_active(
                el,
                mouse_pos,
                vis_box,
                clip,
                clip_round,
                scroll_acc,
                frame.ancestor_linear,
                frame.ancestor_scale,
                hit_nodes,
            );

            let child_clip = if is_portal {
                None
            } else if node_clips_pointer(el) {
                intersect_clip(clip, layout_box)
            } else {
                clip
            };
            let child_clip_round = next_overflow_round_clip(el, is_portal, layout_box, clip_round);

            let has_variants = el
                .class_set
                .as_ref()
                .is_some_and(|cs| !cs.variants.is_empty());
            if has_variants || has_spin || has_css_anim || has_active_css || css_needs {
                let hovered = inside;
                let prev_pointer_hover = el.class_set.as_ref().and_then(|cs| cs.last_pointer_hover);
                let hover_edge = has_variants && prev_pointer_hover.unwrap_or(false) != hovered;
                if has_variants {
                    if let Some(cs) = el.class_set.as_mut() {
                        cs.last_pointer_hover = Some(hovered);
                    }
                }
                let focused = el
                    .focus_id
                    .as_deref()
                    .is_some_and(|id| !id.is_empty() && id == focused_id);
                let disabled = el.attrs.get("disabled").is_some_and(|v| v != "false")
                    || el.attrs.get("aria-disabled").is_some_and(|v| v == "true");
                let ctx_fp = variant_ctx_fingerprint(
                    hovered,
                    focused,
                    disabled,
                    dark,
                    frame.responsive_width,
                    &ancestor_group_hovered,
                    &peer_hovered,
                    &el.attrs,
                );
                let ctx_unchanged = el
                    .class_set
                    .as_ref()
                    .and_then(|cs| cs.last_ctx_fp)
                    .is_some_and(|fp| fp == ctx_fp);
                // Idle transform/theme variants can skip. Hover *color* cannot: Tabs/Toggle
                // chrome restamps rest bg/fg every frame before this pass (Button does not —
                // it writes rest-or-hover itself). Skipping would paint rest after one flash.
                let hover_color_needs_redraw =
                    hovered && el.class_set.as_ref().is_some_and(ClassSet::has_hover_color);
                if !ctx_unchanged
                    || has_spin
                    || has_css_anim
                    || has_active_css
                    || css_needs
                    || hover_color_needs_redraw
                {
                    let attrs_snapshot = el.attrs.clone();
                    let ctx = VariantContext {
                        hovered,
                        focused,
                        disabled,
                        active: false,
                        dark,
                        viewport_w: frame.responsive_width,
                        group_hovered: &ancestor_group_hovered,
                        peer_hovered: &peer_hovered,
                        attrs: Some(&attrs_snapshot),
                    };
                    let before = el.style.clone();
                    let before_text = el.text_style.clone();
                    if !el.transition_snapshot.has_any_tweens() {
                        let rest = VariantContext {
                            hovered: false,
                            focused: false,
                            active: false,
                            group_hovered: &[],
                            peer_hovered: &[],
                            ..ctx.clone()
                        };
                        let initial: Vec<String> = el
                            .class_set
                            .as_ref()
                            .map(|classes| {
                                classes
                                    .variants
                                    .iter()
                                    .filter(|(selectors, _)| {
                                        selectors
                                            .iter()
                                            .all(|selector| rest.matches_selector(selector))
                                    })
                                    .map(|(_, utility)| utility.clone())
                                    .collect()
                            })
                            .unwrap_or_default();
                        let classes = el.class_set.take();
                        for utility in initial {
                            apply_known_utility_ex(el, &utility, false);
                        }
                        el.class_set = classes;
                        if !el.style.transition.is_empty() {
                            super::animation::snap_element_transitions_to_current(
                                &el.inner.style,
                                &mut el.inner.transition_snapshot,
                            );
                        }
                    }
                    let local = resolve_class_variants(el, &ctx);
                    let spun = tick_animate_spin(el);
                    let defer_shell_css = crate::utils::modal_motion::modal_shell_defer_css_tick(
                        el.template_id.as_deref(),
                    )
                        || crate::utils::floating_motion::floating_panel_defer_css_tick(
                            el.template_id.as_deref(),
                        );
                    let css_anim = if !defer_shell_css {
                        crate::style::ensure_used_layout_from_bounds(el);
                        let mut named = frame.named.clone();
                        super::scroll_timeline::push_named_timelines(
                            el, &mut named, scroll_acc, viewport_h,
                        );
                        let progress = super::animation::TimelineProgress {
                            scroll: scroller_timeline_progress(el).unwrap_or(frame.scroll_progress),
                            view: view_timeline_progress(el, scroll_acc, viewport_h),
                        };
                        let was_active = el.animation_snapshot.has_active(&el.style);
                        let active = super::animation::tick_element_animations_ex_named(
                            &mut el.inner.style,
                            &mut el.inner.text_style,
                            &el.inner.used_layout,
                            &mut el.inner.animation_snapshot,
                            frame_dt(),
                            progress,
                            &named,
                        );
                        el.inner.specified_present = el.inner.animation_snapshot.specified.clone();
                        active || was_active
                    } else {
                        false
                    };
                    let css_trans = if !defer_shell_css {
                        super::animation::tick_element_transitions_with_text(
                            &mut el.inner.style,
                            &mut el.inner.text_style,
                            &mut el.inner.transition_snapshot,
                            el.inner.animation_snapshot.lock(),
                            frame_dt(),
                        )
                    } else {
                        false
                    };
                    if let Some(cs) = el.class_set.as_mut() {
                        cs.last_ctx_fp = Some(ctx_fp);
                    }
                    if local != LayoutStatus::Clean
                        || spun
                        || css_anim
                        || css_trans
                        || el.style != before
                        || el.text_style != before_text
                        || hover_edge
                    {
                        *impact = impact.merge(local.merge(LayoutStatus::StyleChanged));
                        // Paint-only CSS ticks must not set hover-dirty. Gallery pointer
                        // storms (`hover:scale` tiles, sidebar) `take()` this flag on
                        // every CursorMoved — extra `dt≈0` paints freeze Marker spin.
                        if !(spun || css_anim || css_trans)
                            && (local != LayoutStatus::Clean
                                || el.style != before
                                || el.text_style != before_text
                                || hover_edge)
                        {
                            mark_hover_paint_dirty();
                        }
                        if spun
                            || css_anim
                            || css_trans
                            || hover_edge
                            || el.style != before
                            || el.text_style != before_text
                        {
                            crate::app::damage::push_frame_paint_element_change(
                                el,
                                scroll_acc,
                                before.opacity,
                            );
                        }
                        if css_trans
                            || el.style != before
                            || el.text_style != before_text
                            || hover_edge
                        {
                            // ViewportsOnly `reuse_idle` skips idle wrappers. First
                            // hover:scale / hover:underline / hover:text frame is not
                            // `collect_skip_live` yet — without a spine invalidate,
                            // skip-replays the rest Bake (lost enlarge, Link/Breadcrumb
                            // hover chrome). Zero that spine only; a global bust
                            // rehashes the gallery on every trigger switch.
                            queue_paint_hash_spine(
                                &mut paint_hash_spines,
                                layout_tree,
                                el,
                                el.collect_skip_live && !hover_edge,
                            );
                        }
                        // scale/rotate/translate/opacity are paint-only. Pushing them into Taffy every
                        // transition frame dirties flex geometry and can make sidebar hit-test flicker.
                        let need_taffy = matches!(
                            local,
                            LayoutStatus::LayoutChanged | LayoutStatus::StructureChanged
                        ) || style_needs_taffy_sync(&before, &el.style);
                        if need_taffy {
                            // Leave `hover:p-*` with local=Clean (restore base) still changes
                            // padding every tween frame. StyleChanged skips `compute_layout`.
                            *impact = impact.merge(LayoutStatus::LayoutChanged);
                            if el.layout_node.is_some() {
                                layout_tree.update_taffy_node_style(el);
                            }
                        }
                    }
                }
            }

            let mut pushed_group: Option<Option<String>> = None;
            if inside {
                if let Some(name) = el.attrs.get("jadix_group") {
                    if name == "1" {
                        pushed_group = Some(None);
                    } else if !name.is_empty() {
                        pushed_group = Some(Some(name.clone()));
                    }
                }
            }
            frame.group_for_children = if let Some(pushed) = pushed_group {
                let mut names: Vec<Option<String>> = ancestor_group_hovered
                    .iter()
                    .map(|n| n.map(|s| s.to_string()))
                    .collect();
                names.push(pushed);
                names
            } else {
                Vec::new()
            };

            let mut child_scroll_acc = scroll_acc;
            if !is_portal {
                child_scroll_acc = crate::app::tree_hit::next_hit_scroll_acc(el, scroll_acc);
            }
            frame.child_scroll_acc = child_scroll_acc;
            frame.child_clip = child_clip;
            frame.child_clip_round = child_clip_round;
            let (child_lin, child_sc) = crate::core::tree::child_paint_ancestor(
                el,
                frame.ancestor_linear,
                frame.ancestor_scale,
            );
            frame.child_ancestor_linear = child_lin;
            frame.child_ancestor_scale = child_sc;
            let mut child_named = frame.named.clone();
            super::scroll_timeline::push_named_timelines(
                el,
                &mut child_named,
                scroll_acc,
                viewport_h,
            );
            frame.child_named = child_named;
            frame.child_scroll_progress =
                scroller_timeline_progress(el).unwrap_or(frame.scroll_progress);
            frame.prev_peer_owned.clear();
            frame.next_child = 0;
            frame.started = true;
        }

        // SAFETY: exclusive subtree walk; LTR peer is recorded after the previous
        // sibling's frame has fully unwound (same order as the old recursive walk).
        let el = unsafe { &mut *ptr };
        let n = el.inner.children.len();
        if frame.next_child >= n {
            let after_style = el.after_style_sync.clone();
            let chrome_changed = after_style.is_some_and(|sync| {
                sync.run(el, &|element| {
                    hit_nodes.contains(&(element as *const Element))
                })
            });
            let flow_changed = super::liquid_flow::sync(el, hit_nodes);
            if chrome_changed || flow_changed {
                *impact = impact.merge(LayoutStatus::StyleChanged);
                queue_paint_hash_spine(&mut paint_hash_spines, layout_tree, el, false);
            }
            continue;
        }
        if frame.next_child > 0 {
            record_peer_hover(
                &mut frame.prev_peer_owned,
                &el.inner.children[frame.next_child - 1],
                mouse_pos,
                frame.child_scroll_acc,
                frame.child_clip,
                frame.child_clip_round,
                frame.child_ancestor_linear,
                frame.child_ancestor_scale,
                hit_nodes,
            );
        }
        let i = frame.next_child;
        frame.next_child += 1;
        let ancestor_groups = if frame.group_for_children.is_empty() {
            frame.ancestor_groups.clone()
        } else {
            frame.group_for_children.clone()
        };
        let peer = frame.prev_peer_owned.clone();
        let child_scroll_acc = frame.child_scroll_acc;
        let child_clip = frame.child_clip;
        let child_clip_round = frame.child_clip_round;
        let child_lin = frame.child_ancestor_linear;
        let child_sc = frame.child_ancestor_scale;
        let child_named = frame.child_named.clone();
        let child_scroll_progress = frame.child_scroll_progress;
        let child_ptr = std::ptr::addr_of_mut!(el.inner.children[i]);
        let responsive_width = frame.responsive_width;
        stack.push(frame);
        stack.push(Frame {
            ptr: child_ptr,
            responsive_width,
            scroll_acc: child_scroll_acc,
            clip: child_clip,
            clip_round: child_clip_round,
            ancestor_linear: child_lin,
            ancestor_scale: child_sc,
            ancestor_groups,
            peer,
            next_child: 0,
            started: false,
            child_scroll_acc: [0.0, 0.0],
            child_clip: None,
            child_clip_round: None,
            child_ancestor_linear: crate::style::MAT2D_IDENTITY,
            child_ancestor_scale: [1.0, 1.0],
            group_for_children: Vec::new(),
            prev_peer_owned: Vec::new(),
            named: child_named,
            scroll_progress: child_scroll_progress,
            child_named: Vec::new(),
            child_scroll_progress: 0.0,
        });
    }
    apply_paint_hash_spines(el, layout_tree, &paint_hash_spines);
}

/// Idle hover/CSS edges must rehash the dirty spine, not the whole gallery.
fn queue_paint_hash_spine(
    spines: &mut Vec<taffy::prelude::NodeId>,
    layout_tree: &mut crate::layout::layout_tree::LayoutTree,
    el: &Element,
    already_live: bool,
) {
    if already_live {
        return;
    }
    if let Some(id) = el.layout_node {
        spines.push(id);
    } else {
        layout_tree.note_collect_skip_hash_bust_if_idle(false);
    }
}

fn apply_paint_hash_spines(
    root: &mut Element,
    layout_tree: &mut crate::layout::layout_tree::LayoutTree,
    spines: &[taffy::prelude::NodeId],
) {
    if spines.is_empty() {
        return;
    }
    if !crate::layout::collect_cache::invalidate_collect_paint_hash_spines(root, spines) {
        layout_tree.note_collect_skip_hash_bust_if_idle(false);
    }
}

/// Advance `rotate_deg` when legacy `jadix_animate=spin` **and** no CSS animations list.
/// Prefer `Style.animations` (utility `animate-spin` now writes that).
fn tick_animate_spin(el: &mut Element) -> bool {
    if !el.style.animations.is_empty() {
        return false;
    }
    if el.attrs.get("jadix_animate").map(|v| v.as_str()) != Some("spin") {
        return false;
    }
    el.style.rotate_deg = (el.style.rotate_deg + 6.0) % 360.0;
    true
}

fn apply_or_update_named_animation(
    el: &mut Element,
    name: &str,
    duration_ms: f32,
    timing: &str,
    count: Option<f32>,
    reverse: bool,
) {
    let mut entry = crate::style::AnimationEntry {
        name: name.to_string(),
        duration_ms: duration_ms.max(1.0),
        timing: crate::style::TimingFunction::from_css(timing),
        ..Default::default()
    };
    entry.iteration_count = match count {
        Some(n) if n > 0.0 && n.is_finite() => crate::style::AnimationIterationCount::Finite(n),
        _ => crate::style::AnimationIterationCount::Infinite,
    };
    entry.direction = if reverse {
        crate::style::AnimationDirection::Reverse
    } else {
        crate::style::AnimationDirection::Normal
    };
    if let Some(existing) = el.style.animations.iter_mut().find(|e| e.name == name) {
        *existing = entry;
    } else {
        el.style.animations.push(entry);
    }
}

fn sync_shimmer_animation(el: &mut Element) {
    if !crate::renderer::ShimmerParams::effect_active(&el.attrs) {
        el.style.animations.retain(|e| e.name != "tw-shimmer");
        if el.attrs.get("jadix_animate").map(|s| s.as_str()) == Some("tw-shimmer") {
            el.attrs
                .insert("jadix_animate".to_string(), "none".to_string());
        }
        return;
    }
    let params = crate::renderer::ShimmerParams::from_shared_attrs(&el.attrs);
    apply_or_update_named_animation(
        el,
        "tw-shimmer",
        params.duration_ms,
        "linear",
        params.count,
        params.reverse,
    );
    el.attrs
        .insert("jadix_animate".to_string(), "tw-shimmer".to_string());
}

fn ensure_shimmer_epoch(el: &mut Element) {
    if !el.attrs.contains_key(crate::renderer::ATTR_SHIMMER_EPOCH) {
        el.attrs.insert(
            crate::renderer::ATTR_SHIMMER_EPOCH.to_string(),
            format!("{}", crate::style::global_time_s()),
        );
    }
}

fn enable_shimmer(el: &mut Element) {
    ensure_shimmer_epoch(el);
    el.attrs.insert(
        crate::renderer::ATTR_GLYPH_FX.to_string(),
        "shimmer".to_string(),
    );
    sync_shimmer_animation(el);
}

fn enable_shimmer_box(el: &mut Element) {
    ensure_shimmer_epoch(el);
    el.attrs.insert(
        crate::renderer::ATTR_SHIMMER_BOX.to_string(),
        "1".to_string(),
    );
    sync_shimmer_animation(el);
}

fn disable_shimmer(el: &mut Element) {
    el.attrs
        .insert(crate::renderer::ATTR_GLYPH_FX.to_string(), String::new());
    el.attrs.remove(crate::renderer::ATTR_SHIMMER_BOX);
    el.attrs.remove(crate::renderer::ATTR_SHIMMER_REVERSE);
    el.attrs.remove(crate::renderer::ATTR_SHIMMER_ONCE);
    el.attrs.remove(crate::renderer::ATTR_SHIMMER_COUNT);
    sync_shimmer_animation(el);
}

fn shimmer_modifiers_live(el: &Element) -> bool {
    crate::renderer::ShimmerParams::effect_active(&el.attrs)
}

fn apply_builtin_animation(
    el: &mut Element,
    name: &str,
    duration: &str,
    timing: &str,
    iteration: &str,
) {
    el.style.animations.clear();
    let mut entry = crate::style::AnimationEntry {
        name: name.to_string(),
        ..Default::default()
    };
    if let Some(ms) = crate::core::tree::parse_css_time_ms_strict(duration) {
        entry.duration_ms = ms;
    }
    entry.timing = crate::style::TimingFunction::from_css(timing);
    if let Some(c) = crate::style::AnimationIterationCount::from_css(iteration) {
        entry.iteration_count = c;
    }
    el.style.animations.push(entry);
}

fn apply_text_overflow_utility(el: &mut Element, mode: crate::style::TextOverflow) -> LayoutStatus {
    let before = (
        el.text_style.overflow,
        el.text_style.white_space,
        el.style.overflow.x,
        el.style.overflow.y,
    );
    el.apply_text_overflow_mode(mode);
    let after = (
        el.text_style.overflow,
        el.text_style.white_space,
        el.style.overflow.x,
        el.style.overflow.y,
    );
    if before == after {
        LayoutStatus::Clean
    } else {
        LayoutStatus::LayoutChanged
    }
}

fn apply_text_align_utility(el: &mut Element, kw: &str) -> LayoutStatus {
    use crate::style::{TextAlign, TextAlignDeclaration};
    let before = (
        el.text_style.align_resolved(),
        el.attrs.contains_key("text_align"),
    );
    let declaration = TextAlign::parse_declaration(kw);
    let impact = el.set_text_align(declaration);
    match declaration {
        TextAlignDeclaration::Inherit => {
            el.attrs.remove("text_align");
        }
        TextAlignDeclaration::Initial => {
            el.text_style.align = Some(TextAlign::Start);
            el.attrs
                .insert("text_align".to_string(), "start".to_string());
        }
        TextAlignDeclaration::Value(v) => {
            el.text_style.align = Some(v);
            el.attrs
                .insert("text_align".to_string(), v.as_str().to_string());
        }
        TextAlignDeclaration::Invalid => return LayoutStatus::Clean,
    }
    let after = (
        el.text_style.align_resolved(),
        el.attrs.contains_key("text_align"),
    );
    if before == after && impact == LayoutStatus::Clean {
        LayoutStatus::Clean
    } else {
        LayoutStatus::LayoutChanged
    }
}

fn ensure_transition_entry(el: &mut Element, prop: crate::style::TransitionProperty) {
    if !el.style.transition.iter().any(|e| e.property == prop) {
        el.style.transition.push(crate::style::TransitionEntry {
            property: prop,
            duration_ms: 150.0,
            delay_ms: 0.0,
            timing: crate::style::TimingFunction::Ease,
        });
    }
}

fn apply_known_utility(el: &mut Element, token: &str) -> LayoutStatus {
    apply_known_utility_ex(el, token, true)
}

fn tw_px(len: TwLength) -> f32 {
    len.to_px(crate::layout::dimension::css_root_font_size_px())
}

fn tw_dim(len: TwLength) -> crate::layout::dimension::CssDim {
    use crate::layout::dimension::CssDim;
    match len {
        TwLength::Px(v) => CssDim::Px(v),
        TwLength::Rem(v) => CssDim::Rem(v),
        TwLength::Em(v) => CssDim::Em(v),
    }
}

fn apply_known_utility_ex(el: &mut Element, token: &str, mark_overrides: bool) -> LayoutStatus {
    let Some(known) = classify_utility(token) else {
        return LayoutStatus::Clean;
    };
    match known {
        KnownUtility::DisplayNone => {
            el.style.display = taffy::style::Display::None;
            LayoutStatus::LayoutChanged
        }
        KnownUtility::DisplayFlex => {
            el.style.display = taffy::style::Display::Flex;
            LayoutStatus::LayoutChanged
        }
        KnownUtility::FlexRow => {
            *el = std::mem::take(el).flex_row();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::FlexCol => {
            *el = std::mem::take(el).flex_col();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::FlexWrap => {
            *el = std::mem::take(el).flex_wrap_kw("wrap");
            LayoutStatus::LayoutChanged
        }
        KnownUtility::FlexNoWrap => {
            *el = std::mem::take(el).flex_wrap_kw("nowrap");
            LayoutStatus::LayoutChanged
        }
        KnownUtility::DisplayGrid => {
            *el = std::mem::take(el).display_grid();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::ItemsCenter => {
            *el = std::mem::take(el).items_center();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::JustifyCenter => {
            *el = std::mem::take(el).justify_center();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::JustifyEnd => {
            *el = std::mem::take(el).justify_end();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Grow(v) => {
            *el = std::mem::take(el).flex_grow(v);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Shrink(v) => {
            *el = std::mem::take(el).flex_shrink(v);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Overflow(v) => {
            *el = std::mem::take(el).overflow(v);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::OverflowX(v) => {
            *el = std::mem::take(el).overflow_x(v);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::OverflowY(v) => {
            *el = std::mem::take(el).overflow_y(v);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::RoundedFull => {
            const FULL: [f32; 4] = [-1.0, -1.0, -1.0, -1.0];
            if el.style.radii == FULL {
                LayoutStatus::Clean
            } else {
                el.style.radii = FULL;
                // Radius is paint-only (Taffy does not consume it). Claiming
                // LayoutChanged on every bind_class re-apply hitch the page
                // while CSS spring Instant keeps running.
                LayoutStatus::StyleChanged
            }
        }
        KnownUtility::Rounded(len) => {
            let r = tw_px(len);
            if el
                .style
                .radii
                .iter()
                .all(|x| (*x - r).abs() <= f32::EPSILON)
            {
                LayoutStatus::Clean
            } else {
                el.style.radii = [r, r, r, r];
                LayoutStatus::StyleChanged
            }
        }
        KnownUtility::GridCols(n) => {
            *el = std::mem::take(el)
                .display_grid()
                .grid_template_columns(cols_equal_fr(n));
            LayoutStatus::LayoutChanged
        }
        KnownUtility::GridRows(n) => {
            *el = std::mem::take(el)
                .display_grid()
                .grid_template_rows(cols_equal_fr(n));
            LayoutStatus::LayoutChanged
        }
        KnownUtility::GapX(v) => {
            *el = std::mem::take(el).gap_x(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::GapY(v) => {
            *el = std::mem::take(el).gap_y(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Gap(v) => {
            *el = std::mem::take(el).gap(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Width(v) => {
            *el = std::mem::take(el).w(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Height(v) => {
            *el = std::mem::take(el).h(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MinWidth(v) => {
            *el = std::mem::take(el).min_w(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MaxWidth(v) => {
            *el = std::mem::take(el).max_w(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MinHeight(v) => {
            *el = std::mem::take(el).min_h(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MaxHeight(v) => {
            *el = std::mem::take(el).max_h(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Padding(v) => {
            *el = std::mem::take(el).p(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PaddingX(v) => {
            *el = std::mem::take(el).px(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PaddingY(v) => {
            *el = std::mem::take(el).py(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PaddingT(v) => {
            *el = std::mem::take(el).pt(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PaddingR(v) => {
            *el = std::mem::take(el).pr(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PaddingB(v) => {
            *el = std::mem::take(el).pb(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PaddingL(v) => {
            *el = std::mem::take(el).pl(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Margin(v) => {
            *el = std::mem::take(el).m(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MarginX(v) => {
            *el = std::mem::take(el).mx(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MarginY(v) => {
            *el = std::mem::take(el).my(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MarginT(v) => {
            *el = std::mem::take(el).mt(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MarginR(v) => {
            *el = std::mem::take(el).mr(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MarginB(v) => {
            *el = std::mem::take(el).mb(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MarginL(v) => {
            *el = std::mem::take(el).ml(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::BgColor(name) => {
            let color = parse_color(name);
            if mark_overrides {
                mark_variant_style_override(el, VariantStyleField::Bg);
            }
            if el.style.bg_color != color {
                el.style.bg_color = color;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::TextColor(name) => {
            let color = parse_text_color(name);
            if mark_overrides {
                mark_variant_style_override(el, VariantStyleField::Fg);
            }
            if el.text_style.color != Some(color) {
                el.text_style.color = Some(color);
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::TextSize(len) => {
            let px = tw_px(len);
            let mut changed = el.text_style.size != Some(px);
            if changed {
                el.text_style.size = Some(px);
            }
            if let Some(sync) = el.input_paint_sync.as_mut() {
                if (sync.text_size_px - px).abs() > f32::EPSILON {
                    sync.text_size_px = px;
                    changed = true;
                }
            }
            if changed {
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::FontWeight(w) => {
            let w = w.weight_u16();
            if el.text_style.weight != Some(w) {
                el.text_style.weight = Some(w);
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::FontFamilySans => {
            let stack = crate::utils::font::default_ui_sans_family_list().to_string();
            if el.text_style.font_family.as_deref() != Some(stack.as_str()) {
                el.text_style.font_family = Some(stack);
                el.attrs.insert("font_family".to_string(), "1".to_string());
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::FontFamilySerif => {
            let stack = crate::utils::font::default_ui_serif_family_list().to_string();
            if el.text_style.font_family.as_deref() != Some(stack.as_str()) {
                el.text_style.font_family = Some(stack);
                el.attrs.insert("font_family".to_string(), "1".to_string());
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::FontFamilyMono => {
            let stack = crate::utils::font::default_ui_mono_family_list().to_string();
            if el.text_style.font_family.as_deref() != Some(stack.as_str()) {
                el.text_style.font_family = Some(stack);
                el.attrs.insert("font_family".to_string(), "1".to_string());
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::BorderWidth(width) => {
            el.style.set_uniform_border_width(width);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::BoxShadow(value) => {
            let shadows = crate::style::parse_box_shadows(value.replace('_', " "));
            el.style.set_shadows_from_parsed(&shadows);
            LayoutStatus::StyleChanged
        }
        KnownUtility::LeadingPx(height) => {
            if mark_overrides {
                mark_variant_style_override(el, VariantStyleField::Leading);
            }
            if el.text_style.line_height_px == Some(height)
                && el.text_style.line_height_scale.is_none()
            {
                LayoutStatus::Clean
            } else {
                el.text_style.line_height_px = Some(height);
                el.text_style.line_height_scale = None;
                el.text_style.line_height_specified = true;
                LayoutStatus::LayoutChanged
            }
        }
        KnownUtility::Leading(scale) => {
            if mark_overrides {
                mark_variant_style_override(el, VariantStyleField::Leading);
            }
            let next = scale.max(0.01);
            if el.text_style.line_height_scale == Some(next)
                && el.text_style.line_height_px.is_none()
            {
                LayoutStatus::Clean
            } else {
                el.text_style.line_height_px = None;
                el.text_style.line_height_scale = Some(next);
                el.text_style.line_height_specified = true;
                LayoutStatus::LayoutChanged
            }
        }
        KnownUtility::AlignSelf(kw) => {
            let keyword = match kw {
                "start" => "flex-start",
                "end" => "flex-end",
                keyword => keyword,
            };
            *el = std::mem::take(el).align_self_kw(keyword);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Alignment(property, keyword) => {
            let keyword = if let Some(position) = keyword.strip_prefix("safe-") {
                format!("safe {position}")
            } else if let Some(position) = keyword.strip_prefix("unsafe-") {
                format!("unsafe {position}")
            } else {
                match keyword {
                    "start" => "flex-start".to_string(),
                    "end" => "flex-end".to_string(),
                    "first-baseline" => "first baseline".to_string(),
                    keyword => keyword.to_string(),
                }
            };
            *el = match property {
                "align-items" => std::mem::take(el).flex_items(keyword),
                "align-content" => std::mem::take(el).align_content_kw(keyword),
                "align-self" => std::mem::take(el).align_self_kw(keyword),
                "justify-content" => std::mem::take(el).justify(keyword),
                "justify-items" => std::mem::take(el).justify_items_kw(keyword),
                "justify-self" => std::mem::take(el).justify_self_kw(keyword),
                _ => return LayoutStatus::Clean,
            };
            LayoutStatus::LayoutChanged
        }
        KnownUtility::TextAlignLast(keyword) => el.set_text_align_last(keyword),
        KnownUtility::Opacity(v) => {
            if (el.style.opacity - v).abs() > f32::EPSILON {
                el.style.opacity = v;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::TranslateX(len) => {
            let px = tw_px(len);
            if (el.style.translate[0] - px).abs() > f32::EPSILON {
                el.style.translate[0] = px;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::TranslateXPct(p) => {
            if (el.style.translate_pct[0] - p).abs() > f32::EPSILON {
                el.style.translate_pct[0] = p;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::TranslateY(len) => {
            let px = tw_px(len);
            if (el.style.translate[1] - px).abs() > f32::EPSILON {
                el.style.translate[1] = px;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::TranslateYPct(p) => {
            if (el.style.translate_pct[1] - p).abs() > f32::EPSILON {
                el.style.translate_pct[1] = p;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::Truncate => {
            apply_text_overflow_utility(el, crate::style::TextOverflow::Ellipsis)
        }
        KnownUtility::TextOverflow(kw) => {
            apply_text_overflow_utility(el, crate::style::TextOverflow::parse(kw))
        }
        KnownUtility::TextAlign(kw) => apply_text_align_utility(el, kw),
        KnownUtility::WhitespaceNowrap => {
            use crate::style::WhiteSpace;
            if el.text_style.white_space != WhiteSpace::Nowrap {
                el.text_style.white_space = WhiteSpace::Nowrap;
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }

        KnownUtility::PositionFixed => {
            *el = std::mem::take(el).fixed();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PositionAbsolute => {
            *el = std::mem::take(el).absolute();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PositionRelative => {
            *el = std::mem::take(el).relative();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PositionSticky => {
            *el = std::mem::take(el).sticky();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PositionStatic => {
            *el = std::mem::take(el).static_position();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::InsetAll(v) => {
            *el = std::mem::take(el).inset(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::InsetX(v) => {
            *el = std::mem::take(el).left(v.as_str()).right(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::InsetY(v) => {
            *el = std::mem::take(el).top(v.as_str()).bottom(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::InsetTop(v) => {
            *el = std::mem::take(el).top(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::InsetRight(v) => {
            *el = std::mem::take(el).right(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::InsetBottom(v) => {
            *el = std::mem::take(el).bottom(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::InsetLeft(v) => {
            *el = std::mem::take(el).left(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PointerEventsAuto => {
            *el = std::mem::take(el).pointer_events("auto");
            LayoutStatus::Clean
        }
        KnownUtility::PointerEventsNone => {
            *el = std::mem::take(el).pointer_events_none();
            LayoutStatus::Clean
        }
        KnownUtility::PointerEventsBoxNone => {
            *el = std::mem::take(el).pointer_events_box_none();
            LayoutStatus::Clean
        }
        KnownUtility::Cursor(kw) => {
            *el = std::mem::take(el).cursor(kw);
            LayoutStatus::Clean
        }
        KnownUtility::AspectRatio(r) => {
            if el.style.aspect_ratio != Some(r) {
                el.style.aspect_ratio = Some(r);
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::Rotate(deg) => {
            if (el.style.rotate_deg - deg).abs() > f32::EPSILON {
                el.style.rotate_deg = deg;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::Scale(s) => {
            let new = [s, s];
            if (el.style.scale[0] - new[0]).abs() > f32::EPSILON
                || (el.style.scale[1] - new[1]).abs() > f32::EPSILON
            {
                el.style.scale = new;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::ScaleX(s) => {
            if (el.style.scale[0] - s).abs() > f32::EPSILON {
                el.style.scale[0] = s;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::ScaleY(s) => {
            if (el.style.scale[1] - s).abs() > f32::EPSILON {
                el.style.scale[1] = s;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::Zoom(z) => {
            if (el.style.zoom - z).abs() > f32::EPSILON {
                el.style.zoom = z;
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::SkewX(deg) => {
            use super::transform::TransformFunction;
            use crate::layout::dimension::CssAngle;
            let func = TransformFunction::SkewX(CssAngle::Deg(deg));
            let list = el.style.transform.get_or_insert_with(Vec::new);
            // Replace existing SkewX or append
            if let Some(pos) = list
                .iter()
                .position(|f| matches!(f, TransformFunction::SkewX(_)))
            {
                list[pos] = func;
            } else {
                list.push(func);
            }
            LayoutStatus::StyleChanged
        }
        KnownUtility::SkewY(deg) => {
            use super::transform::TransformFunction;
            use crate::layout::dimension::CssAngle;
            let func = TransformFunction::SkewY(CssAngle::Deg(deg));
            let list = el.style.transform.get_or_insert_with(Vec::new);
            if let Some(pos) = list
                .iter()
                .position(|f| matches!(f, TransformFunction::SkewY(_)))
            {
                list[pos] = func;
            } else {
                list.push(func);
            }
            LayoutStatus::StyleChanged
        }
        KnownUtility::TransformOrigin(kw) => {
            el.style.transform_origin = super::transform::parse_transform_origin(kw);
            LayoutStatus::StyleChanged
        }
        KnownUtility::BgGradientTo(angle) => {
            let g = el.style.bg_gradient.get_or_insert_with(Default::default);
            g.as_linear_mut().angle_deg = angle;
            LayoutStatus::StyleChanged
        }
        KnownUtility::GradientFrom(name) => {
            let color = parse_color(name);
            let g = el.style.bg_gradient.get_or_insert_with(Default::default);
            g.as_linear_mut().set_from(color);
            LayoutStatus::StyleChanged
        }
        KnownUtility::GradientVia(name) => {
            let color = parse_color(name);
            let g = el.style.bg_gradient.get_or_insert_with(Default::default);
            g.as_linear_mut().set_via(color);
            LayoutStatus::StyleChanged
        }
        KnownUtility::GradientTo(name) => {
            let color = parse_color(name);
            let g = el.style.bg_gradient.get_or_insert_with(Default::default);
            g.as_linear_mut().set_to(color);
            LayoutStatus::StyleChanged
        }
        KnownUtility::LetterSpacing(len) => {
            let px = tw_px(len);
            el.text_style.letter_spacing_specified = true;
            if (el.text_style.letter_spacing - px).abs() > f32::EPSILON {
                el.text_style.letter_spacing = px;
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::TextDirectionLtr => {
            use crate::style::TextDirection;
            *el = std::mem::take(el).text_direction(TextDirection::Ltr);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::TextDirectionRtl => {
            use crate::style::TextDirection;
            *el = std::mem::take(el).text_direction(TextDirection::Rtl);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::WritingMode(kw) => {
            *el = std::mem::take(el).writing_mode(kw);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::TransitionOpacity => {
            ensure_transition_entry(el, crate::style::TransitionProperty::Opacity);
            LayoutStatus::StyleChanged
        }
        KnownUtility::TransitionTransform => {
            ensure_transition_entry(el, crate::style::TransitionProperty::Transform);
            LayoutStatus::StyleChanged
        }
        KnownUtility::TransitionAll => {
            ensure_transition_entry(el, crate::style::TransitionProperty::All);
            LayoutStatus::StyleChanged
        }
        KnownUtility::TransitionColors => {
            ensure_transition_entry(el, crate::style::TransitionProperty::BackgroundColor);
            ensure_transition_entry(el, crate::style::TransitionProperty::BorderColor);
            ensure_transition_entry(el, crate::style::TransitionProperty::OutlineColor);
            ensure_transition_entry(el, crate::style::TransitionProperty::Color);
            LayoutStatus::StyleChanged
        }
        KnownUtility::TransitionPadding => {
            ensure_transition_entry(el, crate::style::TransitionProperty::Padding);
            LayoutStatus::StyleChanged
        }
        KnownUtility::TransitionNone => {
            el.style.transition.clear();
            LayoutStatus::StyleChanged
        }
        KnownUtility::TransitionDuration(ms) => {
            let ms_f = ms as f32;
            for entry in &mut el.style.transition {
                entry.duration_ms = ms_f;
            }
            LayoutStatus::StyleChanged
        }
        KnownUtility::TransitionDelay(ms) => {
            let ms_f = ms as f32;
            for entry in &mut el.style.transition {
                entry.delay_ms = ms_f;
            }
            LayoutStatus::StyleChanged
        }
        KnownUtility::TransitionEase(name) => {
            let tf = crate::style::TimingFunction::from_css(name);
            for entry in &mut el.style.transition {
                entry.timing = tf;
            }
            LayoutStatus::StyleChanged
        }
        KnownUtility::WillChange(hint) => {
            el.style.will_change = match hint {
                "transform" => crate::style::WillChangeHint::Transform,
                "opacity" => crate::style::WillChangeHint::Opacity,
                "scroll-position" => crate::style::WillChangeHint::ScrollPosition,
                "contents" => crate::style::WillChangeHint::Contents,
                _ => crate::style::WillChangeHint::Auto,
            };
            LayoutStatus::StyleChanged
        }
        KnownUtility::AnimateSpin => {
            apply_builtin_animation(el, "spin", "1s", "linear", "infinite");
            el.attrs
                .insert("jadix_animate".to_string(), "spin".to_string());
            LayoutStatus::StyleChanged
        }
        KnownUtility::AnimatePing => {
            apply_builtin_animation(el, "ping", "1s", "cubic-bezier(0,0,0.2,1)", "infinite");
            el.attrs
                .insert("jadix_animate".to_string(), "ping".to_string());
            LayoutStatus::StyleChanged
        }
        KnownUtility::AnimatePulse => {
            apply_builtin_animation(el, "pulse", "2s", "cubic-bezier(0.4,0,0.6,1)", "infinite");
            el.attrs
                .insert("jadix_animate".to_string(), "pulse".to_string());
            LayoutStatus::StyleChanged
        }
        KnownUtility::AnimateBounce => {
            apply_builtin_animation(el, "bounce", "1s", "ease", "infinite");
            el.attrs
                .insert("jadix_animate".to_string(), "bounce".to_string());
            LayoutStatus::StyleChanged
        }
        KnownUtility::AnimateNone => {
            el.style.animations.clear();
            el.attrs
                .insert("jadix_animate".to_string(), "none".to_string());
            LayoutStatus::StyleChanged
        }
        KnownUtility::GroupMarker { name } => {
            let value = name.unwrap_or("1");
            let prev = el
                .attrs
                .insert("jadix_group".to_string(), value.to_string());
            if prev.as_deref() == Some(value) {
                LayoutStatus::Clean
            } else {
                LayoutStatus::StyleChanged
            }
        }
        KnownUtility::PeerMarker { name } => {
            let value = name.unwrap_or("1");
            let prev = el.attrs.insert("jadix_peer".to_string(), value.to_string());
            if prev.as_deref() == Some(value) {
                LayoutStatus::Clean
            } else {
                LayoutStatus::StyleChanged
            }
        }
        KnownUtility::BoxBorder => {
            let prev = el.style.box_sizing;
            *el = std::mem::take(el).border_box();
            if prev == taffy::style::BoxSizing::BorderBox {
                LayoutStatus::Clean
            } else {
                LayoutStatus::LayoutChanged
            }
        }
        KnownUtility::BoxContent => {
            let prev = el.style.box_sizing;
            *el = std::mem::take(el).content_box();
            if prev == taffy::style::BoxSizing::ContentBox {
                LayoutStatus::Clean
            } else {
                LayoutStatus::LayoutChanged
            }
        }
        KnownUtility::OutlineStyle(kw) => {
            use crate::style::LineStyle;
            let s = LineStyle::from_css_keyword(kw);
            if el.style.outline_style == s {
                LayoutStatus::Clean
            } else {
                el.style.outline_style = s;
                // Tailwind `outline` implies currentColor; default transparent → foreground.
                if !s.is_invisible() && el.style.outline_color == [0.0, 0.0, 0.0, 0.0] {
                    el.style.outline_color = parse_color("foreground");
                }
                LayoutStatus::StyleChanged
            }
        }
        KnownUtility::OutlineWidth(px) => {
            if (el.style.outline_width - px).abs() < f32::EPSILON {
                LayoutStatus::Clean
            } else {
                el.style.outline_width = px;
                // Ensure visible color when width is set without explicit color.
                if px > 0.0 && el.style.outline_color == [0.0, 0.0, 0.0, 0.0] {
                    el.style.outline_color = parse_color("foreground");
                }
                LayoutStatus::StyleChanged
            }
        }
        KnownUtility::OutlineOffset(len) => {
            let px = tw_px(len);
            if (el.style.outline_offset - px).abs() < f32::EPSILON {
                LayoutStatus::Clean
            } else {
                el.style.outline_offset = px;
                LayoutStatus::StyleChanged
            }
        }
        KnownUtility::OutlineColor(c) => {
            let color = parse_color(c);
            if el.style.outline_color == color {
                LayoutStatus::Clean
            } else {
                el.style.outline_color = color;
                LayoutStatus::StyleChanged
            }
        }
        KnownUtility::RingWidth(px) => {
            let px = px.max(0.0);
            if el.style.focus_ring_width == Some(px) {
                LayoutStatus::Clean
            } else {
                el.style.focus_ring_width = Some(px);
                LayoutStatus::StyleChanged
            }
        }
        KnownUtility::RingColor(c) => {
            let color = parse_color(c);
            if el.style.focus_ring_color == Some(color) {
                LayoutStatus::Clean
            } else {
                el.style.focus_ring_color = Some(color);
                LayoutStatus::StyleChanged
            }
        }
        KnownUtility::Underline(on) => {
            if el.text_style.underline == Some(on) {
                LayoutStatus::Clean
            } else {
                el.text_style.underline = Some(on);
                LayoutStatus::StyleChanged
            }
        }
        KnownUtility::UnderlineOffset(len) => {
            let px = tw_px(len).max(0.0);
            // Initial class apply (`mark_overrides`): don't clobber an already-authored offset
            // (author `underline-offset-[2px]` / `.text_underline_offset()` before CVA).
            // Variant re-resolve must still apply `hover:underline-offset-*`.
            if (mark_overrides && el.text_style.underline_offset.is_some())
                || el.text_style.underline_offset == Some(px)
            {
                LayoutStatus::Clean
            } else {
                el.text_style.underline_offset = Some(px);
                LayoutStatus::StyleChanged
            }
        }
        KnownUtility::FilterNone => {
            if el.style.filter.is_none() {
                LayoutStatus::Clean
            } else {
                el.style.filter = None;
                LayoutStatus::StyleChanged
            }
        }
        KnownUtility::FilterBlur(len) => {
            set_single_filter(el, crate::style::FilterFunction::Blur(Some(tw_dim(len))))
        }
        KnownUtility::BackdropBlur(len) => {
            let px = tw_px(len).max(0.0);
            if (el.style.backdrop_blur - px).abs() < 1e-4 {
                LayoutStatus::Clean
            } else {
                el.style.backdrop_blur = px;
                LayoutStatus::StyleChanged
            }
        }
        KnownUtility::Glass(params) => {
            el.style.glass = (params[0] >= 1.0).then_some(crate::style::GlassMaterial {
                refraction_index: params[0],
                thickness: params[1],
                dispersion: params[2],
                highlight: params[3],
            });
            if el.style.glass.is_some() {
                el.style.backdrop_blur = el.style.backdrop_blur.max(0.6);
                el.set_attr("data-stacking-context", "1");
            }
            LayoutStatus::StyleChanged
        }
        KnownUtility::FilterBrightness(v) => {
            set_single_filter(el, crate::style::FilterFunction::Brightness(Some(v)))
        }
        KnownUtility::FilterContrast(v) => {
            set_single_filter(el, crate::style::FilterFunction::Contrast(Some(v)))
        }
        KnownUtility::FilterGrayscale(v) => {
            set_single_filter(el, crate::style::FilterFunction::Grayscale(Some(v)))
        }
        KnownUtility::FilterSepia(v) => {
            set_single_filter(el, crate::style::FilterFunction::Sepia(Some(v)))
        }
        KnownUtility::FilterInvert(v) => {
            set_single_filter(el, crate::style::FilterFunction::Invert(Some(v)))
        }
        KnownUtility::FilterSaturate(v) => {
            set_single_filter(el, crate::style::FilterFunction::Saturate(Some(v)))
        }
        KnownUtility::FilterHueRotate(deg) => set_single_filter(
            el,
            crate::style::FilterFunction::HueRotate(Some(crate::layout::dimension::CssAngle::Deg(
                deg,
            ))),
        ),
        KnownUtility::FilterDropShadow(spec) => {
            let css = drop_shadow_utility_css(spec);
            let next = crate::style::parse_filter(&css);
            if el.style.filter == next {
                LayoutStatus::Clean
            } else {
                el.style.filter = next;
                LayoutStatus::StyleChanged
            }
        }
        KnownUtility::Shimmer => {
            enable_shimmer(el);
            LayoutStatus::StyleChanged
        }
        KnownUtility::ShimmerBox => {
            enable_shimmer_box(el);
            LayoutStatus::StyleChanged
        }
        KnownUtility::ShimmerOnce => {
            el.attrs.insert(
                crate::renderer::ATTR_SHIMMER_ONCE.to_string(),
                "1".to_string(),
            );
            el.attrs.insert(
                crate::renderer::ATTR_SHIMMER_COUNT.to_string(),
                "1".to_string(),
            );
            if shimmer_modifiers_live(el) {
                sync_shimmer_animation(el);
            }
            LayoutStatus::StyleChanged
        }
        KnownUtility::ShimmerCount(n) => {
            match n {
                Some(times) => {
                    el.attrs.remove(crate::renderer::ATTR_SHIMMER_ONCE);
                    el.attrs.insert(
                        crate::renderer::ATTR_SHIMMER_COUNT.to_string(),
                        format!("{times}"),
                    );
                }
                None => {
                    el.attrs.remove(crate::renderer::ATTR_SHIMMER_ONCE);
                    el.attrs.insert(
                        crate::renderer::ATTR_SHIMMER_COUNT.to_string(),
                        "infinite".to_string(),
                    );
                }
            }
            if shimmer_modifiers_live(el) {
                sync_shimmer_animation(el);
            }
            LayoutStatus::StyleChanged
        }
        KnownUtility::ShimmerReverse => {
            el.attrs.insert(
                crate::renderer::ATTR_SHIMMER_REVERSE.to_string(),
                "1".to_string(),
            );
            if shimmer_modifiers_live(el) {
                sync_shimmer_animation(el);
            }
            LayoutStatus::StyleChanged
        }
        KnownUtility::ShimmerNone => {
            disable_shimmer(el);
            LayoutStatus::StyleChanged
        }
        KnownUtility::ShimmerDuration(ms) => {
            el.attrs.insert(
                crate::renderer::ATTR_SHIMMER_DURATION_MS.to_string(),
                ms.to_string(),
            );
            if shimmer_modifiers_live(el) {
                sync_shimmer_animation(el);
            }
            LayoutStatus::StyleChanged
        }
        KnownUtility::ShimmerSpread(px) => {
            el.attrs.insert(
                crate::renderer::ATTR_SHIMMER_SPREAD_PX.to_string(),
                format!("{px}"),
            );
            LayoutStatus::StyleChanged
        }
        KnownUtility::ShimmerAngle(deg) => {
            el.attrs.insert(
                crate::renderer::ATTR_SHIMMER_ANGLE_DEG.to_string(),
                format!("{deg}"),
            );
            LayoutStatus::StyleChanged
        }
        KnownUtility::ShimmerColor(name) => {
            let token = name
                .strip_prefix('[')
                .and_then(|s| s.strip_suffix(']'))
                .unwrap_or(name);
            let color = parse_text_color(token);
            el.attrs.insert(
                crate::renderer::ATTR_SHIMMER_COLOR.to_string(),
                crate::renderer::format_rgba8(color),
            );
            LayoutStatus::StyleChanged
        }
    }
}

fn set_single_filter(el: &mut Element, func: crate::style::FilterFunction) -> LayoutStatus {
    let next = Some(vec![func]);
    if el.style.filter == next {
        LayoutStatus::Clean
    } else {
        el.style.filter = next;
        LayoutStatus::StyleChanged
    }
}

/// Tailwind v3 `drop-shadow*` presets → CSS `filter` strings (hex alpha for parser support).
fn drop_shadow_utility_css(spec: &str) -> String {
    match spec {
        "" => {
            // default `drop-shadow`
            "drop-shadow(0 1px 2px #0000001a) drop-shadow(0 1px 1px #0000000f)".into()
        }
        "sm" => "drop-shadow(0 1px 1px #0000000d)".into(),
        "md" => "drop-shadow(0 4px 3px #00000012) drop-shadow(0 2px 2px #0000000f)".into(),
        "lg" => "drop-shadow(0 10px 8px #0000000a) drop-shadow(0 4px 3px #0000001a)".into(),
        "xl" => "drop-shadow(0 20px 13px #00000008) drop-shadow(0 8px 5px #00000014)".into(),
        "2xl" => "drop-shadow(0 25px 25px #00000026)".into(),
        "none" => "drop-shadow(0 0 transparent)".into(),
        // Arbitrary: `drop-shadow-[0_35px_35px_#000]` or already `drop-shadow(...)`.
        other => {
            let other = other.replace('_', " ");
            if other.starts_with("drop-shadow(") {
                other
            } else {
                format!("drop-shadow({other})")
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/utility.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/p1_utility.rs"]
mod p1_utility_tests;
