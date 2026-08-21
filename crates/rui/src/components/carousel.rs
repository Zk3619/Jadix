//! # Carousel
//!
//! 对齐 solid-ui / ShadCN **Carousel**（Embla 心智，无第三方引擎）：
//!
//! ```text
//! Carousel / CarouselMulti / CarouselStack   // 三种根组件
//! ├── [Prev] Content [Next]                 // controls=outside|inside
//! └── CarouselDots（可选；Single/Stack 默认开，Multi 默认关）
//! ```
//!
//! - **Carousel**：单页满幅。
//! - **CarouselMulti**：多列列表；上一/下一环形；`endAlign=pad|flush`。
//! - **CarouselStack**：两侧堆叠 coverflow。
//! - **切换动画**：轨道 `translate` / 堆叠 transform 由弹簧或 reactive_sync 驱动。
//! - **无缝循环**：轨道首尾克隆；末项再「下一」同向滑到首项克隆后瞬移复位（非反向长扫）。
//! - **拖拽**：单页/多列视口内拖动；堆叠用按钮/键盘。
//! - **步进**：`scrollBy="slide"|"page"`；控件位 `controls="outside"|"inside"`。
//!
//! ## 用法
//! ```ignore
//! ui! {
//!     <Carousel id="one">…</Carousel>
//!     <CarouselMulti id="many" perView={2.2} scrollBy="page" endAlign="flush">…</CarouselMulti>
//!     <CarouselStack id="stack">…</CarouselStack>
//! }
//! ```

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use rui_macros::component;
use winit::event::KeyEvent;
use winit::keyboard::{Key, NamedKey};

use crate::app::pointer_events::{EventPhase, PointerEvent};
use crate::app::reactive::reactive_view;
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_signal, Context, Signal,
};
use crate::core::tree::{parse_text_color, Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::style::{use_spring, Spring, UnitSlot};
use crate::layout::dimension::{dim_length_px, CssDim};
use crate::tokens;
use crate::utils::lucide::lucide_glyph;
use crate::utils::resolve_instance_id;
use crate::utils::text_node::build_icon_text_node;

const ATTR_SLOT: &str = "data-slot";
const SLOT_CAROUSEL: &str = "carousel";
const SLOT_CONTENT: &str = "carousel-content";
const SLOT_ITEM: &str = "carousel-item";
const SLOT_PREV: &str = "carousel-previous";
const SLOT_NEXT: &str = "carousel-next";
const SLOT_DOTS: &str = "carousel-dots";
const DRAG_THRESHOLD_PX: f32 = 28.0;

/// 轨道方向（对位 ShadCN `orientation` / Embla `axis`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CarouselOrientation {
    #[default]
    Horizontal,
    Vertical,
}

impl CarouselOrientation {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "vertical" | "y" => Self::Vertical,
            _ => Self::Horizontal,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }
}

/// 导航按钮相对内容区的位置。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CarouselControls {
    /// 内容两侧外部（默认，对位 ShadCN `-left/right-12` 心智）。
    #[default]
    Outside,
    /// 叠在内容内侧。
    Inside,
}

impl CarouselControls {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "inside" | "inset" | "overlay" => Self::Inside,
            _ => Self::Outside,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Outside => "outside",
            Self::Inside => "inside",
        }
    }
}

/// 上一 / 下一 步进单位。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CarouselScrollBy {
    /// 每次移动 1 张（默认）。
    #[default]
    Slide,
    /// 每次移动一整屏（`floor(per_view).max(1)` 张）。
    Page,
}

impl CarouselScrollBy {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "page" | "pages" | "screen" => Self::Page,
            _ => Self::Slide,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Slide => "slide",
            Self::Page => "page",
        }
    }
}

/// 轮播形态（拆成不同根组件：`Carousel` / `CarouselMulti` / `CarouselStack`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CarouselKind {
    /// 单页满幅。
    #[default]
    Single,
    /// 多列列表（`per_view` peek）。
    Multi,
    /// 两侧堆叠（coverflow 心智）。
    Stack,
}

impl CarouselKind {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "multi" | "list" | "peek" => Self::Multi,
            "stack" | "coverflow" | "cover-flow" => Self::Stack,
            _ => Self::Single,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Multi => "multi",
            Self::Stack => "stack",
        }
    }

    pub fn default_dots(self) -> bool {
        !matches!(self, Self::Multi)
    }

    pub fn default_per_view(self) -> f32 {
        match self {
            Self::Multi => 2.2,
            _ => 1.0,
        }
    }
}

/// 多列 + 整页翻页时的末端对齐。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CarouselEndAlign {
    /// 尾部追加空白，便于整页翻到空槽。
    Pad,
    /// 末项贴齐内容区右缘（默认）。
    #[default]
    Flush,
}

impl CarouselEndAlign {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "pad" | "blank" | "blanks" | "spacer" => Self::Pad,
            _ => Self::Flush,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pad => "pad",
            Self::Flush => "flush",
        }
    }
}

/// 轮播上下文（复合子树 / [`use_carousel`]）。
#[derive(Clone)]
pub struct CarouselCtx {
    pub id: String,
    pub index: Signal<usize>,
    pub count: Signal<usize>,
    pub orientation: CarouselOrientation,
    pub loop_slides: bool,
    /// 一屏可见页数（`1` = 单页；`2.2` = 两页半 peek）。
    pub per_view: f32,
    pub autoplay_ms: u64,
    pub controls: CarouselControls,
    pub scroll_by: CarouselScrollBy,
    pub kind: CarouselKind,
    pub end_align: CarouselEndAlign,
    /// 是否自动注入底部进度点（显式 `<CarouselDots/>` 始终保留）。
    pub show_dots: bool,
    /// 两侧堆叠层数（默认 1 = 左右各一张）。
    pub stack_sides: u32,
    pub drag_px: Signal<f32>,
    pub viewport_len: Signal<f32>,
    pub paused: Signal<bool>,
}

fn offset_spring_key(id: &str) -> String {
    format!("{id}__offset")
}

fn offset_spring(id: &str) -> Spring {
    use_spring(&offset_spring_key(id), 0.0)
}

/// 无缝循环：弹簧先滚到首/尾克隆槽，到位后再瞬移到真实项。
///
/// 注意：此 flag 存在 `thread_local`，寿命长于组件树。`sync_offset_target` 在它存在时会
/// 直接 return——因此任何「丢掉 flag 却不 settle 弹簧 / 重挂载不清理」都会导致
/// 半途冻结，且重进页面后仍无法跟手切换。
#[derive(Clone, Copy)]
struct LoopAnim {
    target_offset: f32,
    settle_index: usize,
    settle_offset: f32,
}

thread_local! {
    static AUTOPLAY_AT: RefCell<HashMap<String, Instant>> = RefCell::new(HashMap::new());
    static LOOP_ANIM: RefCell<HashMap<String, LoopAnim>> = RefCell::new(HashMap::new());
}

fn next_autoplay_at(id: &str) -> Option<Instant> {
    AUTOPLAY_AT.with(|m| m.borrow().get(id).copied())
}

fn set_next_autoplay_at(id: &str, at: Option<Instant>) {
    AUTOPLAY_AT.with(|m| {
        let mut m = m.borrow_mut();
        match at {
            Some(t) => {
                m.insert(id.to_string(), t);
            }
            None => {
                m.remove(id);
            }
        }
    });
}

fn loop_anim(id: &str) -> Option<LoopAnim> {
    LOOP_ANIM.with(|m| m.borrow().get(id).copied())
}

fn set_loop_anim(id: &str, anim: LoopAnim) {
    LOOP_ANIM.with(|m| {
        m.borrow_mut().insert(id.to_string(), anim);
    });
}

fn take_loop_anim(id: &str) -> Option<LoopAnim> {
    LOOP_ANIM.with(|m| m.borrow_mut().remove(id))
}

