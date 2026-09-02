//! # VirtualMasonry
//!
//! 变高卡片瀑布流虚拟化（**experimental**；WP-3.10）：最短列优先装箱，
//! 只挂载视口 + overscan 内的格子。行高来自 [`item_height`] 回调与测宽缓存
//!（WP-2.1），对照 EUI `virtualMasonry`。
//!
//! 滚动偏移每帧 [`Signal::set_silent`]，窗口用 [`ReactiveSync`] `peek`。
//! 可见格按 `template_id`（`{id}__cell_{i}`）增量复用。

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::components::scroll_area::scroll_area;
use crate::core::reactivity::use_signal;
use crate::core::scroll_viewport::{
    scroll_offset_y_signal_key, scroll_viewport_h_signal_key, scroll_viewport_w_signal_key,
};
use crate::core::tree::{Element, LayoutStatus};
use crate::primitives::r#box::r#box;
use crate::utils::virtual_masonry::{
    MasonryItemFrame, MasonryLayout, height_from_cache, pack_masonry, visible_masonry_indices,
};

const RANGE_ATTR: &str = "__rui_vmasonry_range";
const REUSED_ATTR: &str = "__rui_vmasonry_reused";
const CREATED_ATTR: &str = "__rui_vmasonry_created";
const DROPPED_ATTR: &str = "__rui_vmasonry_dropped";
const TOTAL_H_ATTR: &str = "__rui_vmasonry_total_h";

pub struct VirtualMasonryBuilder {
    id: String,
    item_count: usize,
    columns: usize,
    gap: f32,
    height: f32,
    overscan_viewports: f32,
    scrollbar_visibility: String,
    item_height: Rc<dyn Fn(usize, f32) -> f32>,
    render_item: Rc<dyn Fn(usize, f32, f32) -> Element>,
}

