//! WP-G.2: leaked tracked reads inside `isolate_reads` are countable; `peek` is not.

use crate::app::reactive::{bind_text, reset_registered_anchors};
use crate::core::reactivity::{
    create_effect, create_signal, dispose_effect, isolate_reads, isolated_discarded_read_count,
    reset_isolated_discarded_read_count,
};
use crate::core::tree::Element;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[test]
fn isolated_discarded_read_bare_get_is_observable_and_does_not_update() {
    reset_registered_anchors();
    reset_isolated_discarded_read_count();
    let n = create_signal(1_i32);
    let runs = Rc::new(Cell::new(0_u32));
    let kept = Rc::new(RefCell::new(None::<Element>));

    let runs_e = Rc::clone(&runs);
    let kept_e = Rc::clone(&kept);
    let root = create_effect(move || {
        runs_e.set(runs_e.get() + 1);
        let el = isolate_reads(|| {
            let v = n.get();
            Element::new().text(format!("{v}"))
        });
        *kept_e.borrow_mut() = Some(el);
    });

    assert!(
        isolated_discarded_read_count() >= 1,
        "bare get inside isolate_reads must count as a discarded read"
    );
    assert_eq!(
        kept.borrow()
            .as_ref()
            .and_then(|e| e.text.clone())
            .as_deref(),
        Some("1")
    );

    let before = runs.get();
    n.set(2);
    assert_eq!(
        runs.get(),
        before,
        "discarded edge must not re-run the isolating effect"
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

#[test]
fn isolated_discarded_read_peek_does_not_count() {
    reset_isolated_discarded_read_count();
    let n = create_signal(1_i32);
    let root = create_effect(move || {
        isolate_reads(|| {
            let _ = n.peek();
        });
    });
    assert_eq!(
        isolated_discarded_read_count(),
        0,
        "peek / untrack must not warn"
    );
    dispose_effect(root);
    reset_isolated_discarded_read_count();
}

#[test]
fn isolated_discarded_read_bind_text_does_not_count() {
    reset_registered_anchors();
    reset_isolated_discarded_read_count();
    let n = create_signal("a".to_string());
    let runs = Rc::new(Cell::new(0_u32));
    let kept = Rc::new(RefCell::new(None::<Element>));

    let runs_e = Rc::clone(&runs);
    let kept_e = Rc::clone(&kept);
    let root = create_effect(move || {
        let el = isolate_reads(|| {
            let runs_b = Rc::clone(&runs_e);
            bind_text("g2_bind", move || {
                runs_b.set(runs_b.get() + 1);
                n.get()
            })
        });
        *kept_e.borrow_mut() = Some(el);
    });

    assert_eq!(
        isolated_discarded_read_count(),
        0,
        "lowered bind_text must subscribe its own Render effect, not the isolate frame"
    );
    let after_mount = runs.get();
    n.set("b".to_string());
    assert!(
        runs.get() > after_mount,
        "bind_text leaf must still re-run on set"
    );

    drop(kept.borrow_mut().take());
    dispose_effect(root);
    reset_registered_anchors();
    reset_isolated_discarded_read_count();
}