/// 完成无缝段：弹簧瞬移到真实槽并更新 index，然后放开 sync。
fn finish_loop_anim(id: &str, index: Signal<usize>) {
    let Some(anim) = take_loop_anim(id) else {
        return;
    };
    offset_spring(id).snap(anim.settle_offset);
    index.set(anim.settle_index);
}

/// 挂载时清理 thread_local 残留（稳定 id 的 signal/spring 会保活，但 LOOP_ANIM/拖拽不会随树销毁）。
fn reset_carousel_ephemeral(id: &str) {
    let _ = take_loop_anim(id);
    // 丢掉无缝 flag 时若弹簧停在克隆槽，后续 sync 会从错位「滑」回真实项，看起来像自动切页。
    // 先冻在当前位置，等 Content / 视口 sync 再对齐。
    let spring = offset_spring(id);
    spring.snap(spring.peek());
    CAROUSEL_DRAG.with(|d| {
        let same = d
            .borrow()
            .as_ref()
            .map(|x| x.id == id)
            .unwrap_or(false);
        if same {
            *d.borrow_mut() = None;
            crate::app::release_pointer_capture();
        }
    });
}

/// 开始无缝弹簧：若 `Spring` 内部 target 已是目标值但 current 停在半途，
/// `set` 会 early-return 且不再积分 → maybe_finish 永远等不到。先 snap 到 current 再 set。
fn spring_animate_to(id: &str, target: f32) {
    let spring = offset_spring(id);
    let cur = spring.peek();
    if (cur - target).abs() < 0.0001 {
        spring.snap(target);
        return;
    }
    spring.snap(cur);
    spring.set(target);
}

/// 深拷贝滑块视觉树（无交互 / 无 layout node），供轨道首尾克隆。
fn clone_carousel_slide_visual(src: &Element) -> Element {
    let mut el = Element::new();
    el.style = src.style.clone();
    el.text = src.text.clone();
    el.text_spans = src.text_spans.clone();
    el.text_shape_suffix = src.text_shape_suffix.clone();
    el.text_style = src.text_style.clone();
    el.image_texture = src.image_texture.clone();
    el.class_set = src.class_set.clone();
    el.attrs = src.attrs.clone();
    el.cursor = src.cursor.clone();
    el.focus_id = None;
    el.focusable = false;
    el.template_id = None;
    el.template_hash = None;
    el.children = src
        .children
        .iter()
        .map(clone_carousel_slide_visual)
        .collect();
    el.set_attr("data-carousel-clone", "1");
    el
}

fn carousel_context() -> Context<CarouselCtx> {
    static CTX: OnceLock<Context<CarouselCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<CarouselCtx>)
}

/// 读取最近的 [`Carousel`] 上下文。
pub fn use_carousel() -> Option<CarouselCtx> {
    use_context(carousel_context())
}

