//! # MessageScroller
//!
//! ShadCN-style conversation scroller. Aligns with
//! `Reference/ShadCN-UI/.../bases/base/ui/message-scroller.tsx`.
//!
//! Chrome (https://ui.shadcn.com/docs/components/base/message-scroller):
//! - Provider: headless scroll state (auto-scroll, default position, jump).
//!   Does not own messages / AI / transport.
//! - Root: `relative flex size-full min-h-0 flex-col overflow-hidden`
//! - Viewport: `overflow-y-auto size-full min-h-0` (engine scroll viewport)
//! - Content: `flex h-max flex-col`
//! - Item: `min-w-0 shrink-0`; `messageId`; optional `scrollAnchor`
//! - Button: `direction=start|end`; inert (`opacity:0`, no pointer events)
//!   until there is content in that direction
//!
//! ```text
//! MessageScrollerProvider          // context · autoScroll · defaultScrollPosition
//! └── MessageScroller              // relative column · overflow hidden
//!     ├── MessageScrollerViewport  // overflow-y-auto · size-full
//!     │   └── MessageScrollerContent
//!     │       └── MessageScrollerItem+
//!     └── MessageScrollerButton    // absolute jump · start | end
//! ```
//!
//! ## Usage (dual style · ADR-9)
//! ```ignore
//! // A · RSX / tags (camelCase aliases: autoScroll, defaultScrollPosition, messageId)
//! <MessageScrollerProvider autoScroll={true} defaultScrollPosition="end">
//!     <MessageScroller>
//!         <MessageScrollerViewport>
//!             <MessageScrollerContent>
//!                 <MessageScrollerItem messageId="1">
//!                     <Message align="start">…</Message>
//!                 </MessageScrollerItem>
//!             </MessageScrollerContent>
//!         </MessageScrollerViewport>
//!         <MessageScrollerButton direction="end" />
//!     </MessageScroller>
//! </MessageScrollerProvider>
//!
//! // B · Builder
//! Element::from(
//!     message_scroller_provider()
//!         .auto_scroll(true)
//!         .default_scroll_position("end")
//!         .child(
//!             message_scroller()
//!                 .h("20rem")
//!                 .child(
//!                     message_scroller_viewport().child(
//!                         message_scroller_content().child(
//!                             message_scroller_item().message_id("1").child(message_row),
//!                         ),
//!                     ),
//!                 )
//!                 .child(message_scroller_button().direction("end")),
//!         ),
//! )
//! ```
//!
//! ## Notes
//! - `<MessageScrollerProvider>` is a compound `ui!` scope
//!   ([`message_scroller_scope`]) so Viewport / Button see context while they
//!   build. Parts use the generic PascalCase → `*Props` path.
//! - `autoScroll` follows new content **only while at the live end**. Wheel /
//!   jump away releases it; [`MessageScrollerButton`] / [`MessageScrollerCtx::scroll_to_end`]
//!   re-engages. Returning to the end also re-engages.
//! - `defaultScrollPosition`: `"end"` (default) | `"start"` | `"last-anchor"`.
//!   Applied once when the viewport first has a measured size.
//! - `scrollAnchor` on Item: last-anchor opening uses that row (minus
//!   `scrollPreviousItemPeek`). IntersectionObserver visibility is not ported;
//!   [`use_message_scroller_visibility`] returns an empty stub.
//! - Prepend-history offset compensation is not applied; append + live-edge
//!   follow is the supported growth path.

use std::sync::OnceLock;

use rui_macros::component;

use crate::components::base::BaseComponentBuilder;
use crate::components::button::button;
use crate::components::scrollbar::{scrollbar, ScrollbarOrientation};
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_signal, with_child_owner, Context, Signal,
};
use crate::core::scroll_viewport::{
    activate_scroll_viewport, scroll_content_h_signal_key, scroll_offset_y_signal_key,
    scroll_target_y_signal_key, scroll_viewport_h_signal_key, SCROLL_BEHAVIOR_JUMP,
};
use crate::core::tree::{parse_text_color, Element, LayoutStatus, ATTR_POINTER_EVENTS};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::r#box::vstack;
use crate::style::with_class_list;
use crate::tokens;
use crate::utils::resolve_instance_id;
use crate::utils::take_fragment;
use crate::utils::take_packed;
use crate::utils::text_node::build_lucide_icon_text_node;

const ATTR_SLOT: &str = "data-slot";
const ATTR_DIRECTION: &str = "data-direction";
const ATTR_ACTIVE: &str = "data-active";
const ATTR_MESSAGE_ID: &str = "data-message-id";
const ATTR_SCROLL_ANCHOR: &str = "data-scroll-anchor";
const ATTR_AUTOSCROLLING: &str = "data-autoscrolling";

