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
//!             frag.set_attr("jadix_fragment", "1");
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
use crate::utils::lucide_glyph;
use crate::utils::popup::{
    mark_floating_with_gap, PopupPlacement, ATTR_FLOATING_MATCH_TRIGGER, ATTR_FLOATING_MIN_W,
    ATTR_FLOATING_STRATEGY, ATTR_FLOATING_W,
};
use crate::utils::take_packed;
use crate::utils::text_node::build_named_icon_text_node;
use crate::utils::text_node::{build_text_node, TextNodeConfig};

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
            if el.text_style.size.is_none() {
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
        style_fallback_subtree(
            &mut c,
            ctx.size.fallback_text_px(),
            ctx.size.fallback_icon_px(),
            parse_text_color("muted-foreground"),
        );
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
        .or_else(|| el.attrs.get("jadix_badge_variant"))
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
        if el.style.bg_color[3] == 0.0 {
            el.style.bg_color = el
                .children
                .iter()
                .find(|child| child.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_FALLBACK))
                .map(|child| child.style.bg_color)
                .unwrap_or_else(|| parse_color("muted"));
        }
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
    let rest = key.strip_prefix("jadix-avatar-src:")?;
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
        .get("jadix_utility_class")
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
        .wrap("wrap")
        .max_h("20rem")
        .overflow_y("auto")
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

struct AvatarGroupEntry {
    focus_id: String,
    faces: Vec<AvatarFaceSpec>,
    max_visible: usize,
    count_size: AvatarSize,
    manual_visible_n: usize,
    manual_has_count: bool,
    rendered_visible: Cell<usize>,
    expanded: Signal<bool>,
    placement: Cell<PopupPlacement>,
    expand_class: String,
}

thread_local! {
    static AVATAR_GROUPS: RefCell<HashMap<String, AvatarGroupEntry>> =
        RefCell::new(HashMap::new());
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
                rendered_visible: Cell::new(visible_n),
                expanded,
                placement: Cell::new(placement),
                expand_class,
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
    let (target, others) = AVATAR_GROUPS.with(|groups| {
        let groups = groups.borrow();
        let target = groups.get(id).map(|entry| entry.expanded);
        let others: Vec<_> = groups
            .iter()
            .filter(|(key, entry)| key.as_str() != id && entry.expanded.peek())
            .map(|(_, entry)| entry.expanded)
            .collect();
        (target, others)
    });
    for other in others {
        other.try_set(false);
    }
    if let Some(target) = target {
        let next = !target.peek();
        if next {
            crate::utils::popup::disarm_underlay_close();
        }
        target.try_set(next);
    }
}

#[cfg(all(test, feature = "product-tests"))]
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

fn build_avatar_group_expand_panel(
    floating_id: &str,
    trigger_fid: &str,
    placement: PopupPlacement,
    faces: &[AvatarFaceSpec],
    expand_class: Option<&str>,
    trigger_h: f32,
) -> Element {
    let content_w = expand_content_width(trigger_h, faces.len()).min(320.0);
    let mut panel = default_expand_container(expand_group_faces(faces), expand_class)
        .w(content_w)
        .max_w("100%");
    panel.set_attr(ATTR_FLOATING_W, format!("{content_w}px"));
    panel.set_attr("data-avatar-group-popover", "1");
    panel = panel.opacity_group(true);
    panel.set_attr("data-slot", "popover-content");
    mark_floating_with_gap(&mut panel, floating_id, trigger_fid, placement, 6.0);
    panel.set_attr(ATTR_FLOATING_STRATEGY, "fixed");
    panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
    panel.set_attr(ATTR_FLOATING_MIN_W, "0");
    panel.align_content_kw("flex-start").absolute()
}

