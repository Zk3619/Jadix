//! WP-R.7 / G8: `ui!` auto-lowering under `isolate_reads` / `#[component]`.
//! Naked `.get()` that is not lowered is discarded by isolate_reads → UI never updates.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use rui::app::reactive::{
    keyed_list_item_owner_count, registered_binding_count, reset_registered_anchors,
    reset_unkeyed_for_rebuild_count, unkeyed_for_rebuild_count,
};
use rui::component;
use rui::core::reactivity::{
    clear_ui_root_effect, create_effect, create_memo, create_root, create_signal, dispose_effect,
    isolate_reads, isolated_discarded_read_count, mark_next_effect_as_ui_root,
    reset_isolated_discarded_read_count, take_flushed_effects, ui_root_effect_dep_count,
    use_signal, EffectPhase, Signal,
};
use rui::core::store::create_store;
use rui::core::tree::Element;
use rui::prelude::{Button, ButtonProps};
use rui::ui;

fn collect_ids(el: &Element, out: &mut Vec<String>) {
    if let Some(id) = &el.template_id {
        out.push(id.clone());
    }
    for c in &el.children {
        collect_ids(c, out);
    }
}

fn template_contains(el: &Element, needle: &str) -> bool {
    el.template_id
        .as_deref()
        .is_some_and(|id| id.contains(needle))
        || el.children.iter().any(|c| template_contains(c, needle))
}

fn find_text(el: &Element, want: &str) -> bool {
    el.text.as_deref() == Some(want) || el.children.iter().any(|c| find_text(c, want))
}

thread_local! {
    static REACTIVE_CHIP_BODY: Cell<u32> = const { Cell::new(0) };
    static DEFAULT_LIVE_BODY: Cell<u32> = const { Cell::new(0) };
    static FOR_RENDER_COUNTS: RefCell<HashMap<i32, u32>> = RefCell::new(HashMap::new());
}

#[component(reactive)]
fn ReactiveChip(id: String, label: Signal<String>) -> Element {
    REACTIVE_CHIP_BODY.with(|c| c.set(c.get() + 1));
    ui! {
        <Box>{ label.get() }</Box>
    }
}

#[test]
fn text_child_get_lowers_to_bind() {
    reset_registered_anchors();
    let label = use_signal("__rui_wp_r7_text", || "hello".to_string());
    label.set_silent("hello".to_string());
    let el = ui! {
        <Box>{ label.get() }</Box>
    };
    let mut ids = Vec::new();
    collect_ids(&el, &mut ids);
    assert!(
        template_contains(&el, ":bind#"),
        "text child with .get() must lower to bind_text; ids={ids:?}"
    );
    assert!(find_text(&el, "hello"));
    reset_registered_anchors();
}

#[test]
fn if_else_get_lowers_to_bind() {
    reset_registered_anchors();
    let ok = use_signal("__rui_wp_r7_if", || true);
    ok.set_silent(true);
    let el = ui! {
        <Box>{ if ok.get() { "Ready" } else { "Blocked" } }</Box>
    };
    assert!(
        template_contains(&el, ":bind#"),
        "if/else reading .get() must lower to bind_text"
    );
    assert!(find_text(&el, "Ready"));
    reset_registered_anchors();
}

#[test]
fn deref_get_lowers_to_bind() {
    reset_registered_anchors();
    let n = use_signal("__rui_wp_r7_deref", || 3_i32);
    n.set_silent(3);
    let el = ui! {
        <Box>{ (*&n).get().to_string() }</Box>
    };
    assert!(
        template_contains(&el, ":bind#"),
        "(*sig).get() must still be detected as a signal read"
    );
    reset_registered_anchors();
}

#[test]
fn show_when_get_lowers_to_show_reactive() {
    reset_registered_anchors();
    let open = use_signal("__rui_wp_r7_show", || true);
    open.set_silent(true);
    let el = ui! {
        <Box>
            <Show when={open.get()}>
                <Box>"panel"</Box>
            </Show>
        </Box>
    };
    let mut ids = Vec::new();
    collect_ids(&el, &mut ids);
    assert!(
        template_contains(&el, ":Show#"),
        "<Show when={{…get()}}> must lower to show_reactive; ids={ids:?}"
    );
    reset_registered_anchors();
}

