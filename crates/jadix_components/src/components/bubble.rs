//! # Bubble
//!
//! ShadCN **base** 聊天气泡：对齐
//! `Reference/ShadCN-UI/.../bases/base/ui/bubble.tsx` 与
//! [Bubble 文档](https://ui.shadcn.com/docs/components/base/bubble)。
//!
//! ```text
//! BubbleGroup
//! ├── Bubble
//! │   ├── BubbleContent
//! │   └── BubbleReactions
//! └── Bubble …
//! ```
//!
//! ## 用法（双写法 · ADR-9）
//! ```ignore
//! // A · RSX / 标签
//! <BubbleGroup>
//!     <Bubble variant="default" align="start">
//!         <BubbleContent>"Hey there! What's up?"</BubbleContent>
//!     </Bubble>
//!     <Bubble variant="secondary" align="end">
//!         <BubbleContent>"All good on my side."</BubbleContent>
//!         <BubbleReactions side="bottom" align="center" layout="overlay">
//!             <Badge variant="secondary">"👍 2"</Badge>
//!         </BubbleReactions>
//!     </Bubble>
//! </BubbleGroup>
//!
//! // B · Builder
//! Element::from(
//!     bubble_group()
//!         .child(
//!             bubble()
//!                 .variant("muted")
//!                 .align("start")
//!                 .child(bubble_content().text("Builder bubble"))
//!         )
//! )
//! // 全局样式覆盖（与 Button / Card 等相同，非 Bubble 专有 API）：
//! // <BubbleContent class="bg-card text-foreground">"…"</BubbleContent>
//! // <BubbleContent bg="destructive/20" text_color="destructive">"…"</BubbleContent>
//! ```
//!
//! ## 注意
//! - **展示型**：空 `id` 不写 `template_id`（WP-2.8）。
//! - `<Bubble>` 由宏展开为 [`bubble_scope`]；[`BubbleContent`] 读取 [`BubbleCtx`] 的 `variant`。
//! - 动态 `variant={sig.get()}` / `align={sig.get()}` → [`apply_bound_bubble_variant`] /
//!   [`apply_bound_bubble_align`]（Solid bind，不整树重建）。
//! - 变体 chroming 走主题令牌（`destructive/10` 等）；样式覆盖与其它组件相同：
//!   RSX 上写 `class` / `bg` / `base`，或 `bind_class`（见 [`crate::style::utility`]）。
//! - `ghost` 变体去框 / 去内边距，根 `max_w` 放宽为 `100%`（对位 ShadCN）。
//! - [`BubbleReactions`] 默认独立行；省略 `align` 时左气泡右下、右气泡左下（朝向会话内侧）。
//!   可用 `side` / `align` 覆盖到上/下 × 左/中/右。`layout="overlay"` 用像素位移咬住气泡边
//!   （仍走文档流，避免 `position:absolute` 在停滚后让文字相对壳错位）。`max_visible` 控制折叠前
//!   展示数（默认 3），父气泡宽度不足时自动收入 `+N`。`+N` 展开走根级 Portal
//!   （与菜单 / Popover 同一套 `portal_sync`，互斥）；就地盖住原条，宽度不够则换行。底条只向下、
//!   顶条只向上，绝不翻向盖住当前气泡。

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

use crate::app::portal_overlay::{OverlayKind, OverlayMode};
use crate::components::base::SlotChrome;
use crate::components::label::label;
use crate::core::reactivity::{
    create_context, create_effect, provide_context, use_context, use_signal, with_child_owner,
    Context, Signal,
};
use crate::core::tree::{parse_color, parse_text_color, Element, IntoF32, LayoutStatus};
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box, vstack, BoxBuilder};
use crate::style::{is_dark_theme, with_class_list};
use crate::tokens;
use crate::utils::anchored_overlay::{
    anchored_overlay_content, provide_anchored_overlay_context, AnchoredOverlayContentOpts,
    FloatingMotionKind, FloatingStrategy, Placement,
};
#[cfg(all(test, feature = "product-tests"))]
use crate::utils::overflow_fit::tighten_horizontal_constraint;
use crate::utils::popup::{
    PopupPlacement, ATTR_FLOATING_EXPAND_FROM_H, ATTR_FLOATING_EXPAND_FROM_W,
};
use crate::utils::text_node::{build_text_node, TextNodeConfig};

const ATTR_SLOT: &str = "data-slot";
const ATTR_VARIANT: &str = "data-variant";
const ATTR_ALIGN: &str = "data-align";
const ATTR_ALIGN_INHERIT: &str = "data-align-inherit";
const ATTR_SIDE: &str = "data-side";
const ATTR_LAYOUT: &str = "data-layout";
const ATTR_BUBBLE_VARIANT: &str = "data-bubble-variant";
const ATTR_BUBBLE_VARIANT_LEGACY: &str = "jadix_bubble_variant";
const SLOT_GROUP: &str = "bubble-group";
const SLOT_BUBBLE: &str = "bubble";
const SLOT_CONTENT: &str = "bubble-content";
const SLOT_REACTIONS: &str = "bubble-reactions";
const ATTR_REACTIONS_ID: &str = "data-reactions-id";
const ATTR_REACTIONS_INNER: &str = "data-reactions-inner";
const ATTR_REACTIONS_OVERLAY: &str = "data-reactions-overlay";
const ATTR_OVERFLOW_CHIP: &str = "data-reaction-overflow";
const ATTR_MAX_VISIBLE: &str = "data-reactions-max-visible";
/// Default visible reaction chips before folding the rest into `+N`.
pub const DEFAULT_MAX_VISIBLE_REACTIONS: usize = 3;
/// Overlay 叠进气泡边的距离。用像素 `translate`（不是 `translate_pct` / `position:absolute`）：
/// 百分比位移和绝对定位都会让停滚后文字相对气泡壳上下错开。
const REACTION_OVERLAY_OVERLAP: &str = "0.5rem";
/// Overlay 相对气泡左右边的内缩，避免 reaction 条与气泡齐边（独立行与叠加共用）。
const REACTION_EDGE_INSET: &str = "0.5rem";
/// +N 展开浮层最大宽度；超出则换行加高，避免横向溢到邻条气泡。
const REACTION_EXPAND_MAX_W: &str = "12rem";
/// 芯片边长 = `text-xs` 默认行高（1rem）。气泡正文是 `text-sm`（0.875rem / 1.25rem 行高），
/// reaction 小一号，避免 Twemoji 铺满 em 框后看起来和气泡一样大。
const REACTION_CHIP_SIZE: &str = "1rem";
/// Reaction 字形：`text-xs`（0.75rem）。ShadCN 条带相对 `leading-relaxed` 正文更紧凑；
/// Twemoji 若跟正文同用 `text-sm` 会填满字号，视觉上不再小一号。
const REACTION_EMOJI_SIZE: &str = "0.75rem";
/// 折叠胶囊四边内边距（左右=上下，单 emoji 才是正方形圆角，而不是左右更宽把 glyph 挤向一侧）。
const REACTION_BAR_PAD: &str = "0.375rem";
/// 折叠胶囊圆角：`(chip + 2×pad) / 2`。展开层沿用此值，不用 `rounded_full`（否则加高后半径跟着变）。
const REACTION_BAR_RADIUS: &str = "0.875rem";
// Hover scale uses sticky Bake; Bake now composites Twemoji (see text_texture),
// so transition-transform is safe — do not fall back to font outline / MSDF.
const REACTION_CHIP_CLASS: &str =
    "cursor-pointer transition-transform duration-150 ease-out hover:scale-125";