fn require_carousel(part: &str) -> CarouselCtx {
    use_carousel().unwrap_or_else(|| {
        panic!("{part} must be used within <Carousel|CarouselMulti|CarouselStack> / carousel_scope");
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

fn is_carousel_item(el: &Element) -> bool {
    el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_ITEM)
}

fn lucide_chevron(name: &str) -> Element {
    // 与 Calendar / Button+Icon 一致：不强制宽高，交给父级 flex 居中到圆心。
    let glyph = lucide_glyph(name).unwrap_or_else(|| "?".into());
    build_icon_text_node(glyph)
        .text_size(16.0)
        .font_weight(400.0)
        .text_color(parse_text_color("foreground"))
        .text_leading_none()
}

fn step_px(ctx: &CarouselCtx) -> f32 {
    let len = ctx.viewport_len.peek().max(1.0);
    len / ctx.per_view.max(0.25)
}

fn page_size(ctx: &CarouselCtx) -> usize {
    ctx.per_view.floor().max(1.0) as usize
}

fn scroll_delta(ctx: &CarouselCtx) -> usize {
    match ctx.scroll_by {
        CarouselScrollBy::Slide => 1,
        CarouselScrollBy::Page => page_size(ctx),
    }
}

/// Multi / Stack 始终环形导航；Single 尊重 `loop_slides`。
fn wraps_nav(ctx: &CarouselCtx) -> bool {
    matches!(ctx.kind, CarouselKind::Multi | CarouselKind::Stack) || ctx.loop_slides
}

/// 轨道型（非 Stack）且环形：用首尾克隆做同向无缝循环。
fn track_loops(ctx: &CarouselCtx) -> bool {
    wraps_nav(ctx) && ctx.kind != CarouselKind::Stack
}

fn loop_clone_count(ctx: &CarouselCtx) -> usize {
    if !track_loops(ctx) {
        return 0;
    }
    let n = ctx.count.peek();
    if n <= 1 {
        return 0;
    }
    let hi = max_index(ctx);
    // 上一从 0→hi：前缀需覆盖真实项 hi；下一从末项越过：后缀至少一屏。
    let need_prev = n.saturating_sub(hi).max(1);
    let need_view = ctx.per_view.ceil().max(1.0) as usize;
    need_view
        .max(scroll_delta(ctx))
        .max(need_prev)
        .min(n)
}

/// Pad 模式尾部空白槽数量（让末项可停在视口左侧并露出空区）。
fn pad_spacer_count(ctx: &CarouselCtx) -> usize {
    // 无缝循环时用首尾克隆衔接，不再追加空白槽。
    if track_loops(ctx) {
        return 0;
    }
    if ctx.kind != CarouselKind::Multi || ctx.end_align != CarouselEndAlign::Pad {
        return 0;
    }
    ctx.per_view.ceil().max(1.0) as usize - 1
}

fn max_index(ctx: &CarouselCtx) -> usize {
    let n = ctx.count.peek();
    if n == 0 {
        return 0;
    }
    match ctx.kind {
        CarouselKind::Stack | CarouselKind::Single => n.saturating_sub(1),
        CarouselKind::Multi => {
            // 无缝环形时每张都可作为起点；flush/pad 只约束非循环滚动。
            if track_loops(ctx) {
                return n.saturating_sub(1);
            }
            match ctx.end_align {
                // 末项可作为视口起点（右侧靠空白槽 / 空区）。
                CarouselEndAlign::Pad => n.saturating_sub(1),
                // 末项贴齐视口右缘：最多滚到 (n - per_view)。
                CarouselEndAlign::Flush => {
                    let room = (n as f32 - ctx.per_view).max(0.0);
                    (room.ceil() as usize).min(n.saturating_sub(1))
                }
            }
        }
    }
}

fn ideal_offset(ctx: &CarouselCtx) -> f32 {
    if ctx.kind == CarouselKind::Stack {
        return 0.0;
    }
    let step = step_px(ctx);
    let cc = loop_clone_count(ctx);
    let i = ctx.index.peek().min(max_index(ctx));
    let raw = ((i + cc) as f32) * step;
    if ctx.kind == CarouselKind::Multi
        && ctx.end_align == CarouselEndAlign::Flush
        && !track_loops(ctx)
    {
        let n = ctx.count.peek() as f32;
        let max_off = (n - ctx.per_view).max(0.0) * step;
        return -raw.min(max_off);
    }
    -raw
}

fn sync_offset_target(ctx: &CarouselCtx) {
    if is_carousel_dragging_id(&ctx.id) || loop_anim(&ctx.id).is_some() {
        return;
    }
    // 视口未测到真实宽度时 step_px 用 max(vlen,1)≈1，会把弹簧 set 到约 0。
    // 克隆轨道上 offset≈0 露出的是前缀「末项克隆」；随后第一次「下一」会从 0 滑到
    // ideal(1)（跨两格），观感像多跳了一次自动切换，与 Prev/Next 单步割裂。
    if track_loops(ctx) && ctx.viewport_len.peek() <= 1.0 {
        return;
    }
    offset_spring(&ctx.id).set(ideal_offset(ctx));
}

/// 视口宽度变化：首次测量 snap 到克隆坐标系；动画进行中只更新宽度、不打断弹簧。
fn apply_viewport_len(ctx: &CarouselCtx, new_len: f32) {
    if new_len <= 1.0 {
        return;
    }
    let prev = ctx.viewport_len.peek();
    let thresh = (prev.abs() * 0.01).max(2.0);
    if prev > 1.0 && (prev - new_len).abs() <= thresh {
        return;
    }

    ctx.viewport_len.set(new_len);

    if is_carousel_dragging_id(&ctx.id) || loop_anim(&ctx.id).is_some() {
        return;
    }

    // 首次从未知 → 真实宽度：瞬时对齐克隆坐标系（避免先露尾项克隆）。
    if prev <= 1.0 && track_loops(ctx) {
        offset_spring(&ctx.id).snap(ideal_offset(ctx));
        return;
    }

    // 用户切换动画进行中：只记下新宽度供槽宽/step，勿 snap/set，否则会多一段「自动」动效。
    if offset_spring(&ctx.id).is_moving() {
        return;
    }

    sync_offset_target(ctx);
}

fn maybe_finish_loop_snap(ctx: &CarouselCtx) {
    let Some(anim) = loop_anim(&ctx.id) else {
        return;
    };
    let cur = offset_spring(&ctx.id).peek();
    if (cur - anim.target_offset).abs() <= 1.0 {
        finish_loop_anim(&ctx.id, ctx.index);
    }
}

/// 末项「下一」：继续同向滚到首项克隆，再瞬移回真实首项。
/// 动画期间保持当前 index，完成后再切到 0（避免中断时被当成已到站）。
fn begin_seamless_next(ctx: &CarouselCtx) -> bool {
    if !track_loops(ctx) {
        return false;
    }
    let n = ctx.count.peek();
    let cc = loop_clone_count(ctx);
    if n <= 1 || cc == 0 {
        return false;
    }
    let step = step_px(ctx);
    let target = -((n + cc) as f32) * step;
    let settle = -(cc as f32) * step;
    set_loop_anim(
        &ctx.id,
        LoopAnim {
            target_offset: target,
            settle_index: 0,
            settle_offset: settle,
        },
    );
    spring_animate_to(&ctx.id, target);
    true
}

/// 首项「上一」：同向滚到末项克隆，再瞬移回真实末位。
fn begin_seamless_prev(ctx: &CarouselCtx) -> bool {
    if !track_loops(ctx) {
        return false;
    }
    let n = ctx.count.peek();
    let cc = loop_clone_count(ctx);
    if n <= 1 || cc == 0 {
        return false;
    }
    let hi = max_index(ctx);
    let step = step_px(ctx);
    let prefix_pos = (hi + cc).saturating_sub(n);
    let target = -(prefix_pos as f32) * step;
    let settle = -((hi + cc) as f32) * step;
    set_loop_anim(
        &ctx.id,
        LoopAnim {
            target_offset: target,
            settle_index: hi,
            settle_offset: settle,
        },
    );
    spring_animate_to(&ctx.id, target);
    true
}

fn can_scroll_prev(ctx: &CarouselCtx) -> bool {
    let n = ctx.count.peek();
    if n <= 1 {
        return false;
    }
    if wraps_nav(ctx) {
        return true;
    }
    ctx.index.peek() > 0
}

fn can_scroll_next(ctx: &CarouselCtx) -> bool {
    let n = ctx.count.peek();
    if n <= 1 {
        return false;
    }
    if wraps_nav(ctx) {
        return true;
    }
    ctx.index.peek() < max_index(ctx)
}

fn scroll_prev(ctx: &CarouselCtx) {
    let n = ctx.count.peek();
    if n == 0 {
        return;
    }
    // 上一段无缝未完成：只收口，不连跳。
    if loop_anim(&ctx.id).is_some() {
        finish_loop_anim(&ctx.id, ctx.index);
        bump_autoplay(ctx);
        return;
    }
    let cur = ctx.index.peek();
    let hi = max_index(ctx);
    let d = scroll_delta(ctx).max(1);
    if wraps_nav(ctx) && cur == 0 {
        if begin_seamless_prev(ctx) {
            bump_autoplay(ctx);
            return;
        }
        ctx.index.set(hi);
    } else {
        ctx.index.set(cur.saturating_sub(d));
    }
    bump_autoplay(ctx);
    sync_offset_target(ctx);
}

fn scroll_next(ctx: &CarouselCtx) {
    let n = ctx.count.peek();
    if n == 0 {
        return;
    }
    if loop_anim(&ctx.id).is_some() {
        finish_loop_anim(&ctx.id, ctx.index);
        bump_autoplay(ctx);
        return;
    }
    let cur = ctx.index.peek();
    let hi = max_index(ctx);
    let d = scroll_delta(ctx).max(1);
    if wraps_nav(ctx) && cur >= hi {
        if begin_seamless_next(ctx) {
            bump_autoplay(ctx);
            return;
        }
        ctx.index.set(0);
    } else {
        ctx.index.set((cur + d).min(hi));
    }
    bump_autoplay(ctx);
    sync_offset_target(ctx);
}

fn bump_autoplay(ctx: &CarouselCtx) {
    if ctx.autoplay_ms == 0 {
        set_next_autoplay_at(&ctx.id, None);
        return;
    }
    set_next_autoplay_at(
        &ctx.id,
        Some(Instant::now() + Duration::from_millis(ctx.autoplay_ms.max(500))),
    );
}

// ── Drag ─────────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct CarouselDrag {
    id: String,
    start_ptr: f32,
    base_offset: f32,
    index: Signal<usize>,
    count: Signal<usize>,
    per_view: f32,
    wraps: bool,
    loop_clones: usize,
    end_align: CarouselEndAlign,
    kind: CarouselKind,
    viewport_len: Signal<f32>,
    drag_px: Signal<f32>,
    orientation: CarouselOrientation,
    autoplay_ms: u64,
    paused: Signal<bool>,
}

thread_local! {
    static CAROUSEL_DRAG: RefCell<Option<CarouselDrag>> = const { RefCell::new(None) };
    static AUTOPLAY_REG: RefCell<HashMap<String, CarouselCtx>> = RefCell::new(HashMap::new());
}

fn begin_carousel_drag(drag: CarouselDrag) {
    // 拖拽打断无缝段时必须 settle，不能只丢 flag（否则弹簧停在半途且 target 残留）。
    finish_loop_anim(&drag.id, drag.index);
    drag.paused.set(true);
    CAROUSEL_DRAG.with(|d| *d.borrow_mut() = Some(drag));
}

fn drag_max_index(drag: &CarouselDrag) -> usize {
    let n = drag.count.peek();
    if n == 0 {
        return 0;
    }
    match drag.kind {
        CarouselKind::Stack | CarouselKind::Single => n.saturating_sub(1),
        CarouselKind::Multi => {
            if drag.loop_clones > 0 {
                return n.saturating_sub(1);
            }
            match drag.end_align {
                CarouselEndAlign::Pad => n.saturating_sub(1),
                CarouselEndAlign::Flush => {
                    let room = (n as f32 - drag.per_view).max(0.0);
                    (room.ceil() as usize).min(n.saturating_sub(1))
                }
            }
        }
    }
}

fn end_carousel_drag() {
    CAROUSEL_DRAG.with(|d| {
        if let Some(drag) = d.borrow_mut().take() {
            let step = {
                let len = drag.viewport_len.peek().max(1.0);
                len / drag.per_view.max(0.25)
            };
            let visual = drag.base_offset + drag.drag_px.peek();
            let n = drag.count.peek().max(1);
            let hi = drag_max_index(&drag);
            let cc = drag.loop_clones;
            let track_i = if step > 1.0 {
                (-visual / step).round() as i32
            } else {
                cc as i32
            };

            // 过阈值且越过首尾：走同向克隆动画。
            if drag.wraps && cc > 0 && drag.drag_px.peek().abs() > DRAG_THRESHOLD_PX {
                let cur = drag.index.peek();
                if drag.drag_px.peek() < -DRAG_THRESHOLD_PX && cur >= hi {
                    let target = -((n + cc) as f32) * step;
                    let settle = -(cc as f32) * step;
                    set_loop_anim(
                        &drag.id,
                        LoopAnim {
                            target_offset: target,
                            settle_index: 0,
                            settle_offset: settle,
                        },
                    );
                    drag.drag_px.set(0.0);
                    spring_animate_to(&drag.id, target);
                    drag.paused.set(false);
                    if drag.autoplay_ms > 0 {
                        set_next_autoplay_at(
                            &drag.id,
                            Some(
                                Instant::now() + Duration::from_millis(drag.autoplay_ms.max(500)),
                            ),
                        );
                    }
                    return;
                }
                if drag.drag_px.peek() > DRAG_THRESHOLD_PX && cur == 0 {
                    let prefix_pos = (hi + cc).saturating_sub(n);
                    let target = -(prefix_pos as f32) * step;
                    let settle = -((hi + cc) as f32) * step;
                    set_loop_anim(
                        &drag.id,
                        LoopAnim {
                            target_offset: target,
                            settle_index: hi,
                            settle_offset: settle,
                        },
                    );
                    drag.drag_px.set(0.0);
                    spring_animate_to(&drag.id, target);
                    drag.paused.set(false);
                    if drag.autoplay_ms > 0 {
                        set_next_autoplay_at(
                            &drag.id,
                            Some(
                                Instant::now() + Duration::from_millis(drag.autoplay_ms.max(500)),
                            ),
                        );
                    }
                    return;
                }
            }

            let logical = track_i - cc as i32;
            let mut idx = if drag.wraps && cc > 0 {
                let n_i = n as i32;
                let wrapped = ((logical % n_i) + n_i) % n_i;
                (wrapped as usize).min(hi)
            } else {
                logical.clamp(0, hi as i32) as usize
            };
            // 未过拖拽阈值：视为点击，保持按下时的 index，避免动画中途松手被 round 成邻页。
            if drag.drag_px.peek().abs() <= DRAG_THRESHOLD_PX {
                idx = drag.index.peek().min(hi);
            } else if drag.wraps {
                let cur = drag.index.peek();
                if drag.drag_px.peek() < -DRAG_THRESHOLD_PX {
                    idx = if cur >= hi {
                        0
                    } else {
                        (cur + 1).min(hi)
                    };
                } else if drag.drag_px.peek() > DRAG_THRESHOLD_PX {
                    idx = if cur == 0 { hi } else { cur.saturating_sub(1) };
                }
            }
            drag.index.set(idx);
            drag.drag_px.set(0.0);
            let ideal = {
                let raw_off = ((idx + cc) as f32) * step;
                if drag.kind == CarouselKind::Multi
                    && drag.end_align == CarouselEndAlign::Flush
                    && cc == 0
                {
                    let max_off = (n as f32 - drag.per_view).max(0.0) * step;
                    -raw_off.min(max_off)
                } else if drag.kind == CarouselKind::Stack {
                    0.0
                } else {
                    -raw_off
                }
            };
            let spring = offset_spring(&drag.id);
            if cc > 0 && (track_i < cc as i32 || track_i >= (n + cc) as i32) {
                spring.snap(ideal);
            } else {
                spring.set(ideal);
            }
            drag.paused.set(false);
            if drag.autoplay_ms > 0 {
                set_next_autoplay_at(
                    &drag.id,
                    Some(Instant::now() + Duration::from_millis(drag.autoplay_ms.max(500))),
                );
            }
        }
    });
    crate::app::release_pointer_capture();
}

fn is_carousel_dragging_id(id: &str) -> bool {
    CAROUSEL_DRAG.with(|d| {
        d.borrow()
            .as_ref()
            .map(|x| x.id == id)
            .unwrap_or(false)
    })
}

/// CursorMoved：更新拖拽位移。
pub(crate) fn tick_carousel_drag(x: f32, y: f32) -> bool {
    CAROUSEL_DRAG.with(|d| {
        let Some(drag) = d.borrow().clone() else {
            return false;
        };
        let ptr = match drag.orientation {
            CarouselOrientation::Horizontal => x,
            CarouselOrientation::Vertical => y,
        };
        let delta = ptr - drag.start_ptr;
        if (drag.drag_px.peek() - delta).abs() > 0.25 {
            drag.drag_px.set(delta);
            offset_spring(&drag.id).snap(drag.base_offset + delta);
        }
        true
    })
}

pub(crate) fn clear_carousel_drag_if_any() {
    let had = CAROUSEL_DRAG.with(|d| d.borrow().is_some());
    if had {
        end_carousel_drag();
    }
}

#[allow(dead_code)]
pub(crate) fn is_carousel_dragging() -> bool {
    CAROUSEL_DRAG.with(|d| d.borrow().is_some())
}

/// 自动轮播推进；有切换时返回 `true`（需重绘）。
pub(crate) fn tick_carousel_autoplay() -> bool {
    let now = Instant::now();
    let mut advanced = false;
    AUTOPLAY_REG.with(|reg| {
        for ctx in reg.borrow().values() {
            if ctx.autoplay_ms == 0 || ctx.paused.peek() || is_carousel_dragging_id(&ctx.id) {
                continue;
            }
            if loop_anim(&ctx.id).is_some() {
                maybe_finish_loop_snap(ctx);
                advanced = true;
                continue;
            }
            let Some(at) = next_autoplay_at(&ctx.id) else {
                bump_autoplay(ctx);
                continue;
            };
            if now >= at {
                scroll_next(ctx);
                advanced = true;
            }
        }
    });
    advanced
}

/// 下一次自动轮播唤醒时刻。
pub(crate) fn carousel_next_wake_at() -> Option<Instant> {
    AUTOPLAY_REG.with(|reg| {
        reg.borrow()
            .values()
            .filter_map(|c| {
                if c.autoplay_ms == 0 || c.paused.peek() || is_carousel_dragging_id(&c.id) {
                    None
                } else {
                    next_autoplay_at(&c.id)
                }
            })
            .min()
    })
}

fn register_autoplay(ctx: &CarouselCtx) {
    if ctx.autoplay_ms == 0 {
        AUTOPLAY_REG.with(|r| {
            r.borrow_mut().remove(&ctx.id);
        });
        return;
    }
    bump_autoplay(ctx);
    AUTOPLAY_REG.with(|r| {
        r.borrow_mut().insert(ctx.id.clone(), ctx.clone());
    });
}

fn unregister_autoplay(id: &str) {
    AUTOPLAY_REG.with(|r| {
        r.borrow_mut().remove(id);
    });
    let _ = take_loop_anim(id);
}

// ── Scope ────────────────────────────────────────────────────────────────────

fn classify_carousel_kids(
    kids: Vec<Element>,
) -> (
    Option<Element>,
    Option<Element>,
    Option<Element>,
    Option<Element>,
    Vec<Element>,
) {
    let mut content = None;
    let mut prev = None;
    let mut next = None;
    let mut dots = None;
    let mut rest = Vec::new();
    for kid in kids {
        match kid.attrs.get(ATTR_SLOT).map(|s| s.as_str()) {
            Some(SLOT_CONTENT) => content = Some(kid),
            Some(SLOT_PREV) => prev = Some(kid),
            Some(SLOT_NEXT) => next = Some(kid),
            Some(SLOT_DOTS) => dots = Some(kid),
            _ => rest.push(kid),
        }
    }
    (content, prev, next, dots, rest)
}

fn place_nav_inside(
    mut nav: Element,
    orientation: CarouselOrientation,
    which: &str,
) -> Element {
    nav = nav.absolute().z_index(2);
    match (orientation, which) {
        (CarouselOrientation::Horizontal, "prev") => {
            nav.top("50%").left("0.5rem").translate_pct(0.0, -0.5)
        }
        (CarouselOrientation::Horizontal, _) => {
            nav.top("50%").right("0.5rem").translate_pct(0.0, -0.5)
        }
        (CarouselOrientation::Vertical, "prev") => nav
            .left("50%")
            .top("0.5rem")
            .translate_pct(-0.5, 0.0)
            .rotate(90.0),
        (CarouselOrientation::Vertical, _) => nav
            .left("50%")
            .bottom("0.5rem")
            .translate_pct(-0.5, 0.0)
            .rotate(90.0),
    }
}

fn assemble_carousel_chrome(
    ctx: &CarouselCtx,
    content: Option<Element>,
    prev: Option<Element>,
    next: Option<Element>,
    mut dots: Option<Element>,
    rest: Vec<Element>,
) -> Element {
    if dots.is_none() && ctx.show_dots {
        dots = Some(CarouselDots(CarouselDotsProps::default()));
    }
    let stage_body = content.unwrap_or_else(Element::new);
    let horizontal = ctx.orientation == CarouselOrientation::Horizontal;

    let mut col: Element = match ctx.controls {
        CarouselControls::Outside => {
            let mut stage: Element = r#box()
                .relative(true)
                .flex_grow(1.0)
                .min_w(0.0)
                .min_h(0.0)
                // 勿 w(100%)：与 Prev/Next 同处一行时会挤占/错算视口宽度。
                .child(stage_body)
                .into();
            stage.attrs.insert(
                ATTR_SLOT.to_string(),
                "carousel-stage".to_string(),
            );

            let mut track: Element = if horizontal {
                hstack()
                    .w("100%")
                    .items_center()
                    .gap("0.5rem")
                    .into()
            } else {
                vstack()
                    .h("100%")
                    .items_center()
                    .gap("0.5rem")
                    .into()
            };
            if let Some(p) = prev {
                track = track.child(p);
            }
            track = track.child(stage);
            if let Some(n) = next {
                track = track.child(n);
            }
            vstack().w("100%").min_w(0.0).child(track).into()
        }
        CarouselControls::Inside => {
            let mut stage: Element = r#box()
                .relative(true)
                .w("100%")
                .min_w(0.0)
                .child(stage_body)
                .into();
            if let Some(p) = prev {
                stage = stage.child(place_nav_inside(p, ctx.orientation, "prev"));
            }
            if let Some(n) = next {
                stage = stage.child(place_nav_inside(n, ctx.orientation, "next"));
            }
            vstack().w("100%").min_w(0.0).child(stage).into()
        }
    };

    if let Some(d) = dots {
        col = col.child(d);
    }
    for r in rest {
        col = col.child(r);
    }
    col
}

/// 提供 Carousel 上下文并构建子树（宏 `<Carousel>` / `<CarouselMulti>` / `<CarouselStack>` / Builder 共用）。
#[allow(clippy::too_many_arguments)]
pub fn carousel_scope(
    id: impl Into<String>,
    index: Option<Signal<usize>>,
    orientation: impl Into<String>,
    loop_slides: bool,
    per_view: f32,
    autoplay_ms: u64,
    controls: impl Into<String>,
    scroll_by: impl Into<String>,
    kind: impl Into<String>,
    end_align: impl Into<String>,
    dots: Option<bool>,
    stack_sides: u32,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = resolve_instance_id(id, "carousel");
    // thread_local 的 LOOP_ANIM / 拖拽会跨重挂载残留，而 sync 在 loop_anim 存在时直接 return。
    reset_carousel_ephemeral(&id);
    let orientation = CarouselOrientation::parse(&orientation.into());
    let controls = CarouselControls::parse(&controls.into());
    let scroll_by = CarouselScrollBy::parse(&scroll_by.into());
    let kind = CarouselKind::parse(&kind.into());
    let end_align = CarouselEndAlign::parse(&end_align.into());
    let show_dots = dots.unwrap_or_else(|| kind.default_dots());
    // Single 强制一屏一卡；Multi 用传入 / 默认 per_view。
    let per_view = match kind {
        CarouselKind::Single => 1.0,
        CarouselKind::Stack => 1.0,
        CarouselKind::Multi => {
            if per_view > 0.0 {
                per_view.clamp(0.5, 8.0)
            } else {
                kind.default_per_view()
            }
        }
    };
    let stack_sides = stack_sides.clamp(1, 5);
    let loop_slides = match kind {
        CarouselKind::Multi | CarouselKind::Stack => true,
        CarouselKind::Single => loop_slides,
    };
    let index = index.unwrap_or_else(|| use_signal(format!("{id}__idx").as_str(), || 0usize));
    let count = use_signal(format!("{id}__count").as_str(), || 0usize);
    let drag_px = use_signal(format!("{id}__drag").as_str(), || 0.0f32);
    let viewport_len = use_signal(format!("{id}__vlen").as_str(), || 0.0f32);
    let paused = use_signal(format!("{id}__paused").as_str(), || false);
    let _offset = offset_spring(&id);

    let ctx = CarouselCtx {
        id: id.clone(),
        index,
        count,
        orientation,
        loop_slides,
        per_view,
        autoplay_ms,
        controls,
        scroll_by,
        kind,
        end_align,
        show_dots,
        stack_sides,
        drag_px,
        viewport_len,
        paused,
    };
    provide_context(carousel_context(), ctx.clone());
    register_autoplay(&ctx);

    let kids = take_packed(body());
    let (content, prev, next, dots, rest) = classify_carousel_kids(kids);
    let chrome = assemble_carousel_chrome(&ctx, content, prev, next, dots, rest);

    let mut root: Element = r#box()
        .w("100%")
        .min_w(0.0)
        .relative(true)
        .flex_col()
        .child(chrome)
        .into();
    root.attrs
        .insert(ATTR_SLOT.to_string(), SLOT_CAROUSEL.to_string());
    root.attrs.insert(
        "data-orientation".to_string(),
        orientation.as_str().to_string(),
    );
    root.attrs.insert(
        "data-controls".to_string(),
        controls.as_str().to_string(),
    );
    root.attrs.insert(
        "data-scroll-by".to_string(),
        scroll_by.as_str().to_string(),
    );
    root.attrs
        .insert("data-kind".to_string(), kind.as_str().to_string());
    root.attrs.insert(
        "data-end-align".to_string(),
        end_align.as_str().to_string(),
    );
    root.attrs.insert(
        "data-dots".to_string(),
        if show_dots { "1" } else { "0" }.to_string(),
    );
    root = root.accesskit_role("region");
    root.set_attr("aria-roledescription", "carousel");
    root.template_id = Some(id.clone());

    let unreg_id = id.clone();
    root = root.on_unmount(move || {
        unregister_autoplay(&unreg_id);
    });

    let focus_id = format!("{id}__carousel_root");
    let ctx_key = ctx.clone();
    root = interactive(root)
        .focus_id(focus_id)
        .focusable(true)
        .on_hover({
            let paused = ctx.paused;
            let autoplay_ms = ctx.autoplay_ms;
            let id = ctx.id.clone();
            move |h| {
                if autoplay_ms == 0 {
                    return;
                }
                paused.set(h);
                if !h {
                    set_next_autoplay_at(
                        &id,
                        Some(Instant::now() + Duration::from_millis(autoplay_ms.max(500))),
                    );
                }
            }
        })
        .on_key_down({
            let ctx = ctx_key;
            move |ev: KeyEvent| {
                let horizontal = ctx.orientation == CarouselOrientation::Horizontal;
                match &ev.logical_key {
                    Key::Named(NamedKey::ArrowLeft) if horizontal => {
                        scroll_prev(&ctx);
                        true
                    }
                    Key::Named(NamedKey::ArrowRight) if horizontal => {
                        scroll_next(&ctx);
                        true
                    }
                    Key::Named(NamedKey::ArrowUp) if !horizontal => {
                        scroll_prev(&ctx);
                        true
                    }
                    Key::Named(NamedKey::ArrowDown) if !horizontal => {
                        scroll_next(&ctx);
                        true
                    }
                    _ => false,
                }
            }
        })
        .into();
    root
}

// ── RSX parts ────────────────────────────────────────────────────────────────

fn signed_ring_dist(i: usize, cur: usize, n: usize) -> i32 {
    if n == 0 {
        return 0;
    }
    let raw = i as i32 - cur as i32;
    let half = (n as i32) / 2;
    if raw > half {
        raw - n as i32
    } else if raw < -half {
        raw + n as i32
    } else {
        raw
    }
}

fn build_stack_content(ctx: &CarouselCtx, items: Vec<Element>) -> Element {
    use crate::core::tree::parse_color;

    let mut stage: Element = r#box()
        .relative(true)
        .w("100%")
        .h("11rem")
        .min_h("11rem")
        .overflow("hidden")
        .into();

    for (i, mut card) in items.into_iter().enumerate() {
        // 不透明底板，避免下层内容透出。
        card = card
            .absolute()
            .w("56%")
            .max_w("18rem")
            .left("50%")
            .top("50%")
            .translate_pct(-0.5, -0.5)
            .px(0.0)
            .flex_basis("auto")
            .bg(parse_color("background"))
            .overflow("hidden");
        let sync_ctx = ctx.clone();
        let idx = i;
        card = card.reactive_sync(move |el: &mut Element, _lt: &mut LayoutTree| {
            let cur = sync_ctx.index.peek();
            let count = sync_ctx.count.peek().max(1);
            let dist = signed_ring_dist(idx, cur, count);
            let ad = dist.abs();
            let sides = sync_ctx.stack_sides.max(1) as i32;
            let visible = ad <= sides;
            // 侧向错开；中间项 tx=0。
            let tx = dist as f32 * 64.0;
            let sc = if dist == 0 {
                1.0
            } else {
                (1.0 - 0.12 * ad as f32).clamp(0.72, 1.0)
            };
            // 侧卡保持不透明，仅超出 stack_sides 的隐藏，避免透出。
            let op = if visible { 1.0 } else { 0.0 };
            // 中间永远最高；两侧按距离递减。
            let z = if dist == 0 {
                100
            } else if visible {
                100 - ad
            } else {
                0
            };
            let rot = dist as f32 * -6.0;
            let mut changed = false;
            if (el.style.translate[0] - tx).abs() > 0.5 {
                el.style.translate[0] = tx;
                changed = true;
            }
            if (el.style.scale[0] - sc).abs() > 0.01 {
                el.style.scale = [sc, sc];
                changed = true;
            }
            if (el.style.opacity - op).abs() > 0.01 {
                el.style.opacity = op;
                changed = true;
            }
            if el.style.z_index != z {
                el.style.z_index = z;
                changed = true;
            }
            if (el.style.rotate_deg - rot).abs() > 0.5 {
                el.style.rotate_deg = rot;
                changed = true;
            }
            if changed {
                crate::app::damage::push_frame_paint_damage(el.layout_bounds);
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        });
        stage = stage.child(card);
    }

    stage
        .attrs
        .insert(ATTR_SLOT.to_string(), SLOT_CONTENT.to_string());
    stage.template_id = Some(format!("{}:content", ctx.id));
    stage
}

/// solid-ui / ShadCN `CarouselContent`：`overflow-hidden` + 弹簧轨道（可拖拽）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn CarouselContent(children: Option<Element>) -> Element {
    let ctx = require_carousel("CarouselContent");
    let mut kids = children.map(take_packed).unwrap_or_default();
    let mut items = Vec::new();
    for kid in kids.drain(..) {
        if is_carousel_item(&kid) {
            items.push(kid);
        } else {
            for n in take_packed(kid) {
                if is_carousel_item(&n) {
                    items.push(n);
                }
            }
        }
    }
    let n = items.len();
    ctx.count.set(n);
    if n > 0 {
        let hi = max_index(&ctx);
        if ctx.index.peek() > hi {
            ctx.index.set(hi);
        }
    }
    // 视口未就绪时 step≈1，set 会把弹簧拉向假目标；克隆轨道应等真实宽度再对齐。
    if ctx.viewport_len.peek() > 1.0 {
        sync_offset_target(&ctx);
    }

    if ctx.kind == CarouselKind::Stack {
        return build_stack_content(&ctx, items);
    }

    let cc = loop_clone_count(&ctx);
    let mut track_items: Vec<Element> = Vec::with_capacity(items.len() + cc * 2);
    if cc > 0 {
        let n = items.len();
        for src in &items[n - cc..] {
            track_items.push(clone_carousel_slide_visual(src));
        }
    }
    let suffix: Vec<Element> = if cc > 0 {
        items[..cc]
            .iter()
            .map(clone_carousel_slide_visual)
            .collect()
    } else {
        Vec::new()
    };
    track_items.extend(items);
    track_items.extend(suffix);

    // Pad：尾部追加空白槽（无缝循环时已禁用）。
    let spacers = pad_spacer_count(&ctx);
    for _ in 0..spacers {
        let mut spacer: Element = r#box()
            .min_w(0.0)
            .flex_shrink(0.0)
            .flex_grow(0.0)
            .into();
        spacer.set_attr("data-carousel-spacer", "1");
        track_items.push(spacer);
    }

    let items: Vec<Element> = track_items
        .into_iter()
        .map(|mut el| {
            let sync_ctx = ctx.clone();
            let horizontal = ctx.orientation == CarouselOrientation::Horizontal;
            // 清掉 Item 默认的 % width/basis；否则 to_taffy_style 每帧用 unit_specs 盖掉像素宽。
            el.style.set_css_dim(UnitSlot::Width, CssDim::Auto);
            el.style.set_css_dim(UnitSlot::FlexBasis, CssDim::Auto);
            el = el
                .min_w(0.0)
                .flex_shrink(0.0)
                .flex_grow(0.0)
                .reactive_sync(move |node: &mut Element, _lt: &mut LayoutTree| {
                    let vp = sync_ctx.viewport_len.get();
                    if vp <= 1.0 {
                        return LayoutStatus::Clean;
                    }
                    let slot = vp / sync_ctx.per_view.max(0.25);
                    let cur = if horizontal {
                        dim_length_px(node.style.size.width).unwrap_or(0.0)
                    } else {
                        dim_length_px(node.style.size.height).unwrap_or(0.0)
                    };
                    if (cur - slot).abs() <= 0.5 {
                        return LayoutStatus::Clean;
                    }
                    if horizontal {
                        node.style.set_css_dim(UnitSlot::Width, CssDim::Px(slot));
                        node.style
                            .set_css_dim(UnitSlot::FlexBasis, CssDim::Px(slot));
                    } else {
                        node.style.set_css_dim(UnitSlot::Height, CssDim::Px(slot));
                        node.style
                            .set_css_dim(UnitSlot::FlexBasis, CssDim::Px(slot));
                    }
                    LayoutStatus::LayoutChanged
                });
            el
        })
        .collect();

    let mut track: Element = match ctx.orientation {
        CarouselOrientation::Horizontal => hstack()
            .min_w(0.0)
            .items("stretch")
            .children(items)
            .into(),
        CarouselOrientation::Vertical => vstack()
            .w("100%")
            .min_w(0.0)
            .children(items)
            .into(),
    };

    let track_ctx = ctx.clone();
    track = track.reactive_sync(move |el, _layout_tree: &mut LayoutTree| {
        let _ = track_ctx.index.peek();
        let _ = track_ctx.drag_px.peek();
        let _ = track_ctx.viewport_len.peek();
        maybe_finish_loop_snap(&track_ctx);
        sync_offset_target(&track_ctx);
        // 订阅弹簧，动画帧持续刷新（须在 snap 之后读取）。
        let x_or_y = offset_spring(&track_ctx.id).get();
        let want = match track_ctx.orientation {
            CarouselOrientation::Horizontal => [x_or_y, 0.0],
            CarouselOrientation::Vertical => [0.0, x_or_y],
        };
        let hit = (el.style.translate[0] - want[0]).abs() > 0.05
            || (el.style.translate[1] - want[1]).abs() > 0.05;
        if hit {
            el.style.translate = want;
            crate::app::damage::push_frame_paint_damage(el.layout_bounds);
            return LayoutStatus::StyleChanged;
        }
        LayoutStatus::Clean
    });

    let mut viewport: Element = r#box()
        .w("100%")
        .min_w(0.0)
        .overflow("hidden")
        .child(track)
        .into();
    viewport
        .attrs
        .insert(ATTR_SLOT.to_string(), SLOT_CONTENT.to_string());
    viewport.template_id = Some(format!("{}:content", ctx.id));
    viewport = viewport.cursor_kind(crate::core::tree::CursorKind::Grab);

    let vp_ctx = ctx.clone();
    viewport = viewport.reactive_sync(move |el, _lt: &mut LayoutTree| {
        let len = match vp_ctx.orientation {
            CarouselOrientation::Horizontal => el.layout_bounds[2],
            CarouselOrientation::Vertical => el.layout_bounds[3],
        };
        let prev = vp_ctx.viewport_len.peek();
        let thresh = (prev.abs() * 0.01).max(2.0);
        if len > 1.0 && (prev - len).abs() > thresh {
            apply_viewport_len(&vp_ctx, len);
            return LayoutStatus::LayoutChanged;
        }
        LayoutStatus::Clean
    });

    let capture_id = format!("{}:content", ctx.id);
    let down_ctx = ctx.clone();
    let capture_for_down = capture_id.clone();
    viewport = viewport.on_pointer_down(move |e: &PointerEvent| {
        if e.phase == EventPhase::Capture {
            return;
        }
        let ptr = match down_ctx.orientation {
            CarouselOrientation::Horizontal => e.x,
            CarouselOrientation::Vertical => e.y,
        };
        begin_carousel_drag(CarouselDrag {
            id: down_ctx.id.clone(),
            start_ptr: ptr,
            base_offset: offset_spring(&down_ctx.id).peek(),
            index: down_ctx.index,
            count: down_ctx.count,
            per_view: down_ctx.per_view,
            wraps: wraps_nav(&down_ctx),
            loop_clones: loop_clone_count(&down_ctx),
            end_align: down_ctx.end_align,
            kind: down_ctx.kind,
            viewport_len: down_ctx.viewport_len,
            drag_px: down_ctx.drag_px,
            orientation: down_ctx.orientation,
            autoplay_ms: down_ctx.autoplay_ms,
            paused: down_ctx.paused,
        });
        crate::app::set_pointer_capture(capture_for_down.clone());
        e.stop_propagation();
    });
    viewport = viewport.on_pointer_up(move |e: &PointerEvent| {
        if e.phase == EventPhase::Capture {
            return;
        }
        clear_carousel_drag_if_any();
    });

    viewport
}

/// solid-ui / ShadCN `CarouselItem`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn CarouselItem(children: Option<Element>) -> Element {
    let ctx = require_carousel("CarouselItem");
    let kids = children.map(take_packed).unwrap_or_default();
    // 默认单页满宽；Content 会按 per_view 覆盖 basis。
    let gap = if ctx.kind == CarouselKind::Single {
        0.0
    } else {
        // Multi 槽间距用左右各 0.375rem。
        0.375
    };
    // 宽度由 Content 按视口像素写入；此处勿留 %，否则会被 unit_specs 盖回。
    let mut el: Element = r#box()
        .min_w(0.0)
        .flex_shrink(0.0)
        .flex_grow(0.0)
        .children(kids)
        .into();
    if ctx.orientation == CarouselOrientation::Vertical {
        el = el.py(format!("{gap}rem")).px(0.0);
    } else {
        el = el.px(format!("{gap}rem"));
    }
    el.attrs
        .insert(ATTR_SLOT.to_string(), SLOT_ITEM.to_string());
    el = el.accesskit_role("group");
    el.set_attr("aria-roledescription", "slide");
    el
}

