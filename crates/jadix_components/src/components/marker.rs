//! # Marker
//!
//! ShadCN-style conversation marker. Aligns with
//! `Reference/ShadCN-UI/.../bases/base/ui/marker.tsx` — inline status, bordered
//! row, or labeled separator.
//!
//! Chrome (root): `relative flex min-h-4 w-full items-center gap-2 text-left
//! text-sm text-muted-foreground`.
//! Variants: `default` (inline note) · `border` (bottom rule + `pb-2`) ·
//! `separator` (flex-1 hairlines on each side; CSS `::before`/`::after` are
//! real rule nodes here).
//!
//! ```text
//! Marker                          // variant: default | border | separator
//! ├── marker-rule (start)         // separator only · flex-1 hairline
//! ├── marker-cluster              // icon + caption · shrink-0
//! │   ├── MarkerIcon?             // decorative · aria-hidden · size-4
//! │   └── MarkerContent           // min-w-0 · wrap-break-word
//! └── marker-rule (end)
//! ```
//!
//! ## Usage (dual style · ADR-9)
//! ```ignore
//! // A · RSX / tags
//! // PascalCase `<Marker>` is this component. Lowercase `<marker>` stays the
//! // SVG primitive (same split as `<Image>` / `<image>`, `<Switch>` / `<switch>`).
//! <Marker>
//!     <MarkerIcon>
//!         <Icon name="git-branch" size="1rem" />
//!     </MarkerIcon>
//!     <MarkerContent>"Switched to a new branch"</MarkerContent>
//! </Marker>
//! <Marker variant="separator">
//!     <MarkerContent>"Today"</MarkerContent>
//! </Marker>
//! <Marker variant="border" role="status">
//!     <MarkerIcon>
//!         <Icon name="loader-circle" size="1rem" class="animate-spin" />
//!     </MarkerIcon>
//!     <MarkerContent class="shimmer">"Thinking..."</MarkerContent>
//! </Marker>
//!
//! // B · Builder
//! // `marker()` lives on `jadix::components`. Prelude already exports SVG
//! // `marker()`, so the prelude alias is [`ui_marker`].
//! // Root / icon / content each take `.class()` and `.style(|b| …)` (same
//! // [`BaseComponentBuilder`] as RSX props). Prefer `.icon_with` / `.content_with`
//! // when a slot needs its own utilities; `.icon(child)` / `.content(text)` stay
//! // as the short path.
//! Element::from(
//!     marker()
//!         .variant("separator")
//!         .content("Conversation compacted")
//! )
//! Element::from(
//!     ui_marker()
//!         .variant("border")
//!         .icon(icon_child)
//!         .content("Explored 4 files")
//! )
//! Element::from(
//!     marker()
//!         .class("w-full")
//!         .style(|b| b.gap(12.0))
//!         .icon_with(|icon| {
//!             icon.class("text-primary")
//!                 .style(|b| b.w("1.25rem").h("1.25rem"))
//!                 .child(icon_child)
//!         })
//!         .content_with(|c| {
//!             c.class("shimmer")
//!                 .style(|b| b.text_size(16.0))
//!                 .text("Thinking...")
//!         })
//! )
//! ```
//!
//! ## Notes
//! - **Presentational**: empty `id` omits `template_id` (WP-2.8).
//! - `<Marker>` / `MarkerIcon` / `MarkerContent` use the generic PascalCase →
//!   `*Props` path (same as Kbd / Empty; no special `ui!` scope).
//! - Dynamic `variant={sig.get()}` reduces to [`apply_bound_marker_variant`].
//! - Separator hairlines live in a `[rule, label-cluster, rule]` row so leftover
//!   width becomes the lines, not extra space between icon and caption. Rules
//!   stay in the tree (`Display::None` when not separator) so variant binds
//!   restyle in place. `class="flex-col"` stacks icon above content on the
//!   cluster.
//! - `class="shimmer"` on [`MarkerContent`] is the shadcn streaming-text utility
//!   (`<MarkerContent class="shimmer">Thinking...</MarkerContent>`).
//!   Repeat with `shimmer-count-3`. Do not also put `shimmer-box` on streaming
//!   text — that overlay sits above glyphs; use it on a container instead.

use taffy::style::Display;

use crate::components::base::BaseComponentBuilder;
use crate::core::tree::{parse_color, parse_text_color, Element, IntoF32, LayoutStatus};
use crate::primitives::r#box::{hstack, r#box};
use crate::style::{is_variant_style_overridden, with_class_list, VariantStyleField};
use crate::tokens;
use crate::utils::take_packed;
use crate::utils::text_node::{build_text_node, TextNodeConfig};

const ATTR_SLOT: &str = "data-slot";
const ATTR_VARIANT: &str = "data-variant";
const ATTR_EDGE: &str = "data-edge";
const ATTR_MARKER_VARIANT: &str = "jadix_marker_variant";

const SLOT_ROOT: &str = "marker";
const SLOT_ICON: &str = "marker-icon";
const SLOT_CONTENT: &str = "marker-content";
const SLOT_RULE: &str = "marker-rule";
const SLOT_CLUSTER: &str = "marker-cluster";
/// SDF AA swallows a 1px fill; 2px keeps a solid core on the hairline.
const RULE_H: f32 = 2.0;
const RULE_MIN_W: &str = "1rem";

const CONTENT_SIZE_PX: f32 = 14.0;
const CONTENT_WEIGHT: u16 = 400;
const ICON_GLYPH: &str = "1rem";
const MIN_H: &str = "1rem";
const EDGE_START: &str = "start";
const EDGE_END: &str = "end";

// ---------------------------------------------------------------------------
// Variant
// ---------------------------------------------------------------------------

/// Marker layout: `default` (inline) | `border` (bottom rule) | `separator`
/// (labeled divider).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MarkerVariant {
    #[default]
    Default,
    Border,
    Separator,
}

