//! # VirtualList
//!
//! 虚拟列表：顶部/底部 spacer + 仅渲染可见行，可选实际行高测量。
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
use std::rc::Rc;

use crate::components::base::SlotChrome;
use crate::components::scroll_area::{scroll_area, scroll_item_into_view_when};
use crate::core::reactivity::use_signal;
use crate::core::scroll_viewport::scroll_target_y_signal_key;
use crate::core::scroll_viewport::scroll_viewport_w_signal_key;
use crate::core::scroll_viewport::{scroll_offset_y_signal_key, scroll_viewport_h_signal_key};
use crate::core::tree::{Element, LayoutStatus};
use crate::motion::{LayoutAnimation, LayoutMotion};
use crate::primitives::r#box::{r#box, vstack};
use crate::utils::virtual_list::{item_height_for_text, visible_item_range};
pub use crate::virtualization::ListScrollPosition;
use crate::virtualization::VirtualAxis;

const RANGE_ATTR: &str = "__jadix_vlist_range";
const REUSED_ATTR: &str = "__jadix_vlist_reused";
const CREATED_ATTR: &str = "__jadix_vlist_created";
const DROPPED_ATTR: &str = "__jadix_vlist_dropped";
const HEIGHT_ATTR: &str = "__jadix_vlist_item_height";
const ROW_KEY_ATTR: &str = "__jadix_vlist_row_key";

type PositionCallback = Rc<dyn Fn(Option<ListScrollPosition>)>;

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
    item_count: usize,
    item_count_fn: Option<Rc<dyn Fn() -> usize>>,
    focused_index_fn: Option<Rc<dyn Fn() -> Option<usize>>>,
    data_key_fn: Option<Rc<dyn Fn() -> String>>,
    item_key_fn: Option<Rc<dyn Fn(usize) -> String>>,
    content_width_fn: Option<Rc<dyn Fn() -> f32>>,
    item_height: f32,
    item_height_fn: Option<Rc<dyn Fn() -> f32>>,
    measure_rows: bool,
    animate_item_changes: Option<LayoutAnimation>,
    on_scroll_position: Option<PositionCallback>,
    overscan: usize,
    height: f32,
    initial_scroll_at_end: bool,
    follow_end: bool,
    scrollbar_visibility: String,
    scroll_behavior: String,
    render_item: Rc<dyn Fn(usize) -> Element>,
    chrome: SlotChrome,
}

pub fn virtual_list(id: impl Into<String>) -> VirtualListBuilder {
    VirtualListBuilder {
        id: id.into(),
        item_count: 0,
        item_count_fn: None,
        focused_index_fn: None,
        data_key_fn: None,
        item_key_fn: None,
        content_width_fn: None,
        item_height: 32.0,
        item_height_fn: None,
        measure_rows: false,
        animate_item_changes: None,
        on_scroll_position: None,
        overscan: 2,
        height: 240.0,
        initial_scroll_at_end: false,
        follow_end: false,
        scrollbar_visibility: "auto".to_string(),
        scroll_behavior: crate::core::scroll_viewport::DEFAULT_SCROLL_BEHAVIOR.to_string(),
        render_item: Rc::new(|i| Element::new().text(format!("item {i}"))),
        chrome: SlotChrome::default(),
    }
}

