//! Overlay `auto` fade is paint-only: live cubic × collect, hit slab unchanged.
//!
//! Full prepass used to bake cubic into `style.bg` and skip-flex Taffy, which crushed
//! the thumb and jumped the 8px hit slab. ViewportsOnly overlay collect must fade
//! alpha without rewriting `layout_bounds`.

use crate::app::damage::take_frame_paint_nodes;
use crate::app::pointer_events::{
    click_dispatch_path, click_gesture_path, hit_path, resolve_click_from_path,
};
use crate::components::button::button;
use crate::components::scroll_area::scroll_area;
use crate::components::scrollbar::{SCROLLBAR_THUMB_FOCUS_PREFIX, SCROLLBAR_TRACK_FOCUS_PREFIX};
use crate::core::reactivity::{create_effect, create_root, use_signal};
use crate::core::scroll_viewport::{
    scroll_content_h_signal_key, scroll_content_w_signal_key, scroll_offset_x_signal_key,
    scroll_offset_y_signal_key, scroll_viewport_h_signal_key, scroll_viewport_w_signal_key,
    SCROLL_VIEWPORT_FOCUS_PREFIX,
};
use crate::core::tree::{Element, LayoutTree};
use crate::layout::layout_tree::RenderResult;
use crate::style::{tick_animations, use_ui_transition_cubic};
use std::cell::Cell;
use std::rc::Rc;

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

fn thumb_style_alpha(el: &Element) -> Option<f32> {
    if el
        .focus_id
        .as_deref()
        .is_some_and(|f| f.starts_with(SCROLLBAR_THUMB_FOCUS_PREFIX) && f.ends_with(":y"))
    {
        return Some(el.style.bg_color[3]);
    }
    el.children.iter().find_map(thumb_style_alpha)
}

fn settle_cubic(rest: &str, axis: &str, target: f32) {
    let tw = use_ui_transition_cubic(&format!("scrollbar_spring_{rest}:{axis}"), 0.0);
    tw.set(target);
    tick_animations(0.08);
    tick_animations(0.08);
}

fn thumb_fill_alpha(result: &RenderResult, thumb: [f32; 4]) -> f32 {
    result
        .instances
        .iter()
        .filter(|i| {
            (i.rect_pos[0] - thumb[0]).abs() < 2.5
                && (i.rect_pos[1] - thumb[1]).abs() < 2.5
                && (i.rect_size[1] - thumb[3]).abs() < 2.5
                && i.rect_size[0] > 1.0
        })
        .map(|i| i.color[3])
        .fold(0.0_f32, f32::max)
}

