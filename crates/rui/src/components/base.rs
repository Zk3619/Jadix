//! # BaseComponentBuilder
//!
//! 组件根节点的共享样式 / 布局 fluent API。`ui!` 宏把标签上的布局属性
//!(`w` / `gap` / `cols` / `rounded`, etc.) are compiled into the `base` callback and then applied to the [`Element`] produced by the component.
//!
//! ## What it is for
//! - 给任意组件统一挂尺寸、间距、背景、边框、阴影、定位、grid/flex 辅助属性。
//! - 在自定义 `w`/`h` 时推算 `auto_text_size`（约边长 × 0.44，夹在 12–14px），供按钮等内嵌文字默认字号。
//!
//! ## How to use
//! 应用代码一般不必直接 `new`；在 RSX 上写属性即可：
//! ```ignore
//! <Button w="8rem" rounded="0.5rem" gap="0.5rem">"OK"</Button>
//! <GStack cols="3" gap="0.25rem">…</GStack>
//! ```
//! During programmatic assembly: `BaseComponentBuilder::new(el).p("1rem").bg(...)`.
//!
//! ## Notes
//! `text_size` / `font_weight` 会写回根节点；只改 `explicit_*` 而不回写会导致 RSX 样式丢失。
//!
//! ## 属性分层（HTML/CSS）
//! 真源：仓库根 [`docs/COMPONENT_LIBRARY.md`](../../../../docs/COMPONENT_LIBRARY.md) §1。
//! 通用样式写在元素上，不进各 `#[component]` 参数。三层：
//!
//! 1. **框架** — `Element` / `Style` / `TextStyle`（`w`、`color`、`leading`、`overflow`、`class`）。
//! 2. **Base / 根** — RSX 走宏 `is_base` + `chrome_binds`；Builder 走 [`SlotChrome`] /
//!    [`BaseComponentBuilder`]。`#[component]` 已注入 `base`。
//! 3. **组件** — 只声明领域 prop（`variant`、size-token、`checked`、`value`、slots）。
//!    `disabled` 走 [`crate::primitives::interactive::apply_disabled`]。
//!    领域测量与 CSS 短名撞车时改名（Chart `plot_h`、Table `col_w`、Sidebar `rail_w`），
//!    见规范 §1.1：领域名进 `own_prop`，CSS 短名留给根 chrome。
//!
//! 新槽位对照 [`crate::components::card::Card`]：签名只有 `children`（必要时 `id`）。
//! 根上的 `class` / `style` / `w` / `color` / `leading` 走 chrome。叶子消费继承值。

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::core::tree::{Element, IntoBorderWidths, IntoCssDim, IntoF32};
use crate::style::{
    is_variant_style_overridden, mark_variant_style_override, overflow_as_str, with_class_list,
    IntoAlignItems, IntoCursor, IntoFlexWrap, IntoJustifyContent, IntoOverflow, IntoPaintColor,
    IntoTextColor, VariantStyleField,
};

/// Same layout/paint surface as RSX `base` props (`w` / `h` / `text_size` / …).
/// `Rc` so data-driven chrome (Pagination links, Carousel slides) can clone it.
pub type SlotStyleFn = Rc<dyn Fn(BaseComponentBuilder) -> BaseComponentBuilder>;

/// Utility `class` + [`BaseComponentBuilder`] style, matching RSX `class` + layout attrs.
#[derive(Clone, Default)]
pub struct SlotChrome {
    pub class: String,
    pub style: Option<SlotStyleFn>,
}

impl SlotChrome {
    pub fn class(mut self, classes: impl Into<String>) -> Self {
        self.class = classes.into();
        self
    }

    pub fn style(
        mut self,
        f: impl Fn(BaseComponentBuilder) -> BaseComponentBuilder + 'static,
    ) -> Self {
        self.style = Some(Rc::new(f));
        self
    }

    /// Overlay `*ContentOpts` / RSX `class` + Builder `.style()`.
    pub fn from_opts(class: Option<String>, style: Option<SlotStyleFn>) -> Self {
        Self {
            class: class.unwrap_or_default(),
            style,
        }
    }

    /// Apply after the slot component builds (same order as RSX `class` chrome_binds + `base`).
    pub fn apply(&self, mut el: Element) -> Element {
        el = crate::style::apply_stylesheet_rules(el);
        if !self.class.trim().is_empty() {
            el = with_class_list(el, self.class.trim());
        }
        if let Some(ref style) = self.style {
            el = style(BaseComponentBuilder::new(el)).el;
        }
        el
    }

