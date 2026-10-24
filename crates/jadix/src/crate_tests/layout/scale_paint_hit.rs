//! CSS `scale` paint geometry must match `visual_paint_aabb` hit/hover bounds.
//!
//! Baking scale into `rect_size` **and** the SDF affine linear part painted ~scale²
//! while `:hover` leave used scale¹ (gallery red `hover:scale-x-150`).

use crate::core::reactivity::{create_effect, create_root};
use crate::core::tree::{visual_paint_aabb, Element, LayoutTree};
use crate::primitives::r#box::r#box;

#[test]
fn scaled_preview_child_translation_uses_logical_distance() {
    let (_owner, _) = create_root(|| {
        for zoom in [0.35, 0.57, 1.0, 1.5] {
            let mut root = Element::new().w(400.0).h(240.0).scale(zoom)
                .transform_origin("left top").child(
                    Element::new().absolute().left(20.0).top(30.0).w(40.0).h(20.0)
                        .bg([1.0, 0.0, 0.0, 1.0]),
                );
            let mut layout = LayoutTree::new();
            let first = layout.build_and_layout(&mut root, 400.0, 240.0, [-1.0; 2]);
            let initial = first.instances.iter().find(|fill| fill.color[0] > 0.9).unwrap().rect_pos;
            root.children[0].style.translate = [100.0, 60.0];
            let moved = layout.build_and_layout(&mut root, 400.0, 240.0, [-1.0; 2]);
            let fill = moved.instances.iter().find(|fill| fill.color[0] > 0.9).unwrap();
            for axis in 0..2 {
                let expected = [100.0, 60.0][axis] * zoom;
                assert!((fill.rect_pos[axis] - initial[axis] - expected).abs() < 0.1,
                    "zoom={zoom}, axis={axis}, initial={initial:?}, moved={:?}, expected delta={expected}", fill.rect_pos);
            }
            let hit = visual_paint_aabb(&root.children[0], [0.0; 2]);
            assert!((hit[0] - fill.rect_pos[0]).abs() < 0.1);
            assert!((hit[1] - fill.rect_pos[1]).abs() < 0.1);
        }
    });
}

#[test]
fn scaled_preview_transform_list_translation_matches_paint_and_hit() {
    let (_owner, _) = create_root(|| {
        for zoom in [0.35, 0.57, 1.0, 1.5] {
            let mut child = Element::new().absolute().left(20.0).top(30.0)
                .w(80.0).h(40.0).bg([1.0, 0.0, 0.0, 1.0]);
            child.style.transform = crate::style::parse_transform("translate(100px, 50%)");
            let mut root = Element::new().w(400.0).h(240.0).scale(zoom)
                .transform_origin("left top").child(child);
            let mut tree = LayoutTree::new();
            let result = tree.build_and_layout(&mut root, 400.0, 240.0, [-1.0; 2]);
            let fill = result.instances.iter().find(|quad| quad.color[0] > 0.9).unwrap();
            let rendered = crate::style::aabb_of_centered_transform(
                [fill.rect_pos[0], fill.rect_pos[1], fill.rect_size[0], fill.rect_size[1]],
                &fill.transform);
            let hit = visual_paint_aabb(&root.children[0], [0.0; 2]);
            assert!((rendered[0] - 120.0 * zoom).abs() < 0.1, "zoom={zoom}, rect={rendered:?}");
            assert!((rendered[1] - 50.0 * zoom).abs() < 0.1, "zoom={zoom}, rect={rendered:?}");
            for axis in 0..4 {
                assert!((rendered[axis] - hit[axis]).abs() < 0.1);
            }
        }
    });
}

