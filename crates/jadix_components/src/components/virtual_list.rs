//! # VirtualList
//!
//! 虚拟列表：主轴 spacer + 仅渲染可见项，支持横向布局和实际尺寸测量。
//!
//! ## 滚动与 `set_silent`
//!
//! 滚动偏移在每帧预渲染里用 [`Signal::set_silent`] 更新，因此不能靠
//! `reactive_view` + `.get()` 换窗。本组件用 [`ReactiveSync`] 每帧 `peek` 偏移。
//!
//! ## 增量窗口（keyed reuse）
//!
//! `[start, end)` 变化时：按 `template_id`（`{id}__row_{i}`）复用仍可见的行节点，
//! 只 `render_item` 新建进入窗口的行，离开窗口的行随 `HashMap` 丢弃销毁；spacer
//! 同样按 id 复用并改高度。避免整窗拆除重建。

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::{Rc, Weak};
use std::time::Duration;

use crate::components::base::SlotChrome;
use crate::components::scroll_area::{scroll_area, scroll_item_into_view_when};
use crate::core::reactivity::use_signal;
use crate::core::scroll_viewport::{
    cancel_scroll_end_x, cancel_scroll_end_y, reconcile_scroll_end_x, reconcile_scroll_end_y,
    request_scroll_end_x, request_scroll_end_y, scroll_content_h_signal_key,
    scroll_content_w_signal_key, scroll_offset_x_signal_key, scroll_offset_y_signal_key,
    scroll_target_x_signal_key, scroll_target_y_signal_key, scroll_viewport_h_signal_key,
    scroll_viewport_w_signal_key,
};
use crate::core::tree::{Element, LayoutStatus};
use crate::motion::{LayoutAnimation, LayoutMotion};
use crate::primitives::r#box::{r#box, hstack, vstack};
use crate::utils::virtual_list::{item_height_for_text, subtree_needs_virtual_retention};

// Rendering may update its backing data. Retry from a fresh snapshot before
// publishing viewport callbacks, but never let an unstable renderer spin a frame.
pub(super) fn stable_window_sync(
    element: Element,
    sync: impl Fn(&mut Element, &mut crate::core::tree::LayoutTree, &Cell<bool>) -> LayoutStatus
    + 'static,
) -> Element {
    element.reactive_sync(move |root, layout| {
        let mut status = LayoutStatus::Clean;
        for _ in 0..4 {
            let retry = Cell::new(false);
            status = status.merge(sync(root, layout, &retry));
            if !retry.get() {
                return status;
            }
        }
        crate::app::runtime::request_redraw();
        status
    })
}

pub use crate::virtualization::{
    ListContent, ListContentAlign, ListLoadEdges, ListOrder, ListScrollAlign, ListScrollPosition,
    ListStickyHeader, ListStickyHeaders, ListViewability, ListViewport, ViewableItems,
};
use crate::virtualization::{
    ListScrollAhead, ListViewabilityTracker, ParentViewport, UniformList, VirtualAxis,
};

const RANGE_ATTR: &str = "__jadix_vlist_range";
const REUSED_ATTR: &str = "__jadix_vlist_reused";
const CREATED_ATTR: &str = "__jadix_vlist_created";
const DROPPED_ATTR: &str = "__jadix_vlist_dropped";
const HEIGHT_ATTR: &str = "__jadix_vlist_item_height";
const STICKY_ATTR: &str = "__jadix_vlist_sticky";
const PIN_ATTR: &str = "__jadix_vlist_pin";
const ROW_KEY_ATTR: &str = "__jadix_vlist_row_key";
const ROW_INDEX_ATTR: &str = "__jadix_vlist_row_index";

type PositionCallback = Rc<dyn Fn(Option<ListScrollPosition>)>;
type VisibleRangeCallback = Rc<dyn Fn(Option<Range<usize>>)>;

#[derive(Default)]
struct VirtualListGeometry {
    axis: RefCell<Option<VirtualAxis<String>>>,
    count: Cell<usize>,
    item_height: Cell<f32>,
    item_gap: Cell<f32>,
    padding: Cell<[f64; 2]>,
    order: Cell<ListOrder>,
    viewport: RefCell<Option<ListViewport>>,
    sticky_header: Cell<Option<ListStickyHeader>>,
    request: Cell<Option<ListScrollRequest>>,
    binding: RefCell<
        Option<(
            crate::prelude::UiNode,
            Weak<()>,
            crate::core::reactivity::Signal<()>,
        )>,
    >,
}

#[derive(Clone, Copy)]
enum ListScrollRequest {
    Index {
        index: usize,
        align: ListScrollAlign,
        extra: f64,
        smooth: bool,
    },
    End,
}

/// Geometry and jump navigation of one mounted list, updated during frame sync.
/// Unmeasured rows use the configured estimate. Use a separate handle per list.
#[derive(Clone, Default)]
pub struct VirtualListHandle(Rc<VirtualListGeometry>);

impl PartialEq for VirtualListHandle {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for VirtualListHandle {}

impl VirtualListHandle {
    /// Queue a jump in this list's owning window. Latest request wins. Returns
    /// false before attachment, after disposal, or for invalid alignment/offset.
    /// Indices resolve against the next synchronized dataset; missing rows cancel.
    /// Unseen rows use estimates; the destination is realigned after measurement.
    /// Explicit controlled focused_index may override navigation on later frames.
    pub fn scroll_to_index(&self, index: usize, align: ListScrollAlign, extra: f64) -> bool {
        if align.validate().is_err() || !extra.is_finite() {
            return false;
        }
        self.request(ListScrollRequest::Index {
            index,
            align,
            extra,
            smooth: false,
        })
    }

    /// Animate toward the row using the scroll viewport's reduced-motion policy.
    /// Unmeasured rows use estimates; virtualization follows the actual offset.
    pub fn scroll_to_index_smooth(&self, index: usize, align: ListScrollAlign, extra: f64) -> bool {
        if align.validate().is_err() || !extra.is_finite() { return false; }
        self.request(ListScrollRequest::Index { index, align, extra, smooth: true })
    }

    /// Jump to the visual bottom/right using the current measured/estimated extent,
    /// including an empty or inverted list.
    pub fn scroll_to_end(&self) -> bool {
        self.request(ListScrollRequest::End)
    }

    fn request(&self, request: ListScrollRequest) -> bool {
        let binding = self.0.binding.borrow();
        let Some((node, _, _)) = binding
            .as_ref()
            .filter(|(_, owner, lifetime)| owner.strong_count() > 0 && lifetime.is_alive())
        else {
            return false;
        };
        self.0.request.set(Some(request));
        node.mark_layout();
        true
    }

    /// Last synchronized actual viewport; None before attachment/measurement.
    /// Safe to read inside list callbacks, without copying the measurement index.
    pub fn viewport(&self) -> Option<ListViewport> {
        self.0.viewport.borrow().clone()
    }

    /// Last synchronized pin, in natural content coordinates. Hidden or empty
    /// lists return None. Visibility callbacks continue to describe natural rows.
    pub fn sticky_header(&self) -> Option<ListStickyHeader> {
        self.0.sticky_header.get()
    }

    /// Return the row's content offset including scrolling slots and alignment
    /// padding, or the complete extent for `index == count` (also when empty).
    /// Returns None before attachment, outside the list, or during a row update.
    pub fn row_offset(&self, index: usize) -> Option<f64> {
        if self.0.item_height.get() <= 0.0 || index > self.0.count.get() {
            return None;
        }
        let index = if index == self.0.count.get() {
            index
        } else {
            self.0.order.get().index(index, self.0.count.get())?
        };
        let axis = self.0.axis.try_borrow().ok()?;
        let position = match axis.as_ref() {
            Some(axis) => axis.position(index),
            None => UniformList::new(
                self.0.count.get(),
                f64::from(self.0.item_height.get()),
                f64::from(self.0.item_gap.get()),
            )
            .ok()?
            .position(index),
        }?;
        let [leading, trailing] = self.0.padding.get();
        Some(
            position
                + leading
                + if index == self.0.count.get() {
                    trailing
                } else {
                    0.0
                },
        )
    }
}

fn window_key(item_count: usize, data_key: &str, start: usize, end: usize) -> String {
    format!("{item_count}|{start}|{end}|{data_key}")
}

fn parse_window_key(key: &str) -> Option<(usize, usize, usize, String)> {
    let mut parts = key.splitn(4, '|');
    let count = parts.next()?.parse().ok()?;
    let start = parts.next()?.parse().ok()?;
    let end = parts.next()?.parse().ok()?;
    let data = parts.next()?.to_string();
    Some((count, start, end, data))
}

pub struct VirtualListBuilder {
    id: String,
    handle: VirtualListHandle,
    item_count: usize,
    item_count_fn: Option<Rc<dyn Fn() -> usize>>,
    focused_index_fn: Option<Rc<dyn Fn() -> Option<usize>>>,
    focused_index_align: Option<&'static str>,
    retained_indices_fn: Option<Rc<dyn Fn() -> Vec<usize>>>,
    data_key_fn: Option<Rc<dyn Fn() -> String>>,
    item_key_fn: Option<Rc<dyn Fn(usize) -> String>>,
    maintain_visible_content_position: bool,
    content_width_fn: Option<Rc<dyn Fn() -> f32>>,
    horizontal: bool,
    use_parent_scroll: bool,
    inverted: bool,
    item_height: f32,
    item_gap: f32,
    item_height_fn: Option<Rc<dyn Fn() -> f32>>,
    measure_rows: bool,
    sticky_headers: ListStickyHeaders,
    sticky_headers_fn: Option<Rc<dyn Fn() -> Vec<usize>>>,
    sticky_header_offset: f32,
    sticky_header_offset_fn: Option<Rc<dyn Fn() -> f32>>,
    animate_item_changes: Option<LayoutAnimation>,
    on_scroll_position: Option<PositionCallback>,
    on_visible_range_change: Option<VisibleRangeCallback>,
    on_viewable_items_change: Option<Rc<dyn Fn(ViewableItems)>>,
    viewability: ListViewability,
    minimum_view_time: Duration,
    on_load_more: Option<Rc<dyn Fn()>>,
    on_load_more_at_start: Option<Rc<dyn Fn()>>,
    load_more_threshold: f64,
    load_more_at_start_threshold: Option<f64>,
    overscan: usize,
    scroll_ahead: bool,
    height: f32,
    initial_scroll_at_end: bool,
    content_align: ListContentAlign,
    follow_end: Option<bool>,
    follow_end_threshold: f32,
    scrollbar_visibility: String,
    scroll_behavior: String,
    render_item: Rc<dyn Fn(usize) -> Element>,
    slots: [Option<(f32, Element)>; 3],
    chrome: SlotChrome,
}

pub fn virtual_list(id: impl Into<String>) -> VirtualListBuilder {
    VirtualListBuilder {
        id: id.into(),
        handle: VirtualListHandle::default(),
        item_count: 0,
        item_count_fn: None,
        focused_index_fn: None,
        focused_index_align: None,
        retained_indices_fn: None,
        data_key_fn: None,
        item_key_fn: None,
        maintain_visible_content_position: true,
        content_width_fn: None,
        horizontal: false,
        use_parent_scroll: false,
        inverted: false,
        item_height: 32.0,
        item_gap: 0.0,
        item_height_fn: None,
        measure_rows: false,
        sticky_headers: ListStickyHeaders::default(),
        sticky_headers_fn: None,
        sticky_header_offset: 0.0,
        sticky_header_offset_fn: None,
        animate_item_changes: None,
        on_scroll_position: None,
        on_visible_range_change: None,
        on_viewable_items_change: None,
        viewability: ListViewability::default(),
        minimum_view_time: Duration::ZERO,
        on_load_more: None,
        on_load_more_at_start: None,
        load_more_threshold: 1.0,
        load_more_at_start_threshold: None,
        overscan: 2,
        scroll_ahead: false,
        height: 240.0,
        initial_scroll_at_end: false,
        content_align: ListContentAlign::Start,
        follow_end: None,
        follow_end_threshold: 0.0,
        scrollbar_visibility: "auto".to_string(),
        scroll_behavior: crate::core::scroll_viewport::DEFAULT_SCROLL_BEHAVIOR.to_string(),
        render_item: Rc::new(|i| Element::new().text(format!("item {i}"))),
        slots: [None, None, None],
        chrome: SlotChrome::default(),
    }
}

impl VirtualListBuilder {
    crate::impl_slot_chrome_methods!();

