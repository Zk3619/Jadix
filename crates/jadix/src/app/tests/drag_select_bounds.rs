use super::*;
use taffy::style::Overflow;

#[test]
fn input_leaf_for_rest_returns_visual_bounds_under_scroll() {
    // Scroll parent moved content up by 100; leaf abs y=250 → visual y=150.
    let mut leaf = Element::new().w(300.0).h(67.0);
    leaf.layout_bounds = [10.0, 250.0, 300.0, 67.0];
    leaf.text = Some("的\n啊\n啊\n我".into());
    leaf.focus_id = Some("textarea:ta-scroll-sel".into());

    let mut viewport = Element::new().w(400.0).h(200.0);
    // Must be Scroll (not Hidden): `next_hit_scroll_acc` only accumulates
    // scroll offset for Scroll/scroll-viewport ids.
    viewport.style.overflow.y = Overflow::Scroll;
    viewport.scroll_offset = [0.0, 100.0];
    viewport.layout_bounds = [0.0, 0.0, 400.0, 200.0];
    viewport = viewport.child(leaf);

    let (bounds, text, _, _) =
        find_input_text_leaf_for_rest(&viewport, None, "ta-scroll-sel").expect("leaf");
    assert_eq!(text.lines().count(), 4);
    assert!(
        (bounds[1] - 150.0).abs() < 0.5,
        "drag-select bounds must be visual (abs 250 − scroll 100), got y={}",
        bounds[1]
    );
    assert!(
        (bounds[0] - 10.0).abs() < 0.5 && (bounds[2] - 300.0).abs() < 0.5,
        "bounds={bounds:?}"
    );
}

#[test]
fn clamp_pointer_to_textarea_scrollport_pins_outside_hits_to_clip_edge() {
    use crate::core::scroll_viewport::focus_id_for_scroll_viewport;
    use crate::utils::text_field::textarea_scroll_rest;

    let id = "ta-clamp-hit";
    let rest = textarea_scroll_rest(id);
    let vp_fid = focus_id_for_scroll_viewport(&rest);

    let mut leaf = Element::new();
    leaf.layout_bounds = [20.0, 40.0, 800.0, 40.0];
    leaf.text = Some("padding: 8px 16px; background: #ffffff; border-radius: 6px".into());
    leaf.text_style.white_space = crate::style::WhiteSpace::Nowrap;
    leaf.focus_id = Some(format!("textarea:{id}"));

    let mut viewport = Element::new();
    viewport.focus_id = Some(vp_fid);
    viewport.style.overflow.x = Overflow::Scroll;
    viewport.style.overflow.y = Overflow::Scroll;
    viewport.layout_bounds = [20.0, 40.0, 200.0, 120.0];
    viewport.scroll_offset = [0.0, 0.0];
    viewport = viewport.child(leaf);

    let (cx, cy) = clamp_pointer_to_textarea_scrollport(&viewport, id, 500.0, 80.0);
    assert!(
        (cx - 220.0).abs() < 0.5,
        "outside-right pointer must clamp to clip right edge, got {cx}"
    );
    assert!((cy - 80.0).abs() < 0.5);

    let (cx2, _) = clamp_pointer_to_textarea_scrollport(&viewport, id, 0.0, 80.0);
    assert!(
        (cx2 - 20.0).abs() < 0.5,
        "outside-left pointer must clamp to clip left edge, got {cx2}"
    );
}