#[test]
fn overlay_auto_fade_paints_cubic_without_rewriting_hit_slab() {
    const REST: &str = "diag-auto-fade";
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
                    .scrollbar_visibility("auto")
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

            settle_cubic(REST, "y", 1.0);

            let shown =
                layout.build_and_layout_fast(&mut root, 220.0, 120.0, [-1.0, -1.0], true, true);
            let track_shown = find_focus_bounds(&root, SCROLLBAR_TRACK_FOCUS_PREFIX, ":y")
                .expect("Y track shown");
            let thumb_shown = find_focus_bounds(&root, SCROLLBAR_THUMB_FOCUS_PREFIX, ":y")
                .expect("Y thumb shown");
            let shown_a = thumb_fill_alpha(&shown, thumb_shown);
            assert!(
                thumb_style_alpha(&root).is_some_and(|a| a < 0.01),
                "regression seam: ViewportsOnly must not need style.bg un-zeroed"
            );
            assert!(
                shown_a > 0.85,
                "visible cubic must paint thumb even while style.bg is still build-time 0, \
                 got alpha {shown_a} bounds={thumb_shown:?}"
            );
            assert!(
                thumb_shown[3] > 20.0,
                "shown thumb must not be crushed to a top cap, h={}",
                thumb_shown[3]
            );
            assert!(
                track_shown[2] >= 7.5,
                "8px hit slab must stay, track w={}",
                track_shown[2]
            );

            settle_cubic(REST, "y", 0.0);
            let hidden =
                layout.build_and_layout_fast(&mut root, 220.0, 120.0, [-1.0, -1.0], true, true);
            let track_hidden = find_focus_bounds(&root, SCROLLBAR_TRACK_FOCUS_PREFIX, ":y")
                .expect("Y track hidden");
            let thumb_hidden = find_focus_bounds(&root, SCROLLBAR_THUMB_FOCUS_PREFIX, ":y")
                .expect("Y thumb hidden");
            let hidden_a = thumb_fill_alpha(&hidden, thumb_hidden);
            assert!(
                hidden_a < 0.05,
                "hidden cubic must paint ~0, got alpha {hidden_a}"
            );
            assert!(
                (track_shown[0] - track_hidden[0]).abs() < 0.51
                    && (track_shown[1] - track_hidden[1]).abs() < 0.51
                    && (track_shown[2] - track_hidden[2]).abs() < 0.51
                    && (track_shown[3] - track_hidden[3]).abs() < 0.51,
                "fade must not rewrite track hit slab {track_shown:?} → {track_hidden:?}"
            );
            assert!(
                (thumb_shown[2] - thumb_hidden[2]).abs() < 0.51
                    && (thumb_shown[3] - thumb_hidden[3]).abs() < 0.51,
                "fade must not crush thumb size {thumb_shown:?} → {thumb_hidden:?}"
            );
        });
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn overlay_auto_fade_replays_underlay_on_alpha_change_not_idle_rest() {
    const REST: &str = "diag-auto-fade-present";
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
                    .scrollbar_visibility("auto")
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

            let _ = take_frame_paint_nodes();
            settle_cubic(REST, "y", 1.0);
            layout.set_collect_skip_allowed(true);
            let shown =
                layout.build_and_layout_fast(&mut root, 220.0, 120.0, [-1.0, -1.0], true, true);
            let thumb = find_focus_bounds(&root, SCROLLBAR_THUMB_FOCUS_PREFIX, ":y")
                .expect("Y thumb shown");
            let shown_a = thumb_fill_alpha(&shown, thumb);
            assert!(
                shown_a > 0.99,
                "settled show must paint opaque thumb (not mid-fade gray), got {shown_a}"
            );
            let show_dirty = take_frame_paint_nodes();
            assert!(
                !show_dirty.is_empty(),
                "first opaque collect must dest-reset so PartialLoad presents the black thumb"
            );

            let _idle =
                layout.build_and_layout_fast(&mut root, 220.0, 120.0, [-1.0, -1.0], true, true);
            let idle_dirty = take_frame_paint_nodes();
            assert!(
                idle_dirty.is_empty(),
                "settled opaque must not dest-reset the rail every idle frame, got {}",
                idle_dirty.len()
            );

            let tw = use_ui_transition_cubic(&format!("scrollbar_spring_{REST}:y"), 0.0);
            tw.set(0.0);
            let _ = tick_animations(0.016);
            let mid =
                layout.build_and_layout_fast(&mut root, 220.0, 120.0, [-1.0, -1.0], true, true);
            let mid_a = thumb_fill_alpha(&mid, thumb);
            assert!(
                mid_a > 0.05 && mid_a < 0.99,
                "in-flight hide must paint live cubic, got {mid_a}"
            );
            let fade_dirty = take_frame_paint_nodes();
            assert!(
                !fade_dirty.is_empty(),
                "in-flight fade must dest-reset or PartialLoad keeps the previous alpha"
            );
            for dirty in &fade_dirty {
                assert_eq!(dirty.kind, crate::app::damage::PaintDirtyKind::Region);
                assert!(!crate::app::draw_cull::region_replays_own_chrome_only(
                    dirty.key,
                    dirty.bounds,
                    &mid.node_paints,
                ));
            }

            settle_cubic(REST, "y", 0.0);
            let hidden =
                layout.build_and_layout_fast(&mut root, 220.0, 120.0, [-1.0, -1.0], true, true);
            let hidden_a = thumb_fill_alpha(&hidden, thumb);
            assert!(
                hidden_a < 0.05,
                "settled hide must omit the thumb, got {hidden_a}"
            );
            let hide_dirty = take_frame_paint_nodes();
            assert!(
                !hide_dirty.is_empty(),
                "settle-to-zero must dest-reset or retain keeps the last gray pixels"
            );
            let mut covered = false;
            for d in &hide_dirty {
                assert_eq!(d.kind, crate::app::damage::PaintDirtyKind::Region);
                assert!(!crate::app::draw_cull::region_replays_own_chrome_only(
                    d.key,
                    d.bounds,
                    &hidden.node_paints,
                ));
                let wipe = d.bounds;
                let overlaps_thumb = crate::app::damage::intersect_rects(wipe, thumb).is_some();
                if overlaps_thumb {
                    let covers = wipe[0] <= thumb[0] + 1.5
                        && wipe[1] <= thumb[1] + 1.5
                        && wipe[0] + wipe[2] + 1.5 >= thumb[0] + thumb[2]
                        && wipe[1] + wipe[3] + 1.5 >= thumb[1] + thumb[3];
                    assert!(
                        covers,
                        "fade dest-reset must cover the last painted thumb, not a leftover cap\n wipe={wipe:?} thumb={thumb:?} dirty={:?}",
                        d.bounds
                    );
                    covered = true;
                }
            }
            assert!(
                covered,
                "overlay auto hide must dest-reset the painted thumb (not skip CLEAR), hide_dirty={hide_dirty:?} thumb={thumb:?}"
            );

            let _idle_hidden =
                layout.build_and_layout_fast(&mut root, 220.0, 120.0, [-1.0, -1.0], true, true);
            let rest_dirty = take_frame_paint_nodes();
            assert!(
                rest_dirty.is_empty(),
                "settled hidden must not dest-reset the 8px rail every idle tick, got {}",
                rest_dirty.len()
            );
        });
    });
}

