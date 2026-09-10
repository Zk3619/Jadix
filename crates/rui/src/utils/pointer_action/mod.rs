//! Pointer **actions** that are drag, but not list/kanban **drop**.
//!
//! interact.js ships drag / resize / gesture / inertia / multi-touch as one
//! pointer stack. RUI's [`crate::utils::drag`] kit stays the solid-dnd
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

use std::cell::RefCell;
use std::time::Instant;

use crate::core::tree::Element;
use crate::utils::drag::geometry::Axis;
pub use crate::utils::edge_scroll::{AutoScroll, ScrollLock};

use crate::utils::edge_scroll::pump_at;

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
    signal_key: Option<String>,
    last_pointer: [f32; 2],
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
    let (pointer, cfg, motion) = if let Some((p, c)) = resize_edge_scroll() {
        (p, c, resize_motion_axis())
    } else if let Some((p, c)) = gesture_edge_scroll() {
        (p, c, Axis::Xy)
    } else {
        LAST_EDGE_SCROLL_AT.with(|t| t.borrow_mut().take());
        return false;
    };
    LAST_EDGE_SCROLL_AT.with(|slot| pump_at(root, pointer, cfg, motion, slot))
}

/// Start an edge/corner resize. `signal_key` if set is written `[x,y,w,h]` each tick.
pub fn begin_resize(
    start: BoxRect,
    pointer: [f32; 2],
    opts: ResizeOpts,
    signal_key: Option<String>,
) {
    LIVE_RESIZE.with(|s| {
        *s.borrow_mut() = Some(LiveResize {
            session: ResizeSession::begin(start, pointer, opts),
            signal_key,
            last_pointer: pointer,
        });
    });
}

/// Returns the latest box when a resize is live.
pub fn tick_resize(x: f32, y: f32) -> Option<BoxRect> {
    LIVE_RESIZE.with(|s| {
        let mut slot = s.borrow_mut();
        let live = slot.as_mut()?;
        live.last_pointer = [x, y];
        let rect = live.session.tick([x, y]);
        if let Some(key) = live.signal_key.as_deref() {
            crate::core::reactivity::use_signal(key, || rect.to_xywh()).set(rect.to_xywh());
        }
        Some(rect)
    })
}

pub fn end_resize() -> bool {
    on_up()
}

pub fn resize_is_active() -> bool {
    LIVE_RESIZE.with(|s| s.borrow().is_some())
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
            Some("rui_test_resize".into()),
        );
        let r = tick_resize(60.0, 10.0).unwrap();
        assert!((r.width - 60.0).abs() < 1e-3);
        let stored = crate::core::reactivity::use_signal("rui_test_resize", || [0.0_f32; 4]).peek();
        assert!((stored[2] - 60.0).abs() < 1e-3);
        assert!(on_up());
        assert!(!resize_is_active());
    }
}
