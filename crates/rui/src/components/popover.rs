//! # Popover
//!
//! 非模态浮层面板（边框 / 圆角）。`ui!` 中 `<Popover open={…}>` 由宏编成 [`popover_panel`]。
//! 可选 `anchor_focus_id` + `placement` 走统一 floating sync。
//!
//! ## 用法
//! ```ignore
//! <Popover id="demo-popover" open={pop_open}>
//!   <Label>"浮层内容"</Label>
//! </Popover>
//! ```

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::reactive_view;
use crate::core::reactivity::Signal;
use crate::core::tree::Element;
use crate::primitives::r#box::r#box;
use crate::utils::popup::{mark_floating, panel_block_clicks, PopupPlacement};

pub struct PopoverBuilder {
    open: Signal<bool>,
    body: Option<Box<dyn Fn() -> Element>>,
    id: Option<String>,
    z_override: Option<i32>,
    anchor_focus_id: Option<String>,
    placement: PopupPlacement,
}

/// 非模态浮层面板；`open` 为 false 时为空。见模块文档。
pub fn popover_panel(open: Signal<bool>) -> PopoverBuilder {
    PopoverBuilder {
        open,
        body: None,
        id: None,
        z_override: None,
        anchor_focus_id: None,
        placement: PopupPlacement::BottomStart,
    }
}

impl PopoverBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn child(mut self, build: impl Fn() -> Element + 'static) -> Self {
        self.body = Some(Box::new(build));
        self
    }

    pub fn z_index(mut self, z: i32) -> Self {
        self.z_override = Some(z);
        self
    }

    pub fn anchor_focus_id(mut self, fid: impl Into<String>) -> Self {
        self.anchor_focus_id = Some(fid.into());
        self
    }

    pub fn placement(mut self, p: PopupPlacement) -> Self {
        self.placement = p;
        self
    }
}

impl From<PopoverBuilder> for Element {
    fn from(builder: PopoverBuilder) -> Self {
        let PopoverBuilder {
            open,
            body,
            id,
            z_override,
            anchor_focus_id,
            placement,
        } = builder;
        let id = id.unwrap_or_else(|| "popover".into());
        let view_id = format!("{id}__popover_view");

        reactive_view(view_id, move || {
            let is_open = open.get();
            let Some(oz) = portal_sync(&id, open, OverlayKind::Popover, OverlayMode::Stacked)
            else {
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
            if let Some(build) = &body {
                inner = inner.child(build());
            }
            if let Some(anchor) = &anchor_focus_id {
                mark_floating(&mut inner, &id, anchor, placement);
                inner = inner.absolute();
            }
            let z = z_override.unwrap_or(oz.panel);
            panel_block_clicks(inner, z)
        })
    }
}