impl MarkerVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Border => "border",
            Self::Separator => "separator",
        }
    }
}

impl From<&str> for MarkerVariant {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or(Self::Default)
    }
}

impl From<String> for MarkerVariant {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl std::str::FromStr for MarkerVariant {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "border" => Self::Border,
            "separator" | "sep" => Self::Separator,
            _ => Self::Default,
        })
    }
}

fn marker_variant_from_attrs(attrs: &std::collections::HashMap<String, String>) -> MarkerVariant {
    attrs
        .get(ATTR_MARKER_VARIANT)
        .map(|s| MarkerVariant::from(s.as_str()))
        .unwrap_or_default()
}

fn content_text_config(variant: MarkerVariant) -> TextNodeConfig {
    TextNodeConfig {
        size_px: CONTENT_SIZE_PX,
        weight: CONTENT_WEIGHT,
        color: parse_text_color("muted-foreground"),
        text_align: if variant == MarkerVariant::Separator {
            crate::style::TextAlign::Center
        } else {
            crate::style::TextAlign::Start
        },
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Paint
// ---------------------------------------------------------------------------

fn preserve_binding_identity(from: &Element, to: &mut Element) {
    to.template_id = from.template_id.clone();
    to.focus_id = from.focus_id.clone();
    to.on_click = from.on_click.clone();
    to.on_unmount = from.on_unmount.clone();
    to.set_layout_node_id(from.layout_node_id());
}

fn make_rule(edge: &'static str) -> Element {
    let mut el: Element = r#box().into();
    el.set_attr(ATTR_SLOT, SLOT_RULE);
    el.set_attr(ATTR_EDGE, edge);
    el = el.pointer_events_none();
    el
}

fn slot_name(el: &Element) -> Option<&str> {
    el.attrs.get(ATTR_SLOT).map(String::as_str)
}

fn wrap_label_cluster(kids: Vec<Element>) -> Element {
    let mut el: Element = hstack()
        .items_center()
        .gap(tokens::SPACE_2)
        .flex_grow(0.0)
        .flex_shrink(0.0)
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_CLUSTER);
    el
}

/// `[start-rule, label-cluster, end-rule]` so leftover width becomes hairlines,
/// not extra space between icon and caption.
fn ensure_marker_chrome(el: &mut Element) {
    let mut start: Option<Element> = None;
    let mut end: Option<Element> = None;
    let mut cluster: Option<Element> = None;
    let mut loose = Vec::new();
    for child in std::mem::take(&mut el.children) {
        match slot_name(&child) {
            Some(SLOT_RULE) => {
                if child.attrs.get(ATTR_EDGE).map(String::as_str) == Some(EDGE_END) {
                    end = Some(child);
                } else {
                    start = Some(child);
                }
            }
            Some(SLOT_CLUSTER) => cluster = Some(child),
            _ => loose.push(child),
        }
    }
    let cluster = match cluster {
        Some(mut existing) => {
            existing.children.extend(loose);
            existing
        }
        None => wrap_label_cluster(loose),
    };
    // Reuse rule nodes so in-place binds keep Taffy child identities.
    el.children = vec![
        start.unwrap_or_else(|| make_rule(EDGE_START)),
        cluster,
        end.unwrap_or_else(|| make_rule(EDGE_END)),
    ];
}

fn sync_cluster_axis(el: &mut Element) {
    let dir = el.style.flex_direction;
    for child in &mut el.children {
        if slot_name(child) == Some(SLOT_CLUSTER) {
            child.style.flex_direction = dir;
        }
    }
}

/// Opaque divider: theme `border` is ~#E5E5E5 (light) or white@0.1 (dark) —
/// a 1px SDF fill of that token disappears on card. Mix toward foreground.
fn rule_fill_color() -> [f32; 4] {
    let border = tokens::border();
    let fg = parse_color("foreground");
    let card = tokens::card();
    let bo = [
        border[0] * border[3] + card[0] * (1.0 - border[3]),
        border[1] * border[3] + card[1] * (1.0 - border[3]),
        border[2] * border[3] + card[2] * (1.0 - border[3]),
    ];
    [
        bo[0] * 0.55 + fg[0] * 0.45,
        bo[1] * 0.55 + fg[1] * 0.45,
        bo[2] * 0.55 + fg[2] * 0.45,
        1.0,
    ]
}

fn paint_rule(el: &mut Element, variant: MarkerVariant) {
    let edge = el
        .attrs
        .get(ATTR_EDGE)
        .cloned()
        .unwrap_or_else(|| EDGE_START.to_string());
    let mut painted = if variant == MarkerVariant::Separator {
        let mut node = std::mem::take(el)
            // Hidden path sets `w(0)`. Clear it: leftover definite width plus
            // flex-grow cannot paint a hairline after default/border → separator.
            .w("auto")
            .h(RULE_H)
            .min_h(RULE_H)
            .min_w(RULE_MIN_W)
            .flex_grow(1.0)
            .flex_shrink(0.0)
            .flex_basis(0.0)
            .align_self_kw("center")
            .bg(rule_fill_color())
            .pointer_events_none();
        node.style.display = Display::Flex;
        node
    } else {
        let mut node = std::mem::take(el)
            .w(0.0)
            .h(0.0)
            .min_w(0.0)
            .min_h(0.0)
            .flex_grow(0.0)
            .flex_shrink(0.0)
            .flex_basis(0.0)
            .pointer_events_none();
        node.style.display = Display::None;
        node
    };
    painted.set_attr(ATTR_SLOT, SLOT_RULE);
    painted.set_attr(ATTR_EDGE, edge);
    *el = painted;
}

fn slot_resolved_fg(el: &Element, fallback: [u8; 4]) -> [u8; 4] {
    if is_variant_style_overridden(&el.attrs, VariantStyleField::Fg) {
        el.text_style.color.unwrap_or(fallback)
    } else {
        fallback
    }
}

fn paint_icon_child(mut child: Element, fg: [u8; 4]) -> Element {
    child = child.pointer_events_none();
    if child.text_style.font_family.as_deref() == Some("lucide")
        && child.text_style.size_resolved() <= 1.0
    {
        child = child.text_size(IntoF32::into_f32(ICON_GLYPH));
    }
    if child.text.is_some() && !is_variant_style_overridden(&child.attrs, VariantStyleField::Fg) {
        child.text_style.color = Some(fg);
    }
    child
}

fn paint_icon_slot(el: &mut Element, fg: [u8; 4]) {
    let fg = slot_resolved_fg(el, fg);
    let kids: Vec<Element> = std::mem::take(&mut el.children)
        .into_iter()
        .map(|c| paint_icon_child(c, fg))
        .collect();
    // Keep user `.style` / class size on the slot; `build_marker_icon` already
    // stamped the 1rem default.
    let mut painted: Element = std::mem::take(el)
        .flex_shrink(0.0)
        .items_center()
        .justify_center()
        .children(kids)
        .pointer_events_none();
    painted.set_attr(ATTR_SLOT, SLOT_ICON);
    painted.set_attr("aria-hidden", "true");
    *el = painted;
}

fn paint_content_slot(el: &mut Element, variant: MarkerVariant, fg: [u8; 4]) {
    let cfg = content_text_config(variant);
    let fg = slot_resolved_fg(el, fg);
    let size = el.text_style.size.unwrap_or(cfg.size_px);
    let weight = el.text_style.weight.unwrap_or(cfg.weight);
    let align = el.text_style.align.unwrap_or(cfg.text_align);
    let kids: Vec<Element> = std::mem::take(&mut el.children)
        .into_iter()
        .map(|mut child| {
            // Update in place so slot `.class` / `.style` (and shimmer attrs on
            // the caption) survive parent `apply_marker_paint`.
            if !is_variant_style_overridden(&child.attrs, VariantStyleField::Fg) {
                child.text_style.color = Some(fg);
            }
            child.text_style.size = Some(size);
            child.text_style.weight = Some(weight);
            child.text_style.align = Some(align);
            child
        })
        .collect();
    let mut painted = std::mem::take(el).min_w(0.0).children(kids);
    painted = if variant == MarkerVariant::Separator {
        painted.flex_grow(0.0).flex_shrink(0.0).text_center()
    } else {
        painted.text_left()
    };
    painted.set_attr(ATTR_SLOT, SLOT_CONTENT);
    *el = painted;
}

fn paint_loose_child(el: &mut Element, variant: MarkerVariant, fg: [u8; 4]) {
    let cfg = content_text_config(variant);
    if el.text.is_some() {
        if let Some(text) = el.text.take() {
            let mut node = build_text_node(text, cfg);
            preserve_binding_identity(el, &mut node);
            *el = node;
        }
        return;
    }
    if el.text_style.font_family.as_deref() == Some("lucide")
        && el.text_style.size_resolved() <= 1.0
    {
        *el = std::mem::take(el).text_size(IntoF32::into_f32(ICON_GLYPH));
    }
    if el.text.is_some() {
        el.text_style.color = Some(fg);
    }
}

fn paint_marker_slots(el: &mut Element, variant: MarkerVariant) {
    let fg = parse_text_color("muted-foreground");
    let cluster_dir = el.style.flex_direction;
    for child in &mut el.children {
        match child.attrs.get(ATTR_SLOT).map(String::as_str) {
            Some(SLOT_RULE) => paint_rule(child, variant),
            Some(SLOT_CLUSTER) => {
                child.style.flex_grow = 0.0;
                child.style.flex_shrink = 0.0;
                child.style.flex_direction = cluster_dir;
                paint_marker_slots(child, variant);
            }
            Some(SLOT_ICON) => paint_icon_slot(child, fg),
            Some(SLOT_CONTENT) => paint_content_slot(child, variant, fg),
            _ => paint_loose_child(child, variant, fg),
        }
    }
}

/// Apply variant chrome in place so [`bind_attr`] can restyle a mounted root.
pub fn apply_marker_paint(el: &mut Element, variant: MarkerVariant) -> LayoutStatus {
    let prev = marker_variant_from_attrs(&el.attrs);
    if prev == variant && el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_ROOT) {
        return LayoutStatus::Clean;
    }

    let mut painted = std::mem::take(el)
        .w("100%")
        .min_w(0.0)
        .max_w("100%")
        .align_self_kw("stretch")
        .min_h(MIN_H)
        .items_center()
        .gap(tokens::SPACE_2)
        .text_size(CONTENT_SIZE_PX)
        .text_left();

    painted = match variant {
        MarkerVariant::Border => painted
            .border_style("solid")
            .border_bottom(parse_color("border"), 1.0)
            .pb(tokens::SPACE_2),
        MarkerVariant::Default | MarkerVariant::Separator => {
            painted.border_bottom_width(0.0).pb(0.0)
        }
    };

    painted.set_attr(ATTR_SLOT, SLOT_ROOT);
    painted.set_attr(ATTR_VARIANT, variant.as_str());
    painted.set_attr(ATTR_MARKER_VARIANT, variant.as_str());
    ensure_marker_chrome(&mut painted);
    paint_marker_slots(&mut painted, variant);
    *el = painted;

    if prev == variant {
        LayoutStatus::StyleChanged
    } else {
        // Child `display` / flex-grow flips (separator hairlines). Bind
        // `collect_bind_layout_dirty` must dirty this subtree: root-only
        // LayoutChanged leaves Taffy children on the previous Display::None.
        LayoutStatus::StructureChanged
    }
}

