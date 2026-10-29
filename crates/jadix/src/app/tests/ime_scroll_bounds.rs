use super::*;
use taffy::style::Overflow;

#[test]
fn input_display_text_splices_preedit_at_anchor() {
    assert_eq!(input_display_text("ab", "X", Some(1)), "aXb");
    assert_eq!(input_display_text("ab", "", Some(1)), "ab");
}

#[test]
fn find_bounds_by_focus_id_accumulates_scroll_for_ime_visual() {
    // Same space as window_events IME path: layout_bounds − scroll_acc.
    let mut input = Element::new().w(200.0).h(32.0);
    input.layout_bounds = [20.0, 300.0, 200.0, 32.0];
    input.focus_id = Some("input:gal-s2-input-a".into());

    let mut viewport = Element::new().w(400.0).h(200.0);
    viewport.style.overflow.y = Overflow::Scroll;
    viewport.scroll_offset = [0.0, 120.0];
    viewport.layout_bounds = [0.0, 0.0, 400.0, 200.0];
    viewport = viewport.child(input);

    let (bounds, scroll_acc) =
        find_bounds_by_focus_id(&viewport, "input:gal-s2-input-a", [0.0, 0.0])
            .expect("input focus");
    let visual_y = bounds[1] - scroll_acc[1];
    assert!(
        (scroll_acc[1] - 120.0).abs() < 0.5,
        "scroll_acc={scroll_acc:?}"
    );
    assert!(
        (visual_y - 180.0).abs() < 0.5,
        "IME visual y must track scroll (300−120), got {visual_y}"
    );

    viewport.scroll_offset = [0.0, 200.0];
    let (bounds2, sa2) =
        find_bounds_by_focus_id(&viewport, "input:gal-s2-input-a", [0.0, 0.0]).unwrap();
    assert!(((bounds2[1] - sa2[1]) - 100.0).abs() < 0.5);
}

#[test]
fn find_paint_aabb_by_focus_id_tracks_scroll_and_rotate() {
    let mut input = Element::new().w(200.0).h(32.0);
    input.layout_bounds = [20.0, 300.0, 200.0, 32.0];
    input.focus_id = Some("input:gal-s2-input-a".into());

    let mut viewport = Element::new().w(400.0).h(200.0);
    viewport.style.overflow.y = Overflow::Scroll;
    viewport.scroll_offset = [0.0, 120.0];
    viewport.layout_bounds = [0.0, 0.0, 400.0, 200.0];
    viewport = viewport.child(input);

    let (aabb, sa) = find_paint_aabb_by_focus_id(&viewport, "input:gal-s2-input-a", [0.0, 0.0])
        .expect("input focus");
    assert!((sa[1] - 120.0).abs() < 0.5, "sa={sa:?}");
    assert!(
        (aabb[1] - 180.0).abs() < 0.5,
        "paint AABB y is already visual, got {:?}",
        aabb
    );

    let mut rot = Element::new().on_click(|| {});
    rot.layout_bounds = [0.0, 0.0, 100.0, 20.0];
    rot.focus_id = Some("input:rot-ime".into());
    rot.style.rotate_deg = 90.0;
    let paint = crate::core::tree::visual_paint_aabb(&rot, [0.0, 0.0]);
    let (aabb, _) =
        find_paint_aabb_by_focus_id(&rot, "input:rot-ime", [0.0, 0.0]).expect("rotated");
    assert!(
        (aabb[0] - paint[0]).abs() < 0.5 && (aabb[2] - paint[2]).abs() < 0.5,
        "IME fallback must follow paint AABB, got {aabb:?} vs {paint:?}"
    );
    let world = crate::core::tree::layout_local_to_paint_world(
        &rot,
        [90.0, 10.0],
        [0.0, 0.0],
        crate::style::MAT2D_IDENTITY,
        [1.0, 1.0],
    );
    assert!(pointer_hits_focus_paint(
        &rot,
        "input:rot-ime",
        world[0],
        world[1],
        [0.0, 0.0]
    ));
    assert!(!pointer_hits_focus_paint(
        &rot,
        "input:rot-ime",
        90.0,
        10.0,
        [0.0, 0.0]
    ));
    assert_eq!(
        find_focus_id_at(&rot, world[0], world[1], [0.0, 0.0]).as_deref(),
        Some("input:rot-ime")
    );
    assert!(find_focus_id_at(&rot, 90.0, 10.0, [0.0, 0.0]).is_none());

    let caret = find_paint_ime_caret(&rot, "input:rot-ime", [80.0, 10.0], [0.0, 0.0])
        .expect("rotated caret");
    let mapped = crate::core::tree::layout_local_to_paint_world(
        &rot,
        [80.0, 10.0],
        [0.0, 0.0],
        crate::style::MAT2D_IDENTITY,
        [1.0, 1.0],
    );
    assert!(
        (caret[0] - mapped[0]).abs() < 0.5 && (caret[1] - mapped[1]).abs() < 0.5,
        "IME window must sit on the paint caret, got {caret:?} vs {mapped:?}"
    );
    assert!(
        (caret[0] - (paint[0] + 80.0)).abs() > 5.0,
        "IME must not use paint-AABB left + caret_x ({:.1}+80)",
        paint[0]
    );
}

