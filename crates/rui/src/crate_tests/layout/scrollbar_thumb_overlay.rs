//! Regression: overlay-collect thumb origin must match Full Taffy, centered in the track.
//!
//! Track chrome is a 1px layout border and no padding. A previous overlay formula used
//! border+padding (2px) and sat the thumb ~1px down-right of the track.

use crate::components::scroll_area::scroll_area;
use crate::components::scrollbar::{SCROLLBAR_THUMB_FOCUS_PREFIX, SCROLLBAR_TRACK_FOCUS_PREFIX};
use crate::core::reactivity::{create_effect, create_root, use_signal};
use crate::core::scroll_viewport::{
    scroll_content_h_signal_key, scroll_content_w_signal_key, scroll_offset_y_signal_key,
    scroll_viewport_h_signal_key, scroll_viewport_w_signal_key,
};
use crate::core::tree::{Element, LayoutTree};
use crate::layout::layout_tree::RenderResult;

fn find_focus_bounds(el: &Element, prefix: &str, axis_suffix: &str) -> Option<[f32; 4]> {
    if el
        .focus_id
        .as_deref()
        .is_some_and(|f| f.starts_with(prefix) && f.ends_with(axis_suffix))
    {
        return Some(el.layout_bounds);
    }
    el.children
        .iter()
        .find_map(|c| find_focus_bounds(c, prefix, axis_suffix))
}

fn thumb_paint_y(result: &RenderResult, thumb: [f32; 4]) -> Option<f32> {
    result
        .instances
        .iter()
        .filter(|i| {
            (i.rect_pos[0] - thumb[0]).abs() < 2.5
                && (i.rect_size[0] - thumb[2]).abs() < 2.5
                && (i.rect_size[1] - thumb[3]).abs() < 2.5
                && i.rect_size[0] > 1.0
                && i.color[3] > 0.5
        })
        .map(|i| i.rect_pos[1])
        .min_by(|a, b| a.total_cmp(b))
}

#[test]
fn overlay_collect_y_thumb_matches_full_taffy_origin() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let content = Element::new()
                .flex_col()
                .w("100%")
                .gap(4.0)
                .children((0..16).map(|i| {
                    Element::new()
                        .w("100%")
                        .h(24.0)
                        .bg([0.9, 0.9, 0.9, 1.0])
                        .template_id(format!("row-{i}"))
                }));

            let mut root = Element::from(
                scroll_area("diag-thumb-overlay")
                    .w(220.0)
                    .h(120.0)
                    .overflow_y("auto")
                    .overflow_x("hidden")
                    .scrollbar_visibility("always")
                    .child(content),
            );

            let mut layout = LayoutTree::new();
            let full = layout.build_and_layout(&mut root, 220.0, 120.0, [-1.0, -1.0]);
            let (_id, bounds, _max, content_size) = full
                .scroll_metrics
                .iter()
                .find(|(id, ..)| id == "diag-thumb-overlay")
                .expect("scroll metrics");

            use_signal(&scroll_viewport_w_signal_key("diag-thumb-overlay"), || {
                0.0_f32
            })
            .set(bounds[2]);
            use_signal(&scroll_viewport_h_signal_key("diag-thumb-overlay"), || {
                0.0_f32
            })
            .set(bounds[3]);
            use_signal(&scroll_content_w_signal_key("diag-thumb-overlay"), || {
                0.0_f32
            })
            .set(content_size[0]);
            use_signal(&scroll_content_h_signal_key("diag-thumb-overlay"), || {
                0.0_f32
            })
            .set(content_size[1]);

            let taffy_thumb = find_focus_bounds(&root, SCROLLBAR_THUMB_FOCUS_PREFIX, ":y")
                .expect("Y thumb after Full Taffy");
            let taffy_track = find_focus_bounds(&root, SCROLLBAR_TRACK_FOCUS_PREFIX, ":y")
                .expect("Y track after Full Taffy");
            let taffy_left = taffy_thumb[0] - taffy_track[0];
            let taffy_right = (taffy_track[0] + taffy_track[2]) - (taffy_thumb[0] + taffy_thumb[2]);
            assert!(
                (taffy_left - taffy_right).abs() < 0.51,
                "Full Taffy Y thumb must be cross-centered in the track (left gap {taffy_left}, right gap {taffy_right}); \
                 padding+border used to shift it ~1px right"
            );

            let _overlay =
                layout.build_and_layout_fast(&mut root, 220.0, 120.0, [-1.0, -1.0], true, true);
            let overlay_thumb = find_focus_bounds(&root, SCROLLBAR_THUMB_FOCUS_PREFIX, ":y")
                .expect("Y thumb after overlay collect");
            let overlay_track = find_focus_bounds(&root, SCROLLBAR_TRACK_FOCUS_PREFIX, ":y")
                .expect("Y track after overlay collect");
            let overlay_left = overlay_thumb[0] - overlay_track[0];
            let overlay_right =
                (overlay_track[0] + overlay_track[2]) - (overlay_thumb[0] + overlay_thumb[2]);

            assert!(
                (taffy_thumb[0] - overlay_thumb[0]).abs() < 0.51
                    && (taffy_thumb[1] - overlay_thumb[1]).abs() < 0.51,
                "overlay thumb origin {:?} must match Full Taffy {:?}",
                overlay_thumb,
                taffy_thumb
            );
            assert!(
                (overlay_left - overlay_right).abs() < 0.51,
                "overlay Y thumb must be cross-centered in the track (left gap {overlay_left}, right gap {overlay_right})"
            );
        });
    });
}