fn find_focus_mut<'a>(
    el: &'a mut Element,
    prefix: &str,
    axis_suffix: &str,
) -> Option<&'a mut Element> {
    if el
        .focus_id
        .as_deref()
        .is_some_and(|f| f.starts_with(prefix) && f.ends_with(axis_suffix))
    {
        return Some(el);
    }
    let n = el.children.len();
    for i in 0..n {
        if find_focus_bounds(&el.children[i], prefix, axis_suffix).is_some() {
            return find_focus_mut(&mut el.children[i], prefix, axis_suffix);
        }
    }
    None
}

fn poison_taffy_thumb_height(layout: &mut LayoutTree, thumb: &Element, height: f32) {
    let nid = thumb.layout_node.expect("thumb taffy node");
    let mut st = layout.taffy.style(nid).cloned().expect("thumb style");
    st.size.height = taffy::style::Dimension::length(height);
    st.min_size.height = taffy::style::LengthPercentageAuto::length(height);
    st.max_size.height = taffy::style::LengthPercentageAuto::length(height);
    layout.taffy.set_style(nid, st).expect("set thumb style");
    let root = layout.root_node.expect("root");
    layout
        .taffy
        .compute_layout(
            root,
            taffy::geometry::Size {
                width: taffy::style::AvailableSpace::Definite(220.0),
                height: taffy::style::AvailableSpace::Definite(120.0),
            },
        )
        .expect("recompute after poison");
}

#[test]
fn overlay_skip_replay_keeps_signal_thumb_not_taffy_min_cap() {
    const REST: &str = "diag-skip-stamp";
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
                    .scrollbar_visibility("auto")
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
            settle_cubic(REST, "y", 1.0);

            layout.set_collect_skip_allowed(true);
            let _warm =
                layout.build_and_layout_fast(&mut root, 220.0, 120.0, [-1.0, -1.0], true, true);
            let overlay_thumb = find_focus_bounds(&root, SCROLLBAR_THUMB_FOCUS_PREFIX, ":y")
                .expect("overlay thumb");
            assert!(
                overlay_thumb[3] > 20.0,
                "precondition: overlay thumb must be taller than the min cap, h={}",
                overlay_thumb[3]
            );

            let thumb_el = find_focus_mut(&mut root, SCROLLBAR_THUMB_FOCUS_PREFIX, ":y")
                .expect("thumb element");
            poison_taffy_thumb_height(&mut layout, thumb_el, 8.0);
            let nid = thumb_el.layout_node.expect("thumb node");
            let taffy_h = layout.taffy.layout(nid).expect("thumb layout").size.height;
            assert!(
                (taffy_h - 8.0).abs() < 0.51,
                "precondition: Taffy leftover must be the crushed cap, got {taffy_h}"
            );

            let _skipped =
                layout.build_and_layout_fast(&mut root, 220.0, 120.0, [-1.0, -1.0], true, true);
            let (skipped, _patched) = layout.last_collect_skip_counts();
            assert!(
                skipped >= 1,
                "idle overlay collect must be able to skip; otherwise this does not test skip stamp"
            );
            let after = find_focus_bounds(&root, SCROLLBAR_THUMB_FOCUS_PREFIX, ":y")
                .expect("thumb after skip");
            assert!(
                (after[3] - overlay_thumb[3]).abs() < 0.51,
                "skip must stamp overlay thumb, not Taffy min-cap {overlay_thumb:?} → {after:?} taffy_h={taffy_h}"
            );
            assert!(
                after[3] > 20.0,
                "skip must not crush hit to the groove top, h={}",
                after[3]
            );
        });
    });
}

fn find_viewport_mut<'a>(el: &'a mut Element, rest: &str) -> Option<&'a mut Element> {
    let want = format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}{rest}");
    if el.focus_id.as_deref() == Some(want.as_str()) {
        return Some(el);
    }
    fn contains(el: &Element, rest: &str) -> bool {
        let want = format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}{rest}");
        el.focus_id.as_deref() == Some(want.as_str())
            || el.children.iter().any(|c| contains(c, rest))
    }
    let n = el.children.len();
    for i in 0..n {
        if contains(&el.children[i], rest) {
            return find_viewport_mut(&mut el.children[i], rest);
        }
    }
    None
}