    /// Layer `over` on top of `self`: classes concatenate, styles run `self` then `over`.
    pub fn overlay(self, over: Self) -> Self {
        let class = [self.class.trim(), over.class.trim()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let style = match (self.style, over.style) {
            (Some(a), Some(b)) => Some(Rc::new(move |x| b(a(x))) as SlotStyleFn),
            (a, b) => b.or(a),
        };
        Self { class, style }
    }

    pub fn is_empty(&self) -> bool {
        self.class.trim().is_empty() && self.style.is_none()
    }

    /// If `class` is still empty, copy `fallback` (Select / Combobox ctx `content_class`).
    pub fn fallback_class(mut self, fallback: impl AsRef<str>) -> Self {
        if self.class.trim().is_empty() {
            self.class = fallback.as_ref().to_string();
        }
        self
    }
}

thread_local! {
    static PENDING_ITEM_CHROME: RefCell<SlotChrome> = const {
        RefCell::new(SlotChrome {
            class: String::new(),
            style: None,
        })
    };
}

/// Stash chrome for collect-then-paint items ([`crate::components::StepperItem`] /
/// [`crate::components::TimelineItem`]). The returned marker is discarded by
/// `*_scope`, so RSX `chrome_binds` never reach the painted row.
pub fn push_pending_item_chrome(chrome: SlotChrome) {
    PENDING_ITEM_CHROME.with(|c| *c.borrow_mut() = chrome);
}

pub fn take_pending_item_chrome() -> SlotChrome {
    PENDING_ITEM_CHROME.with(|c| std::mem::take(&mut *c.borrow_mut()))
}

/// Cover `data-slot` then match stylesheets. Call **after** AND hooks (`data-variant`, …)
/// and **before** caller `class`.
pub fn stamp_slot(mut el: Element, slot: &str) -> Element {
    el.set_attr("data-slot", slot);
    crate::style::apply_stylesheet_rules(el)
}

/// Stamp `data-slot` then apply chrome. Closed builders call this (or [`SlotMap::paint`])
/// instead of growing per-slot `*_with` methods.
pub fn paint_slot(mut el: Element, slot: &str, chrome: &SlotChrome) -> Element {
    el = stamp_slot(el, slot);
    chrome.apply(el)
}

/// User chrome keyed by `data-slot`. One map per closed Builder; components only stamp names.
#[derive(Clone, Default)]
pub struct SlotMap {
    entries: HashMap<String, SlotChrome>,
}

impl SlotMap {
    pub fn slot(
        mut self,
        name: impl Into<String>,
        f: impl FnOnce(SlotChrome) -> SlotChrome,
    ) -> Self {
        let name = name.into();
        let next = f(self.entries.remove(&name).unwrap_or_default());
        if !next.is_empty() {
            self.entries.insert(name, next);
        }
        self
    }

    pub fn get(&self, slot: &str) -> SlotChrome {
        self.entries.get(slot).cloned().unwrap_or_default()
    }

    /// Stamp `data-slot` and apply the matching chrome (no-op when unset).
    pub fn paint(&self, slot: &str, el: Element) -> Element {
        paint_slot(el, slot, &self.get(slot))
    }
}

/// Author `text-size` on a slot marker (root or any descendant).
pub fn author_text_size(el: &Element) -> Option<f32> {
    el.text_style
        .size
        .or_else(|| el.children.iter().find_map(author_text_size))
}

pub fn author_text_color(el: &Element) -> Option<[u8; 4]> {
    el.text_style
        .color
        .or_else(|| el.children.iter().find_map(author_text_color))
}

pub fn author_text_weight(el: &Element) -> Option<u16> {
    el.text_style
        .weight
        .or_else(|| el.children.iter().find_map(author_text_weight))
}

/// Write `size` onto `el` and every descendant (painted title/description trees).
pub fn overlay_author_text_size(el: &mut Element, size: f32) {
    el.text_style.size = Some(size);
    for child in &mut el.children {
        overlay_author_text_size(child, size);
    }
}

pub fn overlay_author_text_color(el: &mut Element, color: [u8; 4]) {
    el.text_style.color = Some(color);
    for child in &mut el.children {
        overlay_author_text_color(child, color);
    }
}

pub fn overlay_author_text_weight(el: &mut Element, weight: u16) {
    el.text_style.weight = Some(weight);
    for child in &mut el.children {
        overlay_author_text_weight(child, weight);
    }
}

/// If the root carries author marks (`text_size` / `text_color` / `font_weight`),
/// copy those values onto descendants so extract-and-rebuild leaves pick them up.
pub fn overlay_root_author_text(el: &mut Element) {
    if is_variant_style_overridden(&el.attrs, VariantStyleField::Size) {
        if let Some(sz) = el.text_style.size {
            overlay_author_text_size(el, sz);
        }
    }
    if is_variant_style_overridden(&el.attrs, VariantStyleField::Fg) {
        if let Some(c) = el.text_style.color {
            overlay_author_text_color(el, c);
        }
    }
    if is_variant_style_overridden(&el.attrs, VariantStyleField::Weight) {
        if let Some(w) = el.text_style.weight {
            overlay_author_text_weight(el, w);
        }
    }
}

/// Fill component-default type only when the author left the field unset.
pub fn fill_default_text_size(el: &mut Element, px: f32) {
    if el.text_style.size.is_none() {
        el.text_style.size = Some(px);
    }
}

pub fn fill_default_text_weight(el: &mut Element, weight: u16) {
    if el.text_style.weight.is_none() {
        el.text_style.weight = Some(weight);
    }
}

pub fn fill_default_text_color(el: &mut Element, color: [u8; 4]) {
    if is_variant_style_overridden(&el.attrs, VariantStyleField::Fg) {
        return;
    }
    if el.text_style.color.is_none() {
        el.text_style.color = Some(color);
    }
}

/// Utility class captured from a slot marker (`class` attr or `rui_utility_class`).
pub fn author_class(el: &Element) -> String {
    el.attrs
        .get("class")
        .cloned()
        .or_else(|| el.attrs.get("rui_utility_class").cloned())
        .unwrap_or_default()
}

fn author_padding_px(el: &Element) -> Option<[f32; 4]> {
    let l = crate::layout::lp_length_px(el.style.padding.left).unwrap_or(0.0);
    let r = crate::layout::lp_length_px(el.style.padding.right).unwrap_or(0.0);
    let t = crate::layout::lp_length_px(el.style.padding.top).unwrap_or(0.0);
    let b = crate::layout::lp_length_px(el.style.padding.bottom).unwrap_or(0.0);
    if l != 0.0
        || r != 0.0
        || t != 0.0
        || b != 0.0
        || is_variant_style_overridden(&el.attrs, VariantStyleField::Pad)
    {
        Some([l, t, r, b])
    } else {
        None
    }
}

/// Class + text + box chrome captured from a slot marker that will be discarded
/// (Stepper / Timeline extract-and-repaint).
#[derive(Clone, Default)]
pub struct SlotAuthor {
    pub class: String,
    pub size: Option<f32>,
    pub color: Option<[u8; 4]>,
    pub weight: Option<u16>,
    pub bg: Option<[f32; 4]>,
    pub padding: Option<[f32; 4]>,
    pub radii: Option<[f32; 4]>,
    pub border_width: Option<f32>,
    pub border_color: Option<[f32; 4]>,
}

impl SlotAuthor {
    pub fn from_el(el: &Element) -> Self {
        let bg = if is_variant_style_overridden(&el.attrs, VariantStyleField::Bg)
            || el.style.bg_color[3] > 0.0
        {
            Some(el.style.bg_color)
        } else {
            None
        };
        let radii = if el.style.radii.iter().any(|r| *r > 0.0)
            || is_variant_style_overridden(&el.attrs, VariantStyleField::Radius)
        {
            Some(el.style.radii)
        } else {
            None
        };
        let border_width = if el.style.border_width > 0.0
            || is_variant_style_overridden(&el.attrs, VariantStyleField::Border)
        {
            Some(el.style.border_width)
        } else {
            None
        };
        let border_color = if el.style.border_color[3] > 0.0
            || is_variant_style_overridden(&el.attrs, VariantStyleField::Border)
        {
            Some(el.style.border_color)
        } else {
            None
        };
        Self {
            class: author_class(el),
            size: author_text_size(el),
            color: author_text_color(el),
            weight: author_text_weight(el),
            bg,
            padding: author_padding_px(el),
            radii,
            border_width,
            border_color,
        }
    }

