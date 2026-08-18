//! WP-2.9 / BM7：盒模型命中与 overflow+padding 裁剪边。
//!
//! 契约见 `docs/box_model.md`：命中 = `layout_bounds`（border-box）；`outline` 只绘制；
//! overflow clip 边 = 父 border-box（padding **不**内缩裁剪矩形）。

use std::cell::Cell;
use std::rc::Rc;

use crate::app::tree_hit::find_click;
use crate::core::reactivity::{create_effect, create_root};
use crate::core::tree::{Element, LayoutTree};
use crate::primitives::r#box::r#box;
use crate::renderer::InstanceInput;
use crate::style::apply_class_list;

fn length_px(v: taffy::style::LengthPercentage) -> f32 {
    crate::layout::lp_length_px(v).unwrap_or(0.0)
}

#[test]
fn border_box_width_includes_padding_content_box_adds_outside() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let border = Element::from(r#box().w(100.0).h(40.0).px(20.0).border_box());
            let content = Element::from(r#box().w(100.0).h(40.0).px(20.0).content_box());
            let mut root = Element::new()
                .flex_col()
                .w(400.0)
                .h(200.0)
                .child(border)
                .child(content);
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 400.0, 200.0, [-1.0, -1.0]);
            let bw = root.children[0].layout_bounds[2];
            let cw = root.children[1].layout_bounds[2];
            assert!(
                (bw - 100.0).abs() < 1.0,
                "BorderBox w=100 + px=20 must stay 100 outer, got {bw}"
            );
            assert!(
                (cw - 140.0).abs() < 1.0,
                "ContentBox w=100 + px=20 must be 140 outer, got {cw}"
            );
        });
    });
}

#[test]
fn utility_box_content_matches_builder() {
    let mut via_class = Element::new().w(80.0).px(10.0);
    apply_class_list(&mut via_class, "box-content");
    let via_builder = Element::new().w(80.0).px(10.0).content_box();
    assert_eq!(via_class.style.box_sizing, via_builder.style.box_sizing);
    assert_eq!(
        via_class.style.box_sizing,
        taffy::style::BoxSizing::ContentBox
    );
}

#[test]
fn outline_does_not_expand_pointer_hit() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let hits = Rc::new(Cell::new(0u32));
            let hits_cb = hits.clone();
            let mut target: Element = r#box()
                .w(100.0)
                .h(80.0)
                .bg([0.2, 0.4, 0.8, 1.0])
                .outline([1.0, 0.2, 0.2, 1.0], 12.0)
                .into();
            target = target.on_click(move || {
                hits_cb.set(hits_cb.get() + 1);
            });
            let mut root = Element::new().w(300.0).h(200.0).p(40.0).child(target);
            let mut layout = LayoutTree::new();
            let result = layout.build_and_layout(&mut root, 300.0, 200.0, [-1.0, -1.0]);
            let el = &root.children[0];
            let b = el.layout_bounds;
            assert!(
                (b[2] - 100.0).abs() < 1.0 && (b[3] - 80.0).abs() < 1.0,
                "outline must not change layout_bounds, got {:?}",
                b
            );

            assert!(
                find_click(&root, b[0] + 10.0, b[1] + 10.0, [0.0, 0.0]).is_some(),
                "inside border-box must hit"
            );
            assert!(
                find_click(&root, b[0] - 6.0, b[1] + 40.0, [0.0, 0.0]).is_none(),
                "left of border-box but inside outline must miss"
            );
            assert!(
                find_click(&root, b[0] + b[2] + 6.0, b[1] + 40.0, [0.0, 0.0]).is_none(),
                "right of border-box but inside outline must miss"
            );
            assert!(
                find_click(&root, b[0] + 50.0, b[1] - 6.0, [0.0, 0.0]).is_none(),
                "above border-box but inside outline must miss"
            );

            let outline = result
                .instances
                .iter()
                .find(|i| i.border_width > 10.0 && (i.rect_size[0] - (b[2] + 24.0)).abs() < 2.0)
                .expect("outline instance larger than border-box");
            assert!(
                outline.rect_pos[0] < b[0] - 1.0,
                "outline geometry starts outside layout_bounds"
            );
            let _ = hits.get();
        });
    });
}