fn nav_button(
    ctx: &CarouselCtx,
    which: &'static str,
    lucide: &'static str,
    on_click: impl Fn() + 'static,
) -> Element {
    use crate::components::button::button;

    let icon = lucide_chevron(lucide);
    // 产品 Button icon-sm = 2×2rem，items/justify center；lucide 字形不强制框，圆心对齐。
    let mut btn: Element = button(format!("{}:{which}", ctx.id))
        .variant("outline")
        .size("icon-sm")
        .on_click(on_click)
        .child(icon)
        .into();
    btn = btn.rounded_full().flex_shrink(0.0);
    btn.attrs.insert(
        ATTR_SLOT.to_string(),
        if which == "prev" {
            SLOT_PREV.to_string()
        } else {
            SLOT_NEXT.to_string()
        },
    );
    let sync_ctx = ctx.clone();
    let sync_which = which;
    btn = btn.reactive_sync(move |el: &mut Element, _lt: &mut LayoutTree| {
        let enabled = if sync_which == "prev" {
            can_scroll_prev(&sync_ctx)
        } else {
            can_scroll_next(&sync_ctx)
        };
        let target = if enabled { 1.0 } else { 0.45 };
        if (el.style.opacity - target).abs() > 0.01 {
            el.style.opacity = target;
            LayoutStatus::StyleChanged
        } else {
            LayoutStatus::Clean
        }
    });
    btn
}

