//! # Dialog（面板）
//!
//! 样式化模态内容面板：标题 + body。遮罩 / 居中可由调用方组合；
//! 面板本身已 [`portal_panel`](crate::utils::popup::portal_panel) 提升，并登记
//! [`crate::app::portal_overlay`] 栈（Esc LIFO）。
//!
//! ## 用法
//! ```ignore
//! Element::from(
//!     dialog_panel(open_sig, "标题").child(ui! { <Label>"内容"</Label> })
//! )
//! // 建议：<Show when={open.get()}> + 绝对定位遮罩
//! ```
//!
//! ## 注意
//! - 构建时 `portal_sync`（`Modal`）；可见性由父级 `Show` / Reactive 控制。
//! - z-index 由 portal_overlay 栈深度分配。

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::core::reactivity::Signal;
use crate::core::tree::Element;
use crate::primitives::r#box::vstack;
use crate::primitives::text::text;
use crate::tokens;
use crate::utils::popup::portal_panel;

pub struct DialogBuilder {
    open: Signal<bool>,
    title: String,
    body: Option<Element>,
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

    pub fn child(mut self, e: impl Into<Element>) -> Self {
        self.body = Some(e.into());
        self
    }
}

impl From<DialogBuilder> for Element {
    fn from(builder: DialogBuilder) -> Self {
        let id = builder
            .id
            .unwrap_or_else(|| format!("dialog:{}", builder.title));
        let z = portal_sync(id, builder.open, OverlayKind::Modal, OverlayMode::Stacked)
            .map(|z| z.panel)
            .unwrap_or(1000);
        let mut panel: Element = vstack()
            .gap("0.75rem")
            .p("1.25rem")
            .rounded("0.5rem")
            .max_w("28rem")
            .into();
        panel = panel.bg(tokens::background());
        panel.style.border_width = 1.0;
        panel.style.border_color = tokens::border();
        let title_el: Element = text(builder.title.as_str()).size(18.0).weight(600.0).into();
        panel = panel.child(title_el);
        if let Some(body) = builder.body {
            panel = panel.child(body);
        }
        portal_panel(panel, z)
    }
}
