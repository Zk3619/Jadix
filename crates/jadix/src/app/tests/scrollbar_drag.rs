use super::*;

#[test]
fn scaled_scrollbar_drag_preserves_the_grab_point_on_both_axes() {
    crate::core::reactivity::create_root(|| {
        for zoom in [0.35, 0.57, 1.0, 1.5] {
            for axis in [ScrollAxis::X, ScrollAxis::Y] {
                let index = if axis == ScrollAxis::X { 0 } else { 1 };
                let name = if index == 0 { "x" } else { "y" };
                let track_id = format!("scrollbar-track:scaled-drag:{name}");
                let thumb_id = format!("scrollbar-thumb:scaled-drag:{name}");
                let thumb = Element::new()
                    .focus_id(&thumb_id)
                    .absolute()
                    .left(if index == 0 { 80.0 } else { 0.0 })
                    .top(if index == 1 { 80.0 } else { 0.0 })
                    .w(if index == 0 { 40.0 } else { 10.0 })
                    .h(if index == 1 { 40.0 } else { 10.0 });
                let track = Element::new()
                    .focus_id(&track_id)
                    .relative()
                    .w(if index == 0 { 240.0 } else { 10.0 })
                    .h(if index == 1 { 240.0 } else { 10.0 })
                    .child(thumb);
                let mut root = Element::new()
                    .w(600.0)
                    .h(600.0)
                    .scale(1.25)
                    .transform_origin("left top")
                    .child(
                        Element::new()
                            .absolute()
                            .left(70.0)
                            .top(50.0)
                            .w(300.0)
                            .h(300.0)
                            .scale(zoom / 1.25)
                            .transform_origin("left top")
                            .child(track),
                    );
                let mut layout = crate::core::tree::LayoutTree::new();
                layout.build_and_layout(&mut root, 600.0, 600.0, [-1.0; 2]);
                let visual_track =
                    super::super::tree_hit::find_paint_aabb_by_focus_id(&root, &track_id, [0.0; 2])
                        .unwrap()
                        .0;
                let visual_thumb =
                    super::super::tree_hit::find_paint_aabb_by_focus_id(&root, &thumb_id, [0.0; 2])
                        .unwrap()
                        .0;
                let (track, thumb) = scrollbar_drag_bounds(&root, "scaled-drag", axis).unwrap();
                let pointer = visual_thumb[index] + visual_thumb[index + 2] * 0.37;
                let grab = (pointer - thumb[index]).clamp(0.0, thumb[index + 2]);
                let span = track[index + 2] - thumb[index + 2];
                for delta in [0.0, 7.0, 14.0, -7.0] {
                    let offset =
                        ((pointer + delta - track[index] - grab) / span).clamp(0.0, 1.0) * 1000.0;
                    let expected = 400.0 + delta / (200.0 * zoom) * 1000.0;
                    assert!(
                            (offset - expected).abs() < 0.01,
                            "zoom={zoom} axis={axis:?} delta={delta}: offset={offset}, expected={expected}"
                        );
                    let projected = visual_track[index]
                        + offset / 1000.0 * (visual_track[index + 2] - visual_thumb[index + 2])
                        + visual_thumb[index + 2] * 0.37;
                    assert!((projected - pointer - delta).abs() < 0.01);
                }
            }
        }
    });
}