const SLOT_ROOT: &str = "message-scroller";
const SLOT_VIEWPORT: &str = "message-scroller-viewport";
const SLOT_CONTENT: &str = "message-scroller-content";
const SLOT_ITEM: &str = "message-scroller-item";
const SLOT_BUTTON: &str = "message-scroller-button";

const DEFAULT_EDGE_THRESHOLD: f32 = 8.0;
const DEFAULT_PREVIOUS_PEEK: f32 = 48.0;
const BUTTON_INSET: &str = "1rem";

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Opening position, applied once on the first measured layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MessageScrollerDefaultPosition {
    Start,
    #[default]
    End,
    LastAnchor,
}

impl MessageScrollerDefaultPosition {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::End => "end",
            Self::LastAnchor => "last-anchor",
        }
    }
}

impl From<&str> for MessageScrollerDefaultPosition {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or(Self::End)
    }
}

impl From<String> for MessageScrollerDefaultPosition {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl std::str::FromStr for MessageScrollerDefaultPosition {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "start" | "top" => Self::Start,
            "last-anchor" | "last_anchor" | "anchor" => Self::LastAnchor,
            _ => Self::End,
        })
    }
}

/// Jump-to-edge button direction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MessageScrollerDirection {
    Start,
    #[default]
    End,
}

impl MessageScrollerDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::End => "end",
        }
    }

    pub fn is_end(self) -> bool {
        self == Self::End
    }
}

impl From<&str> for MessageScrollerDirection {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or(Self::End)
    }
}

impl From<String> for MessageScrollerDirection {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl std::str::FromStr for MessageScrollerDirection {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "start" | "top" => Self::Start,
            _ => Self::End,
        })
    }
}

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

/// Provider state shared with Viewport / Button / hooks.
#[derive(Clone)]
pub struct MessageScrollerCtx {
    pub id: String,
    pub viewport_id: String,
    pub auto_scroll: bool,
    pub default_scroll_position: MessageScrollerDefaultPosition,
    pub scroll_edge_threshold: f32,
    pub scroll_previous_item_peek: f32,
    pub following: Signal<bool>,
    pub initialized: Signal<bool>,
    last_content_h: Signal<f32>,
}

impl MessageScrollerCtx {
    /// Jump (or smooth-seek) to the live end and re-engage auto-scroll.
    pub fn scroll_to_end(&self, jump: bool) {
        self.following.set(true);
        let max = max_scroll_y(&self.viewport_id);
        set_scroll_y(&self.viewport_id, max, jump);
    }

    /// Jump (or smooth-seek) to the start and release auto-scroll.
    pub fn scroll_to_start(&self, jump: bool) {
        self.following.set(false);
        set_scroll_y(&self.viewport_id, 0.0, jump);
    }

    pub fn is_at_end(&self) -> bool {
        let max = max_scroll_y(&self.viewport_id);
        current_offset_y(&self.viewport_id) >= (max - self.scroll_edge_threshold).max(0.0)
    }

    pub fn is_at_start(&self) -> bool {
        current_offset_y(&self.viewport_id) <= self.scroll_edge_threshold
    }
}

fn message_scroller_context() -> Context<MessageScrollerCtx> {
    static CTX: OnceLock<Context<MessageScrollerCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<MessageScrollerCtx>)
}

/// ShadCN `useMessageScroller`.
pub fn use_message_scroller() -> Option<MessageScrollerCtx> {
    use_context(message_scroller_context())
}

fn require_message_scroller(part: &str) -> MessageScrollerCtx {
    use_message_scroller().unwrap_or_else(|| {
        panic!("{part} must be used within <MessageScrollerProvider> / message_scroller_scope");
    })
}

/// Jump helpers bound to the nearest Provider (ShadCN `useMessageScrollerScrollable`).
#[derive(Clone)]
pub struct MessageScrollerScrollable {
    pub ctx: MessageScrollerCtx,
}

impl MessageScrollerScrollable {
    pub fn scroll_to_end(&self) {
        self.ctx.scroll_to_end(true);
    }

    pub fn scroll_to_start(&self) {
        self.ctx.scroll_to_start(true);
    }
}

pub fn use_message_scroller_scrollable() -> Option<MessageScrollerScrollable> {
    use_message_scroller().map(|ctx| MessageScrollerScrollable { ctx })
}

/// Visibility registry stub (ShadCN IntersectionObserver is not ported).
#[derive(Clone, Debug, Default)]
pub struct MessageScrollerVisibility {
    pub in_view: Vec<String>,
}

pub fn use_message_scroller_visibility() -> Option<MessageScrollerVisibility> {
    use_message_scroller().map(|_| MessageScrollerVisibility {
        in_view: Vec::new(),
    })
}

