use super::*;
use crate::app::reactive::{immediate_region, reactive_view, reset_registered_anchors};
use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
use crate::core::reactivity::{create_effect, create_root, OwnerHandle};
use crate::core::tree::{Element, LayoutTree};
use std::rc::Rc;

fn harness(mut render: impl FnMut() -> Element + 'static) -> (OwnerHandle, Rc<RefCell<Element>>) {
    reset();
    reset_registered_anchors();
    let tree = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(tree.clone());
    let root = tree.clone();
    let (owner, _) = create_root(move || {
        create_effect(move || {
            register_root(crate::core::reactivity::current_effect_id().unwrap());
            let mut next = render();
            crate::app::test_support::reconcile_elements(&mut next, &mut root.borrow_mut());
            *root.borrow_mut() = next;
        });
    });
    (owner, tree)
}

#[test]
fn hybrid_immediate_ticks_only_its_region_and_can_stop_requesting_frames() {
    let retained = Rc::new(Cell::new(0));
    let ticks = Rc::new(Cell::new(0));
    let active = Rc::new(Cell::new(true));
    let (owner, tree) = harness({
        let retained = retained.clone();
        let ticks = ticks.clone();
        let active = active.clone();
        move || {
            retained.set(retained.get() + 1);
            let ticks = ticks.clone();
            let active = active.clone();
            Element::new().child(immediate_region("hud", move || {
                ticks.set(ticks.get() + 1);
                if active.get() {
                    request_frame();
                }
                Element::new()
                    .children((0..ticks.get()).map(|i| Element::new().text(i.to_string())))
            }))
        }
    });
    assert_eq!(ticks.get(), 1);
    for expected in 2..5 {
        assert!(pending(WindowScope::Primary));
        begin_frame();
        assert_eq!(ticks.get(), expected);
        assert_eq!(tree.borrow().children[0].children.len(), expected);
    }
    active.set(false);
    begin_frame();
    assert!(
        !pending(WindowScope::Primary),
        "no automatic permanent pump"
    );
    assert_eq!(
        retained.get(),
        1,
        "retained parent must not rebuild with HUD"
    );
    let before = ticks.get();
    request_frame();
    begin_frame();
    assert_eq!(ticks.get(), before + 1);
    assert!(!pending(WindowScope::Primary));
    drop(owner);
    assert_eq!(crate::app::reactive::registered_anchor_count(), 0);
    clear_test_reconcile_root();
    reset();
}

#[test]
fn hybrid_nested_regions_run_once_per_frame_and_rebuild_keeps_layout_hot() {
    let roots = Rc::new(Cell::new(0));
    let parent = Rc::new(Cell::new(0));
    let child = Rc::new(Cell::new(0));
    let (owner, tree) = harness({
        let roots = roots.clone();
        let parent = parent.clone();
        let child = child.clone();
        move || {
            roots.set(roots.get() + 1);
            let parent = parent.clone();
            let child = child.clone();
            Element::new()
                .w(200.0)
                .h(100.0)
                .child(immediate_region("outer", move || {
                    parent.set(parent.get() + 1);
                    let child = child.clone();
                    Element::new()
                        .w(100.0)
                        .h(40.0)
                        .child(immediate_region("inner", move || {
                            child.set(child.get() + 1);
                            Element::new().w(40.0).h(20.0).text("warm")
                        }))
                }))
        }
    });
    let mut layout = LayoutTree::new();
    layout.build_and_layout(&mut tree.borrow_mut(), 200.0, 100.0, [-1.0; 2]);
    let ids = {
        let t = tree.borrow();
        (
            t.layout_node,
            t.children[0].layout_node,
            t.children[0].children[0].layout_node,
        )
    };
    let stats = invalidation_stats();
    for _ in 0..4 {
        begin_frame();
    }
    assert_eq!(parent.get(), 5);
    assert_eq!(
        child.get(),
        5,
        "nested old effect must not execute before its parent replaces it"
    );
    rebuild_all();
    rebuild_all();
    begin_frame();
    assert_eq!(roots.get(), 2, "L3 coalesces");
    assert_eq!(parent.get(), 6);
    assert_eq!(child.get(), 6);
    let t = tree.borrow();
    assert_eq!(
        ids,
        (
            t.layout_node,
            t.children[0].layout_node,
            t.children[0].children[0].layout_node
        )
    );
    assert!(
        !layout.is_node_dirty(ids.0.unwrap()),
        "L3 cannot invalidate unchanged Taffy inputs"
    );
    assert_eq!(invalidation_stats().cache_busts, stats.cache_busts);
    assert_eq!(invalidation_stats().rebuilds, stats.rebuilds + 1);
    drop(t);
    drop(owner);
    clear_test_reconcile_root();
    reset();
}

#[test]
fn hybrid_ladder_coalesces_without_promoting_l3_to_l4_and_is_window_scoped() {
    reset();
    let _scope = window_scope::enter_primary();
    let (owner, ()) = create_root(|| {
        create_effect(|| register_root(crate::core::reactivity::current_effect_id().unwrap()));
    });
    request_frame();
    node("leaf").mark_paint();
    node("leaf").mark_layout();
    node("leaf").mark_paint();
    rebuild_all();
    REQUESTS.with(|q| {
        let q = q.borrow();
        let r = &q[&WindowScope::Primary];
        assert!(r.present && r.rebuild && !r.caches);
        assert_eq!(r.nodes.len(), 1);
        assert!(r.nodes["leaf"]);
    });
    let _secondary = window_scope::enter(WindowScope::Secondary(991));
    assert!(!pending(window_scope::active()));
    bust_caches();
    assert!(pending(WindowScope::Secondary(991)));
    REQUESTS.with(|q| {
        let q = q.borrow();
        for scope in [WindowScope::Primary, WindowScope::Secondary(991)] {
            assert!(
                q[&scope].caches && q[&scope].rebuild,
                "L4 fans out to font-sharing windows"
            );
        }
    });
    forget(WindowScope::Secondary(991));
    assert!(!pending(WindowScope::Secondary(991)));
    assert!(pending(WindowScope::Primary));
    drop(owner);
    reset();
}

#[test]
fn hybrid_l3_refreshes_retained_islands_without_frame_subscriptions() {
    let text = Rc::new(Cell::new("first"));
    let (owner, tree) = harness({
        let text = text.clone();
        move || {
            let text = text.clone();
            Element::new().child(reactive_view("locale", move || {
                Element::new().text(text.get())
            }))
        }
    });
    text.set("second");
    assert_eq!(tree.borrow().children[0].text.as_deref(), Some("first"));
    rebuild_all();
    begin_frame();
    assert_eq!(tree.borrow().children[0].text.as_deref(), Some("second"));
    drop(owner);
    clear_test_reconcile_root();
    reset();
}