#[test]
fn icon_name_get_lowers_to_bind_attr() {
    reset_registered_anchors();
    let ok = use_signal("__rui_wp_r7_icon", || true);
    ok.set_silent(true);
    let before = registered_binding_count();
    let mounted = Rc::new(RefCell::new(None::<Element>));
    let mounted_a = Rc::clone(&mounted);
    let (_owner, _) = create_root(|| {
        create_effect(move || {
            let el = ui! {
                <Icon name={ if ok.get() { "check".to_string() } else { "ban".to_string() } } />
            };
            *mounted_a.borrow_mut() = Some(el);
        });
    });
    assert!(
        registered_binding_count() > before
            || mounted
                .borrow()
                .as_ref()
                .is_some_and(|el| template_contains(el, "bind_icon_name")
                    || el.text_style.font_family.as_deref() == Some("lucide")),
        "Icon name={{…get()}} must lower to bind_attr"
    );
    drop(mounted.borrow_mut().take());
    reset_registered_anchors();
}

#[test]
fn marker_variant_get_lowers_to_bind_attr() {
    use rui::prelude::*;
    reset_registered_anchors();
    let variant = use_signal("__rui_wp_r7_marker_var", || "default".to_string());
    variant.set_silent("default".to_string());
    let before = registered_binding_count();
    let mounted = Rc::new(RefCell::new(None::<Element>));
    let mounted_a = Rc::clone(&mounted);
    let (_owner, _) = create_root(|| {
        create_effect(move || {
            isolate_reads(|| {
                let el = ui! {
                    <Marker variant={ variant.get() }>
                        <MarkerContent>"Today"</MarkerContent>
                    </Marker>
                };
                *mounted_a.borrow_mut() = Some(el);
            });
        });
    });
    let el = mounted.borrow_mut().take().expect("mounted");
    let mut ids = Vec::new();
    collect_ids(&el, &mut ids);
    let variant_attr = el.attrs.get("data-variant").cloned();
    assert!(
        registered_binding_count() > before || template_contains(&el, "bind_marker_variant"),
        "Marker variant={{sig.get()}} must lower to bind_attr; ids={ids:?} variant={variant_attr:?} binds={}->{}",
        before,
        registered_binding_count()
    );
    reset_registered_anchors();
}

#[test]
fn icon_src_attaches_texture_key() {
    reset_registered_anchors();
    let el = ui! {
        <Icon src="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==" />
    };
    assert!(
        el.image_texture
            .as_deref()
            .is_some_and(|k| k.starts_with("rui-src:")),
        "Icon src must queue an image texture, got {:?}",
        el.image_texture
    );
    assert!(el.get_attr("rui_icon_src").is_some());
    reset_registered_anchors();
}

#[test]
fn isolate_reads_naked_get_does_not_update() {
    reset_registered_anchors();
    let sig = use_signal("__rui_wp_r7_naked", || "a".to_string());
    sig.set_silent("a".to_string());
    let kept = Rc::new(RefCell::new(None::<String>));
    let kept_e = Rc::clone(&kept);
    let root = create_effect(move || {
        isolate_reads(|| {
            *kept_e.borrow_mut() = Some(sig.get());
        });
    });
    assert_eq!(kept.borrow().as_deref(), Some("a"));
    sig.set("b".to_string());
    assert_eq!(
        kept.borrow().as_deref(),
        Some("a"),
        "naked get() under isolate_reads must not subscribe (leak if UI relied on this)"
    );
    dispose_effect(root);
    reset_registered_anchors();
}

