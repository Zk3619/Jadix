//! # Navbar
//!
//! 顶栏布局壳：左侧 brand / 中间 slots / 右侧 actions。
//! 链接导览请嵌 [`crate::components::navigation_menu`]；页面栈见
//! [`crate::app::navigator::navigation_host`]。
//!
//! **身份（WP-2.8）**：展示型壳，**不要求**用户 `id`；空 / 省略 → 不写 `template_id`（I-1）。

use crate::core::tree::Element;
use crate::primitives::r#box::hstack;
use crate::tokens;

pub struct NavbarBuilder {
    id: String,
    brand: Option<Element>,
    center: Vec<Element>,
    actions: Vec<Element>,
}

/// 展示型入口：默认无用户 id（WP-2.8）。需要热重载槽时再 `.id(...)`。
pub fn navbar() -> NavbarBuilder {
    NavbarBuilder {
        id: String::new(),
        brand: None,
        center: Vec::new(),
        actions: Vec::new(),
    }
}

impl NavbarBuilder {
    /// 可选槽位 / 测试选择器；空串等同未设置。
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }
    pub fn brand(mut self, el: impl Into<Element>) -> Self {
        self.brand = Some(el.into());
        self
    }
    pub fn center_child(mut self, el: impl Into<Element>) -> Self {
        self.center.push(el.into());
        self
    }
    pub fn action(mut self, el: impl Into<Element>) -> Self {
        self.actions.push(el.into());
        self
    }
}

impl From<NavbarBuilder> for Element {
    fn from(builder: NavbarBuilder) -> Self {
        let NavbarBuilder {
            id,
            brand,
            center,
            actions,
        } = builder;
        let mut left = hstack().gap("0.75rem").items_center();
        if let Some(b) = brand {
            left = left.child(b);
        }
        let mut mid = hstack().gap("0.5rem").items_center().flex_grow(1.0);
        for c in center {
            mid = mid.child(c);
        }
        let mut right = hstack().gap("0.5rem").items_center();
        for a in actions {
            right = right.child(a);
        }
        let mut bar: Element = hstack()
            .w("100%")
            .h("3rem")
            .px("1rem")
            .items_center()
            .gap("1rem")
            .child(left)
            .child(mid)
            .child(right)
            .into();
        bar = bar.bg(tokens::background());
        bar.style.border_width = 1.0;
        bar.style.border_color = tokens::border();
        // Display shell: never write Some("") (I-1).
        if !id.trim().is_empty() {
            bar.template_id = Some(id);
        }
        bar
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navbar_without_id_writes_no_template_id() {
        let el: Element = navbar().into();
        assert!(el.template_id.is_none());
    }

    #[test]
    fn navbar_empty_id_writes_no_template_id() {
        let el: Element = navbar().id("").into();
        assert!(el.template_id.is_none());
        assert_ne!(el.template_id.as_deref(), Some(""));
    }

    #[test]
    fn navbar_explicit_id_sets_template_id() {
        let el: Element = navbar().id("nav-main").into();
        assert_eq!(el.template_id.as_deref(), Some("nav-main"));
    }
}