/// Macro bind: reactive `variant={sig.get()}` on `<Marker>`.
pub fn apply_bound_marker_variant(el: &mut Element, variant: impl AsRef<str>) -> LayoutStatus {
    apply_marker_paint(el, MarkerVariant::from(variant.as_ref()))
}

fn build_marker_root(
    variant: MarkerVariant,
    children: Vec<Element>,
    role: Option<String>,
) -> Element {
    let mut el: Element = hstack().children(children).into();
    apply_marker_paint(&mut el, variant);
    if let Some(role) = role.filter(|r| !r.trim().is_empty()) {
        el.set_attr("role", role.as_str());
        el = el.accesskit_role(role);
    }
    el
}

fn build_marker_icon(children: Vec<Element>) -> Element {
    let fg = parse_text_color("muted-foreground");
    let kids: Vec<Element> = children
        .into_iter()
        .map(|c| paint_icon_child(c, fg))
        .collect();
    let mut el: Element = hstack()
        .items_center()
        .justify_center()
        .flex_shrink(0.0)
        .w(ICON_GLYPH)
        .h(ICON_GLYPH)
        .min_w(ICON_GLYPH)
        .min_h(ICON_GLYPH)
        .children(kids)
        .into();
    el = el.pointer_events_none();
    el.set_attr(ATTR_SLOT, SLOT_ICON);
    el.set_attr("aria-hidden", "true");
    el
}

