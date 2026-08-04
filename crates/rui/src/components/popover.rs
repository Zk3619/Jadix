//! # Popover
//!
//! 非模态浮层面板（边框 / 圆角）。`ui!` 中 `<Popover open={…}>` 由宏编成 [`popover_panel`]。
//! **无锚点自动定位**（与 Select 不同）；位置由子树样式自行决定。
//!
//! ## 用法
//! ```ignore
//! <Reactive>
//!   <Popover open={pop_open}>
//!     <Label>"浮层内容"</Label>
//!   </Popover>
//! </Reactive>
//! ```
//!
//! ## 注意
//! - `open.get()==false` 时返回空 Element，并从 overlay 栈弹出。
//! - Esc 走 [`crate::app::portal_overlay`] LIFO；面板 portal + `panel_block_clicks`。
//! - z-index 默认由栈分配；可用 `.z_index` 覆盖 panel z。

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::core::reactivity::Signal;
use crate::core::tree::Element;
use crate::primitives::r#box::r#box;
use crate::utils::popup::panel_block_clicks;

pub struct PopoverBuilder {
    open: Signal<bool>,
    body: Option<Element>,
    id: Option<String>,
    z_override: Option<i32>,
}

/// 非模态浮层面板；`open` 为 false 时为空。见模块文档。
pub fn popover_panel(open: Signal<bool>) -> PopoverBuilder {
    PopoverBuilder {
        open,
        body: None,
        id: None,
        z_override: None,
    }
}

impl PopoverBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn child(mut self, e: impl Into<Element>) -> Self {
        self.body = Some(e.into());
        self
    }

    pub fn z_index(mut self, z: i32) -> Self {
        self.z_override = Some(z);
        self
    }
}

impl From<PopoverBuilder> for Element {
    fn from(builder: PopoverBuilder) -> Self {
        let id = builder.id.unwrap_or_else(|| "popover".into());
        let is_open = builder.open.get();
        let Some(oz) = portal_sync(id, builder.open, OverlayKind::Popover, OverlayMode::Stacked) else {
            return Element::new();
        };
        if !is_open {
            return Element::new();
        }
        let mut inner: Element = r#box()
            .p("0.75rem")
            .rounded("0.375rem")
            .max_w("20rem")
            .into();
        inner.style.border_width = 1.0;
        inner.style.border_color = crate::tokens::border();
        inner = inner.bg(crate::tokens::background());
        if let Some(body) = builder.body {
            inner = inner.child(body);
        }
        let z = builder.z_override.unwrap_or(oz.panel);
        panel_block_clicks(inner, z)
    }
}