/// Overlay thumbs are signal-driven. Fingerprint skip of the relative thumb leaf
/// used to splice last-frame ops (Taffy leftover at the unscrolled top) while
/// `layout_bounds` could still look live — frozen rail while the list scrolls.
#[test]
fn overlay_collect_skip_must_move_painted_thumb_with_scroll() {
    const REST: &str = "diag-thumb-skip-scroll";
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let content = Element::new()
                .flex_col()
                .w("100%")
                .gap(4.0)
                .children((0..16).map(|i| {
                    Element::new()
                        .w("100%")
                        .h(24.0)
                        .bg([0.9, 0.9, 0.9, 1.0])
                        .template_id(format!("row-{i}"))
                }));

            let mut root = Element::from(
                scroll_area(REST)
                    .w(220.0)
                    .h(120.0)
                    .overflow_y("auto")
                    .overflow_x("hidden")
                    .scrollbar_visibility("always")
                    .child(content),
            );

            let mut layout = LayoutTree::new();
            let full = layout.build_and_layout(&mut root, 220.0, 120.0, [-1.0, -1.0]);
            let (_id, bounds, _max, content_size) = full
                .scroll_metrics
                .iter()
                .find(|(id, ..)| id == REST)
                .expect("scroll metrics");

            use_signal(&scroll_viewport_w_signal_key(REST), || 0.0_f32).set(bounds[2]);
            use_signal(&scroll_viewport_h_signal_key(REST), || 0.0_f32).set(bounds[3]);
            use_signal(&scroll_content_w_signal_key(REST), || 0.0_f32).set(content_size[0]);
            use_signal(&scroll_content_h_signal_key(REST), || 0.0_f32).set(content_size[1]);
            use_signal(&scroll_offset_y_signal_key(REST), || 0.0_f32).set(0.0);

            layout.set_collect_skip_allowed(true);
            let rest =
                layout.build_and_layout_fast(&mut root, 220.0, 120.0, [-1.0, -1.0], true, true);
            let thumb0 = find_focus_bounds(&root, SCROLLBAR_THUMB_FOCUS_PREFIX, ":y")
                .expect("Y thumb at rest");
            let y0 = thumb_paint_y(&rest, thumb0).expect("opaque thumb instance at rest");

            use_signal(&scroll_offset_y_signal_key(REST), || 0.0_f32).set(80.0);
            let scrolled =
                layout.build_and_layout_fast(&mut root, 220.0, 120.0, [-1.0, -1.0], true, true);
            let thumb1 = find_focus_bounds(&root, SCROLLBAR_THUMB_FOCUS_PREFIX, ":y")
                .expect("Y thumb after scroll");
            let y1 = thumb_paint_y(&scrolled, thumb1)
                .expect("opaque thumb instance must be painted at the live overlay origin");

            assert!(
                thumb1[1] > thumb0[1] + 8.0,
                "overlay hit box must follow scroll, rest={thumb0:?} scrolled={thumb1:?}"
            );
            assert!(
                y1 > y0 + 8.0,
                "painted thumb must move with the offset, not replay the rest-pose DL (rest_y={y0} scrolled_y={y1})"
            );
        });
    });
}