static BUBBLE_REACTIONS_SEQ: AtomicUsize = AtomicUsize::new(0);

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Bubble `variant`：ShadCN 七种视觉语气。
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum BubbleVariant {
    #[default]
    Default,
    Secondary,
    Muted,
    Tinted,
    Outline,
    Ghost,
    Destructive,
}

impl BubbleVariant {
    pub fn parse(raw: &str) -> Self {
        raw.parse().unwrap_or(Self::Default)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Secondary => "secondary",
            Self::Muted => "muted",
            Self::Tinted => "tinted",
            Self::Outline => "outline",
            Self::Ghost => "ghost",
            Self::Destructive => "destructive",
        }
    }
}

impl From<&str> for BubbleVariant {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for BubbleVariant {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

impl From<&String> for BubbleVariant {
    fn from(s: &String) -> Self {
        Self::parse(s)
    }
}

impl std::str::FromStr for BubbleVariant {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "secondary" => Self::Secondary,
            "muted" => Self::Muted,
            "tinted" => Self::Tinted,
            "outline" => Self::Outline,
            "ghost" => Self::Ghost,
            "destructive" | "error" => Self::Destructive,
            _ => Self::Default,
        })
    }
}

/// 气泡行内对齐（`start` / `end`）。
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum BubbleAlign {
    #[default]
    Start,
    End,
}

impl BubbleAlign {
    pub fn parse(raw: &str) -> Self {
        raw.parse().unwrap_or(Self::Start)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::End => "end",
        }
    }

    fn align_self(self) -> &'static str {
        match self {
            Self::Start => "flex-start",
            Self::End => "flex-end",
        }
    }
}

impl From<&str> for BubbleAlign {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for BubbleAlign {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

impl From<&String> for BubbleAlign {
    fn from(s: &String) -> Self {
        Self::parse(s)
    }
}

impl std::str::FromStr for BubbleAlign {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "end" | "right" => Self::End,
            _ => Self::Start,
        })
    }
}

/// [`BubbleReactions`] 垂直锚点（`top` / `bottom`）。
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum BubbleReactionsSide {
    Top,
    #[default]
    Bottom,
}

impl BubbleReactionsSide {
    pub fn parse(raw: &str) -> Self {
        raw.parse().unwrap_or(Self::Bottom)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }
}

impl From<&str> for BubbleReactionsSide {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for BubbleReactionsSide {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

impl From<&String> for BubbleReactionsSide {
    fn from(s: &String) -> Self {
        Self::parse(s)
    }
}

impl std::str::FromStr for BubbleReactionsSide {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "top" | "up" => Self::Top,
            _ => Self::Bottom,
        })
    }
}

/// [`BubbleReactions`] 水平锚点（`start` / `center` / `end`）。省略时跟随父气泡。
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum BubbleReactionsAlign {
    Start,
    Center,
    #[default]
    End,
}

impl BubbleReactionsAlign {
    pub fn parse(raw: &str) -> Self {
        raw.parse().unwrap_or(Self::End)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Center => "center",
            Self::End => "end",
        }
    }

    fn align_self(self) -> &'static str {
        match self {
            Self::Start => "flex-start",
            Self::Center => "center",
            Self::End => "flex-end",
        }
    }
}

impl From<&str> for BubbleReactionsAlign {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for BubbleReactionsAlign {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

impl From<&String> for BubbleReactionsAlign {
    fn from(s: &String) -> Self {
        Self::parse(s)
    }
}

impl std::str::FromStr for BubbleReactionsAlign {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "start" | "left" => Self::Start,
            "center" | "middle" => Self::Center,
            _ => Self::End,
        })
    }
}

/// Reaction 条与气泡的关系：独立行占位，或叠在气泡边缘绘制。
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum BubbleReactionsLayout {
    #[default]
    Row,
    Overlay,
}

impl BubbleReactionsLayout {
    pub fn parse(raw: &str) -> Self {
        raw.parse().unwrap_or(Self::Row)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Row => "row",
            Self::Overlay => "overlay",
        }
    }
}

impl From<&str> for BubbleReactionsLayout {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for BubbleReactionsLayout {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

impl From<&String> for BubbleReactionsLayout {
    fn from(s: &String) -> Self {
        Self::parse(s)
    }
}

impl std::str::FromStr for BubbleReactionsLayout {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "overlay" | "layer" | "absolute" => Self::Overlay,
            _ => Self::Row,
        })
    }
}

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

/// Root 气泡上下文：子件 [`BubbleContent`] / [`BubbleReactions`] 读取 variant 与 align。
#[derive(Clone, Copy, Debug)]
pub struct BubbleCtx {
    pub variant: BubbleVariant,
    pub align: BubbleAlign,
}

fn bubble_context() -> Context<BubbleCtx> {
    static CTX: OnceLock<Context<BubbleCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<BubbleCtx>)
}

/// 读取最近的 [`Bubble`] 上下文。
pub fn use_bubble() -> Option<BubbleCtx> {
    use_context(bubble_context())
}

fn require_bubble(part: &str) -> BubbleCtx {
    use_bubble().unwrap_or_else(|| {
        panic!("{part} must be used within <Bubble> / bubble_scope");
    })
}

// ---------------------------------------------------------------------------
// Chrome
// ---------------------------------------------------------------------------

/// ShadCN `tinted`：primary 轻染色（混 `muted` 而非 `background`）。
fn tinted_bg() -> [f32; 4] {
    let dark = is_dark_theme();
    let primary = parse_color("primary");
    let base = parse_color("muted");
    let a = if dark { 0.35 } else { 0.14 };
    [
        primary[0] * a + base[0] * (1.0 - a),
        primary[1] * a + base[1] * (1.0 - a),
        primary[2] * a + base[2] * (1.0 - a),
        1.0,
    ]
}

fn bubble_content_chrome(variant: BubbleVariant) -> ([f32; 4], [u8; 4], Option<[f32; 4]>, bool) {
    match variant {
        BubbleVariant::Secondary => (
            parse_color("secondary"),
            parse_text_color("secondary-foreground"),
            None,
            false,
        ),
        BubbleVariant::Muted => (
            parse_color("muted"),
            parse_text_color("muted-foreground"),
            None,
            false,
        ),
        BubbleVariant::Tinted => (tinted_bg(), parse_text_color("foreground"), None, false),
        BubbleVariant::Outline => (
            parse_color("background"),
            parse_text_color("foreground"),
            Some(parse_color("border")),
            false,
        ),
        BubbleVariant::Ghost => (
            [0.0, 0.0, 0.0, 0.0],
            parse_text_color("foreground"),
            None,
            true,
        ),
        BubbleVariant::Destructive => (
            parse_color(if is_dark_theme() {
                "destructive/20"
            } else {
                "destructive/10"
            }),
            parse_text_color("destructive"),
            None,
            false,
        ),
        BubbleVariant::Default => (
            parse_color("primary"),
            parse_text_color("primary-foreground"),
            None,
            false,
        ),
    }
}

fn paint_descendant_text(el: &mut Element, color: [u8; 4]) {
    crate::core::tree::walk_elements_mut(el, |el| {
        if el.text.is_some() {
            el.text_style.color = Some(color);
        }
    });
}

