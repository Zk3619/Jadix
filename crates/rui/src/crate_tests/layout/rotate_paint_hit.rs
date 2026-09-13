//! Rotate paint geometry must match `visual_paint_aabb` hit/hover bounds.
//!
//! Wave 27 left rotate as an unrotated scaled AABB. VS origin `tx/ty` then
//! painted ink that hover / dest-reset `now` did not cover.

use crate::app::tree_hit::{element_self_pointer_hit, find_click, visual_hit_box};
use crate::core::reactivity::{create_effect, create_root};
use crate::core::tree::{visual_paint_aabb, Element, LayoutTree};
use crate::primitives::r#box::r#box;

fn aabb_contains(aabb: [f32; 4], p: [f32; 2]) -> bool {
    p[0] >= aabb[0] && p[0] < aabb[0] + aabb[2] && p[1] >= aabb[1] && p[1] < aabb[1] + aabb[3]
}

#[test]
fn visual_paint_aabb_origin_left_rotate_90_shifts_left() {
    let mut el = Element::new();
    el.layout_bounds = [400.0, 200.0, 100.0, 100.0];
    el.style.rotate_deg = 90.0;
    el.style.transform_origin = crate::style::parse_transform_origin("left top");
    let paint = visual_paint_aabb(&el, [0.0, 0.0]);
    assert!(
        (paint[0] - 300.0).abs() < 0.5 && (paint[1] - 200.0).abs() < 0.5,
        "left-top 90° on 100×100 must sit at (300, 200), got {paint:?}"
    );
    assert!(
        (paint[2] - 100.0).abs() < 0.5 && (paint[3] - 100.0).abs() < 0.5,
        "AABB size must stay 100×100, got {paint:?}"
    );
    assert!(
        aabb_contains(paint, [350.0, 250.0]),
        "painted ink left of the layout box must be hittable, aabb={paint:?}"
    );
    assert!(
        !aabb_contains(paint, [450.0, 250.0]),
        "old layout center must miss after origin-left 90°, aabb={paint:?}"
    );
    el.style.transform_origin = crate::style::parse_transform_origin("center");
    let center = visual_paint_aabb(&el, [0.0, 0.0]);
    assert!(
        aabb_contains(center, [450.0, 250.0]),
        "center origin 90° on a square keeps the layout AABB, aabb={center:?}"
    );
}

#[test]
fn rotated_fill_origin_left_top_aabb_matches_visual() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut tile: Element = r#box().w(100.0).h(100.0).bg([1.0, 0.0, 0.0, 1.0]).into();
            tile.style.rotate_deg = 90.0;
            tile.style.transform_origin = crate::style::parse_transform_origin("left top");
            tile = tile.absolute().left(0.0).top(0.0);
            let mut root = Element::new().w(400.0).h(300.0).child(tile);
            let mut layout = LayoutTree::new();
            let result = layout.build_and_layout(&mut root, 400.0, 300.0, [-1.0, -1.0]);
            let el = &root.children[0];
            let paint = visual_paint_aabb(el, [0.0, 0.0]);
            let fill = result
                .instances
                .iter()
                .find(|i| i.color[0] > 0.5 && i.rect_size[0] > 80.0)
                .expect("expected a red fill instance");
            let inst_aabb = crate::style::aabb_of_centered_transform(
                [
                    fill.rect_pos[0],
                    fill.rect_pos[1],
                    fill.rect_size[0],
                    fill.rect_size[1],
                ],
                &fill.transform,
            );
            assert!(
                (inst_aabb[0] - paint[0]).abs() < 1.0
                    && (inst_aabb[1] - paint[1]).abs() < 1.0
                    && (inst_aabb[2] - paint[2]).abs() < 1.0
                    && (inst_aabb[3] - paint[3]).abs() < 1.0,
                "hit AABB must match VS instance AABB: instance {inst_aabb:?} vs paint {paint:?}"
            );
            assert!(
                (paint[0] - (el.layout_bounds[0] - 100.0)).abs() < 1.0,
                "left-top 90° must shift hit AABB left by the box width, paint={paint:?} layout={:?}",
                el.layout_bounds
            );
            let slot = result.node_paints.iter().find(|n| {
                (n.bounds[2] - paint[2]).abs() < 2.0 && (n.bounds[0] - paint[0]).abs() < 2.0
            });
            assert!(
                slot.is_some(),
                "node-present now-bounds must follow rotated AABB {paint:?}, got {:?}",
                result
                    .node_paints
                    .iter()
                    .map(|n| n.bounds)
                    .collect::<Vec<_>>()
            );
        });
    });
}