#[test]
fn border_edge_is_hittable() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut target: Element = r#box()
                .w(100.0)
                .h(60.0)
                .border([0.1, 0.1, 0.1, 1.0], 4.0)
                .bg([0.9, 0.9, 0.9, 1.0])
                .into();
            target = target.on_click(|| {});
            let mut root = Element::new().w(200.0).h(120.0).p(20.0).child(target);
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 200.0, 120.0, [-1.0, -1.0]);
            let b = root.children[0].layout_bounds;
            assert!(
                find_click(&root, b[0] + 1.0, b[1] + b[3] * 0.5, [0.0, 0.0]).is_some(),
                "click on left border (inside layout_bounds) must hit"
            );
            assert!(
                find_click(&root, b[0] + b[2] - 1.0, b[1] + b[3] * 0.5, [0.0, 0.0]).is_some(),
                "click on right border must hit"
            );
            assert!(
                find_click(&root, b[0] - 2.0, b[1] + b[3] * 0.5, [0.0, 0.0]).is_none(),
                "just outside border-box must miss"
            );
        });
    });
}

#[test]
fn overflow_clip_uses_border_box_not_padding_inset() {
    // Parent 200×200 + p=20 + overflow hidden. Child wider than content (160) so it
    // spills into the padding band. Clip edge stays the parent border-box — padding
    // does not shrink the discard rect (BM7).
    let mut child: Element = r#box()
        .w(200.0)
        .h(40.0)
        .flex_shrink(0.0)
        .bg([0.1, 0.6, 0.3, 1.0])
        .into();
    child.template_id = Some("pad-child".into());
    let mut parent: Element = r#box()
        .flex_col()
        .w(200.0)
        .h(200.0)
        .p(20.0)
        .overflow_x("hidden")
        .overflow_y("hidden")
        .bg([0.95, 0.95, 0.95, 1.0])
        .child(child)
        .into();
    parent.template_id = Some("pad-parent".into());

    let mut root = Element::new().w(240.0).h(240.0).p(10.0).child(parent);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 240.0, 240.0, [-1.0, -1.0]);

    let parent_el = &root.children[0];
    let child_el = &parent_el.children[0];
    let pb = parent_el.layout_bounds;
    let pad = length_px(parent_el.style.padding.left);
    assert!(
        (pb[2] - 200.0).abs() < 1.0,
        "parent border-box 200, got {}",
        pb[2]
    );
    assert!((pad - 20.0).abs() < 0.5, "parent padding 20, got {pad}");
    assert!(
        (child_el.layout_bounds[0] - (pb[0] + pad)).abs() < 1.5,
        "child origin after padding; child_x={} parent_x={} pad={pad}",
        child_el.layout_bounds[0],
        pb[0]
    );

    let child_inst = result
        .instances
        .iter()
        .find(|i| {
            (i.color[1] - 0.6).abs() < 0.05
                && (i.rect_size[1] - 40.0).abs() < 1.5
                && i.color[3] > 0.5
        })
        .expect("padded-overflow child instance");

    assert!(
        child_inst.clip_size[0] >= 0.0 && child_inst.clip_size[1] >= 0.0,
        "overflow clip must be set, got clip_size={:?}",
        child_inst.clip_size
    );
    assert!(
        (child_inst.clip_pos[0] - pb[0]).abs() < 2.0,
        "clip left must be parent border-box, not padding inset; clip_x={} parent_x={} pad={pad}",
        child_inst.clip_pos[0],
        pb[0]
    );
    assert!(
        (child_inst.clip_pos[1] - pb[1]).abs() < 2.0,
        "clip top must be parent border-box, not padding inset; clip_y={} parent_y={}",
        child_inst.clip_pos[1],
        pb[1]
    );
    assert!(
        (child_inst.clip_size[0] - pb[2]).abs() < 2.0,
        "clip width must match parent border-box {}, got {}",
        pb[2],
        child_inst.clip_size[0]
    );
    assert!(
        (child_inst.clip_size[1] - pb[3]).abs() < 2.0,
        "clip height must match parent border-box {}, got {}",
        pb[3],
        child_inst.clip_size[1]
    );
    let _ = InstanceInput::NO_CLIP_SIZE;
}

