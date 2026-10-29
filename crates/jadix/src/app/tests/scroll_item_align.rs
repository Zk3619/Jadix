use super::{
    apply_pending_scroll_item_into_view, cancel_scroll_item_into_view,
    has_pending_scroll_item_align, request_scroll_item_into_view, reset_scroll_item_into_view,
    scroll_offset_to_center_item, set_scroll_offset_with_behavior, ScrollItemAlign,
};
use crate::core::reactivity::create_root;
use crate::core::scroll_viewport::scroll_offset_y_signal_key;

#[test]
fn year_center_uses_real_row_height_not_32px() {
    // 1900..=2100 → 201 items; 2026 is index 126. Real Aria row ≈ 26px, not 32.
    let sy_real = scroll_offset_to_center_item(126, 26.0, 248.0, 201);
    let sy_wrong = scroll_offset_to_center_item(126, 32.0, 248.0, 201);
    assert!(
        (sy_wrong - sy_real).abs() > 500.0,
        "32px guess is hundreds of px off, got real={sy_real} wrong={sy_wrong}"
    );
    let expected = 126.0 * 26.0 + 13.0 - 124.0;
    assert!((sy_real - expected).abs() < 0.5);
}

#[test]
fn pending_align_waits_for_metrics_then_writes_offset() {
    let _ = create_root(|| {
        cancel_scroll_item_into_view("t-year-align");
        request_scroll_item_into_view("t-year-align", 126, 201, ScrollItemAlign::Center);
        assert!(has_pending_scroll_item_align());
        assert!(!apply_pending_scroll_item_into_view(
            "t-year-align",
            0.0,
            0.0
        ));
        assert!(has_pending_scroll_item_align());
        let content_h = 201.0 * 26.0;
        assert!(apply_pending_scroll_item_into_view(
            "t-year-align",
            248.0,
            content_h
        ));
        assert!(!has_pending_scroll_item_align());
        let sy = crate::core::reactivity::use_signal(
            &scroll_offset_y_signal_key("t-year-align"),
            || 0.0_f32,
        )
        .peek();
        let expected = scroll_offset_to_center_item(126, 26.0, 248.0, 201);
        assert!(
            (sy - expected).abs() < 0.5,
            "offset should match real row height, got {sy} expected {expected}"
        );
    });
}

#[test]
fn pending_align_uses_placed_cap_immediately_while_expand_pins() {
    use super::clamp_max_y_for_pending_align;
    use crate::utils::popup::seed_floating_place_max_h_for_test;

    let _ = create_root(|| {
        let overlay_id = "cal-month";
        let rest = "cal-month:menu_scroll";
        cancel_scroll_item_into_view(rest);
        seed_floating_place_max_h_for_test(overlay_id, 200.0);
        request_scroll_item_into_view(rest, 11, 12, ScrollItemAlign::Center);
        let content_h = 12.0 * 32.0;
        let placed_vh = 200.0 - 8.0;
        // Expand pin: write against placed cap now (no post-open jump).
        assert!(apply_pending_scroll_item_into_view(
            rest, content_h, content_h
        ));
        assert!(!has_pending_scroll_item_align());
        let expected = scroll_offset_to_center_item(11, 32.0, placed_vh, 12);
        let sy = crate::core::reactivity::use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32)
            .peek();
        assert!(
            (sy - expected).abs() < 0.5,
            "must center against placed cap while pinned, got {sy} expected {expected}"
        );
        assert!(
            (clamp_max_y_for_pending_align(rest, 0.0) - expected).abs() < 0.5,
            "pre-render clamp must not wipe the align while max_y is 0"
        );
        // Mid-expand layout max is only one row — still must keep the placed offset.
        request_scroll_item_into_view(rest, 11, 12, ScrollItemAlign::Center);
        assert!(apply_pending_scroll_item_into_view(
            rest,
            content_h - 32.0,
            content_h
        ));
        let sy2 =
            crate::core::reactivity::use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32)
                .peek();
        assert!(
            (sy2 - expected).abs() < 0.5,
            "mid-expand must not recenter against the tall viewport, got {sy2}"
        );
    });
}

