//! Pointer **actions** that are drag, but not list/kanban **drop**.
//!
//! interact.js ships drag / resize / gesture / inertia / multi-touch as one
//! pointer stack. Jadix's [`crate::utils::drag`] kit stays the solid-dnd
//! Sortable/Overlay session. This module is the sibling seam: edge resize,
//! two-finger gesture, fling inertia, concurrent pointers.
//!
//! Callers (Gallery, tests, `window_events`) use the small types below;
//! layout widgets like [`crate::components::resizable`] are unchanged.

mod gesture;
mod inertia;
mod pointers;
mod resize;

pub use gesture::{touch_angle, touch_bbox, touch_distance, GestureSession, GestureState};
pub use inertia::{Inertia, InertiaOpts, VelocityTracker};
pub use pointers::{
    gesture_is_active, last_gesture, reset_global, sync_touches, with_pointers, with_pointers_mut,
    ActionKind, BeginError, PointerSet, Quota,
};
pub use resize::{hit_edges, resize_box, Edges, Invert, ResizeAxis, ResizeOpts, ResizeSession};

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use crate::core::reactivity::Signal;
use crate::core::scroll_viewport::{scroll_offset_x_signal_key, scroll_offset_y_signal_key};
use crate::core::tree::{Element, IntoCssDim, LayoutStatus};
use crate::style::{IntoCursor, UnitSlot};
use crate::utils::drag::geometry::Axis;
pub use crate::utils::edge_scroll::{AutoScroll, ScrollLock};

use crate::utils::edge_scroll::{apply, collect_scroll_hits, frame_dt, pump_at, ScrollHit};

/// Pointer-space rectangle `[x, y, width, height]` (may be negative under [`Invert::Negate`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoxRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl BoxRect {
    pub fn from_xywh(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn from_ltrb(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        }
    }

    pub fn to_xywh(self) -> [f32; 4] {
        [self.x, self.y, self.width, self.height]
    }

    pub fn right(self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(self) -> f32 {
        self.y + self.height
    }

    pub fn center(self) -> [f32; 2] {
        [self.x + self.width * 0.5, self.y + self.height * 0.5]
    }

    pub fn normalized(self) -> Self {
        let mut x = self.x;
        let mut y = self.y;
        let mut w = self.width;
        let mut h = self.height;
        if w < 0.0 {
            x += w;
            w = -w;
        }
        if h < 0.0 {
            y += h;
            h = -h;
        }
        Self::from_xywh(x, y, w, h)
    }
}

/// Optional edge-scroll while two-finger gesture is active (touch or desktop dots).
#[derive(Clone, Copy, Debug, Default)]
pub struct GestureOpts {
    pub autoscroll: Option<AutoScroll>,
}

impl GestureOpts {
    pub fn autoscroll(mut self, yes: bool) -> Self {
        self.autoscroll = if yes {
            Some(AutoScroll::default().lock(ScrollLock::None))
        } else {
            None
        };
        self
    }

    pub fn autoscroll_opts(mut self, opts: AutoScroll) -> Self {
        self.autoscroll = Some(opts);
        self
    }
}

/// Live resize driven from `window_events` (one primary pointer).
struct LiveResize {
    session: ResizeSession,
    signal: Option<Signal<[f32; 4]>>,
    last_pointer: [f32; 2],
    start_pointer: [f32; 2],
    // Outer None means unresolved; inner None means the press had no scroll owner.
    scroll_owner: Option<Option<(String, [f32; 2])>>,
}

thread_local! {
    static LIVE_RESIZE: RefCell<Option<LiveResize>> = const { RefCell::new(None) };
    static GESTURE_OPTS: RefCell<GestureOpts> = const { RefCell::new(GestureOpts { autoscroll: None }) };
    static LAST_EDGE_SCROLL_AT: RefCell<Option<Instant>> = const { RefCell::new(None) };
}

/// Configure edge auto-scroll for the next / current gesture session.
pub fn set_gesture_opts(opts: GestureOpts) {
    GESTURE_OPTS.with(|g| *g.borrow_mut() = opts);
}

pub fn gesture_opts() -> GestureOpts {
    GESTURE_OPTS.with(|g| *g.borrow())
}

/// Whether resize or gesture has edge-scroll enabled.
pub fn edge_scroll_active() -> bool {
    resize_edge_scroll().is_some() || gesture_edge_scroll().is_some()
}

