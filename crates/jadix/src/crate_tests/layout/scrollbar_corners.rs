#![cfg(feature = "components")]
//! Dual-axis scrollbar corners: unified L-join (horizontal yields to vertical).
use crate::components::scroll_area::scroll_area;
use crate::components::scrollbar::{
    SCROLLBAR_DEFAULT_CROSS_PX, SCROLLBAR_TRACK_FOCUS_PREFIX,
};
use crate::core::reactivity::{create_effect, create_root};
use crate::core::scrollbar::horizontal_lane_side_inset;
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
            // Auto build starts at edge pad; with Y fitting, lane stays released.
            // Collect must paint that box — not a viewport-width stretch past it.
            {
                let h = find_track_mut(&mut root, "x").expect("x track");
                let edge = crate::layout::lpa_length_px(h.style.inset.right).unwrap_or(0.0);
                let next = horizontal_lane_side_inset(edge, SCROLLBAR_DEFAULT_CROSS_PX, false);
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
            let _ = layout.build_and_layout_fast(&mut root, 200.0, 120.0, [-1.0, -1.0], true, true);
            let h = find_track(&root, "x").expect("x track").layout_bounds;
            let v = find_track(&root, "y").expect("y track").layout_bounds;
            assert_l_join(h, v, "overlay");
        });
    });
}

#[test]
fn scroll_viewport_relayout_does_not_measure_clean_text() {
    fn find_mut<'a>(el: &'a mut Element, id: &str) -> &'a mut Element {
        crate::core::tree::find_by_template_id_mut(el, id).expect("fixture node")
    }
    let (owner, ()) = create_root(|| {
        let content = Element::new()
            .flex_col()
            .flex_shrink(0.0)
            .template_id("basis-content")
            .children((0..100).map(|_| {
                Element::new().flex_shrink(0.0).min_h(20.0).child(
                    Element::new()
                        .w("100%")
                        .min_h(16.0)
                        .text("fixed content")
                        .text_size(14.0),
                )
            }))
            .child(
                Element::new()
                    .absolute()
                    .w(20.0)
                    .h(20.0)
                    .left(0.0)
                    .top(0.0)
                    .template_id("basis-marker"),
            );
        let area: Element = scroll_area("basis-area")
            .w(400.0)
            .h(260.0)
            .scrollbar_inset("outside")
            .child(content)
            .into();
        let mut root = Element::new()
            .flex_col()
            .child(Element::new().h(20.0).flex_shrink(0.0))
            .child(area)
            .child(
                Element::new()
                    .h(20.0)
                    .flex_shrink(0.0)
                    .template_id("basis-footer"),
            );
        let mut layout = LayoutTree::new();
        layout.build_and_layout(&mut root, 400.0, 300.0, [-1.0; 2]);
        let footer = find_mut(&mut root, "basis-footer").layout_bounds;
        let content = find_mut(&mut root, "basis-content").layout_bounds;
        let marker = find_mut(&mut root, "basis-marker");
        let before_x = marker.layout_bounds[0];
        marker.style.inset.left = taffy::style::LengthPercentageAuto::length(40.0);
        layout.mark_element_layout_dirty(marker);
        layout.build_and_layout_fast_ex(&mut root, 400.0, 300.0, [-1.0; 2], false, false, None);
        assert_eq!(find_mut(&mut root, "basis-footer").layout_bounds, footer);
        assert_eq!(find_mut(&mut root, "basis-content").layout_bounds, content);
        assert!(
            (find_mut(&mut root, "basis-marker").layout_bounds[0] - before_x - 40.0).abs() < 0.01
        );
        let calls = layout.text_measure_calls.get();
        eprintln!("scroll-basis/relayout: text_calls={calls}");
        assert_eq!(
            calls, 0,
            "an absolute marker move must not remeasure 100 clean text leaves"
        );
        let marker = find_mut(&mut root, "basis-marker");
        marker.style.inset.left = taffy::style::LengthPercentageAuto::length(80.0);
        let mut queued = std::collections::HashSet::new();
        crate::core::tree::walk_elements(&root, |el| {
            if let Some(id) = el.layout_node {
                queued.insert(id);
            }
        });
        let clean = find_mut(&mut root, "basis-content").children[0].children[0]
            .layout_node
            .unwrap();
        layout.mark_dirty_nodes_in_tree(&root, &queued);
        assert!(
            !layout.is_node_dirty(clean),
            "a broadly queued subtree must preserve unchanged leaves"
        );
        layout.build_and_layout_fast_ex(&mut root, 400.0, 300.0, [-1.0; 2], false, false, None);
        assert_eq!(layout.text_measure_calls.get(), 0);
        assert!(
            (find_mut(&mut root, "basis-marker").layout_bounds[0] - before_x - 80.0).abs() < 0.01
        );
        use crate::core::reactivity::use_signal;
        use crate::core::scroll_viewport::*;
        for (key, value) in [
            (scroll_viewport_w_signal_key("basis-area"), 390.0),
            (scroll_viewport_h_signal_key("basis-area"), 260.0),
            (scroll_content_w_signal_key("basis-area"), 390.0),
            (scroll_content_h_signal_key("basis-area"), 2000.0),
        ] {
            use_signal(&key, || 0.0_f32).set_silent(value);
        }
        // Settle initial track sizing, then move the real scrollbar using the
        // same Full prepass that runs when a chart changes during scrolling.
        crate::app::recompute_scroll_viewport_flags(&mut root);
        crate::app::update_scrollbar_elements_for_test(
            &mut root,
            &mut layout,
            crate::app::ScrollbarUpdateMode::Full,
        );
        layout.build_and_layout_fast_ex(&mut root, 400.0, 300.0, [-1.0; 2], false, false, None);
        let footer = find_mut(&mut root, "basis-footer").layout_bounds;
        let before = find_track(&root, "y").unwrap().children[0].layout_bounds;
        use_signal(&scroll_offset_y_signal_key("basis-area"), || 0.0_f32).set_silent(600.0);
        crate::app::update_scrollbar_elements_for_test(
            &mut root,
            &mut layout,
            crate::app::ScrollbarUpdateMode::Full,
        );
        layout.build_and_layout_fast_ex(&mut root, 400.0, 300.0, [-1.0; 2], false, false, None);
        let after = find_track(&root, "y").unwrap().children[0].layout_bounds;
        assert!(after[1] > before[1], "the real thumb must move");
        assert_eq!(&after[2..], &before[2..]);
        assert_eq!(find_mut(&mut root, "basis-footer").layout_bounds, footer);
        assert_eq!(
            layout.text_measure_calls.get(),
            0,
            "moving an outside scrollbar must not remeasure clean content"
        );
    });
    owner.dispose();
}
