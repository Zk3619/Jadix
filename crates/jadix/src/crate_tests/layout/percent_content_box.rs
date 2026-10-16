//! Percentage size resolves against the parent's **content box** (CSS).
//!
//! With BorderBox (Tailwind / Jadix default): parent `width` includes padding+border;
//! child `width: 100%` = parent_w − padding − border.

use crate::core::reactivity::{create_effect, create_root};
use crate::core::tree::{Element, LayoutTree};
use crate::primitives::r#box::r#box;

fn layout_nested(parent: Element) -> (f32, f32, f32, f32) {
    let mut root = Element::new().w("100%").h("100%").child(parent);
    let mut layout = LayoutTree::new();
    let _ = layout.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
    let p = &root.children[0];
    let c = &p.children[0];
    (
        p.layout_bounds[2],
        c.layout_bounds[2],
        p.layout_bounds[0],
        c.layout_bounds[0],
    )
}

#[test]
fn percent_width_is_parent_content_box_not_border_box() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            // Parent border-box 300 with px=20 → content width 260.
            let child = Element::from(r#box().w("100%").h(20.0));
            let parent = Element::from(r#box().w(300.0).h(100.0).px(20.0).py(10.0).child(child));
            let (parent_w, child_w, parent_x, child_x) = layout_nested(parent);

            assert!(
                (parent_w - 300.0).abs() < 1.0,
                "parent border-box should stay 300, got {parent_w}"
            );
            assert!(
                (child_w - 260.0).abs() < 1.0,
                "child width:100% must be parent content (260), got {child_w} (parent={parent_w})"
            );
            assert!(
                (child_x - (parent_x + 20.0)).abs() < 1.0,
                "child should start after parent padding; child_x={child_x} parent_x={parent_x}"
            );
        });
    });
}

#[test]
fn percent_width_subtracts_parent_border_under_border_box() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            // 300 − 2*20 padding − 2*1 border = 258 content.
            let child = Element::from(r#box().w("100%").h(20.0));
            let parent = Element::from(
                r#box()
                    .w(300.0)
                    .h(100.0)
                    .px(20.0)
                    .border([0.0, 0.0, 0.0, 1.0], 1.0)
                    .child(child),
            );
            let (parent_w, child_w, parent_x, child_x) = layout_nested(parent);
            assert!(
                (parent_w - 300.0).abs() < 1.0,
                "parent border-box 300, got {parent_w}"
            );
            assert!(
                (child_w - 258.0).abs() < 1.0,
                "child 100% must exclude parent padding+border → 258, got {child_w}"
            );
            assert!(
                (child_x - (parent_x + 21.0)).abs() < 1.0,
                "child origin = parent + border + padding; child_x={child_x} parent_x={parent_x}"
            );
        });
    });
}

#[test]
fn percent_width_child_with_own_padding_stays_inside_parent_content() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            // BorderBox: child w(100%) includes its own padding → still equals parent content.
            let child = Element::from(r#box().w("100%").px(24.0).h(40.0));
            let parent = Element::from(r#box().w(300.0).h(100.0).px(20.0).child(child));
            let (_parent_w, child_w, _, _) = layout_nested(parent);
            assert!(
                (child_w - 260.0).abs() < 1.0,
                "border-box child w:100%+padding must still equal parent content 260, got {child_w}"
            );
        });
    });
}

#[test]
fn percent_height_is_parent_content_box() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let child = Element::from(r#box().w(50.0).h("100%"));
            let parent = Element::from(r#box().w(100.0).h(200.0).py(20.0).child(child));
            let mut root = Element::new().w("100%").h("100%").child(parent);
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
            let parent_h = root.children[0].layout_bounds[3];
            let child_h = root.children[0].children[0].layout_bounds[3];
            assert!(
                (parent_h - 200.0).abs() < 1.0,
                "parent height border-box 200, got {parent_h}"
            );
            assert!(
                (child_h - 160.0).abs() < 1.0,
                "child height:100% must be parent content (160), got {child_h}"
            );
        });
    });
}

#[test]
fn max_width_percent_caps_to_parent_content_not_inner() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            // Parent content 260. Child would be 400 without the cap.
            let child = Element::from(r#box().w(400.0).max_w("100%").h(20.0));
            let parent = Element::from(r#box().w(300.0).h(100.0).px(20.0).child(child));
            let (_parent_w, child_w, _, _) = layout_nested(parent);
            assert!(
                (child_w - 260.0).abs() < 1.0,
                "max-width:100% must cap to parent content 260, not inner 400; got {child_w}"
            );
        });
    });
}

