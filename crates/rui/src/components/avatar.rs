//! # Avatar
//!
//! ShadCN **base** 头像：对齐
//! `Reference/ShadCN-UI/.../bases/base/ui/avatar.tsx` 与
//! [Avatar 文档](https://ui.shadcn.com/docs/components/aria/avatar)。
//!
//! ```text
//! Avatar
//! ├── AvatarImage
//! ├── AvatarFallback
//! └── AvatarBadge
//!
//! AvatarGroup
//! ├── Avatar …
//! └── AvatarGroupCount
//! ```
//!
//! ## Usage (dual style · ADR-9)
//! ```ignore
//! // A · RSX / tags
//! <Avatar>
//!     <AvatarImage src="assets/user.png" alt="@shadcn" />
//!     <AvatarFallback>"CN"</AvatarFallback>
//!     <AvatarFallback><Icon name="user" /></AvatarFallback>
//!     <AvatarFallback><Image src="assets/placeholder.png" w="100%" h="100%" /></AvatarFallback>
//!     <AvatarBadge class="bg-green-600" />
//! </Avatar>
//! <Avatar size="sm">
//!     <AvatarFallback>"ER"</AvatarFallback>
//! </Avatar>
//! <AvatarGroup>
//!     <Avatar><AvatarFallback>"A"</AvatarFallback></Avatar>
//!     <Avatar><AvatarFallback>"B"</AvatarFallback></Avatar>
//!     <AvatarGroupCount>
//!         "+3"
//!         <Avatar><AvatarFallback>"X"</AvatarFallback></Avatar>
//!     </AvatarGroupCount>
//! </AvatarGroup>
//! <AvatarGroup expand_placement="top-start" expand_class="bg-card shadow-lg">
//!     …
//! </AvatarGroup>
//!
//! // B · Builder
//! Element::from(
//!     avatar()
//!         .size("lg")
//!         .child(|| {
//!             let mut frag = Element::new();
//!             frag.set_attr("rui_fragment", "1");
//!             frag.children(vec![
//!                 AvatarImage(AvatarImageProps { src: "photo.jpg".into(), ..Default::default() }),
//!                 AvatarFallback(AvatarFallbackProps {
//!                     children: Some(Element::new().text("CN")),
//!                     ..Default::default()
//!                 }),
//!             ])
//!         })
//! )
//! avatar().fallback("CN").image("assets/user.png")
//! avatar().fallback_icon("user")
//! avatar().fallback_image("assets/placeholder.png")
//! avatar().fallback_with(|fb| fb.text("AB").child(custom_node))
//! avatar_fallback().icon("user").into() // Must be within avatar_scope / <Avatar>
//! ```
//!
//! ## Notes
//! - **Presentational**: empty `id` omits `template_id` (WP-2.8).
//! - `<Avatar>` is expanded by the macro to [`avatar_scope`], and the component reads the `size` of [`AvatarCtx`].
//! - Image layered on top of Fallback; `AvatarImage` decoded into **circular alpha mask** map (GPU texture pipeline no
//!   border-radius clip），失败 / 空 `src` / Pending 时 opacity=0，Fallback 露出。
//! - 动态 `size={sig.get()}` → [`apply_bound_avatar_size`]（Solid bind，不整树重建）。

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

use crate::app::pending_images::{
    ensure_avatar_src_loaded, texture_key_for_avatar_src, track_src_load_signal, AvatarImageMask,
    SrcLoadState,
};
use crate::app::portal_overlay::{OverlayKind, OverlayMode};
use crate::app::reactive::apply_bound_size;
use crate::app::reactive::bind_attr;
use crate::components::badge::{apply_badge_variant_paint, BadgeVariant};
use crate::components::base::{BaseComponentBuilder, SlotChrome};
use crate::core::reactivity::{
    create_context, create_effect, provide_context, use_context, use_signal, with_child_owner,
    Context, Signal,
};
use crate::core::tree::{parse_color, Element, IntoF32, LayoutStatus};
use crate::impl_slot_chrome_methods;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::style::{parse_text_color, with_class_list};
use crate::tokens;
use crate::utils::anchored_overlay::{
    anchored_overlay_content, provide_anchored_overlay_context, AnchoredOverlayContentOpts,
    FloatingMotionKind, FloatingStrategy, Placement,
};
use crate::utils::lucide::lucide_glyph;
use crate::utils::overflow_fit::tighten_horizontal_constraint;
use crate::utils::popup::{
    mark_floating_with_gap, PopupPlacement, ATTR_FLOATING_EXPAND_CLIP_X,
    ATTR_FLOATING_EXPAND_FROM_H, ATTR_FLOATING_EXPAND_FROM_W, ATTR_FLOATING_MATCH_TRIGGER,
    ATTR_FLOATING_MIN_W, ATTR_FLOATING_STRATEGY, ATTR_FLOATING_W,
};
use crate::utils::take_packed;
use crate::utils::text_node::build_named_icon_text_node;
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use rui_macros::component;

const ATTR_SLOT: &str = "data-slot";
const ATTR_SIZE: &str = "data-size";
const ATTR_SHAPE: &str = "data-shape";
const SLOT_AVATAR: &str = "avatar";
const SLOT_IMAGE: &str = "avatar-image";
const SLOT_FALLBACK: &str = "avatar-fallback";
const SLOT_BADGE: &str = "avatar-badge";
const SLOT_GROUP: &str = "avatar-group";
const SLOT_GROUP_COUNT: &str = "avatar-group-count";
const ATTR_GROUP_ID: &str = "data-avatar-group-id";
const ATTR_GROUP_ROW: &str = "data-avatar-group-row";
const ATTR_MAX_VISIBLE: &str = "data-avatar-max-visible";
/// Default visible faces before folding the rest into `+N` when `max_visible` is set.
pub const DEFAULT_MAX_VISIBLE_AVATARS: usize = 3;
/// Collapsed group overlap (`-space-x-2`).
const GROUP_OVERLAP: &str = "-0.5rem";
/// Expand chrome padding (equal on all sides); placement shifts by this so faces
/// stay on the collapsed avatars instead of aligning the popover box to the group origin.
const EXPAND_CHROME_PAD: &str = tokens::SPACE_2;
const EXPAND_CHROME_BORDER: f32 = 1.0;

// ---------------------------------------------------------------------------
// Size
// ---------------------------------------------------------------------------

/// Avatar `size`: `default` · `sm` · `lg` (counterpoint to ShadCN `data-size`).
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum AvatarSize {
    #[default]
    Default,
    Sm,
    Lg,
}

impl AvatarSize {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "sm" | "small" => Self::Sm,
            "lg" | "large" => Self::Lg,
            _ => Self::Default,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Sm => "sm",
            Self::Lg => "lg",
        }
    }

    /// Root container side length (new-york `size-8` / `size-6` / `size-10`).
    pub fn box_dim(self) -> &'static str {
        match self {
            Self::Sm => "1.5rem",
            Self::Default => "2rem",
            Self::Lg => "2.5rem",
        }
    }

    pub fn fallback_text_px(self) -> f32 {
        match self {
            Self::Sm => IntoF32::into_f32("0.75rem"),
            Self::Default | Self::Lg => IntoF32::into_f32("0.875rem"),
        }
    }

    pub fn fallback_icon_px(self) -> f32 {
        match self {
            Self::Sm => IntoF32::into_f32("0.75rem"),
            Self::Default => IntoF32::into_f32("0.875rem"),
            Self::Lg => IntoF32::into_f32("1rem"),
        }
    }

    pub fn badge_dim(self) -> &'static str {
        match self {
            Self::Sm => "0.5rem",
            Self::Default => "0.625rem",
            Self::Lg => "0.75rem",
        }
    }

    pub fn badge_icon_px(self) -> f32 {
        match self {
            Self::Sm => 0.0,
            Self::Default | Self::Lg => IntoF32::into_f32("0.5rem"),
        }
    }
}

impl From<&str> for AvatarSize {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for AvatarSize {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

impl From<&String> for AvatarSize {
    fn from(s: &String) -> Self {
        Self::parse(s)
    }
}

impl std::str::FromStr for AvatarSize {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::parse(s))
    }
}

/// Avatar 外形：`circle`（默认）· `rounded-square`（圆角正方形）。
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum AvatarShape {
    #[default]
    Circle,
    RoundedSquare,
}

impl AvatarShape {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "rounded" | "rounded-square" | "square" | "rounded_square" => Self::RoundedSquare,
            _ => Self::Circle,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Circle => "circle",
            Self::RoundedSquare => "rounded-square",
        }
    }

    pub fn image_mask(self) -> AvatarImageMask {
        match self {
            Self::Circle => AvatarImageMask::Circle,
            Self::RoundedSquare => AvatarImageMask::RoundedSquare,
        }
    }

    pub fn apply_radii(self, el: Element) -> Element {
        match self {
            Self::Circle => el.rounded_full(),
            Self::RoundedSquare => el.rounded(IntoF32::into_f32("0.375rem")),
        }
    }
}

impl From<&str> for AvatarShape {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for AvatarShape {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

impl From<&String> for AvatarShape {
    fn from(s: &String) -> Self {
        Self::parse(s)
    }
}

impl std::str::FromStr for AvatarShape {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::parse(s))
    }
}

// ---------------------------------------------------------------------------
// Context / scope
// ---------------------------------------------------------------------------

/// Root 上下文：头像尺寸 + 外形。
#[derive(Clone, Copy, Debug)]
pub struct AvatarCtx {
    pub size: AvatarSize,
    pub shape: AvatarShape,
}

fn avatar_context() -> Context<AvatarCtx> {
    static CTX: OnceLock<Context<AvatarCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<AvatarCtx>)
}

/// Read the most recent [`Avatar`] context.
pub fn use_avatar() -> Option<AvatarCtx> {
    use_context(avatar_context())
}

fn require_avatar(part: &str) -> AvatarCtx {
    use_avatar().unwrap_or_else(|| {
        panic!("{part} must be used within <Avatar> / avatar_scope");
    })
}
fn apply_root_shape(el: &mut Element, shape: AvatarShape) {
    el.set_attr(ATTR_SHAPE, shape.as_str());
    *el = shape.apply_radii(std::mem::take(el));
}

fn apply_layer_shape(el: Element, shape: AvatarShape) -> Element {
    shape.apply_radii(el).overflow("hidden")
}

fn apply_root_size(el: &mut Element, size: AvatarSize) -> LayoutStatus {
    let dim = size.box_dim();
    el.set_attr(ATTR_SIZE, size.as_str());
    let mut status = LayoutStatus::Clean;
    for axis in ["w", "h", "min_w", "min_h", "max_w", "max_h"] {
        status = status.merge(apply_bound_size(el, axis, dim));
    }
    status
}

fn apply_fallback_metrics(el: &mut Element, size: AvatarSize) {
    let fg = parse_text_color("muted-foreground");
    let px = size.fallback_text_px();
    let icon_px = size.fallback_icon_px();
    if let Some(text) = el.text.take() {
        let node = build_text_node(
            text,
            TextNodeConfig {
                size_px: px,
                weight: 500,
                color: fg,
                ..Default::default()
            },
        );
        el.children.push(node);
    }
    for child in &mut el.children {
        style_fallback_subtree(child, px, icon_px, fg);
    }
}

fn style_fallback_image_fill(el: &mut Element) {
    for axis in ["w", "h"] {
        let _ = apply_bound_size(el, axis, "100%");
    }
}

fn style_fallback_subtree(el: &mut Element, px: f32, icon_px: f32, fg: [u8; 4]) {
    if el.image_texture.is_some() {
        style_fallback_image_fill(el);
    } else if el.text.is_some() {
        let is_lucide = el.text_style.font_family.as_deref() == Some("lucide");
        if is_lucide {
            if el.text_style.size_resolved() <= 1.0 {
                el.text_style.size = Some(icon_px);
            }
            el.text_style.color = Some(fg);
        } else {
            el.text_style.size = Some(px);
            el.text_style.color = Some(fg);
            el.text_style.weight = Some(500);
        }
    }
    for c in &mut el.children {
        style_fallback_subtree(c, px, icon_px, fg);
    }
}

