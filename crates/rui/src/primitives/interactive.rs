//! 交互原语：包装任意 [`Element`]，为其注册指针/键盘/焦点行为（`on_click`、`on_hover`、`on_key_down`、`focusable` 等）。
//!
//! RSX：`<Interactive>` 从空节点起链；复合控件通常先 `r#box()` 搭视觉，再 `interactive(...)` 接事件。

use std::cell::Cell;

use crate::core::tree::Element;
use winit::event::KeyEvent;

thread_local! {
    /// 按钮等控件用 `set_silent` 更新 hover 后需补一帧绘制（不再经由根 `create_effect`）。
    static HOVER_PAINT_DIRTY: Cell<bool> = const { Cell::new(false) };
}

#[inline]
pub(crate) fn mark_hover_paint_dirty() {
    HOVER_PAINT_DIRTY.set(true);
}

#[inline]
pub(crate) fn take_hover_paint_dirty() -> bool {
    HOVER_PAINT_DIRTY.replace(false)
}

pub struct InteractiveBuilder {
    pub(crate) el: Element,
}

impl InteractiveBuilder {
    pub fn new(el: Element) -> Self {
        Self { el }
    }

    pub fn on_click(mut self, handler: impl FnMut() + 'static) -> Self {
        self.el = self.el.on_click(handler);
        self
    }

    pub fn on_mousedown(
        mut self,
        handler: impl FnMut(&crate::app::pointer_events::MouseEvent) + 'static,
    ) -> Self {
        self.el = self.el.on_mousedown(handler);
        self
    }

    pub fn on_mouseup(
        mut self,
        handler: impl FnMut(&crate::app::pointer_events::MouseEvent) + 'static,
    ) -> Self {
        self.el = self.el.on_mouseup(handler);
        self
    }

    pub fn on_mousemove(
        mut self,
        handler: impl FnMut(&crate::app::pointer_events::MouseEvent) + 'static,
    ) -> Self {
        self.el = self.el.on_mousemove(handler);
        self
    }

    pub fn on_mouseover(
        mut self,
        handler: impl FnMut(&crate::app::pointer_events::MouseEvent) + 'static,
    ) -> Self {
        self.el = self.el.on_mouseover(handler);
        self
    }

    pub fn on_mouseout(
        mut self,
        handler: impl FnMut(&crate::app::pointer_events::MouseEvent) + 'static,
    ) -> Self {
        self.el = self.el.on_mouseout(handler);
        self
    }

    pub fn on_dblclick(
        mut self,
        handler: impl FnMut(&crate::app::pointer_events::MouseEvent) + 'static,
    ) -> Self {
        self.el = self.el.on_dblclick(handler);
        self
    }

    pub fn on_hover(mut self, handler: impl FnMut(bool) + 'static) -> Self {
        self.el = self.el.on_hover(handler);
        self
    }

    pub fn on_key_down(mut self, handler: impl FnMut(KeyEvent) -> bool + 'static) -> Self {
        self.el = self.el.on_key_down(handler);
        self
    }

    pub fn focus_id(mut self, id: impl Into<String>) -> Self {
        self.el = self.el.focus_id(id);
        self
    }

    pub fn cursor(mut self, value: impl crate::style::IntoCursor) -> Self {
        self.el = self.el.cursor(value);
        self
    }

    pub fn cursor_kind(mut self, c: crate::core::tree::CursorKind) -> Self {
        self.el = self.el.cursor_kind(c);
        self
    }

    /// Keyboard-focusable. Also enables the shared `:focus-visible` ring (same chrome as Input).
    /// Call [`Self::focus_ring`]`(false)` afterwards to opt out.
    pub fn focusable(mut self, focusable: bool) -> Self {
        self.el = self.el.focusable(focusable);
        self
    }

    /// Shared keyboard focus-visible ring (same chrome as Input; not shown on mouse focus).
    /// Default-on with [`Self::focusable`]`(true)`.
    pub fn focus_ring(mut self, enabled: bool) -> Self {
        self.el = self.el.focus_ring(enabled);
        self
    }

    /// Focus-ring thickness in logical px. Overrides the theme default.
    pub fn ring_width(mut self, width: impl crate::core::tree::IntoF32) -> Self {
        self.el = self.el.ring_width(width);
        self
    }

    /// Focus-ring color. Tokens / hex / `[f32; 4]`. Overrides theme token `ring`.
    pub fn ring_color(mut self, color: impl crate::style::IntoPaintColor) -> Self {
        self.el = self.el.ring_color(color);
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.el = self.el.child(child);
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        for c in children {
            self = self.child(c);
        }
        self
    }

    pub fn text(mut self, text: impl crate::core::tree::IntoNodeText) -> Self {
        self.el = self.el.text(text);
        self
    }
}

impl From<InteractiveBuilder> for Element {
    fn from(builder: InteractiveBuilder) -> Self {
        builder.el
    }
}

impl crate::core::tree::IntoElementList for InteractiveBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

pub fn interactive(el: Element) -> InteractiveBuilder {
    InteractiveBuilder::new(el)
}
