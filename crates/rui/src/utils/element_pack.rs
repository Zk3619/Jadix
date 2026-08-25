//! `rui_fragment` / 匿名包装解包（WP-SI.8）。
//!
//! 复合 RSX 常把多个子节点包进带 `rui_fragment=1` 的壳，或无无文本/无点击的
//! 中间节点承载 children。构建行/列表前先 [`take_packed`]。

use crate::core::tree::Element;

/// 仅展开 `rui_fragment`（勿拆开已是行壳的复合 Item）。
pub fn take_fragment(mut el: Element) -> Vec<Element> {
    if el.attrs.get("rui_fragment").map(|s| s.as_str()) == Some("1") {
        std::mem::take(&mut el.children)
    } else {
        vec![el]
    }
}

/// 展开 fragment / 纯包装节点；否则返回单元素 `vec`。
pub fn take_packed(mut el: Element) -> Vec<Element> {
    if el.attrs.get("rui_fragment").map(|s| s.as_str()) == Some("1") {
        std::mem::take(&mut el.children)
    } else if !el.children.is_empty()
        && el.text.is_none()
        && el.on_click.is_none()
        && el.focus_id.is_none()
    {
        std::mem::take(&mut el.children)
    } else {
        vec![el]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_unpacks_children() {
        let mut frag = Element::new();
        frag.set_attr("rui_fragment", "1");
        frag = frag.children(vec![Element::new().text("a"), Element::new().text("b")]);
        let kids = take_packed(frag);
        assert_eq!(kids.len(), 2);
    }

    #[test]
    fn plain_leaf_stays_singleton() {
        let el = Element::new().text("x");
        let kids = take_packed(el);
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].text.as_deref(), Some("x"));
    }
}