/// solid-ui / ShadCN `CarouselPrevious`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn CarouselPrevious(children: Option<Element>) -> Element {
    let _ = children;
    let ctx = require_carousel("CarouselPrevious");
    let c = ctx.clone();
    nav_button(&ctx, "prev", "chevron-left", move || {
        if can_scroll_prev(&c) {
            scroll_prev(&c);
        }
    })
}

/// solid-ui / ShadCN `CarouselNext`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn CarouselNext(children: Option<Element>) -> Element {
    let _ = children;
    let ctx = require_carousel("CarouselNext");
    let c = ctx.clone();
    nav_button(&ctx, "next", "chevron-right", move || {
        if can_scroll_next(&c) {
            scroll_next(&c);
        }
    })
}

/// 页点指示器（当前页高亮）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn CarouselDots(children: Option<Element>) -> Element {
    let _ = children;
    let ctx = require_carousel("CarouselDots");
    let view_id = format!("{}:dots_view", ctx.id);
    let mut el = reactive_view(view_id, move || {
        let n = ctx.count.get();
        let cur = ctx.index.get();
        let mut row: Element = hstack()
            .gap("0.375rem")
            .items_center()
            .justify_center()
            .w("100%")
            .into();
        for i in 0..n {
            let on = i == cur;
            let mut dot: Element = r#box()
                .w(if on { "1rem" } else { "0.5rem" })
                .h("0.5rem")
                .bg(if on {
                    tokens::primary()
                } else {
                    tokens::muted()
                })
                .into();
            dot = dot.rounded_full();
            let jump = ctx.clone();
            let idx = i;
            dot = interactive(dot)
                .focus_id(format!("{}:dot:{i}", ctx.id))
                .focusable(true)
                .on_click(move || {
                    if loop_anim(&jump.id).is_some() {
                        finish_loop_anim(&jump.id, jump.index);
                    }
                    jump.index.set(idx);
                    bump_autoplay(&jump);
                    sync_offset_target(&jump);
                })
                .into();
            row = row.child(dot);
        }
        row
    });
    // 覆盖 reactive_placeholder 默认 100%×100%，避免把 dots 撑成整列空白。
    el = el.w("100%").h("auto").min_h("0.5rem").mt("0.75rem");
    el.attrs
        .insert(ATTR_SLOT.to_string(), SLOT_DOTS.to_string());
    el
}

