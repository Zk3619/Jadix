//! Delayed hover open/close (HoverCard / Tooltip shared).
//!
//! UI-thread `Instant` deadlines — same shape as Solid `setTimeout` + `onCleanup`.
//! Not in `prelude`. Do not confuse with `lucide_animated`'s hover dirty map.

use std::cell::RefCell;
use std::collections::HashMap;
use web_time::{Duration, Instant};

use crate::app::runtime::request_redraw;
use crate::core::reactivity::{create_effect, on_cleanup, untrack, Signal};
use crate::tree::Element;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimerKind {
    Open,
    Close,
}

struct PendingTimer {
    at: Instant,
    kind: TimerKind,
    open: Signal<bool>,
    trigger_hovered: Signal<bool>,
    content_hovered: Signal<bool>,
}

thread_local! {
    static TIMERS: RefCell<HashMap<String, PendingTimer>> = RefCell::new(HashMap::new());
    static POINTER_TOOLTIPS: RefCell<HashMap<String, Signal<bool>>> = RefCell::new(HashMap::new());
    static POINTER_TOOLTIP_GRACE_UNTIL: RefCell<Option<Instant>> = const { RefCell::new(None) };
}

/// Default delay before a pointer hover opens a tooltip, hover card, or similar surface.
pub const DEFAULT_HOVER_OPEN_DELAY_MS: u64 = 300;

/// Disable the corresponding hover transition without scheduling a wakeup.
/// Explicit open state changes and keyboard focus remain available.
pub const NEVER_HOVER_DELAY_MS: u64 = u64::MAX;

const POINTER_TOOLTIP_GRACE: Duration = Duration::from_millis(DEFAULT_HOVER_OPEN_DELAY_MS);

fn pointer_tooltip_grace_active(until: Option<Instant>, now: Instant) -> bool {
    until.is_some_and(|deadline| deadline > now)
}

/// Track pointer-originated tooltip handoffs. Returns true when this tooltip
/// should bypass its configured open delay.
pub fn pointer_tooltip_enter(id: &str, open: Signal<bool>) -> bool {
    let now = Instant::now();
    let in_grace = POINTER_TOOLTIP_GRACE_UNTIL.with(|grace| {
        let mut grace = grace.borrow_mut();
        if !pointer_tooltip_grace_active(*grace, now) {
            *grace = None;
        }
        pointer_tooltip_grace_active(*grace, now)
    });
    let another_open = POINTER_TOOLTIPS.with(|tooltips| {
        tooltips
            .borrow()
            .iter()
            .any(|(other_id, other_open)| other_id != id && other_open.peek())
    });
    POINTER_TOOLTIPS.with(|tooltips| {
        tooltips.borrow_mut().insert(id.to_string(), open);
    });
    in_grace || another_open
}

/// End pointer ownership for one tooltip and begin the short handoff grace
/// period when the pointer leaves an open tooltip with no successor.
pub fn pointer_tooltip_leave(id: &str) {
    let was_open = POINTER_TOOLTIPS.with(|tooltips| {
        tooltips
            .borrow_mut()
            .remove(id)
            .is_some_and(|open| open.peek())
    });
    let another_open =
        POINTER_TOOLTIPS.with(|tooltips| tooltips.borrow().values().any(|open| open.peek()));
    if was_open && !another_open {
        POINTER_TOOLTIP_GRACE_UNTIL.with(|grace| {
            *grace.borrow_mut() = Some(Instant::now() + POINTER_TOOLTIP_GRACE);
        });
    }
}

/// Remove pointer state when a tooltip owner is disposed.
pub fn clear_pointer_tooltip(id: &str) {
    POINTER_TOOLTIPS.with(|tooltips| {
        tooltips.borrow_mut().remove(id);
    });
}

/// Stop a pending hover handoff when an explicit user action takes over.
pub fn end_pointer_tooltip_grace() {
    POINTER_TOOLTIP_GRACE_UNTIL.with(|grace| *grace.borrow_mut() = None);
}

/// Dual-region hover + delayed open/close signals for one overlay instance.
#[derive(Clone, Copy)]
pub struct HoverIntent {
    pub open: Signal<bool>,
    pub trigger_hovered: Signal<bool>,
    pub content_hovered: Signal<bool>,
    pub open_delay_ms: Signal<u64>,
    pub close_delay_ms: Signal<u64>,
}

impl HoverIntent {
    pub fn any_hovered(self) -> bool {
        self.trigger_hovered.get() || self.content_hovered.get()
    }

    pub fn is_open(self) -> bool {
        untrack(|| self.open.peek())
    }
}

