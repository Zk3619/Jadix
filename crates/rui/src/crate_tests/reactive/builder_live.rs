//! Wave 1 / N.2: Builder `text` / `class` / `w` / `h` / `bg` / `color` / `opacity` accept Signal / FnMut.
//! Filter: `cargo test -p rui --lib builder_live_`

use crate::app::reactive::reset_registered_anchors;
use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
use crate::components::button::button;
use crate::components::label::label;
use crate::core::reactivity::{
    clear_ui_root_effect, create_effect, create_signal, dispose_effect,
    mark_next_effect_as_ui_root, take_flushed_effects, ui_root_effect_dep_count, EffectPhase,
};
use crate::core::tree::Element;
use crate::primitives::r#box::{r#box, vstack};
use crate::primitives::text::text;
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
        el.attrs.get("rui_utility_class").map(String::as_str),
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
        !template_contains(&el, "__rui_text_sig") && !template_contains(&el, "__rui_class_sig"),
        "`.text(sig.get())` / `.class(sig.get())` must not create a live bind"
    );
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
        !template_contains(&el, "__rui_opacity_sig"),
        "`.opacity(sig.get())` must not create a live bind"
    );
    reset_registered_anchors();
}