/// Reaction glyph：走与文本页相同的 [`label`] 路径（彩色 emoji）。
/// 叶子与芯片同宽 + `text-align: center`，字形盒内墨水由引擎水平居中。
/// 高度跟 `leading-none` 行盒，垂直居中交给芯片 `items_center`——
/// 若把叶子拉满正方形，行盒仍只有字号高，墨水会贴在芯片顶部。
fn reaction_chip_shell(glyph: impl Into<String>) -> Element {
    let box_side = IntoF32::into_f32(REACTION_CHIP_SIZE);
    let size = IntoF32::into_f32(REACTION_EMOJI_SIZE);
    let mut leaf: Element = label("")
        .text(glyph.into())
        .size(size)
        .text_leading_none()
        .text_align("center")
        .into();
    leaf = leaf.flex_shrink(0.0).w(box_side).overflow("visible");
    with_class_list(
        r#box()
            .flex_row()
            .flex_shrink(0.0)
            .items_center()
            .justify_center()
            .w(box_side)
            .h(box_side)
            .overflow("visible")
            .child(leaf)
            .into(),
        REACTION_CHIP_CLASS,
    )
}

/// `+N` 圆形徽章：与 emoji 同尺寸，展开浮层时与胶囊内 emoji 视觉融合。
fn reaction_overflow_chip_shell(count: u32) -> Element {
    let box_side = IntoF32::into_f32(REACTION_CHIP_SIZE);
    let size = IntoF32::into_f32(REACTION_EMOJI_SIZE);
    let mut leaf: Element = label("")
        .text(format!("+{count}"))
        .size(size)
        .text_leading_none()
        .text_align("center")
        .into();
    leaf = leaf.flex_shrink(0.0).w(box_side).overflow("visible");
    let mut el: Element = r#box()
        .flex_row()
        .flex_shrink(0.0)
        .items_center()
        .justify_center()
        .w(box_side)
        .h(box_side)
        .rounded_full()
        .bg(parse_color("background"))
        .overflow("visible")
        .child(leaf)
        .into();
    el.set_attr(ATTR_OVERFLOW_CHIP, "1");
    with_class_list(el, REACTION_CHIP_CLASS)
}

fn reaction_overflow_label(text: Option<&str>) -> Option<u32> {
    let t = text?.trim();
    let rest = t.strip_prefix('+')?;
    let n: u32 = rest.parse().ok()?;
    (n > 0).then_some(n)
}

fn extract_reaction_text(el: &Element) -> Option<String> {
    crate::core::tree::first_trimmed_text(el)
}

fn normalize_reaction_items(kids: Vec<Element>) -> Vec<String> {
    let mut items: Vec<String> = kids.iter().filter_map(extract_reaction_text).collect();
    while items
        .last()
        .and_then(|t| reaction_overflow_label(Some(t.as_str())))
        .is_some()
    {
        items.pop();
    }
    items
}

fn reactions_align_from_bubble() -> BubbleReactionsAlign {
    match use_bubble().map(|c| c.align).unwrap_or(BubbleAlign::Start) {
        BubbleAlign::Start => BubbleReactionsAlign::End,
        BubbleAlign::End => BubbleReactionsAlign::Start,
    }
}

fn inner_corner_align(bubble_align: BubbleAlign) -> BubbleReactionsAlign {
    match bubble_align {
        BubbleAlign::Start => BubbleReactionsAlign::End,
        BubbleAlign::End => BubbleReactionsAlign::Start,
    }
}

fn apply_resolved_reaction_align(el: &mut Element, align: BubbleReactionsAlign) {
    el.set_attr(ATTR_ALIGN, align.as_str());
    el.attrs.remove(ATTR_ALIGN_INHERIT);
    if let Some(id) = el.attrs.get(ATTR_REACTIONS_ID) {
        BUBBLE_REACTIONS.with(|m| {
            if let Some(entry) = m.borrow_mut().get_mut(id) {
                entry.align.set(align);
            }
        });
    }
    let taken = std::mem::replace(el, Element::new());
    *el = taken.align_self_kw(align.align_self());
}

fn resolve_inherited_reaction_align(kids: &mut [Element], bubble_align: BubbleAlign) {
    let want = inner_corner_align(bubble_align);
    for el in kids {
        if !is_bubble_reactions(el) {
            continue;
        }
        if el.attrs.get(ATTR_ALIGN_INHERIT).map(|s| s.as_str()) != Some("1") {
            continue;
        }
        apply_resolved_reaction_align(el, want);
    }
}

fn reaction_bar_chrome(chips: Vec<Element>) -> BoxBuilder {
    hstack()
        .w("auto")
        .flex_grow(0.0)
        .flex_shrink(0.0)
        .align_self_kw("flex-start")
        // Same clip box as +N `reaction_expand_chrome`. Hover:scale-125 grows
        // ~2px into the 6px pad; overflow:hidden dest-clips to the pill vis
        // (chip + pad) instead of dest-resetting an overflow:visible rest rect.
        // Collapsed stays in-flow — no portal, no overlay attr, no z-index.
        .overflow("hidden")
        .items("flex-start")
        .justify("flex-start")
        .gap(tokens::SPACE_1)
        .p(REACTION_BAR_PAD)
        .rounded(REACTION_BAR_RADIUS)
        .bg(parse_color("muted"))
        .outline(parse_color("card"), 3.0)
        .children(chips)
}

fn reaction_expand_chrome(
    chips: Vec<Element>,
    side: BubbleReactionsSide,
    expand_class: Option<&str>,
) -> Element {
    // Top: wrap-reverse keeps the first chip row on the trigger-facing (bottom) edge while
    // accordion max-height clips overflow at the top — unveil grows up, chips do not slide.
    let wrap = match side {
        BubbleReactionsSide::Bottom => "wrap",
        BubbleReactionsSide::Top => "wrap-reverse",
    };
    let mut el: Element = reaction_bar_chrome(chips)
        .wrap(wrap)
        .justify("flex-start")
        .items("flex-start")
        .max_w(REACTION_EXPAND_MAX_W)
        .overflow("hidden")
        .into();
    if let Some(c) = expand_class.map(str::trim).filter(|s| !s.is_empty()) {
        el = with_class_list(el, c);
    }
    el
}

fn overflow_reaction_chip(count: u32, _expanded: Signal<bool>, reactions_id: String) -> Element {
    let open_id = reactions_id.clone();
    let click_id = reactions_id.clone();
    interactive(reaction_overflow_chip_shell(count))
        .on_click(move || open_reaction_expand(&click_id))
        .on_hover(move |h| {
            note_reactions_hover(&reactions_id, HoverPart::Bar, h);
            if h {
                open_reaction_expand(&open_id);
            }
        })
        .into()
}

fn collapsed_reaction_slots_for(visible: usize, total: usize) -> usize {
    if total <= visible {
        total.max(1)
    } else {
        visible + 1
    }
}

fn collapsed_reaction_bar_width_for(visible: usize, total: usize) -> f32 {
    let chip = IntoF32::into_f32(REACTION_CHIP_SIZE);
    let gap = IntoF32::into_f32(tokens::SPACE_1);
    let pad = 2.0 * IntoF32::into_f32(REACTION_BAR_PAD);
    let n = collapsed_reaction_slots_for(visible, total) as f32;
    pad + n * chip + (n - 1.0).max(0.0) * gap
}

fn effective_visible_count(
    total: usize,
    max_visible: usize,
    avail_w: Option<f32>,
    width_for: impl Fn(usize, usize) -> f32,
) -> usize {
    if total == 0 {
        return 0;
    }
    let cap = max_visible.min(total);
    let Some(w) = avail_w.filter(|w| *w > 0.0) else {
        return cap;
    };
    for visible in (1..=cap).rev() {
        if width_for(visible, total) <= w + 0.5 {
            return visible;
        }
    }
    1
}