#[test]
fn visual_paint_aabb_rotate_45_center_grows_square() {
    let mut el = Element::new();
    el.layout_bounds = [32.0, 32.0, 16.0, 16.0];
    el.style.rotate_deg = 45.0;
    let paint = visual_paint_aabb(&el, [0.0, 0.0]);
    let diag = 16.0 * std::f32::consts::SQRT_2;
    assert!(
        (paint[2] - diag).abs() < 0.5 && (paint[3] - diag).abs() < 0.5,
        "45° about center must grow the AABB to the diagonal, got {paint:?}"
    );
    assert!(
        paint[0] < 32.0 - 2.0 && paint[1] < 32.0 - 2.0,
        "45° corners must hang outside the layout box, got {paint:?}"
    );
}

#[test]
fn visual_paint_aabb_ancestor_rotate_90_swaps_nonsquare() {
    let mut el = Element::new();
    el.layout_bounds = [60.0, 90.0, 80.0, 20.0];
    el.paint_ancestor_linear = crate::style::mat2d_rotate(90.0f32.to_radians());
    let paint = visual_paint_aabb(&el, [0.0, 0.0]);
    assert!(
        (paint[2] - 20.0).abs() < 0.5 && (paint[3] - 80.0).abs() < 0.5,
        "ancestor 90° must rotate the hit AABB of an 80×20 child, got {paint:?}"
    );
    assert!(
        aabb_contains(paint, [100.0, 70.0]),
        "painted corners above the layout box must be hittable, aabb={paint:?}"
    );
    assert!(
        !aabb_contains(paint, [70.0, 100.0]),
        "old layout left overhang must miss after ancestor 90°, aabb={paint:?}"
    );
}

#[test]
fn ancestor_rotate_child_aabb_matches_collect() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let child: Element = r#box()
                .w(80.0)
                .h(20.0)
                .bg([1.0, 0.0, 0.0, 1.0])
                .absolute(true)
                .left(60.0)
                .top(90.0)
                .into();
            let mut parent = Element::new().w(200.0).h(200.0).child(child);
            parent.style.rotate_deg = 90.0;
            let mut root = Element::new().w(400.0).h(400.0).child(parent);
            let mut layout = LayoutTree::new();
            let result = layout.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
            let child = &root.children[0].children[0];
            let paint = visual_paint_aabb(child, [0.0, 0.0]);
            let fill = result
                .instances
                .iter()
                .find(|i| i.color[0] > 0.5 && (i.rect_size[0] - 80.0).abs() < 1.0)
                .expect("expected the 80×20 child fill");
            let inst_aabb = crate::style::aabb_of_centered_transform(
                [
                    fill.rect_pos[0],
                    fill.rect_pos[1],
                    fill.rect_size[0],
                    fill.rect_size[1],
                ],
                &fill.transform,
            );
            assert!(
                (inst_aabb[0] - paint[0]).abs() < 1.5
                    && (inst_aabb[1] - paint[1]).abs() < 1.5
                    && (inst_aabb[2] - paint[2]).abs() < 1.5
                    && (inst_aabb[3] - paint[3]).abs() < 1.5,
                "child hit AABB must match VS under ancestor rotate: instance {inst_aabb:?} vs paint {paint:?}"
            );
            assert!(
                (paint[2] - 20.0).abs() < 2.0 && (paint[3] - 80.0).abs() < 2.0,
                "centered 80×20 child under parent 90° must swap AABB, got {paint:?}"
            );
        });
    });
}