fn thumb_paint_y(result: &RenderResult, thumb: [f32; 4]) -> Option<f32> {
    result
        .instances
        .iter()
        .filter(|i| {
            i.color[3] > 0.2
                && (i.rect_size[0] - thumb[2]).abs() < 2.5
                && (i.rect_size[1] - thumb[3]).abs() < 2.5
        })
        .min_by(|a, b| {
            let da = (a.rect_pos[0] - thumb[0]).hypot(a.rect_pos[1] - thumb[1]);
            let db = (b.rect_pos[0] - thumb[0]).hypot(b.rect_pos[1] - thumb[1]);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|i| i.rect_pos[1])
}

/// MessageScroller uses jump + overlay sibling rail. Jump leaves collect-skip on, so
/// the track fingerprint must include live offset or the painted thumb freezes.
#[test]
fn overlay_skip_must_repaint_thumb_when_offset_changes() {
    const REST: &str = "diag-skip-offset";
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
            let first =
                layout.build_and_layout_fast(&mut root, 220.0, 120.0, [-1.0, -1.0], true, true);
            let thumb0 =
                find_focus_bounds(&root, SCROLLBAR_THUMB_FOCUS_PREFIX, ":y").expect("Y thumb at 0");
            let paint0 = thumb_paint_y(&first, thumb0).expect("painted thumb at 0");
            assert!(
                (paint0 - thumb0[1]).abs() < 2.5,
                "precondition: overlay collect must paint the thumb at {thumb0:?}, got y={paint0}"
            );

            let max_y = (content_size[1] - bounds[3]).max(0.0);
            assert!(
                max_y > 40.0,
                "precondition: content must overflow, max={max_y}"
            );
            let mid = max_y * 0.5;
            use_signal(&scroll_offset_y_signal_key(REST), || 0.0_f32).set(mid);
            if let Some(vp) = find_viewport_mut(&mut root, REST) {
                vp.scroll_offset[1] = mid;
            }

            let second =
                layout.build_and_layout_fast(&mut root, 220.0, 120.0, [-1.0, -1.0], true, true);
            let thumb1 = find_focus_bounds(&root, SCROLLBAR_THUMB_FOCUS_PREFIX, ":y")
                .expect("Y thumb after offset");
            assert!(
                thumb1[1] > thumb0[1] + 8.0,
                "stamped hit must follow the new offset {thumb0:?} → {thumb1:?}"
            );
            let paint1 = thumb_paint_y(&second, thumb1).expect("painted thumb after offset");
            assert!(
                (paint1 - thumb1[1]).abs() < 2.5,
                "skip must not freeze the painted thumb at y={paint0} while hit moved to {thumb1:?}; \
                 painted y={paint1}"
            );
        });
    });
}

#[test]
fn overlay_thumb_ratio_uses_live_track_not_zero_vh_signal() {
    const REST: &str = "diag-vh-vis";
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
                    .scrollbar_visibility("auto")
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
            use_signal(&scroll_viewport_h_signal_key(REST), || 0.0_f32).set(0.0);
            use_signal(&scroll_content_w_signal_key(REST), || 0.0_f32).set(content_size[0]);
            use_signal(&scroll_content_h_signal_key(REST), || 0.0_f32).set(content_size[1]);
            settle_cubic(REST, "y", 1.0);

            let _shown =
                layout.build_and_layout_fast(&mut root, 220.0, 120.0, [-1.0, -1.0], true, true);
            let track =
                find_focus_bounds(&root, SCROLLBAR_TRACK_FOCUS_PREFIX, ":y").expect("Y track");
            let thumb =
                find_focus_bounds(&root, SCROLLBAR_THUMB_FOCUS_PREFIX, ":y").expect("Y thumb");
            assert!(
                track[3] > 40.0,
                "precondition: overlay track must have live Taffy height, {track:?}"
            );
            assert!(
                thumb[3] > 20.0,
                "ratio must use track along-size (vh_vis), not signal vh=0 min-cap, thumb={thumb:?} track={track:?}"
            );
        });
    });
}

fn find_by_focus_id<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
    if el.focus_id.as_deref() == Some(id) {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_by_focus_id(c, id))
}

fn path_focus_ids(path: &[crate::app::pointer_events::HitPathEntry]) -> Vec<String> {
    path.iter().filter_map(|e| e.focus_id.clone()).collect()
}

fn path_hits_scrollbar(path: &[crate::app::pointer_events::HitPathEntry]) -> bool {
    path.iter().any(|e| {
        e.focus_id.as_deref().is_some_and(|f| {
            f.starts_with(SCROLLBAR_TRACK_FOCUS_PREFIX)
                || f.starts_with(SCROLLBAR_THUMB_FOCUS_PREFIX)
        })
    })
}

