//! Edge auto-scroll (SortableJS `scroll` / `bubbleScroll`).
//!
//! Shared by [`crate::utils::drag`] (list/kanban drag) and
//! [`crate::utils::pointer_action`] (resize / gesture). Held-still pumps run
//! from [`crate::app::lifecycle`] and [`crate::app::window_events`].

use std::cell::RefCell;
use std::time::Instant;

use crate::core::reactivity::use_signal;
use crate::core::scroll_viewport::{
    scroll_content_h_signal_key, scroll_content_w_signal_key, scroll_offset_x_signal_key,
    scroll_offset_y_signal_key, scroll_target_x_signal_key, scroll_target_y_signal_key,
    scroll_viewport_h_signal_key, scroll_viewport_w_signal_key, SCROLL_VIEWPORT_FOCUS_PREFIX,
};
use crate::core::tree::Element;
use crate::utils::drag::geometry::{Axis, Rect};

/// Which axes of **edge auto-scroll** are frozen while dragging.
/// Independent of motion [`Axis`]: `scroll_lock="all"` works with `axis="xy"`.
///
/// RSX: `scroll_lock="auto"|"all"|"x"|"y"|"none"` (aliases `scroll-lock`,
/// `scrollLock`) or flags `scroll-lock-all` / `scroll-lock-x` / `scroll-lock-y`
/// / `scroll-lock-none` / `scroll-lock-auto` (valueless or `={true}`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollLock {
    /// Follow motion axis: `axis=y` skips X scroll, `axis=x` skips Y, `xy` both.
    #[default]
    Auto,
    /// No autoscroll on either axis.
    All,
    /// No horizontal autoscroll.
    X,
    /// No vertical autoscroll.
    Y,
    /// Autoscroll both axes regardless of motion axis.
    None,
}

impl ScrollLock {
    pub fn parse(s: &str) -> Self {
        let raw = s
            .trim()
            .trim_matches(|c| c == '"' || c == '\'')
            .to_ascii_lowercase();
        let key = raw
            .strip_prefix("scroll-lock-")
            .or_else(|| raw.strip_prefix("scroll_lock_"))
            .unwrap_or(raw.as_str());
        match key {
            "all" | "both" => Self::All,
            "x" | "horizontal" | "h" => Self::X,
            "y" | "vertical" | "v" => Self::Y,
            "none" | "off" => Self::None,
            _ => Self::Auto,
        }
    }

    /// `(skip_x, skip_y)` for edge autoscroll.
    pub fn skip_axes(self, motion: Axis) -> (bool, bool) {
        match self {
            Self::All => (true, true),
            Self::X => (true, false),
            Self::Y => (false, true),
            Self::None => (false, false),
            Self::Auto => match motion {
                Axis::Y => (true, false),
                Axis::X => (false, true),
                Axis::Xy | Axis::Start => (false, false),
            },
        }
    }
}

impl From<&str> for ScrollLock {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for ScrollLock {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

/// SortableJS defaults: 30px band, ~10px/frame. We use a slightly wider band.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AutoScroll {
    pub enabled: bool,
    pub sensitivity: f32,
    /// Pixels per 16.67ms (one display frame).
    pub speed: f32,
    /// Which scroll axes are frozen. Default [`ScrollLock::Auto`].
    pub lock: ScrollLock,
}

impl Default for AutoScroll {
    fn default() -> Self {
        Self {
            enabled: true,
            sensitivity: 40.0,
            speed: 14.0,
            lock: ScrollLock::Auto,
        }
    }
}

impl AutoScroll {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Self::default()
        }
    }

    pub fn enabled(mut self, yes: bool) -> Self {
        self.enabled = yes;
        self
    }

    pub fn sensitivity(mut self, px: f32) -> Self {
        self.sensitivity = px.max(0.0);
        self
    }

    pub fn speed(mut self, px: f32) -> Self {
        self.speed = px.max(0.0);
        self
    }

    pub fn lock(mut self, lock: ScrollLock) -> Self {
        self.lock = lock;
        self
    }
}