// ---------------------------------------------------------------------------
// Scroll helpers
// ---------------------------------------------------------------------------

fn max_scroll_y(viewport_id: &str) -> f32 {
    let ch = use_signal(&scroll_content_h_signal_key(viewport_id), || 0.0_f32).peek();
    let vh = use_signal(&scroll_viewport_h_signal_key(viewport_id), || 0.0_f32).peek();
    (ch - vh).max(0.0)
}

fn current_offset_y(viewport_id: &str) -> f32 {
    use_signal(&scroll_offset_y_signal_key(viewport_id), || 0.0_f32).peek()
}

fn content_and_viewport_h(viewport_id: &str) -> (f32, f32) {
    let ch = use_signal(&scroll_content_h_signal_key(viewport_id), || 0.0_f32).peek();
    let vh = use_signal(&scroll_viewport_h_signal_key(viewport_id), || 0.0_f32).peek();
    (ch, vh)
}

fn set_scroll_y(viewport_id: &str, y: f32, jump: bool) {
    let y = y.max(0.0);
    let ty = use_signal(&scroll_target_y_signal_key(viewport_id), || 0.0_f32);
    if jump {
        use_signal(&scroll_offset_y_signal_key(viewport_id), || 0.0_f32).set_silent(y);
        ty.set_silent(y);
    } else {
        ty.set_silent(y);
    }
}

fn is_anchor_item(el: &Element) -> bool {
    el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_ITEM)
        && el
            .attrs
            .get(ATTR_SCROLL_ANCHOR)
            .map(|s| s == "true")
            .unwrap_or(false)
}

fn is_message_item(el: &Element) -> bool {
    el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_ITEM)
}

/// Bottom edge of laid-out [`SLOT_ITEM`] rows inside [`SLOT_CONTENT`], viewport-local px.
/// Caps inflated `content_h` from `min-h-full` / scrollport metrics so End scroll does not
/// land in an empty tail (band-cull hides every turn).
pub(crate) fn layout_scrollable_content_height(
    viewport_el: &Element,
    layout_tree: &LayoutTree,
) -> Option<f32> {
    let vnid = viewport_el.layout_node?;
    let child_nodes = layout_tree.taffy.children(vnid).ok()?;

    for (i, child) in viewport_el.children.iter().enumerate() {
        if child.attrs.get(ATTR_SLOT).map(String::as_str) != Some(SLOT_CONTENT) {
            continue;
        }
        let content_nid = *child_nodes.get(i)?;
        let content_lay = layout_tree.taffy.layout(content_nid).ok()?;
        let origin_y = content_lay.location.y;

        let mut found_item = false;
        let mut max_bottom = origin_y;
        walk_item_bottoms(child, layout_tree, origin_y, &mut found_item, &mut max_bottom);
        if !found_item {
            return None;
        }
        let pad_b = crate::layout::lp_length_px(child.style.padding.bottom)
            .unwrap_or(0.0)
            .max(0.0);
        return Some(max_bottom + pad_b);
    }
    None
}

fn walk_item_bottoms(
    el: &Element,
    layout_tree: &LayoutTree,
    base_y: f32,
    found_item: &mut bool,
    max_bottom: &mut f32,
) {
    let Some(nid) = el.layout_node else {
        for child in &el.children {
            walk_item_bottoms(child, layout_tree, base_y, found_item, max_bottom);
        }
        return;
    };
    let Ok(lay) = layout_tree.taffy.layout(nid) else {
        return;
    };
    let y = base_y + lay.location.y;
    let bottom = y + lay.size.height;
    if is_message_item(el) {
        *found_item = true;
        *max_bottom = max_bottom.max(bottom);
    }
    for child in &el.children {
        walk_item_bottoms(child, layout_tree, y, found_item, max_bottom);
    }
}

fn last_anchor_scroll_y(viewport: &Element, layout_tree: &LayoutTree, peek: f32) -> Option<f32> {
    let mut found: Option<(f32, f32)> = None;
    for child in &viewport.children {
        walk_anchor(child, layout_tree, 0.0, &mut found);
    }
    found.map(|(y, _h)| (y - peek).max(0.0))
}

fn walk_anchor(
    el: &Element,
    layout_tree: &LayoutTree,
    parent_abs_y: f32,
    found: &mut Option<(f32, f32)>,
) {
    let (rel_y, h) = el
        .layout_node
        .and_then(|nid| layout_tree.taffy.layout(nid).ok())
        .map(|l| (l.location.y, l.size.height))
        .unwrap_or((0.0, 0.0));
    let abs_y = parent_abs_y + rel_y;
    if is_anchor_item(el) {
        *found = Some((abs_y, h));
    }
    for child in &el.children {
        walk_anchor(child, layout_tree, abs_y, found);
    }
}