fn resize_edge_scroll() -> Option<([f32; 2], AutoScroll)> {
    LIVE_RESIZE.with(|s| {
        let live = s.borrow();
        let live = live.as_ref()?;
        let cfg = live.session.opts().autoscroll?;
        if !cfg.enabled {
            return None;
        }
        Some((live.last_pointer, cfg))
    })
}

fn gesture_edge_scroll() -> Option<([f32; 2], AutoScroll)> {
    if !gesture_is_active() {
        return None;
    }
    let cfg = GESTURE_OPTS.with(|g| g.borrow().autoscroll)?;
    if !cfg.enabled {
        return None;
    }
    let centroid = last_gesture().map(|g| g.centroid)?;
    Some((centroid, cfg))
}

fn resize_motion_axis() -> Axis {
    LIVE_RESIZE
        .with(|s| {
            s.borrow()
                .as_ref()
                .map(|live| live.session.opts().edges.axis())
        })
        .map(|a| match a {
            ResizeAxis::X => Axis::X,
            ResizeAxis::Y => Axis::Y,
            ResizeAxis::Xy => Axis::Xy,
        })
        .unwrap_or(Axis::Xy)
}

/// Pump edge auto-scroll for live resize / gesture (also while pointer is held still).
pub fn pump_edge_scroll(root: &Element) -> bool {
    if let Some((pointer, cfg)) = resize_edge_scroll() {
        let mut views = Vec::new();
        collect_scroll_hits(root, &mut views);
        let owner = resolve_resize_scroll_owner(&views);
        views.retain(|view| owner.as_deref() == Some(view.rest.as_str()));
        let motion = resize_motion_axis();
        let moved =
            LAST_EDGE_SCROLL_AT.with(|slot| apply(pointer, &views, cfg, frame_dt(slot), motion));
        if moved {
            tick_resize(pointer[0], pointer[1]);
        }
        return moved;
    }
    if let Some((pointer, cfg)) = gesture_edge_scroll() {
        return LAST_EDGE_SCROLL_AT.with(|slot| pump_at(root, pointer, cfg, Axis::Xy, slot));
    }
    LAST_EDGE_SCROLL_AT.with(|t| t.borrow_mut().take());
    false
}

fn resolve_resize_scroll_owner(views: &[ScrollHit]) -> Option<String> {
    LIVE_RESIZE.with(|slot| {
        let mut live = slot.borrow_mut();
        let live = live.as_mut()?;
        live.scroll_owner
            .get_or_insert_with(|| {
                views
                    .iter()
                    .rev()
                    .find(|view| view.rect.contains_point(live.start_pointer))
                    .map(|view| {
                        let x = crate::core::reactivity::use_signal(
                            &scroll_offset_x_signal_key(&view.rest),
                            || 0.0_f32,
                        )
                        .peek();
                        let y = crate::core::reactivity::use_signal(
                            &scroll_offset_y_signal_key(&view.rest),
                            || 0.0_f32,
                        )
                        .peek();
                        (view.rest.clone(), [x, y])
                    })
            })
            .clone()
            .map(|(id, _)| id)
    })
}

fn resize_scroll_delta(live: &LiveResize) -> [f32; 2] {
    let Some(Some((owner, start_offset))) = &live.scroll_owner else {
        return [0.0, 0.0];
    };
    let x =
        crate::core::reactivity::use_signal(&scroll_offset_x_signal_key(owner), || 0.0_f32).peek();
    let y =
        crate::core::reactivity::use_signal(&scroll_offset_y_signal_key(owner), || 0.0_f32).peek();
    [x - start_offset[0], y - start_offset[1]]
}

/// Resolve at press time, before scrolling or relayout can move the source viewport.
pub(crate) fn capture_resize_scroll_owner(root: &Element) {
    if resize_is_active() {
        let mut views = Vec::new();
        collect_scroll_hits(root, &mut views);
        resolve_resize_scroll_owner(&views);
    }
}

/// Start an edge/corner resize. `signal_key` if set is written `[x,y,w,h]` each tick.
pub fn begin_resize(
    start: BoxRect,
    pointer: [f32; 2],
    opts: ResizeOpts,
    signal_key: Option<String>,
) {
    let signal =
        signal_key.map(|key| crate::core::reactivity::use_signal(&key, || start.to_xywh()));
    start_resize(start, pointer, opts, signal);
}