#[test]
fn pending_align_skips_expand_clip_when_content_fits_placed_cap() {
    use crate::utils::popup::seed_floating_place_max_h_for_test;

    let _ = create_root(|| {
        // ColorPicker format Select: 6 rows (~192px) fit in max_height 16rem.
        // DatePicker months overflow that cap, so they never hit this branch.
        let overlay_id = "cp-fmt";
        let rest = "cp-fmt:menu_scroll";
        cancel_scroll_item_into_view(rest);
        seed_floating_place_max_h_for_test(overlay_id, 256.0);
        request_scroll_item_into_view(rest, 5, 6, ScrollItemAlign::Center);
        let content_h = 6.0 * 32.0;
        let accordion_vh = 48.0;
        assert!(
            !apply_pending_scroll_item_into_view(rest, accordion_vh, content_h),
            "accordion clip is not a scroll viewport when the list fits the placed cap"
        );
        assert!(has_pending_scroll_item_align());
        let sy = crate::core::reactivity::use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32)
            .peek();
        assert!(
            sy.abs() < 0.5,
            "must not recenter HSLA against the expanding clip, got {sy}"
        );
    });
}

#[test]
fn pending_align_keeps_placed_cap_when_expand_viewport_is_a_sliver() {
    use crate::utils::popup::seed_floating_place_max_h_for_test;

    let _ = create_root(|| {
        let overlay_id = "cal-month-sliver";
        let rest = "cal-month-sliver:menu_scroll";
        cancel_scroll_item_into_view(rest);
        seed_floating_place_max_h_for_test(overlay_id, 200.0);
        request_scroll_item_into_view(rest, 11, 12, ScrollItemAlign::Center);
        let content_h = 12.0 * 32.0;
        let placed_vh = 200.0 - 8.0;
        // Expand pin-grandchildren: layout viewport is the accordion sliver.
        assert!(apply_pending_scroll_item_into_view(rest, 40.0, content_h));
        let expected = scroll_offset_to_center_item(11, 32.0, placed_vh, 12);
        let sy = crate::core::reactivity::use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32)
            .peek();
        assert!(
                (sy - expected).abs() < 0.5,
                "must keep the placed-cap offset, not recenter in the sliver, got {sy} expected {expected}"
            );
    });
}

#[test]
fn reset_scroll_item_into_view_keeps_offset() {
    let _ = create_root(|| {
        let rest = "cal-month:menu_scroll";
        cancel_scroll_item_into_view(rest);
        set_scroll_offset_with_behavior(rest, 0.0, 140.0, true);
        reset_scroll_item_into_view(rest);
        let sy = crate::core::reactivity::use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32)
            .peek();
        assert!(
            (sy - 140.0).abs() < 0.5,
            "close must not wipe the menu offset, got {sy}"
        );
    });
}

#[test]
fn pending_align_waits_while_viewport_matches_content() {
    let _ = create_root(|| {
        cancel_scroll_item_into_view("t-pin-align");
        request_scroll_item_into_view("t-pin-align", 8, 12, ScrollItemAlign::Center);
        let content_h = 12.0 * 32.0;
        assert!(!apply_pending_scroll_item_into_view(
            "t-pin-align",
            content_h,
            content_h
        ));
        assert!(has_pending_scroll_item_align());
        // Select rebuilds the open menu every frame; re-request must not
        // reset the clip wait or we never survive Expand pin.
        request_scroll_item_into_view("t-pin-align", 8, 12, ScrollItemAlign::Center);
        assert!(!apply_pending_scroll_item_into_view(
            "t-pin-align",
            content_h,
            content_h
        ));
        assert!(apply_pending_scroll_item_into_view(
            "t-pin-align",
            256.0,
            content_h
        ));
        assert!(!has_pending_scroll_item_align());
        let sy =
            crate::core::reactivity::use_signal(&scroll_offset_y_signal_key("t-pin-align"), || {
                0.0_f32
            })
            .peek();
        let expected = scroll_offset_to_center_item(8, 32.0, 256.0, 12);
        assert!(
            (sy - expected).abs() < 0.5,
            "offset after unpin must center, got {sy} expected {expected}"
        );
    });
}