#[test]
fn visual_paint_aabb_offset_rotate_90_swaps_nonsquare() {
    let mut el = Element::new();
    el.layout_bounds = [100.0, 100.0, 40.0, 20.0];
    el.style.offset_path = crate::style::parse_offset_path(r#"path("M 0 0 L 40 0")"#).unwrap();
    el.style.offset_distance = 0.0;
    el.style.offset_rotate_deg = Some(90.0);
    let paint = visual_paint_aabb(&el, [0.0, 0.0]);
    assert!(
        (paint[2] - 20.0).abs() < 0.5 && (paint[3] - 40.0).abs() < 0.5,
        "offset-rotate 90° must swap the hit AABB, got {paint:?}"
    );
}

#[test]
fn visual_paint_aabb_offset_distance_shifts_box() {
    let mut el = Element::new();
    el.layout_bounds = [100.0, 100.0, 40.0, 20.0];
    el.style.offset_path = crate::style::parse_offset_path(r#"path("M 0 0 L 40 0")"#).unwrap();
    el.style.offset_distance = 1.0;
    el.style.offset_rotate_deg = Some(0.0);
    let paint = visual_paint_aabb(&el, [0.0, 0.0]);
    assert!(
        (paint[0] - 140.0).abs() < 0.5 && (paint[1] - 100.0).abs() < 0.5,
        "offset-distance 1 on a 40px path must shift hit AABB, got {paint:?}"
    );
}

#[test]
fn offset_rotate_fill_aabb_matches_visual() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut tile: Element = r#box().w(40.0).h(20.0).bg([1.0, 0.0, 0.0, 1.0]).into();
            tile.style.offset_path =
                crate::style::parse_offset_path(r#"path("M 0 0 L 40 0")"#).unwrap();
            tile.style.offset_distance = 0.0;
            tile.style.offset_rotate_deg = Some(90.0);
            tile = tile.absolute().left(0.0).top(0.0);
            let mut root = Element::new().w(200.0).h(200.0).child(tile);
            let mut layout = LayoutTree::new();
            let result = layout.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
            let el = &root.children[0];
            let paint = visual_paint_aabb(el, [0.0, 0.0]);
            let fill = result
                .instances
                .iter()
                .find(|i| i.color[0] > 0.5 && i.rect_size[0] > 30.0)
                .expect("expected offset-rotated fill");
            let inst_aabb = crate::style::aabb_of_centered_transform(
                [
                    fill.rect_pos[0],
                    fill.rect_pos[1],
                    fill.rect_size[0],
                    fill.rect_size[1],
                ],
                &fill.transform,
            );
            assert!(
                (inst_aabb[0] - paint[0]).abs() < 1.5
                    && (inst_aabb[1] - paint[1]).abs() < 1.5
                    && (inst_aabb[2] - paint[2]).abs() < 1.5
                    && (inst_aabb[3] - paint[3]).abs() < 1.5,
                "offset-rotate hit AABB must match VS: instance {inst_aabb:?} vs paint {paint:?}"
            );
        });
    });
}

#[test]
fn visual_hit_box_matches_visual_paint_aabb_origin_left_rotate() {
    let mut el = Element::new();
    el.layout_bounds = [400.0, 200.0, 100.0, 100.0];
    el.style.rotate_deg = 90.0;
    el.style.transform_origin = crate::style::parse_transform_origin("left top");
    let paint = visual_paint_aabb(&el, [0.0, 0.0]);
    let hit = visual_hit_box(&el, [0.0, 0.0]);
    assert_eq!(
        paint, hit,
        "click/on_hover box must be the same AABB :hover already uses"
    );
}