fn apply_viewport_scroll(
    el: &mut Element,
    layout_tree: &LayoutTree,
    ctx: &MessageScrollerCtx,
) -> LayoutStatus {
    let signal_metrics = content_and_viewport_h(&ctx.viewport_id);
    let layout_metrics =
        crate::layout::layout_tree::scroll_viewport_content_metrics(layout_tree, el);
    let item_extent_h = layout_scrollable_content_height(el, layout_tree);

    let viewport_h = layout_metrics
        .map(|(_, vh)| vh)
        .filter(|vh| *vh > 1.0)
        .or_else(|| {
            if signal_metrics.1 > 1.0 {
                Some(signal_metrics.1)
            } else {
                None
            }
        });
    let Some(viewport_h) = viewport_h else {
        return LayoutStatus::Clean;
    };

    // Item bottoms are the only trustworthy scroll extent (`min-h-full` inflates metrics).
    let scroll_extent_h = item_extent_h.map(|h| h.max(viewport_h));
    let content_h = scroll_extent_h.unwrap_or(viewport_h);

    // Keep scroll signals aligned before collect (post-render `content_h` lags one frame).
    use_signal(&scroll_content_h_signal_key(&ctx.viewport_id), || 0.0_f32)
        .set_silent(content_h);
    use_signal(&scroll_viewport_h_signal_key(&ctx.viewport_id), || 0.0_f32)
        .set_silent(viewport_h);

    let max = scroll_extent_h
        .map(|h| (h - viewport_h).max(0.0))
        .unwrap_or(0.0);
    let threshold = ctx.scroll_edge_threshold;
    let mut changed = false;

    // Stale signal max can overshoot layout (min-h-full / reactive rebuild / band cull).
    let offset = current_offset_y(&ctx.viewport_id);
    if offset > max + 0.5 {
        set_scroll_y(&ctx.viewport_id, max, true);
        changed = true;
    }
    let offset = current_offset_y(&ctx.viewport_id);
    let at_end = offset >= (max - threshold).max(0.0);

    if !ctx.initialized.peek() {
        // Defer End/LastAnchor until turns are laid out — avoids scrolling into empty tail.
        if item_extent_h.is_none() {
            return if changed {
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            };
        }
        let target = match ctx.default_scroll_position {
            MessageScrollerDefaultPosition::Start => 0.0,
            MessageScrollerDefaultPosition::End => max,
            MessageScrollerDefaultPosition::LastAnchor => {
                last_anchor_scroll_y(el, layout_tree, ctx.scroll_previous_item_peek).unwrap_or(0.0)
            }
        };
        set_scroll_y(&ctx.viewport_id, target.min(max), true);
        let follow =
            ctx.auto_scroll && ctx.default_scroll_position == MessageScrollerDefaultPosition::End;
        ctx.following.set_silent(follow);
        ctx.initialized.set_silent(true);
        changed = true;
    } else if ctx.auto_scroll {
        let grew = content_h > ctx.last_content_h.peek() + 0.5;
        if ctx.following.peek() {
            if grew {
                set_scroll_y(&ctx.viewport_id, max, true);
                changed = true;
            } else if !at_end {
                ctx.following.set_silent(false);
                changed = true;
            }
        } else if at_end {
            ctx.following.set_silent(true);
            changed = true;
        }
    }
    ctx.last_content_h.set_silent(content_h);

    let following = ctx.following.peek();
    let want = if following { "true" } else { "false" };
    if el.attrs.get(ATTR_AUTOSCROLLING).map(String::as_str) != Some(want) {
        el.attrs
            .insert(ATTR_AUTOSCROLLING.to_string(), want.to_string());
        changed = true;
    }

    if changed {
        LayoutStatus::StyleChanged
    } else {
        LayoutStatus::Clean
    }
}

