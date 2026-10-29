//! WP-2.4 / R-WP-1: Anchors / Bindings / COMPONENT_* leave no registry residue after Owner dispose.

use std::cell::RefCell;
use std::rc::Rc;

use crate::app::reactive::{
    bind_text, component_children_count, component_props_count, reactive_anchor,
    reactive_component, registered_anchor_count, registered_binding_count,
    reset_registered_anchors,
};
use crate::core::reactivity::{create_effect, create_root};
use crate::core::tree::Element;

pub(crate) fn registry_totals() -> (usize, usize, usize, usize) {
    (
        registered_anchor_count(),
        registered_binding_count(),
        component_props_count(),
        component_children_count(),
    )
}

#[test]
fn owner_dispose_clears_anchor_binding_and_component_props() {
    reset_registered_anchors();
    let baseline = registry_totals();

    let (handle, kept) = create_root(|| {
        // Keep returned Elements alive: dropping them would fire UnmountHook and clear
        // REGISTERED_BINDINGS before Owner dispose (same as a never-mounted node).
        let kept: Rc<RefCell<Vec<Element>>> = Rc::new(RefCell::new(Vec::new()));
        let kept_effect = Rc::clone(&kept);
        create_effect(move || {
            reactive_anchor("wp24_anchor", || Element::new().text("a"));
            kept_effect
                .borrow_mut()
                .push(bind_text("wp24_bind", || "b".to_string()));
            kept_effect
                .borrow_mut()
                .push(reactive_component("wp24_comp", 1_u32, |_| {
                    Element::new().text("c")
                }));
        });
        kept
    });

    let mid = registry_totals();
    assert!(
        mid.0 > baseline.0 && mid.1 > baseline.1 && mid.2 > baseline.2,
        "expected registrations while mounted: baseline={baseline:?} mid={mid:?}"
    );

    drop(kept);
    handle.dispose();
    let after = registry_totals();
    assert_eq!(
        after, baseline,
        "Owner dispose must drop Anchors/Bindings/COMPONENT_*; baseline={baseline:?} after={after:?}"
    );
}

#[test]
fn registry_does_not_grow_across_repeated_owner_cycles() {
    reset_registered_anchors();
    let baseline = registry_totals();

    for i in 0..40 {
        let (handle, kept) = create_root(move || {
            let kept: Rc<RefCell<Vec<Element>>> = Rc::new(RefCell::new(Vec::new()));
            let kept_effect = Rc::clone(&kept);
            create_effect(move || {
                let aid = format!("wp24_cycle_a_{i}");
                let bid = format!("wp24_cycle_b_{i}");
                let cid = format!("wp24_cycle_c_{i}");
                reactive_anchor(aid, Element::new);
                kept_effect
                    .borrow_mut()
                    .push(bind_text(bid, || "x".to_string()));
                kept_effect
                    .borrow_mut()
                    .push(reactive_component(cid, i as u32, |_| Element::new()));
            });
            kept
        });
        drop(kept);
        handle.dispose();
    }

    let after = registry_totals();
    assert_eq!(
        after, baseline,
        "40 mount/dispose cycles must not leave registry residue; baseline={baseline:?} after={after:?}"
    );
}