fn build_marker_content(variant: MarkerVariant, children: Vec<Element>) -> Element {
    let mut el: Element = hstack().min_w(0.0).children(children).into();
    paint_content_slot(&mut el, variant, parse_text_color("muted-foreground"));
    el
}

// ---------------------------------------------------------------------------
// RSX parts
// ---------------------------------------------------------------------------

crate::__component_contract! {
MarkerProps [base_builder];
/// Root conversation marker (`variant`: `default` | `border` | `separator`).
///
/// RSX `<Marker variant="…" role="status">`.
#[allow(non_snake_case)]
pub fn Marker(
identity [] id: String,
default [] variant: MarkerVariant,
plain [] role: Option<String>,
base_builder [] base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = build_marker_root(variant, kids, role);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    sync_cluster_axis(&mut el);
    if !id.trim().is_empty() {
        el.template_id = Some(id);
    }
    el
}
}

crate::__component_contract! {
MarkerIconProps [base_builder];
/// Decorative icon slot (`aria-hidden`). Default glyph size is `1rem` (`size-4`).
///
/// RSX `<MarkerIcon>…</MarkerIcon>`.
#[allow(non_snake_case)]
pub fn MarkerIcon(
base_builder [] base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = build_marker_icon(kids);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}
}

crate::__component_contract! {
MarkerContentProps [base_builder];
/// Marker copy (`min-w-0` · muted `text-sm`; centered when the parent is
/// `separator`).
///
/// RSX `<MarkerContent>…</MarkerContent>`.
#[allow(non_snake_case)]
pub fn MarkerContent(
base_builder [] base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = build_marker_content(MarkerVariant::Default, kids);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}
}

// ---------------------------------------------------------------------------
// Builder (ADR-9 surface B)
// ---------------------------------------------------------------------------

type MarkerSlotStyle = Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>;

fn apply_slot_chrome(mut el: Element, class: &str, style: Option<MarkerSlotStyle>) -> Element {
    if !class.trim().is_empty() {
        el = with_class_list(el, class.trim());
    }
    if let Some(style) = style {
        el = style(BaseComponentBuilder::new(el)).el;
    }
    el
}

fn packed_children(children: Vec<Element>) -> Option<Element> {
    if children.is_empty() {
        None
    } else {
        let mut frag = Element::new();
        frag.set_attr("jadix_fragment", "1");
        Some(frag.children(children))
    }
}

pub struct MarkerIconBuilder {
    children: Vec<Element>,
    class: String,
    style: Option<MarkerSlotStyle>,
}

/// Builder entry for [`MarkerIcon`].
pub fn marker_icon() -> MarkerIconBuilder {
    MarkerIconBuilder {
        children: Vec::new(),
        class: String::new(),
        style: None,
    }
}

impl MarkerIconBuilder {
    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }

    /// Utility classes on the icon slot (`text-primary`, `size-5`, …).
    pub fn class(mut self, classes: impl Into<String>) -> Self {
        self.class = classes.into();
        self
    }

    /// Same [`BaseComponentBuilder`] surface as RSX props (`w` / `h` / `text_color` / …).
    pub fn style(
        mut self,
        f: impl FnOnce(BaseComponentBuilder) -> BaseComponentBuilder + 'static,
    ) -> Self {
        self.style = Some(Box::new(f));
        self
    }
}

