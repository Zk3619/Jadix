//! Rectangle edge/corner resize (interact.js `actions/resize`).
//!
//! Pure geometry: a start box + pointer delta + [`Edges`] → a new box.
//! Invert / square / aspect / min-max live here so Gallery and tests share one
//! seam. Not a layout widget — ShadCN [`crate::components::resizable`] stays
//! the one-axis splitter.

use super::BoxRect;
use crate::utils::edge_scroll::AutoScroll;

/// Which sides of the box the pointer owns (interact `edges`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Edges {
    pub top: bool,
    pub left: bool,
    pub bottom: bool,
    pub right: bool,
}

impl Edges {
    pub const NONE: Self = Self {
        top: false,
        left: false,
        bottom: false,
        right: false,
    };

    pub fn right() -> Self {
        Self {
            right: true,
            ..Self::NONE
        }
    }
    pub fn left() -> Self {
        Self {
            left: true,
            ..Self::NONE
        }
    }
    pub fn top() -> Self {
        Self {
            top: true,
            ..Self::NONE
        }
    }
    pub fn bottom() -> Self {
        Self {
            bottom: true,
            ..Self::NONE
        }
    }
    pub fn bottom_right() -> Self {
        Self {
            bottom: true,
            right: true,
            ..Self::NONE
        }
    }
    pub fn bottom_left() -> Self {
        Self {
            bottom: true,
            left: true,
            ..Self::NONE
        }
    }
    pub fn top_right() -> Self {
        Self {
            top: true,
            right: true,
            ..Self::NONE
        }
    }
    pub fn top_left() -> Self {
        Self {
            top: true,
            left: true,
            ..Self::NONE
        }
    }

    pub fn any(self) -> bool {
        self.top || self.left || self.bottom || self.right
    }

    pub fn axis(self) -> ResizeAxis {
        let x = self.left || self.right;
        let y = self.top || self.bottom;
        match (x, y) {
            (true, false) => ResizeAxis::X,
            (false, true) => ResizeAxis::Y,
            _ => ResizeAxis::Xy,
        }
    }

    /// CSS-ish cursor for the active edges (interact `getCursor`).
    pub fn cursor(self) -> &'static str {
        match (self.top, self.bottom, self.left, self.right) {
            (true, false, true, false) => "nwse-resize",
            (false, true, false, true) => "nwse-resize",
            (true, false, false, true) => "nesw-resize",
            (false, true, true, false) => "nesw-resize",
            (true, false, false, false) | (false, true, false, false) => "ns-resize",
            (false, false, true, false) | (false, false, false, true) => "ew-resize",
            _ => "nwse-resize",
        }
    }
}

/// interact `invert`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Invert {
    /// Clamp so the moving edge cannot pass the opposite (min 0×0, or [`ResizeOpts::min_size`]).
    #[default]
    None,
    /// Allow negative width/height (left may sit to the right of right).
    Negate,
    /// Keep width/height positive by swapping the inverted edges.
    Reposition,
}