fn apply_button_active(
    el: &mut Element,
    ctx: &MessageScrollerCtx,
    direction: MessageScrollerDirection,
) -> LayoutStatus {
    let max = max_scroll_y(&ctx.viewport_id);
    let offset = current_offset_y(&ctx.viewport_id);
    let threshold = ctx.scroll_edge_threshold;
    let overflow = max > threshold;
    let active = overflow
        && match direction {
            MessageScrollerDirection::End => offset < (max - threshold).max(0.0),
            MessageScrollerDirection::Start => offset > threshold,
        };

    let opacity = if active { 1.0 } else { 0.0 };
    let scale = if active { 1.0 } else { 0.95 };
    let ty = if active {
        0.0
    } else if direction.is_end() {
        1.0
    } else {
        -1.0
    };
    let active_attr = if active { "true" } else { "false" };

    let mut changed = false;
    if (el.style.opacity - opacity).abs() > 0.01 {
        el.style.opacity = opacity;
        changed = true;
    }
    if (el.style.scale[0] - scale).abs() > 0.01 || (el.style.scale[1] - scale).abs() > 0.01 {
        el.style.scale = [scale, scale];
        changed = true;
    }
    let pct = el.style.translate_pct;
    if (pct[0] + 0.5).abs() > 0.01 || (pct[1] - ty).abs() > 0.01 {
        el.style.translate_pct = [-0.5, ty];
        changed = true;
    }
    if el.attrs.get(ATTR_ACTIVE).map(String::as_str) != Some(active_attr) {
        el.attrs
            .insert(ATTR_ACTIVE.to_string(), active_attr.to_string());
        changed = true;
    }
    let pe = if active { "auto" } else { "none" };
    if el.attrs.get(ATTR_POINTER_EVENTS).map(String::as_str) != Some(pe) {
        if active {
            el.attrs.remove(ATTR_POINTER_EVENTS);
        } else {
            el.attrs
                .insert(ATTR_POINTER_EVENTS.to_string(), "none".to_string());
        }
        changed = true;
    }

    if changed {
        LayoutStatus::StyleChanged
    } else {
        LayoutStatus::Clean
    }
}

// ---------------------------------------------------------------------------
// Paint helpers
// ---------------------------------------------------------------------------

fn apply_optional_class(mut el: Element, class: &str) -> Element {
    if !class.trim().is_empty() {
        el = with_class_list(el, class.trim());
    }
    el
}

fn apply_base(
    mut el: Element,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
) -> Element {
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

fn packed_children(children: Vec<Element>) -> Option<Element> {
    if children.is_empty() {
        None
    } else {
        let mut frag = Element::new();
        frag.set_attr("rui_fragment", "1");
        Some(frag.children(children))
    }
}

type DelayedEl = Box<dyn FnOnce() -> Element>;

fn delay_el(child: impl Into<Element> + 'static) -> DelayedEl {
    Box::new(move || child.into())
}

fn collect_delayed(children: Vec<DelayedEl>) -> Vec<Element> {
    let mut out = Vec::new();
    for f in children {
        out.extend(take_packed(f()));
    }
    out
}

fn default_arrow_icon(direction: MessageScrollerDirection) -> Element {
    let mut icon = build_lucide_icon_text_node("arrow-down");
    icon = icon
        .text_size(16.0)
        .font_weight(400.0)
        .text_color(parse_text_color("foreground"));
    if !direction.is_end() {
        icon = icon.rotate(180.0);
    }
    icon
}

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

/// Provide MessageScroller context, then build Root / Viewport / Button.
///
/// Used by `ui!` `<MessageScrollerProvider>` and [`message_scroller_provider`].
pub fn message_scroller_scope(
    id: impl Into<String>,
    auto_scroll: bool,
    default_scroll_position: impl Into<MessageScrollerDefaultPosition>,
    scroll_edge_threshold: f32,
    scroll_previous_item_peek: f32,
    class: Option<String>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = resolve_instance_id(id, "message-scroller");
    let default_scroll_position = default_scroll_position.into();
    let scroll_edge_threshold = if scroll_edge_threshold <= 0.0 {
        DEFAULT_EDGE_THRESHOLD
    } else {
        scroll_edge_threshold
    };
    let scroll_previous_item_peek = if scroll_previous_item_peek < 0.0 {
        DEFAULT_PREVIOUS_PEEK
    } else {
        scroll_previous_item_peek
    };

    with_child_owner(|| {
        let viewport_id = format!("{id}:viewport");
        let following = use_signal(&format!("{id}:following"), || auto_scroll);
        let initialized = use_signal(&format!("{id}:init"), || false);
        let last_content_h = use_signal(&format!("{id}:last-ch"), || 0.0_f32);
        // Reactive gallery rebuilds the provider while signals persist — re-run End anchoring
        // and drop stale scroll offsets that would band-cull every turn into blank space.
        initialized.set_silent(false);
        last_content_h.set_silent(0.0);
        use_signal(&scroll_offset_y_signal_key(&viewport_id), || 0.0_f32).set_silent(0.0);
        use_signal(&scroll_target_y_signal_key(&viewport_id), || 0.0_f32).set_silent(0.0);
        provide_context(
            message_scroller_context(),
            MessageScrollerCtx {
                id: id.clone(),
                viewport_id,
                auto_scroll,
                default_scroll_position,
                scroll_edge_threshold,
                scroll_previous_item_peek,
                following,
                initialized,
                last_content_h,
            },
        );
        let kids = take_fragment(body());
        let mut el = if kids.len() == 1 {
            kids.into_iter().next().unwrap_or_else(Element::new)
        } else {
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            frag.children(kids)
        };
        if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
            el = with_class_list(el, &c);
        }
        el.template_id = Some(id.clone());
        el
    })
}