#[test]
fn align_rect_centers_variable_height_child() {
    let viewport = [10.0, 20.0, 120.0, 200.0];
    let target = [10.0, 20.0 + 288.0, 120.0, 48.0];
    let max = [0.0, (12.0 * 32.0 + 48.0) - 200.0];
    let [_, sy] = super::scroll_offset_to_align_rect(
        viewport,
        target,
        [0.0, 0.0],
        max,
        ScrollItemAlign::Center,
    );
    let expected = (288.0_f32 + 24.0 - 100.0).clamp(0.0, max[1]);
    assert!(
        (sy - expected).abs() < 0.5,
        "variable-height child must center, got {sy} expected {expected}"
    );
}

#[test]
fn descendant_align_uses_layout_bounds() {
    use crate::core::scroll_viewport::focus_id_for_scroll_viewport;
    use crate::core::tree::Element;
    let _ = create_root(|| {
        let rest = "custom-list";
        cancel_scroll_item_into_view(rest);
        super::request_scroll_descendant_into_view(rest, "row-9", ScrollItemAlign::Center);
        let mut row = Element::new().template_id("row-9");
        row.layout_bounds = [0.0, 288.0, 100.0, 32.0];
        let mut vp = Element::new().focus_id(focus_id_for_scroll_viewport(rest));
        vp.layout_bounds = [0.0, 0.0, 100.0, 200.0];
        vp.children.push(row);
        let mut root = Element::new();
        root.children.push(vp);
        let found = super::resolve_pending_descendant_bounds(&root);
        let content_h = 12.0 * 32.0;
        assert!(super::apply_pending_scroll_align(
            rest,
            [0.0, 0.0, 100.0, 200.0],
            100.0,
            content_h,
            found.get(rest).copied(),
        ));
        let sy = crate::core::reactivity::use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32)
            .peek();
        let expected = super::scroll_offset_to_align_rect(
            [0.0, 0.0, 100.0, 200.0],
            [0.0, 288.0, 100.0, 32.0],
            [0.0, 0.0],
            [0.0, content_h - 200.0],
            ScrollItemAlign::Center,
        )[1];
        assert!(
            (sy - expected).abs() < 0.5,
            "descendant align must use layout box, got {sy} expected {expected}"
        );
    });
}

#[test]
fn descendant_alignment_is_stable_after_scrolling() {
    use crate::core::tree::Element;

    create_root(|| {
        let rest = "repeat-descendant-align";
        let mut row = Element::new().template_id("repeat-target");
        row.layout_bounds = [410.0, 420.0, 40.0, 32.0];
        let mut viewport = Element::new()
            .focus_id(crate::core::scroll_viewport::focus_id_for_scroll_viewport(
                rest,
            ))
            .child(row);
        viewport.layout_bounds = [10.0, 20.0, 120.0, 200.0];
        for _ in 0..3 {
            super::request_scroll_descendant_into_view(
                rest,
                "repeat-target",
                ScrollItemAlign::Center,
            );
            let found = super::resolve_pending_descendant_bounds(&viewport);
            assert!(super::apply_pending_scroll_align(
                rest,
                viewport.layout_bounds,
                1000.0,
                1000.0,
                found.get(rest).copied(),
            ));
            let offset = [
                crate::core::reactivity::use_signal(
                    &crate::core::scroll_viewport::scroll_offset_x_signal_key(rest),
                    || 0.0_f32,
                )
                .peek(),
                crate::core::reactivity::use_signal(
                    &crate::core::scroll_viewport::scroll_offset_y_signal_key(rest),
                    || 0.0_f32,
                )
                .peek(),
            ];
            assert_eq!(
                offset,
                [360.0, 316.0],
                "repeated alignment must not accumulate offsets"
            );
            viewport.scroll_offset = offset;
        }
    });
}