impl Invert {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "negate" | "negative" => Self::Negate,
            "reposition" | "swap" => Self::Reposition,
            _ => Self::None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResizeAxis {
    #[default]
    Xy,
    X,
    Y,
}

impl ResizeAxis {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "x" | "horizontal" => Self::X,
            "y" | "vertical" => Self::Y,
            _ => Self::Xy,
        }
    }

    fn apply(self, dx: f32, dy: f32) -> (f32, f32) {
        match self {
            Self::X => (dx, 0.0),
            Self::Y => (0.0, dy),
            Self::Xy => (dx, dy),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResizeOpts {
    pub edges: Edges,
    pub invert: Invert,
    pub square: bool,
    pub preserve_aspect: bool,
    pub axis: ResizeAxis,
    /// Floor after invert. interact's `none` invert is 0×0; set this to keep a usable chrome.
    pub min_size: [f32; 2],
    /// Optional cap (RUI beyond interact).
    pub max_size: Option<[f32; 2]>,
    /// Grow/shrink about the center instead of the opposite edge (RUI beyond).
    pub from_center: bool,
    /// Optional edge auto-scroll while resizing (interact `autoScroll` on resize).
    pub autoscroll: Option<AutoScroll>,
}

impl Default for ResizeOpts {
    fn default() -> Self {
        Self {
            edges: Edges::NONE,
            invert: Invert::None,
            square: false,
            preserve_aspect: false,
            axis: ResizeAxis::Xy,
            min_size: [0.0, 0.0],
            max_size: None,
            from_center: false,
            autoscroll: None,
        }
    }
}

impl ResizeOpts {
    pub fn edges(mut self, edges: Edges) -> Self {
        self.edges = edges;
        self
    }
    pub fn invert(mut self, invert: Invert) -> Self {
        self.invert = invert;
        self
    }
    pub fn square(mut self, yes: bool) -> Self {
        self.square = yes;
        self
    }
    pub fn preserve_aspect(mut self, yes: bool) -> Self {
        self.preserve_aspect = yes;
        self
    }
    pub fn min_size(mut self, w: f32, h: f32) -> Self {
        self.min_size = [w.max(0.0), h.max(0.0)];
        self
    }
    pub fn max_size(mut self, w: f32, h: f32) -> Self {
        self.max_size = Some([w.max(0.0), h.max(0.0)]);
        self
    }
    pub fn from_center(mut self, yes: bool) -> Self {
        self.from_center = yes;
        self
    }
    /// Enable edge auto-scroll with defaults (`sensitivity` 40, `speed` 14).
    pub fn autoscroll(mut self, yes: bool) -> Self {
        self.autoscroll = if yes {
            Some(AutoScroll::default().lock(crate::utils::edge_scroll::ScrollLock::None))
        } else {
            None
        };
        self
    }
    /// Full edge-scroll config. `None` disables.
    pub fn autoscroll_opts(mut self, opts: AutoScroll) -> Self {
        self.autoscroll = Some(opts);
        self
    }
}

/// Hit-test pointer against a box's edge band (interact `margin`).
pub fn hit_edges(rect: BoxRect, pointer: [f32; 2], margin: f32) -> Edges {
    let m = margin.max(1.0);
    let [x, y] = pointer;
    let inside_x = x >= rect.x - m && x <= rect.right() + m;
    let inside_y = y >= rect.y - m && y <= rect.bottom() + m;
    if !inside_x || !inside_y {
        return Edges::NONE;
    }
    Edges {
        left: (x - rect.x).abs() <= m,
        right: (rect.right() - x).abs() <= m,
        top: (y - rect.y).abs() <= m,
        bottom: (rect.bottom() - y).abs() <= m,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ResizeSession {
    start: BoxRect,
    start_pointer: [f32; 2],
    opts: ResizeOpts,
    aspect: f32,
}

impl ResizeSession {
    pub fn begin(start: BoxRect, start_pointer: [f32; 2], opts: ResizeOpts) -> Self {
        let aspect = if start.height.abs() > 1e-6 {
            start.width / start.height
        } else {
            1.0
        };
        Self {
            start,
            start_pointer,
            opts,
            aspect,
        }
    }

    pub fn opts(&self) -> ResizeOpts {
        self.opts
    }

    pub fn tick(&self, pointer: [f32; 2]) -> BoxRect {
        resize_box(
            self.start,
            self.start_pointer,
            pointer,
            self.opts,
            self.aspect,
        )
    }
}

pub fn resize_box(
    start: BoxRect,
    start_pointer: [f32; 2],
    pointer: [f32; 2],
    opts: ResizeOpts,
    aspect: f32,
) -> BoxRect {
    let (dx, dy) = opts
        .axis
        .apply(pointer[0] - start_pointer[0], pointer[1] - start_pointer[1]);
    let edges = opts.edges;
    let mut left = start.x;
    let mut top = start.y;
    let mut right = start.right();
    let mut bottom = start.bottom();

    if opts.from_center {
        if edges.left || edges.right {
            left -= dx;
            right += dx;
        }
        if edges.top || edges.bottom {
            top -= dy;
            bottom += dy;
        }
    } else {
        if edges.left {
            left += dx;
        }
        if edges.right {
            right += dx;
        }
        if edges.top {
            top += dy;
        }
        if edges.bottom {
            bottom += dy;
        }
    }

    if opts.square {
        let w = right - left;
        let h = bottom - top;
        let side = w.abs().max(h.abs());
        apply_square_width(&mut left, &mut right, top, bottom, side, edges);
        apply_square_height(left, right, &mut top, &mut bottom, side, edges);
    } else if opts.preserve_aspect && aspect.is_finite() && aspect > 1e-6 {
        apply_aspect(&mut left, &mut right, &mut top, &mut bottom, aspect, edges);
    }

    match opts.invert {
        Invert::None => {
            let min_w = opts.min_size[0];
            let min_h = opts.min_size[1];
            if right < left + min_w {
                if edges.left && !edges.right {
                    left = right - min_w;
                } else {
                    right = left + min_w;
                }
            }
            if bottom < top + min_h {
                if edges.top && !edges.bottom {
                    top = bottom - min_h;
                } else {
                    bottom = top + min_h;
                }
            }
        }
        Invert::Negate => {}
        Invert::Reposition => {
            if left > right {
                std::mem::swap(&mut left, &mut right);
            }
            if top > bottom {
                std::mem::swap(&mut top, &mut bottom);
            }
        }
    }

    if let Some([max_w, max_h]) = opts.max_size {
        let w = right - left;
        if w.abs() > max_w && max_w > 0.0 {
            if w >= 0.0 {
                if edges.left && !edges.right {
                    left = right - max_w;
                } else {
                    right = left + max_w;
                }
            } else if edges.left && !edges.right {
                left = right + max_w;
            } else {
                right = left - max_w;
            }
        }
        let h = bottom - top;
        if h.abs() > max_h && max_h > 0.0 {
            if h >= 0.0 {
                if edges.top && !edges.bottom {
                    top = bottom - max_h;
                } else {
                    bottom = top + max_h;
                }
            } else if edges.top && !edges.bottom {
                top = bottom + max_h;
            } else {
                bottom = top - max_h;
            }
        }
    }

    BoxRect::from_ltrb(left, top, right, bottom)
}

fn apply_square_width(
    left: &mut f32,
    right: &mut f32,
    top: f32,
    bottom: f32,
    side: f32,
    edges: Edges,
) {
    let target = side.abs();
    if edges.left && !edges.right {
        *left = *right - target;
    } else if edges.right && !edges.left {
        *right = *left + target;
    } else {
        let _ = (top, bottom);
        let mid = (*left + *right) * 0.5;
        *left = mid - target * 0.5;
        *right = mid + target * 0.5;
    }
}

fn apply_square_height(
    left: f32,
    right: f32,
    top: &mut f32,
    bottom: &mut f32,
    side: f32,
    edges: Edges,
) {
    let target = side.abs();
    let _ = (left, right);
    if edges.top && !edges.bottom {
        *top = *bottom - target;
    } else if edges.bottom && !edges.top {
        *bottom = *top + target;
    } else {
        let mid = (*top + *bottom) * 0.5;
        *top = mid - target * 0.5;
        *bottom = mid + target * 0.5;
    }
}

fn apply_aspect(
    left: &mut f32,
    right: &mut f32,
    top: &mut f32,
    bottom: &mut f32,
    aspect: f32,
    edges: Edges,
) {
    let w = *right - *left;
    let h = *bottom - *top;
    if w.abs() < 1e-6 && h.abs() < 1e-6 {
        return;
    }
    let corner = (edges.left || edges.right) && (edges.top || edges.bottom);
    if corner {
        // Prefer the larger relative change so a corner drag feels like the pointer.
        if (w.abs() / aspect - h.abs()).abs() <= (h.abs() * aspect - w.abs()).abs() {
            set_height(top, bottom, w.abs() / aspect, edges);
        } else {
            set_width(left, right, h.abs() * aspect, edges);
        }
    } else if edges.left || edges.right {
        set_height(top, bottom, w.abs() / aspect, edges);
    } else {
        set_width(left, right, h.abs() * aspect, edges);
    }
}

fn set_width(left: &mut f32, right: &mut f32, width: f32, edges: Edges) {
    if edges.left && !edges.right {
        *left = *right - width;
    } else {
        *right = *left + width;
    }
}

fn set_height(top: &mut f32, bottom: &mut f32, height: f32, edges: Edges) {
    if edges.top && !edges.bottom {
        *top = *bottom - height;
    } else {
        *bottom = *top + height;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_xywh(x: f32, y: f32, w: f32, h: f32) -> BoxRect {
        BoxRect::from_xywh(x, y, w, h)
    }

    #[test]
    fn right_edge_grows_width() {
        let start = box_xywh(10.0, 20.0, 100.0, 50.0);
        let out = resize_box(
            start,
            [110.0, 45.0],
            [130.0, 45.0],
            ResizeOpts::default().edges(Edges::right()),
            2.0,
        );
        assert!((out.width - 120.0).abs() < 1e-4);
        assert!((out.x - 10.0).abs() < 1e-4);
        assert!((out.height - 50.0).abs() < 1e-4);
    }

    #[test]
    fn invert_none_stops_at_opposite_edge() {
        let start = box_xywh(0.0, 0.0, 80.0, 40.0);
        let out = resize_box(
            start,
            [80.0, 20.0],
            [-40.0, 20.0],
            ResizeOpts::default()
                .edges(Edges::right())
                .min_size(0.0, 0.0),
            2.0,
        );
        assert!(out.width >= 0.0);
        assert!(out.width.abs() < 1e-4);
        assert!((out.x - 0.0).abs() < 1e-4);
    }

    #[test]
    fn invert_reposition_swaps_when_crossed() {
        let start = box_xywh(0.0, 0.0, 80.0, 40.0);
        let out = resize_box(
            start,
            [80.0, 20.0],
            [-20.0, 20.0],
            ResizeOpts::default()
                .edges(Edges::right())
                .invert(Invert::Reposition),
            2.0,
        );
        assert!(out.width > 0.0);
        assert!((out.width - 20.0).abs() < 1e-3);
        assert!(out.x < 0.0);
    }

    #[test]
    fn invert_negate_keeps_negative_width() {
        let start = box_xywh(0.0, 0.0, 80.0, 40.0);
        let out = resize_box(
            start,
            [80.0, 20.0],
            [-20.0, 20.0],
            ResizeOpts::default()
                .edges(Edges::right())
                .invert(Invert::Negate),
            2.0,
        );
        assert!(out.width < 0.0);
    }

    #[test]
    fn square_corner_equalizes() {
        let start = box_xywh(0.0, 0.0, 40.0, 40.0);
        let out = resize_box(
            start,
            [40.0, 40.0],
            [70.0, 50.0],
            ResizeOpts::default()
                .edges(Edges::bottom_right())
                .square(true),
            1.0,
        );
        assert!((out.width - out.height).abs() < 1e-3);
        assert!(out.width >= 30.0);
    }

    #[test]
    fn preserve_aspect_on_width() {
        let start = box_xywh(0.0, 0.0, 80.0, 40.0);
        let out = resize_box(
            start,
            [80.0, 20.0],
            [120.0, 20.0],
            ResizeOpts::default()
                .edges(Edges::right())
                .preserve_aspect(true),
            2.0,
        );
        assert!((out.width / out.height - 2.0).abs() < 1e-3);
        assert!((out.width - 120.0).abs() < 1e-3);
    }

    #[test]
    fn min_and_max_size() {
        let start = box_xywh(0.0, 0.0, 80.0, 40.0);
        let too_small = resize_box(
            start,
            [80.0, 20.0],
            [10.0, 20.0],
            ResizeOpts::default()
                .edges(Edges::right())
                .min_size(40.0, 10.0),
            2.0,
        );
        assert!((too_small.width - 40.0).abs() < 1e-3);
        let too_big = resize_box(
            start,
            [80.0, 20.0],
            [400.0, 20.0],
            ResizeOpts::default()
                .edges(Edges::right())
                .max_size(100.0, 200.0),
            2.0,
        );
        assert!((too_big.width - 100.0).abs() < 1e-3);
    }

    #[test]
    fn from_center_grows_both_sides() {
        let start = box_xywh(50.0, 50.0, 40.0, 40.0);
        let out = resize_box(
            start,
            [90.0, 70.0],
            [110.0, 70.0],
            ResizeOpts::default()
                .edges(Edges::right())
                .from_center(true),
            1.0,
        );
        assert!((out.width - 80.0).abs() < 1e-3);
        assert!((out.center()[0] - 70.0).abs() < 1e-3);
    }

    #[test]
    fn hit_edges_margin() {
        let r = box_xywh(10.0, 10.0, 80.0, 40.0);
        let e = hit_edges(r, [10.0, 30.0], 8.0);
        assert!(e.left && !e.right);
        let miss = hit_edges(r, [200.0, 200.0], 8.0);
        assert!(!miss.any());
    }

    #[test]
    fn session_tick_matches_free_fn() {
        let start = box_xywh(0.0, 0.0, 10.0, 10.0);
        let opts = ResizeOpts::default().edges(Edges::bottom_right());
        let sess = ResizeSession::begin(start, [10.0, 10.0], opts);
        let a = sess.tick([18.0, 22.0]);
        let b = resize_box(start, [10.0, 10.0], [18.0, 22.0], opts, 1.0);
        assert_eq!(a.to_xywh(), b.to_xywh());
    }
}
