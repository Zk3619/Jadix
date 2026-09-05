//! # VirtualList
//!
//! 固定行高虚拟列表：顶部/底部 spacer + 仅渲染可见行。
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

use std::collections::HashMap;
use std::rc::Rc;

use crate::components::base::SlotChrome;
use crate::components::scroll_area::scroll_area;
use crate::core::reactivity::use_signal;
use crate::core::scroll_viewport::{scroll_offset_y_signal_key, scroll_viewport_h_signal_key};
use crate::core::tree::{Element, LayoutStatus};
use crate::primitives::r#box::{r#box, vstack};
use crate::utils::virtual_list::{item_height_for_text, visible_item_range};

const RANGE_ATTR: &str = "__rui_vlist_range";
const REUSED_ATTR: &str = "__rui_vlist_reused";
const CREATED_ATTR: &str = "__rui_vlist_created";
const DROPPED_ATTR: &str = "__rui_vlist_dropped";

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
    data_key_fn: Option<Rc<dyn Fn() -> String>>,
    item_height: f32,
    overscan: usize,
    height: f32,
    scrollbar_visibility: String,
    render_item: Rc<dyn Fn(usize) -> Element>,
    chrome: SlotChrome,
}

pub fn virtual_list(id: impl Into<String>) -> VirtualListBuilder {
    VirtualListBuilder {
        id: id.into(),
        item_count: 0,
        item_count_fn: None,
        data_key_fn: None,
        item_height: 32.0,
        overscan: 2,
        height: 240.0,
        scrollbar_visibility: "auto".to_string(),
        render_item: Rc::new(|i| Element::new().text(format!("item {i}"))),
        chrome: SlotChrome::default(),
    }
}

impl VirtualListBuilder {
    crate::impl_slot_chrome_methods!();
    pub fn item_count(mut self, n: usize) -> Self {
        self.item_count = n;
        self
    }

    /// Dynamic row count polled each frame in [`ReactiveSync`] (peek-only; no effect subscription).
    pub fn item_count_fn(mut self, f: impl Fn() -> usize + 'static) -> Self {
        self.item_count_fn = Some(Rc::new(f));
        self
    }

    /// Content fingerprint polled each frame; when it changes, visible rows are re-rendered
    /// even if `[start, end)` is unchanged (TreeView expand/collapse with same window).
    pub fn data_key_fn(mut self, f: impl Fn() -> String + 'static) -> Self {
        self.data_key_fn = Some(Rc::new(f));
        self
    }

    pub fn item_height(mut self, h: f32) -> Self {
        self.item_height = h.max(1.0);
        self
    }

    /// Set row height from font size × leading scale (see [`item_height_for_text`]).
    pub fn item_height_from_font(mut self, font_size: f32, leading_scale: f32) -> Self {
        self.item_height = item_height_for_text(font_size, leading_scale);
        self
    }