fn avatar_group_count_trigger(group_id: &str, count: Element) -> Element {
    let click_id = group_id.to_string();
    let expanded = AVATAR_GROUPS.with(|groups| groups.borrow().get(group_id).unwrap().expanded);
    let mut trigger: Element = interactive(count)
        .focus_id(format!("{group_id}__focus"))
        .focusable(true)
        .cursor("pointer")
        .on_click(move || open_avatar_group_expand(&click_id))
        .into();
    trigger.set_attr("role", "button");
    trigger.set_attr("aria-haspopup", "dialog");
    bind_attr(
        trigger,
        format!("{group_id}__aria_expanded"),
        move || expanded.get(),
        |element, open| {
            element.set_attr("aria-expanded", if open { "true" } else { "false" });
            LayoutStatus::StyleChanged
        },
    )
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
    let mut kids = vec![Element::new().text(format!("+{overflow_n}"))];
    for spec in overflow_faces {
        kids.push(hide_overflow_avatar(rebuild_face(spec)));
    }
    let count = build_avatar_group_count(size, kids, None);
    apply_group_ring(avatar_group_count_trigger(group_id, count))
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

#[cfg(all(test, feature = "product-tests"))]
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
        if el.layout_node_id().is_some() {
            layout_tree.mark_element_layout_dirty(el);
        }
        *impact = impact.merge(LayoutStatus::StructureChanged);
    }
    structure_changed
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
        count = avatar_group_count_trigger(group_id, count);
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

    register_avatar_group_entry(
        group_id.clone(),
        focus_id.clone(),
        faces.clone(),
        max_visible,
        count_size,
        manual_visible_n,
        manual_has_count,
        rendered_visible,
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
            6.0,
            FloatingStrategy::Fixed,
        )),
        kind: OverlayKind::Popover,
        motion: FloatingMotionKind::Fade,
        mode: OverlayMode::Stacked,
        // Content-sized. `"auto"` must not be written as `jadix_floating_w` (that attr is
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
            let (faces, fid, placement, expand_class) = AVATAR_GROUPS.with(|m| {
                let map = m.borrow();
                match map.get(&portal_id) {
                    Some(e) => (
                        e.faces.clone(),
                        e.focus_id.clone(),
                        e.placement.get(),
                        e.expand_class.clone(),
                    ),
                    None => (
                        Vec::new(),
                        String::new(),
                        PopupPlacement::BottomStart,
                        String::new(),
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
            );
            panel
        })
    });

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
    let mut row_el = shell;
    row_el.set_attr(ATTR_SLOT, SLOT_GROUP);
    row_el.set_attr(ATTR_GROUP_ID, &group_id);
    if max_visible != usize::MAX {
        row_el.set_attr(ATTR_MAX_VISIBLE, max_visible.to_string());
    }
    row_el.set_attr(
        "data-state",
        if expanded.peek() { "open" } else { "closed" },
    );
    for phase in [
        crate::authoring::presentation::Phase::State,
        crate::authoring::presentation::Phase::AfterGeometry,
    ] {
        crate::authoring::presentation::attach(&mut row_el, phase, |element, layout, context| {
            let mut status = LayoutStatus::Clean;
            sync_avatar_group_shell(element, context.width_budget, layout, &mut status);
            status
        });
    }
    let scroll_revision = Cell::new(crate::authoring::input::scroll_revision());
    crate::authoring::deadline::attach(
        &mut row_el,
        crate::authoring::deadline::Visibility::Always,
        move |_| {
            let revision = crate::authoring::input::scroll_revision();
            let moved = scroll_revision.replace(revision) != revision;
            let redraw = moved && expanded.try_peek() == Some(true) && expanded.try_set(false);
            crate::authoring::deadline::Poll {
                redraw,
                next_wake: None,
            }
        },
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
            frag.set_attr("jadix_fragment", "1");
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

crate::__component_contract! {
AvatarProps [base_builder];
/// ShadCN `Avatar` root container.
#[allow(non_snake_case)]
pub fn Avatar(
identity [] id: String,
default [] size: AvatarSize,
default [] shape: AvatarShape,
base_builder [] base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
plain [] children: Option<Element>,
) -> Element {
    let class = None;
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = avatar_scope(id, size, shape, class, move || {
        let mut frag = Element::new();
        if kids.is_empty() {
            frag
        } else {
            frag.set_attr("jadix_fragment", "1");
            frag.children(kids)
        }
    });
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}
}

crate::__component_contract! {
AvatarImageProps [base_builder];
/// Avatar image layer (`aspect-square size-full object-cover` mental).
#[allow(non_snake_case)]
pub fn AvatarImage(
identity [] id: String,
default [] src: String,
default [] alt: String,
base_builder [] base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
plain [] children: Option<Element>,
) -> Element {
    let _ = id;
    let mut el = build_avatar_image(&src, &alt, base_builder);
    if let Some(c) = children {
        el = el.child(c);
    }
    el
}
}

crate::__component_contract! {
AvatarFallbackProps [base_builder];
/// Fallback when the image is unavailable (`bg-muted` + centered; supports text / Icon / Image / any child node).
#[allow(non_snake_case)]
pub fn AvatarFallback(
identity [] id: String,
base_builder [] base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
plain [] children: Option<Element>,
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
}

crate::__component_contract! {
AvatarBadgeProps [base_builder];
/// 右下角状态点 / 图标徽标。
#[allow(non_snake_case)]
pub fn AvatarBadge(
identity [] id: String,
default [] variant: BadgeVariant,
base_builder [] base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
plain [] children: Option<Element>,
) -> Element {
    let _ = id;
    let kids = children.map(take_packed).unwrap_or_default();
    build_avatar_badge(variant, kids, base_builder)
}
}

crate::__component_contract! {
AvatarGroupProps [];
/// Overlapping rows of multiple heads (`-space-x-2` + `ring-background`).
///
/// - `expand_placement`: floating expand side for `+N` (default `bottom-start`, in-place cover; faces align to collapsed faces).
/// - `expand_class`: utility class for the expand container (default = popover / border / shadow tokens).
/// - `max_visible`: 折叠前最多展示的头像数；未设置时保留手动 `AvatarGroupCount` 布局，但仍会在宽度不足时自动折叠。
/// - Clicking `AvatarGroupCount` toggles a popover anchored to `+N`, showing all faces.
///   Hover does not open or close it; outside click and Escape use the shared popover lifecycle.
#[allow(non_snake_case)]
pub fn AvatarGroup(
plain [] expand_placement: Option<String>,
plain [] expand_class: Option<String>,
plain [] max_visible: Option<i32>,
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let placement = expand_placement
        .as_deref()
        .map(expand_placement_from_attr)
        .unwrap_or(PopupPlacement::BottomStart);
    let max_visible = max_visible.map(|n| n.max(1) as usize).unwrap_or(usize::MAX);
    build_avatar_group(kids, placement, expand_class, max_visible)
}
}

crate::__component_contract! {
AvatarGroupCountProps [base_builder];
/// "+N" counting capsules in the group.
#[allow(non_snake_case)]
pub fn AvatarGroupCount(
default [] size: AvatarSize,
base_builder [] base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    build_avatar_group_count(size, kids, base_builder)
}
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

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/avatar.rs"]
mod tests;
