//! # Carousel
//!
//! Align solid-ui/ShadCN **Carousel** (Embla Mind, no 3rd party engine):
//!
//! ```text
//! Carousel / CarouselSingle / CarouselMulti / CarouselStack
//! ├── variant = single | multi | stack // Unified variant; dedicated tag lock variant
//! ├── [Prev] Content [Next]             // controls=outside|inside
//! └── CarouselDots (optional; Single/Stack is on by default, Multi is off by default)
//! ```
//!
//! - **`<Carousel variant="…">`**: Unified entrance, use `variant` to coordinate single / multi / stack.
//! - **`<CarouselSingle>` / `<CarouselMulti>` / `<CarouselStack>`**: Macro side locking variant, more precise usage.
//! - **Switch animation**: Both tracks and Stack cards use CSS `transition` (transform / opacity).
//! - **Seamless Loop**: When `seamless` (and `loop`), the track is cloned at the beginning and end; it is reset after sliding in the same direction across borders.
//! - **Drag**: `snap=false` and no autoplay can be freely docked; when combined with `loop+seamless`, the real band will be folded during dragging (this is true for both snapping and free, to avoid dragging out of the white screen).
//! - **Step**: `scrollBy="slide"|"page"`; `pageSnap="peek"|"viewport"` (when page is cut, the tail item is connected to the next page/full screen advancement).
//! - **Control bits**: `controls="outside"|"inside"`.
//!
//! ## Module split (product layer, not framework primitives)
//! - [`track`] / [`snap`] / [`loop_anim`] / [`autoplay`] / [`geometry`] / [`drag`] / [`content`] / [`builder`]: This component’s private layering
//! - For drag sessions see [`utils::pointer_drag`](crate::utils::pointer_drag)
//!
//! ## Usage
//! ```ignore
//! ui! {
//!     <Carousel id="via-variant" variant="single" autoplay={4000}>…</Carousel>
//!     <CarouselSingle id="one" autoplay={4000}>…</CarouselSingle>
//!     <CarouselMulti id="many" perView={2.2} loop seamless>…</CarouselMulti>
//!     <CarouselMulti id="free" itemSizing="fixed" itemWidth="12rem" snap={false} loop={false}>…</CarouselMulti>
//!     <CarouselMulti id="orbit" perView={2.2} loop={true} seamless={true} snap={false}>…</CarouselMulti>
//!     <CarouselMulti id="pages" perView={2.2} scrollBy="page" pageSnap="peek" endAlign="pad" loop={false}>…</CarouselMulti>
//!     <CarouselStack id="stack-auto" autoplay={4000} loop={true}>…</CarouselStack>
//!     <CarouselStack id="stack-drag" loop={true}>…</CarouselStack>
//! }
//! ```
//!
//! Builder: `carousel` / `carousel_single` / `carousel_multi` / `carousel_stack`, or `.variant(...)`.

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
use crate::layout::dimension::CssDim;
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::style::{TimingFunction, TransitionEntry, TransitionProperty, UnitSlot};
use crate::tokens;
use crate::utils::lucide::lucide_glyph;
use crate::utils::motion::{
    clear_motion, ensure_motion_transitions, motion_sync_status, snap_motion_baseline,
};
use crate::utils::resolve_instance_id;
use crate::utils::text_node::build_icon_text_node;
use crate::utils::take_packed;

mod autoplay;
pub mod builder;
pub mod content;
mod drag;
mod geometry;
mod loop_anim;
mod snap;
mod track;

pub use builder::{carousel, carousel_multi, carousel_single, carousel_stack, CarouselBuilder};
#[allow(unused_imports)] // used by external crates / window layer
pub(crate) use drag::is_carousel_dragging;
use drag::*;
pub(crate) use drag::{
    begin_carousel_paint_frame, carousel_next_wake_at, clear_carousel_drag_if_any,
    tick_carousel_autoplay, tick_carousel_drag,
};
use geometry::*;

use autoplay::{decide_autoplay, gate_autoplay_visibility, AutoplayAction};
use loop_anim::{loop_reached, LoopAnim};

const ATTR_SLOT: &str = "data-slot";
const SLOT_CAROUSEL: &str = "carousel";
const SLOT_CONTENT: &str = "carousel-content";
const SLOT_ITEM: &str = "carousel-item";
const SLOT_PREV: &str = "carousel-previous";
const SLOT_NEXT: &str = "carousel-next";
const SLOT_DOTS: &str = "carousel-dots";
const DRAG_THRESHOLD_PX: f32 = 28.0;
/// Orbital page turning: CSS `transition: transform` (including translate), no longer independent of Spring.
const TRACK_TRANSITION_MS: f32 = 400.0;
/// Stack coverflow cut item: transform + opacity.
const STACK_TRANSITION_MS: f32 = 320.0;

/// Orbital orientation (counterpoint ShadCN `orientation` / Embla `axis`).
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

/// The position of the navigation button relative to the content area.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CarouselControls {
    /// Content outside both sides (default, counterpoint ShadCN `-left/right-12` mental).
    #[default]
    Outside,
    /// Stacked inside the content.
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

/// Previous/next step unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CarouselScrollBy {
    /// Move 1 frame at a time (default).
    #[default]
    Slide,
    /// Move one full screen (`floor(per_view).max(1)` pictures) at a time.
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

/// 轮播变体（`variant`）：由 `<Carousel variant>` 统筹，或由
/// `<CarouselSingle>` / `<CarouselMulti>` / `<CarouselStack>` Lock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CarouselVariant {
    /// Full page.
    #[default]
    Single,
    /// Multi-column lists (`per_view` peek).
    Multi,
    /// Stacked on both sides (coverflow mind).
    Stack,
}

/// Old name; use [`CarouselVariant`] in preference.
pub type CarouselKind = CarouselVariant;

impl CarouselVariant {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "multi" | "list" | "peek" => Self::Multi,
            "stack" | "coverflow" | "cover-flow" => Self::Stack,
            "single" | "one" | "full" | "" => Self::Single,
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

/// Multi-column + end alignment when turning full page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CarouselEndAlign {
    /// Add a blank space at the end to facilitate turning the entire page to an empty slot.
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

/// Full page stepping strategy when using multiple columns + `scrollBy=page`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CarouselPageSnap {
    /// When the page is cropped, the last item becomes the first item of the next page (step by `floor(perView)` grid; default, better experience).
    #[default]
    Peek,
    /// Advance by the whole screen viewport (step ≈ `perView` slot width, can stop at non-full screen).
    Viewport,
}

impl CarouselPageSnap {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "viewport" | "screen" | "full" | "page" => Self::Viewport,
            "peek" | "carry" | "overlap" | "item" => Self::Peek,
            _ => Self::Peek,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Peek => "peek",
            Self::Viewport => "viewport",
        }
    }
}

/// Multi-column card width strategy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CarouselItemSizing {
    /// Width = viewport / `perView` (percentage basis, croppable peek).
    #[default]
    Fraction,
    /// Fixed `itemWidth`; clipping peek by overflow.
    Fixed,
}

impl CarouselItemSizing {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "fixed" | "px" | "rem" => Self::Fixed,
            _ => Self::Fraction,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fraction => "fraction",
            Self::Fixed => "fixed",
        }
    }
}

/// Carousel context (composite subtree / [`use_carousel`]).
#[derive(Clone)]
pub struct CarouselCtx {
    pub id: String,
    pub index: Signal<usize>,
    pub count: Signal<usize>,
    pub orientation: CarouselOrientation,
    /// Whether wraparound navigation is allowed at the beginning and end.
    pub loop_slides: bool,
    /// Whether to use head and tail clones to make seamless looping (`loop=false` always false).
    pub seamless: bool,
    /// Whether to release the hand and adsorb to the grid point (forced true when `autoplay>0`).
    pub snap: bool,
    /// Number of pages visible on one screen (`1` = single page; `2.2` = two and a half pages peek); still used for page step estimation when `fixed` is used.
    pub per_view: f32,
    pub autoplay_ms: u64,
    pub controls: CarouselControls,
    pub scroll_by: CarouselScrollBy,
    pub variant: CarouselVariant,
    pub end_align: CarouselEndAlign,
    /// When `scrollBy=page`: Crop the last item to connect to the next page (peek) or advance the entire screen (viewport).
    pub page_snap: CarouselPageSnap,
    pub item_sizing: CarouselItemSizing,
    /// Parsed card width (logical px) in `fixed` mode; 0 in `fraction`.
    pub item_width_px: f32,
    /// Whether to automatically inject bottom progress dots (explicit `<CarouselDots/>` is always retained).
    pub show_dots: bool,
    /// Number of stacking layers on both sides (default 1 = one on each side).
    pub stack_sides: u32,
    pub drag_px: Signal<f32>,
    pub viewport_len: Signal<f32>,
    pub paused: Signal<bool>,
}

fn offset_target_key(id: &str) -> String {
    format!("{id}__offset_tgt")
}

fn offset_target(id: &str) -> Signal<f32> {
    use_signal(&offset_target_key(id), || 0.0f32)
}