fn taffy_len(v: taffy::style::LengthPercentage) -> f32 {
    crate::layout::lp_length_px(v).unwrap_or_else(|| panic!("expected length, got {v:?}"))
}

#[test]
fn vw_vh_dvh_resolve_against_layout_viewport_not_parent() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let child = Element::from(r#box().w("50vw").h("25vh").min_h("10dvh").flex_shrink(0.0));
            let parent = Element::from(r#box().w(200.0).h(100.0).child(child));
            let mut root = Element::new().w("100%").h("100%").child(parent);
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
            let c = &root.children[0].children[0];
            assert!(
                (c.layout_bounds[2] - 400.0).abs() < 1.0,
                "50vw of 800 viewport → 400, not 50% of parent 200; got {}",
                c.layout_bounds[2]
            );
            assert!(
                (c.layout_bounds[3] - 150.0).abs() < 1.0,
                "25vh of 600 viewport → 150, got {}",
                c.layout_bounds[3]
            );
        });
    });
}

#[test]
fn vw_vh_update_on_fast_relayout_when_viewport_resizes() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let child = Element::from(r#box().w("50vw").h("25vh").flex_shrink(0.0));
            let mut root = Element::new().child(child);
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
            assert!((root.children[0].layout_bounds[2] - 400.0).abs() < 1.0);
            assert!((root.children[0].layout_bounds[3] - 150.0).abs() < 1.0);

            let _ =
                layout.build_and_layout_fast(&mut root, 1200.0, 800.0, [-1.0, -1.0], false, false);
            let w = root.children[0].layout_bounds[2];
            let h = root.children[0].layout_bounds[3];
            assert!(
                (w - 600.0).abs() < 1.0,
                "50vw after fast resize 1200 → 600, got {w}"
            );
            assert!(
                (h - 200.0).abs() < 1.0,
                "25vh after fast resize 800 → 200, got {h}"
            );
        });
    });
}

#[test]
fn calc_vw_updates_on_fast_relayout_when_viewport_resizes() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let el = Element::from(r#box().w("calc(50vw - 2rem)").h(20.0).flex_shrink(0.0));
            let mut root = Element::new().child(el);
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
            assert!((root.children[0].layout_bounds[2] - 368.0).abs() < 1.0);

            let _ =
                layout.build_and_layout_fast(&mut root, 1000.0, 600.0, [-1.0, -1.0], false, false);
            let w = root.children[0].layout_bounds[2];
            assert!(
                (w - 468.0).abs() < 1.0,
                "calc(50vw - 2rem) after resize 1000 → 500-32=468, got {w}"
            );
        });
    });
}

#[test]
fn em_padding_uses_element_font_size_not_root_16() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let el = Element::from(
                r#box()
                    .w(120.0)
                    .h(80.0)
                    .text_size(20.0)
                    .p("1em")
                    .border_box(),
            );
            let mut root = Element::new().w(400.0).h(200.0).child(el);
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 400.0, 200.0, [-1.0, -1.0]);
            let node = root.children[0].layout_node.expect("laid out");
            let style = layout.taffy.style(node).expect("taffy style");
            let pad = taffy_len(style.padding.left);
            assert!(
                (pad - 20.0).abs() < 0.5,
                "1em @ text_size 20 → 20px padding, not root 16; got {pad}"
            );
        });
    });
}