/// Fallback RSX entry (compound `ui!` uses [`message_scroller_scope`]).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MessageScrollerProvider(
    id: String,
    auto_scroll: Option<bool>,
    default_scroll_position: MessageScrollerDefaultPosition,
    scroll_edge_threshold: Option<f32>,
    scroll_previous_item_peek: Option<f32>,
    class: Option<String>,
    children: Option<Element>,
) -> Element {
    message_scroller_scope(
        id,
        auto_scroll.unwrap_or(true),
        default_scroll_position,
        scroll_edge_threshold.unwrap_or(DEFAULT_EDGE_THRESHOLD),
        scroll_previous_item_peek.unwrap_or(DEFAULT_PREVIOUS_PEEK),
        class,
        move || children.unwrap_or_else(Element::new),
    )
}

// ---------------------------------------------------------------------------
// RSX parts
// ---------------------------------------------------------------------------

/// Frame: relative column, `size-full`, clip overflow.
///
/// RSX `<MessageScroller>`.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MessageScroller(
    id: String,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = vstack()
        .w("100%")
        .h("100%")
        .min_w(0.0)
        .min_h(0.0)
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .relative(true)
        .overflow("hidden")
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_ROOT);
    el = apply_base(el, base_builder);
    if !id.trim().is_empty() {
        el.template_id = Some(id);
    }
    el
}

/// Scroll viewport (`overflow-y-auto` · `size-full` · `min-h-0`).
///
/// RSX `<MessageScrollerViewport>`.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MessageScrollerViewport(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let ctx = require_message_scroller("MessageScrollerViewport");
    let kids = children.map(take_packed).unwrap_or_default();
    let mut inner: Element = vstack()
        .w("100%")
        .h("100%")
        .min_w(0.0)
        .min_h(0.0)
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .children(kids)
        .into();
    inner = activate_scroll_viewport(
        inner,
        &ctx.viewport_id,
        "hidden",
        "auto",
        SCROLL_BEHAVIOR_JUMP,
        "scrollbar",
    );
    inner.set_attr(ATTR_SLOT, SLOT_VIEWPORT);
    inner.template_id = Some(ctx.viewport_id.clone());
    inner = apply_base(inner, base_builder);
    let sync_ctx = ctx.clone();
    inner = inner
        .reactive_sync(move |el, layout_tree| apply_viewport_scroll(el, layout_tree, &sync_ctx));

    let bar: Element = scrollbar(&format!("__rui_ms_sb_{}", ctx.viewport_id))
        .target(ctx.viewport_id.clone())
        .orientation(ScrollbarOrientation::Vertical)
        .visibility("auto")
        .overlay(true)
        .absolute()
        .top(2.0)
        .bottom(2.0)
        .right(2.0)
        .z_index(1000)
        .into();

    vstack()
        .relative(true)
        .w("100%")
        .h("100%")
        .min_w(0.0)
        .min_h(0.0)
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .overflow("hidden")
        .child(inner)
        .child(bar)
        .into()
}

/// Message column (`h-max` · flex-col).
///
/// RSX `<MessageScrollerContent>`.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MessageScrollerContent(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = vstack()
        .w("100%")
        .min_w(0.0)
        .h("max-content")
        .gap(tokens::SPACE_6)
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_CONTENT);
    apply_base(el, base_builder)
}

/// One turn (`min-w-0` · `shrink-0`). Optional `messageId` / `scrollAnchor`.
///
/// RSX `<MessageScrollerItem messageId="…" scrollAnchor>`.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MessageScrollerItem(
    message_id: String,
    scroll_anchor: bool,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = vstack()
        .w("100%")
        .min_w(0.0)
        .flex_shrink(0.0)
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_ITEM);
    if !message_id.trim().is_empty() {
        el.set_attr(ATTR_MESSAGE_ID, message_id.trim());
        el.template_id = Some(format!("ms-item:{message_id}"));
    }
    if scroll_anchor {
        el.set_attr(ATTR_SCROLL_ANCHOR, "true");
    }
    apply_base(el, base_builder)
}

