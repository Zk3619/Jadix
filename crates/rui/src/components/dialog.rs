//! # Dialog（面板）
//!
//! 样式化模态内容面板：标题 + body。遮罩 / 居中可由调用方组合；
//! 面板本身已 [`portal_panel`](crate::utils::popup::portal_panel) 提升，并登记
//! [`crate::app::portal_overlay`] 栈（Esc LIFO）。
//!
//! ## 用法
//! ```ignore
//! Element::from(
//!     dialog_panel(open_sig, "标题").child(move || ui! { <Label>"内容"</Label> })
//! )
//! ```
//!
//! ## 注意
//! - `open` 由组件内 [`crate::app::reactive::reactive_view`] 边界订阅，无需手动 `<Show>` / `<Reactive>`。
//! - `.child` 接收 `Fn() -> Element`，以便开闭时重复构建内容。
//! - z-index 由 portal_overlay 栈深度分配。

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::reactive_view;
use crate::core::reactivity::Signal;
use crate::core::tree::Element;
use crate::primitives::r#box::vstack;
use crate::primitives::text::text;
use crate::tokens;
use crate::utils::popup::portal_panel;

pub struct DialogBuilder {
    open: Signal<bool>,
    title: String,
    body: Option<Box<dyn Fn() -> Element>>,
    id: Option<String>,
}

/// 样式化模态面板；见模块文档。
pub fn dialog_panel(open: Signal<bool>, title: impl Into<String>) -> DialogBuilder {
    DialogBuilder {
        open,
        title: title.into(),
        body: None,
        id: None,
    }
}

impl DialogBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn child(mut self, build: impl Fn() -> Element + 'static) -> Self {
        self.body = Some(Box::new(build));
        self
    }
}

impl From<DialogBuilder> for Element {
    fn from(builder: DialogBuilder) -> Self {
        let DialogBuilder {
            open,
            title,
            body,
            id,
        } = builder;
        let id = id.unwrap_or_else(|| format!("dialog:{title}"));
        let view_id = format!("{id}__dialog_view");
        let template_id = id.clone();

        let mut root = reactive_view(view_id, move || {
            let is_open = open.get();
            let Some(oz) = portal_sync(&id, open, OverlayKind::Modal, OverlayMode::Stacked) else {
                return Element::new();
            };
            if !is_open {
                return Element::new();
            }

            let mut panel: Element = vstack()
                .gap("0.75rem")
                .p("1.25rem")
                .rounded("0.5rem")
                .max_w("28rem")
                .into();
            panel = panel.bg(tokens::background());
            panel.style.border_width = 1.0;
            panel.style.border_color = tokens::border();
            let title_el: Element = text(title.as_str()).size(18.0).weight(600.0).into();
            panel = panel.child(title_el);
            if let Some(build) = &body {
                panel = panel.child(build());
            }
            portal_panel(panel, oz.panel)
        });
        root.template_id = Some(template_id);
        root
    }
}