fn collapsed_reaction_chips(
    all: &[String],
    visible: usize,
    expanded_sig: Signal<bool>,
    reactions_id: &str,
) -> Vec<Element> {
    if all.len() <= visible {
        return all.iter().map(|t| reaction_chip_shell(t.clone())).collect();
    }
    let hidden = all.len() - visible;
    let mut chips: Vec<Element> = all[..visible]
        .iter()
        .map(|t| reaction_chip_shell(t.clone()))
        .collect();
    chips.push(overflow_reaction_chip(
        hidden as u32,
        expanded_sig,
        reactions_id.to_string(),
    ));
    chips
}

fn overlay_reaction_chips(all: &[String]) -> Vec<Element> {
    all.iter().map(|t| reaction_chip_shell(t.clone())).collect()
}

struct BubbleReactionsEntry {
    focus_id: String,
    all_items: Vec<String>,
    max_visible: usize,
    rendered_visible: Cell<usize>,
    expanded: Signal<bool>,
    last_expanded: Cell<bool>,
    side: Cell<BubbleReactionsSide>,
    align: Cell<BubbleReactionsAlign>,
    /// Optional utility class for the expand portal chrome (default = reaction bar paint).
    expand_class: String,
    bar_hover: Cell<bool>,
    panel_hover: Cell<bool>,
    hover_armed: Cell<bool>,
}

thread_local! {
    static BUBBLE_REACTIONS: RefCell<HashMap<String, BubbleReactionsEntry>> =
        RefCell::new(HashMap::new());
    static CONTENT_PACK_CHROME: RefCell<HashMap<u64, SlotChrome>> = RefCell::new(HashMap::new());
    static CONTENT_PACK_CHROME_SEQ: Cell<u64> = const { Cell::new(1) };
}

const ATTR_PACK_CHROME: &str = "jadix_bubble_pack_chrome";

fn stash_content_pack_chrome(chrome: SlotChrome) -> Option<u64> {
    if chrome.class.trim().is_empty() && chrome.style.is_none() {
        return None;
    }
    let id = CONTENT_PACK_CHROME_SEQ.with(|c| {
        let n = c.get();
        c.set(n + 1);
        n
    });
    CONTENT_PACK_CHROME.with(|m| m.borrow_mut().insert(id, chrome));
    Some(id)
}

fn take_content_pack_chrome(el: &Element) -> SlotChrome {
    let Some(id) = el
        .attrs
        .get(ATTR_PACK_CHROME)
        .and_then(|s| s.parse::<u64>().ok())
    else {
        return SlotChrome::default();
    };
    CONTENT_PACK_CHROME
        .with(|m| m.borrow_mut().remove(&id))
        .unwrap_or_default()
}

fn register_bubble_reactions_entry(
    id: String,
    focus_id: String,
    all_items: Vec<String>,
    max_visible: usize,
    rendered_visible: usize,
    expanded: Signal<bool>,
    side: BubbleReactionsSide,
    align: BubbleReactionsAlign,
    expand_class: String,
) {
    BUBBLE_REACTIONS.with(|map| {
        map.borrow_mut().insert(
            id,
            BubbleReactionsEntry {
                focus_id,
                all_items,
                max_visible,
                rendered_visible: Cell::new(rendered_visible),
                expanded,
                last_expanded: Cell::new(false),
                side: Cell::new(side),
                align: Cell::new(align),
                expand_class,
                bar_hover: Cell::new(false),
                panel_hover: Cell::new(false),
                hover_armed: Cell::new(false),
            },
        );
    });
}

fn unregister_bubble_reactions(id: &str) {
    BUBBLE_REACTIONS.with(|map| {
        map.borrow_mut().remove(id);
    });
}

#[derive(Clone, Copy)]
enum HoverPart {
    Bar,
    Panel,
}

fn note_reactions_hover(id: &str, part: HoverPart, on: bool) {
    BUBBLE_REACTIONS.with(|m| {
        let map = m.borrow();
        let Some(e) = map.get(id) else {
            return;
        };
        match part {
            HoverPart::Bar => e.bar_hover.set(on),
            HoverPart::Panel => e.panel_hover.set(on),
        }
        if on {
            e.hover_armed.set(true);
        }
    });
}

/// Open this expand and dismiss every other reaction popover (portal_sync exclusive
/// is not enough: a stale panel can sit on the hoist layer after id/handler rebuilds).
fn open_reaction_expand(id: &str) {
    let (self_exp, others): (Option<Signal<bool>>, Vec<Signal<bool>>) =
        BUBBLE_REACTIONS.with(|m| {
            let map = m.borrow();
            let mut others = Vec::new();
            let mut self_exp = None;
            for (other_id, e) in map.iter() {
                if other_id == id {
                    e.hover_armed.set(true);
                    self_exp = Some(e.expanded);
                    continue;
                }
                if e.expanded.peek() {
                    e.hover_armed.set(false);
                    e.bar_hover.set(false);
                    e.panel_hover.set(false);
                    others.push(e.expanded);
                }
            }
            (self_exp, others)
        });
    if let Some(s) = self_exp {
        s.try_set(true);
    }
    for s in others {
        s.try_set(false);
    }
}

fn dismiss_idle_reaction_hover(id: &str) -> bool {
    let expanded = BUBBLE_REACTIONS.with(|entries| {
        let entries = entries.borrow();
        let entry = entries.get(id)?;
        if !entry.expanded.peek()
            || !entry.hover_armed.get()
            || entry.bar_hover.get()
            || entry.panel_hover.get()
        {
            return None;
        }
        entry.hover_armed.set(false);
        Some(entry.expanded)
    });
    expanded.is_some_and(|expanded| expanded.peek() && expanded.try_set(false))
}

#[cfg(all(test, feature = "product-tests"))]
fn dismiss_idle_reaction_hovers() -> bool {
    let mut to_close: Vec<Signal<bool>> = Vec::new();
    BUBBLE_REACTIONS.with(|m| {
        for e in m.borrow().values() {
            if !e.expanded.peek() || !e.hover_armed.get() {
                continue;
            }
            if e.bar_hover.get() || e.panel_hover.get() {
                continue;
            }
            e.hover_armed.set(false);
            to_close.push(e.expanded);
        }
    });
    let mut closed = false;
    for s in to_close {
        if s.peek() && s.try_set(false) {
            closed = true;
        }
    }
    closed
}

fn register_bubble_reactions_redraw(reactions_id: &str, expanded: Signal<bool>) {
    let key = format!("{reactions_id}__redraw");
    use_signal(key.as_str(), move || {
        create_effect(move || {
            let _ = expanded.get();
            crate::app::runtime::request_redraw();
        });
        ()
    });
}

fn reaction_expand_placement(side: BubbleReactionsSide) -> PopupPlacement {
    match side {
        BubbleReactionsSide::Bottom => PopupPlacement::BottomStart,
        BubbleReactionsSide::Top => PopupPlacement::TopStart,
    }
}

/// Collapsed reaction bar height (chip + vertical padding).
fn reaction_collapsed_bar_height() -> f32 {
    IntoF32::into_f32(REACTION_CHIP_SIZE) + 2.0 * IntoF32::into_f32(REACTION_BAR_PAD)
}

fn collapsed_reaction_bar_width(item_count: usize, visible: usize) -> f32 {
    collapsed_reaction_bar_width_for(visible, item_count)
}