#[test]
fn calc_vw_minus_rem_resolves_at_layout() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let el = Element::from(r#box().w("calc(50vw - 2rem)").h(20.0).flex_shrink(0.0));
            let mut root = Element::new().w(800.0).h(600.0).child(el);
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
            let w = root.children[0].layout_bounds[2];
            assert!(
                (w - 368.0).abs() < 1.0,
                "calc(50vw - 2rem) = 400-32 = 368, got {w}"
            );
        });
    });
}

#[test]
fn calc_percent_minus_px_uses_parent_content_box() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            // Parent border-box 300 + px=20 → content 260.
            let child = Element::from(
                r#box()
                    .w("calc(100% - 40px)")
                    .h("calc(100% - 20px)")
                    .flex_shrink(0.0),
            );
            let parent = Element::from(r#box().w(300.0).h(100.0).px(20.0).py(10.0).child(child));
            let mut root = Element::new().w(800.0).h(600.0).child(parent);
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
            let c = &root.children[0].children[0];
            assert!(
                (c.layout_bounds[2] - 220.0).abs() < 1.0,
                "calc(100% - 40px) of content 260 → 220, got {}",
                c.layout_bounds[2]
            );
            // Parent content height = 100 - 20 py = 80; calc(100% - 20px) → 60.
            assert!(
                (c.layout_bounds[3] - 60.0).abs() < 1.0,
                "calc(100% - 20px) of content 80 → 60, got {}",
                c.layout_bounds[3]
            );
        });
    });
}

#[test]
fn padding_block_two_values_and_gap_units() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let el = Element::from(
                r#box()
                    .w(100.0)
                    .h(80.0)
                    .text_size(10.0)
                    .padding_block("10px 20px")
                    .padding_inline("4px 8px")
                    .gap("0.5rem")
                    .row_gap("2em"),
            );
            let mut root = Element::new().w(400.0).h(200.0).child(el);
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 400.0, 200.0, [-1.0, -1.0]);
            let node = root.children[0].layout_node.expect("laid out");
            let style = layout.taffy.style(node).expect("taffy style");
            assert!((taffy_len(style.padding.top) - 10.0).abs() < 0.5);
            assert!((taffy_len(style.padding.bottom) - 20.0).abs() < 0.5);
            assert!((taffy_len(style.padding.left) - 4.0).abs() < 0.5);
            assert!((taffy_len(style.padding.right) - 8.0).abs() < 0.5);
            assert!(
                (taffy_len(style.gap.width) - 8.0).abs() < 0.5,
                "gap 0.5rem → 8, got {:?}",
                style.gap.width
            );
            assert!(
                (taffy_len(style.gap.height) - 20.0).abs() < 0.5,
                "row_gap 2em @ 10px → 20, got {:?}",
                style.gap.height
            );
        });
    });
}

#[test]
fn into_border_widths_parses_1_to_4_values() {
    use crate::core::tree::IntoBorderWidths;
    assert_eq!(4.0_f32.into_border_widths(), [4.0; 4]);
    assert_eq!("2px 8px".into_border_widths(), [2.0, 8.0, 2.0, 8.0]);
    assert_eq!("1px 2px 3px".into_border_widths(), [1.0, 2.0, 3.0, 2.0]);
    assert_eq!(
        "4px 8px 12px 16px".into_border_widths(),
        [4.0, 8.0, 12.0, 16.0]
    );
    assert_eq!("0.25rem".into_border_widths(), [4.0; 4]);
}