/// Absolute jump control. Default icon is Lucide `arrow-down` (rotated for `start`).
///
/// RSX `<MessageScrollerButton direction="end">`.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MessageScrollerButton(
    direction: MessageScrollerDirection,
    variant: String,
    size: String,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let ctx = require_message_scroller("MessageScrollerButton");
    let kids = children.map(take_packed).unwrap_or_default();
    let variant = if variant.trim().is_empty() {
        "secondary"
    } else {
        variant.trim()
    };
    let size = if size.trim().is_empty() {
        "icon-sm"
    } else {
        size.trim()
    };
    let btn_id = format!("{}:btn-{}", ctx.id, direction.as_str());
    let jump_ctx = ctx.clone();
    let jump_dir = direction;
    let mut el: Element = button(btn_id)
        .variant(variant)
        .size(size)
        .on_click(move || {
            if jump_dir.is_end() {
                jump_ctx.scroll_to_end(true);
            } else {
                jump_ctx.scroll_to_start(true);
            }
        })
        .children(if kids.is_empty() {
            vec![default_arrow_icon(direction)]
        } else {
            kids
        })
        .into();
    el = el
        .absolute()
        .left("50%")
        .translate_pct(-0.5, 0.0)
        .z_index(10);
    if direction.is_end() {
        el = el.bottom(BUTTON_INSET);
    } else {
        el = el.top(BUTTON_INSET);
    }
    el.set_attr(ATTR_SLOT, SLOT_BUTTON);
    el.set_attr(ATTR_DIRECTION, direction.as_str());
    el.set_attr(ATTR_ACTIVE, "false");
    el = el.opacity(0.0).pointer_events_none();
    el = apply_base(el, base_builder);
    let sync_ctx = ctx.clone();
    el = el.reactive_sync(move |node, _layout_tree| apply_button_active(node, &sync_ctx, jump_dir));
    el
}

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

pub struct MessageScrollerProviderBuilder {
    id: String,
    auto_scroll: bool,
    default_scroll_position: MessageScrollerDefaultPosition,
    scroll_edge_threshold: f32,
    scroll_previous_item_peek: f32,
    class: String,
    children: Vec<DelayedEl>,
}

/// Builder entry for [`MessageScrollerProvider`].
pub fn message_scroller_provider() -> MessageScrollerProviderBuilder {
    MessageScrollerProviderBuilder {
        id: String::new(),
        auto_scroll: true,
        default_scroll_position: MessageScrollerDefaultPosition::End,
        scroll_edge_threshold: DEFAULT_EDGE_THRESHOLD,
        scroll_previous_item_peek: DEFAULT_PREVIOUS_PEEK,
        class: String::new(),
        children: Vec::new(),
    }
}

impl MessageScrollerProviderBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn auto_scroll(mut self, on: bool) -> Self {
        self.auto_scroll = on;
        self
    }

    pub fn default_scroll_position(
        mut self,
        pos: impl Into<MessageScrollerDefaultPosition>,
    ) -> Self {
        self.default_scroll_position = pos.into();
        self
    }

    pub fn scroll_edge_threshold(mut self, px: f32) -> Self {
        self.scroll_edge_threshold = px;
        self
    }

    pub fn scroll_previous_item_peek(mut self, px: f32) -> Self {
        self.scroll_previous_item_peek = px;
        self
    }

    pub fn class(mut self, classes: impl Into<String>) -> Self {
        self.class = classes.into();
        self
    }

    pub fn child(mut self, child: impl Into<Element> + 'static) -> Self {
        self.children.push(delay_el(child));
        self
    }
}

impl crate::core::tree::IntoElementList for MessageScrollerProviderBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<MessageScrollerProviderBuilder> for Element {
    fn from(builder: MessageScrollerProviderBuilder) -> Self {
        let class = if builder.class.trim().is_empty() {
            None
        } else {
            Some(builder.class)
        };
        message_scroller_scope(
            builder.id,
            builder.auto_scroll,
            builder.default_scroll_position,
            builder.scroll_edge_threshold,
            builder.scroll_previous_item_peek,
            class,
            move || packed_children(collect_delayed(builder.children)).unwrap_or_else(Element::new),
        )
    }
}

pub struct MessageScrollerBuilder {
    id: String,
    class: String,
    w: Option<String>,
    h: Option<String>,
    children: Vec<DelayedEl>,
}

/// Builder entry for [`MessageScroller`] (the frame, not the Provider).
pub fn message_scroller() -> MessageScrollerBuilder {
    MessageScrollerBuilder {
        id: String::new(),
        class: String::new(),
        w: None,
        h: None,
        children: Vec::new(),
    }
}

impl MessageScrollerBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn class(mut self, classes: impl Into<String>) -> Self {
        self.class = classes.into();
        self
    }

    pub fn w(mut self, w: impl Into<String>) -> Self {
        self.w = Some(w.into());
        self
    }

    pub fn h(mut self, h: impl Into<String>) -> Self {
        self.h = Some(h.into());
        self
    }

    pub fn child(mut self, child: impl Into<Element> + 'static) -> Self {
        self.children.push(delay_el(child));
        self
    }
}