#[test]
fn nudge_hscroll_uses_inner_edge_hot_zone_and_leaf_width() {
    use crate::core::reactivity::{create_root, use_signal};
    use crate::core::scroll_viewport::{
        focus_id_for_scroll_viewport, scroll_content_w_signal_key, scroll_offset_x_signal_key,
        scroll_offset_y_signal_key, scroll_viewport_w_signal_key,
    };
    use crate::utils::text_field::textarea_scroll_rest;

    create_root(|| {
        let id = "ta-hscroll-edge";
        let rest = textarea_scroll_rest(id);
        let vp_fid = focus_id_for_scroll_viewport(&rest);

        use_signal(&scroll_offset_x_signal_key(&rest), || 0.0_f32).set_silent(0.0);
        use_signal(&scroll_offset_y_signal_key(&rest), || 0.0_f32).set_silent(0.0);
        use_signal(&scroll_viewport_w_signal_key(&rest), || 0.0_f32).set_silent(0.0);
        // Stale/zero content-w: nudge must fall back to leaf layout width.
        use_signal(&scroll_content_w_signal_key(&rest), || 0.0_f32).set_silent(0.0);

        let mut leaf = Element::new();
        leaf.layout_bounds = [0.0, 0.0, 600.0, 40.0];
        leaf.text = Some("x".repeat(80));
        leaf.text_style.white_space = crate::style::WhiteSpace::Nowrap;
        leaf.text_style.size = Some(14.0);
        leaf.text_style.line_height_px = Some(21.0);
        leaf.focus_id = Some(format!("textarea:{id}"));

        let mut viewport = Element::new();
        viewport.focus_id = Some(vp_fid);
        viewport.style.overflow.x = Overflow::Scroll;
        viewport.style.overflow.y = Overflow::Scroll;
        viewport.layout_bounds = [0.0, 0.0, 200.0, 80.0];
        viewport.scroll_offset = [0.0, 0.0];
        viewport = viewport.child(leaf);

        // Pointer inside clip but within the thin inner belt of the right edge.
        let tick = nudge_textarea_hscroll_for_drag(&mut viewport, id, 199.0, 20.0, 1.0 / 60.0);
        assert_eq!(tick, TextDragHScrollTick::Scrolled);
        let sx = use_signal(&scroll_offset_x_signal_key(&rest), || 0.0_f32).peek();
        assert!(sx > 0.05, "inner edge hot-zone must advance sx, got {sx}");
    });
}

#[test]
fn input_drag_select_hit_space_uses_leaf_visual_bounds_for_nowrap() {
    use crate::core::reactivity::{create_root, use_signal};
    use crate::core::scroll_viewport::{focus_id_for_scroll_viewport, scroll_offset_x_signal_key};
    use crate::style::TextOverflow;
    use crate::utils::text_field::textarea_scroll_rest;

    create_root(|| {
        let id = "ta-drag-hit-sx";
        let rest = textarea_scroll_rest(id);
        let vp_fid = focus_id_for_scroll_viewport(&rest);

        use_signal(&scroll_offset_x_signal_key(&rest), || 0.0_f32).set_silent(120.0);

        let mut leaf = Element::new();
        leaf.layout_bounds = [32.0, 68.0, 800.0, 40.0];
        leaf.text = Some("padding: 8px 16px; background: #ffffff; border-radius: 6px".into());
        leaf.text_style.white_space = crate::style::WhiteSpace::Nowrap;
        leaf.text_style.overflow = TextOverflow::Clip;
        leaf.focus_id = Some(format!("textarea:{id}"));

        let mut viewport = Element::new();
        viewport.focus_id = Some(vp_fid);
        viewport.style.overflow.x = Overflow::Scroll;
        viewport.style.overflow.y = Overflow::Scroll;
        viewport.layout_bounds = [20.0, 40.0, 200.0, 120.0];
        // Tree scroll still zero — signal owns sx during drag edge auto-scroll.
        viewport.scroll_offset = [0.0, 0.0];
        // Leaf visual x = layout_x − sx (padding inset from clip left).
        let leaf_visual = [-88.0, 68.0, 800.0, 40.0];
        let style = leaf.text_style.as_ref().clone();
        viewport = viewport.child(leaf);
        let (mx, _, hit_bounds, hit_style) = input_drag_select_hit_space(
            &viewport,
            id,
            500.0,
            80.0,
            leaf_visual,
            &style,
            [0.0, 0.0],
        );

        assert_eq!(hit_bounds, leaf_visual);
        assert_eq!(hit_style.scroll_x, 0.0);
        assert!(
            (mx - 220.0).abs() < 0.5,
            "pointer must clamp to clip right, got {mx}"
        );
        let content_x = mx - hit_bounds[0];
        assert!(
            (content_x - 308.0).abs() < 0.5,
            "content x must match panned leaf origin at clip edge, got {content_x}"
        );
    });
}