#[test]
fn per_side_border_writes_taffy_rect_and_paints_strips() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let el = Element::from(
                r#box()
                    .w(100.0)
                    .h(80.0)
                    .bg([0.9, 0.9, 0.9, 1.0])
                    .border_style("solid")
                    .border_top([1.0, 0.0, 0.0, 1.0], 4.0)
                    .border_right([0.0, 1.0, 0.0, 1.0], 8.0)
                    .border_bottom([0.0, 0.0, 1.0, 1.0], 12.0)
                    .border_left([1.0, 1.0, 0.0, 1.0], 16.0),
            );
            let mut root = Element::new().w(240.0).h(160.0).p(10.0).child(el);
            let mut layout = LayoutTree::new();
            let result = layout.build_and_layout(&mut root, 240.0, 160.0, [-1.0, -1.0]);
            let node = root.children[0].layout_node.expect("laid out");
            let style = layout.taffy.style(node).expect("taffy style");
            assert!((taffy_len(style.border.top) - 4.0).abs() < 0.5);
            assert!((taffy_len(style.border.right) - 8.0).abs() < 0.5);
            assert!((taffy_len(style.border.bottom) - 12.0).abs() < 0.5);
            assert!((taffy_len(style.border.left) - 16.0).abs() < 0.5);

            let b = root.children[0].layout_bounds;
            assert!(
                (b[2] - 100.0).abs() < 1.0 && (b[3] - 80.0).abs() < 1.0,
                "BorderBox outer stays 100×80, got {:?}",
                b
            );

            let top = result
                .instances
                .iter()
                .find(|i| {
                    (i.rect_size[1] - 4.0).abs() < 0.6
                        && (i.rect_size[0] - 100.0).abs() < 1.5
                        && (i.color[0] - 1.0).abs() < 0.05
                        && i.color[1] < 0.1
                        && i.border_width < 0.1
                })
                .expect("top border strip");
            assert!((top.rect_pos[0] - b[0]).abs() < 1.5);
            assert!((top.rect_pos[1] - b[1]).abs() < 1.5);

            let left = result
                .instances
                .iter()
                .find(|i| {
                    (i.rect_size[0] - 16.0).abs() < 0.6
                        && (i.color[0] - 1.0).abs() < 0.05
                        && (i.color[1] - 1.0).abs() < 0.05
                        && i.border_width < 0.1
                })
                .expect("left border strip");
            assert!(
                (left.rect_size[1] - 64.0).abs() < 1.5,
                "left strip height = 80-4-12 = 64, got {}",
                left.rect_size[1]
            );
            assert!(
                result
                    .instances
                    .iter()
                    .all(|i| (i.border_width - 16.0).abs() > 0.5),
                "non-uniform border must not use max-width SDF stroke"
            );
        });
    });
}

#[test]
fn per_side_border_content_box_adds_outside() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let el = Element::from(
                r#box()
                    .w(100.0)
                    .h(40.0)
                    .content_box()
                    .border_style("solid")
                    .border_width("10px 20px 6px 4px"),
            );
            let mut root = Element::new().w(300.0).h(120.0).child(el);
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 300.0, 120.0, [-1.0, -1.0]);
            let b = root.children[0].layout_bounds;
            assert!(
                (b[2] - 124.0).abs() < 1.0,
                "ContentBox w=100 + L4 + R20 → 124, got {}",
                b[2]
            );
            assert!(
                (b[3] - 56.0).abs() < 1.0,
                "ContentBox h=40 + T10 + B6 → 56, got {}",
                b[3]
            );
        });
    });
}

#[test]
fn uniform_border_still_uses_sdf_stroke() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let el = Element::from(
                r#box()
                    .w(80.0)
                    .h(40.0)
                    .bg([0.2, 0.3, 0.4, 1.0])
                    .border([0.1, 0.1, 0.1, 1.0], 3.0),
            );
            let mut root = Element::new().w(200.0).h(100.0).p(8.0).child(el);
            let mut layout = LayoutTree::new();
            let result = layout.build_and_layout(&mut root, 200.0, 100.0, [-1.0, -1.0]);
            let stroke = result
                .instances
                .iter()
                .find(|i| (i.border_width - 3.0).abs() < 0.5)
                .expect("uniform border uses SDF border_width");
            assert!((stroke.rect_size[0] - 80.0).abs() < 1.5);
            assert!((stroke.rect_size[1] - 40.0).abs() < 1.5);
        });
    });
}