fn start_resize(
    start: BoxRect,
    pointer: [f32; 2],
    opts: ResizeOpts,
    signal: Option<Signal<[f32; 4]>>,
) {
    LIVE_RESIZE.with(|s| {
        *s.borrow_mut() = Some(LiveResize {
            session: ResizeSession::begin(start, pointer, opts),
            signal,
            last_pointer: pointer,
            start_pointer: pointer,
            scroll_owner: None,
        });
    });
}

/// Returns the latest box when a resize is live.
pub fn tick_resize(x: f32, y: f32) -> Option<BoxRect> {
    let (rect, signal) = LIVE_RESIZE.with(|s| {
        let mut slot = s.borrow_mut();
        let live = slot.as_mut()?;
        live.last_pointer = [x, y];
        let scroll = resize_scroll_delta(live);
        let rect = live.session.tick([x + scroll[0], y + scroll[1]]);
        Some((rect, live.signal))
    })?;
    if let Some(signal) = signal {
        signal.try_set(rect.to_xywh());
    }
    Some(rect)
}

pub fn end_resize() -> bool {
    on_up()
}

pub fn resize_is_active() -> bool {
    LIVE_RESIZE.with(|s| s.borrow().is_some())
}

/// The edge/corner cursor stays fixed until the current resize ends.
pub fn active_resize_cursor() -> Option<crate::style::CursorValue> {
    LIVE_RESIZE.with(|s| {
        s.borrow()
            .as_ref()?
            .session
            .opts()
            .edges
            .cursor()
            .into_cursor()
    })
}

/// Bind an absolute element's `[left, top, width, height]` to a rectangle signal.
/// Updates the existing node in the frame prepass, without rebuilding its children.
pub fn bind_resize_rect(mut el: Element, rect: Signal<[f32; 4]>) -> Element {
    let previous = el.reactive_sync.take();
    let last = rect.peek();
    apply_resize_rect(&mut el, last);
    let last = Cell::new(last);
    el.reactive_sync(move |el, layout| {
        let status = previous
            .as_ref()
            .map_or(LayoutStatus::Clean, |sync| sync.run(el, layout));
        let next = rect.peek();
        if last.replace(next) == next {
            return status;
        }
        apply_resize_rect(el, next);
        status.merge(LayoutStatus::LayoutChanged)
    })
}

fn apply_resize_rect(el: &mut Element, rect: [f32; 4]) {
    for (slot, value) in [
        UnitSlot::InsetL,
        UnitSlot::InsetT,
        UnitSlot::Width,
        UnitSlot::Height,
    ]
    .into_iter()
    .zip(rect)
    {
        el.style.set_css_dim(slot, value.into_css_dim());
    }
}

/// Make an absolute box resize like a window, without visible handles.
///
/// `margin` is the edge band in logical pixels inside the element's hit box.
/// Corners select two edges; the interior retains its original pointer behavior.
/// This binds `rect` to the element and replaces `opts.edges` with the hit edges.
/// Existing pointer handlers and frame synchronization are preserved.
///
/// ```
/// use jadix::core::{reactivity::Signal, tree::Element};
/// use jadix::utils::pointer_action::{resize_on_edges, ResizeOpts};
/// let rect = Signal::new([40.0, 28.0, 220.0, 120.0]);
/// let panel = resize_on_edges(Element::new().absolute(), rect,
///     ResizeOpts::default().min_size(120.0, 80.0), 8.0);
/// ```
pub fn resize_on_edges(
    el: Element,
    rect: Signal<[f32; 4]>,
    opts: ResizeOpts,
    margin: f32,
) -> Element {
    use winit::event::MouseButton;

    let mut el = bind_resize_rect(el, rect);
    let hover = Rc::new(Cell::new(Edges::NONE));
    let on_move = el.on_mousemove.take();
    let move_hover = hover.clone();
    el = el.on_mousemove(move |event| {
        if !resize_is_active() {
            let [x, y, w, h] = event.current_bounds;
            let edges = hit_edges(BoxRect::from_xywh(x, y, w, h), [event.x, event.y], margin);
            if move_hover.replace(edges) != edges {
                crate::app::runtime::request_redraw();
            }
        }
        if let Some(handler) = &on_move {
            (handler.borrow_mut())(event);
        }
    });
    let on_out = el.on_mouseout.take();
    let out_hover = hover.clone();
    el = el.on_mouseout(move |event| {
        if out_hover.replace(Edges::NONE).any() {
            crate::app::runtime::request_redraw();
        }
        if let Some(handler) = &on_out {
            (handler.borrow_mut())(event);
        }
    });
    let on_down = el.on_mousedown.take();
    el = el.on_mousedown(move |event| {
        if event.button == MouseButton::Left {
            let [x, y, w, h] = event.current_bounds;
            let edges = hit_edges(BoxRect::from_xywh(x, y, w, h), [event.x, event.y], margin);
            if edges.any() {
                let [x, y, w, h] = rect.peek();
                start_resize(
                    BoxRect::from_xywh(x, y, w, h),
                    [event.x, event.y],
                    opts.edges(edges),
                    Some(rect),
                );
                crate::app::runtime::request_redraw();
                event.prevent_default();
                event.stop_propagation();
                return;
            }
        }
        if let Some(handler) = &on_down {
            (handler.borrow_mut())(event);
        }
    });
    let previous = el.reactive_sync.take();
    let original_cursor = el.cursor.clone();
    el.reactive_sync(move |el, layout| {
        let status = previous
            .as_ref()
            .map_or(LayoutStatus::Clean, |sync| sync.run(el, layout));
        let edges = hover.get();
        let cursor = if edges.any() {
            edges.cursor().into_cursor()
        } else {
            original_cursor.clone()
        };
        if el.cursor == cursor {
            return status;
        }
        el.cursor = cursor;
        status.merge(LayoutStatus::StyleChanged)
    })
}