// ── Builder ──────────────────────────────────────────────────────────────────

pub struct CarouselBuilder {
    id: String,
    slides: Vec<Rc<dyn Fn() -> Element>>,
    index: Option<Signal<usize>>,
    orientation: CarouselOrientation,
    loop_slides: bool,
    per_view: f32,
    autoplay_ms: u64,
    controls: CarouselControls,
    scroll_by: CarouselScrollBy,
    kind: CarouselKind,
    end_align: CarouselEndAlign,
    dots: Option<bool>,
    stack_sides: u32,
}

/// Builder 入口：默认单页 `Carousel`。
pub fn carousel(id: impl Into<String>) -> CarouselBuilder {
    CarouselBuilder {
        id: id.into(),
        slides: Vec::new(),
        index: None,
        orientation: CarouselOrientation::Horizontal,
        loop_slides: true,
        per_view: 1.0,
        autoplay_ms: 0,
        controls: CarouselControls::Outside,
        scroll_by: CarouselScrollBy::Slide,
        kind: CarouselKind::Single,
        end_align: CarouselEndAlign::Flush,
        dots: None,
        stack_sides: 1,
    }
}

/// 多列列表 Builder（`CarouselMulti`）。
pub fn carousel_multi(id: impl Into<String>) -> CarouselBuilder {
    carousel(id)
        .kind(CarouselKind::Multi)
        .per_view(2.2)
        .scroll_by(CarouselScrollBy::Slide)
}

