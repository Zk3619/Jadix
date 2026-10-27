//! 交互原语：包装任意 [`Element`]，为其注册指针/键盘/焦点/`disabled` 行为（`on_click`、`on_hover`、`on_key_down`、`focusable` 等）。
//!
//! RSX：`<Interactive>` 从空节点起链；复合控件通常先 `r#box()` 搭视觉，再 `interactive(...)` 接事件。
//! `disabled` 是本原语（[`apply_disabled`]）；组件签名仍声明领域状态，live RSX 由宏 `bind_attr`。
//! 分层约定见 `docs/COMPONENT_LIBRARY.md` §1.2。

use std::cell::Cell;

use crate::core::tree::Element;
use winit::event::KeyEvent;

thread_local! {
    /// 按钮等控件用 `set_silent` 更新 hover 后需补一帧绘制（不再经由根 `create_effect`）。
    static HOVER_PAINT_DIRTY: Cell<bool> = const { Cell::new(false) };
}

#[inline]
pub fn mark_hover_paint_dirty() {
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

    /// Domain disabled: HTML attr, focus, and in-place paint descriptors.
    /// Live RSX `disabled={sig.get()}` uses [`apply_disabled`] via `bind_attr`.
    pub fn disabled(mut self, disabled: bool) -> Self {
        let _ = apply_disabled(&mut self.el, disabled);
        if disabled {
            self.el = self.el.cursor_not_allowed();
        }
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

/// Interactive primitive: stamp `disabled`, focus, and paint-sync flags in place.
///
/// Button hover chrome and Input/Textarea glyphs read these fields each frame;
/// callers do not rebuild the tree. [`crate::app::reactive::bind_attr`] uses this
/// for every RSX `disabled={…}` on a `#[component]`.
pub fn apply_disabled(el: &mut Element, disabled: bool) -> crate::core::tree::LayoutStatus {
    use crate::core::tree::{CursorKind, LayoutStatus};

    let mut changed = false;

    let has_attr = el.attrs.get("disabled").map(String::as_str) == Some("true");
    if disabled != has_attr {
        if disabled {
            el.set_attr("disabled", "true");
        } else {
            el.attrs.remove("disabled");
        }
        changed = true;
    }

    let interactive = el.disabled_sync.is_some() || el.input_paint_sync.is_some();
    if interactive {
        if el.focusable == disabled {
            el.set_focusable(!disabled);
            changed = true;
        }
    } else if disabled && el.focusable {
        el.set_focusable(false);
        changed = true;
    }

    if let Some(sync) = el.disabled_sync {
        changed |= sync(el, disabled);
    }

    if let Some(sync) = el.input_paint_sync.as_mut() {
        if sync.disabled != disabled {
            sync.disabled = disabled;
            changed = true;
        }
        let want: crate::core::tree::CursorValue = if disabled {
            CursorKind::NotAllowed.into()
        } else {
            CursorKind::Text.into()
        };
        if el.cursor.as_ref() != Some(&want) {
            el.cursor = Some(want);
            changed = true;
        }
    }

    if changed {
        LayoutStatus::StyleChanged
    } else {
        LayoutStatus::Clean
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tree::{CursorKind, Element, LayoutStatus};
    use crate::primitives::r#box::hstack;

    #[test]
    fn apply_disabled_stamps_attr_and_focus() {
        let mut el: Element = hstack().into();
        el.set_focusable(true);
        assert_eq!(apply_disabled(&mut el, true), LayoutStatus::StyleChanged);
        assert_eq!(el.attrs.get("disabled").map(String::as_str), Some("true"));
        assert!(!el.focusable);
        assert_eq!(apply_disabled(&mut el, true), LayoutStatus::Clean);
        assert_eq!(apply_disabled(&mut el, false), LayoutStatus::StyleChanged);
        assert!(!el.attrs.contains_key("disabled"));
        assert!(
            !el.focusable,
            "generic nodes do not gain focus just because disabled is cleared"
        );
    }

    #[test]
    fn apply_disabled_restores_focus_on_paint_sync_nodes() {
        let mut el: Element = hstack().into();
        el.disabled_sync = Some(|element, disabled| {
            let changed = element.attrs.get("test_disabled").is_some() != disabled;
            if disabled {
                element.set_attr("test_disabled", "true");
            } else {
                element.attrs.remove("test_disabled");
            }
            changed
        });
        el.set_focusable(true);
        assert_eq!(apply_disabled(&mut el, true), LayoutStatus::StyleChanged);
        assert!(!el.focusable);
        assert!(el.attrs.contains_key("test_disabled"));
        assert_eq!(apply_disabled(&mut el, false), LayoutStatus::StyleChanged);
        assert!(el.focusable);
        assert!(!el.attrs.contains_key("test_disabled"));
    }

    #[test]
    fn interactive_builder_disabled_sets_not_allowed_cursor() {
        let el: Element = interactive(hstack().into()).disabled(true).into();
        assert_eq!(el.attrs.get("disabled").map(String::as_str), Some("true"));
        assert_eq!(el.cursor, Some(CursorKind::NotAllowed.into()));
    }
}