#[test]
fn visual_hit_box_matches_visual_paint_aabb_origin_left_scale() {
    let mut el = Element::new();
    el.layout_bounds = [400.0, 200.0, 64.0, 64.0];
    el.style.scale = [1.5, 1.0];
    el.style.transform_origin = crate::style::parse_transform_origin("left top");
    let paint = visual_paint_aabb(&el, [0.0, 0.0]);
    let hit = visual_hit_box(&el, [0.0, 0.0]);
    assert_eq!(
        paint, hit,
        "scale-about-origin click box must match paint AABB"
    );
}

#[test]
fn find_click_origin_left_rotate_90_hits_painted_aabb() {
    let mut el = Element::new().w(100.0).h(100.0).on_click(|| {});
    el.layout_bounds = [400.0, 200.0, 100.0, 100.0];
    el.style.rotate_deg = 90.0;
    el.style.transform_origin = crate::style::parse_transform_origin("left top");
    assert!(
        find_click(&el, 350.0, 250.0, [0.0, 0.0]).is_some(),
        "painted ink left of the layout box must click"
    );
    assert!(
        find_click(&el, 450.0, 250.0, [0.0, 0.0]).is_none(),
        "old layout center must miss after origin-left 90°"
    );
    assert!(
        element_self_pointer_hit(&el, 350.0, 250.0, [0.0, 0.0]),
        "self hit on painted ink (stamps identity, own rotate)"
    );
    assert!(!element_self_pointer_hit(&el, 450.0, 250.0, [0.0, 0.0]));
}