/// 两侧堆叠 Builder（`CarouselStack`）。
pub fn carousel_stack(id: impl Into<String>) -> CarouselBuilder {
    carousel(id).kind(CarouselKind::Stack).per_view(1.0)
}

impl CarouselBuilder {
    pub fn slide(mut self, build: impl Fn() -> Element + 'static) -> Self {
        self.slides.push(Rc::new(build));
        self
    }

    pub fn index_signal(mut self, s: Signal<usize>) -> Self {
        self.index = Some(s);
        self
    }

    pub fn orientation(mut self, o: CarouselOrientation) -> Self {
        self.orientation = o;
        self
    }

    pub fn loop_slides(mut self, yes: bool) -> Self {
        self.loop_slides = yes;
        self
    }

    pub fn per_view(mut self, n: f32) -> Self {
        self.per_view = n;
        self
    }

    pub fn autoplay_ms(mut self, ms: u64) -> Self {
        self.autoplay_ms = ms;
        self
    }

    pub fn controls(mut self, c: CarouselControls) -> Self {
        self.controls = c;
        self
    }

    pub fn scroll_by(mut self, s: CarouselScrollBy) -> Self {
        self.scroll_by = s;
        self
    }

    pub fn kind(mut self, k: CarouselKind) -> Self {
        self.kind = k;
        self
    }

