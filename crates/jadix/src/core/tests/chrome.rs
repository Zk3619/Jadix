use super::*;

#[test]
fn transparent_track_does_not_fade_with_thumb() {
    let mut track = [0.0, 0.0, 0.0, 0.0];
    let mut track_border = [0.0, 0.0, 0.0, 0.0];
    let mut thumb = [0.2, 0.2, 0.2, 1.0];
    let mut thumb_border = [0.0, 0.0, 0.0, 0.0];
    normalize_scrollbar_base_for_auto(
        "auto",
        false,
        0.0,
        0.0,
        &mut track,
        &mut track_border,
        &mut thumb,
        &mut thumb_border,
    );
    apply_scrollbar_visible_alpha(
        0.5,
        false,
        false,
        &mut track,
        &mut thumb,
        &mut track_border,
        &mut thumb_border,
    );
    assert_eq!(track[3], 0.0);
    assert!((thumb[3] - 0.5).abs() < 1e-4);
}

#[test]
fn opaque_track_and_thumb_both_fade() {
    let mut track = [0.9, 0.9, 0.9, 1.0];
    let mut track_border = [0.0, 0.0, 0.0, 0.0];
    let mut thumb = [0.2, 0.2, 0.2, 1.0];
    let mut thumb_border = [0.0, 0.0, 0.0, 0.0];
    apply_scrollbar_visible_alpha(
        0.5,
        true,
        false,
        &mut track,
        &mut thumb,
        &mut track_border,
        &mut thumb_border,
    );
    assert!((track[3] - 0.5).abs() < 1e-4);
    assert!((thumb[3] - 0.5).abs() < 1e-4);
}

#[test]
fn auto_bar_paint_alpha_reads_live_cubic_not_style() {
    use crate::core::reactivity::create_root;
    use crate::style::{tick_animations, use_ui_transition_cubic};

    let (_owner, _) = create_root(|| {
        let mut el = Element::new();
        el.focus_id = Some("scrollbar-thumb:fade-seam:y".into());
        use_signal(&scrollbar_visibility_signal_key("fade-seam", "y"), || {
            "auto".to_string()
        })
        .set("auto".to_string());
        let tw = use_ui_transition_cubic("scrollbar_spring_fade-seam:y", 0.0);
        tw.set(1.0);
        tick_animations(0.08);
        tick_animations(0.08);
        let shown = auto_scrollbar_live_paint_alpha(&el).expect("auto thumb");
        assert!(shown > 0.99, "settled show cubic must be ~1, got {shown}");
        tw.set(0.0);
        tick_animations(0.08);
        tick_animations(0.08);
        let hidden = auto_scrollbar_live_paint_alpha(&el).expect("auto thumb");
        assert!(hidden < 0.01, "settled hide cubic must be ~0, got {hidden}");
        let mut zeroed = [0.2, 0.2, 0.2, 0.0];
        zeroed = auto_scrollbar_restore_base_alpha(&el, zeroed);
        assert!(
            (zeroed[3] - 1.0).abs() < 1e-4,
            "collect must restore build-time auto zeroing, got {}",
            zeroed[3]
        );
    });
}

#[test]
fn dynamically_hidden_scrollbars_suppress_track_and_thumb_paint() {
    crate::core::reactivity::create_root(|| {
        let visibility = use_signal(&scrollbar_visibility_signal_key("hidden-bar", "y"), || {
            "always".to_string()
        });
        let mut track = Element::new().focus_id("scrollbar-track:hidden-bar:y");
        track.set_attr(ATTR_SCROLLBAR_TRACK_FADES, "0");
        let thumb = Element::new().focus_id("scrollbar-thumb:hidden-bar:y");
        assert_eq!(auto_scrollbar_live_paint_alpha(&track), None);
        assert_eq!(auto_scrollbar_live_paint_alpha(&thumb), None);
        visibility.set("never".to_string());
        assert_eq!(auto_scrollbar_live_paint_alpha(&track), Some(0.0));
        assert_eq!(auto_scrollbar_live_paint_alpha(&thumb), Some(0.0));
        assert!(auto_scrollbar_needs_node_present(&track));
        assert!(auto_scrollbar_needs_node_present(&thumb));
        visibility.set("always".to_string());
        assert_eq!(auto_scrollbar_live_paint_alpha(&track), None);
        assert_eq!(auto_scrollbar_live_paint_alpha(&thumb), None);
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn auto_hide_must_retarget_cubic_before_tick_or_collect_stays_opaque() {
    use crate::core::reactivity::create_root;
    use crate::style::{any_ui_animation_active, tick_animations, use_ui_transition_cubic};

    let (_owner, _) = create_root(|| {
        let tw = use_ui_transition_cubic("scrollbar_spring_hide-order:y", 0.0);
        tw.set(1.0);
        tick_animations(0.08);
        tick_animations(0.08);
        assert!(tw.peek() > 0.99);

        // Old present-then-set order: tick/collect still see cubic 1.
        let _ = tick_animations(0.016);
        let collect_after_tick = tw.peek();
        tw.set(0.0);
        assert!(
            collect_after_tick > 0.99,
            "post-present set cannot change this frame's overlay collect, got {collect_after_tick}"
        );
        assert!(
            any_ui_animation_active(),
            "hide tween is active after set; paint_clock must pump it without force_redraw_count"
        );

        tw.set(1.0);
        tick_animations(0.08);
        tick_animations(0.08);
        tw.set(0.0);
        let _ = tick_animations(0.016);
        let collect_after_retarget = tw.peek();
        assert!(
                collect_after_retarget < 0.99,
                "paint-only hide must tick after retarget so collect leaves 1, got {collect_after_retarget}"
            );
    });
}