#[test]
fn reactive_component_isolate_reads_field_get_does_not_rerun_body() {
    reset_registered_anchors();
    REACTIVE_CHIP_BODY.with(|c| c.set(0));
    let label = use_signal("__rui_wp_r7_chip", || "Ready".to_string());
    label.set_silent("Ready".to_string());
    let mounted = Rc::new(RefCell::new(None::<Element>));
    let mounted_a = Rc::clone(&mounted);

    let (_owner, _) = create_root(|| {
        create_effect(move || {
            let el = ReactiveChip(ReactiveChipProps {
                id: "wp-r7-chip".into(),
                label: Some(label),
                children: None,
                base: None,
            });
            *mounted_a.borrow_mut() = Some(el);
        });
    });

    assert_eq!(
        REACTIVE_CHIP_BODY.with(|c| c.get()),
        1,
        "component body runs once at mount"
    );
    {
        let tree = mounted.borrow();
        let tree = tree.as_ref().expect("mounted");
        assert!(
            template_contains(tree, ":bind#") || template_contains(tree, "__component"),
            "reactive chip must expose bind leaf and/or component placeholder"
        );
    }

    label.set("Blocked".to_string());
    assert_eq!(
        REACTIVE_CHIP_BODY.with(|c| c.get()),
        1,
        "field Signal::set must not re-run #[component(reactive)] body (isolate_reads)"
    );

    drop(mounted.borrow_mut().take());
    reset_registered_anchors();
}

#[component]
fn DefaultLiveLabel(n: Signal<i32>) -> Element {
    DEFAULT_LIVE_BODY.with(|c| c.set(c.get() + 1));
    ui! { { n.get() } }
}

/// Wave 4 + Wave 1：默认 `#[component]` isolate 丢边，但 `ui! { {n.get()} }` 仍降 `bind_*`。
#[test]
fn default_component_ui_get_still_updates() {
    reset_registered_anchors();
    reset_isolated_discarded_read_count();
    DEFAULT_LIVE_BODY.with(|c| c.set(0));
    let n = create_signal(1_i32);
    let mounted = Rc::new(RefCell::new(None::<Element>));
    let mounted_a = Rc::clone(&mounted);

    let (_owner, _) = create_root(|| {
        create_effect(move || {
            let el = DefaultLiveLabel(DefaultLiveLabelProps {
                n: Some(n),
                children: None,
                base: None,
            });
            *mounted_a.borrow_mut() = Some(el);
        });
    });

    assert_eq!(
        DEFAULT_LIVE_BODY.with(|c| c.get()),
        1,
        "component body runs once at mount"
    );
    {
        let tree = mounted.borrow();
        let tree = tree.as_ref().expect("mounted");
        assert!(
            template_contains(tree, ":bind#") || find_text(tree, "1"),
            "ui! {{n.get()}} must lower to bind_text; tree text/id missing"
        );
    }
    assert_eq!(
        isolated_discarded_read_count(),
        0,
        "lowered ui! get must not discard onto the isolate frame"
    );

    let _ = take_flushed_effects();
    n.set(2);
    assert_eq!(
        DEFAULT_LIVE_BODY.with(|c| c.get()),
        1,
        "leaf set must not re-run default #[component] body"
    );
    assert!(
        take_flushed_effects()
            .iter()
            .any(|(_, phase)| *phase == EffectPhase::Render),
        "ui! {{n.get()}} must still flush a bind Render effect"
    );

    drop(mounted.borrow_mut().take());
    reset_registered_anchors();
    reset_isolated_discarded_read_count();
}

#[test]
fn for_each_get_with_key_lowers_to_keyed_reactive() {
    reset_registered_anchors();
    let items = use_signal("__rui_wp_r7_for", || vec![1_i32, 2, 3]);
    items.set_silent(vec![1, 2, 3]);
    FOR_RENDER_COUNTS.with(|m| m.borrow_mut().clear());
    let mounted = Rc::new(RefCell::new(None::<Element>));
    let mounted_a = Rc::clone(&mounted);

    let (_owner, _) = create_root(|| {
        create_effect(move || {
            let el = ui! {
                <For each={items.get()} key={*item} item={item}>
                    {
                        {
                            FOR_RENDER_COUNTS.with(|m| {
                                *m.borrow_mut().entry(*item).or_insert(0) += 1;
                            });
                            Element::new().text(format!("{item}"))
                        }
                    }
                </For>
            };
            *mounted_a.borrow_mut() = Some(el);
        });
    });

    FOR_RENDER_COUNTS.with(|m| {
        let map = m.borrow();
        assert_eq!(map.get(&1), Some(&1));
        assert_eq!(map.get(&2), Some(&1));
        assert_eq!(map.get(&3), Some(&1));
    });
    assert_eq!(
        keyed_list_item_owner_count(),
        3,
        "ui! <For each={{get()}} key> must register keyed item owners (not for_reactive)"
    );

    items.set(vec![3, 1, 2]);
    FOR_RENDER_COUNTS.with(|m| {
        let map = m.borrow();
        assert_eq!(
            map.get(&1),
            Some(&1),
            "reorder must not re-render unchanged keys (I-7 via ui! lowering)"
        );
        assert_eq!(map.get(&2), Some(&1));
        assert_eq!(map.get(&3), Some(&1));
    });
    assert_eq!(keyed_list_item_owner_count(), 3);

    drop(mounted.borrow_mut().take());
    reset_registered_anchors();
}