#[test]
fn text_drag_edge_speed_grows_with_overshoot() {
    let char_w = 8.0;
    let remaining = 400.0;
    let dt = 1.0 / 60.0;
    let near = text_drag_edge_scroll_step(4.0, char_w, remaining, dt);
    let mid = text_drag_edge_scroll_step(40.0, char_w, remaining, dt);
    let jump = text_drag_edge_scroll_step(200.0, char_w, remaining, dt);
    assert!(
        near > 0.2 && near <= char_w,
        "near-edge must crawl ≤1 glyph/frame, got {near}"
    );
    assert!(
        mid > near * 1.5,
        "mid overshoot must be clearly faster than near: near={near} mid={mid}"
    );
    assert!(
        (jump - remaining).abs() < 0.01,
        "far overshoot must jump remaining, got {jump}"
    );
    assert_eq!(
        text_drag_edge_scroll_step(4.0, char_w, remaining, 0.0),
        0.0,
        "zero dt must not crawl"
    );
}

#[test]
fn input_hscroll_nudge_is_slower_near_edge_than_far() {
    use crate::core::reactivity::{create_root, use_signal};
    use crate::style::TextOverflow;
    use crate::utils::text_field::{input_scroll_x_signal_key, input_text_template_id};

    create_root(|| {
        let mut fs = crate::renderer::font_loader::create_font_system();
        crate::renderer::font_loader::apply_default_families(fs.db_mut());
        crate::text::install_shared_font_system(&mut fs);

        let id = "in-hscroll-edge";
        use_signal(&input_scroll_x_signal_key(id), || 0.0_f32).set_silent(0.0);

        let mut leaf = Element::new();
        leaf.layout_bounds = [10.0, 10.0, 120.0, 24.0];
        leaf.text = Some("abcdefghijklmnopqrstuvwxyz0123456789".repeat(6));
        leaf.text_style.overflow = TextOverflow::Hidden;
        leaf.text_style.size = Some(14.0);
        leaf.text_style.line_height_px = Some(21.0);
        leaf.focus_id = Some(format!("input:{id}"));
        leaf.template_id = Some(input_text_template_id(id));

        let tick_near = nudge_input_hscroll_for_drag(&mut leaf, id, 10.0 + 120.0 + 2.0, 1.0 / 60.0);
        assert_eq!(tick_near, TextDragHScrollTick::Scrolled);
        let sx_near = use_signal(&input_scroll_x_signal_key(id), || 0.0_f32).peek();
        assert!(sx_near > 0.05, "near-edge must advance, got {sx_near}");
        assert!(
            sx_near <= 14.0,
            "near-edge must crawl (~1 glyph/frame), got {sx_near}"
        );

        use_signal(&input_scroll_x_signal_key(id), || 0.0_f32).set_silent(0.0);
        leaf.text_style.scroll_x = 0.0;
        let tick_far =
            nudge_input_hscroll_for_drag(&mut leaf, id, 10.0 + 120.0 + 200.0, 1.0 / 60.0);
        assert_eq!(tick_far, TextDragHScrollTick::Scrolled);
        let sx_far = use_signal(&input_scroll_x_signal_key(id), || 0.0_f32).peek();
        assert!(
            sx_far > sx_near + 40.0,
            "far from the clip must jump toward the line end: near={sx_near} far={sx_far}"
        );
    });
}

#[test]
fn input_drag_select_hit_space_clamps_to_leaf_clip() {
    use crate::style::TextOverflow;

    let mut leaf = Element::new();
    leaf.layout_bounds = [20.0, 10.0, 100.0, 24.0];
    leaf.text = Some("overflowing single line text".into());
    leaf.text_style.overflow = TextOverflow::Hidden;
    leaf.focus_id = Some("input:in-hit-clamp".into());
    let style = leaf.text_style.as_ref().clone();
    let (mx, _, _, _) = input_drag_select_hit_space(
        &leaf,
        "in-hit-clamp",
        500.0,
        20.0,
        [20.0, 10.0, 100.0, 24.0],
        &style,
        [0.0, 0.0],
    );
    assert!(
        (mx - 120.0).abs() < 0.5,
        "pointer past the clip must stay on the visible right edge, got {mx}"
    );
}