/// Negative gap ≈ collapsed bar height so the portal panel covers the pill in place.
fn reaction_expand_cover_gap() -> f32 {
    -reaction_collapsed_bar_height()
}

fn build_reaction_expand_panel(
    _floating_id: &str,
    _trigger_fid: &str,
    side: BubbleReactionsSide,
    align: BubbleReactionsAlign,
    chips: Vec<Element>,
    visible: usize,
    expand_class: Option<&str>,
) -> Element {
    let from_w = collapsed_reaction_bar_width(chips.len(), visible);
    let mut panel = reaction_expand_chrome(chips, side, expand_class);
    panel.set_attr(ATTR_REACTIONS_OVERLAY, "1");
    panel.set_attr(
        ATTR_FLOATING_EXPAND_FROM_H,
        format!("{}", reaction_collapsed_bar_height()),
    );
    panel.set_attr(ATTR_FLOATING_EXPAND_FROM_W, format!("{from_w}"));
    panel.set_attr(ATTR_SIDE, side.as_str());
    panel.set_attr(ATTR_ALIGN, align.as_str());
    // wrap-reverse already packs the first line to the trigger edge; keep cross-start.
    panel.align_content_kw("flex-start")
}

fn find_reactions_inner_mut(el: &mut Element) -> Option<&mut Element> {
    for child in &mut el.children {
        if child.attrs.get(ATTR_REACTIONS_INNER).map(|s| s.as_str()) == Some("1") {
            return Some(child);
        }
    }
    None
}

fn rebuild_reactions_inner(el: &mut Element, id: &str, visible: usize) {
    let Some(inner) = find_reactions_inner_mut(el) else {
        return;
    };
    let (chips, expanded) = BUBBLE_REACTIONS.with(|m| {
        let map = m.borrow();
        let Some(entry) = map.get(id) else {
            return (Vec::new(), None);
        };
        (
            collapsed_reaction_chips(&entry.all_items, visible, entry.expanded, id),
            Some(entry.expanded),
        )
    });
    let Some(expanded) = expanded else {
        return;
    };
    let _ = expanded;
    let mut next: Element = reaction_bar_chrome(chips).into();
    next.set_attr(ATTR_REACTIONS_INNER, "1");
    *inner = next;
}

fn sync_bubble_reactions_row(
    el: &mut Element,
    constraint_w: Option<f32>,
    layout_tree: &mut crate::layout::layout_tree::LayoutTree,
    impact: &mut LayoutStatus,
) -> bool {
    let Some(id) = el.attrs.get(ATTR_REACTIONS_ID).cloned() else {
        return false;
    };
    let mut structure_changed = false;
    BUBBLE_REACTIONS.with(|m| {
        let mut binding = m.borrow_mut();
        let Some(entry) = binding.get_mut(&id) else {
            return;
        };
        let expanded = entry.expanded.peek();
        if entry.last_expanded.get() != expanded {
            entry.last_expanded.set(expanded);
            el.set_attr("data-state", if expanded { "open" } else { "closed" });
        }
        let total = entry.all_items.len();
        let next_visible = effective_visible_count(
            total,
            entry.max_visible,
            constraint_w,
            collapsed_reaction_bar_width_for,
        );
        if next_visible != entry.rendered_visible.get() {
            entry.rendered_visible.set(next_visible);
            structure_changed = true;
        }
    });
    if structure_changed {
        rebuild_reactions_inner(el, &id, {
            BUBBLE_REACTIONS.with(|m| {
                m.borrow()
                    .get(&id)
                    .map(|e| e.rendered_visible.get())
                    .unwrap_or(0)
            })
        });
        if el.layout_node_id().is_some() {
            layout_tree.mark_element_layout_dirty(el);
        }
        *impact = impact.merge(LayoutStatus::StructureChanged);
    }
    structure_changed
}

/// 每帧把 `data-state` 与 `expanded` 对齐；指针已离开条带和浮层时收起；宽度不足时自动折叠 `+N`。
#[cfg(all(test, feature = "product-tests"))]
pub(crate) fn sync_bubble_reactions_subtree(
    el: &mut Element,
    layout_tree: &mut crate::layout::layout_tree::LayoutTree,
    impact: &mut LayoutStatus,
    viewport_w: f32,
) {
    if dismiss_idle_reaction_hovers() {
        crate::app::runtime::request_redraw();
    }
    let root_budget = (viewport_w > 0.0).then_some(viewport_w);
    walk_bubble_reactions_sync(el, root_budget, layout_tree, impact);
}

#[cfg(all(test, feature = "product-tests"))]
fn walk_bubble_reactions_sync(
    el: &mut Element,
    constraint_w: Option<f32>,
    layout_tree: &mut crate::layout::layout_tree::LayoutTree,
    impact: &mut LayoutStatus,
) {
    let mut stack: Vec<(*mut Element, Option<f32>)> = vec![(el as *mut Element, constraint_w)];
    while let Some((ptr, constraint_w)) = stack.pop() {
        let el = unsafe { &mut *ptr };
        let child_constraint = tighten_horizontal_constraint(el, constraint_w);
        if is_bubble_reactions(el) {
            let _ = sync_bubble_reactions_row(el, child_constraint, layout_tree, impact);
        }
        let n = el.children.len();
        for i in (0..n).rev() {
            stack.push((std::ptr::addr_of_mut!(el.children[i]), child_constraint));
        }
    }
}

fn apply_reactions_shell_placement(
    mut shell: BoxBuilder,
    side: BubbleReactionsSide,
    align: BubbleReactionsAlign,
    layout: BubbleReactionsLayout,
) -> BoxBuilder {
    shell = shell
        .relative(true)
        .align_self_kw(align.align_self())
        .mx(REACTION_EDGE_INSET);
    match layout {
        BubbleReactionsLayout::Row => shell,
        BubbleReactionsLayout::Overlay => {
            // Pixel translate (not %) so the pill and its glyphs share one offset.
            let dy = IntoF32::into_f32(REACTION_OVERLAY_OVERLAP);
            let taken = std::mem::replace(&mut shell.el, Element::new());
            shell.el = match side {
                BubbleReactionsSide::Top => taken.translate(0.0, dy),
                BubbleReactionsSide::Bottom => taken.translate(0.0, -dy),
            };
            shell
        }
    }
}