    /// Participate in the nearest registered same-axis scroll ancestor's layout.
    /// The list has its full natural extent and no private scroll viewport.
    /// Visibility uses the full parent viewport and signed local offsets. Initial
    /// end, follow-end, inverted order and key-reorder anchoring are disabled.
    /// Missing/hidden owners defer navigation and mount only explicit retained rows.
    /// A registered scroll ancestor is required; the window is not an implicit owner.
    pub fn use_parent_scroll(mut self) -> Self {
        self.use_parent_scroll = true;
        self
    }

    /// Preserve the leading row and depth when stable item keys move. Enabled
    /// by default with item_key_fn. Deleted anchors select the next surviving
    /// old row, then the preceding one, at its start. Full replacement keeps the
    /// pixel offset clamped to the new content. Same-count edits need data_key.
    /// Explicit navigation and follow_end take priority. False retains pixels
    /// during key mutations; size measurement still preserves its row anchor.
    pub fn maintain_visible_content_position(mut self, enabled: bool) -> Self {
        self.maintain_visible_content_position = enabled;
        self
    }

    /// Virtualize along X. Item extents and gaps apply to widths; the measured
    /// viewport width controls navigation. Direction is fixed at construction.
    /// `height` still sizes the viewport; `content_width_fn` is vertical-only.
    pub fn horizontal(mut self) -> Self {
        self.horizontal = true;
        self
    }

    /// Place item 0 at the visual end (bottom/right), opening there and aligning
    /// short content there. Public indices and loading edges retain dataset order;
    /// scroll alignment and offsets use visual coordinates. Stable keys preserve
    /// the leading visible row during data edits. Without keys, fixed-size lists
    /// retain distance from the end; measured lists retain rows by logical index.
    /// Growth while at the visual end follows that end by default; follow_end(false)
    /// disables pinning (unkeyed fixed-size lists still retain end distance).
    /// Sticky headers are disabled in this mode. Direction is fixed at construction.
    pub fn inverted(mut self) -> Self {
        self.inverted = true;
        self
    }

    /// Main-axis item estimate, in pixels, for either orientation.
    pub fn item_extent(self, extent: f32) -> Self {
        self.item_height(extent)
    }

    /// Dynamic main-axis item estimate, polled each frame.
    pub fn item_extent_fn(self, f: impl Fn() -> f32 + 'static) -> Self {
        self.item_height_fn(f)
    }

    /// Share the list's existing geometry without copying its measurement index.
    pub fn handle(mut self, handle: VirtualListHandle) -> Self {
        self.handle = handle;
        self
    }
    pub fn scroll_behavior(mut self, behavior: impl Into<String>) -> Self {
        self.scroll_behavior = behavior.into();
        self
    }
    pub fn item_count(mut self, n: usize) -> Self {
        self.item_count = n;
        self
    }

    /// Dynamic row count polled each frame in [`ReactiveSync`] (peek-only; no effect subscription).
    pub fn item_count_fn(mut self, f: impl Fn() -> usize + 'static) -> Self {
        self.item_count_fn = Some(Rc::new(f));
        self
    }

    /// Keep the focused row and its neighbors mounted, revealing it when needed.
    /// The callback is polled in [`ReactiveSync`] without subscribing the UI effect.
    /// One-shot destinations wait for a measured viewport; newer navigation or a
    /// data/count revision cancels a pending destination.
    pub fn focused_index_fn(mut self, f: impl Fn() -> Option<usize> + 'static) -> Self {
        self.focused_index_fn = Some(Rc::new(f));
        self
    }

    /// Focus owners, live label selections and declared portal hosts are kept
    /// automatically. This callback adds application-specific retention.
    /// Keep selected data indices mounted outside the viewport, for example an
    /// open overlay's host or a selection endpoint. Polled each frame without
    /// subscribing the UI effect. Does not scroll or report these rows as visible.
    /// Duplicates and out-of-bounds indices are ignored. Inverted lists still use
    /// data indices; callers update indices when data moves, while `item_key_fn`
    /// preserves surviving mounted nodes. Retain only the rows that need state.
    pub fn retained_indices_fn(mut self, f: impl Fn() -> Vec<usize> + 'static) -> Self {
        self.retained_indices_fn = Some(Rc::new(f));
        self
    }

    /// Align a requested focused row using the same geometry as the mounted window.
    /// Other values retain the default nearest-edge reveal.
    pub fn focused_index_align(mut self, align: &str) -> Self {
        self.focused_index_align = match align {
            "start" => Some("start"),
            "center" => Some("center"),
            "end" => Some("end"),
            _ => None,
        };
        self
    }

    /// Content fingerprint polled each frame; when it changes, visible rows are re-rendered
    /// even if `[start, end)` is unchanged (TreeView expand/collapse with same window).
    pub fn data_key_fn(mut self, f: impl Fn() -> String + 'static) -> Self {
        self.data_key_fn = Some(Rc::new(f));
        self
    }

    /// Stable identity for each item. Keys must be unique and follow the item
    /// across insertions and reordering. In measured mode, extents and mounted
    /// row identities are retained by key; `data_key_fn` signals when the
    /// ordered item set or rendered contents change. Duplicate keys panic.
    pub fn item_key_fn(mut self, f: impl Fn(usize) -> String + 'static) -> Self {
        self.item_key_fn = Some(Rc::new(f));
        self
    }

    pub fn item_height(mut self, h: f32) -> Self {
        self.item_height = if h.is_finite() { h.max(1.0) } else { 1.0 };
        self
    }

    /// Space between rows in logical pixels, with no trailing gap. Excluded
    /// from row measurement and visibility thresholds. Invalid values retain
    /// the previous gap. Works with uniform and measured row heights.
    pub fn item_gap(mut self, gap: f32) -> Self {
        if gap.is_finite() && gap >= 0.0 {
            self.item_gap = gap;
        }
        self
    }

    /// Poll a uniform row height each frame. Size changes preserve the first
    /// visible row and its pixel depth (clamped to the new height), then snap
    /// offset and animation target together. Invalid heights use `item_height`.
    pub fn item_height_fn(mut self, height: impl Fn() -> f32 + 'static) -> Self {
        self.item_height_fn = Some(Rc::new(height));
        self
    }

    /// Measure mounted rows after layout, using `item_height` as the estimate
    /// for unmeasured rows. Later size changes preserve the viewport's row and
    /// depth; `follow_end` also applies to measurement updates. Cross-axis size and data
    /// key changes invalidate measurements. Horizontal lists measure widths, vertical
    /// lists measure heights. This mode keeps an index for up to
    /// 1,000,000 rows; the default fixed-height mode needs no such index.
    pub fn measure_rows(mut self) -> Self {
        self.measure_rows = true;
        self
    }

    /// Pin section headers on the main axis. Indices are sorted/deduplicated;
    /// at most one extra row is mounted outside the ordinary window. The next
    /// header pushes the current one away; the last stops at the content end.
    pub fn sticky_header_indices(mut self, indices: impl IntoIterator<Item = usize>) -> Self {
        self.sticky_headers = ListStickyHeaders::new(indices);
        self.sticky_headers_fn = None;
        self
    }

    /// Refresh header indices on count or data-key changes, never on scrolling.
    /// Update `data_key_fn` when section membership changes at the same count.
    pub fn sticky_header_indices_fn(mut self, f: impl Fn() -> Vec<usize> + 'static) -> Self {
        self.sticky_headers_fn = Some(Rc::new(f));
        self
    }

    /// Signed main-axis viewport inset, e.g. for an overlapping toolbar.
    /// Nonfinite values retain the current inset.
    pub fn sticky_header_offset(mut self, offset: f32) -> Self {
        if offset.is_finite() {
            self.sticky_header_offset = offset;
        }
        self
    }

    /// Poll the inset each frame. Nonfinite results use the configured inset.
    pub fn sticky_header_offset_fn(mut self, f: impl Fn() -> f32 + 'static) -> Self {
        self.sticky_header_offset_fn = Some(Rc::new(f));
        self
    }

    /// Animate retained rows when stable item keys move or change size.
    ///
    /// Requires [`Self::item_key_fn`]. New rows and rows recycled for another
    /// key appear at their final position. The animation uses the shared layout
    /// motion feature and respects reduced-motion settings.
    pub fn animate_item_changes(mut self, mode: LayoutAnimation) -> Self {
        self.animate_item_changes = Some(mode);
        self
    }

    /// Report the row and depth at the viewport's start on frame sync, only
    /// when changed. Uses the latest measured viewport; before measurement or
    /// with empty data the value is None. Coordinates exclude overscan.
    pub fn on_scroll_position(
        mut self,
        callback: impl Fn(Option<ListScrollPosition>) + 'static,
    ) -> Self {
        self.on_scroll_position = Some(Rc::new(callback));
        self
    }

    /// Report the half-open actual visible range when its row boundaries change.
    /// Excludes overscan and focus pins; None means no measured viewport yet.
    /// Measured empty data reports Some(0..0). Runs after geometry/row updates.
    pub fn on_visible_range_change(
        mut self,
        callback: impl Fn(Option<Range<usize>>) + 'static,
    ) -> Self {
        self.on_visible_range_change = Some(Rc::new(callback));
        self
    }

    /// Required overlap for partial rows; complete rows always qualify.
    /// Invalid fractions retain the current policy. Excludes overscan/focus pins.
    pub fn viewability(mut self, policy: ListViewability) -> Self {
        if policy.validate().is_ok() {
            self.viewability = policy;
        }
        self
    }

    /// Continuous qualifying time per row before reporting it (default zero).
    /// Dataset revisions and hidden/offscreen suspension restart dwell.
    pub fn minimum_view_time(mut self, duration: Duration) -> Self {
        self.minimum_view_time = duration;
        self
    }

    /// Report sorted current/previous indices after layout when membership changes.
    /// Runs outside geometry borrows; callbacks may read the handle/update data.
    /// Uses the node-owned deadline scheduler, with no idle wake or background timer.
    pub fn on_viewable_items_change(mut self, callback: impl Fn(ViewableItems) + 'static) -> Self {
        self.on_viewable_items_change = Some(Rc::new(callback));
        self
    }

    /// Request another page near the end, including an initially empty list.
    /// Fires once per approach, rearmed by count/data-key changes or moving away.
    /// The caller owns loading/exhaustion guards. Uses the measured viewport,
    /// never overscan; callbacks may inspect the handle or update data signals.
    pub fn on_load_more(mut self, callback: impl Fn() + 'static) -> Self {
        self.on_load_more = Some(Rc::new(callback));
        self
    }

    /// Request preceding rows near the start of nonempty data. Like end loading,
    /// initial proximity can trigger it; use a caller guard when history ends.
    pub fn on_load_more_at_start(mut self, callback: impl Fn() + 'static) -> Self {
        self.on_load_more_at_start = Some(Rc::new(callback));
        self
    }

    /// Prefetch distance in viewport extents (default 1). Zero waits for an edge;
    /// invalid/negative values retain the current threshold.
    pub fn load_more_threshold(mut self, viewports: f64) -> Self {
        if viewports.is_finite() && viewports >= 0.0 {
            self.load_more_threshold = viewports;
        }
        self
    }

    /// Override only the logical start prefetch distance. Otherwise both edges
    /// use load_more_threshold. Invalid values retain the previous setting.
    pub fn load_more_at_start_threshold(mut self, viewports: f64) -> Self {
        if viewports.is_finite() && viewports >= 0.0 {
            self.load_more_at_start_threshold = Some(viewports);
        }
        self
    }

    /// Enable cross-axis overflow in a vertical list, with minimum body width polled each frame.
    pub fn content_width_fn(mut self, width: impl Fn() -> f32 + 'static) -> Self {
        self.content_width_fn = Some(Rc::new(width));
        self
    }

    /// Set row height from font size × leading scale (see [`item_height_for_text`]).
    pub fn item_height_from_font(self, font_size: f32, leading_scale: f32) -> Self {
        self.item_height(item_height_for_text(font_size, leading_scale))
    }

    pub fn overscan(mut self, n: usize) -> Self {
        self.overscan = n;
        self
    }

    /// Add up to one viewport of directional pre-rendering during continuous
    /// scrolling, on top of row overscan. Defaults to false. Supports both axes,
    /// measured rows and inversion; actual visibility/loading remain unchanged.
    /// Teleports, navigation, dataset changes and resize rebase prediction.
    pub fn scroll_ahead(mut self, enabled: bool) -> Self {
        self.scroll_ahead = enabled;
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = if h.is_finite() { h.max(1.0) } else { 1.0 };
        self
    }