    pub fn apply(&self, mut el: Element) -> Element {
        if !self.class.trim().is_empty() {
            el = with_class_list(el, self.class.trim());
        }
        if let Some(sz) = self.size {
            overlay_author_text_size(&mut el, sz);
            mark_variant_style_override(&mut el, VariantStyleField::Size);
        }
        if let Some(c) = self.color {
            overlay_author_text_color(&mut el, c);
            mark_variant_style_override(&mut el, VariantStyleField::Fg);
        }
        if let Some(w) = self.weight {
            overlay_author_text_weight(&mut el, w);
            mark_variant_style_override(&mut el, VariantStyleField::Weight);
        }
        if let Some(bg) = self.bg {
            el.style.bg_color = bg;
            mark_variant_style_override(&mut el, VariantStyleField::Bg);
        }
        if let Some([l, t, r, b]) = self.padding {
            el.style.padding.left = taffy::style::LengthPercentage::length(l);
            el.style.padding.top = taffy::style::LengthPercentage::length(t);
            el.style.padding.right = taffy::style::LengthPercentage::length(r);
            el.style.padding.bottom = taffy::style::LengthPercentage::length(b);
            mark_variant_style_override(&mut el, VariantStyleField::Pad);
        }
        if let Some(radii) = self.radii {
            el.style.radii = radii;
            mark_variant_style_override(&mut el, VariantStyleField::Radius);
        }
        if let Some(w) = self.border_width {
            el.style.border_width = w;
            mark_variant_style_override(&mut el, VariantStyleField::Border);
        }
        if let Some(c) = self.border_color {
            el.style.border_color = c;
            mark_variant_style_override(&mut el, VariantStyleField::Border);
        }
        el
    }
}

impl std::fmt::Debug for SlotChrome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlotChrome")
            .field("class", &self.class)
            .field("has_style", &self.style.is_some())
            .finish()
    }
}

pub fn packed_children(children: Vec<Element>) -> Option<Element> {
    if children.is_empty() {
        None
    } else {
        let mut frag = Element::new();
        frag.set_attr("rui_fragment", "1");
        Some(frag.children(children))
    }
}

pub fn data_slot(el: &Element) -> Option<&str> {
    el.attrs.get("data-slot").map(String::as_str)
}

/// If `el` already carries `data-slot=expected`, keep it; otherwise wrap.
pub fn ensure_data_slot(
    el: Element,
    expected: &str,
    wrap: impl FnOnce(Element) -> Element,
) -> Element {
    if data_slot(&el) == Some(expected) {
        el
    } else {
        wrap(el)
    }
}

/// `.slot("data-slot-name", |c| c.class(..).style(..))` for closed Builders that store `slots: SlotMap`.
#[macro_export]
macro_rules! impl_slot_map_methods {
    () => {
        /// Style an internal node by its `data-slot` (framework slot chrome).
        pub fn slot(
            mut self,
            name: impl Into<String>,
            f: impl FnOnce($crate::components::base::SlotChrome) -> $crate::components::base::SlotChrome,
        ) -> Self {
            self.slots = self.slots.slot(name, f);
            self
        }
    };
}

/// `.content_class()` / `.content_style()` aliases that write [`SlotMap`] slot `$slot`.
#[macro_export]
macro_rules! impl_overlay_content_slot {
    ($slot:expr) => {
        /// Overlay `class` on the floating panel. Same as `.slot($slot, |c| c.class(..))`.
        pub fn content_class(self, classes: impl Into<String>) -> Self {
            self.slot($slot, |c| c.class(classes))
        }

        /// Overlay style on the floating panel. Same as `.slot($slot, |c| c.style(..))`.
        pub fn content_style(
            self,
            f: impl Fn(
                    $crate::components::base::BaseComponentBuilder,
                ) -> $crate::components::base::BaseComponentBuilder
                + 'static,
        ) -> Self {
            self.slot($slot, |c| c.style(f))
        }
    };
}