#[test]
fn scaled_preview_input_chrome_matches_glyph_scale() {
    let (_owner, _) = create_root(|| {
        let mut baseline = None;
        for zoom in [1.0, 0.35, 0.57, 1.5] {
            let mut field = Element::new().w(240.0).h(28.0).text("Shanghai").text_size(14.0);
            field.text_style.input_caret = Some(crate::style::InputCaretDraw {
                caret_byte_index: 8,
                selection: Some((0, 8)),
                selection_bg: [80, 120, 200, 90],
                overwrite_highlight: None,
                preedit_underline: None,
                extra_carets: Vec::new(),
                extra_selections: Vec::new(),
                extra_preedit_underlines: Vec::new(),
                preedit_ul: Vec::new(),
                drop_caret: false,
                caret_color: None,
                selection_foreground: None,
            });
            let mut root = Element::new().w(400.0).h(240.0).scale(zoom)
                .transform_origin("left top").child(field);
            let mut layout = LayoutTree::new();
            let result = layout.build_and_layout(&mut root, 400.0, 240.0, [-1.0; 2]);
            let text = result.text_nodes.first().unwrap();
            let chrome: Vec<_> = result.instances.iter().chain(&result.input_chrome_instances)
                .map(|quad| [(quad.rect_pos[0] - text.x) / zoom,
                    (quad.rect_pos[1] - text.y) / zoom,
                    quad.rect_size[0] / zoom, quad.rect_size[1] / zoom]).collect();
            assert!(!chrome.is_empty());
            if let Some(expected) = baseline.as_ref() {
                let expected: &Vec<[f32; 4]> = expected;
                assert_eq!(chrome.len(), expected.len());
                for (actual, expected) in chrome.iter().zip(expected) {
                    for axis in 0..4 {
                        assert!((actual[axis] - expected[axis]).abs() < 0.1,
                            "zoom={zoom}, chrome={actual:?}, expected={expected:?}");
                    }
                }
            } else {
                baseline = Some(chrome);
            }
        }
    });
}

#[test]
fn scaled_fill_instance_matches_visual_aabb_not_scale_squared() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut tile: Element = r#box().w(64.0).h(64.0).bg([1.0, 0.0, 0.0, 1.0]).into();
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

#[test]
fn scaled_fill_origin_left_top_matches_visual_aabb() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut tile: Element = r#box().w(64.0).h(64.0).bg([1.0, 0.0, 0.0, 1.0]).into();
            tile.style.scale = [1.5, 1.0];
            tile.style.transform_origin = crate::style::parse_transform_origin("left top");
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
                (fill.rect_pos[0] - paint[0]).abs() < 1.0
                    && (fill.rect_size[0] - paint[2]).abs() < 1.0,
                "left-top scale bake must match hit AABB: instance {:?}/{:?} vs paint {paint:?}",
                fill.rect_pos,
                fill.rect_size
            );
            // Grows to the right of the layout box, not equally both sides.
            assert!(
                (fill.rect_pos[0] - el.layout_bounds[0]).abs() < 1.0,
                "left-top origin must pin vis_x to layout left, got {} vs layout {}",
                fill.rect_pos[0],
                el.layout_bounds[0]
            );
            let center_x =
                el.layout_bounds[0] + (el.layout_bounds[2] - el.layout_bounds[2] * 1.5) * 0.5;
            assert!(
                (fill.rect_pos[0] - center_x).abs() > 1.0,
                "left-top vis_x must not equal center-origin inset {center_x}"
            );
            let slot = result.node_paints.iter().find(|n| {
                (n.bounds[2] - paint[2]).abs() < 2.0 && (n.bounds[0] - paint[0]).abs() < 2.0
            });
            assert!(
                slot.is_some(),
                "node-present now-bounds must follow origin scale AABB {paint:?}, got {:?}",
                result
                    .node_paints
                    .iter()
                    .map(|n| n.bounds)
                    .collect::<Vec<_>>()
            );
        });
    });
}

fn aabb_contains(aabb: [f32; 4], p: [f32; 2]) -> bool {
    p[0] >= aabb[0] && p[0] < aabb[0] + aabb[2] && p[1] >= aabb[1] && p[1] < aabb[1] + aabb[3]
}

#[test]
fn visual_paint_aabb_origin_left_covers_right_overhang_not_left() {
    let mut el = Element::new();
    el.layout_bounds = [400.0, 200.0, 64.0, 64.0];
    el.style.scale = [1.5, 1.0];
    el.style.transform_origin = crate::style::parse_transform_origin("left top");
    let paint = visual_paint_aabb(&el, [0.0, 0.0]);
    assert!(
        (paint[0] - 400.0).abs() < 0.5 && (paint[2] - 96.0).abs() < 0.5,
        "left-top scale-x 1.5 AABB should grow right from 400, got {paint:?}"
    );
    assert!(
        aabb_contains(paint, [470.0, 232.0]),
        "right overhang must be hittable, aabb={paint:?}"
    );
    assert!(
        !aabb_contains(paint, [390.0, 232.0]),
        "old center-origin left overhang must miss, aabb={paint:?}"
    );
    el.style.transform_origin = crate::style::parse_transform_origin("center");
    let center = visual_paint_aabb(&el, [0.0, 0.0]);
    assert!(
        aabb_contains(center, [390.0, 232.0]),
        "center origin must still cover the left overhang, aabb={center:?}"
    );
}