    /// Start with the final rows visible. The initial position is a snap, so it does not
    /// animate on mount. Later row-count changes keep the user's current scroll position
    /// unless [`Self::follow_end`] is enabled and the user remains at the end.
    pub fn initial_scroll_at_end(mut self) -> Self {
        self.initial_scroll_at_end = true;
        self
    }

    /// Align short content within the main-axis viewport. This does not change
    /// initial scrolling or following. Defaults to Start, preserving natural size.
    pub fn content_align(mut self, align: ListContentAlign) -> Self {
        self.content_align = align;
        self
    }

    /// Keep the viewport at the end when rows or viewport geometry change,
    /// provided the user was within the configured threshold (zero by default).
    /// Scrolling away suspends following. This does not change the initial position.
    pub fn follow_end(mut self, follow: bool) -> Self {
        self.follow_end = Some(follow);
        self
    }

    /// Enable end following within this many logical units of the previous end.
    /// Nonfinite or negative values leave the configuration unchanged. A later
    /// follow_end(false) disables following without changing the threshold.
    pub fn follow_end_threshold(mut self, distance: f32) -> Self {
        if distance.is_finite() && distance >= 0.0 {
            self.follow_end_threshold = distance;
            self.follow_end = Some(true);
        }
        self
    }

    /// Scrollbar visibility (`auto` / `always` / `scroll` / …). Prefer `always` in demos.
    pub fn scrollbar_visibility(mut self, v: impl Into<String>) -> Self {
        self.scrollbar_visibility = v.into();
        self
    }

    /// Scrolling header before the first logical item. Extent is its full main-axis
    /// size, including any desired spacing; item_gap applies only between rows.
    /// In inverted lists the header follows the reversed rows visually.
    pub fn header(mut self, extent: f32, content: impl Into<Element>) -> Self {
        assert!(
            extent.is_finite() && extent >= 0.0,
            "invalid list header extent"
        );
        self.slots[0] = Some((extent, content.into()));
        self
    }

    /// Scrolling footer after the last logical item, with an explicit main-axis size.
    pub fn footer(mut self, extent: f32, content: impl Into<Element>) -> Self {
        assert!(
            extent.is_finite() && extent >= 0.0,
            "invalid list footer extent"
        );
        self.slots[1] = Some((extent, content.into()));
        self
    }

    /// Content shown between header and footer when the dataset is empty.
    /// Its node is parked across nonempty states and restored without rebuilding.
    pub fn empty(mut self, extent: f32, content: impl Into<Element>) -> Self {
        assert!(
            extent.is_finite() && extent >= 0.0,
            "invalid list empty extent"
        );
        self.slots[2] = Some((extent, content.into()));
        self
    }