/// Chrome-only named-slot Builder (`*_with(|p| p.class(..).style(..))`).
/// Prefer [`SlotMap`] / `.slot()` on closed roots; keep this for real Tier 1 part builders.
#[macro_export]
macro_rules! impl_chrome_part_builder {
    ($builder:ident, $entry:ident) => {
        pub struct $builder {
            chrome: $crate::components::base::SlotChrome,
        }

        pub fn $entry() -> $builder {
            $builder {
                chrome: $crate::components::base::SlotChrome::default(),
            }
        }

        impl $builder {
            $crate::impl_slot_chrome_methods!();
        }
    };
}

/// `.class()` / `.style()` for builders that store chrome as `chrome: SlotChrome`.
#[macro_export]
macro_rules! impl_slot_chrome_methods {
    () => {
        pub fn class(mut self, classes: impl Into<String>) -> Self {
            self.chrome.class = classes.into();
            self
        }

        /// Same [`BaseComponentBuilder`] surface as RSX props (`w` / `h` / `text_size` / …).
        pub fn style(
            mut self,
            f: impl Fn(
                    $crate::components::base::BaseComponentBuilder,
                ) -> $crate::components::base::BaseComponentBuilder
                + 'static,
        ) -> Self {
            self.chrome.style = Some(::std::rc::Rc::new(f));
            self
        }
    };
}

/// `.class()` / `.style()` for opts that store chrome as `$field: SlotChrome`
/// (overlay `panel`, Progress track/circle `chrome`).
#[macro_export]
macro_rules! impl_content_opts_chrome {
    () => {
        $crate::impl_content_opts_chrome!(panel);
    };
    ($field:ident) => {
        pub fn class(mut self, classes: impl Into<String>) -> Self {
            self.$field.class = classes.into();
            self
        }

        /// Same [`BaseComponentBuilder`] surface as RSX layout attrs (`w` / `p` / `text_size` / …).
        pub fn style(
            mut self,
            f: impl Fn(
                    $crate::components::base::BaseComponentBuilder,
                ) -> $crate::components::base::BaseComponentBuilder
                + 'static,
        ) -> Self {
            self.$field.style = Some(::std::rc::Rc::new(f));
            self
        }
    };
}

/// Presentational slot Builder: children + [`SlotChrome`], `From` via `$comp($props { ..Default })`.
#[macro_export]
macro_rules! impl_simple_slot_builder {
    ($builder:ident, $entry:ident, $comp:ident, $props:ident) => {
        pub struct $builder {
            children: Vec<$crate::core::tree::Element>,
            chrome: $crate::components::base::SlotChrome,
        }

        pub fn $entry() -> $builder {
            $builder {
                children: Vec::new(),
                chrome: $crate::components::base::SlotChrome::default(),
            }
        }

        impl $builder {
            $crate::impl_slot_chrome_methods!();

            pub fn child(mut self, child: impl Into<$crate::core::tree::Element>) -> Self {
                self.children
                    .extend($crate::utils::take_packed(child.into()));
                self
            }

            pub fn text(mut self, content: impl Into<String>) -> Self {
                self.children
                    .push($crate::core::tree::Element::new().text(content.into()));
                self
            }
        }

        impl From<$builder> for $crate::core::tree::Element {
            fn from(builder: $builder) -> Self {
                let el = $comp($props {
                    children: $crate::components::base::packed_children(builder.children),
                    base: None,
                    ..Default::default()
                });
                let el = $crate::style::apply_stylesheet_rules(el);
                builder.chrome.apply(el)
            }
        }

        impl $crate::core::tree::IntoElementList for $builder {
            fn into_element_list(self) -> Vec<$crate::core::tree::Element> {
                vec![self.into()]
            }
        }
    };
}

pub struct BaseComponentBuilder {
    pub el: Element,
    pub(crate) label: Option<String>,
    pub(crate) has_custom_w: bool,
    pub(crate) has_custom_h: bool,
    pub(crate) has_custom_rounded: bool,
    pub(crate) custom_w_px: Option<f32>,
    pub(crate) custom_h_px: Option<f32>,
    pub(crate) explicit_text_size_px: Option<f32>,
    pub(crate) auto_text_size_px: Option<f32>,
    pub(crate) explicit_text_weight: Option<u16>,
}

impl BaseComponentBuilder {
    pub fn new(el: Element) -> Self {
        Self {
            el,
            label: None,
            has_custom_w: false,
            has_custom_h: false,
            has_custom_rounded: false,
            custom_w_px: None,
            custom_h_px: None,
            explicit_text_size_px: None,
            auto_text_size_px: None,
            explicit_text_weight: None,
        }
    }

    fn text_size_from_edge_px(edge_px: f32) -> f32 {
        (edge_px * 0.44).clamp(12.0, 14.0)
    }

    fn recompute_auto_text_size(&mut self) {
        let base = match (self.custom_w_px, self.custom_h_px) {
            (Some(w), Some(h)) => w.min(h),
            (Some(w), None) => w,
            (None, Some(h)) => h,
            (None, None) => return,
        };
        self.auto_text_size_px = Some(Self::text_size_from_edge_px(base));
    }

