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
//! ## 用法（双写法 · ADR-9）
//! ```ignore
//! // A · RSX / 标签
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
//!     <AvatarGroupCount>"+3"</AvatarGroupCount>
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
//! avatar_fallback().icon("user").into() // 须在 avatar_scope / <Avatar> 内
//! ```
//!
//! ## 注意
//! - **展示型**：空 `id` 不写 `template_id`（WP-2.8）。
//! - `<Avatar>` 由宏展开为 [`avatar_scope`]，子件读取 [`AvatarCtx`] 的 `size`。
//! - 图片层叠在 Fallback 之上；`AvatarImage` 解码为**圆形 alpha 遮罩**贴图（GPU 纹理管线无
//!   border-radius clip），失败 / 空 `src` / Pending 时 opacity=0，Fallback 露出。
//! - 动态 `size={sig.get()}` → [`apply_bound_avatar_size`]（Solid bind，不整树重建）。

use std::sync::OnceLock;

use crate::app::pending_images::{
    ensure_avatar_src_loaded, texture_key_for_avatar_src, track_src_load_signal, AvatarImageMask,
    SrcLoadState,
};
use crate::app::reactive::apply_bound_size;
use crate::app::reactive::bind_attr;
use crate::components::badge::{apply_badge_variant_paint, BadgeVariant};
use crate::components::base::BaseComponentBuilder;
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_signal, with_child_owner, Context, Signal,
};
use crate::core::tree::{parse_color, Element, IntoF32, LayoutStatus};
use crate::primitives::r#box::{hstack, r#box};
use crate::style::{parse_text_color, with_class_list};
use crate::utils::lucide::lucide_glyph;
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

// ---------------------------------------------------------------------------
// Size
// ---------------------------------------------------------------------------

/// Avatar `size`：`default` · `sm` · `lg`（对位 ShadCN `data-size`）。
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

    /// 根容器边长（new-york `size-8` / `size-6` / `size-10`）。
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

/// 读取最近的 [`Avatar`] 上下文。
pub fn use_avatar() -> Option<AvatarCtx> {
    use_context(avatar_context())
}

fn require_avatar(part: &str) -> AvatarCtx {
    use_avatar().unwrap_or_else(|| {
        panic!("{part} must be used within <Avatar> / avatar_scope");
    })
}

