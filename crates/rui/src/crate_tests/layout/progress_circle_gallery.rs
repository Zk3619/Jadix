//! Gallery-shaped ProgressCircle: root effect defers anchors → reconcile → layout paint.

use crate::app::reactive::reset_registered_anchors;
use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
use crate::components::{progress_circle, progress_circle_svg, ProgressCircle, ProgressCircleProps, ProgressCircleSize};
use crate::core::reactivity::{
    create_effect, create_root, mark_next_effect_as_ui_root, use_signal,
};
use crate::core::tree::Element;
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::svg::ATTR_SVG_SCENE;
use crate::primitives::{hstack, vstack};
use std::cell::RefCell;
use std::rc::Rc;

fn tree_has_svg_scene(el: &Element) -> bool {
    el.get_attr(ATTR_SVG_SCENE).is_some()
        || el.children.iter().any(tree_has_svg_scene)
}

fn tree_has_placeholder(el: &Element) -> bool {
    el.template_hash.as_deref() == Some(crate::app::reactive::REACTIVE_PLACEHOLDER_HASH)
        || el.children.iter().any(tree_has_placeholder)
}

fn count_attr(el: &Element, key: &str) -> usize {
    let here = usize::from(el.get_attr(key).is_some());
    here + el.children.iter().map(|c| count_attr(c, key)).sum::<usize>()
}

/// Static SVG in gallery-shaped HStack must layout and produce textured draws.
#[test]
fn static_progress_circle_in_hstack_paints() {
    let md = progress_circle_svg(0.45, ProgressCircleSize::Md, None, None, false);
    let sm = progress_circle_svg(0.45, ProgressCircleSize::Sm, None, None, false);
    let lg = progress_circle_svg(0.45, ProgressCircleSize::Lg, None, None, false);
    let mut root: Element = hstack()
        .gap("1rem")
        .items_center()
        .children([md, sm, lg])
        .into();
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 600.0, 200.0, [-1.0, -1.0]);
    assert_eq!(count_attr(&root, ATTR_SVG_SCENE), 3, "three svg leaves");
    assert!(
        result.textured_draws.len() >= 3,
        "expected >=3 textured draws, got {:?}",
        result.textured_draws
    );
    for (i, child) in root.children.iter().enumerate() {
        let [_, _, w, h] = child.layout_bounds;
        assert!(
            w > 20.0 && h > 20.0,
            "child {i} must have positive layout bounds, got [{w}, {h}]"
        );
    }
}

/// Mimics lifecycle: ui root effect builds tree while anchor effects defer, then reconcile.
#[test]
fn progress_circle_ui_root_effect_mounts_svg() {
    reset_registered_anchors();
    clear_test_reconcile_root();

    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    let live_check = Rc::clone(&live);

    mark_next_effect_as_ui_root();
    let (_owner, _): (_, ()) = create_root(|| {
        create_effect(move || {
            let progress_val = use_signal("gal_progress_test", || 0.45f32);
            let builder_el: Element = progress_circle()
                .id("gal-pc-builder")
                .value_signal(progress_val)
                .size(ProgressCircleSize::Lg)
                .into();
            let row: Element = hstack()
                .gap("1rem")
                .items_center()
                .child(ProgressCircle(ProgressCircleProps {
                    id: "gal-pc-md".into(),
                    value_signal: Some(progress_val),
                    size: Some(ProgressCircleSize::Md),
                    ..Default::default()
                }))
                .child(ProgressCircle(ProgressCircleProps {
                    id: "gal-pc-sm".into(),
                    value_signal: Some(progress_val),
                    size: Some(ProgressCircleSize::Sm),
                    ..Default::default()
                }))
                .child(builder_el)
                .into();
            let card: Element = vstack()
                .w(600.0)
                .gap("0.5rem")
                .child(row)
                .into();
            live.borrow_mut().children = vec![card];
        });
    });

    let tree = live_check.borrow();
    assert!(
        tree_has_svg_scene(&tree),
        "after deferred anchor flush, mounted tree must contain rui_svg_scene; placeholders={} scenes={}",
        tree_has_placeholder(&tree),
        count_attr(&tree, ATTR_SVG_SCENE)
    );
    assert!(
        !tree_has_placeholder(&tree) || count_attr(&tree, ATTR_SVG_SCENE) > 0,
        "bind_prop must not leave empty placeholders without svg scenes"
    );

    clear_test_reconcile_root();
    reset_registered_anchors();
}

