//! Headless popup helpers: Escape 关闭栈、`open` 信号注册、全屏点击 underlay。
use std::cell::RefCell;

use crate::core::reactivity::Signal;
use crate::core::tree::Element;
use crate::primitives::r#box::r#box;
use crate::primitives::interactive::interactive;

thread_local! {
    /// Last-registered popup `open` signal (Escape closes the most recently registered).
    static LAST_POPUP_OPEN: RefCell<Option<Signal<bool>>> = RefCell::new(None);
}

/// Call when building a dialog/dropdown so [`try_close_top_popup`] can clear `open`.
pub fn register_last_popup_for_escape(open: Signal<bool>) {
    LAST_POPUP_OPEN.with(|s| {
        *s.borrow_mut() = Some(open);
    });
}

/// Sets `open` to false if it was true. Returns whether a popup was closed.
pub fn try_close_top_popup() -> bool {
    LAST_POPUP_OPEN.with(|s| {
        let borrow = s.borrow();
        let Some(sig) = borrow.as_ref() else {
            return false;
        };
        if sig.get() {
            sig.set(false);
            true
        } else {
            false
        }
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PopupPlacement {
    #[default]
    BottomStart,
    BottomEnd,
    TopStart,
    TopEnd,
}

/// Full-viewport underlay: click closes popup. Combine with high `z_index` on siblings.
pub fn underlay_close_on_click(open: Signal<bool>) -> Element {
    let mut el: Element = interactive(
        r#box()
            .w("100%")
            .h("100%")
            .into(),
    )
    .on_click(move || open.set(false))
    .into();
    el.style.z_index = 900;
    el
}

/// Panel wrapper that stops hit-test from reaching underlay (`on_click` noop).
pub fn panel_block_clicks(mut inner: Element, z: i32) -> Element {
    inner.style.z_index = z;
    inner.on_click(|| {})
}