#[test]
fn match_key_get_lowers_to_switch_reactive() {
    reset_registered_anchors();
    let tab = use_signal("__rui_wp_r7_match_key", || 0_i32);
    tab.set_silent(0);
    let el = ui! {
        <Box>
            <Match key={tab.get()}>
                { |k| Element::new().text(format!("arm-{k}")) }
            </Match>
        </Box>
    };
    let mut ids = Vec::new();
    collect_ids(&el, &mut ids);
    assert!(
        template_contains(&el, ":Match#"),
        "<Match key={{…get()}}> must lower to switch_reactive; ids={ids:?}"
    );
    reset_registered_anchors();
}

#[test]
fn match_when_arms_lower_to_match_when_reactive() {
    reset_registered_anchors();
    let ok = use_signal("__rui_wp_r7_match_when", || true);
    ok.set_silent(true);
    let el = ui! {
        <Box>
            <Match fallback={Element::new().text("none")}>
                <MatchArm when={ok.get()}>
                    <Box>"yes"</Box>
                </MatchArm>
                <When when={!ok.get()}>
                    <Box>"no"</Box>
                </When>
            </Match>
        </Box>
    };
    let mut ids = Vec::new();
    collect_ids(&el, &mut ids);
    assert!(
        template_contains(&el, ":Match#"),
        "<Match>/<MatchArm when> must lower to match_when_reactive; ids={ids:?}"
    );
    reset_registered_anchors();
}

#[test]
fn for_each_get_with_key_and_fallback_stays_keyed() {
    reset_registered_anchors();
    let items = use_signal("__rui_wp_r7_for_fb", || Vec::<i32>::new());
    items.set_silent(Vec::new());
    FOR_RENDER_COUNTS.with(|m| m.borrow_mut().clear());
    let mounted = Rc::new(RefCell::new(None::<Element>));
    let mounted_a = Rc::clone(&mounted);

    let (_owner, _) = create_root(|| {
        create_effect(move || {
            let el = ui! {
                <For
                    each={items.get()}
                    key={*item}
                    item={item}
                    fallback={Element::new().text("empty")}
                >
                    {
                        {
                            FOR_RENDER_COUNTS.with(|m| {
                                *m.borrow_mut().entry(*item).or_insert(0) += 1;
                            });
                            Element::new().text(format!("{item}"))
                        }
                    }
                </For>
            };
            *mounted_a.borrow_mut() = Some(el);
        });
    });

    assert_eq!(
        keyed_list_item_owner_count(),
        0,
        "empty keyed For with fallback has no item owners"
    );

    items.set(vec![1_i32, 2]);
    FOR_RENDER_COUNTS.with(|m| {
        let map = m.borrow();
        assert_eq!(map.get(&1), Some(&1));
        assert_eq!(map.get(&2), Some(&1));
    });
    assert_eq!(
        keyed_list_item_owner_count(),
        2,
        "<For each={{get()}} key fallback> must stay on for_keyed_reactive_with_fallback"
    );

    items.set(vec![2, 1]);
    FOR_RENDER_COUNTS.with(|m| {
        let map = m.borrow();
        assert_eq!(map.get(&1), Some(&1), "reorder must not re-render keys");
        assert_eq!(map.get(&2), Some(&1));
    });
    assert_eq!(keyed_list_item_owner_count(), 2);

    drop(mounted.borrow_mut().take());
    reset_registered_anchors();
}