/// Overlay `auto` bar is visible during / after wheel (cubic=1, z=1000). Content
/// clicks in the viewport must still hit the button, not the overlay track.
#[test]
fn overlay_visible_must_not_steal_content_button_hit() {
    const REST: &str = "diag-click-during-scroll";
    const BTN: &str = "scroll-click-btn";
    let clicked = Rc::new(Cell::new(false));
    let clicked_flag = clicked.clone();
    let (_handle, _) = create_root(|| {
        create_effect(move || {
            let btn: Element = button(BTN)
                .w("100%")
                .on_click({
                    let clicked_flag = clicked_flag.clone();
                    move || clicked_flag.set(true)
                })
                .into();
            let content = Element::new()
                .flex_col()
                .w("100%")
                .gap(8.0)
                .child(Element::new().w("100%").h(40.0).bg([0.8, 0.8, 0.8, 1.0]))
                .child(btn)
                .children((0..12).map(|i| {
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
                    .scrollbar_visibility("auto")
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
            use_signal(&scroll_offset_y_signal_key(REST), || 0.0_f32).set(36.0);
            settle_cubic(REST, "y", 1.0);

            let _shown =
                layout.build_and_layout_fast(&mut root, 220.0, 120.0, [-1.0, -1.0], true, true);

            let track = find_focus_bounds(&root, SCROLLBAR_TRACK_FOCUS_PREFIX, ":y")
                .expect("Y overlay track");
            assert!(
                track[2] < bounds[2] * 0.5,
                "overlay track must stay an edge slab, not cover the viewport: track={track:?} viewport={bounds:?}"
            );

            let btn_el = find_by_focus_id(&root, BTN).expect("button");
            let vis = crate::core::tree::visual_paint_aabb(btn_el, btn_el.paint_scroll_acc);
            let (x, y) = (vis[0] + vis[2] * 0.5, vis[1] + vis[3] * 0.5);
            assert!(
                vis[2] > 8.0 && vis[3] > 8.0,
                "button must have a hittable visual box, vis={vis:?} layout={:?}",
                btn_el.layout_bounds
            );
            assert!(
                y >= bounds[1] && y <= bounds[1] + bounds[3],
                "probe must sit in the viewport, y={y} vis={vis:?} viewport={bounds:?}"
            );

            let path = hit_path(&root, x, y);
            let ids = path_focus_ids(&path);
            assert!(
                !path_hits_scrollbar(&path),
                "overlay z=1000 must not steal the content click at ({x},{y}) vis={vis:?} track={track:?} path={ids:?}"
            );
            let (cb, _) = resolve_click_from_path(&root, &path);
            assert!(
                cb.is_some(),
                "button on_click must be on the hit path during overlay-visible scroll, ({x},{y}) vis={vis:?} track={track:?} path={ids:?}"
            );
            cb.unwrap().borrow_mut()();
            assert!(clicked.get(), "resolved on_click must run");
        });
    });
}

/// Engine-driven scroll between mousedown and mouseup must not cancel a still-pointer
/// click: the press path is the gesture, not the live mouseup hit.
#[test]
fn click_must_survive_inertial_scroll_under_stationary_pointer() {
    const REST: &str = "diag-click-inertia";
    const BTN: &str = "inertia-click-btn";
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let btn: Element = button(BTN).w("100%").on_click(|| {}).into();
            let content = Element::new()
                .flex_col()
                .w("100%")
                .gap(8.0)
                .child(Element::new().w("100%").h(24.0))
                .child(btn)
                .children((0..16).map(|i| {
                    Element::new()
                        .w("100%")
                        .h(24.0)
                        .template_id(format!("row-{i}"))
                }));

            let mut root = Element::from(
                scroll_area(REST)
                    .w(220.0)
                    .h(120.0)
                    .overflow_y("auto")
                    .overflow_x("hidden")
                    .scrollbar_visibility("auto")
                    .scroll_behavior("smooth")
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
            use_signal(&scroll_offset_y_signal_key(REST), || 0.0_f32).set(8.0);
            settle_cubic(REST, "y", 1.0);
            layout.build_and_layout_fast(&mut root, 220.0, 120.0, [-1.0, -1.0], true, true);

            let btn_el = find_by_focus_id(&root, BTN).expect("button");
            let vis = crate::core::tree::visual_paint_aabb(btn_el, btn_el.paint_scroll_acc);
            let (x, y) = (vis[0] + vis[2] * 0.5, vis[1] + vis[3] * 0.5);
            let down = hit_path(&root, x, y);
            let down_ids = path_focus_ids(&down);
            assert!(
                down.iter().any(|e| e.on_click.is_some()),
                "press must hit the button, path={down_ids:?} vis={vis:?}"
            );

            use_signal(&scroll_offset_y_signal_key(REST), || 0.0_f32).set(48.0);
            layout.build_and_layout_fast(&mut root, 220.0, 120.0, [-1.0, -1.0], true, true);
            let up = hit_path(&root, x, y);
            let up_ids = path_focus_ids(&up);
            let lca = click_dispatch_path(&down, &up);
            let (lca_cb, _) = resolve_click_from_path(&root, &lca);
            assert!(
                lca_cb.is_none(),
                "precondition: live mouseup LCA must have lost the button, down={down_ids:?} up={up_ids:?} lca={:?}",
                path_focus_ids(&lca)
            );
            let click_path = click_gesture_path(&down, &up, false, false);
            let (cb, _) = resolve_click_from_path(&root, &click_path);
            assert!(
                cb.is_some(),
                "still pointer must click the press target after engine scroll, down={down_ids:?} up={up_ids:?} click={:?}",
                path_focus_ids(&click_path)
            );
        });
    });
}