    pub fn w(mut self, w: impl IntoCssDim) -> Self {
        let dim = w.into_css_dim();
        if let Some(px) = dim
            .as_immediate_dimension()
            .and_then(crate::layout::dim_length_px)
        {
            self.custom_w_px = Some(px);
            self.recompute_auto_text_size();
        }
        self.has_custom_w = true;
        self.el = self.el.w(dim);
        self
    }

    pub fn h(mut self, h: impl IntoCssDim) -> Self {
        let dim = h.into_css_dim();
        if let Some(px) = dim
            .as_immediate_dimension()
            .and_then(crate::layout::dim_length_px)
        {
            self.custom_h_px = Some(px);
            self.recompute_auto_text_size();
        }
        self.has_custom_h = true;
        self.el = self.el.h(dim);
        self
    }

    pub fn min_w(mut self, w: impl IntoCssDim) -> Self {
        self.el = self.el.min_w(w);
        self
    }

    pub fn min_h(mut self, h: impl IntoCssDim) -> Self {
        self.el = self.el.min_h(h);
        self
    }

    /// CSS `aspect-ratio` (numeric / `"16/9"` / `"auto"`…). See [`crate::core::tree::parse_css_aspect_ratio`].
    pub fn aspect_ratio(mut self, ratio: impl crate::core::tree::IntoAspectRatioValue) -> Self {
        self.el = self.el.aspect_ratio(ratio);
        self
    }

    pub fn animation(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation(value);
        self
    }
    pub fn animation_name(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_name(value);
        self
    }
    pub fn animation_duration(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_duration(value);
        self
    }
    pub fn animation_delay(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_delay(value);
        self
    }
    pub fn animation_timing_function(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_timing_function(value);
        self
    }
    pub fn animation_iteration_count(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_iteration_count(value);
        self
    }
    pub fn animation_direction(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_direction(value);
        self
    }
    pub fn animation_fill_mode(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_fill_mode(value);
        self
    }
    pub fn animation_play_state(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_play_state(value);
        self
    }
    pub fn animation_composition(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_composition(value);
        self
    }
    pub fn animation_timeline(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_timeline(value);
        self
    }
    pub fn animation_range(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_range(value);
        self
    }
    pub fn animation_range_start(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_range_start(value);
        self
    }
    pub fn animation_range_end(mut self, value: impl AsRef<str>) -> Self {
        self.el = self.el.animation_range_end(value);
        self
    }

    pub fn p(mut self, p: impl IntoCssDim) -> Self {
        self.el = self.el.p(p);
        mark_variant_style_override(&mut self.el, VariantStyleField::Pad);
        self
    }

    pub fn rounded(mut self, radius: impl IntoF32) -> Self {
        self.has_custom_rounded = true;
        self.el = self.el.rounded(radius);
        mark_variant_style_override(&mut self.el, VariantStyleField::Radius);
        self
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.label = Some(text.into());
        self
    }

    pub fn bg(mut self, bg: impl IntoPaintColor) -> Self {
        self.el = self.el.bg(bg);
        mark_variant_style_override(&mut self.el, VariantStyleField::Bg);
        self
    }

    pub fn opacity(mut self, o: impl crate::core::tree::IntoOpacity) -> Self {
        self.el = self.el.opacity(o);
        self
    }

    pub fn box_shadow(mut self, spec: impl AsRef<str>) -> Self {
        self.el = self.el.box_shadow(spec);
        mark_variant_style_override(&mut self.el, VariantStyleField::Shadow);
        self
    }

    pub fn popup_shadow(mut self) -> Self {
        self.el = self.el.popup_shadow();
        mark_variant_style_override(&mut self.el, VariantStyleField::Shadow);
        self
    }

    pub fn filter(mut self, spec: impl AsRef<str>) -> Self {
        self.el = self.el.filter(spec);
        self
    }

    pub fn text_shadow(mut self, spec: impl AsRef<str>) -> Self {
        self.el = self.el.text_shadow(spec);
        self
    }

    pub fn box_shadow_color(mut self, color: [f32; 4]) -> Self {
        self.el = self.el.box_shadow_color(color);
        mark_variant_style_override(&mut self.el, VariantStyleField::Shadow);
        self
    }
    pub fn box_shadow_blur(mut self, blur: f32) -> Self {
        self.el = self.el.box_shadow_blur(blur);
        mark_variant_style_override(&mut self.el, VariantStyleField::Shadow);
        self
    }
    pub fn box_shadow_offset_x(mut self, dx: f32) -> Self {
        self.el = self.el.box_shadow_offset_x(dx);
        mark_variant_style_override(&mut self.el, VariantStyleField::Shadow);
        self
    }
    pub fn box_shadow_offset_y(mut self, dy: f32) -> Self {
        self.el = self.el.box_shadow_offset_y(dy);
        mark_variant_style_override(&mut self.el, VariantStyleField::Shadow);
        self
    }
    pub fn box_shadow_spread(mut self, spread: f32) -> Self {
        self.el = self.el.box_shadow_spread(spread);
        mark_variant_style_override(&mut self.el, VariantStyleField::Shadow);
        self
    }
    pub fn box_shadow_inset(mut self, inset: bool) -> Self {
        self.el = self.el.box_shadow_inset(inset);
        mark_variant_style_override(&mut self.el, VariantStyleField::Shadow);
        self
    }

