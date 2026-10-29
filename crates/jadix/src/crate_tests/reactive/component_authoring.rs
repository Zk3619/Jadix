//! WP-3.14：`#[component]` / [`component_root`] 实例 Owner；`untrack` vs `isolate_reads` 锚点契约。
//! Wave 4 (N.3)：默认 `#[component]` 体走 [`component_isolated_body`]。
#![allow(non_snake_case)]

use crate::app::reactive::{
    bind_text, reactive_anchor, registered_anchor_count, reset_registered_anchors,
};
use crate::core::reactivity::{
    component_root, create_context, create_effect, create_root, create_signal, dispose_effect,
    isolate_reads, isolated_discarded_read_count, provide_context,
    reset_isolated_discarded_read_count, take_flushed_effects, untrack, use_context, Context,
    Signal,
};
use crate::core::tree::Element;
use jadix_macros::component;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
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
            reactive_anchor("wp314_untrack_anchor", Element::new);
        });
        assert_eq!(
            registered_anchor_count(),
            before,
            "untrack 关掉 tracking，浮层 Calendar/Combobox 无法注册"
        );
        create_effect(|| {
            isolate_reads(|| {
                reactive_anchor("wp314_isolate_anchor", Element::new);
            });
        });
        assert!(
            registered_anchor_count() > before,
            "isolate_reads 必须仍能注册 reactive_anchor"
        );
    });
}

thread_local! {
    static LEAK_BODY: Cell<u32> = const { Cell::new(0) };
    static BIND_BODY: Cell<u32> = const { Cell::new(0) };
    static BIND_LEAF: Cell<u32> = const { Cell::new(0) };
}

#[component(crate_path = "crate")]
fn LeakLabel(n: Signal<i32>) -> Element {
    LEAK_BODY.with(|c| c.set(c.get() + 1));
    Element::new().text(format!("{}", n.get()))
}

#[component(crate_path = "crate")]
fn BoundLabel(n: Signal<i32>) -> Element {
    BIND_BODY.with(|c| c.set(c.get() + 1));
    bind_text("n3_bound_label", move || {
        BIND_LEAF.with(|c| c.set(c.get() + 1));
        n.get().to_string()
    })
}

#[component(crate_path = "crate")]
fn PeekLabel(n: Signal<i32>) -> Element {
    Element::new().text(format!("{}", n.peek()))
}

/// Wave 4：默认 `#[component]` 体内裸 `get()` 丢边，不订父 effect；文本卡住。
#[test]
fn default_component_bare_get_does_not_rerun_body() {
    reset_registered_anchors();
    reset_isolated_discarded_read_count();
    LEAK_BODY.with(|c| c.set(0));
    let n = create_signal(1_i32);
    let kept = Rc::new(RefCell::new(None::<Element>));
    let kept_e = Rc::clone(&kept);

    let root = create_effect(move || {
        let el = LeakLabel(LeakLabelProps {
            n: Some(n),
            children: None,
            base: None,
        });
        *kept_e.borrow_mut() = Some(el);
    });

    assert_eq!(LEAK_BODY.with(|c| c.get()), 1, "body runs once at mount");
    assert!(
        isolated_discarded_read_count() >= 1,
        "bare get inside default #[component] must count as a discarded read (G.2)"
    );
    assert_eq!(
        kept.borrow()
            .as_ref()
            .and_then(|e| e.text.clone())
            .as_deref(),
        Some("1")
    );

    let _ = take_flushed_effects();
    n.set(2);
    assert_eq!(
        LEAK_BODY.with(|c| c.get()),
        1,
        "leaf set must not re-run default #[component] body"
    );
    assert!(
        take_flushed_effects().is_empty(),
        "discarded edge must not re-run the parent effect"
    );
    assert_eq!(
        kept.borrow()
            .as_ref()
            .and_then(|e| e.text.clone())
            .as_deref(),
        Some("1"),
        "UI stays stuck — that is the leaked-bind failure mode"
    );

    drop(kept.borrow_mut().take());
    dispose_effect(root);
    reset_registered_anchors();
    reset_isolated_discarded_read_count();
}

/// Wave 4：`bind_text` 仍订自己的 Render effect；组件体不因叶子 `set` 重跑。
#[test]
fn default_component_bind_text_updates_without_body_rerun() {
    reset_registered_anchors();
    reset_isolated_discarded_read_count();
    BIND_BODY.with(|c| c.set(0));
    BIND_LEAF.with(|c| c.set(0));
    let n = create_signal(1_i32);
    let kept = Rc::new(RefCell::new(None::<Element>));
    let kept_e = Rc::clone(&kept);

    let root = create_effect(move || {
        let el = BoundLabel(BoundLabelProps {
            n: Some(n),
            children: None,
            base: None,
        });
        *kept_e.borrow_mut() = Some(el);
    });

    assert_eq!(BIND_BODY.with(|c| c.get()), 1, "body runs once at mount");
    assert_eq!(
        isolated_discarded_read_count(),
        0,
        "lowered bind_text must subscribe its own Render effect, not the isolate frame"
    );
    let after_mount = BIND_LEAF.with(|c| c.get());
    assert!(after_mount >= 1, "bind_text compute runs at mount");

    let _ = take_flushed_effects();
    n.set(2);
    assert_eq!(
        BIND_BODY.with(|c| c.get()),
        1,
        "leaf set must not re-run default #[component] body"
    );
    assert!(
        BIND_LEAF.with(|c| c.get()) > after_mount,
        "bind_text leaf must still re-run on set"
    );
    assert!(
        take_flushed_effects()
            .iter()
            .any(|(_, phase)| *phase == crate::core::reactivity::EffectPhase::Render),
        "leaf set must flush the bind Render effect"
    );

    drop(kept.borrow_mut().take());
    dispose_effect(root);
    reset_registered_anchors();
    reset_isolated_discarded_read_count();
}

/// Wave 4：`peek` 不丢边、不报警。
#[test]
fn default_component_peek_does_not_count_as_discarded() {
    reset_isolated_discarded_read_count();
    let n = create_signal(1_i32);
    let root = create_effect(move || {
        let _ = PeekLabel(PeekLabelProps {
            n: Some(n),
            children: None,
            base: None,
        });
    });
    assert_eq!(
        isolated_discarded_read_count(),
        0,
        "peek must not warn / count as a discarded read"
    );
    dispose_effect(root);
    reset_isolated_discarded_read_count();
}