#[test]
fn reactive_island_inner_get_still_lowers_to_bind() {
    reset_registered_anchors();
    let label = use_signal("__rui_wp_r7_island", || "in".to_string());
    label.set_silent("in".to_string());
    // Compile-time: `{get()}` inside `<Reactive>` must still be bind_text (see rui_macros
    // `rsx_lowering_tests`). Runtime: the island placeholder is the ui! child.
    let el = ui! {
        <Box>
            <Reactive id={"wp-r7-isle"}>
                <Box>{ label.get() }</Box>
            </Reactive>
        </Box>
    };
    assert!(
        template_contains(&el, "wp-r7-isle"),
        "<Reactive> must still emit a structural placeholder"
    );
    reset_registered_anchors();
}

#[test]
fn unknown_attr_get_lowers_to_bind_attr() {
    reset_registered_anchors();
    let status = use_signal("__rui_wp_r7_attr", || "ok".to_string());
    status.set_silent("ok".to_string());
    let el = ui! {
        <Box>
            <Box data_status={status.get()} />
        </Box>
    };
    let mut ids = Vec::new();
    collect_ids(&el, &mut ids);
    assert!(
        template_contains(&el, "bind_attr:"),
        "unknown attr with .get() must lower to bind_attr; ids={ids:?}"
    );
    assert!(
        el.children
            .iter()
            .any(|c| c.get_attr("data_status").map(String::as_str) == Some("ok")),
        "apply_bound_attr must write the custom attr on the bound node"
    );
    reset_registered_anchors();
}

#[test]
fn with_lowers_to_bind_peek_does_not() {
    reset_registered_anchors();
    let label = use_signal("__rui_wp_r7_with", || "w".to_string());
    label.set_silent("w".to_string());
    let with_el = ui! {
        <Box>{ label.with(|s| s.clone()) }</Box>
    };
    assert!(
        template_contains(&with_el, ":bind#"),
        "{{sig.with(|s| …)}} is a tracked read and must lower to bind_text"
    );
    assert!(find_text(&with_el, "w"));

    let peek_el = ui! {
        <Box>{ label.peek().clone() }</Box>
    };
    assert!(
        !template_contains(&peek_el, ":bind#"),
        "peek() must not subscribe / must not lower to bind"
    );
    assert!(find_text(&peek_el, "w"));
    reset_registered_anchors();
}

#[test]
fn format_get_lowers_to_bind_text() {
    reset_registered_anchors();
    let n = use_signal("__rui_wp_n1_fmt", || 7_i32);
    n.set_silent(7);
    let el = ui! {
        <Box>{ format!("{}", n.get()) }</Box>
    };
    assert!(
        template_contains(&el, ":bind#"),
        "format! with .get() must lower to bind_text"
    );
    assert!(find_text(&el, "7"));
    reset_registered_anchors();
}

#[test]
fn builder_live_text_class_ui_counter_canary() {
    reset_registered_anchors();
    clear_ui_root_effect();
    let count = create_signal(0_i32);
    let kept = Rc::new(RefCell::new(None::<Element>));
    let kept_e = Rc::clone(&kept);

    mark_next_effect_as_ui_root();
    let root = create_effect(move || {
        let el = ui! {
            <VStack>
                { count.get() }
            </VStack>
        };
        *kept_e.borrow_mut() = Some(el);
    });

    assert_eq!(
        ui_root_effect_dep_count(),
        0,
        "ui! Counter must not subscribe the UI root"
    );
    assert!(find_text(kept.borrow().as_ref().unwrap(), "0"));
    assert!(
        template_contains(kept.borrow().as_ref().unwrap(), ":bind#"),
        "ui! text child get must lower to bind_text"
    );

    let _ = take_flushed_effects();
    count.set(1);
    let render_runs = take_flushed_effects()
        .into_iter()
        .filter(|(_, phase)| *phase == EffectPhase::Render)
        .count();
    assert_eq!(render_runs, 1, "count.set must run one text Render effect");

    drop(kept.borrow_mut().take());
    dispose_effect(root);
    clear_ui_root_effect();
    reset_registered_anchors();
}