impl VirtualListBuilder {
    crate::impl_slot_chrome_methods!();
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
    pub fn focused_index_fn(mut self, f: impl Fn() -> Option<usize> + 'static) -> Self {
        self.focused_index_fn = Some(Rc::new(f));
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

    /// Poll a uniform row height each frame. Size changes preserve the first
    /// visible row and its pixel depth (clamped to the new height), then snap
    /// offset and animation target together. Invalid heights use `item_height`.
    pub fn item_height_fn(mut self, height: impl Fn() -> f32 + 'static) -> Self {
        self.item_height_fn = Some(Rc::new(height));
        self
    }

    /// Measure mounted rows after layout, using `item_height` as the estimate
    /// for unmeasured rows. Later size changes preserve the viewport's row and
    /// depth; `follow_end` also applies to measurement updates. Width and data
    /// key changes invalidate measurements. This mode keeps an index for up to
    /// 1,000,000 rows; the default fixed-height mode needs no such index.
    pub fn measure_rows(mut self) -> Self {
        self.measure_rows = true;
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

    /// Enables horizontal scrolling with a minimum body width polled each frame.
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

    /// Keep the viewport at the end when rows or viewport geometry change,
    /// provided the user was already at the end. Scrolling away suspends
    /// following until the user returns. This does not change the initial position.
    pub fn follow_end(mut self, follow: bool) -> Self {
        self.follow_end = follow;
        self
    }

    /// Scrollbar visibility (`auto` / `always` / `scroll` / …). Prefer `always` in demos.
    pub fn scrollbar_visibility(mut self, v: impl Into<String>) -> Self {
        self.scrollbar_visibility = v.into();
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

fn make_spacer(list_id: &str, which: &str, height: f32) -> Element {
    let tid = if which == "top" {
        top_spacer_id(list_id)
    } else {
        bot_spacer_id(list_id)
    };
    Element::from(r#box().w("100%").h(height).min_h(height).flex_shrink(0.0)).template_id(tid)
}

fn focused_item_window(
    offset: f32,
    viewport: f32,
    item_height: f32,
    item_count: usize,
    overscan: usize,
    focused: Option<usize>,
) -> (usize, usize, Option<&'static str>) {
    let focused = focused.filter(|index| *index < item_count);
    let mut window_offset = offset;
    let mut reveal_align = None;
    if let Some(index) = focused {
        let (visible_start, visible_end) =
            visible_item_range(offset, viewport, item_height, item_count, 0);
        if index < visible_start {
            reveal_align = Some("end");
            window_offset = (index as f32 * item_height + item_height - viewport).max(0.0);
        } else if index >= visible_end {
            reveal_align = Some("start");
            window_offset = (index as f32 * item_height)
                .min((item_count as f32 * item_height - viewport).max(0.0));
        }
    }
    let (mut start, mut end) =
        visible_item_range(window_offset, viewport, item_height, item_count, overscan);
    if let Some(index) = focused {
        start = start.min(index.saturating_sub(1));
        end = end.max(index.saturating_add(2).min(item_count));
    }
    (start, end, reveal_align)
}

fn tune_spacer(mut el: Element, height: f32) -> Element {
    el = el.h(height).min_h(height).flex_shrink(0.0);
    el
}

fn tune_row(
    mut el: Element,
    list_id: &str,
    index: usize,
    stable_key: Option<&str>,
    axis_key: &str,
    item_height: f32,
    measured: bool,
    animate_item_changes: Option<LayoutAnimation>,
) -> Element {
    if measured {
        // The wrapper includes authored child margins in its measured height.
        // Reused wrappers retain their layout and native child identities.
        el = el
            .template_id(row_id(list_id, index, stable_key))
            .w("100%")
            .flex_shrink(0.0);
        el.attrs
            .insert(ROW_KEY_ATTR.to_string(), axis_key.to_string());
    } else {
        el = el
            .template_id(row_id(list_id, index, stable_key))
            .h(item_height)
            .min_h(item_height)
            .max_h(item_height)
            .w("100%")
            .flex_shrink(0.0);
        el.attrs
            .insert(ROW_KEY_ATTR.to_string(), axis_key.to_string());
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
    start: usize,
    end: usize,
) -> [f32; 3] {
    match axis {
        Some(axis) => [
            axis.total() as f32,
            axis.position(start).unwrap() as f32,
            (axis.total() - axis.position(end).unwrap()).max(0.0) as f32,
        ],
        None => [
            count as f32 * height,
            start as f32 * height,
            count.saturating_sub(end) as f32 * height,
        ],
    }
}

fn render_row(render: &dyn Fn(usize) -> Element, index: usize, measured: bool) -> Element {
    let child = render(index);
    if measured {
        vstack().child(child).into()
    } else {
        child
    }
}

fn apply_body_style(root: &mut Element, total_h: f32) {
    // Keep layout_node; only refresh size so taffy can reuse the body node.
    *root = std::mem::take(root)
        .w("100%")
        .h(total_h)
        .min_h(total_h)
        .flex_shrink(0.0);
}

fn build_body(
    list_id: &str,
    item_count: usize,
    item_height: f32,
    start: usize,
    end: usize,
    render_item: &dyn Fn(usize) -> Element,
    axis: Option<&VirtualAxis<String>>,
    stable_keys: Option<&[String]>,
    animate_item_changes: Option<LayoutAnimation>,
) -> Element {
    let [total_h, top_h, bottom_h] = row_geometry(axis, item_count, item_height, start, end);

    let mut body = vstack()
        .w("100%")
        .h(total_h)
        .min_h(total_h)
        .flex_shrink(0.0);
    if top_h > 0.0 {
        body = body.child(make_spacer(list_id, "top", top_h));
    }
    for i in start..end {
        let row = tune_row(
            render_row(render_item, i, axis.is_some()),
            list_id,
            i,
            stable_key_for(i, stable_keys),
            &axis_key(i, stable_keys),
            item_height,
            axis.is_some(),
            animate_item_changes,
        );
        body = body.child(row);
    }
    if bottom_h > 0.0 {
        body = body.child(make_spacer(list_id, "bot", bottom_h));
    }
    let mut body: Element = body.into();
    body.attrs
        .insert(HEIGHT_ATTR.to_string(), item_height.to_string());
    body
}

/// Incremental window update: reuse still-visible keyed rows; create/drop the rest.
#[expect(
    clippy::too_many_arguments,
    reason = "The existing rendering adapter keeps its independent geometry, state and callback parameters"
)]
fn apply_body_window(
    root: &mut Element,
    list_id: &str,
    item_count: usize,
    data_key: &str,
    item_height: f32,
    start: usize,
    end: usize,
    render_item: &dyn Fn(usize) -> Element,
    axis: Option<&VirtualAxis<String>>,
    stable_keys: Option<&[String]>,
    animate_item_changes: Option<LayoutAnimation>,
    current_item_count: &dyn Fn() -> usize,
) -> LayoutStatus {
    let key = window_key(item_count, data_key, start, end);
    let height_key = item_height.to_string();
    if root.attrs.get(RANGE_ATTR).map(String::as_str) == Some(key.as_str())
        && root.attrs.get(HEIGHT_ATTR) == Some(&height_key)
    {
        return LayoutStatus::Clean;
    }

    let reuse_rows = root
        .attrs
        .get(RANGE_ATTR)
        .and_then(|prev| parse_window_key(prev))
        .map(|(prev_count, _, _, prev_data)| {
            (stable_keys.is_some() || prev_count == item_count) && prev_data == data_key
        })
        .unwrap_or(false);

    let top_tid = top_spacer_id(list_id);
    let bot_tid = bot_spacer_id(list_id);
    let mut old_rows: HashMap<String, Element> = HashMap::new();
    let mut old_top: Option<Element> = None;
    let mut old_bot: Option<Element> = None;
    for child in std::mem::take(&mut root.children) {
        match child.template_id.as_deref() {
            Some(id) if id == top_tid => old_top = Some(child),
            Some(id) if id == bot_tid => old_bot = Some(child),
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

    let [total_h, top_h, bottom_h] = row_geometry(axis, item_count, item_height, start, end);

    let mut reused = 0_u32;
    let mut created = 0_u32;
    let mut next = Vec::with_capacity(end.saturating_sub(start).saturating_add(2));

    if top_h > 0.0 {
        let sp = match old_top.take() {
            Some(el) => tune_spacer(el, top_h),
            None => make_spacer(list_id, "top", top_h),
        };
        next.push(sp);
    }

    for i in start..end {
        if i >= current_item_count() {
            break;
        }
        let axis_key = axis_key(i, stable_keys);
        let stable_key = stable_key_for(i, stable_keys);
        if reuse_rows {
            if let Some(el) = old_rows.remove(&axis_key) {
                reused += 1;
                next.push(tune_row(
                    el,
                    list_id,
                    i,
                    stable_key,
                    &axis_key,
                    item_height,
                    axis.is_some(),
                    animate_item_changes,
                ));
                continue;
            }
        }
        created += 1;
        next.push(tune_row(
            render_row(render_item, i, axis.is_some()),
            list_id,
            i,
            stable_key,
            &axis_key,
            item_height,
            axis.is_some(),
            animate_item_changes,
        ));
    }

    if bottom_h > 0.0 {
        let sp = match old_bot.take() {
            Some(el) => tune_spacer(el, bottom_h),
            None => make_spacer(list_id, "bot", bottom_h),
        };
        next.push(sp);
    }

    let dropped = old_rows.len() as u32;
    // Remaining `old_rows` / unused spacers drop here (destroy).
    drop(old_rows);
    drop(old_top);
    drop(old_bot);

    apply_body_style(root, total_h);
    root.children = next;
    root.attrs.insert(RANGE_ATTR.to_string(), key);
    root.attrs.insert(HEIGHT_ATTR.to_string(), height_key);
    root.attrs
        .insert(REUSED_ATTR.to_string(), reused.to_string());
    root.attrs
        .insert(CREATED_ATTR.to_string(), created.to_string());
    root.attrs
        .insert(DROPPED_ATTR.to_string(), dropped.to_string());
    LayoutStatus::StructureChanged
}

impl From<VirtualListBuilder> for Element {
    fn from(builder: VirtualListBuilder) -> Self {
        let VirtualListBuilder {
            id,
            item_count,
            item_count_fn,
            focused_index_fn,
            data_key_fn,
            item_key_fn,
            content_width_fn,
            item_height,
            item_height_fn,
            measure_rows,
            animate_item_changes,
            on_scroll_position,
            overscan,
            height,
            initial_scroll_at_end,
            follow_end,
            scrollbar_visibility,
            scroll_behavior,
            render_item,
            chrome,
        } = builder;
        let scroll_id = id.clone();
        let scroll_id_sync = scroll_id.clone();
        let list_id_sync = scroll_id.clone();
        let render_for_sync = Rc::clone(&render_item);
        let item_count_fn_sync = item_count_fn.clone();
        let focused_index_fn_sync = focused_index_fn.clone();
        let data_key_fn_sync = data_key_fn.clone();
        let item_key_fn_sync = item_key_fn.clone();
        let last_revealed_focus = Rc::new(Cell::new(None));
        let last_revealed_focus_sync = Rc::clone(&last_revealed_focus);

        let initial_count = item_count_fn.as_ref().map(|f| f()).unwrap_or(item_count);
        let initial_data_key = data_key_fn.as_ref().map(|f| f()).unwrap_or_default();
        let initial_stable_keys = item_key_fn
            .as_ref()
            .map(|f| collect_stable_keys(initial_count, f.as_ref()));
        let initial_axis_keys = axis_keys(initial_count, initial_stable_keys.as_deref());
        let fallback_item_height = item_height;
        let read_item_height = move || {
            item_height_fn
                .as_ref()
                .map(|f| f())
                .filter(|height| height.is_finite() && *height >= 1.0)
                .unwrap_or(fallback_item_height)
        };
        let item_height = read_item_height();
        let axis = RefCell::new(measure_rows.then(|| {
            VirtualAxis::new(initial_axis_keys.iter().cloned(), f64::from(item_height))
                .expect("measured VirtualList requires finite heights and at most 1,000,000 rows")
        }));
        let previous_width = Cell::new(0.0_f32);
        let previous_data_key = RefCell::new(initial_data_key.clone());
        let previous_stable_keys = RefCell::new(initial_stable_keys.clone());
        let previous_item_height = Cell::new(item_height);
        let previous_geometry = Cell::new((initial_count, height));
        let previous_position = Cell::new(None);
        let initial_offset = if initial_scroll_at_end {
            (initial_count as f32 * item_height - height).max(0.0)
        } else {
            0.0
        };
        if initial_scroll_at_end && initial_count > 0 {
            scroll_item_into_view_when(
                &scroll_id,
                initial_count - 1,
                initial_count,
                "end",
                "before",
            );
        }
        let (start0, end0) =
            visible_item_range(initial_offset, height, item_height, initial_count, overscan);
        let mut body = build_body(
            &id,
            initial_count,
            item_height,
            start0,
            end0,
            render_item.as_ref(),
            axis.borrow().as_ref(),
            initial_stable_keys.as_deref(),
            animate_item_changes,
        );
        body.attrs.insert(
            RANGE_ATTR.to_string(),
            window_key(initial_count, &initial_data_key, start0, end0),
        );
        body.attrs.insert(REUSED_ATTR.to_string(), "0".into());
        body.attrs
            .insert(HEIGHT_ATTR.to_string(), item_height.to_string());
        body.attrs.insert(CREATED_ATTR.to_string(), "0".into());
        body.attrs.insert(DROPPED_ATTR.to_string(), "0".into());
        let horizontal_scroll = content_width_fn.is_some();
        if let Some(width) = &content_width_fn {
            body = body.min_w(width().max(0.0));
        }
        body = body.reactive_sync(move |root, _layout| {
            let sy_sig = use_signal(&scroll_offset_y_signal_key(&scroll_id_sync), || 0.0_f32);
            let vh_sig = use_signal(&scroll_viewport_h_signal_key(&scroll_id_sync), || height);
            let mut sy = sy_sig.try_peek().unwrap_or(0.0);
            let vh_raw = vh_sig.try_peek().unwrap_or(height);
            let vh = if vh_raw.is_finite() && vh_raw > 1.0 {
                vh_raw
            } else {
                height
            };
            let count = item_count_fn_sync
                .as_ref()
                .map(|f| f())
                .unwrap_or(item_count);
            let data_key = data_key_fn_sync.as_ref().map(|f| f()).unwrap_or_default();
            let item_height = read_item_height();
            let old_height = previous_item_height.replace(item_height);
            let (old_count, old_viewport) = previous_geometry.replace((count, vh));
            let data_changed = *previous_data_key.borrow() != data_key;
            let stable_keys = item_key_fn_sync.as_ref().map(|f| {
                if count != old_count || data_changed {
                    collect_stable_keys(count, f.as_ref())
                } else {
                    previous_stable_keys.borrow().clone().unwrap_or_default()
                }
            });
            let keys_changed = stable_keys
                .as_ref()
                .is_some_and(|keys| previous_stable_keys.borrow().as_ref() != Some(keys));
            *previous_stable_keys.borrow_mut() = stable_keys.clone();
            let mut layout_rebase_y = 0.0_f32;
            let mut axis = axis.borrow_mut();
            if let Some(axis) = axis.as_mut() {
                // Capture once before the batch: clamping after each row would
                // lose the original anchor when multiple rows change together.
                let raw_sy = sy;
                sy = if sy.is_finite() { sy.max(0.0) } else { 0.0 };
                let old_sy = sy;
                let anchor = axis.anchor(f64::from(sy)).unwrap();
                let at_end =
                    follow_end && sy >= (axis.total() - f64::from(old_viewport)).max(0.0) as f32;
                let width =
                    use_signal(&scroll_viewport_w_signal_key(&scroll_id_sync), || 0.0_f32).peek();
                let old_width = previous_width.replace(width);
                *previous_data_key.borrow_mut() = data_key.clone();
                let reset = (data_changed && stable_keys.is_none())
                    || width != old_width
                    || item_height != old_height;
                let mut changed = reset || axis.len() != count || keys_changed;
                if reset {
                    let keys = axis_keys(count, stable_keys.as_deref());
                    *axis = VirtualAxis::new(keys, f64::from(item_height))
                        .expect("measured VirtualList requires at most 1,000,000 rows");
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
                        let measured = child.layout_bounds[3];
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
                let max = (axis.total() - f64::from(vh)).max(0.0);
                if changed || vh != old_viewport || sy != raw_sy || f64::from(sy) > max {
                    let restored = anchor
                        .as_ref()
                        .and_then(|anchor| axis.restore_anchor(anchor, f64::from(vh)).ok());
                    sy = if at_end {
                        max
                    } else {
                        restored.unwrap_or(f64::from(sy).min(max))
                    } as f32;
                    if !at_end {
                        if let Some(index) =
                            anchor.as_ref().and_then(|anchor| axis.index(&anchor.key))
                        {
                            // Preserve the half-open row boundary after f64 -> f32.
                            sy = sy
                                .min((axis.position(index + 1).unwrap() as f32).next_down())
                                .max(0.0);
                        }
                    }
                    sy_sig.set_silent(sy);
                    use_signal(&scroll_target_y_signal_key(&scroll_id_sync), || 0.0_f32)
                        .set_silent(sy);
                    layout_rebase_y = sy - old_sy;
                    root.attrs.remove(HEIGHT_ATTR);
                }
            } else {
                let max_offset = (count as f64 * f64::from(item_height) - f64::from(vh)).max(0.0);
                let old_max_offset = (old_count as f64 * f64::from(old_height)
                    - f64::from(old_viewport))
                .max(0.0) as f32;
                if follow_end
                    && sy >= old_max_offset
                    && (count != old_count || item_height != old_height || vh != old_viewport)
                {
                    sy = max_offset as f32;
                    sy_sig.set_silent(sy);
                    use_signal(&scroll_target_y_signal_key(&scroll_id_sync), || 0.0_f32)
                        .set_silent(sy);
                } else if item_height != old_height {
                    let old_offset = f64::from(sy.max(0.0));
                    let row = (old_offset / f64::from(old_height)).floor();
                    let within = old_offset - row * f64::from(old_height);
                    let row_end = ((row + 1.0) * f64::from(item_height)) as f32;
                    sy = (row * f64::from(item_height) + within.min(f64::from(item_height)))
                        .min(max_offset) as f32;
                    // Keep the first visible row even when f32 rounding lands on
                    // its exclusive end after a height reduction.
                    sy = sy.min(row_end.next_down()).max(0.0);
                    sy_sig.set_silent(sy);
                    use_signal(&scroll_target_y_signal_key(&scroll_id_sync), || 0.0_f32)
                        .set_silent(sy);
                } else if f64::from(sy) > max_offset {
                    // Remount the real end immediately after data shrinkage, before
                    // the renderer publishes its next clamped offset.
                    sy = max_offset as f32;
                    sy_sig.set_silent(sy);
                    use_signal(&scroll_target_y_signal_key(&scroll_id_sync), || 0.0_f32)
                        .set_silent(sy);
                }
            }
            let focused = focused_index_fn_sync
                .as_ref()
                .and_then(|f| f())
                .filter(|index| *index < count);
            let (start, end, reveal_align) = if let Some(axis) = axis.as_ref() {
                if let Some(index) = focused {
                    let revealed = axis
                        .reveal(
                            &axis_key(index, stable_keys.as_deref()),
                            f64::from(sy),
                            f64::from(vh),
                        )
                        .unwrap() as f32;
                    if revealed != sy {
                        sy = revealed;
                        sy_sig.set_silent(sy);
                        use_signal(&scroll_target_y_signal_key(&scroll_id_sync), || 0.0_f32)
                            .set_silent(sy);
                    }
                }
                let window = axis
                    .window(f64::from(sy), f64::from(vh), 0.0, &[], usize::MAX)
                    .unwrap();
                let mut start = window.visible.start.saturating_sub(overscan);
                let mut end = window.visible.end.saturating_add(overscan).min(count);
                if let Some(index) = focused {
                    start = start.min(index.saturating_sub(1));
                    end = end.max(index.saturating_add(2).min(count));
                }
                (start, end, None)
            } else {
                focused_item_window(sy, vh, item_height, count, overscan, focused)
            };
            if let Some(callback) = &on_scroll_position {
                let position = if count > 0 && vh_raw.is_finite() && vh_raw > 0.0 && sy.is_finite()
                {
                    let offset = f64::from(sy.max(0.0));
                    if let Some(axis) = axis.as_ref() {
                        axis.scroll_position(offset, f64::from(vh_raw)).unwrap()
                    } else {
                        let index =
                            ((offset / f64::from(item_height)).floor() as usize).min(count - 1);
                        Some(ListScrollPosition {
                            index,
                            offset: offset - index as f64 * f64::from(item_height),
                        })
                    }
                } else {
                    None
                };
                if previous_position.replace(Some(position)) != Some(position) {
                    callback(position);
                }
            }
            if let Some(index) = focused {
                if let Some(align) = reveal_align {
                    if last_revealed_focus_sync.get() != Some(index) {
                        scroll_item_into_view_when(&scroll_id_sync, index, count, align, "after");
                    }
                    last_revealed_focus_sync.set(Some(index));
                } else {
                    last_revealed_focus_sync.set(None);
                }
            } else {
                last_revealed_focus_sync.set(None);
            }
            if layout_rebase_y != 0.0 && animate_item_changes.is_some() && stable_keys.is_some() {
                for row in &root.children {
                    if row.attrs.contains_key(ROW_KEY_ATTR) {
                        if let Some(motion) = row.layout_motion_handle() {
                            motion.rebase_position([0.0, layout_rebase_y]);
                        }
                    }
                }
            }
            let mut status = apply_body_window(
                root,
                &list_id_sync,
                count,
                &data_key,
                item_height,
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
            );
            if let Some(width) = &content_width_fn {
                let width = width().max(0.0);
                let previous = root.style.min_size.width;
                *root = std::mem::take(root).min_w(width);
                if root.style.min_size.width != previous && status == LayoutStatus::Clean {
                    status = LayoutStatus::LayoutChanged;
                }
            }
            status
        });

        chrome.apply(
            scroll_area(&scroll_id)
                .overflow_y("auto")
                .overflow_x(if horizontal_scroll { "auto" } else { "hidden" })
                .scroll_behavior(scroll_behavior)
                .scrollbar_visibility(scrollbar_visibility)
                .w("100%")
                .h(height)
                .child(body)
                .into(),
        )
    }
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/virtual_list.rs"]
mod tests;
