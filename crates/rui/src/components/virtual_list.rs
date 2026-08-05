//! # VirtualList
//!
//! 固定行高虚拟列表：顶部/底部 spacer + 仅渲染可见行。

use crate::app::reactive::reactive_view;
use crate::components::scroll_area::scroll_area;
use crate::core::reactivity::use_signal;
use crate::core::scroll_viewport::{scroll_offset_y_signal_key, scroll_viewport_h_signal_key};
use crate::core::tree::Element;
use crate::primitives::r#box::{r#box, vstack};
use crate::utils::virtual_list::visible_item_range;

pub struct VirtualListBuilder {
    id: String,
    item_count: usize,
    item_height: f32,
    overscan: usize,
    height: f32,
    render_item: Box<dyn Fn(usize) -> Element>,
}

pub fn virtual_list(id: impl Into<String>) -> VirtualListBuilder {
    VirtualListBuilder {
        id: id.into(),
        item_count: 0,
        item_height: 32.0,
        overscan: 2,
        height: 240.0,
        render_item: Box::new(|i| Element::new().text(format!("item {i}"))),
    }
}

impl VirtualListBuilder {
    pub fn item_count(mut self, n: usize) -> Self {
        self.item_count = n;
        self
    }

    pub fn item_height(mut self, h: f32) -> Self {
        self.item_height = h.max(1.0);
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

    pub fn render_item(mut self, f: impl Fn(usize) -> Element + 'static) -> Self {
        self.render_item = Box::new(f);
        self
    }
}

impl From<VirtualListBuilder> for Element {
    fn from(builder: VirtualListBuilder) -> Self {
        let VirtualListBuilder {
            id,
            item_count,
            item_height,
            overscan,
            height,
            render_item,
        } = builder;
        let view_id = format!("{id}__vlist_view");
        let scroll_id = id.clone();

        reactive_view(view_id, move || {
            let sy = use_signal(&scroll_offset_y_signal_key(&scroll_id), || 0.0_f32).get();
            let vh = use_signal(&scroll_viewport_h_signal_key(&scroll_id), || height).get();
            let vh = if vh > 1.0 { vh } else { height };
            let (start, end) = visible_item_range(sy, vh, item_height, item_count, overscan);
            let total_h = item_count as f32 * item_height;
            let top_h = start as f32 * item_height;
            let bottom_h = (item_count.saturating_sub(end) as f32) * item_height;

            let mut body = vstack().w("100%");
            if top_h > 0.0 {
                body = body.child(r#box().w("100%").h(top_h));
            }
            for i in start..end {
                let mut row = render_item(i);
                row = row.h(item_height).w("100%");
                body = body.child(row);
            }
            if bottom_h > 0.0 {
                body = body.child(r#box().w("100%").h(bottom_h));
            }
            // Ensure total scroll height even when empty spacers.
            let _ = total_h;

            scroll_area(&scroll_id)
                .overflow_y("auto")
                .overflow_x("hidden")
                .w("100%")
                .h(height)
                .child(body)
                .into()
        })
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
    }

    #[test]
    fn virtual_list_builds() {
        let _el: Element = virtual_list("vl1")
            .item_count(100)
            .item_height(24.0)
            .render_item(|i| Element::new().text(format!("#{i}")))
            .into();
    }
}