/// Gallery uses `switch_reactive` + `isolate_reads`; section remount must re-fill anchors.
#[test]
fn progress_circle_switch_reactive_remounts_svg() {
    use crate::app::reactive::switch_reactive;
    use crate::core::reactivity::isolate_reads;

    reset_registered_anchors();
    clear_test_reconcile_root();

    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    let live_check = Rc::clone(&live);

    mark_next_effect_as_ui_root();
    let (_owner, section): (_, _) = create_root(|| {
        let section = use_signal("gal_nav_sw", || "overlays".to_string());
        create_effect({
            let section = section.clone();
            move || {
                let sec = section.clone();
                let body = switch_reactive(
                    "test:gal:section",
                    move || sec.get(),
                    move |key| {
                        isolate_reads(|| {
                            if key == "overlays" {
                                let progress_val = use_signal("gal_progress_sw", || 0.45f32);
                                let row: Element = hstack()
                                    .gap("1rem")
                                    .items_center()
                                    .child(ProgressCircle(ProgressCircleProps {
                                        id: "gal-pc-sw".into(),
                                        value_signal: Some(progress_val),
                                        size: Some(ProgressCircleSize::Md),
                                        ..Default::default()
                                    }))
                                    .into();
                                vstack().w(600.0).child(row).into()
                            } else {
                                Element::new().w(100.0).h(40.0)
                            }
                        })
                    },
                );
                live.borrow_mut().children = vec![body];
            }
        });
        section
    });

    assert!(
        tree_has_svg_scene(&live_check.borrow()),
        "first overlays mount must produce svg scene"
    );

    section.set("forms".to_string());
    section.set("overlays".to_string());

    assert!(
        tree_has_svg_scene(&live_check.borrow()),
        "remount overlays after section switch must restore svg scene, scenes={}",
        count_attr(&live_check.borrow(), ATTR_SVG_SCENE)
    );

    clear_test_reconcile_root();
    reset_registered_anchors();
}

/// Full layout after ui-root mount: reproduces gallery paint path.
#[test]
fn progress_circle_ui_root_layout_has_textured_draws() {
    reset_registered_anchors();
    clear_test_reconcile_root();

    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));

    let live_layout = Rc::clone(&live);

    mark_next_effect_as_ui_root();
    let (_owner, _): (_, ()) = create_root(|| {
        create_effect(move || {
            let progress_val = use_signal("gal_progress_layout", || 0.45f32);
            let row: Element = hstack()
                .gap("1rem")
                .items_center()
                .child(ProgressCircle(ProgressCircleProps {
                    id: "gal-pc-layout-md".into(),
                    value_signal: Some(progress_val),
                    size: Some(ProgressCircleSize::Md),
                    ..Default::default()
                }))
                .into();
            live_layout.borrow_mut().children = vec![row];
        });
    });

    let mut root = Element::new();
    std::mem::swap(&mut root, &mut *live.borrow_mut());
    clear_test_reconcile_root();
    reset_registered_anchors();

    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 400.0, 120.0, [-1.0, -1.0]);
    assert!(
        tree_has_svg_scene(&root),
        "layout tree must still contain svg scene attrs"
    );
    assert!(
        !result.textured_draws.is_empty(),
        "gallery-shaped reactive mount must produce textured draws, got {:?}",
        result.textured_draws
    );
}