#[inline(never)]
fn build_bubble_reactions_element(
    side: BubbleReactionsSide,
    align: BubbleReactionsAlign,
    layout: BubbleReactionsLayout,
    expand_class: Option<String>,
    max_visible: usize,
    kids: Vec<Element>,
) -> Element {
    let seq = BUBBLE_REACTIONS_SEQ.fetch_add(1, Ordering::Relaxed);
    let reactions_id = format!("bubble_rx_{seq}");
    let focus_id = format!("{reactions_id}__focus");
    let exp_key = format!("{reactions_id}__exp");
    let expanded = use_signal(exp_key.as_str(), || false);
    let all_items = normalize_reaction_items(kids);
    let expand_class = expand_class.unwrap_or_default();
    let max_visible = max_visible.max(1);
    let rendered_visible = effective_visible_count(
        all_items.len(),
        max_visible,
        None,
        collapsed_reaction_bar_width_for,
    );

    register_bubble_reactions_entry(
        reactions_id.clone(),
        focus_id.clone(),
        all_items.clone(),
        max_visible,
        rendered_visible,
        expanded,
        side,
        align,
        expand_class,
    );
    register_bubble_reactions_redraw(&reactions_id, expanded);

    let mut inner: Element = reaction_bar_chrome(collapsed_reaction_chips(
        &all_items,
        rendered_visible,
        expanded,
        &reactions_id,
    ))
    .into();
    inner.set_attr(ATTR_REACTIONS_INNER, "1");

    let portal_id = reactions_id.clone();
    let trigger_fid = use_signal(format!("{reactions_id}__ao_trigger_fid").as_str(), || {
        focus_id.clone()
    });
    let expand_placeholder = with_child_owner(|| {
        provide_anchored_overlay_context(expanded, reactions_id.clone(), trigger_fid);
        let ao_opts = AnchoredOverlayContentOpts {
            placement: Some(Placement::trigger(
                focus_id.clone(),
                reaction_expand_placement(side),
                reaction_expand_cover_gap(),
                FloatingStrategy::Fixed,
            )),
            kind: OverlayKind::Popover,
            motion: FloatingMotionKind::Expand,
            mode: OverlayMode::Stacked,
            match_trigger_width: true,
            ..Default::default()
        };
        anchored_overlay_content(ao_opts, move || {
            let (chips, side, align, fid, expand_class, visible) = BUBBLE_REACTIONS.with(|m| {
                let map = m.borrow();
                match map.get(&portal_id) {
                    Some(e) => (
                        overlay_reaction_chips(&e.all_items),
                        e.side.get(),
                        e.align.get(),
                        e.focus_id.clone(),
                        e.expand_class.clone(),
                        e.rendered_visible.get(),
                    ),
                    None => (
                        Vec::new(),
                        BubbleReactionsSide::Bottom,
                        BubbleReactionsAlign::End,
                        String::new(),
                        String::new(),
                        DEFAULT_MAX_VISIBLE_REACTIONS,
                    ),
                }
            });
            let mut panel = build_reaction_expand_panel(
                &portal_id,
                &fid,
                side,
                align,
                chips,
                visible,
                Some(expand_class.as_str()).filter(|s| !s.is_empty()),
            );
            // Don't seed the 160px floating default; width matches the trigger after sync.
            panel = panel.w("auto");
            let hover_id = portal_id.clone();
            let panel_fid = format!("{hover_id}__panel");
            interactive(panel)
                .focus_id(panel_fid)
                .on_hover(move |h| note_reactions_hover(&hover_id, HoverPart::Panel, h))
                .into()
        })
    });
    let placeholder = expand_placeholder;

    let items = match align {
        BubbleReactionsAlign::Center => "center",
        _ => "start",
    };
    let shell = apply_reactions_shell_placement(
        vstack()
            .w("auto")
            .flex_shrink(0.0)
            .overflow("visible")
            .items(items)
            .gap(0.0)
            .children(vec![inner, placeholder]),
        side,
        align,
        layout,
    );
    let hover_id = reactions_id.clone();
    let mut row: Element = interactive(shell.into())
        .on_hover(move |h| note_reactions_hover(&hover_id, HoverPart::Bar, h))
        .into();

    row.set_attr(ATTR_SLOT, SLOT_REACTIONS);
    row.set_attr(ATTR_SIDE, side.as_str());
    row.set_attr(ATTR_ALIGN, align.as_str());
    row.set_attr(ATTR_LAYOUT, layout.as_str());
    row.set_attr(ATTR_REACTIONS_ID, &reactions_id);
    row.set_attr(ATTR_MAX_VISIBLE, max_visible.to_string());
    row.focus_id = Some(focus_id);
    row.set_attr(
        "data-state",
        if expanded.peek() { "open" } else { "closed" },
    );

    for phase in [
        crate::authoring::presentation::Phase::State,
        crate::authoring::presentation::Phase::AfterGeometry,
    ] {
        crate::authoring::presentation::attach(&mut row, phase, |element, layout, context| {
            let mut status = LayoutStatus::Clean;
            sync_bubble_reactions_row(element, context.width_budget, layout, &mut status);
            status
        });
    }
    let hover_id = reactions_id.clone();
    crate::authoring::deadline::attach(
        &mut row,
        crate::authoring::deadline::Visibility::Always,
        move |_| crate::authoring::deadline::Poll {
            redraw: dismiss_idle_reaction_hover(&hover_id),
            next_wake: None,
        },
    );
    let id_cleanup = reactions_id.clone();
    row = row.on_unmount(move || unregister_bubble_reactions(&id_cleanup));

    row
}

const ATTR_CONTENT_PACK: &str = "jadix_bubble_content_pack";

fn is_bubble_reactions(el: &Element) -> bool {
    el.attrs
        .get(ATTR_SLOT)
        .map(|s| s.as_str() == SLOT_REACTIONS)
        .unwrap_or(false)
}

fn build_bubble_content_element(variant: BubbleVariant, kids: Vec<Element>) -> Element {
    let (bg, text_color, border, ghost) = bubble_content_chrome(variant);
    let mut shell = vstack()
        .w("auto")
        .max_w("100%")
        .min_w(0.0)
        .flex_shrink(0.0)
        .items("start")
        .overflow("hidden");
    if ghost {
        shell = shell.px(0.0).py(0.0).rounded(0.0);
    } else {
        shell = shell
            .px(tokens::SPACE_3)
            .py(tokens::SPACE_2)
            .rounded("1rem");
    }
    let mut el: Element = shell.bg(bg).children(kids).into();
    if let Some(b) = border {
        el = el.border_style("solid").border_color(b).border_width(1.0);
    }
    el.set_attr(ATTR_SLOT, SLOT_CONTENT);
    el.set_attr(ATTR_VARIANT, variant.as_str());
    el.set_attr(ATTR_BUBBLE_VARIANT, variant.as_str());
    el.set_attr(ATTR_BUBBLE_VARIANT_LEGACY, variant.as_str());
    el = crate::style::apply_stylesheet_rules(el);
    for child in &mut el.children {
        if let Some(text_content) = child.text.take() {
            *child = build_text_node(
                text_content,
                TextNodeConfig {
                    size_px: IntoF32::into_f32("0.875rem"),
                    weight: 400,
                    color: text_color,
                    ..Default::default()
                },
            );
        }
    }
    // 嵌套正文（Label / 多段）不在直接子节点上，仍要吃到变体前景色。
    paint_descendant_text(&mut el, text_color);
    el
}

fn finalize_bubble_children(variant: BubbleVariant, kids: Vec<Element>) -> Vec<Element> {
    let mut out: Vec<Element> = kids
        .into_iter()
        .map(|c| {
            if is_bubble_content(&c) || is_bubble_reactions(&c) {
                c
            } else if c.attrs.get(ATTR_CONTENT_PACK).map(|s| s.as_str()) == Some("1") {
                let chrome = take_content_pack_chrome(&c);
                // Pack 只是搬运壳。单子节点时 take_packed 不会拆开，字色就停在壳上，
                // default（primary 底）会变成全黑、正文不可见。
                let mut pack = c;
                let kids = std::mem::take(&mut pack.children);
                chrome.apply(build_bubble_content_element(variant, kids))
            } else {
                build_bubble_content_element(variant, vec![c])
            }
        })
        .collect();
    // Row + top：提到最前，作为独立行坐在气泡上方。
    // Overlay + top：留在最后，配合 column-reverse，后绘所以盖住气泡边。
    if let Some(i) = out.iter().position(|c| {
        is_bubble_reactions(c)
            && c.attrs.get(ATTR_SIDE).map(|s| s.as_str()) == Some("top")
            && c.attrs.get(ATTR_LAYOUT).map(|s| s.as_str()) != Some("overlay")
    }) {
        let r = out.remove(i);
        out.insert(0, r);
    }
    out
}