/// A laid-out `scroll:` viewport (rest id + box).
#[derive(Clone, Debug)]
pub struct ScrollHit {
    pub rest: String,
    pub rect: Rect,
}

/// Signed scroll delta when `pointer` sits in the edge band of `[start, start+size]`.
pub fn edge_delta(pointer: f32, start: f32, size: f32, sensitivity: f32, step: f32) -> f32 {
    if size <= sensitivity * 2.0 {
        return 0.0;
    }
    if pointer < start + sensitivity {
        -step
    } else if pointer > start + size - sensitivity {
        step
    } else {
        0.0
    }
}

pub fn collect_scroll_hits(root: &Element, out: &mut Vec<ScrollHit>) {
    if let Some(fid) = &root.focus_id {
        if let Some(rest) = fid.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX) {
            let rect = Rect::from_bounds(root.layout_bounds);
            if rect.is_usable() {
                out.push(ScrollHit {
                    rest: rest.to_string(),
                    rect,
                });
            }
        }
    }
    for c in &root.children {
        collect_scroll_hits(c, out);
    }
}

fn nudge_axis(
    offset_key: &str,
    target_key: &str,
    max_key_vp: &str,
    max_key_content: &str,
    delta: f32,
) -> bool {
    if delta.abs() < 0.05 {
        return false;
    }
    let cur = use_signal(offset_key, || 0.0_f32).peek();
    let vp = use_signal(max_key_vp, || 0.0_f32).peek();
    let content = use_signal(max_key_content, || 0.0_f32).peek();
    let max = (content - vp).max(0.0);
    let next = (cur + delta).clamp(0.0, max);
    if (next - cur).abs() < 0.5 {
        return false;
    }
    use_signal(offset_key, || 0.0_f32).set_silent(next);
    use_signal(target_key, || 0.0_f32).set_silent(next);
    true
}

fn frame_dt(last_at: &RefCell<Option<Instant>>) -> f32 {
    let now = Instant::now();
    let dt = last_at
        .borrow()
        .map(|prev| now.saturating_duration_since(prev).as_secs_f32())
        .unwrap_or(1.0 / 60.0);
    *last_at.borrow_mut() = Some(now);
    dt
}

/// Scroll every overflowing viewport whose edge band contains `pointer`.
///
/// [`AutoScroll::lock`] chooses frozen axes; [`ScrollLock::Auto`] follows `motion`.
pub fn apply(
    pointer: [f32; 2],
    views: &[ScrollHit],
    cfg: AutoScroll,
    dt: f32,
    motion: Axis,
) -> bool {
    if !cfg.enabled {
        return false;
    }
    let (skip_x, skip_y) = cfg.lock.skip_axes(motion);
    let step = (cfg.speed * (dt * 60.0).clamp(0.25, 2.5)).max(1.0);
    let mut moved = false;
    for v in views {
        let dx = if skip_x {
            0.0
        } else {
            edge_delta(pointer[0], v.rect.x, v.rect.width, cfg.sensitivity, step)
        };
        let dy = if skip_y {
            0.0
        } else {
            edge_delta(pointer[1], v.rect.y, v.rect.height, cfg.sensitivity, step)
        };
        if dx.abs() > 0.05 {
            moved |= nudge_axis(
                &scroll_offset_x_signal_key(&v.rest),
                &scroll_target_x_signal_key(&v.rest),
                &scroll_viewport_w_signal_key(&v.rest),
                &scroll_content_w_signal_key(&v.rest),
                dx,
            );
        }
        if dy.abs() > 0.05 {
            moved |= nudge_axis(
                &scroll_offset_y_signal_key(&v.rest),
                &scroll_target_y_signal_key(&v.rest),
                &scroll_viewport_h_signal_key(&v.rest),
                &scroll_content_h_signal_key(&v.rest),
                dy,
            );
        }
    }
    moved
}

