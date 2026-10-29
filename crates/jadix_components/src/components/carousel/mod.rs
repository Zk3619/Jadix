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
//! - For drag sessions see [`utils::drag::pointer`](crate::utils::drag::pointer)
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
use crate::utils::lucide_glyph;
use crate::utils::motion::{clear_motion, ensure_motion_transitions, snap_motion_baseline};
use crate::utils::resolve_instance_id;
use crate::utils::take_packed;
use crate::utils::text_node::build_icon_text_node;

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
#[cfg(all(test, feature = "product-tests"))]
pub(crate) use drag::{begin_carousel_paint_frame, tick_carousel_autoplay};
#[cfg(all(test, feature = "product-tests"))]
pub(crate) use drag::{clear_carousel_drag_if_any, tick_carousel_drag};
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
    let n = el.children.len();
    for i in 0..n {
        crate::core::tree::walk_elements_mut(&mut el.children[i], |child| {
            if (child.style.scale[0] - 1.0).abs() > 0.001
                || (child.style.scale[1] - 1.0).abs() > 0.001
            {
                child.style.scale = [1.0, 1.0];
            }
            if child.text.is_some() {
                child.text_style.white_space = crate::style::WhiteSpace::Nowrap;
            }
        });
    }
}

/// Interpolate scale/transparency/z at a continuous distance from center (drag to avoid hard-cut transitions of ad=0.5).
fn stack_card_appearance(ad: f32, sides: f32, idx: usize, count: usize) -> (f32, f32, i32) {
    // ad=0 → sc1/op1; ad=1 → ≈0.82/0.72; ad≥2 → lower limit.
    let sc = (1.0 - 0.18 * ad).clamp(0.72, 1.0);
    // A side card must fade out as it leaves, rather than disappear on the
    // first drag pixel past `sides`. End the fade before the ring distance
    // flips sides (notably with three cards).
    let fade_span = (count as f32 * 0.5 - sides).clamp(0.01, 1.0);
    let visibility = ((sides + fade_span - ad) / fade_span).clamp(0.0, 1.0);
    if visibility <= 0.0 {
        return (sc, 0.0, 0);
    }
    let op = if ad <= 1.0 {
        (1.0 - 0.28 * ad).clamp(0.72, 1.0)
    } else {
        (0.72 - 0.14 * (ad - 1.0)).clamp(0.42, 0.72)
    };
    // Fine-grained z + slot breaks juxtaposition (z used to be the same when the preceding and following items ad were close, and the band was out of order, causing the text to "stop following").
    let z = ((10_000.0 - ad * 1_000.0) * 10.0).round() as i32 + (count - idx) as i32;
    (sc, op * visibility, z.max(1))
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

type CarouselKids = (
    Option<Element>,
    Option<Element>,
    Option<Element>,
    Option<Element>,
    Vec<Element>,
);

fn classify_carousel_kids(kids: Vec<Element>) -> CarouselKids {
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
    let stage_body = content.unwrap_or_default();
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
            12.0 * crate::layout::dimension::css_root_font_size_px()
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
    #[cfg(all(test, feature = "product-tests"))]
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

    if autoplay_ms > 0 {
        bump_autoplay(&ctx);
        let autoplay_ctx = ctx.clone();
        crate::authoring::deadline::attach(
            &mut root,
            crate::authoring::deadline::Visibility::Visible,
            move |context| poll_autoplay(&autoplay_ctx, context),
        );
    }
    #[cfg(all(test, feature = "product-tests"))]
    {
        let unreg_id = id.clone();
        root = root.on_unmount(move || unregister_autoplay(&unreg_id));
    }

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
#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/mod.rs"]
mod tests;
