//! Flex 布局组件：对齐浏览器 / Solid `Flex`（`display: flex` + 方向）。
//!
//! RSX：`<Flex direction="row|column" …>`；多子节点由宏打成 `rui_fragment`，本组件再展开为 flex 子项。

use crate::core::tree::Element;
use crate::primitives::{hstack, vstack};
use rui_macros::component;

fn unpack_children(children: Option<Element>) -> Vec<Element> {
    match children {
        Some(mut c) if c.attrs.get("rui_fragment").map(|s| s.as_str()) == Some("1") => {
            std::mem::take(&mut c.children)
        }
        Some(c) => vec![c],
        None => Vec::new(),
    }
}

/// 弹性布局容器（默认横向 `row`，类似 CSS `display: flex`）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Flex(direction: Option<String>, children: Option<Element>) -> Element {
    let dir = direction.unwrap_or_else(|| "row".into());
    let dir_l = dir.to_ascii_lowercase();
    let mut el: Element = match dir_l.as_str() {
        "column" | "col" | "flex-col" | "column-reverse" => vstack().into(),
        _ => hstack().into(),
    };
    if dir_l == "column-reverse" {
        el.style.flex_direction = taffy::style::FlexDirection::ColumnReverse;
    } else if dir_l == "row-reverse" {
        el.style.flex_direction = taffy::style::FlexDirection::RowReverse;
    }
    for child in unpack_children(children) {
        el = el.child(child);
    }
    el
}