fn apply_content_chrome(el: &mut Element, variant: BubbleVariant) {
    let (bg, text_color, border, ghost) = bubble_content_chrome(variant);
    el.style.bg_color = bg;
    if ghost {
        el.style.border_width = 0.0;
        el.style.border_color = [0.0, 0.0, 0.0, 0.0];
    } else if let Some(b) = border {
        el.style.border_color = b;
        el.style.border_width = 1.0;
    } else {
        el.style.border_width = 0.0;
        el.style.border_color = [0.0, 0.0, 0.0, 0.0];
    }
    el.set_attr(ATTR_VARIANT, variant.as_str());
    el.set_attr(ATTR_BUBBLE_VARIANT, variant.as_str());
    el.set_attr(ATTR_BUBBLE_VARIANT_LEGACY, variant.as_str());
    paint_descendant_text(el, text_color);
}

fn is_bubble_content(el: &Element) -> bool {
    el.attrs
        .get(ATTR_SLOT)
        .map(|s| s.as_str() == SLOT_CONTENT)
        .unwrap_or(false)
}

fn repaint_bubble_contents(root: &mut Element, variant: BubbleVariant) {
    for child in &mut root.children {
        if is_bubble_content(child) {
            apply_content_chrome(child, variant);
        }
    }
}

fn apply_bubble_root_align(el: &mut Element, align: BubbleAlign) {
    el.set_attr(ATTR_ALIGN, align.as_str());
    let taken = std::mem::replace(el, Element::new());
    *el = taken.align_self_kw(align.align_self());
}

fn overlay_reactions_side(kids: &[Element]) -> Option<BubbleReactionsSide> {
    kids.iter().find_map(|c| {
        if !is_bubble_reactions(c) {
            return None;
        }
        if c.attrs.get(ATTR_LAYOUT).map(|s| s.as_str()) != Some("overlay") {
            return None;
        }
        Some(BubbleReactionsSide::parse(
            c.attrs
                .get(ATTR_SIDE)
                .map(|s| s.as_str())
                .unwrap_or("bottom"),
        ))
    })
}

fn build_bubble_root(
    variant: BubbleVariant,
    align: BubbleAlign,
    mut kids: Vec<Element>,
) -> Element {
    resolve_inherited_reaction_align(&mut kids, align);
    let ghost = variant == BubbleVariant::Ghost;
    let max_w = if ghost { "100%" } else { "80%" };
    let mut shell = vstack()
        .relative(true)
        .w("auto")
        .max_w(max_w)
        .min_w(0.0)
        .flex_shrink(0.0)
        .overflow("visible")
        .items("start")
        .gap(tokens::SPACE_1)
        .align_self_kw(align.align_self());
    let overlay_side = overlay_reactions_side(&kids);
    if overlay_side.is_some() {
        // Overlay 用像素 translate 咬边，不占负 margin（Taffy 会把列高算成 0）。
        shell = shell.gap(0.0);
    }
    let mut el: Element = shell.children(kids).into();
    if overlay_side == Some(BubbleReactionsSide::Top) {
        // 树序仍是 content → reactions（后绘盖住气泡），视觉上 reactions 在顶。
        el.style.flex_direction = taffy::style::FlexDirection::ColumnReverse;
    }
    el.set_attr(ATTR_SLOT, SLOT_BUBBLE);
    el.set_attr(ATTR_VARIANT, variant.as_str());
    el.set_attr(ATTR_ALIGN, align.as_str());
    el.set_attr(ATTR_BUBBLE_VARIANT, variant.as_str());
    el.set_attr(ATTR_BUBBLE_VARIANT_LEGACY, variant.as_str());
    crate::style::apply_stylesheet_rules(el)
}

/// 将 Bubble 变体 chroming 应用到根与其 [`BubbleContent`] 子树。
pub fn apply_bubble_variant_paint(el: &mut Element, variant: impl AsRef<str>) -> LayoutStatus {
    let variant = BubbleVariant::from(variant.as_ref());
    let prev = el
        .attrs
        .get(ATTR_BUBBLE_VARIANT)
        .map(|s| s.as_str())
        .unwrap_or("default");
    if prev == variant.as_str() {
        return LayoutStatus::Clean;
    }
    el.set_attr(ATTR_VARIANT, variant.as_str());
    el.set_attr(ATTR_BUBBLE_VARIANT, variant.as_str());
    el.set_attr(ATTR_BUBBLE_VARIANT_LEGACY, variant.as_str());
    if variant == BubbleVariant::Ghost {
        let taken = std::mem::replace(el, Element::new());
        *el = taken.max_w("100%");
    }
    repaint_bubble_contents(el, variant);
    LayoutStatus::StyleChanged
}

/// Solid-style in-place bind for `<Bubble variant={sig.get()}>`.
pub fn apply_bound_bubble_variant(el: &mut Element, variant: impl AsRef<str>) -> LayoutStatus {
    apply_bubble_variant_paint(el, variant)
}

/// Solid-style in-place bind for `<Bubble align={sig.get()}>`.
pub fn apply_bound_bubble_align(el: &mut Element, align: impl AsRef<str>) -> LayoutStatus {
    let align = BubbleAlign::from(align.as_ref());
    let prev = el
        .attrs
        .get(ATTR_ALIGN)
        .map(|s| s.as_str())
        .unwrap_or("start");
    if prev == align.as_str() {
        return LayoutStatus::Clean;
    }
    apply_bubble_root_align(el, align);
    LayoutStatus::StyleChanged
}

fn take_packed(mut el: Element) -> Vec<Element> {
    if el.attrs.get("jadix_fragment").map(|s| s.as_str()) == Some("1") {
        return std::mem::take(&mut el.children);
    }
    if el.attrs.contains_key(ATTR_SLOT) {
        return vec![el];
    }
    let bare = el.text.as_ref().is_none_or(|t| t.is_empty());
    if bare && el.children.len() > 1 {
        std::mem::take(&mut el.children)
    } else {
        vec![el]
    }
}

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

/// 提供 Bubble 上下文并构建根容器（`ui!` `<Bubble>` / Builder 共用）。
#[inline(never)]
pub fn bubble_scope(
    variant: BubbleVariant,
    align: BubbleAlign,
    body: impl FnOnce() -> Element,
) -> Element {
    with_child_owner(|| {
        provide_context(bubble_context(), BubbleCtx { variant, align });
        let kids = finalize_bubble_children(variant, take_packed(body()));
        build_bubble_root(variant, align, kids)
    })
}

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

pub struct BubbleGroupBuilder {
    container: BoxBuilder,
    children: Vec<Element>,
    chrome: SlotChrome,
}