impl crate::core::tree::IntoElementList for MessageScrollerBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<MessageScrollerBuilder> for Element {
    fn from(builder: MessageScrollerBuilder) -> Self {
        let mut el = MessageScroller(MessageScrollerProps {
            id: builder.id,
            children: packed_children(collect_delayed(builder.children)),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        if let Some(w) = builder.w {
            el = el.w(w);
        }
        if let Some(h) = builder.h {
            el = el.h(h);
        }
        apply_optional_class(el, &builder.class)
    }
}

pub struct MessageScrollerViewportBuilder {
    class: String,
    children: Vec<Element>,
}

pub fn message_scroller_viewport() -> MessageScrollerViewportBuilder {
    MessageScrollerViewportBuilder {
        class: String::new(),
        children: Vec::new(),
    }
}

impl MessageScrollerViewportBuilder {
    pub fn class(mut self, classes: impl Into<String>) -> Self {
        self.class = classes.into();
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
}

impl crate::core::tree::IntoElementList for MessageScrollerViewportBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<MessageScrollerViewportBuilder> for Element {
    fn from(builder: MessageScrollerViewportBuilder) -> Self {
        let el = MessageScrollerViewport(MessageScrollerViewportProps {
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        apply_optional_class(el, &builder.class)
    }
}

pub struct MessageScrollerContentBuilder {
    class: String,
    children: Vec<Element>,
}

pub fn message_scroller_content() -> MessageScrollerContentBuilder {
    MessageScrollerContentBuilder {
        class: String::new(),
        children: Vec::new(),
    }
}

impl MessageScrollerContentBuilder {
    pub fn class(mut self, classes: impl Into<String>) -> Self {
        self.class = classes.into();
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
}

impl crate::core::tree::IntoElementList for MessageScrollerContentBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<MessageScrollerContentBuilder> for Element {
    fn from(builder: MessageScrollerContentBuilder) -> Self {
        let el = MessageScrollerContent(MessageScrollerContentProps {
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        apply_optional_class(el, &builder.class)
    }
}

pub struct MessageScrollerItemBuilder {
    message_id: String,
    scroll_anchor: bool,
    class: String,
    children: Vec<Element>,
}

pub fn message_scroller_item() -> MessageScrollerItemBuilder {
    MessageScrollerItemBuilder {
        message_id: String::new(),
        scroll_anchor: false,
        class: String::new(),
        children: Vec::new(),
    }
}

impl MessageScrollerItemBuilder {
    pub fn message_id(mut self, id: impl Into<String>) -> Self {
        self.message_id = id.into();
        self
    }

    pub fn scroll_anchor(mut self, on: bool) -> Self {
        self.scroll_anchor = on;
        self
    }

    pub fn class(mut self, classes: impl Into<String>) -> Self {
        self.class = classes.into();
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
}

impl crate::core::tree::IntoElementList for MessageScrollerItemBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<MessageScrollerItemBuilder> for Element {
    fn from(builder: MessageScrollerItemBuilder) -> Self {
        let el = MessageScrollerItem(MessageScrollerItemProps {
            message_id: Some(builder.message_id),
            scroll_anchor: Some(builder.scroll_anchor),
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        apply_optional_class(el, &builder.class)
    }
}

pub struct MessageScrollerButtonBuilder {
    direction: MessageScrollerDirection,
    variant: String,
    size: String,
    class: String,
    children: Vec<Element>,
}

pub fn message_scroller_button() -> MessageScrollerButtonBuilder {
    MessageScrollerButtonBuilder {
        direction: MessageScrollerDirection::End,
        variant: String::new(),
        size: String::new(),
        class: String::new(),
        children: Vec::new(),
    }
}

impl MessageScrollerButtonBuilder {
    pub fn direction(mut self, direction: impl Into<MessageScrollerDirection>) -> Self {
        self.direction = direction.into();
        self
    }

    pub fn variant(mut self, variant: impl Into<String>) -> Self {
        self.variant = variant.into();
        self
    }

    pub fn size(mut self, size: impl Into<String>) -> Self {
        self.size = size.into();
        self
    }

    pub fn class(mut self, classes: impl Into<String>) -> Self {
        self.class = classes.into();
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
}

impl crate::core::tree::IntoElementList for MessageScrollerButtonBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<MessageScrollerButtonBuilder> for Element {
    fn from(builder: MessageScrollerButtonBuilder) -> Self {
        let el = MessageScrollerButton(MessageScrollerButtonProps {
            direction: Some(builder.direction),
            variant: Some(builder.variant),
            size: Some(builder.size),
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        apply_optional_class(el, &builder.class)
    }
}