    pub fn render_item(mut self, f: impl Fn(usize) -> Element + 'static) -> Self {
        self.render_item = Rc::new(f);
        self
    }
}

fn top_spacer_id(list_id: &str) -> String {
    format!("{list_id}__vlist_top")
}

fn bot_spacer_id(list_id: &str) -> String {
    format!("{list_id}__vlist_bot")
}

fn row_id(list_id: &str, index: usize, stable_key: Option<&str>) -> String {
    match stable_key {
        Some(key) => {
            let encoded = encode_key(key);
            format!("{list_id}__key_{encoded}")
        }
        None => format!("{list_id}__row_{index}"),
    }
}

fn axis_keys(count: usize, stable_keys: Option<&[String]>) -> Vec<String> {
    match stable_keys {
        Some(keys) => keys.iter().map(|key| format!("k:{key}")).collect(),
        None => (0..count).map(|index| format!("i:{index}")).collect(),
    }
}

fn collect_stable_keys(count: usize, key_fn: &dyn Fn(usize) -> String) -> Vec<String> {
    let keys = (0..count).map(|index| key_fn(index)).collect::<Vec<_>>();
    let unique = keys.iter().collect::<HashSet<_>>();
    assert_eq!(
        unique.len(),
        keys.len(),
        "VirtualList item keys must be unique"
    );
    keys
}

fn axis_key(index: usize, stable_keys: Option<&[String]>) -> String {
    stable_keys
        .and_then(|keys| keys.get(index))
        .map(|key| format!("k:{key}"))
        .unwrap_or_else(|| format!("i:{index}"))
}

fn stable_key_for(index: usize, stable_keys: Option<&[String]>) -> Option<&str> {
    stable_keys.and_then(|keys| keys.get(index).map(String::as_str))
}

fn parse_row_index(list_id: &str, template_id: &str) -> Option<usize> {
    let prefix = format!("{list_id}__row_");
    template_id.strip_prefix(&prefix)?.parse().ok()
}

fn main_extent(el: Element, horizontal: bool, extent: f32) -> Element {
    if horizontal {
        el.w(extent).min_w(extent).h("100%").flex_shrink(0.0)
    } else {
        el.h(extent).min_h(extent).w("100%").flex_shrink(0.0)
    }
}

fn make_spacer(horizontal: bool, list_id: &str, which: &str, height: f32) -> Element {
    let tid = format!("{list_id}__vlist_{which}");
    main_extent(r#box().into(), horizontal, height).template_id(tid)
}

#[cfg(test)]
fn focused_item_window(
    offset: f32,
    viewport: f32,
    item_height: f32,
    item_gap: f32,
    item_count: usize,
    overscan: usize,
    focused: Option<usize>,
) -> (usize, usize, Option<&'static str>) {
    if !offset.is_finite() || !viewport.is_finite() || viewport <= 0.0 {
        return (0, 0, None);
    }
    let offset = offset.max(0.0);
    let geometry =
        UniformList::new(item_count, f64::from(item_height), f64::from(item_gap)).unwrap();
    let range = |offset| {
        let v = geometry
            .viewport(f64::from(offset), f64::from(viewport))
            .unwrap()
            .visible;
        (
            v.start.saturating_sub(overscan),
            v.end.saturating_add(overscan).min(item_count),
        )
    };
    let focused = focused.filter(|index| *index < item_count);
    let mut window_offset = offset;
    let mut reveal_align = None;
    if let Some(index) = focused {
        let visible = geometry
            .viewport(f64::from(offset), f64::from(viewport))
            .unwrap()
            .visible;
        let (visible_start, visible_end) = (visible.start, visible.end);
        if index < visible_start {
            reveal_align = Some("end");
            window_offset =
                (geometry.position(index).unwrap() as f32 + item_height - viewport).max(0.0);
        } else if index >= visible_end {
            reveal_align = Some("start");
            window_offset = (geometry.position(index).unwrap() as f32)
                .min((geometry.total() as f32 - viewport).max(0.0));
        }
    }
    let (mut start, mut end) = range(window_offset);
    if let Some(index) = focused {
        start = start.min(index.saturating_sub(1));
        end = end.max(index.saturating_add(2).min(item_count));
    }
    (start, end, reveal_align)
}

fn tune_spacer(horizontal: bool, mut el: Element, height: f32) -> Element {
    el = main_extent(el, horizontal, height);
    el
}

fn tune_row(
    horizontal: bool,
    mut el: Element,
    list_id: &str,
    index: usize,
    stable_key: Option<&str>,
    axis_key: &str,
    item_height: f32,
    item_gap: f32,
    measured: bool,
    animate_item_changes: Option<LayoutAnimation>,
) -> Element {
    el = el
        .template_id(row_id(list_id, index, stable_key))
        .flex_shrink(0.0);
    el = if horizontal {
        el.h("100%")
    } else {
        el.w("100%")
    };
    if !measured {
        el = main_extent(el, horizontal, item_height);
        el = if horizontal {
            el.max_w(item_height)
        } else {
            el.max_h(item_height)
        };
    }
    el.attrs
        .insert(ROW_KEY_ATTR.to_string(), axis_key.to_string());
    el.attrs.insert(ROW_INDEX_ATTR.into(), index.to_string());
    if item_gap > 0.0 || el.attrs.contains_key("__jadix_vlist_gap") {
        el = if horizontal {
            el.mr(item_gap)
        } else {
            el.mb(item_gap)
        };
        el.attrs.insert("__jadix_vlist_gap".into(), String::new());
    }
    if let (Some(mode), Some(key)) = (animate_item_changes, stable_key) {
        let layout_id = format!("{list_id}__vlist_item_{}", encode_key(key));
        el = el
            .layout_motion(&LayoutMotion::new(mode))
            .layout_id(layout_id);
    }
    el
}

fn encode_key(key: &str) -> String {
    key.as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn row_geometry(
    axis: Option<&VirtualAxis<String>>,
    count: usize,
    height: f32,
    gap: f32,
    start: usize,
    end: usize,
) -> [f32; 3] {
    match axis {
        Some(axis) => [
            axis.total() as f32,
            axis.position(start).unwrap() as f32,
            (axis.total() - axis.position(end).unwrap()).max(0.0) as f32,
        ],
        None => {
            let geometry = UniformList::new(count, f64::from(height), f64::from(gap)).unwrap();
            [
                geometry.total() as f32,
                geometry.position(start).unwrap() as f32,
                (geometry.total() - geometry.position(end).unwrap()).max(0.0) as f32,
            ]
        }
    }
}

fn render_row(
    horizontal: bool,
    render: &dyn Fn(usize) -> Element,
    index: usize,
    measured: bool,
) -> Element {
    let child = render(index);
    if measured {
        if horizontal {
            hstack().child(child).into()
        } else {
            vstack().child(child).into()
        }
    } else {
        child
    }
}

fn apply_body_style(horizontal: bool, root: &mut Element, total_h: f32) {
    // Keep layout_node; only refresh size so taffy can reuse the body node.
    *root = main_extent(std::mem::take(root), horizontal, total_h);
}

// Only list-owned wrappers are translated; authored child styles stay intact.
fn apply_sticky_header(
    root: &mut Element,
    horizontal: bool,
    pinned_id: Option<&str>,
    translation: f32,
) -> LayoutStatus {
    let mut changed = false;
    for row in &mut root.children {
        if !row.attrs.contains_key(ROW_KEY_ATTR) {
            continue;
        }
        let active = pinned_id.is_some() && row.template_id.as_deref() == pinned_id;
        let value = active.then(|| translation.to_string());
        if row.attrs.get(STICKY_ATTR) == value.as_ref() {
            continue;
        }
        if active || row.attrs.contains_key(STICKY_ATTR) {
            let offset = if active { translation } else { 0.0 };
            let el = std::mem::take(row)
                .relative()
                .z_index(if active { 1 } else { 0 });
            *row = if horizontal {
                el.left(offset)
            } else {
                el.top(offset)
            };
            match value {
                Some(v) => {
                    row.attrs.insert(STICKY_ATTR.into(), v);
                }
                None => {
                    row.attrs.remove(STICKY_ATTR);
                }
            }
            changed = true;
        }
    }
    if changed {
        LayoutStatus::LayoutChanged
    } else {
        LayoutStatus::Clean
    }
}

// At most three authored nodes. Park the empty state across data changes; keep
// slot wrappers out of row measurement, callbacks and keyed-row accounting.
struct ListSlots {
    extents: [f64; 3],
    parked: RefCell<[Option<Element>; 3]>,
    present: bool,
}

impl ListSlots {
    fn new(mut slots: [Option<(f32, Element)>; 3], horizontal: bool, inverted: bool) -> Self {
        if inverted {
            slots.swap(0, 1);
        }
        let present = slots.iter().any(Option::is_some);
        let extents = slots
            .each_ref()
            .map(|slot| slot.as_ref().map_or(0.0, |s| f64::from(s.0)));
        let mut i = 0;
        let parked = RefCell::new(slots.map(|slot| {
            let index = i;
            i += 1;
            slot.map(|(extent, child)| {
                let mut wrapper = main_extent(
                    if horizontal {
                        hstack().child(child).into()
                    } else {
                        vstack().child(child).into()
                    },
                    horizontal,
                    extent,
                );
                wrapper
                    .attrs
                    .insert("__jadix_vlist_slot".into(), index.to_string());
                wrapper
            })
        }));
        Self {
            extents,
            parked,
            present,
        }
    }

    fn insets(&self, count: usize) -> [f64; 2] {
        [
            self.extents[0] + if count == 0 { self.extents[2] } else { 0.0 },
            self.extents[1],
        ]
    }

    fn container(
        &self,
        horizontal: bool,
        count: usize,
        side: usize,
        extent: f64,
        old: Option<Element>,
    ) -> Element {
        let mut container = old.unwrap_or_else(|| {
            if horizontal {
                hstack().into()
            } else {
                vstack().into()
            }
        });
        let mut parked = self.parked.borrow_mut();
        for child in std::mem::take(&mut container.children) {
            if let Some(i) = child
                .attrs
                .get("__jadix_vlist_slot")
                .and_then(|i| i.parse::<usize>().ok())
            {
                parked[i] = Some(child);
            }
        }
        let padding = (extent - self.insets(count)[side]).max(0.0) as f32;
        if side == 0 && padding > 0.0 {
            container
                .children
                .push(main_extent(Element::new(), horizontal, padding));
        }
        for i in if side == 0 { &[0, 2][..] } else { &[1][..] } {
            if *i != 2 || count == 0 {
                if let Some(child) = parked[*i].take() {
                    container.children.push(child);
                }
            }
        }
        if side == 1 && padding > 0.0 {
            container
                .children
                .push(main_extent(Element::new(), horizontal, padding));
        }
        container
            .attrs
            .insert("__jadix_vlist_slot_container".into(), side.to_string());
        main_extent(container, horizontal, extent as f32)
    }
}

// Resolve only mounted interaction owners. The common stable-key path checks
// its last index; measured moves use the axis map. Uniform moves share one
// dataset scan, rather than one scan per retained row on every frame.
fn interaction_retained_rows(
    root: &Element,
    count: usize,
    reuse_rows: bool,
    axis: Option<&VirtualAxis<String>>,
    stable_keys: Option<&[String]>,
) -> Vec<usize> {
    if !reuse_rows && stable_keys.is_none() {
        return Vec::new();
    }
    let focused = use_signal("__jadix_button_focus_id", String::new).peek();
    let mut retained = Vec::new();
    let mut moved_keys = HashSet::new();
    for row in &root.children {
        let Some(key) = row.attrs.get(ROW_KEY_ATTR) else {
            continue;
        };
        if !subtree_needs_virtual_retention(row, &focused) {
            continue;
        }
        if let Some(keys) = stable_keys {
            let Some(stable_key) = key.strip_prefix("k:") else {
                continue;
            };
            let previous = row
                .attrs
                .get(ROW_INDEX_ATTR)
                .and_then(|i| i.parse::<usize>().ok());
            if let Some(index) = previous.filter(|i| keys.get(*i).is_some_and(|k| k == stable_key))
            {
                retained.push(index);
            } else if let Some(axis) = axis {
                if let Some(index) = axis.index(key).filter(|i| *i < count) {
                    retained.push(index);
                }
            } else {
                moved_keys.insert(stable_key);
            }
        } else if let Some(index) = key
            .strip_prefix("i:")
            .and_then(|i| i.parse::<usize>().ok())
            .filter(|i| *i < count)
        {
            retained.push(index);
        }
    }
    if !moved_keys.is_empty() {
        for (index, key) in stable_keys.unwrap().iter().take(count).enumerate() {
            if moved_keys.remove(key.as_str()) {
                retained.push(index);
            }
            if moved_keys.is_empty() {
                break;
            }
        }
    }
    retained
}

/// Incremental window update: reuse still-visible keyed rows; create/drop the rest.
#[expect(
    clippy::too_many_arguments,
    reason = "The existing rendering adapter keeps its independent geometry, state and callback parameters"
)]
fn apply_body_window(
    horizontal: bool,
    root: &mut Element,
    list_id: &str,
    item_count: usize,
    data_key: &str,
    item_height: f32,
    item_gap: f32,
    start: usize,
    end: usize,
    render_item: &dyn Fn(usize) -> Element,
    axis: Option<&VirtualAxis<String>>,
    stable_keys: Option<&[String]>,
    animate_item_changes: Option<LayoutAnimation>,
    current_item_count: &dyn Fn() -> usize,
    snapshot_is_current: &dyn Fn() -> bool,
    sticky: Option<ListStickyHeader>,
    sticky_enabled: bool,
    padding: [f64; 2],
    slots: &ListSlots,
    retained: &[usize],
) -> LayoutStatus {
    let key = window_key(item_count, data_key, start, end);
    let height_key = format!("{item_height}|{}|{}", padding[0], padding[1]);
    let reuse_rows = root
        .attrs
        .get(RANGE_ATTR)
        .and_then(|prev| parse_window_key(prev))
        .map(|(prev_count, _, _, prev_data)| {
            (stable_keys.is_some() || prev_count == item_count) && prev_data == data_key
        })
        .unwrap_or(false);
    let interaction_rows =
        interaction_retained_rows(root, item_count, reuse_rows, axis, stable_keys);
    // Only retain disjoint outliers. Never expand the ordinary range to bridge
    // an overlay host and the viewport: that can mount an entire large dataset.
    let mut outliers: Vec<_> = retained
        .iter()
        .copied()
        .chain(interaction_rows)
        .chain(sticky.map(|pin| pin.index))
        .filter(|i| *i < item_count && !(start..end).contains(i))
        .collect();
    outliers.sort_unstable();
    outliers.dedup();
    let pin_key = if outliers.is_empty() && sticky.is_none() {
        String::new()
    } else {
        format!("{outliers:?}|{:?}", sticky.map(|pin| pin.index))
    };
    let pinned_id =
        sticky.map(|pin| row_id(list_id, pin.index, stable_key_for(pin.index, stable_keys)));
    let translation = sticky.map_or(0.0, |pin| pin.translation as f32);
    if root.attrs.get(RANGE_ATTR).map(String::as_str) == Some(key.as_str())
        && root.attrs.get(HEIGHT_ATTR) == Some(&height_key)
        && root.attrs.get(PIN_ATTR) == Some(&pin_key)
    {
        return apply_sticky_header(root, horizontal, pinned_id.as_deref(), translation);
    }

    let top_tid = top_spacer_id(list_id);
    let bot_tid = bot_spacer_id(list_id);
    let mut old_rows: HashMap<String, Element> = HashMap::new();
    let mut old_top: Option<Element> = None;
    let mut old_bot: Option<Element> = None;
    let gap_prefix = format!("{list_id}__vlist_gap_");
    let mut old_gaps = HashMap::new();
    let mut old_slots = [None, None];
    for child in std::mem::take(&mut root.children) {
        if let Some(side) = child
            .attrs
            .get("__jadix_vlist_slot_container")
            .and_then(|s| s.parse::<usize>().ok())
        {
            old_slots[side] = Some(child);
            continue;
        }
        match child.template_id.as_deref() {
            Some(id) if id == top_tid => old_top = Some(child),
            Some(id) if id == bot_tid => old_bot = Some(child),
            Some(id) if id.starts_with(&gap_prefix) => {
                if let Ok(index) = id[gap_prefix.len()..].parse::<usize>() {
                    old_gaps.insert(index, child);
                }
            }
            Some(id) => {
                if let Some(key) = child
                    .attrs
                    .get(ROW_KEY_ATTR)
                    .cloned()
                    .or_else(|| parse_row_index(list_id, id).map(|index| format!("i:{index}")))
                {
                    old_rows.insert(key, child);
                }
            }
            None => {}
        }
    }

    let [total_h, _, _] = row_geometry(axis, item_count, item_height, item_gap, start, end);
    let position = |i| row_geometry(axis, item_count, item_height, item_gap, i, i)[1];
    let mut reused = 0_u32;
    let mut created = 0_u32;
    let mut next = Vec::with_capacity(
        end.saturating_sub(start)
            .saturating_add(outliers.len().saturating_mul(2))
            .saturating_add(4),
    );
    let split = outliers.partition_point(|i| *i < start);
    let indices = outliers[..split]
        .iter()
        .copied()
        .chain(start..end)
        .chain(outliers[split..].iter().copied());
    let row_padding = if slots.present { [0.0; 2] } else { padding };
    if slots.present {
        next.push(slots.container(horizontal, item_count, 0, padding[0], old_slots[0].take()));
    }
    let mut cursor = 0;
    let snapshot_valid = snapshot_is_current();
    for i in indices {
        if !snapshot_valid || i >= current_item_count() {
            break;
        }
        if cursor < i || (cursor == 0 && row_padding[0] > 0.0) {
            let extent = position(i) - position(cursor)
                + if cursor == 0 {
                    row_padding[0] as f32
                } else {
                    0.0
                };
            let (which, previous) = if cursor == 0 {
                ("top".to_string(), old_top.take())
            } else {
                (format!("gap_{i}"), old_gaps.remove(&i))
            };
            next.push(previous.map_or_else(
                || make_spacer(horizontal, list_id, &which, extent),
                |el| tune_spacer(horizontal, el, extent),
            ));
        }
        cursor = i + 1;
        let axis_key = axis_key(i, stable_keys);
        let stable_key = stable_key_for(i, stable_keys);
        if reuse_rows {
            if let Some(el) = old_rows.remove(&axis_key) {
                reused += 1;
                next.push(tune_row(
                    horizontal,
                    el,
                    list_id,
                    i,
                    stable_key,
                    &axis_key,
                    item_height,
                    if i + 1 < item_count { item_gap } else { 0.0 },
                    axis.is_some(),
                    animate_item_changes,
                ));
                continue;
            }
        }
        let row = render_row(
            horizontal,
            render_item,
            i,
            axis.is_some() || item_gap > 0.0 || sticky_enabled,
        );
        if !snapshot_is_current() {
            break;
        }
        created += 1;
        next.push(tune_row(
            horizontal,
            row,
            list_id,
            i,
            stable_key,
            &axis_key,
            item_height,
            if i + 1 < item_count { item_gap } else { 0.0 },
            axis.is_some(),
            animate_item_changes,
        ));
    }

    let bottom_h = total_h - position(cursor.min(item_count))
        + row_padding[1] as f32
        + if cursor == 0 {
            row_padding[0] as f32
        } else {
            0.0
        };
    if bottom_h > 0.0 {
        let sp = match old_bot.take() {
            Some(el) => tune_spacer(horizontal, el, bottom_h),
            None => make_spacer(horizontal, list_id, "bot", bottom_h),
        };
        next.push(sp);
    }

    if slots.present {
        next.push(slots.container(horizontal, item_count, 1, padding[1], old_slots[1].take()));
    }

    let dropped = old_rows.len() as u32;
    // Remaining `old_rows` / unused spacers drop here (destroy).
    drop(old_rows);
    drop(old_top);
    drop(old_bot);

    apply_body_style(
        horizontal,
        root,
        total_h + padding[0] as f32 + padding[1] as f32,
    );
    root.children = next;
    root.attrs.insert(RANGE_ATTR.to_string(), key);
    root.attrs.insert(HEIGHT_ATTR.to_string(), height_key);
    root.attrs.insert(PIN_ATTR.to_string(), pin_key);
    root.attrs
        .insert(REUSED_ATTR.to_string(), reused.to_string());
    root.attrs
        .insert(CREATED_ATTR.to_string(), created.to_string());
    root.attrs
        .insert(DROPPED_ATTR.to_string(), dropped.to_string());
    apply_sticky_header(root, horizontal, pinned_id.as_deref(), translation);
    LayoutStatus::StructureChanged
}

fn attach_list_viewability(
    element: &mut Element,
    state: Option<Rc<RefCell<(ListViewabilityTracker, Range<usize>)>>>,
    callback: Option<Rc<dyn Fn(ViewableItems)>>,
) {
    if let (Some(state), Some(callback)) = (state, callback) {
        let origin = web_time::Instant::now();
        crate::authoring::deadline::attach(
            element,
            crate::authoring::deadline::Visibility::Visible,
            move |context| {
                let (change, next_deadline) = {
                    let mut state = state.borrow_mut();
                    if context.resumed {
                        state.0.restart();
                    }
                    let candidates = state.1.clone();
                    let change = state
                        .0
                        .update(candidates, context.now.saturating_duration_since(origin));
                    (change, state.0.next_deadline())
                };
                if let Some(change) = change {
                    callback(change);
                }
                crate::authoring::deadline::Poll {
                    redraw: false,
                    next_wake: next_deadline.and_then(|deadline| origin.checked_add(deadline)),
                }
            },
        );
    }
}

impl From<VirtualListBuilder> for Element {
    fn from(builder: VirtualListBuilder) -> Self {
        let VirtualListBuilder {
            id,
            handle,
            item_count,
            item_count_fn,
            focused_index_fn,
            focused_index_align,
            retained_indices_fn,
            data_key_fn,
            item_key_fn,
            maintain_visible_content_position,
            content_width_fn,
            horizontal,
            use_parent_scroll,
            inverted,
            item_height,
            item_gap,
            item_height_fn,
            measure_rows,
            sticky_headers,
            sticky_headers_fn,
            sticky_header_offset,
            sticky_header_offset_fn,
            animate_item_changes,
            on_scroll_position,
            on_visible_range_change,
            on_viewable_items_change,
            viewability,
            minimum_view_time,
            on_load_more,
            on_load_more_at_start,
            load_more_threshold,
            load_more_at_start_threshold,
            overscan,
            scroll_ahead,
            height,
            initial_scroll_at_end,
            content_align,
            follow_end,
            follow_end_threshold,
            scrollbar_visibility,
            scroll_behavior,
            render_item,
            slots,
            chrome,
        } = builder;
        let inverted = inverted && !use_parent_scroll;
        let order = if inverted {
            ListOrder::Reversed
        } else {
            ListOrder::Forward
        };
        let read_retained = move |count| {
            retained_indices_fn
                .as_ref()
                .map(|f| {
                    let mut rows = f();
                    rows.retain_mut(|i| match order.index(*i, count) {
                        Some(index) => {
                            *i = index;
                            true
                        }
                        None => false,
                    });
                    rows
                })
                .unwrap_or_default()
        };
        // Project only at the adapter boundary. The measurement index, mounted
        // window, layout order and native scroll coordinates stay forward.
        let render_count = Rc::new(Cell::new(item_count));
        let render_item = if inverted {
            let count = render_count.clone();
            Rc::new(move |index| render_item(order.index(index, count.get()).unwrap()))
                as Rc<dyn Fn(usize) -> Element>
        } else {
            render_item
        };
        let item_key_fn = if inverted && (item_key_fn.is_some() || measure_rows) {
            let count = render_count.clone();
            Some(Rc::new(move |index| {
                let index = order.index(index, count.get()).unwrap();
                item_key_fn
                    .as_ref()
                    .map_or_else(|| index.to_string(), |key| key(index))
            }) as Rc<dyn Fn(usize) -> String>)
        } else {
            item_key_fn
        };
        let focused_index_fn = focused_index_fn.map(|focus| {
            let count = render_count.clone();
            Rc::new(move || focus().and_then(|index| order.index(index, count.get())))
                as Rc<dyn Fn() -> Option<usize>>
        });
        let sticky_headers = if inverted {
            ListStickyHeaders::default()
        } else {
            sticky_headers
        };
        let sticky_headers_fn = sticky_headers_fn.filter(|_| !inverted);
        let content_align = if use_parent_scroll {
            ListContentAlign::Start
        } else if inverted {
            ListContentAlign::End
        } else {
            content_align
        };
        let initial_scroll_at_end = !use_parent_scroll && (initial_scroll_at_end || inverted);
        let follow_end = follow_end.unwrap_or(inverted);
        let slots = ListSlots::new(slots, horizontal, inverted);
        handle.0.order.set(order);
        let offset_key = if horizontal {
            scroll_offset_x_signal_key
        } else {
            scroll_offset_y_signal_key
        };
        let target_key = if horizontal {
            scroll_target_x_signal_key
        } else {
            scroll_target_y_signal_key
        };
        let viewport_key = if horizontal {
            scroll_viewport_w_signal_key
        } else {
            scroll_viewport_h_signal_key
        };
        let cross_key = if horizontal {
            scroll_viewport_h_signal_key
        } else {
            scroll_viewport_w_signal_key
        };
        let reconcile_end = if horizontal {
            reconcile_scroll_end_x
        } else {
            reconcile_scroll_end_y
        };
        let cancel_end = if horizontal {
            cancel_scroll_end_x
        } else {
            cancel_scroll_end_y
        };
        let request_end = if horizontal {
            request_scroll_end_x
        } else {
            request_scroll_end_y
        };
        let dimension = if horizontal { 2 } else { 3 };
        let content_width_fn = content_width_fn.filter(|_| !horizontal);
        let scroll_id = id.clone();
        let scroll_id_sync = scroll_id.clone();
        let list_id_sync = scroll_id.clone();
        let render_for_sync = Rc::clone(&render_item);
        let item_count_fn_sync = item_count_fn.clone();
        let focused_index_fn_sync = focused_index_fn.clone();
        let data_key_fn_sync = data_key_fn.clone();
        let item_key_fn_sync = item_key_fn.clone();

        let initial_count = item_count_fn.as_ref().map(|f| f()).unwrap_or(item_count);
        render_count.set(initial_count);
        let initial_data_key = data_key_fn.as_ref().map(|f| f()).unwrap_or_default();
        let sticky_enabled = sticky_headers_fn.is_some() || !sticky_headers.indices().is_empty();
        let sticky_headers = RefCell::new(
            sticky_headers_fn
                .as_ref()
                .map(|f| ListStickyHeaders::new(f()))
                .unwrap_or(sticky_headers),
        );
        let read_sticky_offset = move || {
            sticky_header_offset_fn
                .as_ref()
                .map(|f| f())
                .filter(|v| v.is_finite())
                .unwrap_or(sticky_header_offset)
        };
        let initial_stable_keys = item_key_fn
            .as_ref()
            .map(|f| Rc::<[String]>::from(collect_stable_keys(initial_count, f.as_ref())));
        let fallback_item_height = item_height;
        let read_item_height = move || {
            item_height_fn
                .as_ref()
                .map(|f| f())
                .filter(|height| height.is_finite() && *height >= 1.0)
                .unwrap_or(fallback_item_height)
        };
        let item_height = read_item_height();
        let initial_uniform =
            UniformList::new(initial_count, f64::from(item_height), f64::from(item_gap)).unwrap();
        let initial_content = ListContent::new(
            initial_uniform.total(),
            slots.insets(initial_count),
            f64::from(height),
            content_align,
        )
        .unwrap();
        let initial_padding = initial_content.insets();
        handle.0.padding.set(initial_padding);
        let navigation_owner = Rc::new(());
        *handle.0.binding.borrow_mut() = Some((
            crate::prelude::node(id.clone()),
            Rc::downgrade(&navigation_owner),
            crate::core::reactivity::Signal::new(()),
        ));
        handle.0.request.set(None);
        handle.0.sticky_header.set(None);
        let navigation_retry = RefCell::new(None::<(ListScrollRequest, f32, String)>);
        let pending_focus = Cell::new(None);
        handle.0.count.set(initial_count);
        handle.0.item_height.set(item_height);
        handle.0.item_gap.set(item_gap);
        *handle.0.viewport.borrow_mut() = None;
        *handle.0.axis.borrow_mut() = measure_rows.then(|| {
            VirtualAxis::new(
                axis_keys(initial_count, initial_stable_keys.as_deref()),
                f64::from(item_height),
            )
            .and_then(|mut axis| {
                axis.set_gap(f64::from(item_gap))?;
                Ok(axis)
            })
            .expect("measured VirtualList requires finite heights and at most 1,000,000 rows")
        });
        let previous_width = Cell::new(0.0_f32);
        let previous_data_key = RefCell::new(initial_data_key.clone());
        let previous_stable_keys = RefCell::new(initial_stable_keys.clone());
        let previous_item_height = Cell::new(item_height);
        let previous_geometry = Cell::new((initial_count, height));
        let previous_content = Cell::new(initial_content);
        let previous_position = Cell::new(None);
        let previous_visible_range = RefCell::new(None);
        let previous_load_edges = Cell::new(ListLoadEdges::default());
        let scroll_prediction = RefCell::new(ListScrollAhead::default());
        let viewable_state = on_viewable_items_change.as_ref().map(|_| {
            Rc::new(RefCell::new((
                ListViewabilityTracker::new(minimum_view_time),
                0..0,
            )))
        });
        let viewable_state_sync = viewable_state.clone();
        let initial_offset = if initial_scroll_at_end {
            initial_content.max_offset(f64::from(height)).unwrap() as f32
        } else {
            0.0
        };
        if initial_scroll_at_end && (horizontal || inverted || slots.present) {
            handle.0.request.set(Some(ListScrollRequest::End));
        } else if initial_scroll_at_end && initial_count > 0 {
            scroll_item_into_view_when(
                &scroll_id,
                initial_count - 1,
                initial_count,
                "end",
                "before",
            );
        }
        let initial_viewport = initial_content
            .viewport(f64::from(initial_offset), f64::from(height), |o, v| {
                initial_uniform.viewport(o, v)
            })
            .unwrap();
        let start0 = if use_parent_scroll {
            0
        } else {
            initial_viewport.visible.start.saturating_sub(overscan)
        };
        let end0 = if use_parent_scroll {
            0
        } else {
            initial_viewport
                .visible
                .end
                .saturating_add(overscan)
                .min(initial_count)
        };
        let initial_pin = initial_content
            .sticky_header(
                &initial_viewport,
                &sticky_headers.borrow(),
                f64::from(read_sticky_offset()),
                |i| (initial_uniform.position(i).unwrap(), f64::from(item_height)),
            )
            .unwrap();
        let mut body: Element = if horizontal {
            hstack().into()
        } else {
            vstack().into()
        };
        apply_body_window(
            horizontal,
            &mut body,
            &id,
            initial_count,
            &initial_data_key,
            item_height,
            item_gap,
            start0,
            end0,
            render_item.as_ref(),
            handle.0.axis.borrow().as_ref(),
            initial_stable_keys.as_deref(),
            animate_item_changes,
            &|| item_count_fn.as_ref().map_or(item_count, |f| f()),
            &|| {
                item_count_fn.as_ref().map_or(item_count, |f| f()) == initial_count
                    && data_key_fn.as_ref().map_or_else(String::new, |f| f()) == initial_data_key
            },
            initial_pin.filter(|_| !use_parent_scroll),
            sticky_enabled,
            initial_padding,
            &slots,
            &read_retained(initial_count),
        );
        body.attrs.insert(
            RANGE_ATTR.to_string(),
            window_key(initial_count, &initial_data_key, start0, end0),
        );
        body.attrs.insert(REUSED_ATTR.to_string(), "0".into());
        body.attrs.insert(CREATED_ATTR.to_string(), "0".into());
        body.attrs.insert(DROPPED_ATTR.to_string(), "0".into());
        let horizontal_scroll = content_width_fn.is_some();
        if let Some(width) = &content_width_fn {
            body = body.min_w(width().max(0.0));
        }
        if use_parent_scroll {
            let content_key = if horizontal {
                scroll_content_w_signal_key
            } else {
                scroll_content_h_signal_key
            };
            body = stable_window_sync(body, move |root, layout, retry| {
                let _navigation_owner = &navigation_owner;
                let owner = root
                    .layout_node_id()
                    .and_then(|node| layout.ancestor_scroll_viewport(node, horizontal));
                let origin = owner.map_or(0.0, |(_, origin)| origin);
                let scroll_id = owner.map(|(id, _)| id);
                let offset_signal = scroll_id.map(|id| use_signal(&offset_key(id), || 0.0_f32));
                let parent_offset = offset_signal
                    .as_ref()
                    .and_then(|s| s.try_peek())
                    .filter(|v| v.is_finite() && *v >= 0.0)
                    .unwrap_or(0.0);
                let viewport = scroll_id
                    .map(|id| use_signal(&viewport_key(id), || 0.0_f32).peek())
                    .filter(|v| v.is_finite() && *v > 0.0)
                    .unwrap_or(0.0);
                let mut parent =
                    ParentViewport::new(f64::from(parent_offset), f64::from(viewport), origin)
                        .unwrap();
                let old_content = previous_content.get();
                let count = item_count_fn_sync
                    .as_ref()
                    .map(|f| f())
                    .unwrap_or(item_count);
                render_count.set(count);
                let data_key = data_key_fn_sync.as_ref().map(|f| f()).unwrap_or_default();
                let data_changed = *previous_data_key.borrow() != data_key;
                let (old_count, _) = previous_geometry.replace((count, viewport));
                let item_height = read_item_height();
                let old_height = previous_item_height.replace(item_height);
                let uniform =
                    UniformList::new(count, f64::from(item_height), f64::from(item_gap)).unwrap();
                let stable_keys = item_key_fn_sync.as_ref().map(|f| {
                    if count != old_count || data_changed {
                        Rc::<[String]>::from(collect_stable_keys(count, f.as_ref()))
                    } else {
                        previous_stable_keys.borrow().clone().unwrap_or_default()
                    }
                });
                let keys_changed = stable_keys != *previous_stable_keys.borrow();
                if count != old_count || data_changed {
                    *previous_stable_keys.borrow_mut() = stable_keys.clone();
                    *previous_data_key.borrow_mut() = data_key.clone();
                    if let Some(f) = &sticky_headers_fn {
                        *sticky_headers.borrow_mut() = ListStickyHeaders::new(f());
                    }
                }
                handle.0.count.set(count);
                handle.0.item_height.set(item_height);
                // Invoke authored callbacks before borrowing the measurement index.
                let retained = read_retained(count);
                // A one-shot focus callback can run before its scroll owner is measured.
                // Keep that destination until a real viewport can publish the offset.
                if data_changed || count != old_count || handle.0.request.get().is_some() {
                    pending_focus.set(None);
                }
                let focused = focused_index_fn_sync
                    .as_ref()
                    .and_then(|f| f())
                    .or_else(|| pending_focus.take())
                    .filter(|i| *i < count);
                pending_focus.set(focused.filter(|_| viewport <= 0.0));
                let sticky_offset = read_sticky_offset();
                let mut axis = handle.0.axis.borrow_mut();
                let mut compensation = 0.0;
                let mut geometry_changed = false;
                if let Some(axis) = axis.as_mut() {
                    let width = scroll_id
                        .map(|id| use_signal(&cross_key(id), || 0.0_f32).peek())
                        .unwrap_or(0.0);
                    let reset = (data_changed && stable_keys.is_none())
                        || width != previous_width.replace(width)
                        || old_height != item_height;
                    geometry_changed = reset || keys_changed || axis.len() != count;
                    if reset {
                        *axis = VirtualAxis::new(
                            axis_keys(count, stable_keys.as_deref()),
                            f64::from(item_height),
                        )
                        .expect("measured VirtualList requires at most 1,000,000 rows");
                        axis.set_gap(f64::from(item_gap)).unwrap();
                    } else if keys_changed {
                        *axis = axis
                            .replacement(
                                axis_keys(count, stable_keys.as_deref()),
                                f64::from(item_height),
                            )
                            .expect("VirtualList item keys must be unique");
                    } else {
                        // Mounted rows are in natural order. Undo earlier deltas
                        // when testing old row ends, and write the owner only once.
                        let mut measured_delta = 0.0;
                        for row in &root.children {
                            let Some(key) = row.attrs.get(ROW_KEY_ATTR) else {
                                continue;
                            };
                            let Some(index) = axis.index(key) else {
                                continue;
                            };
                            let old = axis.extent(key).unwrap();
                            let measured = f64::from(row.layout_bounds[dimension]);
                            if measured.is_finite()
                                && measured > 0.0
                                && (measured - old).abs() > 0.5
                            {
                                let old_end = old_content.insets()[0]
                                    + axis.position(index).unwrap()
                                    - measured_delta
                                    + old;
                                if parent.offset() > 0.0 && old_end <= parent.offset() {
                                    compensation += measured - old;
                                }
                                measured_delta += measured - old;
                                geometry_changed |= axis.measure(key, measured).unwrap();
                            }
                        }
                        if stable_keys.is_none() && count != axis.len() {
                            if count > axis.len() {
                                axis.append(
                                    (axis.len()..count)
                                        .map(|i| (format!("i:{i}"), f64::from(item_height))),
                                )
                                .expect("measured VirtualList requires at most 1,000,000 rows");
                            } else {
                                axis.replace(
                                    (0..count).map(|i| format!("i:{i}")),
                                    f64::from(item_height),
                                )
                                .unwrap();
                            }
                        }
                    }
                } else if !keys_changed && !data_changed && old_height != item_height {
                    let above = ((parent.offset() - old_content.insets()[0] + f64::from(item_gap))
                        / f64::from(old_height + item_gap))
                    .floor()
                    .max(0.0) as usize;
                    compensation = above.min(old_count).min(count) as f64
                        * f64::from(item_height - old_height);
                }
                let content = ListContent::new(
                    axis.as_ref().map_or(uniform.total(), VirtualAxis::total),
                    slots.insets(count),
                    f64::from(viewport),
                    ListContentAlign::Start,
                )
                .unwrap();
                let delta = content.total() - old_content.total();
                let parent_total = scroll_id
                    .map(|id| use_signal(&content_key(id), || 0.0_f32).peek())
                    .filter(|v| v.is_finite() && *v >= 0.0)
                    .map(f64::from)
                    .unwrap_or(0.0);
                let parent_total = (parent_total + delta)
                    .max(origin + content.total())
                    .max(f64::from(viewport));
                if parent.offset() > 0.0 && old_content.total() <= parent.offset() {
                    compensation = delta;
                }
                let corrected = (parent.parent_offset() + compensation)
                    .clamp(0.0, (parent_total - parent.extent()).max(0.0));
                parent = ParentViewport::new(corrected, parent.extent(), origin).unwrap();
                if geometry_changed {
                    root.attrs.remove(HEIGHT_ATTR);
                }
                previous_content.set(content);
                handle.0.padding.set(content.insets());
                let bounds = |index| {
                    (
                        axis.as_ref().map_or_else(
                            || uniform.position(index).unwrap(),
                            |axis| axis.position(index).unwrap(),
                        ),
                        axis.as_ref().map_or(f64::from(item_height), |axis| {
                            axis.extent(&axis_key(index, stable_keys.as_deref()))
                                .unwrap()
                        }),
                    )
                };
                let query = |parent| {
                    content
                        .viewport_in_parent(parent, |o, v| {
                            axis.as_ref().map_or_else(
                                || uniform.viewport(o, v),
                                |axis| axis.viewport_geometry(o, v),
                            )
                        })
                        .unwrap()
                };
                let request = handle.0.request.take().or_else(|| {
                    navigation_retry
                        .borrow_mut()
                        .take()
                        .and_then(|(request, last, revision)| {
                            (revision == data_key && (parent_offset - last).abs() <= 0.5)
                                .then_some(request)
                        })
                });
                navigation_retry.borrow_mut().take();
                if let Some(request) = request {
                    if viewport > 0.0 {
                        let target = match request {
                            ListScrollRequest::End => parent
                                .scroll_to(
                                    content.total(),
                                    0.0,
                                    parent_total,
                                    ListScrollAlign::End,
                                    0.0,
                                )
                                .ok(),
                            ListScrollRequest::Index {
                                index,
                                align,
                                extra,
                                ..
                            } => (index < count).then(|| {
                                let (start, extent) = bounds(index);
                                parent
                                    .scroll_to(
                                        start + content.insets()[0],
                                        extent,
                                        parent_total,
                                        align,
                                        extra,
                                    )
                                    .unwrap()
                            }),
                        };
                        if let Some(target) = target {
                            if matches!(request, ListScrollRequest::Index { smooth: true, .. }) {
                                if let Some(id) = scroll_id {
                                    cancel_end(id);
                                    use_signal(&target_key(id), || 0.0_f32).set(target as f32);
                                }
                            } else {
                                parent = ParentViewport::new(target, parent.extent(), origin).unwrap();
                            }
                            if let (Some(_), ListScrollRequest::Index { index, .. }) =
                                (axis.as_ref(), request)
                            {
                                let key = axis_key(index, stable_keys.as_deref());
                                let measured = root.children.iter().any(|row| {
                                    row.attrs.get(ROW_KEY_ATTR) == Some(&key)
                                        && row.layout_bounds[dimension].is_finite()
                                        && row.layout_bounds[dimension] > 0.0
                                });
                                if !matches!(request, ListScrollRequest::Index { smooth: true, .. })
                                    && !measured && query(parent).visible.contains(&index) {
                                    *navigation_retry.borrow_mut() =
                                        Some((request, target as f32, data_key.clone()));
                                }
                            }
                        }
                    } else {
                        handle.0.request.set(Some(request));
                    }
                }
                if viewport > 0.0 {
                    if let Some(index) = focused {
                        let align = match focused_index_align {
                            Some("center") => ListScrollAlign::Center,
                            Some("end") => ListScrollAlign::End,
                            Some(_) => ListScrollAlign::Start,
                            None => ListScrollAlign::Nearest,
                        };
                        let (start, extent) = bounds(index);
                        let target = parent
                            .scroll_to(
                                start + content.insets()[0],
                                extent,
                                parent_total,
                                align,
                                0.0,
                            )
                            .unwrap();
                        parent = ParentViewport::new(target, parent.extent(), origin).unwrap();
                    }
                }
                let navigation_changed =
                    viewport > 0.0 && (parent.parent_offset() as f32 != parent_offset);
                if navigation_changed {
                    let id = scroll_id.unwrap();
                    cancel_end(id);
                    offset_signal
                        .as_ref()
                        .unwrap()
                        .set_silent(parent.parent_offset() as f32);
                    use_signal(&target_key(id), || 0.0_f32)
                        .set_silent(parent.parent_offset() as f32);
                }
                let state = query(parent);
                let mut start = if state.visible.is_empty() {
                    state.visible.start
                } else {
                    state.visible.start.saturating_sub(overscan)
                };
                let mut end = if state.visible.is_empty() {
                    start
                } else {
                    state.visible.end.saturating_add(overscan).min(count)
                };
                let mut retained = retained;
                if let Some(index) = focused {
                    retained.push(index);
                }
                if scroll_ahead && !state.visible.is_empty() {
                    let mut prediction = scroll_prediction.borrow_mut();
                    if data_changed
                        || count != old_count
                        || content != old_content
                        || navigation_changed
                        || request.is_some()
                    {
                        prediction.reset();
                    }
                    let lead = prediction
                        .update(parent.parent_offset(), parent.extent())
                        .unwrap();
                    let ahead = query(
                        ParentViewport::new(
                            (parent.parent_offset() + lead).max(0.0),
                            parent.extent(),
                            origin,
                        )
                        .unwrap(),
                    )
                    .visible;
                    if !ahead.is_empty() {
                        start = start.min(ahead.start);
                        end = end.max(ahead.end);
                    }
                } else {
                    scroll_prediction.borrow_mut().reset();
                }
                let viewport_state = (viewport > 0.0).then_some(state);
                let sticky = viewport_state.as_ref().and_then(|state| {
                    content
                        .sticky_header(
                            state,
                            &sticky_headers.borrow(),
                            f64::from(sticky_offset),
                            bounds,
                        )
                        .unwrap()
                });
                if let Some(state) = &viewable_state_sync {
                    let candidates = viewport_state.as_ref().map_or(0..0, |v| {
                        content
                            .viewable_items(v, viewability, |i| {
                                let (start, extent) = bounds(i);
                                (start, start + extent)
                            })
                            .unwrap()
                    });
                    let mut state = state.borrow_mut();
                    if count != old_count || data_changed {
                        state.0.restart();
                    }
                    state.1 = candidates;
                }
                let position = on_scroll_position.as_ref().map(|_| {
                    viewport_state
                        .as_ref()
                        .filter(|v| !v.visible.is_empty())
                        .map(|v| ListScrollPosition {
                            index: v.visible.start,
                            offset: (v.offset - content.insets()[0] - bounds(v.visible.start).0)
                                .max(0.0),
                        })
                });
                let snapshot_is_current = || {
                    item_count_fn_sync.as_ref().map_or(item_count, |f| f()) == count
                        && data_key_fn_sync.as_ref().map_or_else(String::new, |f| f()) == data_key
                };
                let mut status = apply_body_window(
                    horizontal,
                    root,
                    &list_id_sync,
                    count,
                    &data_key,
                    item_height,
                    item_gap,
                    start,
                    end,
                    render_for_sync.as_ref(),
                    axis.as_ref(),
                    stable_keys.as_deref(),
                    animate_item_changes,
                    &|| {
                        item_count_fn_sync
                            .as_ref()
                            .map(|f| f())
                            .unwrap_or(item_count)
                    },
                    &snapshot_is_current,
                    sticky,
                    sticky_enabled,
                    content.insets(),
                    &slots,
                    &retained,
                );
                if navigation_changed && status == LayoutStatus::Clean {
                    status = LayoutStatus::LayoutChanged;
                }
                if let Some(width) = &content_width_fn {
                    let previous = root.style.min_size.width;
                    *root = std::mem::take(root).min_w(width().max(0.0));
                    if root.style.min_size.width != previous && status == LayoutStatus::Clean {
                        status = LayoutStatus::LayoutChanged;
                    }
                }
                drop(axis);
                if !snapshot_is_current() {
                    root.attrs.remove(RANGE_ATTR);
                    retry.set(true);
                    return status;
                }
                *handle.0.viewport.borrow_mut() = viewport_state.clone();
                handle.0.sticky_header.set(sticky);
                let visible_range = viewport_state.as_ref().map(|v| v.visible.clone());
                let range_changed =
                    previous_visible_range.borrow().as_ref() != Some(&visible_range);
                if range_changed {
                    *previous_visible_range.borrow_mut() = Some(visible_range.clone());
                }
                let near = if viewport > 0.0 {
                    parent
                        .load_edges(
                            content.total(),
                            [
                                load_more_at_start_threshold.unwrap_or(load_more_threshold),
                                load_more_threshold,
                            ],
                            count > 0,
                            ListOrder::Forward,
                        )
                        .unwrap()
                } else {
                    ListLoadEdges::default()
                };
                let load = near.entered(
                    previous_load_edges.replace(near),
                    count != old_count || data_changed,
                );
                if let (Some(callback), Some(position)) = (&on_scroll_position, position) {
                    if previous_position.replace(Some(position)) != Some(position) {
                        callback(position);
                    }
                }
                if range_changed {
                    if let Some(callback) = &on_visible_range_change {
                        callback(visible_range);
                    }
                }
                if load.start {
                    if let Some(callback) = &on_load_more_at_start {
                        callback();
                    }
                }
                if load.end {
                    if let Some(callback) = &on_load_more {
                        callback();
                    }
                }
                status
            });
            let mut element = chrome.apply(body.template_id(&id).flex_shrink(0.0));
            attach_list_viewability(&mut element, viewable_state, on_viewable_items_change);
            return element;
        }
        body = stable_window_sync(body, move |root, _layout, retry| {
            let _navigation_owner = &navigation_owner;
            let sy_sig = use_signal(&offset_key(&scroll_id_sync), || 0.0_f32);
            let vh_sig = use_signal(&viewport_key(&scroll_id_sync), || height);
            let mut sy = sy_sig.try_peek().unwrap_or(0.0);
            let navigation_offset = sy;
            let mut navigation_changed = false;
            let vh_raw = vh_sig.try_peek().unwrap_or(height);
            let vh = if vh_raw.is_finite() && vh_raw > 0.0 {
                vh_raw
            } else {
                height
            };
            let count = item_count_fn_sync
                .as_ref()
                .map(|f| f())
                .unwrap_or(item_count);
            render_count.set(count);
            let data_key = data_key_fn_sync.as_ref().map(|f| f()).unwrap_or_default();
            let sticky_offset = read_sticky_offset();
            let item_height = read_item_height();
            let old_height = previous_item_height.replace(item_height);
            let uniform =
                UniformList::new(count, f64::from(item_height), f64::from(item_gap)).unwrap();
            let (old_count, old_viewport) = previous_geometry.replace((count, vh));
            let data_changed = *previous_data_key.borrow() != data_key;
            if count != old_count || data_changed {
                if let Some(f) = &sticky_headers_fn {
                    *sticky_headers.borrow_mut() = ListStickyHeaders::new(f());
                }
            }
            let stable_keys = item_key_fn_sync.as_ref().map(|f| {
                if count != old_count || data_changed {
                    Rc::<[String]>::from(collect_stable_keys(count, f.as_ref()))
                } else {
                    previous_stable_keys.borrow().clone().unwrap_or_default()
                }
            });
            let keys_changed = (count != old_count || data_changed)
                && stable_keys
                    .as_ref()
                    .is_some_and(|keys| previous_stable_keys.borrow().as_ref() != Some(keys));
            let old_keys = previous_stable_keys.borrow().clone();
            if count != old_count || data_changed {
                *previous_stable_keys.borrow_mut() = stable_keys.clone();
                *previous_data_key.borrow_mut() = data_key.clone();
            }
            let old_content = previous_content.get();
            let mut layout_rebase_y = 0.0_f32;
            handle.0.count.set(count);
            handle.0.item_height.set(item_height);
            let mut axis = handle.0.axis.borrow_mut();
            let keyed_position = if keys_changed && maintain_visible_content_position {
                let offset = if sy.is_finite() {
                    f64::from(sy.max(0.0))
                } else {
                    0.0
                };
                let offset = old_content.anchor_offset(offset).unwrap();
                let position = if let Some(axis) = axis.as_ref() {
                    offset
                        .and_then(|offset| axis.anchor(offset).unwrap())
                        .map(|anchor| ListScrollPosition {
                            index: axis.index(&anchor.key).unwrap(),
                            offset: anchor.within,
                        })
                } else {
                    UniformList::new(old_count, f64::from(old_height), f64::from(item_gap))
                        .unwrap()
                        .anchor(offset.unwrap_or(0.0))
                        .unwrap()
                        .filter(|_| offset.is_some())
                };
                let indices: HashMap<_, _> = stable_keys
                    .as_deref()
                    .unwrap()
                    .iter()
                    .enumerate()
                    .map(|(index, key)| (key, index))
                    .collect();
                position.and_then(|position| {
                    position
                        .remap_keys(old_keys.as_deref().unwrap_or_default(), |key| {
                            indices.get(key).copied()
                        })
                        .unwrap()
                })
            } else {
                None
            };
            if let Some(axis) = axis.as_mut() {
                // Capture once before the batch: clamping after each row would
                // lose the original anchor when multiple rows change together.
                let raw_sy = sy;
                sy = if sy.is_finite() { sy.max(0.0) } else { 0.0 };
                let old_sy = sy;
                let anchor = old_content
                    .anchor_offset(f64::from(sy))
                    .unwrap()
                    .and_then(|offset| axis.anchor(offset).unwrap());
                let at_end = follow_end
                    && sy
                        >= (old_content.max_offset(f64::from(old_viewport)).unwrap() as f32
                            - follow_end_threshold)
                            .max(0.0);
                let width = use_signal(&cross_key(&scroll_id_sync), || 0.0_f32).peek();
                let old_width = previous_width.replace(width);
                let reset = (data_changed && stable_keys.is_none())
                    || width != old_width
                    || item_height != old_height;
                let mut changed = reset || axis.len() != count || keys_changed;
                if reset {
                    let keys = axis_keys(count, stable_keys.as_deref());
                    *axis = VirtualAxis::new(keys, f64::from(item_height))
                        .expect("measured VirtualList requires at most 1,000,000 rows");
                    axis.set_gap(f64::from(item_gap)).unwrap();
                } else if keys_changed {
                    let keys = axis_keys(count, stable_keys.as_deref());
                    *axis = axis
                        .replacement(keys, f64::from(item_height))
                        .expect("VirtualList item keys must be unique");
                } else {
                    for child in &root.children {
                        let Some(key) = child.attrs.get(ROW_KEY_ATTR) else {
                            continue;
                        };
                        let measured = child.layout_bounds[dimension];
                        if measured.is_finite()
                            && measured > 0.0
                            && axis
                                .extent(key)
                                .is_some_and(|old| (old - f64::from(measured)).abs() > 0.5)
                        {
                            changed |= axis.measure(key, f64::from(measured)).unwrap();
                        }
                    }
                    if stable_keys.is_none() {
                        if count > axis.len() {
                            axis.append(
                                (axis.len()..count)
                                    .map(|index| (format!("i:{index}"), f64::from(item_height))),
                            )
                            .expect("measured VirtualList requires at most 1,000,000 rows");
                        } else if count < axis.len() {
                            axis.replace(
                                (0..count).map(|index| format!("i:{index}")),
                                f64::from(item_height),
                            )
                            .unwrap();
                        }
                    }
                }
                let content = ListContent::new(
                    axis.total(),
                    slots.insets(count),
                    f64::from(vh),
                    content_align,
                )
                .unwrap();
                let max = content.max_offset(f64::from(vh)).unwrap();
                if changed || vh != old_viewport || sy != raw_sy || f64::from(sy) > max {
                    let position = if keys_changed {
                        keyed_position
                    } else {
                        anchor.as_ref().and_then(|anchor| {
                            axis.index(&anchor.key).map(|index| ListScrollPosition {
                                index,
                                offset: anchor.within,
                            })
                        })
                    };
                    let restored = position.and_then(|position| {
                        content
                            .restore(
                                axis.position(position.index)?,
                                axis.position(position.index + 1)?,
                                position.offset,
                                f64::from(vh),
                            )
                            .ok()
                    });
                    let end_offset = reconcile_end(
                        &scroll_id_sync,
                        max as f32,
                        restored.unwrap_or(f64::from(sy).min(max)) as f32,
                    );
                    sy = if let Some(offset) = end_offset {
                        f64::from(offset)
                    } else if at_end {
                        max
                    } else {
                        restored.unwrap_or(f64::from(sy).min(max))
                    } as f32;
                    if !at_end && end_offset.is_none() {
                        if let Some(ListScrollPosition { index, .. }) = position {
                            // Preserve the half-open row boundary after f64 -> f32.
                            sy = sy
                                .min(
                                    ((axis.position(index + 1).unwrap() + content.insets()[0])
                                        as f32)
                                        .next_down(),
                                )
                                .max(0.0);
                        }
                    }
                    sy_sig.set_silent(sy);
                    if end_offset.is_none() {
                        use_signal(&target_key(&scroll_id_sync), || 0.0_f32).set_silent(sy);
                    }
                    layout_rebase_y = sy - old_sy;
                    root.attrs.remove(HEIGHT_ATTR);
                }
            } else {
                let old_sy = sy;
                let content = ListContent::new(
                    uniform.total(),
                    slots.insets(count),
                    f64::from(vh),
                    content_align,
                )
                .unwrap();
                let max_offset = content.max_offset(f64::from(vh)).unwrap();
                let old_max_offset =
                    old_content.max_offset(f64::from(old_viewport)).unwrap() as f32;
                if let Some(offset) = reconcile_end(&scroll_id_sync, max_offset as f32, sy) {
                    sy = offset;
                } else if follow_end
                    && sy >= (old_max_offset - follow_end_threshold).max(0.0)
                    && (count != old_count || item_height != old_height || vh != old_viewport)
                {
                    sy = max_offset as f32;
                    sy_sig.set_silent(sy);
                    use_signal(&target_key(&scroll_id_sync), || 0.0_f32).set_silent(sy);
                } else if inverted
                    && keyed_position.is_none()
                    && (count != old_count || item_height != old_height || vh != old_viewport)
                {
                    sy = (max_offset as f32 - (old_max_offset - sy).max(0.0))
                        .clamp(0.0, max_offset as f32);
                    sy_sig.set_silent(sy);
                    use_signal(&target_key(&scroll_id_sync), || 0.0_f32).set_silent(sy);
                } else if keyed_position.is_some() || item_height != old_height {
                    let position = keyed_position.or_else(|| {
                        if keys_changed {
                            return None;
                        }
                        UniformList::new(old_count, f64::from(old_height), f64::from(item_gap))
                            .unwrap()
                            .anchor(
                                old_content
                                    .anchor_offset(f64::from(sy.max(0.0)))
                                    .unwrap()
                                    .unwrap_or(0.0),
                            )
                            .unwrap()
                            .filter(|_| {
                                old_content
                                    .anchor_offset(f64::from(sy.max(0.0)))
                                    .unwrap()
                                    .is_some()
                            })
                            .filter(|position| position.index < count)
                    });
                    sy = position
                        .and_then(|position| {
                            content
                                .restore(
                                    uniform.position(position.index)?,
                                    uniform.position(position.index + 1)?,
                                    position.offset,
                                    f64::from(vh),
                                )
                                .ok()
                        })
                        .unwrap_or(f64::from(sy).min(max_offset)) as f32;
                    if let Some(position) = position {
                        sy = sy
                            .min(
                                ((uniform.position(position.index + 1).unwrap()
                                    + content.insets()[0]) as f32)
                                    .next_down(),
                            )
                            .max(0.0);
                    }
                    sy_sig.set_silent(sy);
                    use_signal(&target_key(&scroll_id_sync), || 0.0_f32).set_silent(sy);
                } else if f64::from(sy) > max_offset {
                    // Remount the real end immediately after data shrinkage, before
                    // the renderer publishes its next clamped offset.
                    sy = max_offset as f32;
                    sy_sig.set_silent(sy);
                    use_signal(&target_key(&scroll_id_sync), || 0.0_f32).set_silent(sy);
                }
                layout_rebase_y = sy - old_sy;
            }
            let content = ListContent::new(
                axis.as_ref().map_or(uniform.total(), VirtualAxis::total),
                slots.insets(count),
                f64::from(vh),
                content_align,
            )
            .unwrap();
            previous_content.set(content);
            handle.0.padding.set(content.insets());
            drop(axis);
            let request = handle.0.request.take().or_else(|| {
                navigation_retry
                    .borrow_mut()
                    .take()
                    .and_then(|(request, last, revision)| {
                        (revision == data_key && (navigation_offset - last).abs() <= 0.5)
                            .then_some(request)
                    })
            });
            // A newer request always discards any measurement retry.
            navigation_retry.borrow_mut().take();
            if let Some(request) = request {
                if vh_raw.is_finite() && vh_raw > 0.0 {
                    let axis = handle.0.axis.borrow();
                    let destination = match request {
                        ListScrollRequest::End => {
                            Some(content.max_offset(f64::from(vh_raw)).unwrap())
                        }
                        ListScrollRequest::Index {
                            index,
                            align,
                            extra,
                            ..
                        } => order.index(index, count).and_then(|index| {
                            let start = axis.as_ref().map_or_else(
                                || uniform.position(index).unwrap(),
                                |axis| axis.position(index).unwrap(),
                            );
                            let extent = axis.as_ref().map_or(f64::from(item_height), |axis| {
                                axis.extent(&axis_key(index, stable_keys.as_deref()))
                                    .unwrap()
                            });
                            content
                                .scroll_to(
                                    start,
                                    extent,
                                    f64::from(sy.max(0.0)),
                                    f64::from(vh_raw),
                                    align,
                                    extra,
                                )
                                .ok()
                        }),
                    };
                    if let Some(destination) = destination {
                        cancel_end(&scroll_id_sync);
                        let smooth = matches!(request, ListScrollRequest::Index { smooth: true, .. });
                        if smooth {
                            use_signal(&target_key(&scroll_id_sync), || 0.0_f32).set(destination as f32);
                        } else {
                            navigation_changed = sy != destination as f32;
                            sy = destination as f32;
                            sy_sig.set_silent(sy);
                            use_signal(&target_key(&scroll_id_sync), || 0.0_f32).set_silent(sy);
                        }
                        if matches!(request, ListScrollRequest::End) {
                            request_end(&scroll_id_sync, sy);
                        }
                        if let (Some(axis), ListScrollRequest::Index { index, .. }) =
                            (axis.as_ref(), request)
                        {
                            let index = order.index(index, count).unwrap();
                            let key = axis_key(index, stable_keys.as_deref());
                            let measured = root.children.iter().any(|row| {
                                row.attrs.get(ROW_KEY_ATTR) == Some(&key)
                                    && row.layout_bounds[dimension].is_finite()
                                    && row.layout_bounds[dimension] > 0.0
                            });
                            // Wait only if the destination row will actually be mounted.
                            let visible = content
                                .viewport(destination, f64::from(vh_raw), |o, v| {
                                    axis.viewport_geometry(o, v)
                                })
                                .unwrap()
                                .visible;
                            if !smooth && !measured && visible.contains(&index) {
                                *navigation_retry.borrow_mut() =
                                    Some((request, sy, data_key.clone()));
                            }
                        }
                    }
                } else {
                    // Hidden/unmeasured viewports consume no navigation request.
                    handle.0.request.set(Some(request));
                }
            }
            if data_changed || count != old_count || request.is_some() {
                pending_focus.set(None);
            }
            let focused = focused_index_fn_sync
                .as_ref()
                .and_then(|f| f())
                .or_else(|| pending_focus.take())
                .filter(|index| *index < count);
            pending_focus.set(focused.filter(|_| !vh_raw.is_finite() || vh_raw <= 0.0));
            let retained = read_retained(count);
            let axis = handle.0.axis.borrow();
            let bounds = |index| {
                let start = axis.as_ref().map_or_else(
                    || uniform.position(index).unwrap(),
                    |axis| axis.position(index).unwrap(),
                );
                let extent = axis.as_ref().map_or(f64::from(item_height), |axis| {
                    axis.extent(&axis_key(index, stable_keys.as_deref()))
                        .unwrap()
                });
                (start, extent)
            };
            let query = |offset, extent| {
                content
                    .viewport(offset, extent, |o, v| {
                        if let Some(axis) = axis.as_ref() {
                            axis.viewport_geometry(o, v)
                        } else {
                            uniform.viewport(o, v)
                        }
                    })
                    .unwrap()
            };
            if let Some(index) = focused {
                let align = match focused_index_align {
                    Some("center") => Some(ListScrollAlign::Center),
                    Some("end") => Some(ListScrollAlign::End),
                    Some(_) => Some(ListScrollAlign::Start),
                    None if axis.is_none() && !horizontal && !inverted && item_gap == 0.0 => {
                        let visible = query(f64::from(sy.max(0.0)), f64::from(vh)).visible;
                        if index < visible.start {
                            Some(ListScrollAlign::End)
                        } else if index >= visible.end {
                            Some(ListScrollAlign::Start)
                        } else {
                            None
                        }
                    }
                    None => Some(ListScrollAlign::Nearest),
                };
                let (start, extent) = bounds(index);
                let aligned = align.map_or(sy, |align| {
                    content
                        .scroll_to(
                            start,
                            extent,
                            f64::from(sy.max(0.0)),
                            f64::from(vh),
                            align,
                            0.0,
                        )
                        .unwrap() as f32
                });
                if aligned != sy {
                    cancel_end(&scroll_id_sync);
                    navigation_changed = true;
                    sy = aligned;
                    // Even before a viewport is published, mount only the focus
                    // neighborhood. Defer the actual scroll until it is visible.
                    if vh_raw.is_finite() && vh_raw > 0.0 {
                        sy_sig.set_silent(sy);
                        use_signal(&target_key(&scroll_id_sync), || 0.0_f32).set_silent(sy);
                    }
                }
            }
            let state = query(
                if sy.is_finite() {
                    f64::from(sy.max(0.0))
                } else {
                    0.0
                },
                f64::from(vh),
            );
            let mut start = state.visible.start.saturating_sub(overscan);
            let mut end = state.visible.end.saturating_add(overscan).min(count);
            if let Some(index) = focused {
                start = start.min(index.saturating_sub(1));
                end = end.max(index.saturating_add(2).min(count));
            }
            if scroll_ahead {
                let mut prediction = scroll_prediction.borrow_mut();
                if data_changed
                    || count != old_count
                    || item_height != old_height
                    || content != old_content
                    || sy != navigation_offset
                    || request.is_some()
                    || !vh_raw.is_finite()
                    || vh_raw <= 0.0
                    || !sy.is_finite()
                {
                    prediction.reset();
                }
                if vh_raw.is_finite() && vh_raw > 0.0 && sy.is_finite() {
                    let lead = prediction
                        .update(state.offset, state.viewport_extent)
                        .unwrap();
                    if lead != 0.0 {
                        let first = (state.offset + lead.min(0.0)).max(0.0);
                        let last = (state.offset + state.viewport_extent + lead.max(0.0))
                            .min(state.total_extent);
                        let ahead = query(first, (last - first).max(0.0)).visible;
                        start = start.min(ahead.start);
                        end = end.max(ahead.end);
                    }
                }
            }
            let padding = content.insets();
            let mut viewport =
                (vh_raw.is_finite() && vh_raw > 0.0 && sy.is_finite()).then_some(state);
            let sticky = viewport.as_ref().and_then(|state| {
                content
                    .sticky_header(
                        state,
                        &sticky_headers.borrow(),
                        f64::from(sticky_offset),
                        bounds,
                    )
                    .unwrap()
            });
            if let Some(state) = &viewable_state_sync {
                let candidates = viewport.as_ref().map_or(0..0, |v| {
                    content
                        .viewable_items(v, viewability, |i| {
                            let (start, extent) = bounds(i);
                            (start, start + extent)
                        })
                        .unwrap()
                });
                let mut state = state.borrow_mut();
                if count != old_count || data_changed {
                    state.0.restart();
                }
                state.1 = order.range(candidates, count).unwrap();
            }
            let position = on_scroll_position.as_ref().map(|_| {
                viewport
                    .as_ref()
                    .filter(|v| !v.visible.is_empty())
                    .map(|v| {
                        let index = v.visible.start;
                        ListScrollPosition {
                            index: order.index(index, count).unwrap(),
                            offset: (v.offset - padding[0] - bounds(index).0).max(0.0),
                        }
                    })
            });
            if let Some(viewport) = &mut viewport {
                viewport.visible = order.range(viewport.visible.clone(), count).unwrap();
            }
            if layout_rebase_y != 0.0 && animate_item_changes.is_some() && stable_keys.is_some() {
                for row in &root.children {
                    if row.attrs.contains_key(ROW_KEY_ATTR) {
                        if let Some(motion) = row.layout_motion_handle() {
                            motion.rebase_position(if horizontal {
                                [layout_rebase_y, 0.0]
                            } else {
                                [0.0, layout_rebase_y]
                            });
                        }
                    }
                }
            }
            let snapshot_is_current = || {
                item_count_fn_sync.as_ref().map_or(item_count, |f| f()) == count
                    && data_key_fn_sync.as_ref().map_or_else(String::new, |f| f()) == data_key
            };
            let mut status = apply_body_window(
                horizontal,
                root,
                &list_id_sync,
                count,
                &data_key,
                item_height,
                item_gap,
                start,
                end,
                render_for_sync.as_ref(),
                axis.as_ref(),
                stable_keys.as_deref(),
                animate_item_changes,
                &|| {
                    item_count_fn_sync
                        .as_ref()
                        .map(|count| count())
                        .unwrap_or(item_count)
                },
                &snapshot_is_current,
                sticky,
                sticky_enabled,
                padding,
                &slots,
                &retained,
            );
            if navigation_changed && status == LayoutStatus::Clean {
                status = LayoutStatus::LayoutChanged;
            }
            if let Some(width) = &content_width_fn {
                let width = width().max(0.0);
                let previous = root.style.min_size.width;
                *root = std::mem::take(root).min_w(width);
                if root.style.min_size.width != previous && status == LayoutStatus::Clean {
                    status = LayoutStatus::LayoutChanged;
                }
            }
            drop(axis);
            if !snapshot_is_current() {
                root.attrs.remove(RANGE_ATTR);
                retry.set(true);
                return status;
            }
            *handle.0.viewport.borrow_mut() = viewport.clone();
            handle.0.sticky_header.set(sticky);
            let visible_range = viewport.as_ref().map(|v| v.visible.clone());
            let range_changed = previous_visible_range.borrow().as_ref() != Some(&visible_range);
            if range_changed {
                *previous_visible_range.borrow_mut() = Some(visible_range.clone());
            }
            let near = viewport
                .as_ref()
                .and_then(|v| {
                    ListLoadEdges::new(
                        v.offset,
                        v.viewport_extent,
                        v.total_extent,
                        [
                            load_more_at_start_threshold.unwrap_or(load_more_threshold),
                            load_more_threshold,
                        ],
                        v.item_count > 0,
                        order,
                    )
                    .ok()
                })
                .unwrap_or_default();
            let data_updated = count != old_count || data_changed;
            let load = near.entered(previous_load_edges.replace(near), data_updated);
            if let (Some(callback), Some(position)) = (&on_scroll_position, position) {
                if previous_position.replace(Some(position)) != Some(position) {
                    callback(position);
                }
            }
            if range_changed {
                if let Some(callback) = &on_visible_range_change {
                    callback(visible_range);
                }
            }
            if load.start {
                if let Some(callback) = &on_load_more_at_start {
                    callback();
                }
            }
            if load.end {
                if let Some(callback) = &on_load_more {
                    callback();
                }
            }
            status
        });
        let mut element = chrome.apply(
            scroll_area(&scroll_id)
                .overflow_y(if horizontal { "hidden" } else { "auto" })
                .overflow_x(if horizontal || horizontal_scroll {
                    "auto"
                } else {
                    "hidden"
                })
                .scroll_behavior(scroll_behavior)
                .scrollbar_visibility(scrollbar_visibility)
                .w("100%")
                .h(height)
                .child(body)
                .into(),
        );
        attach_list_viewability(&mut element, viewable_state, on_viewable_items_change);
        element
    }
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/virtual_list.rs"]
mod tests;
