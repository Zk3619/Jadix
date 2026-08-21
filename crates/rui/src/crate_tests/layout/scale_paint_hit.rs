//! CSS `scale` paint geometry must match `visual_paint_aabb` hit/hover bounds.
//!
//! Baking scale into `rect_size` **and** the SDF affine linear part painted ~scale²
//! while `:hover` leave used scale¹ (gallery red `hover:scale-x-150`).

use crate::core::reactivity::{create_effect, create_root};
use crate::core::tree::{visual_paint_aabb, Element, LayoutTree};
use crate::primitives::r#box::r#box;

#[test]
fn scaled_fill_instance_matches_visual_aabb_not_scale_squared() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut tile: Element = r#box()
                .w(64.0)
                .h(64.0)
                .bg([1.0, 0.0, 0.0, 1.0])
                .into();
            tile.style.scale = [1.5, 1.0];
            let mut root = Element::new().w(400.0).h(200.0).p(40.0).child(tile);
            let mut layout = LayoutTree::new();
            let result = layout.build_and_layout(&mut root, 400.0, 200.0, [-1.0, -1.0]);
            let el = &root.children[0];
            let paint = visual_paint_aabb(el, [0.0, 0.0]);
            let fill = result
                .instances
                .iter()
                .find(|i| i.color[0] > 0.5 && i.rect_size[0] > 1.0)
                .expect("expected a red fill instance");
            assert!(
                (fill.rect_size[0] - paint[2]).abs() < 1.0,
                "fill width must match visual AABB (got instance {} vs paint {}), not scale²",
                fill.rect_size[0],
                paint[2]
            );
            assert!(
                (fill.rect_size[0] - el.layout_bounds[2] * 1.5).abs() < 1.0,
                "fill width should be layout×1.5, got {} (layout {})",
                fill.rect_size[0],
                el.layout_bounds[2]
            );
            // Independent scale is baked into rect_size; matrix linear scale must stay ~1.
            assert!(
                (fill.transform[0] - 1.0).abs() < 0.02,
                "transform.a must not re-apply style.scale (got {})",
                fill.transform[0]
            );
        });
    });
}