impl crate::core::tree::IntoElementList for MarkerIconBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<MarkerIconBuilder> for Element {
    fn from(builder: MarkerIconBuilder) -> Self {
        let el = MarkerIcon(MarkerIconProps {
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        apply_slot_chrome(el, &builder.class, builder.style)
    }
}

pub struct MarkerContentBuilder {
    children: Vec<Element>,
    class: String,
    style: Option<MarkerSlotStyle>,
}

/// Builder entry for [`MarkerContent`].
pub fn marker_content() -> MarkerContentBuilder {
    MarkerContentBuilder {
        children: Vec::new(),
        class: String::new(),
        style: None,
    }
}

impl MarkerContentBuilder {
    pub fn text(mut self, content: impl Into<String>) -> Self {
        self.children.push(Element::new().text(content.into()));
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }

    /// Utility classes on the content slot (`shimmer`, `shimmer-duration-1000`, …).
    pub fn class(mut self, classes: impl Into<String>) -> Self {
        self.class = classes.into();
        self
    }

    /// Same [`BaseComponentBuilder`] surface as RSX props (`text_size` / `text_color` / …).
    pub fn style(
        mut self,
        f: impl FnOnce(BaseComponentBuilder) -> BaseComponentBuilder + 'static,
    ) -> Self {
        self.style = Some(Box::new(f));
        self
    }
}

impl crate::core::tree::IntoElementList for MarkerContentBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<MarkerContentBuilder> for Element {
    fn from(builder: MarkerContentBuilder) -> Self {
        let el = MarkerContent(MarkerContentProps {
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        apply_slot_chrome(el, &builder.class, builder.style)
    }
}

pub struct MarkerBuilder {
    id: String,
    variant: MarkerVariant,
    role: Option<String>,
    class: String,
    style: Option<MarkerSlotStyle>,
    children: Vec<Element>,
}

/// Builder entry for [`Marker`].
///
/// Prelude exports this as [`crate::prelude::ui_marker`] because
/// [`crate::primitives::svg::marker`] already owns the `marker` name.
pub fn marker() -> MarkerBuilder {
    MarkerBuilder {
        id: String::new(),
        variant: MarkerVariant::Default,
        role: None,
        class: String::new(),
        style: None,
        children: Vec::new(),
    }
}

/// Prelude-safe alias of [`marker`] (SVG already owns `marker()` on `jadix::prelude`).
pub fn ui_marker() -> MarkerBuilder {
    marker()
}

impl MarkerBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn variant(mut self, variant: impl Into<MarkerVariant>) -> Self {
        self.variant = variant.into();
        self
    }

    pub fn role(mut self, role: impl Into<String>) -> Self {
        self.role = Some(role.into());
        self
    }

    pub fn class(mut self, classes: impl Into<String>) -> Self {
        self.class = classes.into();
        self
    }

    /// Same [`BaseComponentBuilder`] surface as RSX props (`gap` / `w` / `px` / …).
    pub fn style(
        mut self,
        f: impl FnOnce(BaseComponentBuilder) -> BaseComponentBuilder + 'static,
    ) -> Self {
        self.style = Some(Box::new(f));
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn icon(mut self, child: impl Into<Element>) -> Self {
        let el: Element = child.into();
        let el = if slot_name(&el) == Some(SLOT_ICON) {
            el
        } else {
            marker_icon().child(el).into()
        };
        self.children.push(el);
        self
    }

    /// Style the icon slot, then attach a child (`class` / `style` / `child`).
    pub fn icon_with(mut self, f: impl FnOnce(MarkerIconBuilder) -> MarkerIconBuilder) -> Self {
        self.children.push(f(marker_icon()).into());
        self
    }

    pub fn content(mut self, text: impl Into<String>) -> Self {
        self.children
            .push(marker_content().text(text.into()).into());
        self
    }

    /// Style the content slot (`shimmer`, `text_size`, …).
    pub fn content_with(
        mut self,
        f: impl FnOnce(MarkerContentBuilder) -> MarkerContentBuilder,
    ) -> Self {
        self.children.push(f(marker_content()).into());
        self
    }

    pub fn content_child(mut self, child: impl Into<Element>) -> Self {
        let el: Element = child.into();
        let el = if slot_name(&el) == Some(SLOT_CONTENT) {
            el
        } else {
            marker_content().child(el).into()
        };
        self.children.push(el);
        self
    }
}

impl crate::core::tree::IntoElementList for MarkerBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<MarkerBuilder> for Element {
    fn from(builder: MarkerBuilder) -> Self {
        let mut el = Marker(MarkerProps {
            id: builder.id,
            variant: Some(builder.variant),
            role: builder.role,
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        el = apply_slot_chrome(el, &builder.class, builder.style);
        sync_cluster_axis(&mut el);
        el
    }
}

#[cfg(all(test, feature = "product-tests"))]
mod tests {
    use super::*;
    use crate::style::apply_class_list;

    fn slot_of(el: &Element) -> Option<&str> {
        el.attrs.get(ATTR_SLOT).map(String::as_str)
    }

    fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if slot_of(el) == Some(slot) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_slot(c, slot))
    }

    fn rules_of(el: &Element) -> Vec<&Element> {
        el.children
            .iter()
            .filter(|c| slot_of(c) == Some(SLOT_RULE))
            .collect()
    }

    #[test]
    fn marker_default_slot_and_muted_copy() {
        let el: Element = marker().content("Switched to a new branch").into();
        assert_eq!(slot_of(&el), Some(SLOT_ROOT));
        assert_eq!(
            el.attrs.get(ATTR_VARIANT).map(String::as_str),
            Some("default")
        );
        assert!(el.template_id.is_none());
        let content = find_slot(&el, SLOT_CONTENT).expect("content slot");
        let leaf = content
            .children
            .iter()
            .find(|c| c.text.as_deref() == Some("Switched to a new branch"))
            .expect("caption leaf");
        assert_eq!(
            leaf.text_style.color_resolved(),
            parse_text_color("muted-foreground")
        );
        assert_eq!(leaf.text_style.size_resolved(), CONTENT_SIZE_PX);
        for rule in rules_of(&el) {
            assert_eq!(rule.style.display, Display::None);
        }
    }

    #[test]
    fn marker_separator_shows_flex_rules() {
        let el: Element = marker()
            .variant("separator")
            .content("Conversation compacted")
            .into();
        assert_eq!(
            el.attrs.get(ATTR_VARIANT).map(String::as_str),
            Some("separator")
        );
        let rules = rules_of(&el);
        assert_eq!(rules.len(), 2);
        assert!(rules
            .iter()
            .all(|r| r.style.display != Display::None && r.style.flex_grow > 0.0));
        let content = find_slot(&el, SLOT_CONTENT).expect("content");
        assert_eq!(content.style.flex_grow, 0.0);
        assert_eq!(content.style.flex_shrink, 0.0);
    }

    #[test]
    fn marker_border_paints_bottom_rule() {
        let el: Element = marker()
            .variant("border")
            .content("Opened implementation notes")
            .into();
        assert_eq!(
            el.attrs.get(ATTR_VARIANT).map(String::as_str),
            Some("border")
        );
        assert!(el.style.resolved_border_widths()[2] > 0.0);
        assert_eq!(el.style.resolved_border_colors()[2], parse_color("border"));
    }

    #[test]
    fn marker_icon_is_aria_hidden() {
        let el = MarkerIcon(MarkerIconProps {
            children: Some(Element::new().text("branch")),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        assert_eq!(slot_of(&el), Some(SLOT_ICON));
        assert_eq!(
            el.attrs.get("aria-hidden").map(String::as_str),
            Some("true")
        );
    }

    #[test]
    fn builder_assembles_icon_and_content() {
        let el: Element = marker()
            .icon(Element::new().text("⌕"))
            .content("Explored 4 files")
            .into();
        assert_eq!(slot_of(&el), Some(SLOT_ROOT));
        assert!(find_slot(&el, SLOT_ICON).is_some());
        assert!(find_slot(&el, SLOT_CONTENT).is_some());
        assert!(find_slot(&el, SLOT_CLUSTER).is_some());
    }

    #[test]
    fn apply_bound_marker_variant_preserves_template_id() {
        let mut el: Element = marker().content("Today").into();
        el.template_id = Some("marker-bind-test".into());
        assert_eq!(
            apply_bound_marker_variant(&mut el, "separator"),
            LayoutStatus::StructureChanged
        );
        assert_eq!(el.template_id.as_deref(), Some("marker-bind-test"));
        assert_eq!(
            el.attrs.get(ATTR_VARIANT).map(String::as_str),
            Some("separator")
        );
        assert!(rules_of(&el)
            .iter()
            .all(|r| r.style.display != Display::None));
        for rule in rules_of(&el) {
            assert!(
                crate::layout::dim_length_px(rule.style.size.width).is_none(),
                "separator bind must clear hidden w(0), got {:?}",
                rule.style.size.width
            );
            assert!(rule.style.flex_grow > 0.0);
        }
        assert_eq!(
            apply_bound_marker_variant(&mut el, "separator"),
            LayoutStatus::Clean
        );
    }

    fn layout_marker_hairlines(
        marker_el: Element,
    ) -> (Element, crate::layout::layout_tree::RenderResult) {
        let mut root = Element::new().w(320.0).h(64.0).child(marker_el);
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 320.0, 64.0, [-1.0, -1.0]);
        (root, result)
    }

    fn assert_visible_hairlines(root: &Element, result: &crate::layout::layout_tree::RenderResult) {
        let marker = root
            .children
            .iter()
            .find(|c| slot_of(c) == Some(SLOT_ROOT))
            .expect("marker root");
        let rules: Vec<&Element> = marker
            .children
            .iter()
            .filter(|c| slot_of(c) == Some(SLOT_RULE))
            .collect();
        assert_eq!(rules.len(), 2);
        let card = tokens::card();
        for rule in &rules {
            let [_, _, w, h] = rule.layout_bounds;
            assert!(
                w > 24.0,
                "separator hairline should take leftover width, got w={w} bounds={:?}",
                rule.layout_bounds
            );
            assert!(
                h >= 1.5 && h <= 4.0,
                "separator hairline should survive SDF AA, got h={h}"
            );
            assert!(
                (rule.style.bg_color[3] - 1.0).abs() < 0.01,
                "separator hairline must be opaque, bg={:?}",
                rule.style.bg_color
            );
            assert!(
                (luma(rule.style.bg_color) - luma(card)).abs() > 0.2,
                "hairline contrast vs card too low: {:?} card={:?}",
                rule.style.bg_color,
                card
            );
        }
        let hairlines = result
            .instances
            .iter()
            .filter(|i| i.rect_size[1] <= 4.0 && i.rect_size[0] > 24.0 && i.color[3] > 0.5)
            .count();
        assert!(
            hairlines >= 2,
            "expected painted hairline instances, got {hairlines} (instances={})",
            result.instances.len()
        );
    }

    #[test]
    fn bound_variant_default_to_separator_keeps_hairline_geometry() {
        let mut marker_el: Element = marker().content("Today").into();
        assert_eq!(
            apply_bound_marker_variant(&mut marker_el, "separator"),
            LayoutStatus::StructureChanged
        );
        let (root, result) = layout_marker_hairlines(marker_el);
        assert_visible_hairlines(&root, &result);
    }

    #[test]
    fn bound_variant_separator_hairlines_survive_incremental_layout() {
        let marker_el: Element = marker().content("Today").into();
        let mut root = Element::new().w(320.0).h(64.0).child(marker_el);
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 320.0, 64.0, [-1.0, -1.0]);

        let mut dirty = Vec::new();
        {
            let marker = root
                .children
                .iter_mut()
                .find(|c| slot_of(c) == Some(SLOT_ROOT))
                .expect("marker root");
            assert_eq!(
                apply_bound_marker_variant(marker, "separator"),
                LayoutStatus::StructureChanged
            );
            crate::core::tree::collect_bind_layout_dirty(
                marker,
                crate::app::reactive::sanitize_leaf_bind_status(LayoutStatus::StructureChanged),
                &mut dirty,
            );
        }
        assert!(
            !dirty.is_empty(),
            "separator bind must dirty the marker subtree"
        );
        let dirty_set: std::collections::HashSet<_> = dirty.into_iter().collect();
        tree.mark_dirty_nodes_in_tree(&root, &dirty_set);
        let result =
            tree.build_and_layout_fast_ex(&mut root, 320.0, 64.0, [-1.0, -1.0], false, false, None);
        assert_visible_hairlines(&root, &result);
    }

    #[test]
    fn dual_surface_compiles_shape() {
        let _b: Element = marker().variant("border").content("X").into();
        let _u: Element = ui_marker().variant("separator").content("Y").into();
        let el = Marker(MarkerProps {
            id: String::new(),
            variant: Some(MarkerVariant::Default),
            children: None,
            base: None,
            base_builder: None,
            ..Default::default()
        });
        let _: Element = el;
    }

    fn luma(c: [f32; 4]) -> f32 {
        0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
    }

    #[test]
    fn separator_rules_get_visible_hairline_geometry() {
        let marker_el: Element = marker().variant("separator").content("Today").into();
        let mut root = Element::new().w(320.0).h(64.0).child(marker_el);
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 320.0, 64.0, [-1.0, -1.0]);
        let marker = root
            .children
            .iter()
            .find(|c| slot_of(c) == Some(SLOT_ROOT))
            .expect("marker root");
        let rules: Vec<&Element> = marker
            .children
            .iter()
            .filter(|c| slot_of(c) == Some(SLOT_RULE))
            .collect();
        assert_eq!(rules.len(), 2);
        let card = tokens::card();
        for rule in &rules {
            let [_, _, w, h] = rule.layout_bounds;
            assert!(
                w > 24.0,
                "separator hairline should take leftover width, got w={w} bounds={:?}",
                rule.layout_bounds
            );
            assert!(
                h >= 1.5 && h <= 4.0,
                "separator hairline should survive SDF AA, got h={h}"
            );
            assert!(
                (rule.style.bg_color[3] - 1.0).abs() < 0.01,
                "separator hairline must be opaque, bg={:?}",
                rule.style.bg_color
            );
            assert!(
                (luma(rule.style.bg_color) - luma(card)).abs() > 0.2,
                "hairline contrast vs card too low: {:?} card={:?}",
                rule.style.bg_color,
                card
            );
        }
        let hairlines = result
            .instances
            .iter()
            .filter(|i| i.rect_size[1] <= 4.0 && i.rect_size[0] > 24.0 && i.color[3] > 0.5)
            .count();
        assert!(
            hairlines >= 2,
            "expected painted hairline instances, got {hairlines} (instances={})",
            result.instances.len()
        );
    }

    fn text_box(n: &crate::layout::layout_tree::TextLayoutInfo) -> [f32; 4] {
        [n.x, n.y, n.width, n.height]
    }

    fn aabb_overlap(a: [f32; 4], b: [f32; 4], slack: f32) -> bool {
        a[0] + slack < b[0] + b[2]
            && b[0] + slack < a[0] + a[2]
            && a[1] + slack < b[1] + b[3]
            && b[1] + slack < a[1] + a[3]
    }

    fn contains_box(outer: [f32; 4], inner: [f32; 4], pad: f32) -> bool {
        inner[0] + 0.5 >= outer[0] - pad
            && inner[1] + 0.5 >= outer[1] - pad
            && inner[0] + inner[2] <= outer[0] + outer[2] + pad + 0.5
            && inner[1] + inner[3] <= outer[1] + outer[3] + pad + 0.5
    }

    /// Collect ink must sit in the caption / rule slots. A full-width root
    /// `text_left` used to paint the caption at the Marker origin (叠在发丝线上)
    /// and dest-reset then smeared the leftover glyphs.
    #[test]
    fn marker_collect_ink_stays_in_slots_not_root_origin() {
        let default_el: Element = marker()
            .icon(Element::new().text("⌕"))
            .content("Explored 4 files")
            .into();
        let sep_el: Element = marker()
            .variant("separator")
            .icon(Element::new().text("⌕"))
            .content("Explored 4 files")
            .into();

        for (label, marker_el) in [("default", default_el), ("separator", sep_el)] {
            let mut root = Element::new().w(320.0).h(64.0).child(marker_el.w("100%"));
            let mut tree = crate::layout::layout_tree::LayoutTree::new();
            let result = tree.build_and_layout(&mut root, 320.0, 64.0, [-1.0, -1.0]);
            let marker = root
                .children
                .iter()
                .find(|c| slot_of(c) == Some(SLOT_ROOT))
                .unwrap_or_else(|| panic!("{label}: marker root"));
            let content =
                find_slot(marker, SLOT_CONTENT).unwrap_or_else(|| panic!("{label}: content slot"));
            let icon = find_slot(marker, SLOT_ICON).unwrap_or_else(|| panic!("{label}: icon slot"));
            let caption = result
                .text_nodes
                .iter()
                .find(|n| n.text.contains("Explored 4 files"))
                .unwrap_or_else(|| panic!("{label}: caption collect"));
            let caption_box = text_box(caption);
            assert!(
                contains_box(content.layout_bounds, caption_box, 4.0),
                "{label}: caption collect must sit in the content slot, not the Marker origin\n caption={caption_box:?} content={:?} marker={:?}",
                content.layout_bounds,
                marker.layout_bounds
            );
            assert!(
                caption_box[0] + 0.5 >= icon.layout_bounds[0] + icon.layout_bounds[2] - 1.0,
                "{label}: caption must start after the icon (渲染错位)\n caption={caption_box:?} icon={:?}",
                icon.layout_bounds
            );
            if label == "separator" {
                for rule in rules_of(marker) {
                    let hair = result.instances.iter().find(|i| {
                        i.rect_size[1] <= 4.0
                            && i.rect_size[0] > 24.0
                            && (i.rect_pos[0] - rule.layout_bounds[0]).abs() < 4.0
                    });
                    if let Some(h) = hair {
                        let hb = [h.rect_pos[0], h.rect_pos[1], h.rect_size[0], h.rect_size[1]];
                        assert!(
                            !aabb_overlap(hb, caption_box, 1.0),
                            "{label}: hairline overlaps caption (弄脏)\n hair={hb:?} caption={caption_box:?} rule={:?}",
                            rule.layout_bounds
                        );
                        let cy = caption_box[1] + caption_box[3] * 0.5;
                        let hy = hb[1] + hb[3] * 0.5;
                        assert!(
                            (hy - cy).abs() <= 8.0,
                            "{label}: hairline Y {hy} must stay near caption center {cy} (渲染错位)\n hair={hb:?} caption={caption_box:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn default_and_separator_keep_icon_content_gap() {
        fn gap_after_layout(variant: &str) -> f32 {
            let marker_el: Element = marker()
                .variant(variant)
                .icon(Element::new().text("i"))
                .content("Hello")
                .into();
            let mut root = Element::new().w(320.0).h(64.0).child(marker_el);
            let mut tree = crate::layout::layout_tree::LayoutTree::new();
            let _ = tree.build_and_layout(&mut root, 320.0, 64.0, [-1.0, -1.0]);
            let marker = root
                .children
                .iter()
                .find(|c| slot_of(c) == Some(SLOT_ROOT))
                .expect("marker");
            let icon = find_slot(marker, SLOT_ICON).expect("icon");
            let content = find_slot(marker, SLOT_CONTENT).expect("content");
            content.layout_bounds[0] - (icon.layout_bounds[0] + icon.layout_bounds[2])
        }
        let default_gap = gap_after_layout("default");
        let separator_gap = gap_after_layout("separator");
        assert!(
            (default_gap - separator_gap).abs() < 1.0,
            "separator must not open space between icon and caption: default={default_gap} separator={separator_gap}"
        );
    }

    #[test]
    fn flex_col_class_stacks_cluster() {
        let el: Element = marker()
            .class("flex-col")
            .icon(Element::new().text("i"))
            .content("Syncing completed")
            .into();
        let cluster = find_slot(&el, SLOT_CLUSTER).expect("cluster");
        assert_eq!(
            cluster.style.flex_direction,
            taffy::style::FlexDirection::Column
        );
    }

    #[test]
    fn marker_role_status_is_forwarded() {
        let el: Element = marker().role("status").content("Thinking...").into();
        assert_eq!(el.attrs.get("role").map(String::as_str), Some("status"));
    }

    #[test]
    fn marker_content_shimmer_class() {
        let el: Element = marker_content().class("shimmer").text("Thinking...").into();
        assert_eq!(slot_of(&el), Some(SLOT_CONTENT));
        assert_eq!(
            el.attrs
                .get(crate::renderer::ATTR_GLYPH_FX)
                .map(String::as_str),
            Some("shimmer")
        );
        assert!(el
            .style
            .animations
            .iter()
            .any(|a| a.name == "tw-shimmer" && a.duration_ms == 2000.0));
    }

    #[test]
    fn builder_styles_icon_content_and_root_slots() {
        let fg = parse_text_color("foreground");
        let el: Element = marker()
            .style(|b| b.gap(12.0))
            .icon_with(|icon| {
                icon.class("text-primary")
                    .style(|b| b.w("1.25rem").h("1.25rem"))
                    .child(Element::new().text("⌕"))
            })
            .content_with(|c| {
                c.class("shimmer")
                    .style(move |b| b.text_size(16.0).text_color(fg))
                    .text("Thinking...")
            })
            .into();

        let icon = find_slot(&el, SLOT_ICON).expect("icon");
        let default_icon: Element = marker_icon().child(Element::new().text("x")).into();
        assert_eq!(
            crate::layout::dim_length_px(icon.style.size.width),
            Some(20.0)
        );
        assert_eq!(
            crate::layout::dim_length_px(default_icon.style.size.width),
            Some(16.0)
        );
        assert_eq!(
            icon.attrs
                .get(crate::style::ATTR_OVERRIDE_FG)
                .map(String::as_str),
            Some("1")
        );
        let glyph = icon
            .children
            .iter()
            .find(|c| c.text.as_deref() == Some("⌕"))
            .expect("glyph");
        assert_eq!(
            glyph.text_style.color_resolved(),
            parse_text_color("primary")
        );

        let content = find_slot(&el, SLOT_CONTENT).expect("content");
        assert_eq!(
            content
                .attrs
                .get(crate::renderer::ATTR_GLYPH_FX)
                .map(String::as_str),
            Some("shimmer")
        );
        let leaf = content
            .children
            .iter()
            .find(|c| c.text.as_deref() == Some("Thinking..."))
            .expect("caption");
        assert_eq!(leaf.text_style.color_resolved(), fg);
        assert_eq!(leaf.text_style.size_resolved(), 16.0);
    }

    #[test]
    fn builder_icon_does_not_double_wrap_existing_slot() {
        let el: Element = marker()
            .icon(
                marker_icon()
                    .class("text-primary")
                    .child(Element::new().text("i")),
            )
            .content("X")
            .into();
        fn count(el: &Element, slot: &str) -> usize {
            let here = usize::from(slot_of(el) == Some(slot));
            here + el.children.iter().map(|c| count(c, slot)).sum::<usize>()
        }
        assert_eq!(count(&el, SLOT_ICON), 1);
    }

    #[test]
    fn marker_spin_icon_child_is_pointer_events_none() {
        let mut glyph = crate::utils::build_named_icon_text_node("loader-circle", "\u{ea0e}");
        apply_class_list(&mut glyph, "animate-spin");
        assert_eq!(
            glyph.pointer_events_mode(),
            crate::core::tree::PointerEvents::BoundingBox,
            "lucide Icon defaults to bounding-box before the Marker slot stamps none"
        );
        let el: Element = marker()
            .icon(glyph)
            .content_with(|c| c.class("shimmer").text("Thinking..."))
            .into();
        let icon = find_slot(&el, SLOT_ICON).expect("icon slot");
        assert_eq!(
            icon.pointer_events_mode(),
            crate::core::tree::PointerEvents::None
        );
        let spin = icon
            .children
            .iter()
            .find(|c| c.attrs.get("jadix_animate").map(String::as_str) == Some("spin"))
            .expect("spin glyph");
        assert_eq!(
            spin.pointer_events_mode(),
            crate::core::tree::PointerEvents::None,
            "spin AABB must not steal hover from the Marker row"
        );
    }
}