    pub fn text_shadow_color(mut self, color: [u8; 4]) -> Self {
        self.el = self.el.text_shadow_color(color);
        self
    }
    pub fn text_shadow_blur(mut self, blur: impl IntoF32) -> Self {
        self.el = self.el.text_shadow_blur(blur);
        self
    }
    pub fn text_shadow_offset_x(mut self, dx: impl IntoF32) -> Self {
        self.el = self.el.text_shadow_offset_x(dx);
        self
    }
    pub fn text_shadow_offset_y(mut self, dy: impl IntoF32) -> Self {
        self.el = self.el.text_shadow_offset_y(dy);
        self
    }

    pub fn border_style(mut self, style: impl Into<crate::style::LineStyle>) -> Self {
        self.el = self.el.border_style(style);
        self
    }
    pub fn border_color(mut self, color: impl IntoPaintColor) -> Self {
        self.el = self.el.border_color(color);
        mark_variant_style_override(&mut self.el, VariantStyleField::Border);
        self
    }
    pub fn border_width(mut self, width: impl IntoBorderWidths) -> Self {
        self.el = self.el.border_width(width);
        self
    }
    pub fn border_top_width(mut self, width: impl IntoF32) -> Self {
        self.el = self.el.border_top_width(width);
        self
    }
    pub fn border_right_width(mut self, width: impl IntoF32) -> Self {
        self.el = self.el.border_right_width(width);
        self
    }
    pub fn border_bottom_width(mut self, width: impl IntoF32) -> Self {
        self.el = self.el.border_bottom_width(width);
        self
    }
    pub fn border_left_width(mut self, width: impl IntoF32) -> Self {
        self.el = self.el.border_left_width(width);
        self
    }
    pub fn border_top_color(mut self, color: [f32; 4]) -> Self {
        self.el = self.el.border_top_color(color);
        mark_variant_style_override(&mut self.el, VariantStyleField::Border);
        self
    }
    pub fn border_right_color(mut self, color: [f32; 4]) -> Self {
        self.el = self.el.border_right_color(color);
        mark_variant_style_override(&mut self.el, VariantStyleField::Border);
        self
    }
    pub fn border_bottom_color(mut self, color: [f32; 4]) -> Self {
        self.el = self.el.border_bottom_color(color);
        mark_variant_style_override(&mut self.el, VariantStyleField::Border);
        self
    }
    pub fn border_left_color(mut self, color: [f32; 4]) -> Self {
        self.el = self.el.border_left_color(color);
        mark_variant_style_override(&mut self.el, VariantStyleField::Border);
        self
    }

