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

/// Wheel frames promote scrollbar prepass to `Full` while `scroll_only` skips
/// `compute_layout`. Collect used to gate signal geometry on `overlay_collect` only,
/// so the rail painted Taffy leftover (min-cap at the unscrolled top) for one frame
/// — thumb jumps to track start while `sy` / page content stay mid-scroll. Drag kept
/// `overlay_collect` via `active_drag_rest`; wheel did not.
#[test]
fn overlay_thumb_stays_mid_when_full_scroll_only_skips_overlay_flag() {
    const REST: &str = "diag-thumb-full-scroll-only";
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let content = Element::new()
                .flex_col()
                .w("100%")
                .gap(4.0)
                .children((0..24).map(|i| {
                    Element::new()
                        .w("100%")
                        .h(28.0)
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
            assert!(
                content_size[1] > bounds[3] + 40.0,
                "precondition: overflowed content"
            );

            use_signal(&scroll_viewport_w_signal_key(REST), || 0.0_f32).set(bounds[2]);
            use_signal(&scroll_viewport_h_signal_key(REST), || 0.0_f32).set(bounds[3]);
            use_signal(&scroll_content_w_signal_key(REST), || 0.0_f32).set(content_size[0]);
            use_signal(&scroll_content_h_signal_key(REST), || 0.0_f32).set(content_size[1]);
            use_signal(&scroll_offset_y_signal_key(REST), || 0.0_f32).set(120.0);

            // ViewportsOnly + overlay: live signal geometry (healthy reference).
            let overlay =
                layout.build_and_layout_fast(&mut root, 220.0, 120.0, [-1.0, -1.0], true, true);
            let track = find_focus_bounds(&root, SCROLLBAR_TRACK_FOCUS_PREFIX, ":y")
                .expect("Y track");
            let thumb_ref = find_focus_bounds(&root, SCROLLBAR_THUMB_FOCUS_PREFIX, ":y")
                .expect("Y thumb overlay");
            let y_ref = thumb_paint_y(&overlay, thumb_ref).expect("painted overlay thumb");
            assert!(
                y_ref > track[1] + 12.0,
                "precondition: overlay mid-scroll thumb; y={y_ref} track={track:?}"
            );

            // Wheel-shaped frame: scroll_only (no compute_layout) + Full collect without
            // overlay_collect. Taffy still holds the sy=0 leftover from the first Full.
            let wheeled =
                layout.build_and_layout_fast(&mut root, 220.0, 120.0, [-1.0, -1.0], true, false);
            let thumb = find_focus_bounds(&root, SCROLLBAR_THUMB_FOCUS_PREFIX, ":y")
                .expect("Y thumb after Full scroll_only");
            let y = thumb_paint_y(&wheeled, thumb).expect("painted thumb");
            assert!(
                (y - y_ref).abs() < 2.0,
                "wheel Full+scroll_only must keep signal thumb mid-track, not Taffy leftover at start; \
                 y={y} y_ref={y_ref} track={track:?} thumb={thumb:?}"
            );
            assert!(
                y > track[1] + 12.0,
                "thumb must not sit at track start while sy=120; y={y} track={track:?} thumb={thumb:?}"
            );
        });
    });
}

/// Mid-scroll overlay seed used to overwrite content_h with a cull `ch≈vh` sample
/// in the same collect walk (post-render stale-zero is too late). Thumb t→0 for one
/// frame, then recovers when the next seed/signal is healthy.
#[test]
fn overlay_thumb_stays_mid_track_when_content_signal_stays_overflowed() {
    const REST: &str = "diag-thumb-seed-cull";
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let content = Element::new()
                .flex_col()
                .w("100%")
                .gap(4.0)
                .children((0..24).map(|i| {
                    Element::new()
                        .w("100%")
                        .h(28.0)
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
            assert!(
                content_size[1] > bounds[3] + 40.0,
                "precondition: overflowed content"
            );

            use_signal(&scroll_viewport_w_signal_key(REST), || 0.0_f32).set(bounds[2]);
            use_signal(&scroll_viewport_h_signal_key(REST), || 0.0_f32).set(bounds[3]);
            use_signal(&scroll_content_w_signal_key(REST), || 0.0_f32).set(content_size[0]);
            use_signal(&scroll_content_h_signal_key(REST), || 0.0_f32).set(content_size[1]);
            use_signal(&scroll_offset_y_signal_key(REST), || 0.0_f32).set(120.0);

            let scrolled =
                layout.build_and_layout_fast(&mut root, 220.0, 120.0, [-1.0, -1.0], true, true);
            let track = find_focus_bounds(&root, SCROLLBAR_TRACK_FOCUS_PREFIX, ":y")
                .expect("Y track");
            let thumb = find_focus_bounds(&root, SCROLLBAR_THUMB_FOCUS_PREFIX, ":y")
                .expect("Y thumb mid-scroll");
            let y = thumb_paint_y(&scrolled, thumb).expect("painted thumb");
            let track_top = track[1];
            let track_bottom = track[1] + track[3];
            assert!(
                y > track_top + 12.0,
                "thumb must not sit at track start while sy=120; y={y} track={track:?} thumb={thumb:?}"
            );
            assert!(
                y + thumb[3] < track_bottom - 4.0,
                "thumb must remain on-track; y={y} thumb_h={} track={track:?}",
                thumb[3]
            );
        });
    });
}