fn rect_covers(outer: [f32; 4], inner: [f32; 4]) -> bool {
    outer[0] <= inner[0] + 1.5
        && outer[1] <= inner[1] + 1.5
        && outer[0] + outer[2] + 1.5 >= inner[0] + inner[2]
        && outer[1] + outer[3] + 1.5 >= inner[1] + inner[3]
}

/// Gallery wrap=off: after the horizontal auto-bar fades, dest-reset must erase
/// the track that was on screen. A wipe shrunk to the sliver outside the field
/// leaves that rectangle in PartialLoad.
#[test]
fn horizontal_auto_hide_replay_covers_the_painted_track() {
    const REST: &str = "diag-h-fade";
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let content = Element::new().w(640.0).h(24.0).bg([0.2, 0.2, 0.2, 1.0]);
            let mut root = Element::from(
                scroll_area(REST)
                    .w(220.0)
                    .h(96.0)
                    .rounded(6.0)
                    .bg([1.0, 1.0, 1.0, 1.0])
                    .overflow_x("auto")
                    .overflow_y("hidden")
                    .scrollbar_visibility("auto")
                    .child(content),
            );

            let mut layout = LayoutTree::new();
            let full = layout.build_and_layout(&mut root, 220.0, 96.0, [-1.0, -1.0]);
            let (_id, bounds, _max, content_size) = full
                .scroll_metrics
                .iter()
                .find(|(id, ..)| id == REST)
                .expect("scroll metrics");
            use_signal(&scroll_viewport_w_signal_key(REST), || 0.0_f32).set(bounds[2]);
            use_signal(&scroll_viewport_h_signal_key(REST), || 0.0_f32).set(bounds[3]);
            use_signal(&scroll_content_w_signal_key(REST), || 0.0_f32).set(content_size[0]);
            use_signal(&scroll_content_h_signal_key(REST), || 0.0_f32).set(content_size[1]);
            use_signal(&scroll_offset_x_signal_key(REST), || 0.0_f32).set(0.0);

            let _ = take_frame_paint_nodes();
            settle_cubic(REST, "x", 1.0);
            layout.set_collect_skip_allowed(true);
            let shown =
                layout.build_and_layout_fast(&mut root, 220.0, 96.0, [-1.0, -1.0], true, true);
            let track = find_focus_bounds(&root, SCROLLBAR_TRACK_FOCUS_PREFIX, ":x")
                .expect("X track shown");
            let thumb = find_focus_bounds(&root, SCROLLBAR_THUMB_FOCUS_PREFIX, ":x")
                .expect("X thumb shown");
            assert!(
                track[2] > 40.0 && track[3] > 4.0,
                "shown horizontal track must be a real bar, got {track:?}"
            );
            assert!(thumb_fill_alpha(&shown, track) > 0.99);
            let _ = take_frame_paint_nodes();

            settle_cubic(REST, "x", 0.0);
            let hidden =
                layout.build_and_layout_fast(&mut root, 220.0, 96.0, [-1.0, -1.0], true, true);
            let hide_dirty = take_frame_paint_nodes();
            let mut covered_track = false;
            for d in &hide_dirty {
                assert_eq!(d.kind, crate::app::damage::PaintDirtyKind::Region);
                assert!(!crate::app::draw_cull::region_replays_own_chrome_only(
                    d.key,
                    d.bounds,
                    &hidden.node_paints,
                ));
                if rect_covers(d.bounds, track) {
                    covered_track = true;
                }
            }
            assert!(
                covered_track,
                "horizontal hide replay must cover the last painted track\n track={track:?} thumb={thumb:?} field={bounds:?}\n dirties={hide_dirty:?}"
            );
            let hidden_track_a = thumb_fill_alpha(&hidden, track);
            assert!(
                hidden_track_a < 0.05,
                "settled hide must not keep painting the track, alpha={hidden_track_a} track={track:?}"
            );
        });
    });
}

