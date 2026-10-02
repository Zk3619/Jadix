//! Dual-axis scrollbar corners: unified L-join (horizontal yields to vertical).

use crate::components::scroll_area::scroll_area;
use crate::components::scrollbar::{
    horizontal_lane_side_inset, SCROLLBAR_DEFAULT_CROSS_PX, SCROLLBAR_TRACK_FOCUS_PREFIX,
};
use crate::core::reactivity::{create_effect, create_root};
use crate::core::tree::{Element, LayoutTree};

fn find_track_mut<'a>(el: &'a mut Element, axis: &str) -> Option<&'a mut Element> {
    let suffix = format!(":{axis}");
    if el
        .focus_id
        .as_deref()
        .is_some_and(|f| f.starts_with(SCROLLBAR_TRACK_FOCUS_PREFIX) && f.ends_with(&suffix))
    {
        return Some(el);
    }
    el.children.iter_mut().find_map(|c| find_track_mut(c, axis))
}

fn find_track<'a>(el: &'a Element, axis: &str) -> Option<&'a Element> {
    let suffix = format!(":{axis}");
    if el
        .focus_id
        .as_deref()
        .is_some_and(|f| f.starts_with(SCROLLBAR_TRACK_FOCUS_PREFIX) && f.ends_with(&suffix))
    {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_track(c, axis))
}

fn overflowing_content() -> Element {
    Element::new()
        .flex_col()
        .min_w(280.0)
        .gap(4.0)
        .children((0..12).map(|i| {
            Element::new()
                .w("100%")
                .h(20.0)
                .bg([0.9, 0.9, 0.9, 1.0])
                .template_id(format!("row-{i}"))
        }))
}

fn layout_area(id: &str, inset: &str) -> (Element, [f32; 4], [f32; 4]) {
    let mut root = Element::from(
        scroll_area(id)
            .w(200.0)
            .h(120.0)
            .overflow_x("auto")
            .overflow_y("auto")
            .scrollbar_visibility("always")
            .scrollbar_inset(inset)
            .child(overflowing_content()),
    );
    let mut layout = LayoutTree::new();
    let _ = layout.build_and_layout(&mut root, 200.0, 120.0, [-1.0, -1.0]);
    let x = find_track(&root, "x").expect("x track").layout_bounds;
    let y = find_track(&root, "y").expect("y track").layout_bounds;
    (root, x, y)
}

fn rects_overlap(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[0] + b[2] && a[0] + a[2] > b[0] && a[1] < b[1] + b[3] && a[1] + a[3] > b[1]
}

fn assert_l_join(h: [f32; 4], v: [f32; 4], label: &str) {
    let h_right = h[0] + h[2];
    let v_left = v[0];
    let v_bottom = v[1] + v[3];
    let h_bottom = h[1] + h[3];
    assert!(
        (h_right - v_left).abs() < 1.5,
        "{label}: H should meet V (L-join), not leave a gutter: h_right={h_right} v_left={v_left} h={h:?} v={v:?}"
    );
    assert!(
        (v_bottom - h_bottom).abs() < 1.5,
        "{label}: V should own the corner square: v_bottom={v_bottom} h_bottom={h_bottom} h={h:?} v={v:?}"
    );
    assert!(
        !rects_overlap(h, v),
        "{label}: tracks must not overlap: h={h:?} v={v:?}"
    );
}

#[test]
fn inside_overlay_uses_l_join_horizontal_yields_to_vertical() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let (_root, hx, vy) = layout_area("sa-corner-inside", "inside");
            assert_l_join(hx, vy, "inside");
        });
    });
}

#[test]
fn outside_lanes_meet_as_l_join_without_dead_square() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let (_root, hx, vy) = layout_area("sa-corner-outside", "outside");
            assert_l_join(hx, vy, "outside");
        });
    });
}

#[test]
fn horizontal_bar_spans_full_width_when_vertical_bar_is_hidden() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = Element::from(
                scroll_area("sa-h-only")
                    .w(240.0)
                    .h(80.0)
                    .overflow_x("auto")
                    .overflow_y("auto")
                    .scrollbar_visibility("auto")
                    .child(Element::new().w(480.0).h(24.0).bg([0.8, 0.8, 0.8, 1.0])),
            );
            // Prepass contract: drop only the lane-side inset, then Taffy places the bar.
            // Collect must paint that box — not a viewport-width stretch past it.
            {
                let h = find_track_mut(&mut root, "x").expect("x track");
                let authored = crate::layout::lpa_length_px(h.style.inset.right).unwrap_or(0.0);
                let next =
                    horizontal_lane_side_inset(authored, SCROLLBAR_DEFAULT_CROSS_PX, false);
                h.style.inset.right = taffy::style::LengthPercentageAuto::length(next);
            }
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 240.0, 80.0, [-1.0, -1.0]);
            let placed = find_track(&root, "x").expect("x track").layout_bounds;
            let _ = layout.build_and_layout_fast(&mut root, 240.0, 80.0, [-1.0, -1.0], true, true);
            let h = find_track(&root, "x").expect("x track").layout_bounds;
            assert!(
                (h[0] - placed[0]).abs() < 0.6 && (h[2] - placed[2]).abs() < 0.6,
                "overlay collect must not move the track off its layout box: placed={placed:?} painted={h:?}"
            );
            let right = h[0] + h[2];
            assert!(
                right > 240.0 - 6.0 && right <= 240.0 + 0.5,
                "horizontal track should stop at the edge pad, not a vertical lane: h={h:?} right={right}"
            );
            let v = find_track(&root, "y").expect("y track").layout_bounds;
            assert!(
                v[2] < 1.0 || v[3] < 1.0,
                "vertical track should be hidden when content fits: v={v:?}"
            );
        });
    });
}

#[test]
fn overlay_collect_keeps_l_join_when_both_bars_show() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = Element::from(
                scroll_area("sa-corner-overlay")
                    .w(200.0)
                    .h(120.0)
                    .overflow_x("auto")
                    .overflow_y("auto")
                    .scrollbar_visibility("always")
                    .child(overflowing_content()),
            );
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 200.0, 120.0, [-1.0, -1.0]);
            let _ =
                layout.build_and_layout_fast(&mut root, 200.0, 120.0, [-1.0, -1.0], true, true);
            let h = find_track(&root, "x").expect("x track").layout_bounds;
            let v = find_track(&root, "y").expect("y track").layout_bounds;
            assert_l_join(h, v, "overlay");
        });
    });
}