fn apply_badge_metrics(el: &mut Element, size: AvatarSize) {
    let dim = size.badge_dim();
    for axis in ["w", "h", "min_w", "min_h"] {
        let _ = apply_bound_size(el, axis, dim);
    }
    let icon_px = size.badge_icon_px();
    if icon_px > 0.0 {
        for c in &mut el.children {
            if c.text_style.font_family.as_deref() == Some("lucide") {
                if c.text_style.size_resolved() <= 1.0 {
                    c.text_style.size = Some(icon_px);
                }
            }
        }
    } else {
        // sm：隐藏 badge 内 svg（对位 `[&>svg]:hidden`）
        el.children
            .retain(|c| c.text_style.font_family.as_deref() != Some("lucide"));
    }
}

fn restyle_avatar_subtree(el: &mut Element, ctx: AvatarCtx) {
    match el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) {
        Some(SLOT_FALLBACK) => {
            apply_fallback_metrics(el, ctx.size);
            apply_root_shape(el, ctx.shape);
        }
        Some(SLOT_IMAGE) => {
            *el = apply_layer_shape(std::mem::take(el), ctx.shape);
        }
        Some(SLOT_BADGE) => apply_badge_metrics(el, ctx.size),
        Some(SLOT_AVATAR) => apply_root_shape(el, ctx.shape),
        _ => {}
    }
    for c in &mut el.children {
        restyle_avatar_subtree(c, ctx);
    }
}

fn fill_avatar_layer(el: Element) -> Element {
    el.absolute().top(0.0).left(0.0).right(0.0).bottom(0.0)
}

fn order_avatar_children(children: Vec<Element>) -> Vec<Element> {
    let mut fallbacks = Vec::new();
    let mut images = Vec::new();
    let mut badges = Vec::new();
    let mut rest = Vec::new();
    for c in children {
        match c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) {
            Some(SLOT_FALLBACK) => fallbacks.push(c),
            Some(SLOT_IMAGE) => images.push(c),
            Some(SLOT_BADGE) => badges.push(c),
            _ => rest.push(c),
        }
    }
    fallbacks
        .into_iter()
        .chain(images)
        .chain(badges)
        .chain(rest)
        .collect()
}

fn build_avatar_root(
    id: Option<String>,
    size: AvatarSize,
    shape: AvatarShape,
    mut children: Vec<Element>,
) -> Element {
    let dim = size.box_dim();
    let mut el: Element = r#box().into();
    el = el
        .relative()
        .flex_shrink(0.0)
        .overflow("hidden")
        .w(dim)
        .h(dim)
        .min_w(dim)
        .min_h(dim)
        .max_w(dim)
        .max_h(dim);
    apply_root_shape(&mut el, shape);
    el.set_attr(ATTR_SLOT, SLOT_AVATAR);
    el.set_attr(ATTR_SIZE, size.as_str());
    if let Some(id) = id.filter(|s| !s.is_empty()) {
        el.template_id = Some(id);
    }
    let ctx = AvatarCtx { size, shape };
    children = order_avatar_children(children);
    for mut c in children.drain(..) {
        restyle_avatar_subtree(&mut c, ctx);
        el = el.child(c);
    }
    el
}

/// 提供 Avatar 上下文并构建根容器（宏 `<Avatar>` → 本函数）。
pub fn avatar_scope(
    id: impl Into<String>,
    size: AvatarSize,
    shape: AvatarShape,
    class: Option<String>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = id.into();
    let mut root = with_child_owner(|| {
        provide_context(avatar_context(), AvatarCtx { size, shape });
        let kids = take_packed(body());
        build_avatar_root(
            if id.is_empty() {
                None
            } else {
                Some(id.clone())
            },
            size,
            shape,
            kids,
        )
    });
    if let Some(ref c) = class {
        if !c.trim().is_empty() {
            root = with_class_list(root, c);
        }
    }
    root
}

// ---------------------------------------------------------------------------
// Parts
// ---------------------------------------------------------------------------

const ATTR_LOAD_STATE: &str = "data-load-state";
const ATTR_AVATAR_SRC: &str = "data-avatar-src";

fn load_state_label(state: SrcLoadState) -> &'static str {
    match state {
        SrcLoadState::Unknown => "unknown",
        SrcLoadState::Pending => "pending",
        SrcLoadState::Ready => "ready",
        SrcLoadState::Failed => "failed",
    }
}

fn apply_avatar_image_load(el: &mut Element, key: &str, state: SrcLoadState) -> LayoutStatus {
    let mut status = LayoutStatus::Clean;
    let op = if state.is_ready() { 1.0 } else { 0.0 };
    if (el.style.opacity - op).abs() > f32::EPSILON {
        el.style.opacity = op;
        status = status.merge(LayoutStatus::StyleChanged);
    }
    let want_tex = state.is_ready().then(|| key.to_string());
    if el.image_texture != want_tex {
        el.image_texture = want_tex;
        status = status.merge(LayoutStatus::StyleChanged);
    }
    status
}

fn bind_avatar_image_load(el: Element, key: String, load: Signal<SrcLoadState>) -> Element {
    let key_apply = key.clone();
    bind_attr(
        el,
        format!("{key}__avatar_img_load"),
        move || load.get(),
        move |el, state| {
            el.set_attr(ATTR_LOAD_STATE, load_state_label(state));
            apply_avatar_image_load(el, &key_apply, state)
        },
    )
}