/// Page-switch Full prepass runs while the new rest's vw/vh signals are still 0.
/// Collapsing the overlay track then leaves ViewportsOnly unable to restore a
/// hit slab — wheel/hover never light the bar on later pages.
#[test]
fn full_prepass_must_not_collapse_overlay_track_before_metrics() {
    const REST: &str = "diag-switch-collapse";
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
                    .scrollbar_visibility("auto")
                    .child(content),
            );
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 220.0, 120.0, [-1.0, -1.0]);
            // New rest: signals unset (page switch before post-render).
            use_signal(&scroll_viewport_w_signal_key(REST), || 0.0_f32).set(0.0);
            use_signal(&scroll_viewport_h_signal_key(REST), || 0.0_f32).set(0.0);
            use_signal(&scroll_content_w_signal_key(REST), || 0.0_f32).set(0.0);
            use_signal(&scroll_content_h_signal_key(REST), || 0.0_f32).set(0.0);
            crate::app::update_scrollbar_elements_for_test(
                &mut root,
                &mut layout,
                crate::app::ScrollbarUpdateMode::Full,
            );
            let _ = layout.build_and_layout(&mut root, 220.0, 120.0, [-1.0, -1.0]);
            let track_after_zero =
                find_focus_bounds(&root, SCROLLBAR_TRACK_FOCUS_PREFIX, ":y").expect("track");
            assert!(
                track_after_zero[2] > 2.0 && track_after_zero[3] > 40.0,
                "unset metrics must not collapse the overlay hit slab\n track={track_after_zero:?}"
            );

            let full = layout.build_and_layout(&mut root, 220.0, 120.0, [-1.0, -1.0]);
            let (_id, bounds, _max, content_size) = full
                .scroll_metrics
                .iter()
                .find(|(id, ..)| id == REST)
                .expect("metrics");
            use_signal(&scroll_viewport_w_signal_key(REST), || 0.0_f32).set(bounds[2]);
            use_signal(&scroll_viewport_h_signal_key(REST), || 0.0_f32).set(bounds[3]);
            use_signal(&scroll_content_w_signal_key(REST), || 0.0_f32).set(content_size[0]);
            use_signal(&scroll_content_h_signal_key(REST), || 0.0_f32).set(content_size[1]);
            crate::app::update_scrollbar_elements_for_test(
                &mut root,
                &mut layout,
                crate::app::ScrollbarUpdateMode::ViewportsOnly,
            );
            settle_cubic(REST, "y", 1.0);
            let shown =
                layout.build_and_layout_fast(&mut root, 220.0, 120.0, [-1.0, -1.0], true, true);
            let track =
                find_focus_bounds(&root, SCROLLBAR_TRACK_FOCUS_PREFIX, ":y").expect("track");
            let thumb =
                find_focus_bounds(&root, SCROLLBAR_THUMB_FOCUS_PREFIX, ":y").expect("thumb");
            let a = thumb_fill_alpha(&shown, thumb);
            let path = hit_path(&root, track[0] + track[2] * 0.5, track[1] + track[3] * 0.5);
            let hit = crate::app::pointer_events::path_deepest_focus_id(&path);
            assert!(
                track[3] > 40.0 && a > 0.85 && hit.is_some_and(|id| id.contains("scrollbar")),
                "after metrics arrive, ViewportsOnly must keep a hittable painted bar\n\
                 track={track:?} thumb={thumb:?} a={a} hit={hit:?}"
            );
        });
    });
}

/// H-only `auto`: build must not bake a Y gutter, and unmeasured Full must not
/// invent one. ViewportsOnly after metrics stays at the edge pad (no second Full).
#[test]
fn auto_h_only_must_not_reserve_vertical_lane_gutter() {
    const REST: &str = "diag-h-lane-release";
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = Element::from(
                scroll_area(REST)
                    .w(240.0)
                    .h(80.0)
                    .overflow_x("auto")
                    .overflow_y("auto")
                    .scrollbar_visibility("auto")
                    .child(Element::new().w(480.0).h(24.0).bg([0.8, 0.8, 0.8, 1.0])),
            );
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 240.0, 80.0, [-1.0, -1.0]);
            use_signal(&scroll_viewport_w_signal_key(REST), || 0.0_f32).set(0.0);
            use_signal(&scroll_viewport_h_signal_key(REST), || 0.0_f32).set(0.0);
            use_signal(&scroll_content_w_signal_key(REST), || 0.0_f32).set(0.0);
            use_signal(&scroll_content_h_signal_key(REST), || 0.0_f32).set(0.0);
            crate::app::update_scrollbar_elements_for_test(
                &mut root,
                &mut layout,
                crate::app::ScrollbarUpdateMode::Full,
            );
            let _ = layout.build_and_layout(&mut root, 240.0, 80.0, [-1.0, -1.0]);
            let h_zero =
                find_focus_bounds(&root, SCROLLBAR_TRACK_FOCUS_PREFIX, ":x").expect("h track");
            let h_right_zero = h_zero[0] + h_zero[2];
            assert!(
                h_right_zero > 240.0 - 6.0 && h_right_zero <= 240.0 + 0.5,
                "unmeasured auto must not reserve a Y gutter\n h={h_zero:?} right={h_right_zero}"
            );

            let full = layout.build_and_layout(&mut root, 240.0, 80.0, [-1.0, -1.0]);
            let (_id, bounds, _max, content_size) = full
                .scroll_metrics
                .iter()
                .find(|(id, ..)| id == REST)
                .expect("metrics");
            use_signal(&scroll_viewport_w_signal_key(REST), || 0.0_f32).set(bounds[2]);
            use_signal(&scroll_viewport_h_signal_key(REST), || 0.0_f32).set(bounds[3]);
            use_signal(&scroll_content_w_signal_key(REST), || 0.0_f32).set(content_size[0]);
            use_signal(&scroll_content_h_signal_key(REST), || 0.0_f32).set(content_size[1]);

            crate::app::update_scrollbar_elements_for_test(
                &mut root,
                &mut layout,
                crate::app::ScrollbarUpdateMode::ViewportsOnly,
            );
            let _ = layout.build_and_layout(&mut root, 240.0, 80.0, [-1.0, -1.0]);
            let h = find_focus_bounds(&root, SCROLLBAR_TRACK_FOCUS_PREFIX, ":x").expect("h track");
            let h_right = h[0] + h[2];
            assert!(
                h_right > 240.0 - 6.0 && h_right <= 240.0 + 0.5,
                "H-only after metrics must keep H at the edge pad\n h={h:?} right={h_right}"
            );

            crate::app::update_scrollbar_elements_for_test(
                &mut root,
                &mut layout,
                crate::app::ScrollbarUpdateMode::Full,
            );
            let _ = layout.build_and_layout(&mut root, 240.0, 80.0, [-1.0, -1.0]);
            let v = find_focus_bounds(&root, SCROLLBAR_TRACK_FOCUS_PREFIX, ":y").expect("y track");
            assert!(
                v[2] < 1.0 || v[3] < 1.0,
                "Y must collapse when content fits vertically: v={v:?}"
            );
        });
    });
}