#[test]
fn after_smooth_writes_target_not_offset() {
    use crate::core::scroll_viewport::{
        scroll_behavior_signal_key, scroll_target_y_signal_key, SCROLL_BEHAVIOR_SMOOTH,
    };
    let _ = create_root(|| {
        let rest = "after-smooth";
        cancel_scroll_item_into_view(rest);
        crate::core::reactivity::use_signal(&scroll_behavior_signal_key(rest), || {
            SCROLL_BEHAVIOR_SMOOTH.to_string()
        })
        .set_silent(SCROLL_BEHAVIOR_SMOOTH.to_string());
        super::request_scroll_item_into_view_when(
            rest,
            8,
            12,
            ScrollItemAlign::Center,
            super::ScrollAlignWhen::After,
        );
        let content_h = 12.0 * 32.0;
        assert!(apply_pending_scroll_item_into_view(rest, 200.0, content_h));
        let sy = crate::core::reactivity::use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32)
            .peek();
        let ty = crate::core::reactivity::use_signal(&scroll_target_y_signal_key(rest), || 0.0_f32)
            .peek();
        let expected = scroll_offset_to_center_item(8, 32.0, 200.0, 12);
        assert!(
            sy.abs() < 0.5,
            "after+smooth must not snap offset, got {sy}"
        );
        assert!(
            (ty - expected).abs() < 0.5,
            "after+smooth must chase target, got {ty} expected {expected}"
        );
    });
}

#[cfg(feature = "product-tests")]
#[test]
fn public_scroll_alignment_uses_smooth_unless_jump_is_explicit() {
    use crate::core::scroll_viewport::ScrollViewportSignalKeys;
    let _ = create_root(|| {
        for (id, behavior) in [
            ("public-align-default", "smooth"),
            ("public-align-jump", "jump"),
        ] {
            let keys = ScrollViewportSignalKeys::new(id);
            if behavior == "jump" {
                crate::core::reactivity::use_signal(keys.behavior.as_str(), || "jump".to_string());
            }
            crate::components::scroll_item_into_view(id, 8, 12, "center");
            assert!(apply_pending_scroll_item_into_view(id, 200.0, 384.0));
            let target =
                crate::core::reactivity::use_signal(keys.target_y.as_str(), || 0.0_f32).peek();
            let offset =
                crate::core::reactivity::use_signal(keys.offset_y.as_str(), || 0.0_f32).peek();
            assert!(target > 0.0);
            assert_eq!(offset, if behavior == "smooth" { 0.0 } else { target });
        }
    });
}

#[test]
fn after_jump_snaps_offset() {
    use crate::core::scroll_viewport::{scroll_behavior_signal_key, SCROLL_BEHAVIOR_JUMP};
    let _ = create_root(|| {
        let rest = "after-jump";
        cancel_scroll_item_into_view(rest);
        crate::core::reactivity::use_signal(&scroll_behavior_signal_key(rest), || {
            SCROLL_BEHAVIOR_JUMP.to_string()
        })
        .set_silent(SCROLL_BEHAVIOR_JUMP.to_string());
        super::request_scroll_item_into_view_when(
            rest,
            8,
            12,
            ScrollItemAlign::Center,
            super::ScrollAlignWhen::After,
        );
        let content_h = 12.0 * 32.0;
        assert!(apply_pending_scroll_item_into_view(rest, 200.0, content_h));
        let sy = crate::core::reactivity::use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32)
            .peek();
        let expected = scroll_offset_to_center_item(8, 32.0, 200.0, 12);
        assert!(
            (sy - expected).abs() < 0.5,
            "after+jump must snap, got {sy} expected {expected}"
        );
    });
}

#[test]
fn after_waits_for_real_clip_not_placed_cap() {
    use crate::utils::popup::seed_floating_place_max_h_for_test;
    let _ = create_root(|| {
        let overlay_id = "after-pin";
        let rest = "after-pin:menu_scroll";
        cancel_scroll_item_into_view(rest);
        seed_floating_place_max_h_for_test(overlay_id, 200.0);
        super::request_scroll_item_into_view_when(
            rest,
            11,
            12,
            ScrollItemAlign::Center,
            super::ScrollAlignWhen::After,
        );
        let content_h = 12.0 * 32.0;
        assert!(
            !apply_pending_scroll_item_into_view(rest, content_h, content_h),
            "after must wait until expand unpins"
        );
        assert!(has_pending_scroll_item_align());
        assert!(apply_pending_scroll_item_into_view(rest, 200.0, content_h));
    });
}
