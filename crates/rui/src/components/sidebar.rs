//! # Sidebar
//!
//! 侧栏布局壳：竖向导航槽位 + 可选页脚。
//!
//! 默认 `flex-shrink: 0` 且 `min-width` 与宽度一致，避免主内容区固有宽把侧栏挤扁
//! （典型 app-shell：rail 不缩，主区 `min-width: 0`）。
//!
//! 槽位内可聚焦控件（如 Gallery 导航 Button）自动获得 ↑↓ / Home / End **焦点环** roving。

use crate::core::tree::Element;
use crate::primitives::r#box::vstack;
use crate::tokens;
use crate::utils::roving::{attach_focus_roving, FocusRovingAxis};

pub struct SidebarBuilder {
    id: String,
    width: String,
    header: Option<Element>,
    items: Vec<Element>,
    footer: Option<Element>,
}

pub fn sidebar(id: impl Into<String>) -> SidebarBuilder {
    SidebarBuilder {
        id: id.into(),
        width: "14rem".into(),
        header: None,
        items: Vec::new(),
        footer: None,
    }
}

impl SidebarBuilder {
    pub fn w(mut self, w: impl Into<String>) -> Self {
        self.width = w.into();
        self
    }
    pub fn header(mut self, el: impl Into<Element>) -> Self {
        self.header = Some(el.into());
        self
    }
    pub fn item(mut self, el: impl Into<Element>) -> Self {
        self.items.push(el.into());
        self
    }
    pub fn items(mut self, els: Vec<Element>) -> Self {
        self.items.extend(els);
        self
    }
    pub fn footer(mut self, el: impl Into<Element>) -> Self {
        self.footer = Some(el.into());
        self
    }
}

impl From<SidebarBuilder> for Element {
    fn from(builder: SidebarBuilder) -> Self {
        let SidebarBuilder {
            id,
            width,
            header,
            items,
            footer,
        } = builder;
        let mut body = vstack().gap("0.25rem").flex_grow(1.0).w("100%").p("0.5rem");
        for item in items {
            body = body.child(item);
        }
        let mut col = vstack().w(width.as_str()).h("100%").gap("0.5rem");
        if let Some(h) = header {
            col = col.child(h);
        }
        col = col.child(body);
        if let Some(f) = footer {
            col = col.child(f);
        }
        let mut el: Element = col.into();
        // App-shell: fixed rail must not flex-shrink when the main pane's content has a large
        // min-content width (gallery Forms inputs). Pair with main `min_w(0)`.
        el = el.flex_shrink(0.0).min_w(width.as_str());
        el = el.bg(tokens::muted());
        el.style.border_width = 1.0;
        el.style.border_color = tokens::border();
        el.template_id = Some(id);
        attach_focus_roving(&mut el, FocusRovingAxis::Vertical);
        el
    }
}