pub fn virtual_masonry(id: impl Into<String>) -> VirtualMasonryBuilder {
    VirtualMasonryBuilder {
        id: id.into(),
        item_count: 0,
        columns: 2,
        gap: 8.0,
        height: 240.0,
        overscan_viewports: 1.0,
        scrollbar_visibility: "auto".to_string(),
        item_height: Rc::new(|_, col_w| col_w.max(1.0)),
        render_item: Rc::new(|i, w, h| {
            Element::from(r#box().w(w).h(h).min_h(h).text(format!("item {i}")))
        }),
    }
}

impl VirtualMasonryBuilder {
    pub fn item_count(mut self, n: usize) -> Self {
        self.item_count = n;
        self
    }

    pub fn columns(mut self, n: usize) -> Self {
        self.columns = n.max(1);
        self
    }

    pub fn gap(mut self, g: f32) -> Self {
        self.gap = g.max(0.0);
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h.max(1.0);
        self
    }

    /// 视口高度倍数的 overscan（EUI `overscanViewports`，默认 1）。
    pub fn overscan_viewports(mut self, n: f32) -> Self {
        self.overscan_viewports = n.max(0.0);
        self
    }

    pub fn scrollbar_visibility(mut self, v: impl Into<String>) -> Self {
        self.scrollbar_visibility = v.into();
        self
    }

    /// `(index, column_width) -> height`。可接测宽或 [`height_from_cache`]。
    pub fn item_height(mut self, f: impl Fn(usize, f32) -> f32 + 'static) -> Self {
        self.item_height = Rc::new(f);
        self
    }

    /// `(index, width, height) -> cell`。
    pub fn render_item(mut self, f: impl Fn(usize, f32, f32) -> Element + 'static) -> Self {
        self.render_item = Rc::new(f);
        self
    }
}

fn cell_id(list_id: &str, index: usize) -> String {
    format!("{list_id}__cell_{index}")
}

fn parse_cell_index(list_id: &str, template_id: &str) -> Option<usize> {
    let prefix = format!("{list_id}__cell_");
    template_id.strip_prefix(&prefix)?.parse().ok()
}

fn collect_heights(
    count: usize,
    col_w: f32,
    cache: &HashMap<usize, f32>,
    item_height: &dyn Fn(usize, f32) -> f32,
) -> Vec<f32> {
    (0..count)
        .map(|i| height_from_cache(cache, i, col_w, |idx, w| item_height(idx, w)))
        .collect()
}

fn harvest_measured_heights(
    root: &Element,
    list_id: &str,
    cache: &mut HashMap<usize, f32>,
) -> bool {
    let mut changed = false;
    for child in &root.children {
        let Some(i) = child
            .template_id
            .as_deref()
            .and_then(|id| parse_cell_index(list_id, id))
        else {
            continue;
        };
        let h = child.layout_bounds[3];
        if h <= 0.5 {
            continue;
        }
        let prev = cache.get(&i).copied();
        if prev.is_none_or(|p| (p - h).abs() > 0.5) {
            cache.insert(i, h);
            changed = true;
        }
    }
    changed
}

fn tune_cell(mut el: Element, list_id: &str, frame: &MasonryItemFrame) -> Element {
    el = el
        .template_id(cell_id(list_id, frame.index))
        .absolute()
        .left(frame.x)
        .top(frame.y)
        .w(frame.width)
        .h(frame.height)
        .min_h(frame.height)
        .max_h(frame.height)
        .flex_shrink(0.0);
    el
}

fn apply_body_style(root: &mut Element, total_h: f32) {
    *root = std::mem::take(root)
        .relative()
        .w("100%")
        .h(total_h)
        .min_h(total_h)
        .flex_shrink(0.0);
}

fn window_key(visible: &[usize], total_h: f32, col_w: f32) -> String {
    format!("{visible:?}|{total_h:.1}|{col_w:.1}")
}

fn build_body(
    list_id: &str,
    layout: &MasonryLayout,
    visible: &[usize],
    render_item: &dyn Fn(usize, f32, f32) -> Element,
) -> Element {
    let mut body = r#box()
        .relative(true)
        .w("100%")
        .h(layout.total_height)
        .min_h(layout.total_height)
        .flex_shrink(0.0);
    for &i in visible {
        let frame = &layout.items[i];
        let cell = tune_cell(render_item(i, frame.width, frame.height), list_id, frame);
        body = body.child(cell);
    }
    body.into()
}

fn apply_body_window(
    root: &mut Element,
    list_id: &str,
    layout: &MasonryLayout,
    visible: &[usize],
    render_item: &dyn Fn(usize, f32, f32) -> Element,
) -> LayoutStatus {
    let key = window_key(visible, layout.total_height, layout.column_width);
    if root.attrs.get(RANGE_ATTR).map(String::as_str) == Some(key.as_str()) {
        return LayoutStatus::Clean;
    }

    let mut old: HashMap<usize, Element> = HashMap::new();
    for child in std::mem::take(&mut root.children) {
        if let Some(i) = child
            .template_id
            .as_deref()
            .and_then(|id| parse_cell_index(list_id, id))
        {
            old.insert(i, child);
        }
    }

    let mut reused = 0_u32;
    let mut created = 0_u32;
    let mut next = Vec::with_capacity(visible.len());
    for &i in visible {
        let frame = &layout.items[i];
        if let Some(el) = old.remove(&i) {
            reused += 1;
            next.push(tune_cell(el, list_id, frame));
        } else {
            created += 1;
            next.push(tune_cell(
                render_item(i, frame.width, frame.height),
                list_id,
                frame,
            ));
        }
    }
    let dropped = old.len() as u32;
    drop(old);

    apply_body_style(root, layout.total_height);
    root.children = next;
    root.attrs.insert(RANGE_ATTR.to_string(), key);
    root.attrs
        .insert(REUSED_ATTR.to_string(), reused.to_string());
    root.attrs
        .insert(CREATED_ATTR.to_string(), created.to_string());
    root.attrs
        .insert(DROPPED_ATTR.to_string(), dropped.to_string());
    root.attrs.insert(
        TOTAL_H_ATTR.to_string(),
        format!("{:.1}", layout.total_height),
    );
    LayoutStatus::StructureChanged
}

impl From<VirtualMasonryBuilder> for Element {
    fn from(builder: VirtualMasonryBuilder) -> Self {
        let VirtualMasonryBuilder {
            id,
            item_count,
            columns,
            gap,
            height,
            overscan_viewports,
            scrollbar_visibility,
            item_height,
            render_item,
        } = builder;
        let scroll_id = id.clone();
        let list_id_sync = scroll_id.clone();
        let render_for_sync = Rc::clone(&render_item);
        let height_fn = Rc::clone(&item_height);
        let cache = Rc::new(RefCell::new(HashMap::<usize, f32>::new()));
        let cache_sync = Rc::clone(&cache);
        let last_col_w = Rc::new(RefCell::new(0.0_f32));

        let content_w0 = 320.0;
        let col_w0 = crate::utils::virtual_masonry::masonry_column_width(content_w0, columns, gap);
        let heights0 = collect_heights(item_count, col_w0, &cache.borrow(), height_fn.as_ref());
        let layout0 = pack_masonry(&heights0, columns, content_w0, gap);
        let vis0 =
            visible_masonry_indices(&layout0.items, 0.0, height, height * overscan_viewports);
        let mut body = build_body(&id, &layout0, &vis0, render_item.as_ref());
        body.attrs.insert(
            RANGE_ATTR.to_string(),
            window_key(&vis0, layout0.total_height, layout0.column_width),
        );
        body.attrs.insert(REUSED_ATTR.to_string(), "0".into());
        body.attrs.insert(CREATED_ATTR.to_string(), "0".into());
        body.attrs.insert(DROPPED_ATTR.to_string(), "0".into());
        body.attrs.insert(
            TOTAL_H_ATTR.to_string(),
            format!("{:.1}", layout0.total_height),
        );

        body = body.reactive_sync(move |root, _layout| {
            harvest_measured_heights(root, &list_id_sync, &mut cache_sync.borrow_mut());
            let sy = use_signal(&scroll_offset_y_signal_key(&scroll_id), || 0.0_f32).peek();
            let vh = use_signal(&scroll_viewport_h_signal_key(&scroll_id), || height).peek();
            let vw = use_signal(&scroll_viewport_w_signal_key(&scroll_id), || 0.0_f32).peek();
            let vh = if vh > 1.0 { vh } else { height };
            let content_w = if vw > 1.0 { vw } else { content_w0 };
            let col_w =
                crate::utils::virtual_masonry::masonry_column_width(content_w, columns, gap);
            if (*last_col_w.borrow() - col_w).abs() > 0.5 {
                cache_sync.borrow_mut().clear();
                *last_col_w.borrow_mut() = col_w;
            }
            let heights =
                collect_heights(item_count, col_w, &cache_sync.borrow(), height_fn.as_ref());
            let layout = pack_masonry(&heights, columns, content_w, gap);
            let overscan_px = vh * overscan_viewports;
            let vis = visible_masonry_indices(&layout.items, sy, vh, overscan_px);
            apply_body_window(root, &list_id_sync, &layout, &vis, render_for_sync.as_ref())
        });

        scroll_area(&id)
            .overflow_y("auto")
            .overflow_x("hidden")
            .scroll_behavior("jump")
            .scrollbar_visibility(scrollbar_visibility)
            .w("100%")
            .h(height)
            .child(body)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_masonry_builds() {
        let _el: Element = virtual_masonry("vm1")
            .item_count(50)
            .columns(3)
            .item_height(|i, _| 40.0 + (i % 3) as f32 * 10.0)
            .render_item(|i, w, h| Element::from(r#box().w(w).h(h).text(format!("#{i}"))))
            .into();
    }

    #[test]
    fn apply_body_window_reuses_overlap() {
        let heights = vec![40.0; 30];
        let layout = pack_masonry(&heights, 2, 200.0, 8.0);
        let vis_a: Vec<usize> = (0..8).collect();
        let mut root = build_body("vm", &layout, &vis_a, &|i, w, h| {
            Element::from(r#box().w(w).h(h).text(format!("c{i}")))
        });
        root.attrs.insert(
            RANGE_ATTR.to_string(),
            window_key(&vis_a, layout.total_height, layout.column_width),
        );
        for child in &mut root.children {
            if let Some(i) = child
                .template_id
                .as_deref()
                .and_then(|id| parse_cell_index("vm", id))
            {
                if (4..8).contains(&i) {
                    child.layout_node = Some(taffy::prelude::NodeId::from(2000u64 + i as u64));
                }
            }
        }

        let vis_b: Vec<usize> = (4..12).collect();
        assert_eq!(
            apply_body_window(&mut root, "vm", &layout, &vis_b, &|i, w, h| {
                Element::from(r#box().w(w).h(h).text(format!("c{i}")))
            }),
            LayoutStatus::StructureChanged
        );
        assert_eq!(root.attrs.get(REUSED_ATTR).map(String::as_str), Some("4"));
        assert_eq!(root.attrs.get(CREATED_ATTR).map(String::as_str), Some("4"));
        assert_eq!(root.attrs.get(DROPPED_ATTR).map(String::as_str), Some("4"));

        let node4 = root
            .children
            .iter()
            .find(|c| c.template_id.as_deref() == Some("vm__cell_4"))
            .and_then(|c| c.layout_node);
        assert_eq!(node4, Some(taffy::prelude::NodeId::from(2004u64)));
    }
}
