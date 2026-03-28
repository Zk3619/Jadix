use crate::core::tree::Element;
use crate::primitives::tabs::tabs_headless;
use rui_macros::component;

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Tabs(id: String, count: usize, children: Option<Element>) -> Element {
    let mut el = tabs_headless_root(id, count);
    if let Some(c) = children {
        el = el.child(c);
    }
    el
}

/// Keyboard Left/Right + focus; combine with your own tab labels row using the same `id` scope for selection state if needed.
pub fn tabs_headless_root(id: impl Into<String>, tab_count: usize) -> Element {
    tabs_headless(id, tab_count).into()
}
