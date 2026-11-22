//! GPU event-loop adapter for shared hover intent.
use crate::{app::runtime::request_redraw, tree::Element};
pub use jadix_core::hover_intent::{clear, clear_pointer_tooltip, end_pointer_tooltip_grace, pointer_tooltip_enter, pointer_tooltip_leave, tick_hover_intent, hover_intent_next_wake_at, set_hovered, HoverIntent, DEFAULT_HOVER_OPEN_DELAY_MS, NEVER_HOVER_DELAY_MS};

pub fn arm_open(id: &str, intent: HoverIntent) { jadix_core::hover_intent::arm_open(id, intent); request_redraw(); }
pub fn arm_close(id: &str, intent: HoverIntent) { jadix_core::hover_intent::arm_close(id, intent); request_redraw(); }
pub fn bind_hover_intent_effect(id: &str, intent: HoverIntent) { jadix_core::hover_intent::bind_hover_intent_effect_with_wake(id, intent, request_redraw); }

pub fn chain_region_hover(
    mut el: Element,
    on_enter: impl Fn() + 'static,
    on_leave: impl Fn() + 'static,
) -> Element {
    let prev = el.on_hover.clone();
    el = el.on_hover(move |hovered| {
        if let Some(ref rc) = prev {
            rc.borrow_mut()(hovered);
        }
        if hovered {
            on_enter();
        } else {
            on_leave();
        }
    });
    el
}