#[test]
fn find_click_offset_distance_hits_shifted_box() {
    let mut el = Element::new().w(40.0).h(20.0).on_click(|| {});
    el.layout_bounds = [100.0, 100.0, 40.0, 20.0];
    el.style.offset_path = crate::style::parse_offset_path(r#"path("M 0 0 L 40 0")"#).unwrap();
    el.style.offset_distance = 1.0;
    el.style.offset_rotate_deg = Some(0.0);
    assert!(
        find_click(&el, 160.0, 110.0, [0.0, 0.0]).is_some(),
        "click must follow offset-path translate, aabb should start at x=140"
    );
    assert!(
        find_click(&el, 110.0, 110.0, [0.0, 0.0]).is_none(),
        "old layout origin must miss after offset-distance 1"
    );
}

#[test]
fn pointer_to_layout_local_inverts_origin_left_rotate_90() {
    let mut el = Element::new();
    el.layout_bounds = [400.0, 200.0, 100.0, 100.0];
    el.style.rotate_deg = 90.0;
    el.style.transform_origin = crate::style::parse_transform_origin("left top");
    let local = crate::core::tree::pointer_to_layout_local(
        &el,
        [350.0, 250.0],
        [0.0, 0.0],
        crate::style::MAT2D_IDENTITY,
        [1.0, 1.0],
    );
    assert!(
        (local[0] - 50.0).abs() < 0.75 && (local[1] - 50.0).abs() < 0.75,
        "painted ink at (350,250) must map back to layout (50,50), got {local:?}"
    );
}

#[test]
fn find_click_parent_rotate_uses_live_ancestor_before_collect() {
    let mut child = Element::new().w(80.0).h(20.0).on_click(|| {});
    child.layout_bounds = [60.0, 90.0, 80.0, 20.0];
    child.paint_ancestor_linear = crate::style::MAT2D_IDENTITY;
    child.paint_ancestor_scale = [1.0, 1.0];
    let mut parent = Element::new().w(200.0).h(200.0).child(child);
    parent.layout_bounds = [0.0, 0.0, 200.0, 200.0];
    parent.style.rotate_deg = 90.0;
    parent.style.transform_origin = crate::style::parse_transform_origin("center");
    assert!(
        find_click(&parent, 100.0, 130.0, [0.0, 0.0]).is_some(),
        "live parent rotate must hit the child paint AABB before collect stamps"
    );
    assert!(
        find_click(&parent, 40.0, 100.0, [0.0, 0.0]).is_none(),
        "old child layout overhang must miss after parent 90°"
    );
}

#[test]
fn layout_local_roundtrips_origin_left_rotate_90() {
    let mut el = Element::new();
    el.layout_bounds = [400.0, 200.0, 100.0, 100.0];
    el.style.rotate_deg = 90.0;
    el.style.transform_origin = crate::style::parse_transform_origin("left top");
    let world = crate::core::tree::layout_local_to_paint_world(
        &el,
        [50.0, 50.0],
        [0.0, 0.0],
        crate::style::MAT2D_IDENTITY,
        [1.0, 1.0],
    );
    assert!(
        (world[0] - 350.0).abs() < 0.75 && (world[1] - 250.0).abs() < 0.75,
        "layout (50,50) must paint at (350,250) after origin-left 90°, got {world:?}"
    );
    let back = crate::core::tree::pointer_to_layout_local(
        &el,
        world,
        [0.0, 0.0],
        crate::style::MAT2D_IDENTITY,
        [1.0, 1.0],
    );
    assert!(
        (back[0] - 50.0).abs() < 0.75 && (back[1] - 50.0).abs() < 0.75,
        "paint world must invert back to layout (50,50), got {back:?}"
    );
}

#[test]
fn clip_path_circle_rotate_90_hits_painted_not_layout() {
    let mut el = Element::new()
        .w(100.0)
        .h(100.0)
        .clip_path("circle(25% at 75% 50%)")
        .on_click(|| {});
    el.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    el.style.rotate_deg = 90.0;
    assert!(
        find_click(&el, 50.0, 75.0, [0.0, 0.0]).is_some(),
        "clip circle on the right must rotate to the bottom and still click"
    );
    assert!(
        find_click(&el, 75.0, 50.0, [0.0, 0.0]).is_none(),
        "old layout-space clip circle must miss after 90°"
    );
}

#[test]
fn clip_path_circle_rotate_90_gpu_poly_follows_vs() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut tile: Element = r#box()
                .w(100.0)
                .h(100.0)
                .bg([1.0, 0.0, 0.0, 1.0])
                .clip_path("circle(25% at 75% 50%)")
                .absolute(true)
                .left(0.0)
                .top(0.0)
                .into();
            tile.style.rotate_deg = 90.0;
            let mut root = Element::new().w(200.0).h(200.0).child(tile);
            let mut layout = LayoutTree::new();
            let result = layout.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
            let idx = result
                .instances
                .iter()
                .position(|i| i.color[0] > 0.5 && i.rect_size[0] > 80.0)
                .expect("rotated clipped fill");
            let poly = result
                .clip_polys
                .get(idx)
                .copied()
                .unwrap_or(crate::style::GpuClipPoly::NONE);
            assert!(
                poly.is_active(),
                "rotate must emit a tessellated clip-path poly, not layout AABB only"
            );
            assert!(
                poly.contains_point([50.0, 75.0]),
                "GPU clip must contain the painted circle, poly={poly:?}"
            );
            assert!(
                !poly.contains_point([75.0, 50.0]),
                "GPU clip must not keep the unrotated layout circle"
            );
        });
    });
}

