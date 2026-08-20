//! P.2.3: long-session registry stability (Gallery-like section switching + named signals).

use std::cell::RefCell;
use std::rc::Rc;

use crate::app::reactive::{
    reactive_anchor, reset_registered_anchors, switch_reactive,
};
use crate::core::reactivity::{create_effect, create_root, reactivity_debug_snapshot, use_signal};
use crate::core::tree::Element;
use super::registry_owner::registry_totals;

#[test]
fn switch_reactive_section_cycles_do_not_grow_registry() {
    reset_registered_anchors();
    let baseline = registry_totals();
    let section = use_signal("wp243_gallery_section", || "basics".to_string());
    section.set_silent("basics".to_string());

    let (handle, kept) = create_root(|| {
        let kept: Rc<RefCell<Option<Element>>> = Rc::new(RefCell::new(None));
        let kept_eff = Rc::clone(&kept);
        create_effect(move || {
            let el = switch_reactive(
                "examples/gallery/App:section_body",
                move || section.get(),
                |key| match key.as_str() {
                    "basics" => {
                        reactive_anchor("gal_sec:basics", || Element::new().text("basics"));
                        Element::new().child(
                            crate::app::reactive::reactive_placeholder("gal_sec:basics"),
                        )
                    }
                    "forms" => {
                        reactive_anchor("gal_sec:forms", || Element::new().text("forms"));
                        Element::new().child(
                            crate::app::reactive::reactive_placeholder("gal_sec:forms"),
                        )
                    }
                    "data" => {
                        reactive_anchor("gal_sec:data", || Element::new().text("data"));
                        Element::new().child(
                            crate::app::reactive::reactive_placeholder("gal_sec:data"),
                        )
                    }
                    _ => Element::new(),
                },
            );
            *kept_eff.borrow_mut() = Some(el);
        });
        kept
    });

    let after_mount = registry_totals();
    assert!(
        after_mount.0 > baseline.0,
        "expected switch anchor registered: baseline={baseline:?} after={after_mount:?}"
    );

    for i in 0..40 {
        let key = match i % 3 {
            0 => "basics",
            1 => "forms",
            _ => "data",
        };
        section.set(key.to_string());
    }

    let after_cycles = registry_totals();
    assert_eq!(
        after_cycles, after_mount,
        "40 section switches must not grow REGISTERED_* ; mount={after_mount:?} cycles={after_cycles:?}"
    );

    drop(kept);
    handle.dispose();
    assert_eq!(
        registry_totals(),
        baseline,
        "dispose must restore registry baseline"
    );
}

#[test]
fn named_signals_plateau_after_repeated_use_signal_keys() {
    reset_registered_anchors();
    let snap0 = reactivity_debug_snapshot();

    for round in 0..40 {
        for i in 0..8 {
            let key = format!("wp243_named_{i}");
            let _ = use_signal(key.as_str(), || round);
        }
    }

    let snap1 = reactivity_debug_snapshot();
    assert_eq!(
        snap0.named_signals + 8,
        snap1.named_signals,
        "each unique use_signal key registers once; snap0={} snap1={}",
        snap0.named_signals,
        snap1.named_signals
    );
    assert_eq!(
        snap0.live_signals + 8,
        snap1.live_signals,
        "live signal storage should track named keys only"
    );
}