    pub fn overscan(mut self, n: usize) -> Self {
        self.overscan = n;
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h.max(1.0);
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

fn row_id(list_id: &str, index: usize) -> String {
    format!("{list_id}__row_{index}")
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

fn tune_spacer(mut el: Element, height: f32) -> Element {
    el = el.h(height).min_h(height).flex_shrink(0.0);
    el
}

fn tune_row(mut el: Element, list_id: &str, index: usize, item_height: f32) -> Element {
    el = el
        .template_id(row_id(list_id, index))
        .h(item_height)
        .min_h(item_height)
        .max_h(item_height)
        .w("100%")
        .flex_shrink(0.0);
    el
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
) -> Element {
    let total_h = item_count as f32 * item_height;
    let top_h = start as f32 * item_height;
    let bottom_h = (item_count.saturating_sub(end) as f32) * item_height;

    let mut body = vstack()
        .w("100%")
        .h(total_h)
        .min_h(total_h)
        .flex_shrink(0.0);
    if top_h > 0.0 {
        body = body.child(make_spacer(list_id, "top", top_h));
    }
    for i in start..end {
        let row = tune_row(render_item(i), list_id, i, item_height);
        body = body.child(row);
    }
    if bottom_h > 0.0 {
        body = body.child(make_spacer(list_id, "bot", bottom_h));
    }
    body.into()
}

/// Incremental window update: reuse still-visible keyed rows; create/drop the rest.
fn apply_body_window(
    root: &mut Element,
    list_id: &str,
    item_count: usize,
    data_key: &str,
    item_height: f32,
    start: usize,
    end: usize,
    render_item: &dyn Fn(usize) -> Element,
) -> LayoutStatus {
    let key = window_key(item_count, data_key, start, end);
    if root.attrs.get(RANGE_ATTR).map(String::as_str) == Some(key.as_str()) {
        return LayoutStatus::Clean;
    }

    let reuse_rows = root
        .attrs
        .get(RANGE_ATTR)
        .and_then(|prev| parse_window_key(prev))
        .map(|(prev_count, _, _, prev_data)| prev_count == item_count && prev_data == data_key)
        .unwrap_or(false);

    let top_tid = top_spacer_id(list_id);
    let bot_tid = bot_spacer_id(list_id);
    let mut old_rows: HashMap<usize, Element> = HashMap::new();
    let mut old_top: Option<Element> = None;
    let mut old_bot: Option<Element> = None;
    for child in std::mem::take(&mut root.children) {
        match child.template_id.as_deref() {
            Some(id) if id == top_tid => old_top = Some(child),
            Some(id) if id == bot_tid => old_bot = Some(child),
            Some(id) => {
                if let Some(i) = parse_row_index(list_id, id) {
                    old_rows.insert(i, child);
                }
            }
            None => {}
        }
    }

    let total_h = item_count as f32 * item_height;
    let top_h = start as f32 * item_height;
    let bottom_h = (item_count.saturating_sub(end) as f32) * item_height;

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
        if reuse_rows {
            if let Some(el) = old_rows.remove(&i) {
                reused += 1;
                next.push(tune_row(el, list_id, i, item_height));
                continue;
            }
        }
        created += 1;
        next.push(tune_row(render_item(i), list_id, i, item_height));
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
            data_key_fn,
            item_height,
            overscan,
            height,
            scrollbar_visibility,
            render_item,
            chrome,
        } = builder;
        let scroll_id = id.clone();
        let scroll_id_sync = scroll_id.clone();
        let list_id_sync = scroll_id.clone();
        let render_for_sync = Rc::clone(&render_item);
        let item_count_fn_sync = item_count_fn.clone();
        let data_key_fn_sync = data_key_fn.clone();

        let initial_count = item_count_fn.as_ref().map(|f| f()).unwrap_or(item_count);
        let initial_data_key = data_key_fn.as_ref().map(|f| f()).unwrap_or_default();
        let (start0, end0) = visible_item_range(0.0, height, item_height, initial_count, overscan);
        let mut body = build_body(
            &id,
            initial_count,
            item_height,
            start0,
            end0,
            render_item.as_ref(),
        );
        body.attrs.insert(
            RANGE_ATTR.to_string(),
            window_key(initial_count, &initial_data_key, start0, end0),
        );
        body.attrs.insert(REUSED_ATTR.to_string(), "0".into());
        body.attrs.insert(CREATED_ATTR.to_string(), "0".into());
        body.attrs.insert(DROPPED_ATTR.to_string(), "0".into());
        body = body.reactive_sync(move |root, _layout| {
            let sy_sig = use_signal(&scroll_offset_y_signal_key(&scroll_id_sync), || 0.0_f32);
            let vh_sig = use_signal(&scroll_viewport_h_signal_key(&scroll_id_sync), || height);
            let sy = sy_sig.try_peek().unwrap_or(0.0);
            let vh_raw = vh_sig.try_peek().unwrap_or(height);
            let vh = if vh_raw > 1.0 { vh_raw } else { height };
            let count = item_count_fn_sync
                .as_ref()
                .map(|f| f())
                .unwrap_or(item_count);
            let data_key = data_key_fn_sync.as_ref().map(|f| f()).unwrap_or_default();
            let (start, end) = visible_item_range(sy, vh, item_height, count, overscan);
            apply_body_window(
                root,
                &list_id_sync,
                count,
                &data_key,
                item_height,
                start,
                end,
                render_for_sync.as_ref(),
            )
        });

        chrome.apply(
            scroll_area(&scroll_id)
                .overflow_y("auto")
                .overflow_x("hidden")
                .scroll_behavior("jump")
                .scrollbar_visibility(scrollbar_visibility)
                .w("100%")
                .h(height)
                .child(body)
                .into(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::virtual_list::visible_item_range;

    #[test]
    fn visible_range_helper() {
        let (a, b) = visible_item_range(0.0, 100.0, 20.0, 100, 1);
        assert_eq!(a, 0);
        assert!(b > a);
        let (a2, b2) = visible_item_range(40.0, 100.0, 20.0, 100, 2);
        assert_eq!(a2, 0);
        assert_eq!(b2, 9);
    }

    #[test]
    fn virtual_list_builds() {
        let _el: Element = virtual_list("vl1")
            .item_count(100)
            .item_height(24.0)
            .render_item(|i| Element::new().text(format!("#{i}")))
            .into();
    }

    #[test]
    fn builder_style_reaches_list_root() {
        let el: Element = virtual_list("vl-chrome")
            .item_count(3)
            .item_height(24.0)
            .style(|b| b.text_size(20.0))
            .render_item(|i| Element::new().text(format!("#{i}")))
            .into();
        fn find_size(el: &Element, want: f32) -> bool {
            el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
                || (el.text_style.size_resolved() - want).abs() < 0.01
                || el.children.iter().any(|c| find_size(c, want))
        }
        assert!(
            find_size(&el, 20.0),
            "virtual_list .style must land on the scroll root"
        );
    }

    #[test]
    fn build_body_total_height_matches_item_count() {
        let body = build_body("vl", 120, 21.0, 11, 28, &|i| {
            Element::new().text(format!("{i}"))
        });
        match crate::layout::dim_kind(body.style.size.height) {
            crate::layout::DimKind::Length(h) => assert!((h - 120.0 * 21.0).abs() < 0.01),
            other => panic!("expected Length total height, got {other:?}"),
        }
    }

    #[test]
    fn apply_body_window_is_idempotent_for_same_range() {
        let mut root = build_body("vl", 50, 20.0, 0, 10, &|i| {
            Element::new().text(format!("{i}"))
        });
        root.attrs
            .insert(RANGE_ATTR.to_string(), window_key(50, "", 0, 10));
        assert_eq!(
            apply_body_window(&mut root, "vl", 50, "", 20.0, 0, 10, &|i| {
                Element::new().text(format!("{i}"))
            }),
            LayoutStatus::Clean
        );
    }

    #[test]
    fn apply_body_window_updates_when_item_count_changes_same_range() {
        let mut root = build_body("vl", 50, 20.0, 0, 10, &|i| {
            Element::new().text(format!("{i}"))
        });
        root.attrs
            .insert(RANGE_ATTR.to_string(), window_key(50, "", 0, 10));
        assert_eq!(
            apply_body_window(&mut root, "vl", 20, "", 20.0, 0, 10, &|i| {
                Element::new().text(format!("n{i}"))
            }),
            LayoutStatus::StructureChanged
        );
        match crate::layout::dim_kind(root.style.size.height) {
            crate::layout::DimKind::Length(h) => assert!((h - 20.0 * 20.0).abs() < 0.01),
            other => panic!("expected Length total height, got {other:?}"),
        }
    }

    #[test]
    fn apply_body_window_reuses_overlap_and_drops_old() {
        let mut root = build_body("vl", 100, 20.0, 0, 10, &|i| {
            Element::new().text(format!("r{i:03}"))
        });
        // Stamp fake layout nodes on rows 5..10 so we can assert identity after reuse.
        for child in &mut root.children {
            if let Some(i) = child
                .template_id
                .as_deref()
                .and_then(|id| parse_row_index("vl", id))
            {
                if (5..10).contains(&i) {
                    child.layout_node = Some(taffy::prelude::NodeId::from(1000u64 + i as u64));
                }
            }
        }
        root.attrs
            .insert(RANGE_ATTR.to_string(), window_key(100, "", 0, 10));

        assert_eq!(
            apply_body_window(&mut root, "vl", 100, "", 20.0, 5, 15, &|i| {
                Element::new().text(format!("r{i:03}"))
            }),
            LayoutStatus::StructureChanged
        );
        assert_eq!(root.attrs.get(REUSED_ATTR).map(String::as_str), Some("5"));
        assert_eq!(root.attrs.get(CREATED_ATTR).map(String::as_str), Some("5"));
        assert_eq!(root.attrs.get(DROPPED_ATTR).map(String::as_str), Some("5"));

        let mut texts = Vec::new();
        for c in &root.children {
            if let Some(t) = &c.text {
                texts.push(t.clone());
            }
        }
        assert!(texts.iter().any(|t| t == "r005"));
        assert!(texts.iter().any(|t| t == "r014"));
        assert!(!texts.iter().any(|t| t == "r000"));

        let node5 = root
            .children
            .iter()
            .find(|c| c.template_id.as_deref() == Some("vl__row_5"))
            .and_then(|c| c.layout_node);
        assert_eq!(node5, Some(taffy::prelude::NodeId::from(1005u64)));
    }
}