    pub fn end_align(mut self, a: CarouselEndAlign) -> Self {
        self.end_align = a;
        self
    }

    pub fn dots(mut self, yes: bool) -> Self {
        self.dots = Some(yes);
        self
    }

    pub fn stack_sides(mut self, n: u32) -> Self {
        self.stack_sides = n.max(1);
        self
    }

    pub fn child(self, body: impl FnOnce() -> Element) -> Element {
        carousel_scope(
            self.id,
            self.index,
            self.orientation.as_str(),
            self.loop_slides,
            self.per_view,
            self.autoplay_ms,
            self.controls.as_str(),
            self.scroll_by.as_str(),
            self.kind.as_str(),
            self.end_align.as_str(),
            self.dots,
            self.stack_sides,
            body,
        )
    }
}

impl From<CarouselBuilder> for Element {
    fn from(builder: CarouselBuilder) -> Self {
        let CarouselBuilder {
            id,
            slides,
            index,
            orientation,
            loop_slides,
            per_view,
            autoplay_ms,
            controls,
            scroll_by,
            kind,
            end_align,
            dots,
            stack_sides,
        } = builder;
        let slides_for_content = slides.clone();
        let inject_dots = dots.unwrap_or_else(|| kind.default_dots());
        carousel_scope(
            id,
            index,
            orientation.as_str(),
            loop_slides,
            per_view,
            autoplay_ms,
            controls.as_str(),
            scroll_by.as_str(),
            kind.as_str(),
            end_align.as_str(),
            dots,
            stack_sides,
            move || {
                let mut items = Vec::new();
                for build in &slides_for_content {
                    let body = build();
                    let mut item: Element = r#box()
                        .min_w(0.0)
                        .w("100%")
                        .flex_basis("100%")
                        .flex_shrink(0.0)
                        .flex_grow(0.0)
                        .px("0.375rem")
                        .child(body)
                        .into();
                    item.attrs
                        .insert(ATTR_SLOT.to_string(), SLOT_ITEM.to_string());
                    item = item.accesskit_role("group");
                    item.set_attr("aria-roledescription", "slide");
                    items.push(item);
                }
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                frag = frag.children(items);
                let content = CarouselContent(CarouselContentProps {
                    children: Some(frag),
                    ..Default::default()
                });
                let prev = CarouselPrevious(CarouselPreviousProps::default());
                let next = CarouselNext(CarouselNextProps::default());
                let mut kids = vec![content, prev, next];
                if inject_dots {
                    kids.push(CarouselDots(CarouselDotsProps::default()));
                }
                let mut root = Element::new();
                root.set_attr("rui_fragment", "1");
                root.children(kids)
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_parse() {
        assert_eq!(
            CarouselOrientation::parse("vertical"),
            CarouselOrientation::Vertical
        );
        assert_eq!(
            CarouselOrientation::parse("horizontal"),
            CarouselOrientation::Horizontal
        );
    }

    #[test]
    fn kind_and_end_align_parse() {
        assert_eq!(CarouselKind::parse("multi"), CarouselKind::Multi);
        assert_eq!(CarouselKind::parse("stack"), CarouselKind::Stack);
        assert!(!CarouselKind::Multi.default_dots());
        assert!(CarouselKind::Single.default_dots());
        assert_eq!(CarouselEndAlign::parse("pad"), CarouselEndAlign::Pad);
        assert_eq!(CarouselEndAlign::parse("flush"), CarouselEndAlign::Flush);
    }

    #[test]
    fn controls_and_scroll_by_parse() {
        assert_eq!(CarouselControls::parse("inside"), CarouselControls::Inside);
        assert_eq!(
            CarouselControls::parse("outside"),
            CarouselControls::Outside
        );
        assert_eq!(CarouselScrollBy::parse("page"), CarouselScrollBy::Page);
        assert_eq!(CarouselScrollBy::parse("slide"), CarouselScrollBy::Slide);
    }

    #[test]
    fn builder_builds() {
        let el: Element = carousel("t-carousel")
            .per_view(2.0)
            .autoplay_ms(0)
            .controls(CarouselControls::Outside)
            .scroll_by(CarouselScrollBy::Page)
            .kind(CarouselKind::Multi)
            .end_align(CarouselEndAlign::Pad)
            .slide(|| r#box().w("100%").h("4rem").into())
            .slide(|| r#box().w("100%").h("4rem").into())
            .into();
        assert_ne!(el.template_id.as_deref(), Some(""));
        assert_eq!(
            el.attrs.get("data-kind").map(|s| s.as_str()),
            Some("multi")
        );
        assert_eq!(
            el.attrs.get("data-end-align").map(|s| s.as_str()),
            Some("pad")
        );
        assert_eq!(el.attrs.get("data-dots").map(|s| s.as_str()), Some("0"));
    }

    /// 回归：视口未就绪时不得把克隆轨道弹簧 set 到 step≈1 的假 ideal（≈0），
    /// 否则首次「下一」会从 0 滑到 ideal(1)，跨两格，像自动多切了一页。
    #[test]
    fn looping_build_does_not_park_offset_at_fake_step_before_viewport() {
        use crate::core::reactivity::create_root;
        use crate::style::use_spring;
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let id = format!("t-car-pre-vp-{}", N.fetch_add(1, Ordering::Relaxed));
        let spring_key = format!("{id}__offset");
        let (cur, target) = create_root(|| {
            let id2 = id.clone();
            let _el: Element = carousel(id)
                .loop_slides(true)
                .slide(|| r#box().w("100%").h("4rem").into())
                .slide(|| r#box().w("100%").h("4rem").into())
                .slide(|| r#box().w("100%").h("4rem").into())
                .into();
            let _ = id2;
            let spring = use_spring(&spring_key, 0.0);
            (spring.peek(), spring.peek_target())
        })
        .1;
        assert!(
            cur.abs() < 0.01 && target.abs() < 0.01,
            "pre-viewport spring must stay at 0, got cur={cur} target={target}"
        );
    }
}
