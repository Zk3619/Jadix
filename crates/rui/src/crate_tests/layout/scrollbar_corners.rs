//! Dual-axis scrollbar corners: unified L-join (horizontal yields to vertical).

use crate::components::scroll_area::scroll_area;
use crate::components::scrollbar::SCROLLBAR_TRACK_FOCUS_PREFIX;
use crate::core::reactivity::{create_effect, create_root};
use crate::core::tree::{Element, LayoutTree};

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