thread_local! {
    static AUTOPLAY_AT: RefCell<HashMap<String, Instant>> = RefCell::new(HashMap::new());
    /// Seamless loop: first roll to the first/last clone slot, then teleport to the real item ([`LoopAnim`]) when in place.
    /// The lifespan is longer than the component tree; `sync_offset_target` will return directly when it exists - if the flag is dropped but not settled, it will freeze halfway.
    static LOOP_ANIM: RefCell<HashMap<String, LoopAnim>> = RefCell::new(HashMap::new());
    /// The previous frame's track visual translate (for dragging starting point; CSS tween interpolated value).
    static LAST_VISUAL_OFFSET: RefCell<HashMap<String, f32>> = RefCell::new(HashMap::new());
    /// The next frame is pinned to this offset first, and then the CSS baseline is seeded; then a new target can be written to continue the animation (to avoid skipping the transition after teleporting).
    static INSTANT_SNAP_TO: RefCell<HashMap<String, f32>> = RefCell::new(HashMap::new());
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

fn request_instant_snap_to(id: &str, pos: f32) {
    INSTANT_SNAP_TO.with(|m| {
        m.borrow_mut().insert(id.to_string(), pos);
    });
}

fn take_instant_snap_to(id: &str) -> Option<f32> {
    INSTANT_SNAP_TO.with(|m| m.borrow_mut().remove(id))
}

fn set_last_visual_offset(id: &str, v: f32) {
    LAST_VISUAL_OFFSET.with(|m| {
        m.borrow_mut().insert(id.to_string(), v);
    });
}

fn last_visual_offset(id: &str) -> f32 {
    LAST_VISUAL_OFFSET.with(|m| {
        m.borrow()
            .get(id)
            .copied()
            .unwrap_or_else(|| offset_target(id).peek())
    })
}

/// Complete the seamless segment: Teleport to the real slot. The index has been switched to settle (dots synchronization) when begin, please confirm again here.
fn finish_loop_anim(id: &str, index: Signal<usize>) {
    let Some(anim) = take_loop_anim(id) else {
        return;
    };
    offset_target(id).set(anim.settle_offset);
    request_instant_snap_to(id, anim.settle_offset);
    index.set(anim.settle_index);
}

/// Navigation breaks seamless segments: remap current clone coordinates to real slots.
fn resolve_loop_anim_keep_offset(id: &str, index: Signal<usize>) {
    let Some(anim) = take_loop_anim(id) else {
        return;
    };
    let cur = last_visual_offset(id);
    let remapped = anim.settle_offset + (cur - anim.target_offset);
    offset_target(id).set(remapped);
    request_instant_snap_to(id, remapped);
    index.set(anim.settle_index);
}

/// Clean up thread_local residue when mounting.
fn reset_carousel_ephemeral(id: &str) {
    let _ = take_loop_anim(id);
    let _ = take_instant_snap_to(id);
    LAST_VISUAL_OFFSET.with(|m| {
        m.borrow_mut().remove(id);
    });
    let cur = last_visual_offset(id);
    offset_target(id).set(cur);
    request_instant_snap_to(id, cur);
    clear_carousel_drag_for_id(id);
}

fn animate_offset_to(id: &str, target: f32) {
    offset_target(id).set(target);
}

/// Resume CSS animation from a known visual position (drag to release/seamless crossover): nail the baseline first and then set the target.
/// Otherwise, the snapshot will be empty when the transition is just restored, and `from=to=new target` will flash.
fn animate_offset_from(id: &str, from_visual: f32, target: f32) {
    request_instant_snap_to(id, from_visual);
    offset_target(id).set(target);
}

fn snap_offset_to(id: &str, target: f32) {
    offset_target(id).set(target);
    request_instant_snap_to(id, target);
}

fn track_motion_entries() -> [TransitionEntry; 1] {
    [TransitionEntry {
        property: TransitionProperty::Transform,
        duration_ms: TRACK_TRANSITION_MS,
        delay_ms: 0.0,
        timing: TimingFunction::CubicBezier(0.22, 1.0, 0.36, 1.0),
    }]
}

fn ensure_track_css_transition(el: &mut Element) {
    ensure_motion_transitions(&mut el.style, &track_motion_entries());
}

fn ensure_stack_card_css_transition(el: &mut Element) {
    if !el.style.transition.is_empty() {
        return;
    }
    let timing = TimingFunction::CubicBezier(0.22, 1.0, 0.36, 1.0);
    el.style.transition.push(TransitionEntry {
        property: TransitionProperty::Transform,
        duration_ms: STACK_TRANSITION_MS,
        delay_ms: 0.0,
        timing,
    });
    el.style.transition.push(TransitionEntry {
        property: TransitionProperty::Opacity,
        duration_ms: STACK_TRANSITION_MS,
        delay_ms: 0.0,
        timing,
    });
}

/// Stack scaling is written on the shell; the subtree remains scale=1, as determined by collect's `ancestor_scale`/
/// `ancestor_linear` inheritance (text takes `paint_scale` + glyphon matrix, shares parent transformation with shell).
/// In this way, the rectangle and the text share the same parent scale to avoid scaling around the center and causing the text to become disconnected when dragging.
fn reset_stack_descendant_scales(el: &mut Element) {
    for child in &mut el.children {
        if (child.style.scale[0] - 1.0).abs() > 0.001 || (child.style.scale[1] - 1.0).abs() > 0.001
        {
            child.style.scale = [1.0, 1.0];
        }
        if child.text.is_some() {
            child.text_style.white_space = crate::style::WhiteSpace::Nowrap;
        }
        reset_stack_descendant_scales(child);
    }
}

/// Interpolate scale/transparency/z at a continuous distance from center (drag to avoid hard-cut transitions of ad=0.5).
fn stack_card_appearance(ad: f32, sides: f32, idx: usize, count: usize) -> (f32, f32, i32) {
    let visible = ad <= sides + 0.01;
    if !visible {
        return (0.68, 0.0, 0);
    }
    // ad=0 → sc1/op1; ad=1 → ≈0.82/0.72; ad≥2 → lower limit.
    let sc = (1.0 - 0.18 * ad).clamp(0.72, 1.0);
    let op = if ad <= 1.0 {
        (1.0 - 0.28 * ad).clamp(0.72, 1.0)
    } else {
        (0.72 - 0.14 * (ad - 1.0)).clamp(0.42, 0.72)
    };
    // Fine-grained z + slot breaks juxtaposition (z used to be the same when the preceding and following items ad were close, and the band was out of order, causing the text to "stop following").
    let z = ((10_000.0 - ad * 1_000.0) * 10.0).round() as i32 + (count - idx) as i32;
    (sc, op, z.max(1))
}

fn clear_track_css_transition(el: &mut Element) {
    clear_motion(el);
}

fn apply_axis_translate(el: &mut Element, orientation: CarouselOrientation, x_or_y: f32) {
    el.style.translate = match orientation {
        CarouselOrientation::Horizontal => [x_or_y, 0.0],
        CarouselOrientation::Vertical => [0.0, x_or_y],
    };
}

/// Deep copy of the slider visual tree (no interaction / no layout node) for head and tail cloning of tracks.
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

/// Read the most recent [`Carousel`] context.
pub fn use_carousel() -> Option<CarouselCtx> {
    use_context(carousel_context())
}

fn require_carousel(part: &str) -> CarouselCtx {
    use_carousel().unwrap_or_else(|| {
        panic!("{part} must be used within <Carousel|CarouselSingle|CarouselMulti|CarouselStack> / carousel_scope");
    })
}
fn is_carousel_item(el: &Element) -> bool {
    el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_ITEM)
}

