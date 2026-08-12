//! WP-2.7 / R-WP-2: root `app_builder` bare `Signal::get()` is observable;
//! bind / Show / For keep the UI-root dep set empty.

use crate::app::reactive::{bind_text, reset_registered_anchors, show_reactive};
use crate::core::reactivity::{
    clear_ui_root_effect, create_effect, create_signal, dispose_effect, mark_next_effect_as_ui_root,
    root_bare_get_count, ui_root_effect_dep_count,
};
use crate::core::tree::Element;
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn bare_get_on_ui_root_is_observable() {
    reset_registered_anchors();
    clear_ui_root_effect();
    let count = create_signal(0_i32);
    let kept = Rc::new(RefCell::new(None::<Element>));

    let kept_e = Rc::clone(&kept);
    mark_next_effect_as_ui_root();
    let root = create_effect(move || {
        // Anti-pattern: naked get() while building the UI root tree.
        let n = count.get();
        *kept_e.borrow_mut() = Some(Element::new().text(format!("n={n}")));
    });

    assert!(
        ui_root_effect_dep_count() >= 1,
        "bare get must subscribe the marked UI root"
    );
    assert_eq!(root_bare_get_count(), ui_root_effect_dep_count());

    count.set(1);
    assert_eq!(
        kept.borrow()
            .as_ref()
            .and_then(|e| e.text.clone())
            .as_deref(),
        Some("n=1"),
        "UI root re-runs on bare-get subscription (whole-tree path)"
    );

    drop(kept.borrow_mut().take());
    dispose_effect(root);
    clear_ui_root_effect();
    reset_registered_anchors();
}

#[test]
fn bind_text_under_ui_root_does_not_subscribe_root() {
    reset_registered_anchors();
    clear_ui_root_effect();
    let label = create_signal("a".to_string());
    let kept = Rc::new(RefCell::new(None::<Element>));

    let kept_e = Rc::clone(&kept);
    mark_next_effect_as_ui_root();
    let root = create_effect(move || {
        let label = label;
        let leaf = bind_text("wp27_bind", move || label.get());
        *kept_e.borrow_mut() = Some(leaf);
    });

    assert_eq!(
        ui_root_effect_dep_count(),
        0,
        "bind_text must attach get() to its own effect, not the UI root"
    );
    assert_eq!(
        kept.borrow()
            .as_ref()
            .and_then(|e| e.text.clone())
            .as_deref(),
        Some("a")
    );

    label.set("b".to_string());
    // Binding effect updates via try_update when runtime is live; without AppState the
    // leaf Element in `kept` is the registration-time snapshot. Root must stay unsubscribed.
    assert_eq!(ui_root_effect_dep_count(), 0);

    drop(kept.borrow_mut().take());
    dispose_effect(root);
    clear_ui_root_effect();
    reset_registered_anchors();
}

#[test]
fn show_reactive_under_ui_root_does_not_subscribe_root() {
    reset_registered_anchors();
    clear_ui_root_effect();
    let open = create_signal(true);
    let kept = Rc::new(RefCell::new(None::<Element>));

    let kept_e = Rc::clone(&kept);
    mark_next_effect_as_ui_root();
    let root = create_effect(move || {
        let open = open;
        let el = show_reactive(
            "wp27_show",
            move || open.get(),
            || Element::new().text("panel"),
        );
        *kept_e.borrow_mut() = Some(el);
    });

    assert_eq!(
        ui_root_effect_dep_count(),
        0,
        "show_reactive when=get() must not subscribe the UI root"
    );

    open.set(false);
    assert_eq!(ui_root_effect_dep_count(), 0);

    drop(kept.borrow_mut().take());
    dispose_effect(root);
    clear_ui_root_effect();
    reset_registered_anchors();
}
