//! WP-3.12：公开 `<canvas>` 叶 — 布局尺寸 + paint 光栅（Builder）。

use crate::core::reactivity::{
    clear_ui_root_effect, create_effect, create_signal, dispose_effect,
    mark_next_effect_as_ui_root, ui_root_effect_dep_count,
};
use crate::core::tree::{Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::canvas::{canvas, ATTR_CANVAS_PAINT};

#[test]
fn canvas_builder_leaf_layout_bounds_and_texture() {
    let cv: Element = canvas()
        .w(160.0)
        .h(90.0)
        .paint(|ctx, size| {
            ctx.clear("secondary");
            ctx.set_fill("chart-1");
            ctx.fill_rect(8.0, 8.0, size.w - 16.0, size.h - 16.0);
            ctx.set_stroke("foreground");
            ctx.set_line_width(3.0);
            ctx.stroke_rect(8.0, 8.0, size.w - 16.0, size.h - 16.0);
            LayoutStatus::Clean
        })
        .into();
    assert!(cv.children.is_empty(), "canvas is a leaf");
    assert!(
        cv.get_attr(ATTR_CANVAS_PAINT).is_some(),
        "must register paint, not an attr stub"
    );

    let mut root = Element::new().w(200.0).h(140.0).child(cv);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 200.0, 140.0, [-1.0, -1.0]);
    let leaf = root.children.first().expect("canvas leaf");
    let [x, y, w, h] = leaf.layout_bounds;
    assert!(
        w > 150.0 && w < 170.0 && h > 80.0 && h < 100.0,
        "leaf layout_bounds should follow w/h, got [{x}, {y}, {w}, {h}]"
    );
    assert!(
        result.textured_draws.iter().any(|d| d.rect[2] > 80.0
            && d.rect[3] > 40.0
            && d.texture_key.starts_with("jadix_canvas:")),
        "canvas must emit jadix_canvas: texture; draws={:?}",
        result.textured_draws
    );
}

#[test]
fn canvas_resize_changes_raster_size() {
    let paint = |ctx: &mut dyn crate::primitives::canvas::Canvas2D,
                 size: crate::primitives::canvas::CanvasSize| {
        ctx.set_fill("#2563eb");
        ctx.fill_rect(0.0, 0.0, size.w, size.h);
        LayoutStatus::LayoutChanged
    };
    let small: Element = canvas().w(40.0).h(20.0).paint(paint).into();
    let big: Element = canvas().w(80.0).h(40.0).paint(paint).into();
    let (k0, w0, h0) =
        crate::primitives::canvas::canvas_raster_from_element_attrs(&small.attrs, 40.0, 20.0)
            .expect("small");
    let (k1, w1, h1) =
        crate::primitives::canvas::canvas_raster_from_element_attrs(&big.attrs, 80.0, 40.0)
            .expect("big");
    assert!(k0.starts_with("jadix_canvas:") && k1.starts_with("jadix_canvas:"));
    assert!((w0 - 40.0).abs() < 0.5 && (h0 - 20.0).abs() < 0.5);
    assert!((w1 - 80.0).abs() < 0.5 && (h1 - 40.0).abs() < 0.5);
}

#[test]
fn canvas_paint_peek_does_not_subscribe_ui_root() {
    clear_ui_root_effect();
    let n = create_signal(0_i32);
    mark_next_effect_as_ui_root();
    let root = create_effect(move || {
        let el: Element = canvas()
            .w(32.0)
            .h(16.0)
            .paint(move |ctx, _| {
                let _ = n.get();
                let _ = n.peek();
                ctx.clear("background");
                LayoutStatus::Clean
            })
            .into();
        let _ = crate::primitives::canvas::canvas_raster_from_element_attrs(&el.attrs, 32.0, 16.0);
    });
    assert_eq!(
        ui_root_effect_dep_count(),
        0,
        "paint is untracked: get/peek must not raise the UI-root dep set"
    );
    dispose_effect(root);
    clear_ui_root_effect();
}
