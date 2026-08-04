//! Grid 布局组件：对齐浏览器 / Solid `Grid`（`display: grid` + 等分轨）。
//!
//! RSX：`<Grid cols={4} rows={6} gap="0.25rem">…</Grid>`；列/行数走 `base`（`cols` / `rows`）。
//! 多子节点由宏打成 `rui_fragment`，本组件展开为 grid 子项（避免被包进 VStack）。

use crate::core::tree::Element;
use crate::primitives::gstack;
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

/// CSS Grid 容器（默认 `display: grid`；用 `cols` / `rows` 设等分 `fr` 轨）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Grid(children: Option<Element>) -> Element {
    let mut el: Element = gstack().into();
    for child in unpack_children(children) {
        el = el.child(child);
    }
    el
}