/// Overlay auto-bar is absolute + z-index, so hide is a page-stack slot with
/// no quads. Own-chrome replay draws nothing and PartialLoad keeps the gray
/// track. A Node flat-fill of that box samples one content row and drops the
/// thumb on the next show. Page replay has to restore the covered content.
#[test]
fn overlay_auto_hide_must_replay_underlay_not_flat_fill() {
    const REST: &str = "diag-auto-fade-region";
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
                        .bg([0.2, 0.45, 0.9, 1.0])
                        .template_id(format!("row-{i}"))
                }));

            let mut root = Element::from(
                scroll_area(REST)
                    .w(220.0)
                    .h(120.0)
                    .bg([1.0, 1.0, 1.0, 1.0])
                    .overflow_y("auto")
                    .overflow_x("hidden")
                    .scrollbar_visibility("auto")
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

            let _ = take_frame_paint_nodes();
            settle_cubic(REST, "y", 1.0);
            layout.set_collect_skip_allowed(true);
            let shown =
                layout.build_and_layout_fast(&mut root, 220.0, 120.0, [-1.0, -1.0], true, true);
            let thumb = find_focus_bounds(&root, SCROLLBAR_THUMB_FOCUS_PREFIX, ":y")
                .expect("Y thumb shown");
            let track = find_focus_bounds(&root, SCROLLBAR_TRACK_FOCUS_PREFIX, ":y")
                .expect("Y track shown");
            let _ = take_frame_paint_nodes();

            settle_cubic(REST, "y", 0.0);
            let hidden =
                layout.build_and_layout_fast(&mut root, 220.0, 120.0, [-1.0, -1.0], true, true);
            let mut hide_dirty = take_frame_paint_nodes();
            let prev = crate::app::draw_cull::last_present_visual_slots(
                &shown.node_paints,
                &shown.instances,
                &[],
                &shown.text_nodes,
                &[],
            );
            let layered: Vec<_> = hidden
                .node_paints
                .iter()
                .filter(|p| p.layered)
                .map(|p| (p.key, p.bounds, p.instance))
                .collect();
            let before = hide_dirty.clone();
            crate::app::draw_cull::prepare_node_present_dirties(
                &mut hide_dirty,
                &hidden.node_paints,
                &prev,
                &hidden.instances,
                &[],
                &[],
                1.0,
                None,
            );
            let mut covered_track = false;
            let mut covered_thumb = false;
            for d in &hide_dirty {
                assert_eq!(
                    d.kind,
                    crate::app::damage::PaintDirtyKind::Region,
                    "hidden bar must page-replay, not flat-fill\n dirty={d:?}\n before={before:?}\n layered={layered:?}"
                );
                assert!(
                    !crate::app::draw_cull::region_replays_own_chrome_only(
                        d.key,
                        d.bounds,
                        &hidden.node_paints,
                    ),
                    "omitted quads must not take own-chrome (that draws nothing)\n dirty={d:?}"
                );
                if rect_covers(d.bounds, track) {
                    covered_track = true;
                }
                if rect_covers(d.bounds, thumb) {
                    covered_thumb = true;
                }
            }
            assert!(
                covered_track && covered_thumb,
                "hide replay must cover the last painted bar\n track={track:?} thumb={thumb:?}\n dirties={hide_dirty:?}"
            );

            let _ = take_frame_paint_nodes();
            settle_cubic(REST, "y", 1.0);
            let again =
                layout.build_and_layout_fast(&mut root, 220.0, 120.0, [-1.0, -1.0], true, true);
            let again_thumb = find_focus_bounds(&root, SCROLLBAR_THUMB_FOCUS_PREFIX, ":y")
                .expect("thumb on reshow");
            let again_a = thumb_fill_alpha(&again, again_thumb);
            assert!(
                again_a > 0.9,
                "reshow must paint the thumb, alpha={again_a} bounds={again_thumb:?}"
            );
        });
    });
}