#[test]
fn find_paint_aabb_fixed_sv_ignores_ancestor_scroll() {
    let mut sv = Element::new().w(100.0).h(80.0).fixed();
    sv.layout_bounds = [20.0, 120.0, 100.0, 80.0];
    sv.focus_id = Some("cp-fixed:sv".into());

    let mut viewport = Element::new().w(280.0).h(200.0);
    viewport.style.overflow.y = Overflow::Scroll;
    viewport.scroll_offset = [0.0, 240.0];
    viewport.layout_bounds = [0.0, 0.0, 280.0, 200.0];
    viewport = viewport.child(sv);

    let (aabb, sa) =
        find_paint_aabb_by_focus_id(&viewport, "cp-fixed:sv", [0.0, 0.0]).expect("fixed sv");
    assert!(
        sa[0].abs() < 0.5 && sa[1].abs() < 0.5,
        "position:fixed must drop page scroll_acc, got {sa:?}"
    );
    assert!(
        (aabb[1] - 120.0).abs() < 0.5,
        "fixed SV window y must not subtract page offset (would glue V→0); got {aabb:?}"
    );
}

#[test]
fn find_bounds_prefers_scrolled_duplicate_focus_id() {
    use crate::core::reactivity::use_signal;
    use crate::core::scroll_viewport::scroll_offset_y_signal_key;

    let mut ghost = Element::new();
    ghost.focus_id = Some("input:dup".into());
    ghost.layout_bounds = [20.0, 400.0, 200.0, 32.0];

    let mut real = Element::new();
    real.focus_id = Some("input:dup".into());
    real.layout_bounds = [20.0, 400.0, 200.0, 32.0];

    let mut viewport = Element::new();
    viewport.focus_id = Some("scroll:root-scroll".into());
    viewport.style.overflow.y = Overflow::Visible; // overflow wrong, but scroll: still counts
    viewport.scroll_offset = [0.0, 220.0];
    use_signal(&scroll_offset_y_signal_key("root-scroll"), || 0.0_f32).set_silent(220.0);
    viewport = viewport.child(real);

    let root = Element::new().child(ghost).child(viewport);
    let (bounds, sa) = find_bounds_by_focus_id(&root, "input:dup", [0.0, 0.0]).unwrap();
    assert!(
        (sa[1] - 220.0).abs() < 0.5,
        "must prefer scrolled match, sa={sa:?}"
    );
    assert!(((bounds[1] - sa[1]) - 180.0).abs() < 0.5);
}