#[test]
fn list_translate_moves_fill_and_hit_aabb() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut tile: Element = r#box()
                .w(100.0)
                .h(40.0)
                .bg([1.0, 0.0, 0.0, 1.0])
                .absolute(true)
                .left(0.0)
                .top(0.0)
                .into();
            tile.style.transform = crate::style::parse_transform("translate(40px, 0px)");
            let mut root = Element::new().w(200.0).h(80.0).child(tile);
            let mut layout = LayoutTree::new();
            let result = layout.build_and_layout(&mut root, 200.0, 80.0, [-1.0, -1.0]);
            let el = &root.children[0];
            let paint = visual_paint_aabb(el, [0.0, 0.0]);
            let fill = result
                .instances
                .iter()
                .find(|i| i.color[0] > 0.5 && (i.rect_size[0] - 100.0).abs() < 1.0)
                .expect("translated fill");
            let inst_aabb = crate::style::aabb_of_centered_transform(
                [
                    fill.rect_pos[0],
                    fill.rect_pos[1],
                    fill.rect_size[0],
                    fill.rect_size[1],
                ],
                &fill.transform,
            );
            assert!(
                (inst_aabb[0] - 40.0).abs() < 1.5,
                "list translate(40px) must shift the VS instance, aabb={inst_aabb:?}"
            );
            assert!(
                (paint[0] - inst_aabb[0]).abs() < 1.5 && (paint[1] - inst_aabb[1]).abs() < 1.5,
                "hit AABB must match VS under list translate: instance {inst_aabb:?} vs paint {paint:?}"
            );
            assert!(
                aabb_contains(paint, [60.0, 20.0]),
                "click must follow list translate, paint={paint:?}"
            );
            assert!(
                !aabb_contains(paint, [10.0, 20.0]),
                "old layout origin must miss after list translate(40px), paint={paint:?}"
            );
        });
    });
}

#[test]
fn parent_list_translate_shifts_child_fill() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let child: Element = r#box()
                .w(40.0)
                .h(20.0)
                .bg([1.0, 0.0, 0.0, 1.0])
                .absolute(true)
                .left(0.0)
                .top(0.0)
                .into();
            let mut parent = Element::new().w(80.0).h(40.0).child(child);
            parent.style.transform = crate::style::parse_transform("translate(30px, 0px)");
            parent = parent.absolute().left(0.0).top(0.0);
            let mut root = Element::new().w(200.0).h(80.0).child(parent);
            let mut layout = LayoutTree::new();
            let result = layout.build_and_layout(&mut root, 200.0, 80.0, [-1.0, -1.0]);
            let fill = result
                .instances
                .iter()
                .find(|i| i.color[0] > 0.5 && (i.rect_size[0] - 40.0).abs() < 1.0)
                .expect("child fill");
            let inst_aabb = crate::style::aabb_of_centered_transform(
                [
                    fill.rect_pos[0],
                    fill.rect_pos[1],
                    fill.rect_size[0],
                    fill.rect_size[1],
                ],
                &fill.transform,
            );
            assert!(
                inst_aabb[0] > 20.0,
                "parent list translate must shift the child instance, aabb={inst_aabb:?}"
            );
        });
    });
}

/// CSS overflow clips the local border box; the clip is then transformed.
/// Axis-aligned layout AABB would miss the diamond tip and accept the old corner.
#[test]
fn overflow_hidden_rotate_45_clips_in_local_border_box() {
    let mut el = Element::new();
    el.layout_bounds = [0.0, 0.0, 80.0, 80.0];
    el.style.overflow.x = taffy::style::Overflow::Hidden;
    el.style.overflow.y = taffy::style::Overflow::Hidden;
    el.style.rotate_deg = 45.0;
    let id = crate::style::MAT2D_IDENTITY;
    let sc = [1.0, 1.0];
    let acc = [0.0, 0.0];
    // Inset from the local top edge so half-open clip tests still contain the point.
    let tip = crate::core::tree::layout_local_to_paint_world(&el, [40.0, 4.0], acc, id, sc);
    let back = crate::core::tree::pointer_to_layout_local(&el, tip, acc, id, sc);
    assert!(
        (back[0] - 40.0).abs() < 0.5 && (back[1] - 4.0).abs() < 0.5,
        "round-trip local↔world; local={back:?} world={tip:?}"
    );
    assert!(
        crate::core::tree::overflow_clip_contains_pointer(&el, tip, acc, id, sc),
        "point on the rotated local top edge must be inside overflow clip, tip={tip:?}"
    );
    let aabb_corner = [80.0, 0.0];
    assert!(
        crate::core::tree::overflow_clip_contains_point(&el, [0.0, 0.0, 80.0, 80.0], aabb_corner),
        "unrotated AABB still contains the layout corner"
    );
    assert!(
        !crate::core::tree::overflow_clip_contains_pointer(&el, aabb_corner, acc, id, sc),
        "layout AABB corner is outside the rotated local box"
    );
}