/// Builder：`BubbleGroup` 纵向分组。
pub fn bubble_group() -> BubbleGroupBuilder {
    BubbleGroupBuilder {
        container: vstack()
            .min_w(0.0)
            .w("100%")
            .overflow("visible")
            .items("start")
            .gap(tokens::SPACE_2),
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl BubbleGroupBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn children(mut self, kids: impl IntoIterator<Item = Element>) -> Self {
        for c in kids {
            self = self.child(c);
        }
        self
    }
}

impl From<BubbleGroupBuilder> for Element {
    fn from(builder: BubbleGroupBuilder) -> Self {
        let mut el: Element = builder.container.children(builder.children).into();
        el.set_attr(ATTR_SLOT, SLOT_GROUP);
        builder.chrome.apply(el)
    }
}

pub struct BubbleBuilder {
    variant: BubbleVariant,
    align: BubbleAlign,
    chrome: SlotChrome,
    children: Vec<Element>,
}

/// Builder：单条气泡根（须在 [`bubble_scope`] 外使用时请 [`BubbleBuilder::into_element`]）。
pub fn bubble() -> BubbleBuilder {
    BubbleBuilder {
        variant: BubbleVariant::Default,
        align: BubbleAlign::Start,
        chrome: SlotChrome::default(),
        children: Vec::new(),
    }
}

impl BubbleBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn variant(mut self, v: impl Into<BubbleVariant>) -> Self {
        self.variant = v.into();
        self
    }

    pub fn align(mut self, a: impl Into<BubbleAlign>) -> Self {
        self.align = a.into();
        self
    }

    pub fn text(self, content: impl Into<String>) -> Self {
        self.child(bubble_content().text(content))
    }

    pub fn content_with(
        self,
        f: impl FnOnce(BubbleContentBuilder) -> BubbleContentBuilder,
    ) -> Self {
        self.child(f(bubble_content()))
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }

    pub fn children(mut self, kids: impl IntoIterator<Item = Element>) -> Self {
        for c in kids {
            self = self.child(c);
        }
        self
    }
}

impl From<BubbleBuilder> for Element {
    fn from(builder: BubbleBuilder) -> Self {
        builder
            .chrome
            .apply(bubble_scope(builder.variant, builder.align, move || {
                let mut frag = Element::new();
                frag.set_attr("jadix_fragment", "1");
                frag.children = builder.children;
                frag
            }))
    }
}

pub struct BubbleContentBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

/// Builder：气泡内容面（须在 `<Bubble>` / [`bubble_scope`] 内，或显式 [`BubbleContent`] 标签）。
pub fn bubble_content() -> BubbleContentBuilder {
    BubbleContentBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl BubbleContentBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn text(mut self, content: impl Into<String>) -> Self {
        self.children.push(Element::new().text(content.into()));
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
}

impl From<BubbleContentBuilder> for Element {
    fn from(builder: BubbleContentBuilder) -> Element {
        let mut el = Element::new();
        el.set_attr(ATTR_CONTENT_PACK, "1");
        if let Some(id) = stash_content_pack_chrome(builder.chrome) {
            el.set_attr(ATTR_PACK_CHROME, id.to_string());
        }
        el.children = builder.children;
        el
    }
}

pub struct BubbleReactionsBuilder {
    side: BubbleReactionsSide,
    align: Option<BubbleReactionsAlign>,
    layout: BubbleReactionsLayout,
    expand_class: String,
    max_visible: usize,
    children: Vec<Element>,
    chrome: SlotChrome,
}

/// Builder：气泡反应条。
pub fn bubble_reactions() -> BubbleReactionsBuilder {
    BubbleReactionsBuilder {
        side: BubbleReactionsSide::Bottom,
        align: None,
        layout: BubbleReactionsLayout::Row,
        expand_class: String::new(),
        max_visible: DEFAULT_MAX_VISIBLE_REACTIONS,
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl BubbleReactionsBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn side(mut self, s: impl Into<BubbleReactionsSide>) -> Self {
        self.side = s.into();
        self
    }

    pub fn align(mut self, a: impl Into<BubbleReactionsAlign>) -> Self {
        self.align = Some(a.into());
        self
    }

    pub fn layout(mut self, v: impl Into<BubbleReactionsLayout>) -> Self {
        self.layout = v.into();
        self
    }

    /// Utility class for the expand portal chrome (default keeps reaction-bar paint).
    pub fn expand_class(mut self, c: impl Into<String>) -> Self {
        self.expand_class = c.into();
        self
    }

    /// 折叠前最多展示的反应数（默认 3）；宽度不足时还会自动减少。
    pub fn max_visible(mut self, n: usize) -> Self {
        self.max_visible = n.max(1);
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }

    pub fn emoji(mut self, glyph: impl Into<String>) -> Self {
        self.children.push(Element::new().text(glyph.into()));
        self
    }
}

impl From<BubbleReactionsBuilder> for Element {
    fn from(builder: BubbleReactionsBuilder) -> Self {
        let el = BubbleReactions(BubbleReactionsProps {
            side: Some(builder.side),
            align: builder.align,
            layout: Some(builder.layout),
            expand_class: Some(builder.expand_class).filter(|s| !s.is_empty()),
            max_visible: Some(builder.max_visible as i32),
            children: {
                let mut frag = Element::new();
                frag.set_attr("jadix_fragment", "1");
                frag.children = builder.children;
                Some(frag)
            },
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

crate::__component_contract! {
BubbleGroupProps [];
/// 同一发送方连续气泡分组。
#[allow(non_snake_case)]
pub fn BubbleGroup(
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    Element::from(bubble_group().children(kids))
}
}

crate::__component_contract! {
BubbleProps [];
/// RSX 备用入口（复合面请走宏 → [`bubble_scope`]）。
#[allow(non_snake_case)]
pub fn Bubble(
default [] variant: BubbleVariant,
default [] align: BubbleAlign,
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    bubble_scope(variant, align, move || {
        let mut frag = Element::new();
        frag.set_attr("jadix_fragment", "1");
        frag.children = kids;
        frag
    })
}
}

crate::__component_contract! {
BubbleContentProps [];
/// RSX 备用入口（复合面请走宏 → [`bubble_scope`]）。
#[allow(non_snake_case)]
pub fn BubbleContent(
plain [] children: Option<Element>,
) -> Element {
    let ctx = require_bubble("BubbleContent");
    let kids = children.map(take_packed).unwrap_or_default();
    build_bubble_content_element(ctx.variant, kids)
}
}

crate::__component_contract! {
BubbleReactionsProps [];
/// 气泡反应 / 快捷操作行。
///
/// - `side`：`top` / `bottom`（默认 `bottom`）
/// - `align`：`start` / `center` / `end`（及 `left` / `right`）。省略时左气泡右下、右气泡左下。
/// - `layout`：`row` 独立行占位（默认）；`overlay` 叠在气泡外侧，不挡正文。
/// - `expand_class`：展开 Portal 容器的 utility class（默认保持折叠反应条同款绘制）。
/// - `max_visible`：折叠前最多展示的反应数（默认 3）；父气泡宽度不足时自动减少并收入 `+N`。
/// - 超过可见上限时折叠为 `+N`，悬停 / 点击打开根级 Portal 浮层（与菜单 / Popover 互斥；不占文档流，可盖住邻条；底条只向下、顶条只向上）。
#[allow(non_snake_case)]
pub fn BubbleReactions(
default [] side: BubbleReactionsSide,
plain [] align: Option<BubbleReactionsAlign>,
default [] layout: BubbleReactionsLayout,
plain [] expand_class: Option<String>,
plain [] max_visible: Option<i32>,
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let max_visible = max_visible
        .map(|n| n.max(1) as usize)
        .unwrap_or(DEFAULT_MAX_VISIBLE_REACTIONS);
    let (align, inherit) = match align {
        Some(a) => (a, false),
        None => {
            if use_bubble().is_some() {
                (reactions_align_from_bubble(), false)
            } else {
                (inner_corner_align(BubbleAlign::Start), true)
            }
        }
    };
    let mut el =
        build_bubble_reactions_element(side, align, layout, expand_class, max_visible, kids);
    if inherit {
        el.set_attr(ATTR_ALIGN_INHERIT, "1");
    }
    el
}
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/bubble.rs"]
mod tests;