#[test]
fn min_width_percent_is_parent_content() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let child = Element::from(r#box().w(40.0).min_w("100%").h(20.0));
            let parent = Element::from(r#box().w(300.0).h(100.0).px(20.0).child(child));
            let (_parent_w, child_w, _, _) = layout_nested(parent);
            assert!(
                (child_w - 260.0).abs() < 1.0,
                "min-width:100% must be parent content 260, got {child_w}"
            );
        });
    });
}

#[test]
fn max_height_percent_caps_to_parent_content() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let child = Element::from(r#box().w(50.0).h(400.0).max_h("100%"));
            let parent = Element::from(r#box().w(100.0).h(200.0).py(20.0).child(child));
            let mut root = Element::new().w("100%").h("100%").child(parent);
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
            let child_h = root.children[0].children[0].layout_bounds[3];
            assert!(
                (child_h - 160.0).abs() < 1.0,
                "max-height:100% must cap to parent content 160, got {child_h}"
            );
        });
    });
}

#[test]
fn fit_content_width_hugs_in_stretch_column() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let inner = Element::from(r#box().w(80.0).h(20.0));
            let child = Element::from(r#box().w("fit-content").child(inner));
            let parent = Element::from(r#box().flex_col().w(400.0).h(100.0).child(child));
            let mut root = Element::new().w("100%").h("100%").child(parent);
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
            let child_w = root.children[0].children[0].layout_bounds[2];
            assert!(
                (child_w - 80.0).abs() < 1.0,
                "w(fit-content) in a stretch column must hug 80, not fill 400; got {child_w}"
            );
        });
    });
}

#[test]
fn slash_fraction_width_is_parent_content_not_inner() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            // Parent border-box 300, px=20 → content 260. 1/2 → 130; 3/4 → 195.
            let half = Element::from(r#box().w("1/2").h(20.0));
            let parent = Element::from(r#box().w(300.0).h(80.0).px(20.0).child(half));
            let (_parent_w, child_w, _, _) = layout_nested(parent);
            assert!(
                (child_w - 130.0).abs() < 1.0,
                "w(1/2) must be half of parent content 260, not inner content; got {child_w}"
            );

            let three_quarters = Element::from(r#box().w("3/4").h(20.0));
            let parent = Element::from(r#box().w(300.0).h(80.0).px(20.0).child(three_quarters));
            let (_parent_w, child_w, _, _) = layout_nested(parent);
            assert!(
                (child_w - 195.0).abs() < 1.0,
                "w(3/4) must be 75% of parent content 260, got {child_w}"
            );
        });
    });
}

#[test]
fn slash_fraction_min_max_are_parent_content() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let child = Element::from(r#box().w(40.0).min_w("1/2").h(20.0));
            let parent = Element::from(r#box().w(300.0).h(80.0).px(20.0).child(child));
            let (_parent_w, child_w, _, _) = layout_nested(parent);
            assert!(
                (child_w - 130.0).abs() < 1.0,
                "min_w(1/2) must be half of parent content 260, got {child_w}"
            );

            let child = Element::from(r#box().w(400.0).max_w("1/2").h(20.0));
            let parent = Element::from(r#box().w(300.0).h(80.0).px(20.0).child(child));
            let (_parent_w, child_w, _, _) = layout_nested(parent);
            assert!(
                (child_w - 130.0).abs() < 1.0,
                "max_w(1/2) must cap to half of parent content 260, got {child_w}"
            );
        });
    });
}

#[test]
fn flex_item_width_100_percent_plus_sibling_overflows_by_design() {
    // Documents CSS: width:100% of a flex row is the full inner main size; a sibling
    // cannot share that space without exceeding the container (CardAction footgun).
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let title = Element::from(r#box().w("100%").h(20.0).min_w(0.0));
            let action = Element::from(r#box().w(40.0).h(20.0).flex_shrink(0.0));
            let row = Element::from(
                r#box()
                    .flex_row()
                    .w(200.0)
                    .h(40.0)
                    .child(title)
                    .child(action),
            );
            let mut root = Element::new().w("100%").h("100%").child(row);
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
            let row_el = &root.children[0];
            let title_w = row_el.children[0].layout_bounds[2];
            let action_w = row_el.children[1].layout_bounds[2];
            // Under flex shrink the title may compress, but the *intent* of w(100%)+sibling
            // is wrong for a header; assert action keeps its size and title is not > row.
            assert!((action_w - 40.0).abs() < 1.0, "action width {action_w}");
            assert!(
                title_w <= 200.0 + 1.0,
                "title must not report wider than row; got {title_w}"
            );
        });
    });
}