#[test]
fn overflow_hidden_rotate_45_gpu_clip_matches_local_hit() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let child: Element = r#box().w(80.0).h(80.0).bg([0.0, 1.0, 0.0, 1.0]).into();
            let mut parent: Element = r#box()
                .w(80.0)
                .h(80.0)
                .overflow("hidden")
                .child(child)
                .into();
            parent.style.rotate_deg = 45.0;
            parent = parent.absolute().left(0.0).top(0.0);
            let mut root = Element::new().w(200.0).h(200.0).child(parent);
            let mut layout = LayoutTree::new();
            let result = layout.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
            let fill = result
                .instances
                .iter()
                .find(|i| i.color[1] > 0.5 && i.rect_size[0] > 70.0)
                .expect("child fill");
            assert!(
                fill.clip_size[0] >= 0.0 && fill.clip_size[1] >= 0.0,
                "descendant must carry overflow clip, clip_size={:?}",
                fill.clip_size
            );
            let el = &root.children[0];
            let id = crate::style::MAT2D_IDENTITY;
            let sc = [1.0, 1.0];
            let acc = [0.0, 0.0];
            let tip = crate::core::tree::layout_local_to_paint_world(el, [4.0, 4.0], acc, id, sc);
            assert!(
                crate::renderer::overflow_clip_contains_after_affine_inv(
                    fill.clip_pos,
                    fill.clip_size,
                    fill.rect_pos,
                    fill.rect_size,
                    &fill.transform,
                    tip,
                ),
                "GPU local clip must keep the diamond vertex, tip={tip:?} clip={:?} xform={:?}",
                (fill.clip_pos, fill.clip_size),
                fill.transform
            );
            let aabb_corner = [80.0, 0.0];
            assert!(
                !crate::renderer::overflow_clip_contains_after_affine_inv(
                    fill.clip_pos,
                    fill.clip_size,
                    fill.rect_pos,
                    fill.rect_size,
                    &fill.transform,
                    aabb_corner,
                ),
                "GPU local clip must drop the unrotated AABB corner"
            );
            assert!(
                tip[0] < fill.clip_pos[0]
                    || tip[0] >= fill.clip_pos[0] + fill.clip_size[0]
                    || tip[1] < fill.clip_pos[1]
                    || tip[1] >= fill.clip_pos[1] + fill.clip_size[1],
                "diamond vertex must sit outside the unrotated clip AABB, tip={tip:?} clip={:?}",
                (fill.clip_pos, fill.clip_size)
            );
        });
    });
}