    pub fn outline_style(mut self, style: impl AsRef<str>) -> Self {
        self.el = self.el.outline_style(style);
        self
    }
    pub fn outline_color(mut self, color: [f32; 4]) -> Self {
        self.el = self.el.outline_color(color);
        self
    }
    pub fn outline_width(mut self, width: f32) -> Self {
        self.el = self.el.outline_width(width);
        self
    }
    /// Focus-ring thickness in logical px. Overrides the theme default.
    pub fn ring_width(mut self, width: impl crate::core::tree::IntoF32) -> Self {
        self.el = self.el.ring_width(width);
        self
    }
    /// Focus-ring color. Tokens / hex / `[f32; 4]`. Overrides theme token `ring`.
    pub fn ring_color(mut self, color: impl IntoPaintColor) -> Self {
        self.el = self.el.ring_color(color);
        self
    }
    /// CSS `caret-color`. Forwards into Input/Textarea paint sync.
    pub fn caret_color(mut self, color: impl IntoTextColor) -> Self {
        self.el = self.el.caret_color(color);
        self
    }
    pub fn selection_background(mut self, color: impl IntoTextColor) -> Self {
        self.el = self.el.selection_background(color);
        self
    }
    pub fn selection_foreground(mut self, color: impl IntoTextColor) -> Self {
        self.el = self.el.selection_foreground(color);
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.el = self.el.child(child);
        self
    }
    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        for c in children {
            self = self.child(c);
        }
        self
    }

    pub fn text_size(mut self, size: impl IntoF32) -> Self {
        let px = size.into_f32();
        self.explicit_text_size_px = Some(px);
        // Same as `font_weight`: RSX `text_size` must be written back to the root node when taking `base`, otherwise only `explicit_*` has no consumers.
        self.el = self.el.text_size(px);
        mark_variant_style_override(&mut self.el, VariantStyleField::Size);
        self
    }

    pub fn font_weight(mut self, weight: impl crate::core::tree::IntoF32Raw) -> Self {
        let w = weight.into_f32_raw().round().clamp(100.0, 900.0) as u16;
        self.explicit_text_weight = Some(w);
        // RSX 将 `font_weight` 作用在 `base` 回调上；必须写回根节点的 `text_style`，否则
        // `resolved_text_weight` will lose the font weight when it is never consumed (e.g. `<Label font_weight=600>`).
        self.el = self.el.font_weight(w as f32);
        mark_variant_style_override(&mut self.el, VariantStyleField::Weight);
        self
    }

    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        self.el = self.el.font_family(family);
        self
    }

    pub fn resolved_text_size(&self, default_px: f32) -> f32 {
        self.explicit_text_size_px
            .or(self.auto_text_size_px)
            .unwrap_or(default_px)
    }

    pub fn resolved_text_weight(&self, default_weight: u16) -> u16 {
        self.explicit_text_weight.unwrap_or(default_weight)
    }
    pub fn flex_grow(mut self, grow: impl crate::core::tree::IntoF32Raw) -> Self {
        self.el = self.el.flex_grow(grow);
        self
    }
    pub fn flex_shrink(mut self, shrink: impl crate::core::tree::IntoF32Raw) -> Self {
        self.el = self.el.flex_shrink(shrink);
        self
    }

    pub fn top(mut self, val: impl IntoCssDim) -> Self {
        self.el = self.el.top(val);
        self
    }
    pub fn right(mut self, val: impl IntoCssDim) -> Self {
        self.el = self.el.right(val);
        self
    }
    pub fn relative(mut self, r: bool) -> Self {
        if r {
            self.el = self.el.relative();
        }
        self
    }
    pub fn absolute(mut self, a: bool) -> Self {
        if a {
            self.el = self.el.absolute();
        }
        self
    }
    pub fn pointer_events(mut self, mode: impl Into<crate::core::tree::PointerEvents>) -> Self {
        self.el = self.el.pointer_events(mode);
        self
    }
    pub fn cursor(mut self, value: impl IntoCursor) -> Self {
        self.el = self.el.cursor(value);
        self
    }
    pub fn bottom(mut self, val: impl IntoCssDim) -> Self {
        self.el = self.el.bottom(val);
        self
    }
    pub fn left(mut self, val: impl IntoCssDim) -> Self {
        self.el = self.el.left(val);
        self
    }
    pub fn inset(mut self, val: impl IntoCssDim) -> Self {
        self.el = self.el.inset(val);
        self
    }
    pub fn z_index(mut self, z: i32) -> Self {
        self.el = self.el.z_index(z);
        self
    }

    // Logical spacing
    pub fn mx(mut self, val: impl IntoCssDim) -> Self {
        self.el = self.el.mx(val);
        self
    }
    pub fn my(mut self, val: impl IntoCssDim) -> Self {
        self.el = self.el.my(val);
        self
    }
    pub fn px(mut self, val: impl IntoCssDim) -> Self {
        self.el = self.el.px(val);
        mark_variant_style_override(&mut self.el, VariantStyleField::Pad);
        self
    }
    pub fn py(mut self, val: impl IntoCssDim) -> Self {
        self.el = self.el.py(val);
        mark_variant_style_override(&mut self.el, VariantStyleField::Pad);
        self
    }
    pub fn pt(mut self, val: impl IntoCssDim) -> Self {
        self.el = self.el.pt(val);
        mark_variant_style_override(&mut self.el, VariantStyleField::Pad);
        self
    }
    pub fn pr(mut self, val: impl IntoCssDim) -> Self {
        self.el = self.el.pr(val);
        mark_variant_style_override(&mut self.el, VariantStyleField::Pad);
        self
    }
    pub fn pb(mut self, val: impl IntoCssDim) -> Self {
        self.el = self.el.pb(val);
        mark_variant_style_override(&mut self.el, VariantStyleField::Pad);
        self
    }
    pub fn pl(mut self, val: impl IntoCssDim) -> Self {
        self.el = self.el.pl(val);
        mark_variant_style_override(&mut self.el, VariantStyleField::Pad);
        self
    }
    pub fn mt(mut self, val: impl IntoCssDim) -> Self {
        self.el = self.el.mt(val);
        self
    }
    pub fn mr(mut self, val: impl IntoCssDim) -> Self {
        self.el = self.el.mr(val);
        self
    }
    pub fn mb(mut self, val: impl IntoCssDim) -> Self {
        self.el = self.el.mb(val);
        self
    }
    pub fn ml(mut self, val: impl IntoCssDim) -> Self {
        self.el = self.el.ml(val);
        self
    }

    // Text enhancements
    pub fn leading(mut self, scale: impl crate::core::tree::IntoF32Raw) -> Self {
        self.el = self.el.text_leading(scale);
        mark_variant_style_override(&mut self.el, VariantStyleField::Leading);
        self
    }
    pub fn leading_none(mut self) -> Self {
        self.el = self.el.text_leading_none();
        mark_variant_style_override(&mut self.el, VariantStyleField::Leading);
        self
    }
    pub fn leading_tight(mut self) -> Self {
        self.el = self.el.text_leading_tight();
        mark_variant_style_override(&mut self.el, VariantStyleField::Leading);
        self
    }
    pub fn leading_snug(mut self) -> Self {
        self.el = self.el.text_leading_snug();
        mark_variant_style_override(&mut self.el, VariantStyleField::Leading);
        self
    }
    pub fn leading_normal(mut self) -> Self {
        self.el = self.el.text_leading_normal();
        mark_variant_style_override(&mut self.el, VariantStyleField::Leading);
        self
    }
    pub fn leading_relaxed(mut self) -> Self {
        self.el = self.el.text_leading_relaxed();
        mark_variant_style_override(&mut self.el, VariantStyleField::Leading);
        self
    }
    pub fn leading_loose(mut self) -> Self {
        self.el = self.el.text_leading_loose();
        mark_variant_style_override(&mut self.el, VariantStyleField::Leading);
        self
    }
    pub fn line_height(mut self, px: impl IntoF32) -> Self {
        self.el = self.el.text_line_height(px);
        self
    }
    pub fn text_color(mut self, color: impl IntoTextColor) -> Self {
        self.el = self.el.text_color(color);
        mark_variant_style_override(&mut self.el, VariantStyleField::Fg);
        self
    }
    pub fn underline(mut self, u: bool) -> Self {
        self.el = self.el.underline(u);
        self
    }
    pub fn text_decoration(mut self, d: impl Into<String>) -> Self {
        self.el = self.el.text_decoration(d.into());
        self
    }
    pub fn text_decoration_line(mut self, line: impl Into<String>) -> Self {
        self.el = self.el.text_decoration_line(line.into());
        self
    }
    pub fn text_decoration_style(mut self, s: impl Into<String>) -> Self {
        self.el = self.el.text_decoration_style(s.into());
        self
    }
    pub fn text_decoration_color(mut self, color: [u8; 4]) -> Self {
        self.el = self.el.text_decoration_color(color);
        self
    }
    pub fn text_decoration_thickness(mut self, t: impl IntoF32) -> Self {
        self.el = self.el.text_decoration_thickness(t);
        self
    }
    pub fn text_underline_offset(mut self, o: impl IntoF32) -> Self {
        self.el = self.el.text_underline_offset(o);
        self
    }
    pub fn text_align(mut self, align: impl Into<crate::style::TextAlignDeclaration>) -> Self {
        self.el = self.el.text_align(align);
        self
    }
    pub fn text_center(self) -> Self {
        self.text_align("center")
    }

    // Layout
    pub fn items(mut self, align: impl IntoAlignItems) -> Self {
        self.el = self.el.flex_items(align);
        self
    }
    pub fn wrap(mut self, w: impl IntoFlexWrap) -> Self {
        self.el = self.el.flex_wrap_kw(w);
        self
    }

    pub fn justify(mut self, keyword: impl IntoJustifyContent) -> Self {
        self.el = self.el.justify(keyword);
        self
    }
    pub fn gap(mut self, g: impl IntoCssDim) -> Self {
        self.el = self.el.gap(g);
        self
    }
    pub fn gap_x(mut self, g: impl IntoCssDim) -> Self {
        self.el = self.el.gap_x(g);
        self
    }
    pub fn gap_y(mut self, g: impl IntoCssDim) -> Self {
        self.el = self.el.gap_y(g);
        self
    }
    pub fn column_gap(mut self, g: impl IntoCssDim) -> Self {
        self.el = self.el.column_gap(g);
        self
    }
    pub fn row_gap(mut self, g: impl IntoCssDim) -> Self {
        self.el = self.el.row_gap(g);
        self
    }

    pub fn display_grid(mut self) -> Self {
        self.el = self.el.display_grid();
        self
    }

    pub fn grid_cols_equal(mut self, n: u16) -> Self {
        self.el = self.el.grid_cols_equal(n);
        self
    }

    pub fn grid_rows_equal(mut self, n: u16) -> Self {
        self.el = self.el.grid_rows_equal(n);
        self
    }

    pub fn flex_basis(mut self, basis: impl IntoCssDim) -> Self {
        self.el = self.el.flex_basis(basis);
        self
    }

    pub fn align_self_kw(mut self, kw: impl Into<String>) -> Self {
        let s = kw.into();
        self.el = self.el.align_self_kw(&s);
        self
    }

    pub fn grid_column_span(mut self, n: u16) -> Self {
        self.el = self.el.grid_column_span(n);
        self
    }

    pub fn grid_row_span(mut self, n: u16) -> Self {
        self.el = self.el.grid_row_span(n);
        self
    }

    pub fn justify_center(mut self) -> Self {
        self.el = self.el.justify_center();
        self
    }

    pub fn items_center(mut self) -> Self {
        self.el = self.el.items_center();
        self
    }

    pub fn scroll_behavior(mut self, b: impl Into<String>) -> Self {
        self.el.set_attr("scroll_behavior", b.into());
        self
    }
    pub fn scrollbar_inset(mut self, i: impl Into<String>) -> Self {
        self.el.set_attr("scrollbar_inset", i.into());
        self
    }
    pub fn scrollbar_inset_x(mut self, i: impl Into<String>) -> Self {
        self.el.set_attr("scrollbar_inset_x", i.into());
        self
    }
    pub fn scrollbar_inset_y(mut self, i: impl Into<String>) -> Self {
        self.el.set_attr("scrollbar_inset_y", i.into());
        self
    }
    pub fn scrollbar_visibility(mut self, v: impl Into<String>) -> Self {
        self.el.set_attr("scrollbar_visibility", v.into());
        self
    }
    pub fn scrollbar_y_side(mut self, s: impl Into<String>) -> Self {
        self.el.set_attr("scrollbar_y_side", s.into());
        self
    }
    pub fn scrollbar_x_side(mut self, s: impl Into<String>) -> Self {
        self.el.set_attr("scrollbar_x_side", s.into());
        self
    }
    pub fn overflow_x(mut self, o: impl IntoOverflow) -> Self {
        let ov = o.into_overflow();
        self.el = self.el.overflow_x(ov);
        self.el.set_attr("overflow_x", overflow_as_str(ov));
        self
    }
    pub fn overflow_y(mut self, o: impl IntoOverflow) -> Self {
        let ov = o.into_overflow();
        self.el = self.el.overflow_y(ov);
        self.el.set_attr("overflow_y", overflow_as_str(ov));
        self
    }
    pub fn overflow(mut self, o: impl IntoOverflow) -> Self {
        let ov = o.into_overflow();
        self.el = self.el.overflow(ov);
        let s = overflow_as_str(ov);
        self.el.set_attr("overflow_x", s);
        self.el.set_attr("overflow_y", s);
        self
    }
    pub fn scrollbar_variant(mut self, v: impl Into<String>) -> Self {
        self.el.set_attr("scrollbar_variant", v.into());
        self
    }

    /// Finish layout chrome, then match stylesheet rules (before caller takes `.el` as RSX class).
    pub fn finish(self) -> Element {
        crate::style::apply_stylesheet_rules(self.el)
    }
}

impl From<BaseComponentBuilder> for Element {
    fn from(builder: BaseComponentBuilder) -> Self {
        builder.finish()
    }
}
