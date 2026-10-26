#![cfg(feature = "components")]
//! Wave 1 / N.2: Builder `text` / `class` / `w` / `h` / `bg` / `color` / `opacity` accept Signal / FnMut.
//! Filter: `cargo test -p jadix --lib builder_live_`

use crate::app::reactive::reset_registered_anchors;
use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
use crate::components::button::button;
use crate::components::label::label;
use crate::core::reactivity::{
    batch, clear_ui_root_effect, component_root, create_effect, create_signal, dispose_effect,
    mark_next_effect_as_ui_root, take_flushed_effects, ui_root_effect_dep_count, EffectPhase,
};
use crate::core::tree::Element;
use crate::primitives::r#box::{r#box, vstack};
use crate::primitives::text::{text, TextBuilder};
use crate::style::parse_color;
use std::cell::RefCell;
use std::rc::Rc;

fn find_text(el: &Element, want: &str) -> bool {
    el.text.as_deref() == Some(want) || el.children.iter().any(|c| find_text(c, want))
}

fn template_contains(el: &Element, needle: &str) -> bool {
    el.template_id
        .as_deref()
        .is_some_and(|id| id.contains(needle))
        || el.children.iter().any(|c| template_contains(c, needle))
}

fn render_flush_count() -> usize {
    take_flushed_effects()
        .into_iter()
        .filter(|(_, phase)| *phase == EffectPhase::Render)
        .count()
}

#[test]
fn builder_live_same_helper_and_loop_bindings_are_owner_scoped_and_rebuild_stable() {
    fn anonymous_row(
        caption: crate::core::reactivity::Signal<&'static str>,
        size: crate::core::reactivity::Signal<f32>,
    ) -> Element {
        vstack()
            .w(size)
            .h(move || size.get())
            .opacity(move || size.get() / 100.0)
            .child(text(caption))
            .child(text(move || caption.get()))
            .into()
    }
    fn ids(tree: &Element) -> Vec<String> {
        let mut ids = Vec::new();
        crate::core::tree::walk_elements(tree, |el| {
            if let Some(id) = &el.template_id {
                ids.push(id.clone());
            }
        });
        ids
    }
    reset_registered_anchors();
    clear_test_reconcile_root();
    let tree = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&tree));
    let caption = create_signal("before");
    let size = create_signal(20.0_f32);
    let rebuild = create_signal(0);
    let root = create_effect({
        let tree = Rc::clone(&tree);
        move || {
            rebuild.get();
            let rows = (0..3).map(|_| anonymous_row(caption, size));
            *tree.borrow_mut() = vstack().children(rows).into();
        }
    });
    let initial_ids = ids(&tree.borrow());
    assert_eq!(initial_ids.len(), 9);
    assert_eq!(
        initial_ids
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        9
    );
    for tick in 1..4 {
        rebuild.set(tick);
        assert_eq!(
            ids(&tree.borrow()),
            initial_ids,
            "owner slots restart on rebuild"
        );
        assert_eq!(crate::app::reactive::registered_binding_count(), 15);
        batch(|| {
            caption.set("after");
            size.set(40.0);
        });
        for row in &tree.borrow().children {
            assert_eq!(row.style.size.width, taffy::style::Dimension::length(40.0));
            assert_eq!(row.style.size.height, taffy::style::Dimension::length(40.0));
            assert!((row.style.opacity - 0.4).abs() < 1e-5);
            assert!(row
                .children
                .iter()
                .all(|el| el.text.as_deref() == Some("after")));
        }
        batch(|| {
            caption.set("before");
            size.set(20.0);
        });
    }
    clear_test_reconcile_root();
    dispose_effect(root);
    assert_eq!(crate::app::reactive::registered_binding_count(), 0);
    reset_registered_anchors();
}

#[test]
fn builder_live_same_helper_in_sibling_component_owners_does_not_collide() {
    fn caption(value: crate::core::reactivity::Signal<&'static str>) -> Element {
        component_root(|| text(value).into())
    }
    reset_registered_anchors();
    clear_test_reconcile_root();
    let tree = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&tree));
    let value = create_signal("old");
    let root = create_effect({
        let tree = Rc::clone(&tree);
        move || {
            *tree.borrow_mut() = vstack().children((0..3).map(|_| caption(value))).into();
        }
    });
    let ids: Vec<_> = tree
        .borrow()
        .children
        .iter()
        .map(|el| el.template_id.clone())
        .collect();
    assert_eq!(
        ids.iter().collect::<std::collections::HashSet<_>>().len(),
        3
    );
    value.set("new");
    assert!(tree
        .borrow()
        .children
        .iter()
        .all(|el| el.text.as_deref() == Some("new")));
    clear_test_reconcile_root();
    dispose_effect(root);
    assert_eq!(crate::app::reactive::registered_binding_count(), 0);
    reset_registered_anchors();
}