/// Clear any pending open/close deadline for `id`.
pub fn clear(id: &str) {
    TIMERS.with(|m| {
        m.borrow_mut().remove(id);
    });
}

fn settle_open(open: Signal<bool>, trigger_hovered: Signal<bool>, content_hovered: Signal<bool>) {
    if (trigger_hovered.peek() || content_hovered.peek()) && !open.peek() {
        open.set(true);
    }
}

fn settle_close(open: Signal<bool>, trigger_hovered: Signal<bool>, content_hovered: Signal<bool>) {
    if !trigger_hovered.peek() && !content_hovered.peek() && open.peek() {
        open.set(false);
    }
}

fn arm(id: &str, delay_ms: u64, kind: TimerKind, intent: HoverIntent) {
    if delay_ms == NEVER_HOVER_DELAY_MS {
        clear(id);
        return;
    }
    if delay_ms == 0 {
        match kind {
            TimerKind::Open => {
                settle_open(intent.open, intent.trigger_hovered, intent.content_hovered)
            }
            TimerKind::Close => {
                settle_close(intent.open, intent.trigger_hovered, intent.content_hovered)
            }
        }
        clear(id);
        return;
    }
    let Some(at) = Instant::now().checked_add(Duration::from_millis(delay_ms)) else {
        clear(id);
        return;
    };
    TIMERS.with(|m| {
        m.borrow_mut().insert(
            id.to_string(),
            PendingTimer {
                at,
                kind,
                open: intent.open,
                trigger_hovered: intent.trigger_hovered,
                content_hovered: intent.content_hovered,
            },
        );
    });
    request_redraw();
}

/// Schedule open after `open_delay_ms` (0 = immediate; [`NEVER_HOVER_DELAY_MS`] = never).
pub fn arm_open(id: &str, intent: HoverIntent) {
    arm(id, intent.open_delay_ms.peek(), TimerKind::Open, intent);
}

/// Schedule close after `close_delay_ms` (0 = immediate; [`NEVER_HOVER_DELAY_MS`] = never).
pub fn arm_close(id: &str, intent: HoverIntent) {
    arm(id, intent.close_delay_ms.peek(), TimerKind::Close, intent);
}

/// Solid-style effect: watch hover intent and arm open/close timers.
pub fn bind_hover_intent_effect(id: &str, intent: HoverIntent) {
    let eff_id = id.to_string();
    create_effect(move || {
        let want = intent.any_hovered();
        let _od = intent.open_delay_ms.get();
        let _cd = intent.close_delay_ms.get();
        let is_open = intent.is_open();
        clear(&eff_id);
        if want {
            if !is_open {
                arm_open(&eff_id, intent);
            }
        } else if is_open {
            arm_close(&eff_id, intent);
        }
    });
    let cleanup_id = id.to_string();
    on_cleanup(move || {
        clear(&cleanup_id);
    });
}

/// Settle expired timers; called from lifecycle. Returns whether `open` changed.
pub fn tick_hover_intent() -> bool {
    let now = Instant::now();
    POINTER_TOOLTIP_GRACE_UNTIL.with(|grace| {
        if !pointer_tooltip_grace_active(*grace.borrow(), now) {
            *grace.borrow_mut() = None;
        }
    });
    let due: Vec<(String, PendingTimer)> = TIMERS.with(|m| {
        let mut map = m.borrow_mut();
        let keys: Vec<String> = map
            .iter()
            .filter(|(_, t)| t.at <= now)
            .map(|(k, _)| k.clone())
            .collect();
        keys.into_iter()
            .filter_map(|k| map.remove(&k).map(|t| (k, t)))
            .collect()
    });
    let mut changed = false;
    for (_id, t) in due {
        match t.kind {
            TimerKind::Open => {
                if (t.trigger_hovered.peek() || t.content_hovered.peek()) && !t.open.peek() {
                    t.open.set(true);
                    changed = true;
                }
            }
            TimerKind::Close => {
                if !t.trigger_hovered.peek() && !t.content_hovered.peek() && t.open.peek() {
                    t.open.set(false);
                    changed = true;
                }
            }
        }
    }
    changed
}

/// Next Instant when the event loop should wake.
pub fn hover_intent_next_wake_at() -> Option<Instant> {
    let timer_wake = TIMERS.with(|m| m.borrow().values().map(|t| t.at).min());
    let grace_wake = POINTER_TOOLTIP_GRACE_UNTIL.with(|grace| *grace.borrow());
    timer_wake.into_iter().chain(grace_wake).min()
}

pub fn set_hovered(sig: Signal<bool>, v: bool) {
    if sig.peek() != v {
        sig.set(v);
    }
}

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

#[cfg(test)]
#[path = "tests/hover_intent.rs"]
mod tests;