fn lucide_chevron(name: &str) -> Element {
    // Consistent with Calendar / Button+Icon: the width and height are not forced, and the parent flex is left to center the circle.
    let glyph = lucide_glyph(name).unwrap_or_else(|| "?".into());
    build_icon_text_node(glyph)
        .text_size(16.0)
        .font_weight(400.0)
        .text_color(parse_text_color("foreground"))
        .text_leading_none()
}
fn can_scroll_prev(ctx: &CarouselCtx) -> bool {
    let n = ctx.count.peek();
    if n <= 1 {
        return false;
    }
    if wraps_nav(ctx) {
        return true;
    }
    if !effective_snap(ctx)
        || (ctx.variant == CarouselVariant::Multi && ctx.scroll_by == CarouselScrollBy::Page)
    {
        // Free docking/full page: subject to offset (page docking does not need to be in an integer format).
        return offset_target(&ctx.id).peek() < -0.5;
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
    if !effective_snap(ctx)
        || (ctx.variant == CarouselVariant::Multi && ctx.scroll_by == CarouselScrollBy::Page)
    {
        let off = offset_target(&ctx.id).peek();
        return off > max_scroll_offset(ctx) + 0.5;
    }
    ctx.index.peek() < max_index(ctx)
}

fn scroll_prev(ctx: &CarouselCtx) {
    let n = ctx.count.peek();
    if n == 0 {
        return;
    }
    // The previous section is seamless and unfinished: remap to the real slot and then continue (do not finish and cut to the stop instantly).
    if loop_anim(&ctx.id).is_some() {
        resolve_loop_anim_keep_offset(&ctx.id, ctx.index);
    }
    if !effective_snap(ctx) {
        scroll_by_free_offset(ctx, 1);
        bump_autoplay(ctx);
        return;
    }
    if ctx.variant == CarouselVariant::Multi
        && ctx.scroll_by == CarouselScrollBy::Page
        && !wraps_nav(ctx)
    {
        scroll_by_page(ctx, 1);
        bump_autoplay(ctx);
        return;
    }
    let cur = ctx.index.peek();
    let hi = max_index(ctx);
    let d = scroll_delta(ctx).max(1);
    if wraps_nav(ctx) && cur < d {
        if begin_seamless_prev(ctx, cur, d) {
            bump_autoplay(ctx);
            return;
        }
        // Stack / No Clone: ​​Fall directly to the ring target.
        ctx.index.set((cur + n - d) % n.max(1));
    } else if !wraps_nav(ctx) {
        ctx.index.set(cur.saturating_sub(d));
    } else {
        ctx.index.set(cur.saturating_sub(d).min(hi));
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
        resolve_loop_anim_keep_offset(&ctx.id, ctx.index);
    }
    if !effective_snap(ctx) {
        scroll_by_free_offset(ctx, -1);
        bump_autoplay(ctx);
        return;
    }
    if ctx.variant == CarouselVariant::Multi
        && ctx.scroll_by == CarouselScrollBy::Page
        && !wraps_nav(ctx)
    {
        scroll_by_page(ctx, -1);
        bump_autoplay(ctx);
        return;
    }
    let cur = ctx.index.peek();
    let hi = max_index(ctx);
    let d = scroll_delta(ctx).max(1);
    if wraps_nav(ctx) && cur + d >= n {
        if begin_seamless_next(ctx, cur, d) {
            bump_autoplay(ctx);
            return;
        }
        ctx.index.set((cur + d) % n.max(1));
    } else if !wraps_nav(ctx) {
        ctx.index.set((cur + d).min(hi));
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

/// Drag and drop start: Clear the deadline to avoid turning the page with the old timer that has expired during the dragging period immediately after letting go.
/// (Hover pauses are still expressed by `paused`; dragging itself is blocked by `is_carousel_dragging_id`.)
fn clear_autoplay_deadline(id: &str, autoplay_ms: u64) {
    if autoplay_ms > 0 {
        set_next_autoplay_at(id, None);
    }
}

/// End of dragging: Recalculate the complete interval (do not change `paused` to avoid interrupting the hovering pause).
fn restart_autoplay_deadline(id: &str, autoplay_ms: u64) {
    if autoplay_ms == 0 {
        set_next_autoplay_at(id, None);
        return;
    }
    set_next_autoplay_at(
        id,
        Some(Instant::now() + Duration::from_millis(autoplay_ms.max(500))),
    );
}

/// Hover: Pause and clear deadline.
fn pause_autoplay_clock(id: &str, paused: Signal<bool>, autoplay_ms: u64) {
    paused.set(true);
    if autoplay_ms > 0 {
        set_next_autoplay_at(id, None);
    }
}

/// Hover End: Resume and restart the full interval.
fn resume_autoplay_clock(id: &str, paused: Signal<bool>, autoplay_ms: u64) {
    paused.set(false);
    restart_autoplay_deadline(id, autoplay_ms);
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

fn place_nav_inside(mut nav: Element, orientation: CarouselOrientation, which: &str) -> Element {
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
        dots = Some(content::CarouselDots(content::CarouselDotsProps::default()));
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
                // Do not w(100%): When placed on the same line as Prev/Next, the viewport width will be squeezed/miscalculated.
                .child(stage_body)
                .into();
            stage
                .attrs
                .insert(ATTR_SLOT.to_string(), "carousel-stage".to_string());

            let mut track: Element = if horizontal {
                hstack().w("100%").items_center().gap("0.5rem").into()
            } else {
                vstack().h("100%").items_center().gap("0.5rem").into()
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
        // Spacing on the chrome column gap:reactive_view will wash out the mt on the placeholder.
        col = col.gap("0.5rem").child(d);
    }
    for r in rest {
        col = col.child(r);
    }
    col
}

/// Provides a Carousel context and builds a subtree (shared with macros `<Carousel variant>` / `<CarouselSingle|Multi|Stack>` / Builder).
#[allow(clippy::too_many_arguments)]
pub fn carousel_scope(
    id: impl Into<String>,
    index: Option<Signal<usize>>,
    orientation: impl Into<String>,
    loop_slides: bool,
    seamless: Option<bool>,
    snap: bool,
    per_view: f32,
    autoplay_ms: u64,
    controls: impl Into<String>,
    scroll_by: impl Into<String>,
    variant: impl Into<String>,
    end_align: impl Into<String>,
    page_snap: impl Into<String>,
    item_sizing: impl Into<String>,
    item_width: impl Into<String>,
    dots: Option<bool>,
    stack_sides: u32,
    body: impl FnOnce() -> Element,
) -> Element {
    use crate::layout::dimension::IntoF32;

    let id = resolve_instance_id(id, "carousel");
    // thread_local's LOOP_ANIM/drag will remain across remounts, and sync will return directly when loop_anim exists.
    reset_carousel_ephemeral(&id);
    let orientation = CarouselOrientation::parse(&orientation.into());
    let controls = CarouselControls::parse(&controls.into());
    let scroll_by = CarouselScrollBy::parse(&scroll_by.into());
    let variant = CarouselVariant::parse(&variant.into());
    let end_align = CarouselEndAlign::parse(&end_align.into());
    let page_snap = CarouselPageSnap::parse(&page_snap.into());
    let item_sizing = CarouselItemSizing::parse(&item_sizing.into());
    let item_width_raw = item_width.into();
    let item_width_px = if item_sizing == CarouselItemSizing::Fixed {
        let px = item_width_raw.as_str().into_f32();
        if px > 1.0 {
            px
        } else {
            12.0 * crate::layout::dimension::CSS_ROOT_FONT_SIZE_PX
        }
    } else {
        0.0
    };
    let show_dots = dots.unwrap_or_else(|| variant.default_dots());
    // Single forces one screen and one card; Multi uses incoming/default per_view (0 = uses variant default).
    let per_view = match variant {
        CarouselVariant::Single => 1.0,
        CarouselVariant::Stack => 1.0,
        CarouselVariant::Multi => {
            if per_view > 0.0 {
                per_view.clamp(0.5, 8.0)
            } else {
                variant.default_per_view()
            }
        }
    };
    let stack_sides = stack_sides.clamp(1, 5);
    // All kinds respect `loop`; `seamless` defaults to loop and is forced to close when loop=false.
    let seamless = if !loop_slides {
        false
    } else {
        seamless.unwrap_or(true)
    };
    // Force adsorption when autoplay is turned on.
    let snap = snap || autoplay_ms > 0;
    let index = index.unwrap_or_else(|| use_signal(format!("{id}__idx").as_str(), || 0usize));
    let count = use_signal(format!("{id}__count").as_str(), || 0usize);
    let drag_px = use_signal(format!("{id}__drag").as_str(), || 0.0f32);
    let viewport_len = use_signal(format!("{id}__vlen").as_str(), || 0.0f32);
    let paused = use_signal(format!("{id}__paused").as_str(), || false);
    let _offset = offset_target(&id);

    let ctx = CarouselCtx {
        id: id.clone(),
        index,
        count,
        orientation,
        loop_slides,
        seamless,
        snap,
        per_view,
        autoplay_ms,
        controls,
        scroll_by,
        variant,
        end_align,
        page_snap,
        item_sizing,
        item_width_px,
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
    root.attrs
        .insert("data-controls".to_string(), controls.as_str().to_string());
    root.attrs
        .insert("data-scroll-by".to_string(), scroll_by.as_str().to_string());
    root.attrs
        .insert("data-variant".to_string(), variant.as_str().to_string());
    root.attrs
        .insert("data-end-align".to_string(), end_align.as_str().to_string());
    root.attrs
        .insert("data-page-snap".to_string(), page_snap.as_str().to_string());
    root.attrs.insert(
        "data-item-sizing".to_string(),
        item_sizing.as_str().to_string(),
    );
    root.attrs.insert(
        "data-seamless".to_string(),
        if seamless { "1" } else { "0" }.to_string(),
    );
    root.attrs.insert(
        "data-snap".to_string(),
        if snap { "1" } else { "0" }.to_string(),
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
                if h {
                    pause_autoplay_clock(&id, paused, autoplay_ms);
                } else {
                    resume_autoplay_clock(&id, paused, autoplay_ms);
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

fn signed_ring_dist_f(i: f32, cur: f32, n: usize) -> f32 {
    if n == 0 {
        return 0.0;
    }
    let n_f = n as f32;
    let mut d = i - cur;
    let half = n_f * 0.5;
    while d > half {
        d -= n_f;
    }
    while d <= -half {
        d += n_f;
    }
    d
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
        assert_eq!(CarouselVariant::parse("multi"), CarouselVariant::Multi);
        assert_eq!(CarouselVariant::parse("stack"), CarouselVariant::Stack);
        assert!(!CarouselVariant::Multi.default_dots());
        assert!(CarouselVariant::Single.default_dots());
        assert_eq!(CarouselEndAlign::parse("pad"), CarouselEndAlign::Pad);
        assert_eq!(CarouselEndAlign::parse("flush"), CarouselEndAlign::Flush);
        assert_eq!(CarouselPageSnap::parse("peek"), CarouselPageSnap::Peek);
        assert_eq!(
            CarouselPageSnap::parse("viewport"),
            CarouselPageSnap::Viewport
        );
        assert_eq!(
            CarouselItemSizing::parse("fixed"),
            CarouselItemSizing::Fixed
        );
        assert_eq!(
            CarouselItemSizing::parse("fraction"),
            CarouselItemSizing::Fraction
        );
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
            .variant(CarouselVariant::Multi)
            .end_align(CarouselEndAlign::Pad)
            .slide(|| r#box().w("100%").h("4rem").into())
            .slide(|| r#box().w("100%").h("4rem").into())
            .into();
        assert_ne!(el.template_id.as_deref(), Some(""));
        assert_eq!(
            el.attrs.get("data-variant").map(|s| s.as_str()),
            Some("multi")
        );
        assert_eq!(
            el.attrs.get("data-end-align").map(|s| s.as_str()),
            Some("pad")
        );
        assert_eq!(el.attrs.get("data-dots").map(|s| s.as_str()), Some("0"));
        assert_eq!(el.attrs.get("data-seamless").map(|s| s.as_str()), Some("1"));
        assert_eq!(el.attrs.get("data-snap").map(|s| s.as_str()), Some("1"));
    }

    #[test]
    fn multi_loop_false_disables_wrap_and_clones() {
        use crate::core::reactivity::create_root;
        create_root(|| {
            let el: Element = carousel_multi("t-car-noloop")
                .loop_slides(false)
                .seamless(true) // Should be forced off by loop=false
                .slide(|| r#box().w("100%").h("4rem").into())
                .slide(|| r#box().w("100%").h("4rem").into())
                .slide(|| r#box().w("100%").h("4rem").into())
                .into();
            assert_eq!(el.attrs.get("data-seamless").map(|s| s.as_str()), Some("0"));
            let ctx = CarouselCtx {
                id: "t-car-noloop".into(),
                index: use_signal("t-car-noloop__idx", || 0usize),
                count: use_signal("t-car-noloop__count", || 3usize),
                orientation: CarouselOrientation::Horizontal,
                loop_slides: false,
                seamless: false,
                snap: true,
                per_view: 2.2,
                autoplay_ms: 0,
                controls: CarouselControls::Outside,
                scroll_by: CarouselScrollBy::Slide,
                variant: CarouselVariant::Multi,
                end_align: CarouselEndAlign::Flush,
                page_snap: CarouselPageSnap::Peek,
                item_sizing: CarouselItemSizing::Fraction,
                item_width_px: 0.0,
                show_dots: false,
                stack_sides: 1,
                drag_px: use_signal("t-car-noloop__drag", || 0.0f32),
                viewport_len: use_signal("t-car-noloop__vlen", || 400.0f32),
                paused: use_signal("t-car-noloop__paused", || false),
            };
            assert!(!wraps_nav(&ctx));
            assert!(!track_loops(&ctx));
            assert_eq!(loop_clone_count(&ctx), 0);
            assert!(!can_scroll_prev(&ctx));
            assert!(can_scroll_next(&ctx));
        });
    }

    #[test]
    fn multi_fraction_index0_left_aligns_first_slide() {
        use crate::core::reactivity::create_root;
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let id = format!("t-car-frac-{}", N.fetch_add(1, Ordering::Relaxed));
        let id_off = id.clone();
        create_root(move || {
            let carousel_el: Element = carousel_multi(id)
                .loop_slides(true)
                .seamless(true)
                .per_view(2.2)
                .controls(CarouselControls::Outside)
                .slide(|| r#box().w("100%").h("4rem").into())
                .slide(|| r#box().w("100%").h("4rem").into())
                .slide(|| r#box().w("100%").h("4rem").into())
                .into();
            let mut root: Element = r#box().w("100%").child(carousel_el).into();
            let mut layout = LayoutTree::new();
            for _ in 0..4 {
                let _ = layout.build_and_layout(&mut root, 600.0, 400.0, [-1.0, -1.0]);
                let mut impact = LayoutStatus::Clean;
                crate::core::tree::sync_reactive_elements_subtree(
                    &mut root,
                    &mut layout,
                    &mut impact,
                    600.0,
                    400.0,
                    [0.0, 0.0],
                );
            }
            fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
                if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(slot) {
                    return Some(el);
                }
                el.children.iter().find_map(|c| find_slot(c, slot))
            }
            let content = find_slot(&root, SLOT_CONTENT).expect("content");
            let vp = content.layout_bounds[2];
            assert!(vp > 40.0, "viewport should be measurable, got {vp}");
            let track = content.children.first().expect("track");
            let offset = track.style.translate[0];
            let step = vp / 2.2;
            // per_view=2.2 → need_view=3 → cc=min(3,3)=3; index0 stops at -cc*step.
            let cc = 3.0f32;
            assert!(
                (offset + cc * step).abs() < 3.0,
                "index0 must park at -cc*step for left-aligned first slide; offset={offset} step={step} vp={vp}"
            );
            let tgt = offset_target(&id_off).peek();
            assert!(
                (tgt - offset).abs() < 1.0 || track.transition_snapshot.has_active_tweens(),
                "translate should match offset target; translate={offset} target={tgt}"
            );
            // First real item (skip prefix clone) left edge ≈ viewport left edge.
            let items: Vec<_> = track
                .children
                .iter()
                .filter(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_ITEM))
                .collect();
            assert!(
                items.len() >= 6,
                "expected prefix clones + real slides, got {}",
                items.len()
            );
            let first_real = items[cc as usize];
            let left_delta = (first_real.layout_bounds[0] + offset) - content.layout_bounds[0];
            // layout_bounds may or may not include transform; both methods accept small errors.
            assert!(
                left_delta.abs() < 4.0 || (first_real.layout_bounds[0] - content.layout_bounds[0]).abs() < 4.0,
                "first real slide should be left-aligned; left_delta={left_delta} item_x={} content_x={} offset={offset}",
                first_real.layout_bounds[0],
                content.layout_bounds[0]
            );
        });
    }

    #[test]
    fn snap_false_parks_at_visual_not_grid() {
        use crate::core::reactivity::create_root;
        create_root(|| {
            let id = "t-car-free-drag";
            let index = use_signal("t-car-free-drag__idx", || 0usize);
            let count = use_signal("t-car-free-drag__count", || 5usize);
            let drag_px = use_signal("t-car-free-drag__drag", || 0.0f32);
            let viewport_len = use_signal("t-car-free-drag__vlen", || 400.0f32);
            let paused = use_signal("t-car-free-drag__paused", || false);
            // Starting offset=0, drag -70px → visual -70; step size ≈181.8, if adsorbed, it will reach 0 or -181.8.
            snap_offset_to(id, 0.0);
            begin_carousel_drag(CarouselDrag {
                id: id.into(),
                start_ptr: 200.0,
                base_offset: 0.0,
                index: index.clone(),
                count: count.clone(),
                per_view: 2.2,
                wraps: false,
                loop_clones: 0,
                end_align: CarouselEndAlign::Flush,
                variant: CarouselVariant::Multi,
                item_sizing: CarouselItemSizing::Fraction,
                item_width_px: 0.0,
                scroll_by: CarouselScrollBy::Slide,
                page_snap: CarouselPageSnap::Peek,
                snap: false,
                viewport_len: viewport_len.clone(),
                drag_px: drag_px.clone(),
                orientation: CarouselOrientation::Horizontal,
                autoplay_ms: 0,
            });
            drag_px.set(-70.0);
            end_carousel_drag();
            let parked = offset_target(id).peek();
            assert!(
                (parked + 70.0).abs() < 1.0,
                "snap=false should park at visual offset, got {parked}"
            );
            let step = 400.0 / 2.2;
            assert!(
                (parked % step).abs() > 1.0 && (parked % step + step).abs() > 1.0,
                "parked offset must not land on an integer grid; parked={parked} step={step}"
            );
            // Regression: When index→nearest is released after letting go, the track automatically syncs and must not pull the free offset back to the grid point.
            let ctx = CarouselCtx {
                id: id.into(),
                index,
                count,
                orientation: CarouselOrientation::Horizontal,
                loop_slides: false,
                seamless: false,
                snap: false,
                per_view: 2.2,
                autoplay_ms: 0,
                controls: CarouselControls::Outside,
                scroll_by: CarouselScrollBy::Slide,
                variant: CarouselVariant::Multi,
                end_align: CarouselEndAlign::Flush,
                page_snap: CarouselPageSnap::Peek,
                item_sizing: CarouselItemSizing::Fraction,
                item_width_px: 0.0,
                show_dots: false,
                stack_sides: 1,
                drag_px,
                viewport_len,
                paused,
            };
            auto_sync_offset_if_snapping(&ctx);
            assert!(
                (offset_target(id).peek() + 70.0).abs() < 1.0,
                "auto sync must not re-snap free park; got {}",
                offset_target(id).peek()
            );
        });
    }

    #[test]
    fn wrap_loop_offset_folds_clone_band() {
        let step = 100.0;
        let n = 5usize;
        let cc = 2usize;
        let start = -(cc as f32) * step; // -200
        let end = start - (n as f32) * step; // -700
        assert!((wrap_loop_offset(start, n, cc, step) - start).abs() < 0.01);
        // Entering prefix: corrected than start → fold to the end
        let into_prefix = start + 40.0;
        let w = wrap_loop_offset(into_prefix, n, cc, step);
        assert!((w - (into_prefix - n as f32 * step)).abs() < 0.01);
        assert!(w < start && w > end);
        // Enter suffix: ≤ end → fold to the first paragraph
        let into_suffix = end - 30.0;
        let w2 = wrap_loop_offset(into_suffix, n, cc, step);
        assert!((w2 - (into_suffix + n as f32 * step)).abs() < 0.01);
        assert!(w2 <= start && w2 > end);
    }

    #[test]
    fn seamless_free_drag_end_wraps_out_of_clone_zone() {
        use crate::core::reactivity::create_root;
        create_root(|| {
            let id = "t-car-seamless-free";
            let index = use_signal("t-car-seamless-free__idx", || 0usize);
            let count = use_signal("t-car-seamless-free__count", || 5usize);
            let drag_px = use_signal("t-car-seamless-free__drag", || 0.0f32);
            let viewport_len = use_signal("t-car-seamless-free__vlen", || 440.0f32);
            let step = 440.0 / 2.2;
            let cc = 3usize;
            let start = -(cc as f32) * step;
            // Drag from index0 into the prefix clone area
            snap_offset_to(id, start);
            begin_carousel_drag(CarouselDrag {
                id: id.into(),
                start_ptr: 200.0,
                base_offset: start,
                index: index.clone(),
                count: count.clone(),
                per_view: 2.2,
                wraps: true,
                loop_clones: cc,
                end_align: CarouselEndAlign::Flush,
                variant: CarouselVariant::Multi,
                item_sizing: CarouselItemSizing::Fraction,
                item_width_px: 0.0,
                scroll_by: CarouselScrollBy::Slide,
                page_snap: CarouselPageSnap::Peek,
                snap: false,
                viewport_len: viewport_len.clone(),
                drag_px: drag_px.clone(),
                orientation: CarouselOrientation::Horizontal,
                autoplay_ms: 0,
            });
            drag_px.set(55.0); // visual = start+55 → prefix
            end_carousel_drag();
            let parked = offset_target(id).peek();
            assert!(
                parked <= start + 0.01,
                "must wrap out of prefix; parked={parked} start={start}"
            );
            let period = 5.0 * step;
            assert!(
                (parked - (start + 55.0 - period)).abs() < 1.0,
                "expected start+55-period; parked={parked}"
            );
        });
    }

    #[test]
    fn page_pad_peek_carries_cropped_item_to_next_page_start() {
        use crate::core::reactivity::create_root;
        create_root(|| {
            let id = "t-car-page-peek";
            let index = use_signal("t-car-page-peek__idx", || 0usize);
            let count = use_signal("t-car-page-peek__count", || 5usize);
            let drag_px = use_signal("t-car-page-peek__drag", || 0.0f32);
            let viewport_len = use_signal("t-car-page-peek__vlen", || 440.0f32);
            let paused = use_signal("t-car-page-peek__paused", || false);
            viewport_len.set(440.0);
            snap_offset_to(id, 0.0);
            let ctx = CarouselCtx {
                id: id.into(),
                index: index.clone(),
                count: count.clone(),
                orientation: CarouselOrientation::Horizontal,
                loop_slides: false,
                seamless: false,
                snap: true,
                per_view: 2.2,
                autoplay_ms: 0,
                controls: CarouselControls::Outside,
                scroll_by: CarouselScrollBy::Page,
                variant: CarouselVariant::Multi,
                end_align: CarouselEndAlign::Pad,
                page_snap: CarouselPageSnap::Peek,
                item_sizing: CarouselItemSizing::Fraction,
                item_width_px: 0.0,
                show_dots: false,
                stack_sides: 1,
                drag_px: drag_px.clone(),
                viewport_len: viewport_len.clone(),
                paused: paused.clone(),
            };
            let vp = 440.0f32;
            let step = vp / 2.2;
            // perView=2.2: Crop the last item on the first screen index=2; after peeking, the item becomes the first item on the next page → offset=-2*step.
            scroll_next(&ctx);
            let after_btn = offset_target(id).peek();
            assert!(
                (after_btn + 2.0 * step).abs() < 1.0,
                "peek page next must start at cropped item; got {after_btn}, want {}",
                -2.0 * step
            );
            assert!(
                (after_btn + vp).abs() > 10.0,
                "peek must not advance a full viewport; got {after_btn}"
            );

            snap_offset_to(id, 0.0);
            index.set(0);
            begin_carousel_drag(CarouselDrag {
                id: id.into(),
                start_ptr: 200.0,
                base_offset: 0.0,
                index: index.clone(),
                count: count.clone(),
                per_view: 2.2,
                wraps: false,
                loop_clones: 0,
                end_align: CarouselEndAlign::Pad,
                variant: CarouselVariant::Multi,
                item_sizing: CarouselItemSizing::Fraction,
                item_width_px: 0.0,
                scroll_by: CarouselScrollBy::Page,
                page_snap: CarouselPageSnap::Peek,
                snap: true,
                viewport_len: viewport_len.clone(),
                drag_px: drag_px.clone(),
                orientation: CarouselOrientation::Horizontal,
                autoplay_ms: 0,
            });
            // Only follow the next page after half a page (page=2*step); no longer use "small displacements are also forced to ±1".
            let page = 2.0 * step;
            drag_px.set(-(page * 0.55));
            end_carousel_drag();
            let after_drag = offset_target(id).peek();
            assert!(
                (after_drag + 2.0 * step).abs() < 1.0,
                "peek drag snap must park at -2*step; got {after_drag}"
            );
        });
    }

    #[test]
    fn page_viewport_advances_full_screen() {
        use crate::core::reactivity::create_root;
        create_root(|| {
            let id = "t-car-page-vp";
            let index = use_signal("t-car-page-vp__idx", || 0usize);
            let count = use_signal("t-car-page-vp__count", || 5usize);
            let drag_px = use_signal("t-car-page-vp__drag", || 0.0f32);
            let viewport_len = use_signal("t-car-page-vp__vlen", || 440.0f32);
            let paused = use_signal("t-car-page-vp__paused", || false);
            viewport_len.set(440.0);
            snap_offset_to(id, 0.0);
            let ctx = CarouselCtx {
                id: id.into(),
                index,
                count,
                orientation: CarouselOrientation::Horizontal,
                loop_slides: false,
                seamless: false,
                snap: true,
                per_view: 2.2,
                autoplay_ms: 0,
                controls: CarouselControls::Outside,
                scroll_by: CarouselScrollBy::Page,
                variant: CarouselVariant::Multi,
                end_align: CarouselEndAlign::Flush,
                page_snap: CarouselPageSnap::Viewport,
                item_sizing: CarouselItemSizing::Fraction,
                item_width_px: 0.0,
                show_dots: false,
                stack_sides: 1,
                drag_px,
                viewport_len,
                paused,
            };
            let vp = 440.0f32;
            scroll_next(&ctx);
            let after = offset_target(id).peek();
            assert!(
                (after + vp).abs() < 1.0,
                "viewport pageSnap must move one screen; got {after}"
            );
        });
    }

    #[test]
    fn seamless_snap_drag_wraps_like_free_and_parks_on_grid() {
        use crate::core::reactivity::create_root;
        create_root(|| {
            let id = "t-car-seamless-snap-wrap";
            let index = use_signal("t-car-seamless-snap-wrap__idx", || 2usize);
            let count = use_signal("t-car-seamless-snap-wrap__count", || 3usize);
            let drag_px = use_signal("t-car-seamless-snap-wrap__drag", || 0.0f32);
            let viewport_len = use_signal("t-car-seamless-snap-wrap__vlen", || 300.0f32);
            let step = 300.0f32;
            let cc = 1usize;
            let hi = 2usize;
            let n = 3usize;
            // stop at last item
            let base = -((hi + cc) as f32) * step;
            snap_offset_to(id, base);
            let _ = take_instant_snap_to(id);
            begin_carousel_drag(CarouselDrag {
                id: id.into(),
                start_ptr: 200.0,
                base_offset: base,
                index: index.clone(),
                count: count.clone(),
                per_view: 1.0,
                wraps: true,
                loop_clones: cc,
                end_align: CarouselEndAlign::Flush,
                variant: CarouselVariant::Single,
                item_sizing: CarouselItemSizing::Fraction,
                item_width_px: 0.0,
                scroll_by: CarouselScrollBy::Slide,
                page_snap: CarouselPageSnap::Peek,
                snap: true,
                viewport_len: viewport_len.clone(),
                drag_px: drag_px.clone(),
                orientation: CarouselOrientation::Horizontal,
                autoplay_ms: 0,
            });
            // Dragging past the last item into the "next" direction should wrap back to the real band in ticks (not clone/blank).
            let _ = tick_carousel_drag(200.0 - (step + 50.0), 0.0);
            let mid = offset_target(id).peek();
            let start = -(cc as f32) * step;
            let end = start - (n as f32) * step;
            assert!(
                mid <= start + 0.01 && mid > end,
                "snap+seamless drag must stay in real band; mid={mid} start={start} end={end}"
            );
            end_carousel_drag();
            assert!(
                loop_anim(id).is_none(),
                "drag wrap should not leave a pending clone loop anim"
            );
            let parked = offset_target(id).peek();
            // Adsorb to grid point: within the true band -(i+cc)*step
            let grid = (0..=hi)
                .map(|i| -((i + cc) as f32) * step)
                .any(|g| (parked - g).abs() < 1.0);
            assert!(
                grid,
                "after release must park on slide grid; parked={parked}"
            );
        });
    }

    #[test]
    fn seamless_snap_past_last_uses_clone_loop_not_long_scroll() {
        use crate::core::reactivity::create_root;
        create_root(|| {
            let id = "t-car-seamless-snap-boundary";
            let index = use_signal("t-car-seamless-snap-boundary__idx", || 2usize);
            let count = use_signal("t-car-seamless-snap-boundary__count", || 3usize);
            let drag_px = use_signal("t-car-seamless-snap-boundary__drag", || 0.0f32);
            let viewport_len = use_signal("t-car-seamless-snap-boundary__vlen", || 300.0f32);
            let step = 300.0f32;
            let cc = 1usize;
            let hi = 2usize;
            let n = 3usize;
            let last_off = -((hi + cc) as f32) * step; // -900
            let first_off = -(cc as f32) * step; // -300
            let clone_first = -((n + cc) as f32) * step; // -1200
            snap_offset_to(id, last_off);
            let _ = take_instant_snap_to(id);
            index.set(hi);
            begin_carousel_drag(CarouselDrag {
                id: id.into(),
                start_ptr: 200.0,
                base_offset: last_off,
                index: index.clone(),
                count,
                per_view: 1.0,
                wraps: true,
                loop_clones: cc,
                end_align: CarouselEndAlign::Flush,
                variant: CarouselVariant::Single,
                item_sizing: CarouselItemSizing::Fraction,
                item_width_px: 0.0,
                scroll_by: CarouselScrollBy::Slide,
                page_snap: CarouselPageSnap::Peek,
                snap: true,
                viewport_len,
                drag_px: drag_px.clone(),
                orientation: CarouselOrientation::Horizontal,
                autoplay_ms: 0,
            });
            // The tail item is dragged out more than halfway and enters the suffix cloning process (the full wrap has not yet been triggered).
            drag_px.set(-(step * 0.6));
            end_carousel_drag();
            assert_eq!(index.peek(), 0, "dots should already show first");
            let anim = loop_anim(id).expect("must use seamless LoopAnim, not long reverse tween");
            assert!(
                (anim.target_offset - clone_first).abs() < 1.0,
                "animate into first's clone; got {}",
                anim.target_offset
            );
            assert_eq!(anim.settle_index, 0);
            assert!((anim.settle_offset - first_off).abs() < 1.0);
            assert!(
                (offset_target(id).peek() - clone_first).abs() < 1.0,
                "target must be clone slot, not real first ({first_off}); got {}",
                offset_target(id).peek()
            );
            // After reaching the clone slot, settle → the real first item
            finish_loop_anim(id, index.clone());
            assert!((offset_target(id).peek() - first_off).abs() < 1.0);
            assert_eq!(index.peek(), 0);
        });
    }

    #[test]
    fn snap_release_follows_visual_when_next_mostly_visible() {
        use crate::core::reactivity::create_root;
        create_root(|| {
            let id = "t-car-snap-visual";
            let index = use_signal("t-car-snap-visual__idx", || 0usize);
            let count = use_signal("t-car-snap-visual__count", || 3usize);
            let drag_px = use_signal("t-car-snap-visual__drag", || 0.0f32);
            let viewport_len = use_signal("t-car-snap-visual__vlen", || 300.0f32);
            let step = 300.0f32;
            let cc = 1usize;
            // The dragging is already halfway (simulating grabbing in the middle of the animation), and then the dragging is until the next item is almost full.
            let start_ideal = -(cc as f32) * step;
            let base = start_ideal - step * 0.55;
            let extra = -step * 0.40; // Total about 0.95 cells → nearest=1
            snap_offset_to(id, base);
            let _ = take_instant_snap_to(id);
            index.set(0);
            begin_carousel_drag(CarouselDrag {
                id: id.into(),
                start_ptr: 200.0,
                base_offset: base,
                index: index.clone(),
                count,
                per_view: 1.0,
                wraps: true,
                loop_clones: cc,
                end_align: CarouselEndAlign::Flush,
                variant: CarouselVariant::Single,
                item_sizing: CarouselItemSizing::Fraction,
                item_width_px: 0.0,
                scroll_by: CarouselScrollBy::Slide,
                page_snap: CarouselPageSnap::Peek,
                snap: true,
                viewport_len,
                drag_px: drag_px.clone(),
                orientation: CarouselOrientation::Horizontal,
                autoplay_ms: 0,
            });
            // This time the gesture displacement is small (below the old threshold), but the vision is halfway through and close to the posterior term.
            drag_px.set(extra);
            assert!(extra.abs() > DRAG_THRESHOLD_PX || (base - start_ideal).abs() > step * 0.5);
            end_carousel_drag();
            assert_eq!(
                index.peek(),
                1,
                "must snap to mostly-visible next slide, not stick on start"
            );
            let want = -((1 + cc) as f32) * step;
            assert!(
                (offset_target(id).peek() - want).abs() < 1.0,
                "offset should park on slide 1; got {}",
                offset_target(id).peek()
            );
        });
    }

    #[test]
    fn drag_clears_autoplay_deadline_until_release() {
        use crate::core::reactivity::create_root;
        create_root(|| {
            let id = "t-car-ap-drag";
            let index = use_signal("t-car-ap-drag__idx", || 0usize);
            let count = use_signal("t-car-ap-drag__count", || 3usize);
            let drag_px = use_signal("t-car-ap-drag__drag", || 0.0f32);
            let viewport_len = use_signal("t-car-ap-drag__vlen", || 300.0f32);
            let paused = use_signal("t-car-ap-drag__paused", || false);
            // The timer is about to expire before simulating drag.
            set_next_autoplay_at(id, Some(Instant::now() + Duration::from_millis(50)));
            begin_carousel_drag(CarouselDrag {
                id: id.into(),
                start_ptr: 100.0,
                base_offset: 0.0,
                index,
                count,
                per_view: 1.0,
                wraps: true,
                loop_clones: 1,
                end_align: CarouselEndAlign::Flush,
                variant: CarouselVariant::Single,
                item_sizing: CarouselItemSizing::Fraction,
                item_width_px: 0.0,
                scroll_by: CarouselScrollBy::Slide,
                page_snap: CarouselPageSnap::Peek,
                snap: true,
                viewport_len,
                drag_px: drag_px.clone(),
                orientation: CarouselOrientation::Horizontal,
                autoplay_ms: 4000,
            });
            assert!(is_carousel_dragging_id(id), "drag session must be active");
            assert!(
                next_autoplay_at(id).is_none(),
                "drag must clear autoplay deadline"
            );
            drag_px.set(-10.0);
            end_carousel_drag();
            assert!(!is_carousel_dragging_id(id));
            let at = next_autoplay_at(id).expect("deadline restarted after release");
            let wait = at.saturating_duration_since(Instant::now());
            assert!(
                wait >= Duration::from_millis(3500),
                "must restart full interval, not fire stale timer; wait={wait:?}"
            );
            assert!(!paused.peek(), "drag must not leave hover-paused stuck");
        });
    }

    #[test]
    fn first_viewport_measure_does_not_flag_layout_resize() {
        use crate::core::reactivity::create_root;
        create_root(|| {
            let id = "t-car-vlen-first";
            let viewport_len = use_signal("t-car-vlen-first__vlen", || 0.0f32);
            let ctx = CarouselCtx {
                id: id.into(),
                index: use_signal("t-car-vlen-first__idx", || 0usize),
                count: use_signal("t-car-vlen-first__count", || 3usize),
                orientation: CarouselOrientation::Horizontal,
                loop_slides: true,
                seamless: false,
                snap: true,
                per_view: 1.0,
                autoplay_ms: 0,
                controls: CarouselControls::Outside,
                scroll_by: CarouselScrollBy::Slide,
                variant: CarouselVariant::Single,
                end_align: CarouselEndAlign::Flush,
                page_snap: CarouselPageSnap::Peek,
                item_sizing: CarouselItemSizing::Fraction,
                item_width_px: 0.0,
                show_dots: false,
                stack_sides: 1,
                drag_px: use_signal("t-car-vlen-first__drag", || 0.0f32),
                viewport_len,
                paused: use_signal("t-car-vlen-first__paused", || false),
            };
            assert!(
                !apply_viewport_len(&ctx, 400.0),
                "first measure (0→width) is not a Taffy resize"
            );
            assert!((ctx.viewport_len.peek() - 400.0).abs() < 0.1);
            assert!(
                apply_viewport_len(&ctx, 520.0),
                "real resize still reports applied"
            );
        });
    }

    #[test]
    fn carousel_sync_first_measure_is_not_layout_changed() {
        use crate::core::reactivity::create_root;
        create_root(|| {
            let mut root: Element = carousel_single("t-car-sync-vlen")
                .autoplay_ms(4000)
                .loop_slides(true)
                .slide(|| r#box().w("100%").h("4rem").into())
                .slide(|| r#box().w("100%").h("4rem").into())
                .slide(|| r#box().w("100%").h("4rem").into())
                .into();
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 600.0, 400.0, [-1.0, -1.0]);
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut root,
                &mut layout,
                &mut impact,
                600.0,
                400.0,
                [0.0, 0.0],
            );
            assert_ne!(
                impact,
                LayoutStatus::LayoutChanged,
                "first viewport measure must not FullClear the page (scrollbar flash)"
            );
            unregister_autoplay("t-car-sync-vlen");
        });
    }

    #[test]
    fn autoplay_does_not_advance_offscreen_or_on_enter() {
        use crate::core::reactivity::create_root;
        create_root(|| {
            let id = "t-car-ap-vis";
            let index = use_signal("t-car-ap-vis__idx", || 0usize);
            let count = use_signal("t-car-ap-vis__count", || 3usize);
            count.set(3);
            let ctx = CarouselCtx {
                id: id.into(),
                index: index.clone(),
                count,
                orientation: CarouselOrientation::Horizontal,
                loop_slides: true,
                seamless: false,
                snap: true,
                per_view: 1.0,
                autoplay_ms: 4000,
                controls: CarouselControls::Outside,
                scroll_by: CarouselScrollBy::Slide,
                variant: CarouselVariant::Single,
                end_align: CarouselEndAlign::Flush,
                page_snap: CarouselPageSnap::Peek,
                item_sizing: CarouselItemSizing::Fraction,
                item_width_px: 0.0,
                show_dots: false,
                stack_sides: 1,
                drag_px: use_signal("t-car-ap-vis__drag", || 0.0f32),
                viewport_len: use_signal("t-car-ap-vis__vlen", || 300.0f32),
                paused: use_signal("t-car-ap-vis__paused", || false),
            };
            register_autoplay(&ctx);

            begin_carousel_paint_frame();
            stamp_carousel_painted(id);
            tick_carousel_autoplay();
            assert_eq!(index.peek(), 0, "first on-screen tick restarts clock");

            set_next_autoplay_at(id, Some(Instant::now() - Duration::from_millis(1)));
            begin_carousel_paint_frame();
            tick_carousel_autoplay();
            assert_eq!(index.peek(), 0, "offscreen must not Advance");

            set_next_autoplay_at(id, Some(Instant::now() - Duration::from_millis(1)));
            begin_carousel_paint_frame();
            stamp_carousel_painted(id);
            tick_carousel_autoplay();
            assert_eq!(index.peek(), 0, "enter must not catch-up Advance");

            set_next_autoplay_at(id, Some(Instant::now() - Duration::from_millis(1)));
            begin_carousel_paint_frame();
            stamp_carousel_painted(id);
            tick_carousel_autoplay();
            assert_eq!(index.peek(), 1, "on-screen autoplay must still Advance");

            unregister_autoplay(id);
        });
    }

    #[test]
    fn stack_coverflow_centers_active_and_offsets_sides() {
        use crate::core::reactivity::create_root;
        create_root(|| {
            let mut root: Element = carousel_stack("t-car-stack-geo")
                .controls(CarouselControls::Outside)
                .stack_sides(1)
                .loop_slides(true)
                .slide(|| r#box().w("100%").h("8rem").into())
                .slide(|| r#box().w("100%").h("8rem").into())
                .slide(|| r#box().w("100%").h("8rem").into())
                .slide(|| r#box().w("100%").h("8rem").into())
                .slide(|| r#box().w("100%").h("8rem").into())
                .into();
            let mut layout = LayoutTree::new();
            for _ in 0..4 {
                let _ = layout.build_and_layout(&mut root, 700.0, 400.0, [-1.0, -1.0]);
                let mut impact = LayoutStatus::Clean;
                crate::core::tree::sync_reactive_elements_subtree(
                    &mut root,
                    &mut layout,
                    &mut impact,
                    700.0,
                    400.0,
                    [0.0, 0.0],
                );
            }
            fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
                if el.attrs.get("data-slot").map(|s| s.as_str()) == Some(slot) {
                    return Some(el);
                }
                el.children.iter().find_map(|c| find_slot(c, slot))
            }
            let content = find_slot(&root, SLOT_CONTENT).expect("content");
            let stage = content.layout_bounds;
            let stage_mid = stage[0] + stage[2] * 0.5;
            // The height is supported by the content (slide h=8rem), consistent with the Single viewport, and there should no longer be a fixed empty height.
            assert!(
                (stage[3] - 128.0).abs() < 24.0,
                "stack stage height should follow slide content (~8rem); got {}",
                stage[3]
            );
            let cards: Vec<_> = content
                .children
                .iter()
                .filter(|c| {
                    c.attrs.get("data-carousel-stack-measure").is_none()
                        && c.style.position == taffy::style::Position::Absolute
                })
                .collect();
            assert_eq!(cards.len(), 5);

            let mut center = None;
            let mut left = None;
            let mut right = None;
            for card in cards {
                let w = card.layout_bounds[2];
                let render_x = card.layout_bounds[0]
                    + card.style.translate[0]
                    + w * card.style.translate_pct[0];
                let mid = render_x + w * 0.5;
                let shell_sc = card.style.scale[0];
                let child_sc = card
                    .children
                    .first()
                    .map(|c| c.style.scale[0])
                    .unwrap_or(1.0);
                assert!(
                    card.style.shadow_color[3] < 0.01 && card.style.shadow_blur < 0.5,
                    "transparent shell must not paint box-shadow (ghost frame)"
                );
                assert!(
                    (child_sc - 1.0).abs() < 0.01,
                    "content scale stays 1; shell carries coverflow scale via ancestor_scale / paint_scale"
                );
                // Card width should be a percentage of the viewport (same mode as Single basis%), not fixed px/rem.
                assert!(
                    (w - stage[2] * 0.46).abs() < 8.0,
                    "card width should be ~46% of stage; w={w} stage={}",
                    stage[2]
                );
                if card.style.z_index >= 99_000 && card.style.opacity > 0.9 {
                    center = Some((
                        mid,
                        w,
                        card.style.translate[0],
                        shell_sc,
                        card.style.opacity,
                    ));
                    assert!(
                        card.style.bg_color[3] < 0.05,
                        "center shell stays transparent; occlusion is page_stack_bands"
                    );
                } else if card.style.opacity > 0.35 && card.style.translate[0] < -1.0 {
                    left = Some((card.style.translate[0].abs(), shell_sc, card.style.opacity));
                    assert!(
                        card.style.bg_color[3] < 0.05,
                        "side shell must stay transparent"
                    );
                } else if card.style.opacity > 0.35 && card.style.translate[0] > 1.0 {
                    right = Some((card.style.translate[0].abs(), shell_sc, card.style.opacity));
                    assert!(
                        card.style.bg_color[3] < 0.05,
                        "side shell must stay transparent"
                    );
                }
            }
            let (c_mid, c_w, c_tx, c_sc, c_op) = center.expect("active card");
            assert!(c_tx.abs() < 1.0, "active card must sit at tx=0, got {c_tx}");
            assert!(
                (c_mid - stage_mid).abs() < 4.0,
                "active card must be stage-centered; mid={c_mid} stage_mid={stage_mid}"
            );
            assert!(
                c_w < stage[2] * 0.55,
                "stack cards should be narrower than ~half stage; w={c_w} stage={}",
                stage[2]
            );
            assert!((c_sc - 1.0).abs() < 0.02 && (c_op - 1.0).abs() < 0.02);
            let (l_off, l_sc, l_op) = left.expect("left side card");
            let (r_off, r_sc, r_op) = right.expect("right side card");
            assert!(
                l_off >= c_w * 0.4 && r_off >= c_w * 0.4,
                "side offset must be ~half card width so sides peek clearly; left={l_off} right={r_off} card_w={c_w}"
            );
            assert!(
                l_sc < 0.9 && r_sc < 0.9 && l_op <= 0.73 && r_op <= 0.73,
                "sides must be scaled down and faded; L sc={l_sc} op={l_op}; R sc={r_sc} op={r_op}"
            );
        });
    }

    #[test]
    fn stack_step_keeps_scaled_side_cards_inside_narrow_viewport() {
        for w in [120.0_f32, 160.0, 200.0, 240.0, 400.0, 700.0] {
            let step = stack_step_px(w);
            let card_w = w * 0.46;
            let side_sc = 0.82;
            // The outer edge of the side card is relative to the left edge of the stage (centered at W/2 - step).
            let outer = w * 0.5 - step - card_w * side_sc * 0.5;
            assert!(
                outer >= -0.5,
                "narrow w={w}: step={step} must keep scaled side inside stage (outer={outer})"
            );
            // Regression: old clamp(min=56) would clip rounded corners when w≲234.
            if w < 230.0 {
                assert!(
                    step < 56.0,
                    "narrow w={w} must not force min step 56 (got {step})"
                );
            }
        }
        assert_eq!(stack_step_px(0.0), 0.0);
        assert_eq!(stack_step_px(4.0), 0.0);
    }

    #[test]
    fn stack_drag_snap_advances_index_and_offset() {
        use crate::core::reactivity::create_root;
        create_root(|| {
            let id = "t-car-stack-drag";
            let index = use_signal("t-car-stack-drag__idx", || 0usize);
            let count = use_signal("t-car-stack-drag__count", || 5usize);
            let drag_px = use_signal("t-car-stack-drag__drag", || 0.0f32);
            let viewport_len = use_signal("t-car-stack-drag__vlen", || 700.0f32);
            let step = stack_step_px(700.0);
            begin_carousel_drag(CarouselDrag {
                id: id.into(),
                start_ptr: 0.0,
                base_offset: 0.0,
                index: index.clone(),
                count: count.clone(),
                per_view: 1.0,
                wraps: true,
                loop_clones: 0,
                end_align: CarouselEndAlign::Flush,
                variant: CarouselVariant::Stack,
                item_sizing: CarouselItemSizing::Fraction,
                item_width_px: 0.0,
                scroll_by: CarouselScrollBy::Slide,
                page_snap: CarouselPageSnap::Peek,
                snap: true,
                viewport_len: viewport_len.clone(),
                drag_px: drag_px.clone(),
                orientation: CarouselOrientation::Horizontal,
                autoplay_ms: 0,
            });
            // Drag half a frame → next picture.
            drag_px.set(-(step * 0.6));
            snap_offset_to(id, -(step * 0.6));
            end_carousel_drag();
            assert_eq!(
                index.peek(),
                1,
                "stack drag past half step should land on next"
            );
            assert!(
                (offset_target(id).peek() + step).abs() < 1.0,
                "stack ideal offset should be -index×step; got {}",
                offset_target(id).peek()
            );
        });
    }

    #[test]
    fn snap_false_forced_true_when_autoplay() {
        use crate::core::reactivity::create_root;
        create_root(|| {
            let el: Element = carousel("t-car-snap-ap")
                .snap(false)
                .autoplay_ms(3000)
                .slide(|| r#box().w("100%").h("4rem").into())
                .into();
            assert_eq!(el.attrs.get("data-snap").map(|s| s.as_str()), Some("1"));
        });
    }

    /// Return: When narrow screen + controls=outside, the first item of Single should be aligned with the full A, instead of exposing the B/C.
    #[test]
    fn narrow_outside_single_parks_on_first_slide_full_width() {
        use crate::core::reactivity::create_root;
        use crate::layout::dim_kind;
        use crate::layout::DimKind;
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let id = format!("t-car-narrow-{}", N.fetch_add(1, Ordering::Relaxed));
        let id_for_spring = id.clone();
        create_root(move || {
            let carousel_el: Element = carousel(id)
                .loop_slides(true)
                .controls(CarouselControls::Outside)
                .variant(CarouselVariant::Single)
                .slide(|| r#box().w("100%").h("4rem").into())
                .slide(|| r#box().w("100%").h("4rem").into())
                .slide(|| r#box().w("100%").h("4rem").into())
                .into();
            let main: Element = r#box()
                .flex_grow(1.0)
                .min_w(0.0)
                .p("1rem")
                .child(carousel_el)
                .into();
            let side: Element = r#box().w(208.0).h("100%").into();
            let mut root: Element = hstack().w("100%").h("100%").children([side, main]).into();
            let mut layout = LayoutTree::new();
            for _ in 0..4 {
                let _ = layout.build_and_layout(&mut root, 500.0, 400.0, [-1.0, -1.0]);
                let mut impact = LayoutStatus::Clean;
                crate::core::tree::sync_reactive_elements_subtree(
                    &mut root,
                    &mut layout,
                    &mut impact,
                    500.0,
                    400.0,
                    [0.0, 0.0],
                );
            }

            fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
                if el.attrs.get("data-slot").map(|s| s.as_str()) == Some(slot) {
                    return Some(el);
                }
                el.children.iter().find_map(|c| find_slot(c, slot))
            }
            let content = find_slot(&root, SLOT_CONTENT).expect("content");
            let vp = content.layout_bounds[2];
            assert!(vp > 40.0, "viewport should be measurable, got {vp}");

            let track = content.children.first().expect("track");
            let offset = track.style.translate[0];
            let tgt = offset_target(&id_for_spring).peek();
            assert!(
                (tgt - offset).abs() < 1.0 || track.transition_snapshot.has_active_tweens(),
                "track translate should track offset target (or be mid CSS tween): translate={offset} target={tgt}"
            );

            let mut widths = Vec::new();
            for item in &track.children {
                if item.attrs.get(ATTR_SLOT).map(|s| s.as_str()) != Some(SLOT_ITEM) {
                    continue;
                }
                match dim_kind(item.style.size.width) {
                    DimKind::Percent(p) => assert!(
                        (p - 1.0).abs() < 0.001,
                        "single slide basis must be 100%, got {p}"
                    ),
                    other => panic!("expected percent width, got {other:?}"),
                }
                widths.push(item.layout_bounds[2]);
                assert!(
                    (item.layout_bounds[2] - vp).abs() < 3.0,
                    "layout width must fill viewport ({vp}), got {}; widths={widths:?}",
                    item.layout_bounds[2]
                );
            }
            assert!(
                widths.len() >= 3,
                "expected real+clone slides, got {}",
                widths.len()
            );
            assert!(
                (offset + vp).abs() < 3.0,
                "index0 must park at -viewport; offset={offset} vp={vp} widths={widths:?}"
            );
        });
    }

    /// User reproduction size 828×320: sidebar + max_w 36rem + outside, the first item must be full.
    #[test]
    fn size_828x320_outside_single_shows_full_first_slide() {
        use crate::core::reactivity::create_root;
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let id = format!("t-car-828-{}", N.fetch_add(1, Ordering::Relaxed));
        let id_spring = id.clone();
        create_root(move || {
            let carousel_el: Element = carousel(id)
                .loop_slides(true)
                .controls(CarouselControls::Outside)
                .variant(CarouselVariant::Single)
                .slide(|| r#box().w("100%").h("8rem").into())
                .slide(|| r#box().w("100%").h("8rem").into())
                .slide(|| r#box().w("100%").h("8rem").into())
                .into();
            let wrap: Element = r#box().w("100%").max_w("36rem").child(carousel_el).into();
            let main: Element = r#box()
                .flex_grow(1.0)
                .min_w(0.0)
                .p("1.5rem")
                .child(wrap)
                .into();
            let side: Element = r#box().w(208.0).h("100%").into();
            let mut root: Element = hstack().w("100%").h("100%").children([side, main]).into();
            let mut layout = LayoutTree::new();
            for _ in 0..6 {
                let _ = layout.build_and_layout(&mut root, 828.0, 320.0, [-1.0, -1.0]);
                let mut impact = LayoutStatus::Clean;
                crate::core::tree::sync_reactive_elements_subtree(
                    &mut root,
                    &mut layout,
                    &mut impact,
                    828.0,
                    320.0,
                    [0.0, 0.0],
                );
            }

            fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
                if el.attrs.get("data-slot").map(|s| s.as_str()) == Some(slot) {
                    return Some(el);
                }
                el.children.iter().find_map(|c| find_slot(c, slot))
            }
            let content = find_slot(&root, SLOT_CONTENT).expect("content");
            let vp = content.layout_bounds[2];
            let track = content.children.first().expect("track");
            let offset = track.style.translate[0];
            let tgt = offset_target(&id_spring).peek();
            let mut widths = Vec::new();
            for item in &track.children {
                if item.attrs.get(ATTR_SLOT).map(|s| s.as_str()) != Some(SLOT_ITEM) {
                    continue;
                }
                widths.push(item.layout_bounds[2]);
            }
            eprintln!("828x320 diag: vp={vp} offset={offset} target={tgt} widths={widths:?}");
            assert!(vp > 100.0, "vp={vp}");
            assert!(
                widths.iter().all(|w| (*w - vp).abs() < 3.0),
                "percent-basis slides must lay out to viewport; vp={vp} widths={widths:?}"
            );
            assert!(
                (offset + vp).abs() < 3.0 && (tgt + vp).abs() < 3.0,
                "index0 must be -vp; offset={offset} target={tgt} vp={vp} widths={widths:?}"
            );
        });
    }

    /// Wide→Narrow: The remaining large slot width/old offset must not allow index0 to stop at B/C.
    #[test]
    fn resize_wide_to_narrow_realigns_single_loop_track() {
        use crate::core::reactivity::create_root;
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let id = format!("t-car-resize-{}", N.fetch_add(1, Ordering::Relaxed));
        create_root(move || {
            let carousel_el: Element = carousel(id)
                .loop_slides(true)
                .controls(CarouselControls::Outside)
                .variant(CarouselVariant::Single)
                .slide(|| r#box().w("100%").h("4rem").into())
                .slide(|| r#box().w("100%").h("4rem").into())
                .slide(|| r#box().w("100%").h("4rem").into())
                .into();
            let mut root: Element = r#box().w("100%").child(carousel_el).into();
            let mut layout = LayoutTree::new();

            fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
                if el.attrs.get("data-slot").map(|s| s.as_str()) == Some(slot) {
                    return Some(el);
                }
                el.children.iter().find_map(|c| find_slot(c, slot))
            }
            fn settle(root: &mut Element, layout: &mut LayoutTree, w: f32, h: f32) {
                for _ in 0..4 {
                    let _ = layout.build_and_layout(root, w, h, [-1.0, -1.0]);
                    let mut impact = LayoutStatus::Clean;
                    crate::core::tree::sync_reactive_elements_subtree(
                        root,
                        layout,
                        &mut impact,
                        w,
                        h,
                        [0.0, 0.0],
                    );
                }
            }

            settle(&mut root, &mut layout, 900.0, 400.0);
            let wide_vp = find_slot(&root, SLOT_CONTENT).unwrap().layout_bounds[2];
            assert!(wide_vp > 400.0, "wide vp={wide_vp}");

            settle(&mut root, &mut layout, 420.0, 400.0);
            let content = find_slot(&root, SLOT_CONTENT).expect("content");
            let vp = content.layout_bounds[2];
            let track = content.children.first().expect("track");
            let offset = track.style.translate[0];
            let mut widths = Vec::new();
            for item in &track.children {
                if item.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_ITEM) {
                    widths.push(item.layout_bounds[2]);
                }
            }
            assert!(
                widths.iter().all(|w| (*w - vp).abs() < 2.0),
                "after shrink, slides must match vp={vp}, widths={widths:?} (was wide_vp={wide_vp})"
            );
            assert!(
                (offset + vp).abs() < 3.0,
                "after shrink, index0 offset must be -vp; offset={offset} vp={vp} widths={widths:?}"
            );
        });
    }

    /// Regression: Clone track targets must not be set to a false ideal (≈0) of step≈1 when the viewport is not ready.
    #[test]
    fn looping_build_does_not_park_offset_at_fake_step_before_viewport() {
        use crate::core::reactivity::create_root;
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let id = format!("t-car-pre-vp-{}", N.fetch_add(1, Ordering::Relaxed));
        let tgt = create_root(|| {
            let id2 = id.clone();
            let _el: Element = carousel(id)
                .loop_slides(true)
                .slide(|| r#box().w("100%").h("4rem").into())
                .slide(|| r#box().w("100%").h("4rem").into())
                .slide(|| r#box().w("100%").h("4rem").into())
                .into();
            offset_target(&id2).peek()
        })
        .1;
        assert!(
            tgt.abs() < 0.01,
            "pre-viewport offset target must stay at 0, got {tgt}"
        );
    }
}