#[test]
fn dest_reset_rotated_overflow_covers_diamond_tip() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let child: Element = r#box().w(80.0).h(80.0).bg([0.0, 1.0, 0.0, 1.0]).into();
            let mut parent: Element = r#box()
                .w(80.0)
                .h(80.0)
                .overflow("hidden")
                .child(child)
                .into();
            parent.style.rotate_deg = 45.0;
            parent = parent.absolute().left(0.0).top(0.0);
            let mut root = Element::new().w(200.0).h(200.0).child(parent);
            let mut layout = LayoutTree::new();
            let result = layout.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
            let fill_i = result
                .instances
                .iter()
                .position(|i| i.color[1] > 0.5 && i.rect_size[0] > 70.0)
                .expect("child fill");
            let fill = &result.instances[fill_i];
            let paint = result
                .node_paints
                .iter()
                .find(|p| p.instance.0 <= fill_i && fill_i < p.instance.1)
                .expect("NodePaint for child fill");
            let clip = paint.overflow_clip.expect("ancestor dest-reset clip");
            let el = &root.children[0];
            let id = crate::style::MAT2D_IDENTITY;
            let sc = [1.0, 1.0];
            let acc = [0.0, 0.0];
            let tip = crate::core::tree::layout_local_to_paint_world(el, [4.0, 4.0], acc, id, sc);
            assert!(
                aabb_contains(clip, tip),
                "paint overflow AABB must cover the diamond vertex, clip={clip:?} tip={tip:?}"
            );
            assert!(
                clip[2] > 90.0 && clip[3] > 90.0,
                "rotated dest-reset clip must be the paint AABB, not layout 80×80, clip={clip:?}"
            );
            let inst_b = crate::app::draw_cull::instance_paint_bounds(fill);
            assert!(
                aabb_contains(inst_b, tip),
                "instance dest-ink AABB must keep the diamond vertex, bounds={inst_b:?} tip={tip:?}"
            );
            let wipe = crate::app::draw_cull::node_present_reset_rect(
                paint.key,
                paint.bounds,
                &result.node_paints,
                &result.instances,
                &[],
                crate::app::draw_cull::NODE_PRESENT_RESET_PAD_PHYSICAL,
                1.0,
                None,
            )
            .expect("dest-reset wipe of rotated overflow child");
            assert!(
                aabb_contains(wipe, tip),
                "dest-reset wipe must cover the diamond vertex, wipe={wipe:?} tip={tip:?}"
            );
            let scissor = crate::app::draw_cull::node_present_scissor_logical(
                crate::app::damage::PaintDirtyKind::Node,
                paint.key,
                paint.bounds,
                &[],
                &result.node_paints,
                8.0,
            );
            assert!(
                aabb_contains(scissor, tip),
                "node-present scissor must cover the diamond vertex, scissor={scissor:?} tip={tip:?}"
            );
            let layout_clip = [
                fill.clip_pos[0],
                fill.clip_pos[1],
                fill.clip_size[0],
                fill.clip_size[1],
            ];
            assert!(
                !aabb_contains(layout_clip, tip),
                "GPU layout clip AABB still excludes the diamond tip (Wave 44), clip={layout_clip:?} tip={tip:?}"
            );
        });
    });
}

#[test]
fn overflow_clip_bounds_for_rotated_parent_covers_diamond() {
    let mut parent = Element::new();
    parent.layout_bounds = [0.0, 0.0, 80.0, 80.0];
    parent.style.overflow.x = taffy::style::Overflow::Hidden;
    parent.style.overflow.y = taffy::style::Overflow::Hidden;
    parent.style.rotate_deg = 45.0;
    let mut child = Element::new();
    child.layout_bounds = [0.0, 0.0, 80.0, 80.0];
    parent.children.push(child);
    let target = &parent.children[0] as *const Element;
    let clip = crate::core::tree::overflow_clip_bounds_for(&parent, target)
        .expect("nearest overflow paint AABB");
    let id = crate::style::MAT2D_IDENTITY;
    let sc = [1.0, 1.0];
    let acc = [0.0, 0.0];
    let tip = crate::core::tree::layout_local_to_paint_world(&parent, [4.0, 4.0], acc, id, sc);
    assert!(
        aabb_contains(clip, tip),
        "island overflow clip must cover the diamond vertex, clip={clip:?} tip={tip:?}"
    );
    assert!(
        clip[2] > 90.0 && clip[3] > 90.0,
        "must be paint AABB not layout 80×80, clip={clip:?}"
    );
    parent.style.rotate_deg = 0.0;
    let axis =
        crate::core::tree::overflow_clip_bounds_for(&parent, target).expect("identity overflow");
    assert!(
        (axis[0] - 0.0).abs() < 0.5
            && (axis[1] - 0.0).abs() < 0.5
            && (axis[2] - 80.0).abs() < 0.5
            && (axis[3] - 80.0).abs() < 0.5,
        "identity overflow clip stays the layout border, clip={axis:?}"
    );
}
