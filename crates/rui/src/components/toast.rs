//! # Toast
//!
//! 全局 toast 队列 + 宿主；Pinned portal 右上角。

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::reactive_view;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::r#box::{r#box, vstack};
use crate::primitives::text::text;
use crate::tokens;
use crate::utils::popup::portal_panel;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct ToastItem {
    pub id: u64,
    pub message: String,
    pub created: Instant,
    pub duration: Duration,
}

impl PartialEq for ToastItem {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.message == other.message
    }
}

const TOAST_QUEUE_KEY: &str = "__rui_toast_queue";
const TOAST_SEQ_KEY: &str = "__rui_toast_seq";
const TOAST_HOST_OPEN: &str = "__rui_toast_host_open";

pub fn toast_queue() -> Signal<Vec<ToastItem>> {
    use_signal(TOAST_QUEUE_KEY, Vec::<ToastItem>::new)
}

/// Push a toast; returns id.
pub fn toast_push(message: impl Into<String>, duration_ms: u64) -> u64 {
    let seq = use_signal(TOAST_SEQ_KEY, || 0_u64);
    let id = seq.get() + 1;
    seq.set(id);
    let mut q = toast_queue().get();
    q.push(ToastItem {
        id,
        message: message.into(),
        created: Instant::now(),
        duration: Duration::from_millis(duration_ms.max(500)),
    });
    toast_queue().set(q);
    use_signal(TOAST_HOST_OPEN, || true).set(true);
    id
}

pub fn toast_dismiss(id: u64) {
    let mut q = toast_queue().get();
    q.retain(|t| t.id != id);
    toast_queue().set(q);
}

fn prune_expired(q: &mut Vec<ToastItem>) -> bool {
    let before = q.len();
    q.retain(|t| t.created.elapsed() < t.duration);
    q.len() != before
}

/// Visual toast strip (legacy API).
pub fn toast(message: impl Into<String>) -> Element {
    let msg: String = message.into();
    let mut el: Element = r#box()
        .px("1rem")
        .py("0.5rem")
        .rounded("0.375rem")
        .mt("0.75rem")
        .into();
    el.style.z_index = 1100;
    el = el.bg(tokens::primary());
    let label: Element = text(msg.as_str())
        .color(tokens::primary_foreground_text())
        .size(0.875 * 16.0)
        .into();
    el.child(label)
}

/// Host that renders the global toast queue (place near app root).
///
/// Returns an **absolute** overlay (`inset: 0`) so the placeholder’s default
/// `w/h: 100%` does not participate in flex sizing. Otherwise a root
/// `HStack { sidebar, ScrollArea, toast_host }` splits the main pane with the
/// toast host and leaves a dead empty column (gallery regression).
pub fn toast_host() -> Element {
    let open = use_signal(TOAST_HOST_OPEN, || true);
    let host = reactive_view("__rui_toast_host_view", move || {
        let mut q = toast_queue().get();
        if prune_expired(&mut q) {
            toast_queue().set(q.clone());
        }
        let _ = open.get();
        let Some(oz) = portal_sync(
            "toast-host",
            open,
            OverlayKind::Tooltip,
            OverlayMode::Pinned,
        ) else {
            return Element::new();
        };
        if q.is_empty() {
            return Element::new();
        }
        let mut stack = vstack().gap(8.0);
        for item in q.iter().rev().take(5) {
            let id = item.id;
            let mut row = toast(item.message.as_str());
            row = row.on_click(move || toast_dismiss(id));
            stack = stack.child(row);
        }
        let mut panel: Element = r#box().child(stack).into();
        panel = panel.absolute().right(16.0).top(16.0);
        portal_panel(panel, oz.panel)
    });
    host.absolute().inset(0.0).w("100%").h("100%")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toast_push_and_dismiss() {
        let id = toast_push("hi", 5000);
        assert!(toast_queue().get().iter().any(|t| t.id == id));
        toast_dismiss(id);
        assert!(!toast_queue().get().iter().any(|t| t.id == id));
    }
}