fn take_packed(mut el: Element) -> Vec<Element> {
    if el.attrs.get("rui_fragment").map(|s| s.as_str()) == Some("1") {
        return std::mem::take(&mut el.children);
    }
    if el.text.as_ref().is_none_or(|t| t.is_empty()) && el.children.len() > 1 {
        return std::mem::take(&mut el.children);
    }
    vec![el]
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
            if el.text_style.size <= 1.0 {
                el.text_style.size = icon_px;
            }
            el.text_style.color = fg;
        } else {
            el.text_style.size = px;
            el.text_style.color = fg;
            el.text_style.weight = 500;
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
                if c.text_style.size <= 1.0 {
                    c.text_style.size = icon_px;
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
    el = el.z_index(if slot == Some(SLOT_IMAGE) { 1 } else { 0 });
    if let Some(slot) = slot {
        el.set_attr(ATTR_SLOT, slot);
    }

    if src.trim().is_empty() {
        el.set_attr(ATTR_LOAD_STATE, "failed");
        el.style.opacity = 0.0;
    } else {
        let key = texture_key_for_avatar_src(src, mask);
        let load = use_signal(format!("{key}__src_load").as_str(), || {
            ensure_avatar_src_loaded(&key, src, mask)
        });
        track_src_load_signal(&key, load);
        el.set_attr(ATTR_LOAD_STATE, load_state_label(load.peek()));
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
        .get("rui_badge_variant")
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
    el = el
        .display_flex()
        .items_center()
        .justify_center()
        .z_index(10);
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
    if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_AVATAR) {
        let ring = parse_color("background");
        el = el.border(ring, 2.0);
    }
    el
}

fn build_avatar_group(children: Vec<Element>) -> Element {
    let mut row = hstack().items_center();
    for (i, child) in children.into_iter().enumerate() {
        let mut c = apply_group_ring(child);
        if i > 0 {
            c = c.ml("-0.5rem");
        }
        row = row.child(c);
    }
    let mut el: Element = row.into();
    el.set_attr(ATTR_SLOT, SLOT_GROUP);
    el
}

fn build_avatar_group_count(
    size: AvatarSize,
    children: Vec<Element>,
    base: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
) -> Element {
    let dim = size.box_dim();
    let ring = parse_color("background");
    let muted = parse_color("muted");
    let fg = parse_text_color("muted-foreground");
    let mut el: Element = r#box()
        .relative(true)
        .flex_shrink(0.0)
        .rounded_full()
        .bg(muted)
        .border(ring, 2.0)
        .w(dim)
        .h(dim)
        .min_w(dim)
        .min_h(dim)
        .into();
    el = el.display_flex().items_center().justify_center();
    el.set_attr(ATTR_SLOT, SLOT_GROUP_COUNT);
    for mut c in children {
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
            el = el.child(node);
        } else {
            if c.text_style.font_family.as_deref() == Some("lucide") && c.text_style.size <= 1.0 {
                c.text_style.size = IntoF32::into_f32("1rem");
            }
            el = el.child(c);
        }
    }
    if let Some(b) = base {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

// ---------------------------------------------------------------------------
// AvatarFallback builder（RSX / AvatarBuilder 共用）
// ---------------------------------------------------------------------------

/// Fallback 内容构建器：文本 · Lucide Icon · Image · 任意子节点。
pub struct AvatarFallbackBuilder {
    children: Vec<Element>,
    image_src: Option<String>,
    base: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
}

/// Fallback 构建入口（须在 [`avatar_scope`] / `<Avatar>` 内求值为 Element）。
pub fn avatar_fallback() -> AvatarFallbackBuilder {
    AvatarFallbackBuilder {
        children: Vec::new(),
        image_src: None,
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

    pub fn into_element(self) -> Element {
        let mut children = self.children;
        if let Some(src) = self.image_src {
            children.push(build_avatar_texture_layer(&src, None));
        }
        build_avatar_fallback(children, self.base)
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

pub struct AvatarBuilder {
    id: String,
    size: AvatarSize,
    shape: AvatarShape,
    class: String,
    body: Option<Box<dyn FnOnce() -> Element>>,
}

/// Builder 入口（ADR-9 面 B）。
pub fn avatar() -> AvatarBuilder {
    AvatarBuilder {
        id: String::new(),
        size: AvatarSize::Default,
        shape: AvatarShape::Circle,
        class: String::new(),
        body: None,
    }
}

impl AvatarBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn size(mut self, s: impl AsRef<str>) -> Self {
        self.size = AvatarSize::parse(s.as_ref());
        self
    }

    pub fn shape(mut self, s: impl AsRef<str>) -> Self {
        self.shape = AvatarShape::parse(s.as_ref());
        self
    }

    pub fn class(mut self, c: impl Into<String>) -> Self {
        self.class = c.into();
        self
    }

    /// 复合面：在 scope 内构建 Image / Fallback / Badge。
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

    /// 便捷：Lucide 图标回退。
    pub fn fallback_icon(self, name: impl AsRef<str>) -> Self {
        self.push_fallback(avatar_fallback().icon(name))
    }

    /// 便捷：铺满 Fallback 层的图片（非 [`AvatarImage`] 加载管线）。
    pub fn fallback_image(self, src: impl Into<String>) -> Self {
        self.push_fallback(avatar_fallback().image(src))
    }

    /// 复合回退：在 [`avatar_fallback`] 上继续 `.text` / `.icon` / `.child`。
    pub fn fallback_with(
        self,
        build: impl FnOnce(AvatarFallbackBuilder) -> AvatarFallbackBuilder + 'static,
    ) -> Self {
        self.push_fallback(build(avatar_fallback()))
    }

    /// 便捷：仅图片（可再链 `.fallback` / `.fallback_icon` 等）。
    pub fn image(mut self, src: impl Into<String>) -> Self {
        let src = src.into();
        self.body = Some(Box::new(move || {
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            frag.children(vec![AvatarImage(AvatarImageProps {
                id: String::new(),
                src: Some(src.clone()),
                alt: Some(String::new()),
                base: None,
                children: None,
                ..Default::default()
            })])
        }));
        self
    }
}

impl From<AvatarBuilder> for Element {
    fn from(builder: AvatarBuilder) -> Self {
        let class = if builder.class.trim().is_empty() {
            None
        } else {
            Some(builder.class)
        };
        let body = builder.body;
        avatar_scope(builder.id, builder.size, builder.shape, class, move || {
            body.map(|b| b()).unwrap_or_else(Element::new)
        })
    }
}

// ---------------------------------------------------------------------------
// RSX components
// ---------------------------------------------------------------------------

/// ShadCN `Avatar` 根容器。
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

/// 头像图片层（`aspect-square size-full object-cover` 心智）。
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

/// 图片不可用时的回退（`bg-muted` + 居中；支持文本 / Icon / Image / 任意子节点）。
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

/// 重叠排列的多头像行（`-space-x-2` + `ring-background`）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AvatarGroup(children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    build_avatar_group(kids)
}

/// 组内「+N」计数胶囊。
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
