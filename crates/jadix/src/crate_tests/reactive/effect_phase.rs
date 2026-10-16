//! WP-R.9 / I-12: bind_* render phase runs before user `create_effect` on the same `set`.

use crate::app::reactive::{bind_text, reset_registered_anchors};
use crate::core::reactivity::{create_computed, create_effect, dispose_effect, use_signal};
use crate::core::tree::Element;
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn bind_observes_new_value_before_user_effect() {
    reset_registered_anchors();
    let sig = use_signal("__jadix_wp_r9_phase", || 0_i32);
    sig.set_silent(0);

    let order = Rc::new(RefCell::new(Vec::<(u32, &'static str, i32)>::new()));
    let mounted = Rc::new(RefCell::new(None::<Element>));

    let order_a = Rc::clone(&order);
    let mounted_a = Rc::clone(&mounted);
    let root = create_effect(move || {
        let order_user = Rc::clone(&order_a);
        create_effect(move || {
            let v = sig.get();
            order_user.borrow_mut().push((1, "user", v));
        });
        let order_bind = Rc::clone(&order_a);
        let el = bind_text("wp_r9:bind", move || {
            let v = sig.get();
            order_bind.borrow_mut().push((0, "bind", v));
            v.to_string()
        });
        *mounted_a.borrow_mut() = Some(el);
    });

    order.borrow_mut().clear();
    sig.set(7);
    let seen: Vec<(&'static str, i32)> = order
        .borrow()
        .iter()
        .map(|(_, name, v)| (*name, *v))
        .collect();
    assert_eq!(
        seen,
        vec![("bind", 7), ("user", 7)],
        "bind_* must observe the new value before user create_effect"
    );

    drop(mounted.borrow_mut().take());
    dispose_effect(root);
    reset_registered_anchors();
}

#[test]
fn computed_writes_are_visible_to_bind_before_user() {
    reset_registered_anchors();
    let src = use_signal("__jadix_wp_r9_src", || 1_i32);
    src.set_silent(1);
    let derived = use_signal("__jadix_wp_r9_derived", || 0_i32);
    derived.set_silent(0);

    let order = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    let mounted = Rc::new(RefCell::new(None::<Element>));

    let order_a = Rc::clone(&order);
    let mounted_a = Rc::clone(&mounted);
    let root = create_effect(move || {
        create_computed(move || {
            derived.set(src.get() * 10);
        });
        let order_user = Rc::clone(&order_a);
        create_effect(move || {
            let _ = derived.get();
            order_user.borrow_mut().push("user");
        });
        let order_bind = Rc::clone(&order_a);
        let el = bind_text("wp_r9:derived", move || {
            let v = derived.get();
            order_bind.borrow_mut().push("bind");
            v.to_string()
        });
        *mounted_a.borrow_mut() = Some(el);
    });

    order.borrow_mut().clear();
    src.set(3);
    assert_eq!(derived.peek(), 30);
    assert_eq!(
        *order.borrow(),
        vec!["bind", "user"],
        "after computed writes, bind still precedes user"
    );

    drop(mounted.borrow_mut().take());
    dispose_effect(root);
    reset_registered_anchors();
}