/// Pump edge scroll for an arbitrary pointer + config (resize / gesture).
pub fn pump_at(
    root: &Element,
    pointer: [f32; 2],
    cfg: AutoScroll,
    motion: Axis,
    last_at: &RefCell<Option<Instant>>,
) -> bool {
    if !cfg.enabled {
        last_at.borrow_mut().take();
        return false;
    }
    let dt = frame_dt(last_at);
    let mut views = Vec::new();
    collect_scroll_hits(root, &mut views);
    apply(pointer, &views, cfg, dt, motion)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_delta_top_and_bottom() {
        assert!(edge_delta(5.0, 0.0, 200.0, 40.0, 10.0) < 0.0);
        assert!(edge_delta(190.0, 0.0, 200.0, 40.0, 10.0) > 0.0);
        assert_eq!(edge_delta(100.0, 0.0, 200.0, 40.0, 10.0), 0.0);
    }

    #[test]
    fn autoscroll_builder() {
        let cfg = AutoScroll::default()
            .enabled(false)
            .sensitivity(30.0)
            .speed(10.0)
            .lock(ScrollLock::None);
        assert!(!cfg.enabled);
        assert!((cfg.sensitivity - 30.0).abs() < 1e-4);
        assert!((cfg.speed - 10.0).abs() < 1e-4);
        assert_eq!(cfg.lock, ScrollLock::None);
    }

    #[test]
    fn apply_scroll_lock_and_auto_axis() {
        use crate::core::reactivity::create_root;
        use crate::core::scroll_viewport::{
            scroll_content_h_signal_key, scroll_content_w_signal_key, scroll_offset_x_signal_key,
            scroll_offset_y_signal_key, scroll_viewport_h_signal_key, scroll_viewport_w_signal_key,
        };

        fn cfg(lock: ScrollLock) -> AutoScroll {
            AutoScroll {
                lock,
                ..AutoScroll::default()
            }
        }
        let views = [ScrollHit {
            rest: "page".into(),
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 200.0,
                height: 300.0,
            },
        }];

        let (_o, x_after_auto_y) = create_root(|| {
            use_signal(&scroll_offset_x_signal_key("page"), || 0.0_f32).set_silent(0.0);
            use_signal(&scroll_viewport_w_signal_key("page"), || 0.0_f32).set_silent(200.0);
            use_signal(&scroll_content_w_signal_key("page"), || 0.0_f32).set_silent(800.0);
            let _ = apply(
                [195.0, 100.0],
                &views,
                cfg(ScrollLock::Auto),
                1.0 / 60.0,
                Axis::Y,
            );
            use_signal(&scroll_offset_x_signal_key("page"), || 0.0_f32).peek()
        });
        assert!(
            x_after_auto_y.abs() < 0.5,
            "auto + axis=y must not pan X; got {x_after_auto_y}"
        );

        let (_o, y_after_auto_x) = create_root(|| {
            use_signal(&scroll_offset_y_signal_key("page"), || 0.0_f32).set_silent(0.0);
            use_signal(&scroll_viewport_h_signal_key("page"), || 0.0_f32).set_silent(300.0);
            use_signal(&scroll_content_h_signal_key("page"), || 0.0_f32).set_silent(800.0);
            let _ = apply(
                [100.0, 285.0],
                &views,
                cfg(ScrollLock::Auto),
                1.0 / 60.0,
                Axis::X,
            );
            use_signal(&scroll_offset_y_signal_key("page"), || 0.0_f32).peek()
        });
        assert!(
            y_after_auto_x.abs() < 0.5,
            "auto + axis=x must not pan Y; got {y_after_auto_x}"
        );

        let (_o, x_none_with_axis_y) = create_root(|| {
            use_signal(&scroll_offset_x_signal_key("page"), || 0.0_f32).set_silent(0.0);
            use_signal(&scroll_viewport_w_signal_key("page"), || 0.0_f32).set_silent(200.0);
            use_signal(&scroll_content_w_signal_key("page"), || 0.0_f32).set_silent(800.0);
            let _ = apply(
                [195.0, 100.0],
                &views,
                cfg(ScrollLock::None),
                1.0 / 60.0,
                Axis::Y,
            );
            use_signal(&scroll_offset_x_signal_key("page"), || 0.0_f32).peek()
        });
        assert!(
            x_none_with_axis_y > 1.0,
            "scroll_lock=none must pan X even when axis=y; got {x_none_with_axis_y}"
        );

        let (_o, x_all) = create_root(|| {
            use_signal(&scroll_offset_x_signal_key("page"), || 0.0_f32).set_silent(0.0);
            use_signal(&scroll_viewport_w_signal_key("page"), || 0.0_f32).set_silent(200.0);
            use_signal(&scroll_content_w_signal_key("page"), || 0.0_f32).set_silent(800.0);
            let _ = apply(
                [195.0, 100.0],
                &views,
                cfg(ScrollLock::All),
                1.0 / 60.0,
                Axis::Xy,
            );
            use_signal(&scroll_offset_x_signal_key("page"), || 0.0_f32).peek()
        });
        assert!(
            x_all.abs() < 0.5,
            "scroll_lock=all must freeze X even with axis=xy; got {x_all}"
        );

        let (_o, x_lock_x) = create_root(|| {
            use_signal(&scroll_offset_x_signal_key("page"), || 0.0_f32).set_silent(0.0);
            use_signal(&scroll_viewport_w_signal_key("page"), || 0.0_f32).set_silent(200.0);
            use_signal(&scroll_content_w_signal_key("page"), || 0.0_f32).set_silent(800.0);
            let _ = apply(
                [195.0, 100.0],
                &views,
                cfg(ScrollLock::X),
                1.0 / 60.0,
                Axis::Xy,
            );
            use_signal(&scroll_offset_x_signal_key("page"), || 0.0_f32).peek()
        });
        assert!(
            x_lock_x.abs() < 0.5,
            "scroll_lock=x must freeze X with axis=xy; got {x_lock_x}"
        );

        let (_o, y_lock_y) = create_root(|| {
            use_signal(&scroll_offset_y_signal_key("page"), || 0.0_f32).set_silent(0.0);
            use_signal(&scroll_viewport_h_signal_key("page"), || 0.0_f32).set_silent(300.0);
            use_signal(&scroll_content_h_signal_key("page"), || 0.0_f32).set_silent(800.0);
            let _ = apply(
                [100.0, 285.0],
                &views,
                cfg(ScrollLock::Y),
                1.0 / 60.0,
                Axis::Xy,
            );
            use_signal(&scroll_offset_y_signal_key("page"), || 0.0_f32).peek()
        });
        assert!(
            y_lock_y.abs() < 0.5,
            "scroll_lock=y must freeze Y with axis=xy; got {y_lock_y}"
        );

        assert_eq!(ScrollLock::parse("scroll-lock-all"), ScrollLock::All);
        assert_eq!(ScrollLock::parse("scroll-lock-x"), ScrollLock::X);
        assert_eq!(ScrollLock::parse("scroll_lock_y"), ScrollLock::Y);
        assert_eq!(ScrollLock::parse("scroll-lock-none"), ScrollLock::None);
        assert_eq!(ScrollLock::parse("x"), ScrollLock::X);
        assert_eq!(ScrollLock::parse("auto"), ScrollLock::Auto);
        assert_eq!(ScrollLock::parse("none"), ScrollLock::None);
        assert_eq!(ScrollLock::All.skip_axes(Axis::Xy), (true, true));
        assert_eq!(ScrollLock::X.skip_axes(Axis::Xy), (true, false));
        assert_eq!(ScrollLock::Y.skip_axes(Axis::Xy), (false, true));
        assert_eq!(ScrollLock::None.skip_axes(Axis::Y), (false, false));
        assert_eq!(ScrollLock::Auto.skip_axes(Axis::Y), (true, false));
        assert_eq!(ScrollLock::Auto.skip_axes(Axis::X), (false, true));
    }
}