fn build_avatar_texture_layer(src: &str, slot: Option<&str>) -> Element {
    let ctx = require_avatar("AvatarImage");
    let mask = ctx.shape.image_mask();
    let mut el = apply_layer_shape(fill_avatar_layer(r#box().into()), ctx.shape);
    // Paint order is fallback → image → badge (`order_avatar_children`). Do not set
    // z-index: any non-zero child z opts the avatar parent into global
    // `page_stack_bands` (CarouselStack path) and desyncs gallery labels from
    // their rects after scroll — same trap as Chart grid/tooltip z.
    if let Some(slot) = slot {
        el.set_attr(ATTR_SLOT, slot);
    }

    if src.trim().is_empty() {
        el.set_attr(ATTR_LOAD_STATE, "failed");
        el.style.opacity = 0.0;
    } else {
        el.set_attr(ATTR_AVATAR_SRC, src);
        let key = texture_key_for_avatar_src(src, mask);
        let load = use_signal(format!("{key}__src_load").as_str(), || {
            ensure_avatar_src_loaded(&key, src, mask)
        });
        track_src_load_signal(&key, load);
        let state = load.peek();
        el.set_attr(ATTR_LOAD_STATE, load_state_label(state));
        // Seed texture/opacity here. Expand-panel rebuilds reuse the collapsed
        // face's `bind_prop` key; that path skips `apply` and would otherwise
        // paint an empty image layer over muted fallback (A/B turn gray).
        let _ = apply_avatar_image_load(&mut el, &key, state);
        el = bind_avatar_image_load(el, key, load);
    }
    el
}

fn build_avatar_image(
    src: &str,
    alt: &str,
    base: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
) -> Element {
    let mut el = build_avatar_texture_layer(src, Some(SLOT_IMAGE));
    if !alt.is_empty() {
        el = el.accesskit_name(alt);
    }
    if let Some(b) = base {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

fn build_avatar_fallback(
    children: Vec<Element>,
    base: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
) -> Element {
    let ctx = require_avatar("AvatarFallback");
    let muted = parse_color("muted");
    let mut el = apply_layer_shape(fill_avatar_layer(r#box().into()), ctx.shape);
    el = el.bg(muted).display_flex().items_center().justify_center();
    el.set_attr(ATTR_SLOT, SLOT_FALLBACK);
    for mut c in children {
        apply_fallback_metrics(&mut c, ctx.size);
        el = el.child(c);
    }
    if el.children.is_empty() {
        apply_fallback_metrics(&mut el, ctx.size);
    }
    if let Some(b) = base {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

fn apply_avatar_badge_ring(el: &mut Element) {
    let key = el
        .attrs
        .get("data-variant")
        .or_else(|| el.attrs.get("rui_badge_variant"))
        .map(|s| s.as_str())
        .unwrap_or("default");
    if key == "outline" {
        return;
    }
    let ring = parse_color("background");
    el.style.border_color = ring;
    el.style.border_width = 2.0;
}

fn build_avatar_badge(
    variant: BadgeVariant,
    children: Vec<Element>,
    base: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
) -> Element {
    let ctx = require_avatar("AvatarBadge");
    let mut el: Element = r#box()
        .absolute(true)
        .right(0.0)
        .bottom(0.0)
        .rounded_full()
        .into();
    el = el.display_flex().items_center().justify_center();
    el.set_attr(ATTR_SLOT, SLOT_BADGE);
    for mut c in children {
        apply_badge_metrics(&mut c, ctx.size);
        el = el.child(c);
    }
    apply_badge_metrics(&mut el, ctx.size);
    apply_badge_variant_paint(&mut el, variant.as_paint_key());
    apply_avatar_badge_ring(&mut el);
    if let Some(b) = base {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

fn apply_group_ring(mut el: Element) -> Element {
    let slot = el.attrs.get(ATTR_SLOT).map(|s| s.as_str());
    if matches!(slot, Some(SLOT_AVATAR) | Some(SLOT_GROUP_COUNT)) {
        let ring = parse_color("background");
        // Ring after overflow so the 2px stroke is not clipped (same order as face roots).
        el = el.border(ring, 2.0);
    }
    el
}

/// Snapshot of an avatar face for expand-panel rebuild (Element is not Clone).
#[derive(Clone, Debug)]
struct AvatarFaceSpec {
    size: AvatarSize,
    shape: AvatarShape,
    src: Option<String>,
    fallback: Option<String>,
    /// Author utility class on the face root (`bg-red-500`, …). Rebuild must keep it.
    class: Option<String>,
}

fn src_from_avatar_texture_key(key: &str) -> Option<String> {
    let rest = key.strip_prefix("rui-avatar-src:")?;
    let (_mask, src) = rest.split_once(':')?;
    if src.is_empty() {
        None
    } else {
        Some(src.to_string())
    }
}

fn extract_fallback_label(el: &Element) -> Option<String> {
    crate::core::tree::find_in_tree(el, |n| {
        n.text.as_ref().is_some_and(|s| !s.is_empty())
            && n.text_style.font_family.as_deref() != Some("lucide")
    })
    .and_then(|n| n.text.clone())
}

fn extract_face_spec(el: &Element) -> Option<AvatarFaceSpec> {
    if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) != Some(SLOT_AVATAR) {
        return None;
    }
    let size = el
        .attrs
        .get(ATTR_SIZE)
        .map(|s| AvatarSize::parse(s))
        .unwrap_or_default();
    let shape = el
        .attrs
        .get(ATTR_SHAPE)
        .map(|s| AvatarShape::parse(s))
        .unwrap_or_default();
    let mut src = None;
    let mut fallback = None;
    crate::core::tree::walk_elements(el, |n| match n.attrs.get(ATTR_SLOT).map(|s| s.as_str()) {
        Some(SLOT_IMAGE) => {
            if src.is_none() {
                if let Some(s) = n.attrs.get(ATTR_AVATAR_SRC).filter(|s| !s.is_empty()) {
                    src = Some(s.clone());
                } else if let Some(key) = n.image_texture.as_deref() {
                    src = src_from_avatar_texture_key(key);
                }
            }
        }
        Some(SLOT_FALLBACK) => {
            if fallback.is_none() {
                fallback = extract_fallback_label(n);
            }
        }
        _ => {}
    });
    Some(AvatarFaceSpec {
        size,
        shape,
        src,
        fallback,
        class: utility_class_of(el),
    })
}

fn utility_class_of(el: &Element) -> Option<String> {
    if let Some(s) = el
        .attrs
        .get("rui_utility_class")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return Some(s.to_string());
    }
    el.class_set.as_ref().and_then(|cs| {
        let joined = cs.base.join(" ");
        (!joined.trim().is_empty()).then_some(joined)
    })
}

fn rebuild_face(spec: &AvatarFaceSpec) -> Element {
    let mut b = avatar().size(spec.size.as_str()).shape(spec.shape.as_str());
    if let Some(class) = spec
        .class
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        b = b.class(class);
    }
    if let Some(src) = spec.src.as_deref().filter(|s| !s.is_empty()) {
        b = b.image(src);
    }
    match spec.fallback.as_deref().filter(|s| !s.is_empty()) {
        Some(fb) => b.fallback(fb).into(),
        None if spec.src.as_deref().filter(|s| !s.is_empty()).is_none() => b.fallback("?").into(),
        None => b.into(),
    }
}

fn expand_chrome_inset() -> f32 {
    IntoF32::into_f32(EXPAND_CHROME_PAD) + EXPAND_CHROME_BORDER
}

fn expand_content_width(face_size: f32, n: usize) -> f32 {
    let n = n.max(1) as f32;
    let gap = IntoF32::into_f32(tokens::SPACE_1);
    n * face_size + (n - 1.0) * gap + 2.0 * expand_chrome_inset()
}

fn default_expand_container(faces: Vec<Element>, expand_class: Option<&str>) -> Element {
    let mut panel: Element = hstack()
        .w("auto")
        .flex_grow(0.0)
        .flex_shrink(0.0)
        .align_self_kw("flex-start")
        .overflow("hidden")
        .items_center()
        .justify("start")
        .wrap("nowrap")
        .gap(tokens::SPACE_1)
        .p(EXPAND_CHROME_PAD)
        .rounded("0.75rem")
        .bg(tokens::popover())
        .border_style("solid")
        .border_width(EXPAND_CHROME_BORDER)
        .border_color(tokens::border())
        .popup_shadow()
        .children(faces)
        .into();
    if let Some(c) = expand_class.map(str::trim).filter(|s| !s.is_empty()) {
        panel = with_class_list(panel, c);
    }
    panel
}

fn expand_placement_from_attr(raw: &str) -> PopupPlacement {
    PopupPlacement::from_attr(raw.trim())
}

fn expand_cover_gap_for_group(trigger_h: f32) -> f32 {
    -trigger_h.max(1.0)
}

struct AvatarGroupEntry {
    focus_id: String,
    faces: Vec<AvatarFaceSpec>,
    max_visible: usize,
    count_size: AvatarSize,
    manual_visible_n: usize,
    manual_has_count: bool,
    /// Number of faces shown in the collapsed row (not nested under Count).
    visible_n: usize,
    has_count: bool,
    rendered_visible: Cell<usize>,
    expanded: Signal<bool>,
    placement: Cell<PopupPlacement>,
    expand_class: String,
    group_hover: Cell<bool>,
    panel_hover: Cell<bool>,
    hover_armed: Cell<bool>,
}

thread_local! {
    static AVATAR_GROUPS: RefCell<HashMap<String, AvatarGroupEntry>> =
        RefCell::new(HashMap::new());
    /// Smooth-scroll catch-up / fling frames: skip hover-open so +N does not
    /// close→open every residual offset tick under a still pointer.
    static EXPAND_HOVER_FROZEN: Cell<bool> = const { Cell::new(false) };
}

static AVATAR_GROUP_SEQ: AtomicUsize = AtomicUsize::new(0);

fn register_avatar_group_entry(
    id: String,
    focus_id: String,
    faces: Vec<AvatarFaceSpec>,
    max_visible: usize,
    count_size: AvatarSize,
    manual_visible_n: usize,
    manual_has_count: bool,
    visible_n: usize,
    has_count: bool,
    expanded: Signal<bool>,
    placement: PopupPlacement,
    expand_class: String,
) {
    AVATAR_GROUPS.with(|m| {
        m.borrow_mut().insert(
            id,
            AvatarGroupEntry {
                focus_id,
                faces,
                max_visible,
                count_size,
                manual_visible_n,
                manual_has_count,
                visible_n,
                has_count,
                rendered_visible: Cell::new(visible_n),
                expanded,
                placement: Cell::new(placement),
                expand_class,
                group_hover: Cell::new(false),
                panel_hover: Cell::new(false),
                hover_armed: Cell::new(false),
            },
        );
    });
}

fn unregister_avatar_group(id: &str) {
    AVATAR_GROUPS.with(|m| {
        m.borrow_mut().remove(id);
    });
}

fn open_avatar_group_expand(id: &str) {
    open_avatar_group_expand_inner(id, false);
}

fn open_avatar_group_expand_from_click(id: &str) {
    open_avatar_group_expand_inner(id, true);
}

fn open_avatar_group_expand_inner(id: &str, force: bool) {
    let frozen = EXPAND_HOVER_FROZEN.get();
    let (self_exp, others): (Option<Signal<bool>>, Vec<Signal<bool>>) = AVATAR_GROUPS.with(|m| {
        let map = m.borrow();
        let mut others = Vec::new();
        let mut self_exp = None;
        for (k, e) in map.iter() {
            if k == id {
                e.hover_armed.set(true);
                self_exp = Some(e.expanded);
                continue;
            }
            if e.expanded.peek() {
                e.hover_armed.set(false);
                e.group_hover.set(false);
                e.panel_hover.set(false);
                others.push(e.expanded);
            }
        }
        (self_exp, others)
    });
    // Never try_set while AVATAR_GROUPS is borrowed — reactive rebuild re-enters the map.
    if let Some(s) = self_exp {
        if force || !frozen {
            s.try_set(true);
        }
    }
    for o in others {
        o.try_set(false);
    }
}

fn reopen_hovered_avatar_group_expands() -> bool {
    let ids: Vec<String> = AVATAR_GROUPS.with(|m| {
        m.borrow()
            .iter()
            .filter(|(_, e)| e.group_hover.get() || e.panel_hover.get())
            .map(|(k, _)| k.clone())
            .collect()
    });
    if ids.is_empty() {
        return false;
    }
    let mut any = false;
    for id in ids {
        if let Some(s) = AVATAR_GROUPS.with(|m| m.borrow().get(&id).map(|e| e.expanded)) {
            if s.try_set(true) {
                any = true;
            }
        }
    }
    any
}

/// Wheel / smooth-scroll catch-up: block hover-open; on thaw, reopen if the pointer
/// is still on +N / the panel (one open, not a close/open per residual frame).
pub(crate) fn set_avatar_group_expand_hover_frozen(frozen: bool) -> bool {
    let was = EXPAND_HOVER_FROZEN.replace(frozen);
    if was && !frozen {
        reopen_hovered_avatar_group_expands()
    } else {
        false
    }
}

fn note_avatar_group_hover(id: &str, panel: bool, on: bool) {
    AVATAR_GROUPS.with(|m| {
        if let Some(e) = m.borrow().get(id) {
            if panel {
                e.panel_hover.set(on);
            } else {
                e.group_hover.set(on);
            }
            if on {
                e.hover_armed.set(true);
            }
        }
    });
}

fn dismiss_idle_avatar_group_hovers() -> bool {
    let to_close: Vec<Signal<bool>> = AVATAR_GROUPS.with(|m| {
        let mut out = Vec::new();
        for e in m.borrow().values() {
            if !e.expanded.peek() || !e.hover_armed.get() {
                continue;
            }
            if e.group_hover.get() || e.panel_hover.get() {
                continue;
            }
            e.hover_armed.set(false);
            out.push(e.expanded);
        }
        out
    });
    let mut any = false;
    for s in to_close {
        if s.peek() && s.try_set(false) {
            any = true;
        }
    }
    any
}

/// Event-loop: collapse avatar +N expand when pointer leaves group and panel.
pub(crate) fn flush_avatar_group_hover_dismiss() -> bool {
    dismiss_idle_avatar_group_hovers()
}

/// Close every expanded avatar group when a scroll ancestor moves (fixed cover panels
/// otherwise drift from trigger/text until the next full placement sync).
///
/// Hover flags stay: residual smooth-scroll frames would otherwise leave/enter +N
/// and flicker the panel. Hover-open is gated by [`set_avatar_group_expand_hover_frozen`].
pub(crate) fn dismiss_avatar_group_expands_on_scroll() -> bool {
    let to_close: Vec<Signal<bool>> = AVATAR_GROUPS.with(|m| {
        let mut out = Vec::new();
        for e in m.borrow().values() {
            if !e.expanded.peek() {
                continue;
            }
            out.push(e.expanded);
        }
        out
    });
    let mut any = false;
    for s in to_close {
        if s.try_set(false) {
            any = true;
        }
    }
    any
}

fn register_avatar_group_redraw(group_id: &str, expanded: Signal<bool>) {
    let key = format!("{group_id}__redraw");
    use_signal(key.as_str(), move || {
        create_effect(move || {
            let _ = expanded.get();
            crate::app::runtime::request_redraw();
        });
    });
}

fn expand_group_faces(faces: &[AvatarFaceSpec]) -> Vec<Element> {
    faces
        .iter()
        .map(|f| apply_group_ring(rebuild_face(f)))
        .collect()
}

fn collapsed_group_slots(visible: usize, total: usize) -> usize {
    if total <= visible {
        total.max(1)
    } else {
        visible + 1
    }
}

fn avatar_row_width(face_size: f32, visible: usize, total: usize) -> f32 {
    collapsed_group_row_width(face_size, collapsed_group_slots(visible, total))
}

fn effective_visible_count(
    total: usize,
    max_visible: usize,
    avail_w: Option<f32>,
    face_size: f32,
) -> usize {
    if total == 0 {
        return 0;
    }
    let cap = max_visible.min(total);
    let Some(w) = avail_w.filter(|w| *w > 0.0) else {
        return cap;
    };
    for visible in (1..=cap).rev() {
        if avatar_row_width(face_size, visible, total) <= w + 0.5 {
            return visible;
        }
    }
    1
}

fn avatar_size_from_el(el: &Element) -> AvatarSize {
    el.attrs
        .get(ATTR_SIZE)
        .map(|s| AvatarSize::parse(s))
        .unwrap_or(AvatarSize::Default)
}

fn resolve_group_visible_n(
    faces_len: usize,
    manual_visible_n: usize,
    manual_has_count: bool,
    max_visible: usize,
    avail_w: Option<f32>,
    face_size: f32,
) -> usize {
    let manual_cap = if manual_has_count {
        manual_visible_n
    } else {
        faces_len
    };
    let cap = if max_visible == usize::MAX {
        manual_cap
    } else {
        max_visible.min(faces_len)
    };
    effective_visible_count(faces_len, cap, avail_w, face_size)
}

fn hide_overflow_avatar(mut c: Element) -> Element {
    c = c
        .absolute()
        .left(0.0)
        .top(0.0)
        .w(0.0)
        .h(0.0)
        .min_w(0.0)
        .min_h(0.0)
        .max_w(0.0)
        .max_h(0.0)
        .opacity(0.0)
        .overflow("hidden")
        .pointer_events_box_none();
    c
}

fn collapsed_group_row_width(face_size: f32, slots: usize) -> f32 {
    let n = slots.max(1) as f32;
    let overlap = IntoF32::into_f32(GROUP_OVERLAP);
    face_size + (n - 1.0) * (face_size + overlap)
}

fn expand_from_width(face_size: f32, visible_n: usize, has_count: bool) -> f32 {
    let slots = visible_n + usize::from(has_count);
    collapsed_group_row_width(face_size, slots) + 2.0 * expand_chrome_inset()
}

fn build_avatar_group_expand_panel(
    floating_id: &str,
    trigger_fid: &str,
    placement: PopupPlacement,
    faces: &[AvatarFaceSpec],
    expand_class: Option<&str>,
    trigger_h: f32,
    visible_n: usize,
    has_count: bool,
) -> Element {
    let chips = expand_group_faces(faces);
    let mut panel = default_expand_container(chips, expand_class);
    // Pin to the gap row. `width: auto` in a full-size portal + wrap stretches the
    // chrome past the last face; `rui_floating_w=auto` is also parsed as trigger px.
    let content_w = expand_content_width(trigger_h, faces.len());
    panel = panel.w(content_w);
    panel.set_attr(ATTR_FLOATING_W, format!("{content_w}px"));
    panel.set_attr("data-avatar-group-overlay", "1");
    mark_floating_with_gap(
        &mut panel,
        floating_id,
        trigger_fid,
        placement,
        expand_cover_gap_for_group(trigger_h),
    );
    panel.set_attr(ATTR_FLOATING_STRATEGY, "fixed");
    // Content-sized: do not stretch to a full-width group shell / trigger.
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
    panel.set_attr(ATTR_FLOATING_MIN_W, "0");
    // Expand from the collapsed face height (same as BubbleReactions) so overflow-hidden
    // does not clamp the first frame to max-height 0.
    panel.set_attr(ATTR_FLOATING_EXPAND_FROM_H, format!("{trigger_h}"));
    // Clip extra faces on the same 200ms as accordion: collapsed row (+N) → full gap layout.
    let from_w = expand_from_width(trigger_h, visible_n, has_count);
    panel.set_attr(ATTR_FLOATING_EXPAND_FROM_W, format!("{from_w}"));
    panel.set_attr(ATTR_FLOATING_EXPAND_CLIP_X, "1");
    // Cover placement pins the *panel box* to the trigger origin; shift by chrome so the
    // first expanded face sits on the first collapsed face.
    let inset = expand_chrome_inset();
    panel
        .translate(-inset, -inset)
        .align_content_kw("flex-start")
        .absolute()
}

fn collect_count_faces(el: &Element, faces: &mut Vec<AvatarFaceSpec>) {
    if let Some(spec) = extract_face_spec(el) {
        faces.push(spec);
        return;
    }
    for c in &el.children {
        collect_count_faces(c, faces);
    }
}

fn split_group_children(
    children: Vec<Element>,
) -> (Vec<Element>, Option<Element>, Vec<AvatarFaceSpec>, usize) {
    let mut avatars = Vec::new();
    let mut count = None;
    let mut faces = Vec::new();
    let mut visible_n = 0usize;
    for child in children {
        if child.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_GROUP_COUNT) {
            // Overflow avatars nested under Count (optional) join the expand face list.
            collect_count_faces(&child, &mut faces);
            count = Some(child);
        } else if let Some(spec) = extract_face_spec(&child) {
            faces.push(spec.clone());
            avatars.push(child);
            visible_n += 1;
        } else {
            avatars.push(child);
        }
    }
    (avatars, count, faces, visible_n)
}

fn build_auto_group_count(
    group_id: &str,
    overflow_n: usize,
    overflow_faces: &[AvatarFaceSpec],
    size: AvatarSize,
) -> Element {
    let open_id = group_id.to_string();
    let click_id = group_id.to_string();
    let hover_id = group_id.to_string();
    let mut kids = vec![Element::new().text(format!("+{overflow_n}"))];
    for spec in overflow_faces {
        kids.push(hide_overflow_avatar(rebuild_face(spec)));
    }
    let mut count: Element = build_avatar_group_count(size, kids, None);
    count = interactive(count)
        .on_click(move || open_avatar_group_expand_from_click(&click_id))
        .on_hover(move |h| {
            note_avatar_group_hover(&hover_id, false, h);
            if h {
                open_avatar_group_expand(&open_id);
            }
        })
        .into();
    apply_group_ring(count)
}

fn build_avatar_group_row_element(
    group_id: &str,
    faces: &[AvatarFaceSpec],
    visible_n: usize,
    count_size: AvatarSize,
) -> Element {
    let mut row: Element = hstack()
        .w("auto")
        .flex_grow(0.0)
        .flex_shrink(0.0)
        .align_self_kw("flex-start")
        .items_center()
        .into();
    row.set_attr(ATTR_GROUP_ROW, "1");
    for (i, spec) in faces.iter().enumerate().take(visible_n) {
        let mut c = apply_group_ring(rebuild_face(spec));
        if i > 0 {
            c = c.ml(GROUP_OVERLAP);
        }
        row = row.child(c);
    }
    let overflow = faces.len().saturating_sub(visible_n);
    if overflow > 0 {
        let mut count = build_auto_group_count(group_id, overflow, &faces[visible_n..], count_size);
        if visible_n > 0 {
            count = count.ml(GROUP_OVERLAP);
        }
        row = row.child(count);
    }
    row
}

fn find_avatar_group_row_mut(el: &mut Element) -> Option<&mut Element> {
    for child in &mut el.children {
        if child.attrs.get(ATTR_GROUP_ROW).map(|s| s.as_str()) == Some("1") {
            return Some(child);
        }
    }
    None
}

#[cfg(test)]
fn find_avatar_group_row<'a>(el: &'a Element) -> Option<&'a Element> {
    for child in &el.children {
        if child.attrs.get(ATTR_GROUP_ROW).map(|s| s.as_str()) == Some("1") {
            return Some(child);
        }
    }
    None
}

fn rebuild_avatar_group_row(el: &mut Element, group_id: &str, visible_n: usize) {
    let Some(row) = find_avatar_group_row_mut(el) else {
        return;
    };
    let next = AVATAR_GROUPS.with(|m| {
        let map = m.borrow();
        let entry = map.get(group_id)?;
        Some(build_avatar_group_row_element(
            group_id,
            &entry.faces,
            visible_n,
            entry.count_size,
        ))
    });
    if let Some(next) = next {
        *row = next;
    }
}

fn sync_avatar_group_shell(
    el: &mut Element,
    constraint_w: Option<f32>,
    layout_tree: &mut crate::layout::layout_tree::LayoutTree,
    impact: &mut LayoutStatus,
) -> bool {
    let Some(group_id) = el.attrs.get(ATTR_GROUP_ID).cloned() else {
        return false;
    };
    let mut structure_changed = false;
    AVATAR_GROUPS.with(|m| {
        let mut binding = m.borrow_mut();
        let Some(entry) = binding.get_mut(&group_id) else {
            return;
        };
        let face_size = entry
            .faces
            .first()
            .map(|f| IntoF32::into_f32(f.size.box_dim()))
            .unwrap_or(32.0);
        let next_visible = resolve_group_visible_n(
            entry.faces.len(),
            entry.manual_visible_n,
            entry.manual_has_count,
            entry.max_visible,
            constraint_w,
            face_size,
        );
        if next_visible != entry.rendered_visible.get() {
            entry.rendered_visible.set(next_visible);
            entry.visible_n = next_visible;
            entry.has_count = entry.faces.len() > next_visible
                || (entry.manual_has_count && entry.faces.len() <= entry.manual_visible_n);
            structure_changed = true;
        }
        let expanded = entry.expanded.peek();
        el.set_attr("data-state", if expanded { "open" } else { "closed" });
    });
    if structure_changed {
        rebuild_avatar_group_row(
            el,
            &group_id,
            AVATAR_GROUPS.with(|m| {
                m.borrow()
                    .get(&group_id)
                    .map(|e| e.rendered_visible.get())
                    .unwrap_or(0)
            }),
        );
        if el.layout_node.is_some() {
            layout_tree.mark_element_layout_dirty(el);
        }
        *impact = impact.merge(LayoutStatus::StructureChanged);
    }
    structure_changed
}

/// 每帧按父级宽度与 `max_visible` 折叠头像行；指针离开组与浮层时收起展开。
pub(crate) fn sync_avatar_group_subtree(
    el: &mut Element,
    layout_tree: &mut crate::layout::layout_tree::LayoutTree,
    impact: &mut LayoutStatus,
    viewport_w: f32,
) {
    if flush_avatar_group_hover_dismiss() {
        crate::app::runtime::request_redraw();
    }
    let root_budget = (viewport_w > 0.0).then_some(viewport_w);
    walk_avatar_group_sync(el, root_budget, layout_tree, impact);
}

fn walk_avatar_group_sync(
    el: &mut Element,
    constraint_w: Option<f32>,
    layout_tree: &mut crate::layout::layout_tree::LayoutTree,
    impact: &mut LayoutStatus,
) {
    let mut stack: Vec<(*mut Element, Option<f32>)> = vec![(el as *mut Element, constraint_w)];
    while let Some((ptr, constraint_w)) = stack.pop() {
        let el = unsafe { &mut *ptr };
        let child_constraint = tighten_horizontal_constraint(el, constraint_w);
        if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_GROUP) {
            let _ = sync_avatar_group_shell(el, child_constraint, layout_tree, impact);
        }
        let n = el.children.len();
        for i in (0..n).rev() {
            stack.push((std::ptr::addr_of_mut!(el.children[i]), child_constraint));
        }
    }
}

fn build_avatar_group_row_manual(
    group_id: &str,
    avatars: Vec<Element>,
    count_el: Option<Element>,
) -> Element {
    let mut row: Element = hstack()
        .w("auto")
        .flex_grow(0.0)
        .flex_shrink(0.0)
        .align_self_kw("flex-start")
        .items_center()
        .into();
    row.set_attr(ATTR_GROUP_ROW, "1");
    let mut avatar_n = 0usize;
    for child in avatars {
        let mut c = apply_group_ring(child);
        if avatar_n > 0 {
            c = c.ml(GROUP_OVERLAP);
        }
        row = row.child(c);
        avatar_n += 1;
    }
    if let Some(mut count) = count_el {
        let open_id = group_id.to_string();
        let click_id = group_id.to_string();
        let hover_id = group_id.to_string();
        count = interactive(count)
            .on_click(move || open_avatar_group_expand_from_click(&click_id))
            .on_hover(move |h| {
                note_avatar_group_hover(&hover_id, false, h);
                if h {
                    open_avatar_group_expand(&open_id);
                }
            })
            .into();
        count = apply_group_ring(count);
        if avatar_n > 0 {
            count = count.ml(GROUP_OVERLAP);
        }
        row = row.child(count);
    }
    row
}

fn build_avatar_group(
    children: Vec<Element>,
    expand_placement: PopupPlacement,
    expand_class: Option<String>,
    max_visible: usize,
) -> Element {
    let (avatars, count_el, faces, manual_visible_n) = split_group_children(children);
    let seq = AVATAR_GROUP_SEQ.fetch_add(1, Ordering::Relaxed);
    let group_id = format!("avatar_grp_{seq}");
    let focus_id = format!("{group_id}__focus");
    let exp_key = format!("{group_id}__exp");
    let expanded = use_signal(exp_key.as_str(), || false);
    let expand_class = expand_class.unwrap_or_default();
    let manual_has_count = count_el.is_some();
    let count_size = count_el
        .as_ref()
        .map(avatar_size_from_el)
        .or_else(|| faces.first().map(|f| f.size))
        .unwrap_or(AvatarSize::Default);
    let face_size = faces
        .first()
        .map(|f| IntoF32::into_f32(f.size.box_dim()))
        .unwrap_or(32.0);
    let rendered_visible = resolve_group_visible_n(
        faces.len(),
        manual_visible_n,
        manual_has_count,
        max_visible,
        None,
        face_size,
    );
    let has_count = faces.len() > rendered_visible || manual_has_count;

    register_avatar_group_entry(
        group_id.clone(),
        focus_id.clone(),
        faces.clone(),
        max_visible,
        count_size,
        manual_visible_n,
        manual_has_count,
        rendered_visible,
        has_count,
        expanded,
        expand_placement,
        expand_class.clone(),
    );
    register_avatar_group_redraw(&group_id, expanded);

    let row = if max_visible == usize::MAX && rendered_visible == manual_visible_n {
        build_avatar_group_row_manual(&group_id, avatars, count_el)
    } else {
        build_avatar_group_row_element(&group_id, &faces, rendered_visible, count_size)
    };

    let portal_id = group_id.clone();
    let trigger_fid = use_signal(format!("{group_id}__ao_trigger_fid").as_str(), || {
        focus_id.clone()
    });
    let ao_opts = AnchoredOverlayContentOpts {
        placement: Some(Placement::trigger(
            focus_id.clone(),
            expand_placement,
            expand_cover_gap_for_group(face_size),
            FloatingStrategy::Fixed,
        )),
        kind: OverlayKind::Popover,
        motion: FloatingMotionKind::Expand,
        mode: OverlayMode::Stacked,
        // Content-sized. `"auto"` must not be written as `rui_floating_w` (that attr is
        // an explicit px; parse fallback is trigger width → overlay locks to ~2 faces).
        width: Some("auto".into()),
        ..Default::default()
    };
    // Keep the 0×0 reactive anchor *inside* the group shell (BubbleReactions), not as a
    // fragment sibling / extra portal. Hoisting the placeholder broke expand rebuild and
    // made the overlay origin the group box instead of the faces.
    let placeholder = with_child_owner(|| {
        provide_anchored_overlay_context(expanded, group_id.clone(), trigger_fid);
        anchored_overlay_content(ao_opts, move || {
            let (faces, fid, placement, expand_class, visible_n, has_count) =
                AVATAR_GROUPS.with(|m| {
                    let map = m.borrow();
                    match map.get(&portal_id) {
                        Some(e) => (
                            e.faces.clone(),
                            e.focus_id.clone(),
                            e.placement.get(),
                            e.expand_class.clone(),
                            e.visible_n,
                            e.has_count,
                        ),
                        None => (
                            Vec::new(),
                            String::new(),
                            PopupPlacement::BottomStart,
                            String::new(),
                            0,
                            false,
                        ),
                    }
                });
            let trigger_h = faces
                .first()
                .map(|f| IntoF32::into_f32(f.size.box_dim()))
                .unwrap_or(32.0);
            let panel = build_avatar_group_expand_panel(
                &portal_id,
                &fid,
                placement,
                &faces,
                Some(expand_class.as_str()).filter(|s| !s.is_empty()),
                trigger_h,
                visible_n,
                has_count,
            );
            let hover_id = portal_id.clone();
            interactive(panel)
                .focus_id(format!("{hover_id}__panel"))
                .on_hover(move |h| note_avatar_group_hover(&hover_id, true, h))
                .into()
        })
    });

    let hover_id = group_id.clone();
    let shell: Element = vstack()
        .w("auto")
        .flex_grow(0.0)
        .flex_shrink(0.0)
        .align_self_kw("flex-start")
        .overflow("visible")
        .gap(0.0)
        .child(row)
        .child(placeholder)
        .into();
    let mut row_el: Element = interactive(shell)
        .on_hover(move |h| note_avatar_group_hover(&hover_id, false, h))
        .into();
    row_el.focus_id = Some(focus_id.clone());
    row_el.set_attr(ATTR_SLOT, SLOT_GROUP);
    row_el.set_attr(ATTR_GROUP_ID, &group_id);
    if max_visible != usize::MAX {
        row_el.set_attr(ATTR_MAX_VISIBLE, max_visible.to_string());
    }
    row_el.set_attr(
        "data-state",
        if expanded.peek() { "open" } else { "closed" },
    );
    let id_cleanup = group_id.clone();
    row_el.on_unmount(move || unregister_avatar_group(&id_cleanup))
}

fn build_avatar_group_count(
    size: AvatarSize,
    children: Vec<Element>,
    base: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
) -> Element {
    let dim = size.box_dim();
    let muted = parse_color("muted");
    let fg = parse_text_color("muted-foreground");
    // Same shell as `build_avatar_root`: overflow-hidden circle + 2px background ring.
    // Face fill lives on an inner layer (like AvatarFallback) so the ring sits *outside*
    // the muted fill and reads as the same inner gray stroke as "C".
    let mut el: Element = r#box().into();
    el = el
        .relative()
        .flex_shrink(0.0)
        .overflow("hidden")
        .w(dim)
        .h(dim)
        .min_w(dim)
        .min_h(dim)
        .max_w(dim)
        .max_h(dim);
    apply_root_shape(&mut el, AvatarShape::Circle);
    el.set_attr(ATTR_SLOT, SLOT_GROUP_COUNT);
    el.set_attr(ATTR_SIZE, size.as_str());
    el = apply_group_ring(el);

    let mut face = apply_layer_shape(fill_avatar_layer(r#box().into()), AvatarShape::Circle);
    face = face
        .bg(muted)
        .display_flex()
        .items_center()
        .justify_center();
    face.set_attr("data-avatar-count-face", "1");

    let mut overflow_faces = Vec::new();
    for mut c in children {
        if c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_AVATAR) {
            c = c
                .absolute()
                .left(0.0)
                .top(0.0)
                .w(0.0)
                .h(0.0)
                .min_w(0.0)
                .min_h(0.0)
                .max_w(0.0)
                .max_h(0.0)
                .opacity(0.0)
                .overflow("hidden")
                .pointer_events_box_none();
            overflow_faces.push(c);
            continue;
        }
        if c.text.is_some() {
            let node = build_text_node(
                c.text.take().unwrap_or_default(),
                TextNodeConfig {
                    size_px: size.fallback_text_px(),
                    weight: 500,
                    color: fg,
                    ..Default::default()
                },
            );
            face = face.child(node);
        } else {
            if c.text_style.font_family.as_deref() == Some("lucide")
                && c.text_style.size_resolved() <= 1.0
            {
                c.text_style.size = Some(IntoF32::into_f32("1rem"));
            }
            face = face.child(c);
        }
    }
    el = el.child(face);
    for c in overflow_faces {
        el = el.child(c);
    }
    if let Some(b) = base {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

// ---------------------------------------------------------------------------
// AvatarFallback builder (common to RSX / AvatarBuilder)
// ---------------------------------------------------------------------------

/// Fallback 内容构建器：文本 · Lucide Icon · Image · 任意子节点。
pub struct AvatarFallbackBuilder {
    children: Vec<Element>,
    image_src: Option<String>,
    class: String,
    base: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
}

/// Fallback 构建入口（须在 [`avatar_scope`] / `<Avatar>` 内求值为 Element）。
pub fn avatar_fallback() -> AvatarFallbackBuilder {
    AvatarFallbackBuilder {
        children: Vec::new(),
        image_src: None,
        class: String::new(),
        base: None,
    }
}

impl AvatarFallbackBuilder {
    pub fn text(mut self, content: impl Into<String>) -> Self {
        self.children.push(Element::new().text(content.into()));
        self
    }

    pub fn icon(mut self, name: impl AsRef<str>) -> Self {
        let name = name.as_ref();
        let glyph = lucide_glyph(name).unwrap_or_else(|| "\u{ea0e}".to_string());
        self.children.push(build_named_icon_text_node(name, glyph));
        self
    }

    pub fn image(mut self, src: impl Into<String>) -> Self {
        self.image_src = Some(src.into());
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        for c in children {
            self = self.child(c);
        }
        self
    }

    pub fn class(mut self, c: impl Into<String>) -> Self {
        self.class = c.into();
        self
    }

    /// Same [`BaseComponentBuilder`] surface as RSX props (`w` / `h` / `text_size` / …).
    pub fn style(
        mut self,
        f: impl FnOnce(BaseComponentBuilder) -> BaseComponentBuilder + 'static,
    ) -> Self {
        self.base = Some(Box::new(f));
        self
    }

    pub fn into_element(self) -> Element {
        let mut children = self.children;
        if let Some(src) = self.image_src {
            children.push(build_avatar_texture_layer(&src, None));
        }
        let mut el = build_avatar_fallback(children, self.base);
        if !self.class.trim().is_empty() {
            el = with_class_list(el, self.class.trim());
        }
        el
    }
}

impl From<AvatarFallbackBuilder> for Element {
    fn from(builder: AvatarFallbackBuilder) -> Self {
        builder.into_element()
    }
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

pub struct AvatarImageBuilder {
    id: String,
    src: String,
    alt: String,
    chrome: SlotChrome,
}

pub fn avatar_image() -> AvatarImageBuilder {
    AvatarImageBuilder {
        id: String::new(),
        src: String::new(),
        alt: String::new(),
        chrome: SlotChrome::default(),
    }
}

impl AvatarImageBuilder {
    impl_slot_chrome_methods!();

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn src(mut self, src: impl Into<String>) -> Self {
        self.src = src.into();
        self
    }

    pub fn alt(mut self, alt: impl Into<String>) -> Self {
        self.alt = alt.into();
        self
    }
}

impl From<AvatarImageBuilder> for Element {
    fn from(builder: AvatarImageBuilder) -> Self {
        let el = AvatarImage(AvatarImageProps {
            id: builder.id,
            src: Some(builder.src),
            alt: Some(builder.alt),
            base: None,
            children: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct AvatarBuilder {
    id: String,
    size: AvatarSize,
    shape: AvatarShape,
    chrome: SlotChrome,
    body: Option<Box<dyn FnOnce() -> Element>>,
}

/// Builder entry (ADR-9 surface B).
pub fn avatar() -> AvatarBuilder {
    AvatarBuilder {
        id: String::new(),
        size: AvatarSize::Default,
        shape: AvatarShape::Circle,
        chrome: SlotChrome::default(),
        body: None,
    }
}

impl AvatarBuilder {
    impl_slot_chrome_methods!();

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn size(mut self, s: impl Into<AvatarSize>) -> Self {
        self.size = s.into();
        self
    }

    pub fn shape(mut self, s: impl Into<AvatarShape>) -> Self {
        self.shape = s.into();
        self
    }

    /// Composite surface: Build Image / Fallback / Badge within scope.
    pub fn child(mut self, body: impl FnOnce() -> Element + 'static) -> Self {
        self.body = Some(Box::new(body));
        self
    }

    fn push_fallback(mut self, fb: AvatarFallbackBuilder) -> Self {
        let prev = self.body.take();
        self.body = Some(Box::new(move || {
            let mut kids = Vec::new();
            if let Some(prev) = prev {
                kids.extend(take_packed(prev()));
            }
            kids.push(fb.into_element());
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            frag.children(kids)
        }));
        self
    }

    /// 便捷：文本回退（缩写）。
    pub fn fallback(self, text: impl Into<String>) -> Self {
        self.push_fallback(avatar_fallback().text(text))
    }

    /// Convenience: Lucide icon fallback.
    pub fn fallback_icon(self, name: impl AsRef<str>) -> Self {
        self.push_fallback(avatar_fallback().icon(name))
    }

    /// Convenient: images filled with Fallback layers (not [`AvatarImage`] loading pipeline).
    pub fn fallback_image(self, src: impl Into<String>) -> Self {
        self.push_fallback(avatar_fallback().image(src))
    }

    /// Composite fallback: continue `.text` / `.icon` / `.child` on [`avatar_fallback`].
    pub fn fallback_with(
        self,
        build: impl FnOnce(AvatarFallbackBuilder) -> AvatarFallbackBuilder + 'static,
    ) -> Self {
        self.push_fallback(build(avatar_fallback()))
    }

    /// Convenience: only images (can be linked to `.fallback` / `.fallback_icon`, etc.).
    pub fn image(self, src: impl Into<String>) -> Self {
        let src = src.into();
        self.image_with(move |img| img.src(src))
    }

    pub fn image_with(
        mut self,
        f: impl FnOnce(AvatarImageBuilder) -> AvatarImageBuilder + 'static,
    ) -> Self {
        self.body = Some(Box::new(move || f(avatar_image()).into()));
        self
    }
}

impl From<AvatarBuilder> for Element {
    fn from(builder: AvatarBuilder) -> Self {
        let class = if builder.chrome.class.trim().is_empty() {
            None
        } else {
            Some(builder.chrome.class.clone())
        };
        let style = builder.chrome.style.clone();
        let body = builder.body;
        let mut el = avatar_scope(builder.id, builder.size, builder.shape, class, move || {
            body.map(|b| b()).unwrap_or_else(Element::new)
        });
        if let Some(style) = style {
            el = style(BaseComponentBuilder::new(el)).el;
        }
        el
    }
}

// ---------------------------------------------------------------------------
// RSX components
// ---------------------------------------------------------------------------

/// ShadCN `Avatar` root container.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Avatar(
    id: String,
    size: AvatarSize,
    shape: AvatarShape,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let class = None;
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = avatar_scope(id, size, shape, class, move || {
        let mut frag = Element::new();
        if kids.is_empty() {
            frag
        } else {
            frag.set_attr("rui_fragment", "1");
            frag.children(kids)
        }
    });
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

/// Avatar image layer (`aspect-square size-full object-cover` mental).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AvatarImage(
    id: String,
    src: String,
    alt: String,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let _ = id;
    let mut el = build_avatar_image(&src, &alt, base_builder);
    if let Some(c) = children {
        el = el.child(c);
    }
    el
}

/// Fallback when the image is unavailable (`bg-muted` + centered; supports text / Icon / Image / any child node).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AvatarFallback(
    id: String,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let _ = id;
    let mut builder = avatar_fallback();
    if let Some(c) = children {
        for kid in take_packed(c) {
            builder = builder.child(kid);
        }
    }
    let mut el = builder.into_element();
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

/// 右下角状态点 / 图标徽标。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AvatarBadge(
    id: String,
    variant: BadgeVariant,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let _ = id;
    let kids = children.map(take_packed).unwrap_or_default();
    build_avatar_badge(variant, kids, base_builder)
}

/// Overlapping rows of multiple heads (`-space-x-2` + `ring-background`).
///
/// - `expand_placement`: floating expand side for `+N` (default `bottom-start`, in-place cover; faces align to collapsed faces).
/// - `expand_class`: utility class for the expand container (default = popover / border / shadow tokens).
/// - `max_visible`: 折叠前最多展示的头像数；未设置时保留手动 `AvatarGroupCount` 布局，但仍会在宽度不足时自动折叠。
/// - Hover/click `AvatarGroupCount` opens a portal covering the group: all faces lay out as
///   independent chips (equal gap, nowrap) inside popover chrome (content width, equal padding).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AvatarGroup(
    expand_placement: Option<String>,
    expand_class: Option<String>,
    max_visible: Option<i32>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let placement = expand_placement
        .as_deref()
        .map(expand_placement_from_attr)
        .unwrap_or(PopupPlacement::BottomStart);
    let max_visible = max_visible.map(|n| n.max(1) as usize).unwrap_or(usize::MAX);
    build_avatar_group(kids, placement, expand_class, max_visible)
}

/// "+N" counting capsules in the group.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AvatarGroupCount(
    size: AvatarSize,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    build_avatar_group_count(size, kids, base_builder)
}

/// Solid-style in-place bind for `<AvatarBadge variant={sig.get()}>`.
pub fn apply_bound_avatar_badge_variant(
    el: &mut Element,
    variant: impl AsRef<str>,
) -> LayoutStatus {
    let status = apply_badge_variant_paint(el, variant);
    apply_avatar_badge_ring(el);
    status
}

/// Solid-style in-place bind for `<Avatar size={sig.get()}>`.
pub fn apply_bound_avatar_size(el: &mut Element, size: impl AsRef<str>) -> LayoutStatus {
    let next = AvatarSize::parse(size.as_ref());
    let prev = el
        .attrs
        .get(ATTR_SIZE)
        .map(|s| AvatarSize::parse(s))
        .unwrap_or(AvatarSize::Default);
    if prev == next {
        return LayoutStatus::Clean;
    }
    let shape = el
        .attrs
        .get(ATTR_SHAPE)
        .map(|s| AvatarShape::parse(s))
        .unwrap_or(AvatarShape::Circle);
    let status = apply_root_size(el, next);
    restyle_avatar_subtree(el, AvatarCtx { size: next, shape });
    status.merge(LayoutStatus::StyleChanged)
}

/// Solid-style in-place bind for `<Avatar shape={sig.get()}>`.
pub fn apply_bound_avatar_shape(el: &mut Element, shape: impl AsRef<str>) -> LayoutStatus {
    let next = AvatarShape::parse(shape.as_ref());
    let prev = el
        .attrs
        .get(ATTR_SHAPE)
        .map(|s| AvatarShape::parse(s))
        .unwrap_or(AvatarShape::Circle);
    if prev == next {
        return LayoutStatus::Clean;
    }
    let size = el
        .attrs
        .get(ATTR_SIZE)
        .map(|s| AvatarSize::parse(s))
        .unwrap_or(AvatarSize::Default);
    apply_root_shape(el, next);
    restyle_avatar_subtree(el, AvatarCtx { size, shape: next });
    LayoutStatus::StyleChanged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avatar_size_parse_and_dims() {
        assert_eq!(AvatarSize::parse("SM"), AvatarSize::Sm);
        assert_eq!(AvatarSize::Sm.box_dim(), "1.5rem");
        assert_eq!(AvatarSize::Default.box_dim(), "2rem");
        assert_eq!(AvatarSize::Lg.box_dim(), "2.5rem");
    }

    #[test]
    fn builder_sets_slot_and_size() {
        let el: Element = avatar().size("lg").fallback("CN").into();
        assert_eq!(
            el.attrs.get(ATTR_SLOT).map(|s| s.as_str()),
            Some(SLOT_AVATAR)
        );
        assert_eq!(el.attrs.get(ATTR_SIZE).map(|s| s.as_str()), Some("lg"));
    }

    #[test]
    fn avatar_group_count_slot() {
        let el = AvatarGroupCount(AvatarGroupCountProps {
            size: Some(AvatarSize::Default),
            children: Some(Element::new().text("+3")),
            base: None,
            ..Default::default()
        });
        assert_eq!(
            el.attrs.get(ATTR_SLOT).map(|s| s.as_str()),
            Some(SLOT_GROUP_COUNT)
        );
        assert!(
            (el.style.border_width - 2.0).abs() < 0.01,
            "+N shell must use the same 2px group ring as faces, got {}",
            el.style.border_width
        );
        let face = el
            .children
            .iter()
            .find(|c| c.attrs.get("data-avatar-count-face").map(|s| s.as_str()) == Some("1"))
            .expect("count inner face");
        let muted = parse_color("muted");
        assert!(
            (face.style.bg_color[0] - muted[0]).abs() < 0.02
                && (face.style.bg_color[1] - muted[1]).abs() < 0.02,
            "inner face must carry muted fill so the ring reads as an inner stroke"
        );
        assert!(
            el.style.bg_color[3] < 0.01,
            "count shell must not fill (fill belongs on the inner face, like AvatarFallback)"
        );
    }

    fn collect_texts(el: &Element, out: &mut Vec<String>) {
        if let Some(t) = el.text.as_ref().filter(|s| !s.is_empty()) {
            out.push(t.clone());
        }
        for c in &el.children {
            collect_texts(c, out);
        }
    }

    #[test]
    fn avatar_group_count_keeps_label_when_overflow_faces_nested() {
        let el = AvatarGroupCount(AvatarGroupCountProps {
            size: Some(AvatarSize::Default),
            children: Some({
                let mut f = Element::new();
                f.set_attr("rui_fragment", "1");
                f = f.children(vec![
                    Element::new().text("+3"),
                    avatar_with_fallback("D"),
                    avatar_with_fallback("E"),
                    avatar_with_fallback("F"),
                ]);
                f
            }),
            ..Default::default()
        });
        let mut texts = Vec::new();
        collect_texts(&el, &mut texts);
        assert!(
            texts.iter().any(|t| t.contains("+3")),
            "+N label must stay on the count capsule, got {texts:?}"
        );
        let nested: Vec<&Element> = el
            .children
            .iter()
            .filter(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_AVATAR))
            .collect();
        assert_eq!(nested.len(), 3, "overflow faces stay as count children");
        for n in nested {
            assert_eq!(
                n.style.position,
                taffy::style::Position::Absolute,
                "overflow faces must be out of the count flex flow"
            );
            assert_eq!(
                crate::layout::dim_length_px(n.style.min_size.width),
                Some(0.0)
            );
        }
    }

    fn avatar_with_fallback(label: &str) -> Element {
        avatar().fallback(label).into()
    }

    #[test]
    fn avatar_group_expand_panel_covers_down_and_wraps_faces() {
        use crate::core::reactivity::create_root;
        use crate::utils::popup::{
            ATTR_FLOATING, ATTR_FLOATING_EXPAND_CLIP_X, ATTR_FLOATING_EXPAND_FROM_H,
            ATTR_FLOATING_EXPAND_FROM_W, ATTR_FLOATING_GAP, ATTR_FLOATING_MATCH_TRIGGER,
            ATTR_FLOATING_STRATEGY, ATTR_FLOATING_W,
        };

        let (_owner, group) = create_root(|| {
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            frag = frag.children(vec![
                avatar_with_fallback("A"),
                avatar_with_fallback("B"),
                AvatarGroupCount(AvatarGroupCountProps {
                    children: Some({
                        let mut f = Element::new();
                        f.set_attr("rui_fragment", "1");
                        f = f.children(vec![Element::new().text("+1"), avatar_with_fallback("C")]);
                        f
                    }),
                    ..Default::default()
                }),
            ]);
            AvatarGroup(AvatarGroupProps {
                expand_placement: Some("bottom-start".into()),
                children: Some(frag),
                ..Default::default()
            })
        });

        fn find_group_slot<'a>(el: &'a Element) -> Option<&'a Element> {
            if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_GROUP) {
                return Some(el);
            }
            el.children.iter().find_map(find_group_slot)
        }

        assert!(
            find_group_slot(&group).is_some(),
            "avatar-group slot must live on the row trigger"
        );

        let group_id = AVATAR_GROUPS.with(|m| {
            m.borrow()
                .keys()
                .next()
                .cloned()
                .expect("avatar group registry")
        });
        let faces = AVATAR_GROUPS.with(|m| {
            let map = m.borrow();
            let e = map.get(&group_id).expect("entry");
            assert_eq!(e.faces.len(), 3, "visible A/B + nested C");
            assert_eq!(e.visible_n, 2);
            assert_eq!(e.placement.get(), PopupPlacement::BottomStart);
            e.faces.clone()
        });

        let panel = build_avatar_group_expand_panel(
            &group_id,
            "fid",
            PopupPlacement::BottomStart,
            &faces,
            None,
            32.0,
            2,
            true,
        );
        assert_eq!(
            panel
                .attrs
                .get("data-avatar-group-overlay")
                .map(|s| s.as_str()),
            Some("1")
        );
        assert_eq!(
            panel.attrs.get(ATTR_FLOATING).map(|s| s.as_str()),
            Some(group_id.as_str())
        );
        assert_eq!(
            panel.attrs.get(ATTR_FLOATING_STRATEGY).map(|s| s.as_str()),
            Some("fixed")
        );
        assert_eq!(
            panel
                .attrs
                .get(ATTR_FLOATING_MATCH_TRIGGER)
                .map(|s| s.as_str()),
            Some("0"),
            "expand width is auto, not stretched to the trigger"
        );
        let expected_w = super::expand_content_width(32.0, 3);
        let floating_w = panel
            .attrs
            .get(ATTR_FLOATING_W)
            .and_then(|s| s.trim().trim_end_matches("px").trim().parse::<f32>().ok());
        assert_eq!(
            floating_w,
            Some(expected_w),
            "expand panel must pin rui_floating_w to the gap-row content width"
        );
        assert_eq!(
            panel
                .attrs
                .get(ATTR_FLOATING_EXPAND_FROM_H)
                .and_then(|s| s.parse::<f32>().ok()),
            Some(32.0),
            "expand motion must start from collapsed face height, not 0"
        );
        let expected_from_w = super::expand_from_width(32.0, 2, true);
        assert_eq!(
            panel
                .attrs
                .get(ATTR_FLOATING_EXPAND_FROM_W)
                .and_then(|s| s.parse::<f32>().ok()),
            Some(expected_from_w),
            "expand motion must clip from collapsed row width so extra faces do not pop in"
        );
        assert_eq!(
            panel
                .attrs
                .get(ATTR_FLOATING_EXPAND_CLIP_X)
                .map(|s| s.as_str()),
            Some("1"),
            "avatar extra faces clip on the x axis (nowrap + max-width lerp)"
        );
        let inset = super::expand_chrome_inset();
        assert!(
            (panel.style.translate[0] + inset).abs() < 0.5
                && (panel.style.translate[1] + inset).abs() < 0.5,
            "cover panel must shift by chrome inset so faces align with faces, got translate={:?}",
            panel.style.translate
        );
        let gap: f32 = panel
            .attrs
            .get(ATTR_FLOATING_GAP)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);
        assert!(gap < 0.0, "in-place cover uses negative gap, got {gap}");
        assert_eq!(panel.children.len(), 3, "expand chrome wraps all faces");
        let gap_px = IntoF32::into_f32(tokens::SPACE_1);
        let panel_gap = crate::layout::lp_length_px(panel.style.gap.width).unwrap_or(0.0);
        assert!(
            (panel_gap - gap_px).abs() < 0.5,
            "expand panel uses uniform gap between all faces, got {panel_gap}"
        );
        for (i, child) in panel.children.iter().enumerate().skip(1) {
            let ml = crate::layout::lpa_length_px(child.style.margin.left).unwrap_or(0.0);
            assert!(
                ml.abs() < 0.5,
                "expand faces must not keep collapsed overlap margin (child {i}), got {ml}"
            );
        }
        let pad_l = crate::layout::lp_length_px(panel.style.padding.left).unwrap_or(0.0);
        let pad_r = crate::layout::lp_length_px(panel.style.padding.right).unwrap_or(0.0);
        let pad_t = crate::layout::lp_length_px(panel.style.padding.top).unwrap_or(0.0);
        let pad_b = crate::layout::lp_length_px(panel.style.padding.bottom).unwrap_or(0.0);
        assert!(
            (pad_l - pad_r).abs() < 0.5 && (pad_t - pad_b).abs() < 0.5 && pad_l > 0.0,
            "expand chrome uses equal padding so faces sit centered, got L{pad_l} R{pad_r} T{pad_t} B{pad_b}"
        );
        assert_eq!(
            panel.style.justify_content,
            Some(taffy::style::JustifyContent::FLEX_START)
        );
        assert_eq!(
            panel.style.flex_wrap,
            taffy::style::FlexWrap::NoWrap,
            "cover panel is a single gap row; wrap would stretch to the portal parent"
        );
    }

    #[test]
    fn avatar_group_expand_panel_hugs_face_row_in_wide_parent() {
        use crate::core::reactivity::create_root;
        use crate::layout::layout_tree::LayoutTree;

        let (_owner, faces) = create_root(|| {
            ["A", "B", "C", "D", "E", "F"]
                .into_iter()
                .map(|label| extract_face_spec(&avatar_with_fallback(label)).expect("face"))
                .collect::<Vec<_>>()
        });
        let panel = build_avatar_group_expand_panel(
            "grp-hug",
            "fid",
            PopupPlacement::BottomStart,
            &faces,
            None,
            32.0,
            3,
            true,
        );
        let mut host = Element::new().relative().w(400.0).h(200.0).child(panel);
        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut host, 400.0, 200.0, [-1.0, -1.0]);
        let w = host.children[0].layout_bounds[2];
        let face = IntoF32::into_f32(AvatarSize::Default.box_dim());
        let gap = IntoF32::into_f32(tokens::SPACE_1);
        let content = 6.0 * face + 5.0 * gap + 2.0 * super::expand_chrome_inset();
        assert!(
            (w - content).abs() < 12.0,
            "expand panel must hug the 6-face row ({content:.1}px), not stretch to parent 400, got {w:.1}"
        );
    }

    #[test]
    fn avatar_group_expand_keeps_image_src_and_fallback_class() {
        use crate::core::reactivity::create_root;

        const SRC: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==";

        let (_owner, (image_spec, class_spec, img_el, class_el)) = create_root(|| {
            let with_img: Element = avatar().image(SRC).fallback("A").into();
            let with_class: Element = avatar().class("bg-red-500").fallback("B").into();
            let image_spec = extract_face_spec(&with_img).expect("image face");
            let class_spec = extract_face_spec(&with_class).expect("class face");
            // Same owner as the collapsed faces: expand rebuild must still seed
            // image_texture even when bind_prop skips a duplicate key.
            let img_el = rebuild_face(&image_spec);
            let class_el = rebuild_face(&class_spec);
            (image_spec, class_spec, img_el, class_el)
        });
        assert_eq!(
            image_spec.src.as_deref(),
            Some(SRC),
            "expand snapshot must keep AvatarImage src so A/B do not fall back to muted"
        );
        assert!(
            class_spec
                .class
                .as_deref()
                .is_some_and(|c| c.split_whitespace().any(|t| t == "bg-red-500")),
            "expand snapshot must keep author class colors, got {:?}",
            class_spec.class
        );

        let mut rebuilt_src = None;
        let mut rebuilt_tex = false;
        crate::core::tree::walk_elements(&img_el, |n| {
            if rebuilt_src.is_none() {
                if let Some(s) = n.attrs.get(ATTR_AVATAR_SRC).filter(|s| !s.is_empty()) {
                    rebuilt_src = Some(s.clone());
                }
            }
            if n.image_texture.as_ref().is_some_and(|k| !k.is_empty()) {
                rebuilt_tex = true;
            }
        });
        assert_eq!(
            rebuilt_src.as_deref(),
            Some(SRC),
            "rebuild_face must emit AvatarImage with the original src"
        );
        assert!(
            rebuilt_tex,
            "expand rebuild must seed image_texture so colored faces do not paint as muted fallback"
        );
        let class_attr = class_el
            .attrs
            .get("rui_utility_class")
            .map(|s| s.as_str())
            .unwrap_or("");
        assert!(
            class_attr.split_whitespace().any(|t| t == "bg-red-500"),
            "rebuild_face must keep bg-red-500, got {class_attr:?}"
        );
    }

    #[test]
    fn avatar_group_open_does_not_reborrow_registry() {
        use crate::core::reactivity::create_root;

        let (_owner, _group) = create_root(|| {
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            frag = frag.children(vec![
                avatar_with_fallback("A"),
                AvatarGroupCount(AvatarGroupCountProps {
                    children: Some(Element::new().text("+1")),
                    ..Default::default()
                }),
            ]);
            AvatarGroup(AvatarGroupProps {
                children: Some(frag),
                ..Default::default()
            })
        });
        let group_id = AVATAR_GROUPS.with(|m| {
            m.borrow()
                .keys()
                .next()
                .cloned()
                .expect("avatar group registry")
        });
        // Synchronous signal writes used to borrow_mut the registry and panic on re-enter.
        note_avatar_group_hover(&group_id, false, true);
        open_avatar_group_expand(&group_id);
        AVATAR_GROUPS.with(|m| {
            assert!(m.borrow().get(&group_id).unwrap().expanded.peek());
        });
    }

    #[test]
    fn avatar_group_expand_anchor_lives_inside_trigger_shell() {
        use crate::app::reactive::REACTIVE_PLACEHOLDER_HASH;
        use crate::core::reactivity::create_root;

        fn find_group<'a>(el: &'a Element) -> Option<&'a Element> {
            if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_GROUP) {
                return Some(el);
            }
            el.children.iter().find_map(find_group)
        }

        fn find_placeholder<'a>(el: &'a Element) -> bool {
            el.template_hash
                .as_deref()
                .is_some_and(|h| h == REACTIVE_PLACEHOLDER_HASH)
                || el.children.iter().any(find_placeholder)
        }

        let (_owner, root) = create_root(|| {
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            frag = frag.children(vec![
                avatar_with_fallback("A"),
                AvatarGroupCount(AvatarGroupCountProps {
                    children: Some(Element::new().text("+1")),
                    ..Default::default()
                }),
            ]);
            AvatarGroup(AvatarGroupProps {
                children: Some(frag),
                ..Default::default()
            })
        });
        let group = find_group(&root).expect("avatar group row");
        assert!(
            group.focus_id.is_some(),
            "row trigger carries focus_id for floating placement"
        );
        assert!(
            find_placeholder(group),
            "reactive expand anchor must stay inside the group shell like BubbleReactions"
        );
        assert_ne!(
            group.attrs.get("rui_portal").map(String::as_str),
            Some("1"),
            "group shell itself is not a portal; only the expand panel hoists"
        );
    }

    #[test]
    fn avatar_group_scroll_dismisses_expand() {
        use crate::core::reactivity::create_root;

        let (_owner, _group) = create_root(|| {
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            frag = frag.children(vec![
                avatar_with_fallback("A"),
                AvatarGroupCount(AvatarGroupCountProps {
                    children: Some(Element::new().text("+1")),
                    ..Default::default()
                }),
            ]);
            AvatarGroup(AvatarGroupProps {
                children: Some(frag),
                ..Default::default()
            })
        });
        let group_id = AVATAR_GROUPS.with(|m| {
            m.borrow()
                .keys()
                .next()
                .cloned()
                .expect("avatar group registry")
        });
        open_avatar_group_expand(&group_id);
        assert!(super::dismiss_avatar_group_expands_on_scroll());
        AVATAR_GROUPS.with(|m| {
            assert!(!m.borrow().get(&group_id).unwrap().expanded.peek());
        });
    }

    #[test]
    fn avatar_group_residual_scroll_does_not_flicker_expand() {
        use crate::core::reactivity::create_root;

        let _ = super::set_avatar_group_expand_hover_frozen(false);
        let (_owner, _group) = create_root(|| {
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            frag = frag.children(vec![
                avatar_with_fallback("A"),
                AvatarGroupCount(AvatarGroupCountProps {
                    children: Some(Element::new().text("+1")),
                    ..Default::default()
                }),
            ]);
            AvatarGroup(AvatarGroupProps {
                children: Some(frag),
                ..Default::default()
            })
        });
        let group_id = AVATAR_GROUPS.with(|m| {
            m.borrow()
                .keys()
                .next()
                .cloned()
                .expect("avatar group registry")
        });
        note_avatar_group_hover(&group_id, false, true);
        open_avatar_group_expand(&group_id);
        AVATAR_GROUPS.with(|m| {
            assert!(m.borrow().get(&group_id).unwrap().expanded.peek());
        });

        // Residual catch-up frames: close once, hover-enter must not reopen until thaw.
        assert!(!super::set_avatar_group_expand_hover_frozen(true));
        for _ in 0..4 {
            super::dismiss_avatar_group_expands_on_scroll();
            note_avatar_group_hover(&group_id, false, true);
            open_avatar_group_expand(&group_id);
            let now_open =
                AVATAR_GROUPS.with(|m| m.borrow().get(&group_id).unwrap().expanded.peek());
            assert!(
                !now_open,
                "hover must not reopen expand while smooth-scroll is catching up"
            );
        }

        assert!(super::set_avatar_group_expand_hover_frozen(false));
        AVATAR_GROUPS.with(|m| {
            assert!(
                m.borrow().get(&group_id).unwrap().expanded.peek(),
                "pointer still on +N after settle: open once"
            );
        });
        let _ = super::set_avatar_group_expand_hover_frozen(false);
    }

    #[test]
    fn avatar_group_hover_opens_after_idle_unfreeze() {
        use crate::core::reactivity::create_root;

        let _ = super::set_avatar_group_expand_hover_frozen(false);
        let (_owner, _group) = create_root(|| {
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            frag = frag.children(vec![
                avatar_with_fallback("A"),
                AvatarGroupCount(AvatarGroupCountProps {
                    children: Some(Element::new().text("+1")),
                    ..Default::default()
                }),
            ]);
            AvatarGroup(AvatarGroupProps {
                children: Some(frag),
                ..Default::default()
            })
        });
        let group_id = AVATAR_GROUPS.with(|m| {
            m.borrow()
                .keys()
                .next()
                .cloned()
                .expect("avatar group registry")
        });
        // Residual freeze must not stay on: idle hover/click still open.
        assert!(!super::set_avatar_group_expand_hover_frozen(true));
        assert!(!super::set_avatar_group_expand_hover_frozen(false));
        note_avatar_group_hover(&group_id, false, true);
        open_avatar_group_expand(&group_id);
        AVATAR_GROUPS.with(|m| {
            assert!(
                m.borrow().get(&group_id).unwrap().expanded.peek(),
                "+N hover must open once freeze is cleared"
            );
        });
        let _ = super::set_avatar_group_expand_hover_frozen(false);
    }

    #[test]
    fn avatar_group_width_collapses_visible_faces() {
        use crate::core::reactivity::create_root;

        let (_owner, group) = create_root(|| {
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            frag = frag.children(vec![
                avatar_with_fallback("A"),
                avatar_with_fallback("B"),
                avatar_with_fallback("C"),
                AvatarGroupCount(AvatarGroupCountProps {
                    children: Some({
                        let mut f = Element::new();
                        f.set_attr("rui_fragment", "1");
                        f = f.children(vec![
                            Element::new().text("+3"),
                            avatar_with_fallback("D"),
                            avatar_with_fallback("E"),
                            avatar_with_fallback("F"),
                        ]);
                        f
                    }),
                    ..Default::default()
                }),
            ]);
            AvatarGroup(AvatarGroupProps {
                children: Some(frag),
                ..Default::default()
            })
        });
        let group_id = AVATAR_GROUPS.with(|m| {
            m.borrow()
                .keys()
                .next()
                .cloned()
                .expect("avatar group registry")
        });
        let mut host = Element::new().w(72.0).child(group);
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        let _ = tree.build_and_layout(&mut host, 72.0, 120.0, [-1.0, -1.0]);
        let mut impact = LayoutStatus::Clean;
        sync_avatar_group_subtree(&mut host, &mut tree, &mut impact, 72.0);
        let rendered = AVATAR_GROUPS.with(|m| {
            m.borrow()
                .get(&group_id)
                .expect("entry")
                .rendered_visible
                .get()
        });
        assert!(
            rendered < 3,
            "narrow column must fold visible avatars into +N, got {rendered}"
        );
        let visible_faces = host
            .children
            .iter()
            .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_GROUP))
            .and_then(find_avatar_group_row)
            .map(|row| {
                row.children
                    .iter()
                    .filter(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_AVATAR))
                    .count()
            })
            .expect("group row");
        assert_eq!(
            visible_faces, rendered,
            "collapsed row must rebuild to match rendered visible count"
        );
    }

    #[test]
    fn avatar_group_max_visible_auto_splits_faces() {
        use crate::core::reactivity::create_root;

        let (_owner, group) = create_root(|| {
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            frag = frag.children(vec![
                avatar_with_fallback("A"),
                avatar_with_fallback("B"),
                avatar_with_fallback("C"),
                avatar_with_fallback("D"),
            ]);
            AvatarGroup(AvatarGroupProps {
                max_visible: Some(2),
                children: Some(frag),
                ..Default::default()
            })
        });
        fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
            if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(slot) {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_slot(c, slot))
        }
        let faces: Vec<_> = find_slot(&group, SLOT_GROUP)
            .into_iter()
            .flat_map(|g| g.children.iter())
            .flat_map(|row| row.children.iter())
            .filter(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_AVATAR))
            .collect();
        assert_eq!(faces.len(), 2, "max_visible=2 keeps two faces in the row");
        assert!(
            find_slot(&group, SLOT_GROUP_COUNT).is_some(),
            "overflow faces must fold into +N"
        );
    }

    #[test]
    fn avatar_group_count_keeps_ring_after_interactive_wrap() {
        use crate::core::reactivity::create_root;
        use crate::core::tree::IntoElementList;

        fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
            if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(slot) {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_slot(c, slot))
        }

        let (_owner, root) = create_root(|| {
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            frag = frag.children(vec![
                avatar_with_fallback("A"),
                AvatarGroupCount(AvatarGroupCountProps {
                    children: Some(Element::new().text("+1")),
                    ..Default::default()
                }),
            ]);
            AvatarGroup(AvatarGroupProps {
                children: Some(frag),
                ..Default::default()
            })
        });
        let kids = root.into_element_list();
        let count = find_slot(&kids[0], SLOT_GROUP_COUNT).expect("count in group row");
        let face = find_slot(&kids[0], SLOT_AVATAR).expect("face in group row");
        assert!(
            (count.style.border_width - 2.0).abs() < 0.01,
            "wrapped +N must keep 2px ring, got {}",
            count.style.border_width
        );
        assert!(
            (face.style.border_width - count.style.border_width).abs() < 0.01,
            "face ring {} vs count ring {}",
            face.style.border_width,
            count.style.border_width
        );
    }

    #[test]
    fn avatar_image_load_failure_falls_back() {
        use crate::core::reactivity::create_root;

        fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
            if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(slot) {
                return Some(el);
            }
            for c in &el.children {
                if let Some(hit) = find_slot(c, slot) {
                    return Some(hit);
                }
            }
            None
        }

        let (_owner, el) = create_root(|| {
            avatar_scope("", AvatarSize::Default, AvatarShape::Circle, None, || {
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                frag.children(vec![
                    AvatarImage(AvatarImageProps {
                        id: String::new(),
                        src: Some("/nonexistent/rui-avatar-fail-test.png".into()),
                        alt: None,
                        base: None,
                        children: None,
                        ..Default::default()
                    }),
                    AvatarFallback(AvatarFallbackProps {
                        id: String::new(),
                        children: Some(Element::new().text("FB")),
                        base: None,
                        ..Default::default()
                    }),
                ])
            })
        });
        let img = find_slot(&el, SLOT_IMAGE).expect("avatar-image");
        assert_eq!(
            img.attrs.get(ATTR_LOAD_STATE).map(|s| s.as_str()),
            Some("failed")
        );
        assert!(img.image_texture.is_none());
        assert!((img.style.opacity - 0.0).abs() < f32::EPSILON);
        fn has_text(el: &Element, want: &str) -> bool {
            if el.text.as_deref() == Some(want) {
                return true;
            }
            el.children.iter().any(|c| has_text(c, want))
        }

        let fb = find_slot(&el, SLOT_FALLBACK).expect("avatar-fallback");
        assert!(has_text(fb, "FB"), "fallback caption must remain visible");
    }

    #[test]
    fn avatar_fallback_applies_shape_radii() {
        let el: Element = avatar().fallback("ER").into();
        let fb = el
            .children
            .iter()
            .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_FALLBACK))
            .expect("fallback");
        assert!(
            fb.style.radii.iter().any(|r| *r < 0.0),
            "circle fallback must use rounded_full radii sentinel"
        );
    }

    #[test]
    fn avatar_rounded_square_shape() {
        let el: Element = avatar().shape("rounded-square").fallback("RS").into();
        assert_eq!(
            el.attrs.get(ATTR_SHAPE).map(|s| s.as_str()),
            Some("rounded-square")
        );
        let fb = el
            .children
            .iter()
            .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_FALLBACK))
            .expect("fallback");
        assert!(
            fb.style.radii.iter().all(|r| *r > 0.0),
            "rounded-square fallback must use explicit corner radii"
        );
    }

    #[test]
    fn avatar_fallback_icon_in_scope() {
        fn find_lucide(el: &Element) -> bool {
            if el.text_style.font_family.as_deref() == Some("lucide") {
                return true;
            }
            el.children.iter().any(find_lucide)
        }

        let el: Element = avatar().fallback_icon("user").into();
        let fb = el
            .children
            .iter()
            .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_FALLBACK))
            .expect("fallback slot");
        assert!(find_lucide(fb), "fallback icon must render lucide glyph");
    }

    #[test]
    fn avatar_fallback_builder_image_fills_layer() {
        let el: Element = avatar()
            .fallback_image("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'><rect width='8' height='8' fill='red'/></svg>")
            .into();
        let fb = el
            .children
            .iter()
            .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_FALLBACK))
            .expect("fallback slot");
        fn find_load_layer(el: &Element) -> bool {
            if el.attrs.get(ATTR_LOAD_STATE).is_some() {
                return true;
            }
            el.children.iter().any(find_load_layer)
        }
        assert!(find_load_layer(fb), "fallback image layer must exist");
    }

    #[test]
    fn avatar_fallback_builder_class_and_style() {
        fn has_text(el: &Element, want: &str) -> bool {
            if el.text.as_deref() == Some(want) {
                return true;
            }
            el.children.iter().any(|c| has_text(c, want))
        }

        let el: Element = avatar()
            .fallback_with(|fb| {
                fb.class("bg-accent")
                    .style(|b| b.text_size(20.0))
                    .text("AB")
            })
            .into();
        let fb = el
            .children
            .iter()
            .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_FALLBACK))
            .expect("fallback slot");
        assert!(
            fb.class_set
                .as_ref()
                .is_some_and(|cs| cs.base.iter().any(|t| t.contains("bg-accent")))
                || fb
                    .attrs
                    .get("rui_utility_class")
                    .is_some_and(|c| c.contains("bg-accent"))
        );
        assert_eq!(fb.text_style.size_resolved(), 20.0);
        assert!(has_text(fb, "AB"), "fallback caption must render");
    }

    #[test]
    fn avatar_image_with_applies_chrome() {
        let el: Element = avatar()
            .image_with(|img| img.src("assets/user.png").class("opacity-80"))
            .into();
        let img = el
            .children
            .iter()
            .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_IMAGE))
            .expect("avatar-image");
        assert!(
            img.class_set
                .as_ref()
                .is_some_and(|cs| cs.base.iter().any(|t| t.contains("opacity-80")))
                || img
                    .attrs
                    .get("rui_utility_class")
                    .is_some_and(|c| c.contains("opacity-80"))
        );
    }

    #[test]
    fn avatar_layers_paint_by_tree_order_without_z_index() {
        use crate::core::reactivity::create_root;
        use crate::core::tree::LayoutTree;

        let (_owner, face) = create_root(|| {
            avatar_scope("", AvatarSize::Lg, AvatarShape::Circle, None, || {
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                frag.children(vec![
                    AvatarImage(AvatarImageProps {
                        id: String::new(),
                        src: Some(
                            "data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg'/>".into(),
                        ),
                        alt: Some(String::new()),
                        base: None,
                        children: None,
                        ..Default::default()
                    }),
                    AvatarFallback(AvatarFallbackProps {
                        id: String::new(),
                        children: Some(Element::new().text("JD")),
                        base: None,
                        ..Default::default()
                    }),
                    build_avatar_badge(BadgeVariant::Default, Vec::new(), None),
                ])
            })
        });
        let slots: Vec<&str> = face
            .children
            .iter()
            .filter_map(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()))
            .collect();
        assert_eq!(
            slots,
            vec![SLOT_FALLBACK, SLOT_IMAGE, SLOT_BADGE],
            "image must paint after fallback, badge last — no z-index needed"
        );
        for c in &face.children {
            assert_eq!(
                c.style.z_index,
                0,
                "slot {:?} z-index would enable gallery page_stack_bands",
                c.attrs.get(ATTR_SLOT)
            );
        }

        let mut col = Element::new().flex_col().w(400.0).gap(8.0);
        col = col.child(Element::new().text("Fallback Image"));
        col = col.child(face);
        col = col.child(Element::new().text("Sizes"));
        let mut root = Element::new()
            .w(400.0)
            .h(280.0)
            .overflow_y("auto")
            .child(col);
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 400.0, 280.0, [-1.0, -1.0]);
        assert!(
            result.overlay_split.page_stack_bands.is_empty(),
            "avatar image/badge must not switch the page onto page_stack_bands, got {}",
            result.overlay_split.page_stack_bands.len()
        );
        root.scroll_offset = [0.0, 40.0];
        let scrolled =
            tree.build_and_layout_fast(&mut root, 400.0, 280.0, [-1.0, -1.0], true, false);
        assert!(
            scrolled.overlay_split.page_stack_bands.is_empty(),
            "scroll_only collect must not grow page_stack_bands from avatars, got {}",
            scrolled.overlay_split.page_stack_bands.len()
        );
        let y0 = result
            .text_nodes
            .iter()
            .find(|n| n.text.contains("Sizes"))
            .map(|n| n.y);
        let y1 = scrolled
            .text_nodes
            .iter()
            .find(|n| n.text.contains("Sizes"))
            .map(|n| n.y);
        if let (Some(before), Some(after)) = (y0, y1) {
            assert!(
                (after - (before - 40.0)).abs() < 0.5,
                "label glyphon y must track scroll (before={before:.2} after={after:.2})"
            );
        }
    }

    #[test]
    fn dual_surface_compiles_shape() {
        let _b: Element = avatar().size("sm").fallback("AB").into();
        let _i: Element = avatar().fallback_icon("user").into();
        let _fb: Element = avatar_scope("", AvatarSize::Default, AvatarShape::Circle, None, || {
            avatar_fallback().text("X").into()
        });
        let el = Avatar(AvatarProps {
            id: String::new(),
            size: Some(AvatarSize::Default),
            shape: Some(AvatarShape::Circle),
            children: None,
            base: None,
            ..Default::default()
        });
        let _: Element = el;
    }
}