/// `mousemove` / `mouseup` hooks for the app loop.
pub fn on_move(x: f32, y: f32) -> bool {
    tick_resize(x, y).is_some()
}

pub fn on_up() -> bool {
    let had = resize_is_active();
    LIVE_RESIZE.with(|s| s.borrow_mut().take());
    if !had {
        return false;
    }
    LAST_EDGE_SCROLL_AT.with(|t| t.borrow_mut().take());
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_rect_normalize() {
        let n = BoxRect::from_xywh(10.0, 10.0, -4.0, -6.0).normalized();
        assert!((n.x - 6.0).abs() < 1e-4);
        assert!((n.y - 4.0).abs() < 1e-4);
        assert!(n.width > 0.0 && n.height > 0.0);
    }

    #[test]
    fn resize_autoscroll_pumps_scroll_viewport() {
        use crate::core::reactivity::create_root;
        use crate::core::scroll_viewport::scroll_offset_y_signal_key;
        use crate::core::tree::Element;
        use crate::utils::edge_scroll::ScrollLock;

        on_up();
        begin_resize(
            BoxRect::from_xywh(0.0, 0.0, 40.0, 20.0),
            [40.0, 10.0],
            ResizeOpts::default()
                .edges(Edges::right())
                .autoscroll_opts(AutoScroll::default().lock(ScrollLock::None)),
            None,
        );
        let (_owner, y) = create_root(|| {
            use crate::core::reactivity::use_signal;
            use crate::core::scroll_viewport::{
                scroll_content_h_signal_key, scroll_viewport_h_signal_key,
                SCROLL_VIEWPORT_FOCUS_PREFIX,
            };
            let mut vp = Element::new();
            vp.focus_id = Some(format!("{}{}", SCROLL_VIEWPORT_FOCUS_PREFIX, "resize"));
            vp.layout_bounds = [0.0, 0.0, 200.0, 120.0];
            use_signal(&scroll_offset_y_signal_key("resize"), || 0.0_f32).set_silent(0.0);
            use_signal(&scroll_viewport_h_signal_key("resize"), || 0.0_f32).set_silent(120.0);
            use_signal(&scroll_content_h_signal_key("resize"), || 0.0_f32).set_silent(800.0);
            tick_resize(40.0, 105.0);
            assert!(pump_edge_scroll(&vp), "resize near bottom edge must scroll");
            use_signal(&scroll_offset_y_signal_key("resize"), || 0.0_f32).peek()
        });
        assert!(y > 0.5, "expected Y scroll, got {y}");
        on_up();
    }

    #[test]
    fn live_resize_writes_signal() {
        on_up();
        begin_resize(
            BoxRect::from_xywh(0.0, 0.0, 40.0, 20.0),
            [40.0, 10.0],
            ResizeOpts::default().edges(Edges::right()),
            Some("jadix_test_resize".into()),
        );
        let r = tick_resize(60.0, 10.0).unwrap();
        assert!((r.width - 60.0).abs() < 1e-3);
        let stored =
            crate::core::reactivity::use_signal("jadix_test_resize", || [0.0_f32; 4]).peek();
        assert!((stored[2] - 60.0).abs() < 1e-3);
        assert!(on_up());
        assert!(!resize_is_active());
    }
}