#[test]
fn builder_live_shared_signals_have_distinct_leaf_sites() {
    fn named_caption(id: &str, caption: crate::core::reactivity::Signal<&'static str>) -> Element {
        label("initial-caption").id(id).text(caption).into()
    }

    reset_registered_anchors();
    clear_test_reconcile_root();
    let tree = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&tree));
    let caption = create_signal("before");
    let classes = create_signal("bg-red-500");
    let size = create_signal(20.0_f32);
    let color = create_signal("red-500");
    let opacity = create_signal(0.2_f32);

    mark_next_effect_as_ui_root();
    let root = create_effect({
        let tree = Rc::clone(&tree);
        move || {
            *tree.borrow_mut() = vstack()
                .child(text(caption))
                .child(TextBuilder::new(caption))
                .child(text(move || caption.get()))
                .child(Element::new().class(classes))
                .child(Element::new().class(classes))
                .child(r#box().w(size))
                .child(r#box().w(size))
                .child(r#box().bg(color))
                .child(r#box().bg(color))
                .child(r#box().text_color(color))
                .child(r#box().text_color(color))
                .child(r#box().opacity(opacity))
                .child(r#box().opacity(opacity))
                .child(text(caption.peek()))
                .child(named_caption("first-caption", caption))
                .child(named_caption("second-caption", caption))
                .into();
        }
    });
    assert_eq!(ui_root_effect_dep_count(), 0);
    let ids: std::collections::HashSet<_> = tree.borrow().children[..13]
        .iter()
        .map(|el| el.template_id.clone().expect("live leaf identity"))
        .collect();
    assert_eq!(ids.len(), 13, "each authored site needs its own binding");

    let _ = take_flushed_effects();
    batch(|| {
        caption.set("after");
        classes.set("bg-blue-500");
        size.set(30.0);
        color.set("blue-500");
        opacity.set(0.8);
    });
    assert_eq!(render_flush_count(), 15);
    {
        let tree = tree.borrow();
        for el in &tree.children[..3] {
            assert_eq!(el.text.as_deref(), Some("after"));
        }
        for el in &tree.children[3..5] {
            assert_eq!(
                el.get_attr("jadix_utility_class").map(String::as_str),
                Some("bg-blue-500")
            );
        }
        for el in &tree.children[5..7] {
            assert_eq!(el.style.size.width, taffy::style::Dimension::length(30.0));
        }
        for el in &tree.children[7..9] {
            assert_eq!(el.style.bg_color, parse_color("blue-500"));
        }
        for el in &tree.children[9..11] {
            assert_eq!(
                el.text_style.color,
                Some(crate::style::parse_text_color("blue-500"))
            );
        }
        for el in &tree.children[11..13] {
            assert!((el.style.opacity - 0.8).abs() < 1e-5);
        }
        assert_eq!(tree.children[13].text.as_deref(), Some("before"));
        assert_eq!(
            tree.children[14].template_id.as_deref(),
            Some("first-caption")
        );
        assert_eq!(
            tree.children[15].template_id.as_deref(),
            Some("second-caption")
        );
        for el in &tree.children[14..] {
            assert_eq!(el.text.as_deref(), Some("after"));
        }
    }

    clear_test_reconcile_root();
    dispose_effect(root);
    assert_eq!(crate::app::reactive::registered_binding_count(), 0);
    clear_ui_root_effect();
    reset_registered_anchors();
}

#[test]
fn builder_live_text_class_signal_binds_in_place() {
    reset_registered_anchors();
    let label_sig = create_signal("hello".to_string());
    let class_sig = create_signal("bg-red-500".to_string());
    let kept = Rc::new(RefCell::new(None::<Element>));
    let kept_e = Rc::clone(&kept);

    mark_next_effect_as_ui_root();
    let root = create_effect(move || {
        let el: Element = vstack()
            .class(class_sig)
            .child(label("t").text(label_sig))
            .into();
        *kept_e.borrow_mut() = Some(el);
    });

    assert_eq!(
        ui_root_effect_dep_count(),
        0,
        "live Builder must not subscribe the UI root"
    );
    let mounted = kept.borrow();
    let el = mounted.as_ref().expect("mounted");
    assert!(find_text(el, "hello"), "initial text");
    assert_eq!(
        el.attrs.get("jadix_utility_class").map(String::as_str),
        Some("bg-red-500")
    );

    let _ = take_flushed_effects();
    label_sig.set("world".to_string());
    assert_eq!(
        render_flush_count(),
        1,
        "text Signal.set must flush one Render leaf"
    );

    let _ = take_flushed_effects();
    class_sig.set("text-lg".to_string());
    assert_eq!(
        render_flush_count(),
        1,
        "class Signal.set must flush one Render leaf"
    );

    drop(mounted);
    drop(kept.borrow_mut().take());
    dispose_effect(root);
    clear_ui_root_effect();
    reset_registered_anchors();
}

#[test]
fn builder_live_text_class_size_bg_color_signals() {
    reset_registered_anchors();
    let w = create_signal(40.0_f32);
    let h = create_signal(20.0_f32);
    let bg = create_signal("red-500".to_string());
    let fg = create_signal("white".to_string());
    let kept = Rc::new(RefCell::new(None::<Element>));
    let kept_e = Rc::clone(&kept);

    let root = create_effect(move || {
        let el: Element = vstack()
            .w(w)
            .h(h)
            .bg(bg)
            .text_color(fg)
            .child(label("n").text("x"))
            .into();
        *kept_e.borrow_mut() = Some(el);
    });

    {
        let mounted = kept.borrow();
        let el = mounted.as_ref().expect("mounted");
        assert_eq!(el.style.bg_color, parse_color("red-500"));
    }

    let _ = take_flushed_effects();
    w.set(80.0);
    assert_eq!(render_flush_count(), 1);
    let _ = take_flushed_effects();
    bg.set("blue-500".to_string());
    assert_eq!(render_flush_count(), 1);

    drop(kept.borrow_mut().take());
    dispose_effect(root);
    reset_registered_anchors();
}

#[test]
fn builder_live_text_class_snapshot_get_does_not_bind() {
    reset_registered_anchors();
    let label_sig = create_signal("snap".to_string());
    let el: Element = vstack()
        .class(label_sig.get())
        .child(text(label_sig.get()))
        .into();
    assert!(
        find_text(&el, "snap"),
        "evaluated get() is still a legal snapshot"
    );
    assert!(
        !template_contains(&el, "__jadix_text_sig") && !template_contains(&el, "__jadix_class_sig"),
        "`.text(sig.get())` / `.class(sig.get())` must not create a live bind"
    );
    let renamed: Element = label("old").text("static").id("new").into();
    assert_eq!(renamed.template_id.as_deref(), Some("new"));
    let anonymous: Element = label("old").id("").text("static").into();
    assert!(anonymous.template_id.is_none());
    reset_registered_anchors();
}

#[test]
fn builder_live_text_class_counter_canary() {
    reset_registered_anchors();
    clear_ui_root_effect();
    let count = create_signal(0_i32);
    let kept = Rc::new(RefCell::new(None::<Element>));
    let kept_e = Rc::clone(&kept);

    mark_next_effect_as_ui_root();
    let root = create_effect(move || {
        let el: Element = vstack()
            .child(label("count").text(count))
            .child(button("inc").text("+").on_click(move || {
                count.set(count.peek() + 1);
            }))
            .into();
        *kept_e.borrow_mut() = Some(el);
    });

    assert_eq!(
        ui_root_effect_dep_count(),
        0,
        "Builder Counter must not subscribe the UI root"
    );
    assert!(
        find_text(kept.borrow().as_ref().unwrap(), "0"),
        "initial count"
    );

    let _ = take_flushed_effects();
    count.set(1);
    assert_eq!(
        render_flush_count(),
        1,
        "count.set must run one text Render effect"
    );

    drop(kept.borrow_mut().take());
    dispose_effect(root);
    clear_ui_root_effect();
    reset_registered_anchors();
}

#[test]
fn builder_live_text_class_fnmut_text() {
    reset_registered_anchors();
    let count = create_signal(2_i32);
    let kept = Rc::new(RefCell::new(None::<Element>));
    let kept_e = Rc::clone(&kept);
    let root = create_effect(move || {
        let el: Element = vstack()
            .child(label("n").text(move || count.get().to_string()))
            .into();
        *kept_e.borrow_mut() = Some(el);
    });
    assert!(find_text(kept.borrow().as_ref().unwrap(), "2"));
    let _ = take_flushed_effects();
    count.set(3);
    assert_eq!(render_flush_count(), 1);
    drop(kept.borrow_mut().take());
    dispose_effect(root);
    reset_registered_anchors();
}

#[test]
fn builder_live_opacity_signal() {
    reset_registered_anchors();
    clear_test_reconcile_root();
    let tree = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&tree));
    let op = create_signal(0.4_f32);

    mark_next_effect_as_ui_root();
    let root = create_effect({
        let tree = Rc::clone(&tree);
        move || {
            let el: Element = r#box().opacity(op).into();
            *tree.borrow_mut() = el;
        }
    });

    assert_eq!(
        ui_root_effect_dep_count(),
        0,
        "live opacity must not subscribe the UI root"
    );
    assert!(
        (tree.borrow().style.opacity - 0.4).abs() < 1e-5,
        "initial opacity {}",
        tree.borrow().style.opacity
    );

    let _ = take_flushed_effects();
    op.set(0.8);
    assert_eq!(
        render_flush_count(),
        1,
        "opacity Signal.set must flush one Render leaf"
    );
    assert!(
        (tree.borrow().style.opacity - 0.8).abs() < 1e-5,
        "opacity must patch in place, got {}",
        tree.borrow().style.opacity
    );

    clear_test_reconcile_root();
    dispose_effect(root);
    clear_ui_root_effect();
    reset_registered_anchors();
}

#[test]
fn builder_live_opacity_snapshot_get_does_not_bind() {
    reset_registered_anchors();
    let op = create_signal(0.2_f32);
    let el: Element = r#box().opacity(op.get()).into();
    assert!(
        (el.style.opacity - 0.2).abs() < 1e-5,
        "evaluated get() is still a legal snapshot"
    );
    assert!(
        !template_contains(&el, "__jadix_opacity_sig"),
        "`.opacity(sig.get())` must not create a live bind"
    );
    reset_registered_anchors();
}