#[test]
fn memo_get_lowers_to_bind_text() {
    reset_registered_anchors();
    let n = create_signal(3_i32);
    let m = create_memo(move || n.get() * 2);
    let el = ui! {
        <Box>{ m.get() }</Box>
    };
    assert!(
        template_contains(&el, ":bind#"),
        "Memo::get must lower to bind_text"
    );
    assert!(find_text(&el, "6"));
    reset_registered_anchors();
}

#[test]
fn store_field_get_lowers_to_bind_text() {
    reset_registered_anchors();
    #[derive(Clone, PartialEq)]
    struct User {
        name: String,
    }
    let store = create_store(User {
        name: "ada".to_string(),
    });
    let name = store.field("name", |u| u.name.clone(), |u, v| u.name = v);
    let el = ui! {
        <Box>{ name.get() }</Box>
    };
    assert!(
        template_contains(&el, ":bind#"),
        "StoreField::get must lower to bind_text"
    );
    assert!(find_text(&el, "ada"));
    reset_registered_anchors();
}

#[test]
fn nested_if_text_get_lowers_to_bind() {
    reset_registered_anchors();
    let ok = use_signal("__rui_wp_n1_nested_if", || true);
    ok.set_silent(true);
    let label = use_signal("__rui_wp_n1_nested_if_label", || "in".to_string());
    label.set_silent("in".to_string());
    let el = ui! {
        <Box>{ if ok.get() { if true { label.get() } else { "no".to_string() } } else { "out".to_string() } }</Box>
    };
    assert!(
        template_contains(&el, ":bind#"),
        "nested if text with .get() must lower to bind_text"
    );
    assert!(find_text(&el, "in"));
    reset_registered_anchors();
}

#[test]
fn event_closure_get_does_not_emit_leaf_bind() {
    reset_registered_anchors();
    let n = use_signal("__rui_wp_n1_click", || 0_i32);
    n.set_silent(0);
    let el = ui! {
        <Button id="n1-click" on_click={move || { let _ = n.get(); }}>"static"</Button>
    };
    assert!(
        !template_contains(&el, ":bind#"),
        "on_click closure .get() is a click-time read, not a leaf bind"
    );
    assert!(find_text(&el, "static"));
    reset_registered_anchors();
}

#[test]
fn unkeyed_for_each_get_rebuilds_whole_table() {
    reset_registered_anchors();
    reset_unkeyed_for_rebuild_count();
    let items = use_signal("__rui_wp_n4_unkeyed_ui", || vec![1_i32, 2]);
    items.set_silent(vec![1, 2]);
    let mounted = Rc::new(RefCell::new(None::<Element>));
    let mounted_a = Rc::clone(&mounted);

    let (_owner, _) = create_root(|| {
        create_effect(move || {
            let el = ui! {
                <For each={items.get()} item={item}>
                    { format!("{item}") }
                </For>
            };
            *mounted_a.borrow_mut() = Some(el);
        });
    });

    let after_mount = unkeyed_for_rebuild_count();
    assert!(
        after_mount >= 1,
        "ui! <For each={{get()}}> without key must take the for_reactive warn path"
    );
    assert_eq!(
        keyed_list_item_owner_count(),
        0,
        "unkeyed For must not register item owners"
    );

    items.set(vec![1, 2, 3]);
    assert!(
        unkeyed_for_rebuild_count() > after_mount,
        "unkeyed list change must rebuild the whole table"
    );

    drop(mounted.borrow_mut().take());
    reset_registered_anchors();
    reset_unkeyed_for_rebuild_count();
}

#[test]
fn opacity_get_lowers_to_bind_prop() {
    reset_registered_anchors();
    let n = use_signal("__rui_wp_n2_opacity", || 0.4_f32);
    n.set_silent(0.4);
    let el = ui! {
        <Box opacity={n.get()}></Box>
    };
    assert!(
        template_contains(&el, "bind_opacity"),
        "opacity={{n.get()}} must lower to bind_prop"
    );
    assert!(
        (el.style.opacity - 0.4).abs() < 1e-5,
        "initial opacity {}",
        el.style.opacity
    );
    reset_registered_anchors();
}
