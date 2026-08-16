//! WP-3.14：`#[component]` / [`component_root`] 实例 Owner；`untrack` vs `isolate_reads` 锚点契约。
#![allow(non_snake_case)]

use crate::app::reactive::{reactive_anchor, registered_anchor_count};
use crate::core::reactivity::{
    component_root, create_context, create_effect, create_root, create_signal, isolate_reads,
    provide_context, untrack, use_context, Context, Signal,
};
use crate::core::tree::Element;
use rui_macros::component;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;

fn probe_ctx() -> Context<String> {
    static CTX: OnceLock<Context<String>> = OnceLock::new();
    *CTX.get_or_init(create_context)
}

thread_local! {
    static SEEN: RefCell<HashMap<String, Option<String>>> = RefCell::new(HashMap::new());
}

fn take_seen(key: &str) -> Option<String> {
    SEEN.with(|s| s.borrow().get(key).cloned().flatten())
}

#[component(crate_path = "crate")]
fn ProbeSlot(label: String, tick: Signal<u32>) -> Element {
    let ctx = probe_ctx();
    provide_context(ctx, label.clone());
    let key = label.clone();
    create_effect(move || {
        let _ = tick.get();
        SEEN.with(|s| {
            s.borrow_mut().insert(key.clone(), use_context(ctx));
        });
    });
    Element::new()
}

/// A：`#[component]` 兄弟实例 `provide_context` 互不覆盖；组件体**不**手写 `with_child_owner`。
#[test]
fn component_macro_sibling_context_isolated_without_manual_owner() {
    SEEN.with(|s| s.borrow_mut().clear());
    create_root(|| {
        let tick = create_signal(0u32);
        let _ = ProbeSlot(ProbeSlotProps {
            label: Some("hour".into()),
            tick: Some(tick),
            children: None,
            base: None,
        });
        let _ = ProbeSlot(ProbeSlotProps {
            label: Some("second".into()),
            tick: Some(tick),
            children: None,
            base: None,
        });
        tick.set(1);
    });
    assert_eq!(take_seen("hour").as_deref(), Some("hour"));
    assert_eq!(take_seen("second").as_deref(), Some("second"));
}

/// A′：纯 Builder 根走同一 [`component_root`]。
#[test]
fn component_root_sibling_context_isolated() {
    SEEN.with(|s| s.borrow_mut().clear());
    create_root(|| {
        let tick = create_signal(0u32);
        for label in ["hour", "second"] {
            let label = label.to_string();
            component_root({
                let tick = tick;
                move || {
                    let ctx = probe_ctx();
                    provide_context(ctx, label.clone());
                    let key = label.clone();
                    create_effect(move || {
                        let _ = tick.get();
                        SEEN.with(|s| {
                            s.borrow_mut().insert(key.clone(), use_context(ctx));
                        });
                    });
                    Element::new()
                }
            });
        }
        tick.set(1);
    });
    assert_eq!(take_seen("hour").as_deref(), Some("hour"));
    assert_eq!(take_seen("second").as_deref(), Some("second"));
}

/// A″：`untrack` 不注册 `reactive_anchor`；`isolate_reads` 注册（升格自 date_picker）。
#[test]
fn untrack_skips_anchor_but_isolate_reads_registers() {
    create_root(|| {
        let before = registered_anchor_count();
        untrack(|| {
            reactive_anchor("wp314_untrack_anchor", || Element::new());
        });
        assert_eq!(
            registered_anchor_count(),
            before,
            "untrack 关掉 tracking，浮层 Calendar/Combobox 无法注册"
        );
        create_effect(|| {
            isolate_reads(|| {
                reactive_anchor("wp314_isolate_anchor", || Element::new());
            });
        });
        assert!(
            registered_anchor_count() > before,
            "isolate_reads 必须仍能注册 reactive_anchor"
        );
    });
}
