use super::*;
use crate::core::tree::Element;

#[test]
fn rapid_clicks_activate_once_per_release_without_double_click_bubbling() {
    let activations = Rc::new(Cell::new(0));
    let mut control = Element::new().on_click({
        let activations = activations.clone();
        move || activations.set(activations.get() + 1)
    });
    control.layout_bounds = [0.0, 0.0, 24.0, 24.0];
    let mut indicator = Element::new();
    indicator.layout_bounds = [4.0, 4.0, 16.0, 16.0];
    control.children.push(indicator);
    let path = hit_path(&control, 8.0, 8.0);
    for detail in 1..=6 {
        let mut release = PointerEvent::new(8.0, 8.0, MouseButton::Left, detail);
        dispatch_pointer(&path, &mut release, PointerKind::Up);
        if detail >= 2 {
            let mut double = PointerEvent::new(8.0, 8.0, MouseButton::Left, detail);
            dispatch_pointer(&path, &mut double, PointerKind::DoubleClick);
        }
        assert_eq!(activations.get(), detail - 1);
        let (callback, _) = resolve_click_from_path(&control, &path);
        (*callback.unwrap().borrow_mut())();
        assert_eq!(activations.get(), detail);
    }
}

#[test]
fn hit_path_orders_root_to_leaf() {
    let mut leaf = Element::new();
    leaf.layout_bounds = [10.0, 10.0, 20.0, 20.0];
    leaf.focus_id = Some("leaf".into());
    leaf = leaf.on_click(|| {});

    let mut root = Element::new();
    root.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    root.focus_id = Some("root".into());
    root.children.push(leaf);

    let path = hit_path(&root, 15.0, 15.0);
    assert!(path.len() >= 2);
    assert_eq!(path[0].focus_id.as_deref(), Some("root"));
    assert_eq!(path.last().unwrap().focus_id.as_deref(), Some("leaf"));
}

#[test]
fn hit_path_identity_eq_same_padding_slide() {
    let mut root = Element::new();
    root.layout_bounds = [0.0, 0.0, 400.0, 400.0];
    root.focus_id = Some("page".into());
    let a = hit_path(&root, 20.0, 20.0);
    let b = hit_path(&root, 80.0, 90.0);
    assert!(
        hit_path_identity_eq(&a, &b),
        "sliding inside one box must be the same hit chain"
    );
    let mut btn = Element::new();
    btn.layout_bounds = [200.0, 200.0, 40.0, 24.0];
    btn.focus_id = Some("btn".into());
    root.children.push(btn);
    let pad = hit_path(&root, 20.0, 20.0);
    let on_btn = hit_path(&root, 210.0, 210.0);
    assert!(
        !hit_path_identity_eq(&pad, &on_btn),
        "entering a widget must change the hit chain"
    );
}

#[test]
fn class_hover_path_may_change_samples_ignores_breakpoint_boxes() {
    let mut a = Element::new();
    crate::style::apply_class_list(&mut a, "md:flex dark:bg-accent");
    a.layout_bounds = [0.0, 0.0, 80.0, 80.0];
    a.focus_id = Some("a".into());
    let mut b = Element::new();
    crate::style::apply_class_list(&mut b, "sm:p-4");
    b.layout_bounds = [80.0, 0.0, 80.0, 80.0];
    b.focus_id = Some("b".into());
    let mut hover = Element::new();
    crate::style::apply_class_list(&mut hover, "hover:scale-125");
    hover.layout_bounds = [160.0, 0.0, 80.0, 80.0];
    hover.focus_id = Some("hover".into());
    let mut root = Element::new();
    root.layout_bounds = [0.0, 0.0, 240.0, 80.0];
    root.children.push(a);
    root.children.push(b);
    root.children.push(hover);

    let on_a = hit_path(&root, 40.0, 40.0);
    let on_b = hit_path(&root, 120.0, 40.0);
    let on_hover = hit_path(&root, 200.0, 40.0);
    assert!(
        on_a.last()
            .is_some_and(|e| !e.class_pointer_hover && !e.class_group_or_peer),
        "md:/dark: must not stamp pointer-hover on the hit path"
    );
    assert!(
        !class_hover_path_may_change_samples(&on_a, &on_b),
        "sliding across breakpoint-only boxes must not schedule a class-hover walk"
    );
    assert!(
        class_hover_path_may_change_samples(&on_b, &on_hover),
        "entering hover:scale must still schedule the class-hover walk"
    );
}

#[test]
fn hit_path_resolves_cursor_focus_and_scroll_without_a_second_walk() {
    let mut page = Element::new();
    page.layout_bounds = [0.0, 0.0, 400.0, 300.0];
    page.focus_id = Some("page".into());
    page.cursor = Some(crate::style::CursorValue::keyword(
        crate::style::CursorKind::Default,
    ));

    let mut scroller = Element::new();
    scroller.layout_bounds = [0.0, 0.0, 400.0, 300.0];
    scroller.focus_id = Some("scroll:main".into());

    let mut tile = Element::new();
    tile.layout_bounds = [20.0, 40.0, 80.0, 40.0];
    tile.focus_id = Some("tile".into());
    tile.cursor = Some(crate::style::CursorValue::keyword(
        crate::style::CursorKind::Pointer,
    ));
    tile = tile.on_hover(|_inside| {});

    scroller.children.push(tile);
    page.children.push(scroller);

    let on_tile = hit_path(&page, 40.0, 50.0);
    assert_eq!(path_deepest_focus_id(&on_tile), Some("tile"));
    assert_eq!(path_deepest_scroll_area_rest(&on_tile), Some("main"));
    assert_eq!(
        path_resolved_cursor(&on_tile).map(|c| c.kind),
        Some(crate::style::CursorKind::Pointer)
    );
    assert!(path_has_on_hover(&on_tile));

    let on_pad = hit_path(&page, 200.0, 200.0);
    assert_eq!(path_deepest_focus_id(&on_pad), Some("scroll:main"));
    assert_eq!(path_deepest_scroll_area_rest(&on_pad), Some("main"));
    assert_eq!(
        path_resolved_cursor(&on_pad).map(|c| c.kind),
        Some(crate::style::CursorKind::Default)
    );
    assert!(
        !path_has_on_hover(&on_pad),
        "padding inside the scroller has no on_hover; CursorMoved must not walk update_hovers"
    );
}

#[test]
fn stop_propagation_blocks_bubble() {
    let fired = Rc::new(Cell::new(0u8));
    let f1 = fired.clone();
    let f2 = fired.clone();

    let mut leaf = Element::new();
    leaf.layout_bounds = [0.0, 0.0, 10.0, 10.0];
    leaf.on_mousedown = Some(Rc::new(RefCell::new(move |e: &PointerEvent| {
        f1.set(f1.get() | 1);
        e.stop_propagation();
    })));

    let mut root = Element::new();
    root.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    root.on_mousedown = Some(Rc::new(RefCell::new(move |_e: &PointerEvent| {
        f2.set(f2.get() | 2);
    })));
    root.children.push(leaf);

    let path = hit_path(&root, 5.0, 5.0);
    let mut ev = PointerEvent::new(5.0, 5.0, MouseButton::Left, 1);
    // Capture runs root first — root sets bit 2, then leaf sets bit 1 and stops
    dispatch_pointer(&path, &mut ev, PointerKind::Down);
    assert_eq!(fired.get() & 1, 1);
    // Root capture still runs before leaf target; bubble must not re-fire root's down
    // (we only fire on_mousedown once per phase walk). Capture already ran root.
    assert!(ev.is_consumed());
}

#[test]
fn stop_propagation_on_child_skips_parent_bubble_when_parent_ignores_capture() {
    let parent_bubble = Rc::new(Cell::new(0u32));
    let p = parent_bubble.clone();

    let mut leaf = Element::new();
    leaf.layout_bounds = [2.0, 2.0, 8.0, 8.0];
    leaf.focus_id = Some("child".into());
    leaf.on_mousedown = Some(Rc::new(RefCell::new(move |e: &PointerEvent| {
        e.stop_propagation();
    })));

    let mut root = Element::new();
    root.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    root.focus_id = Some("parent".into());
    root.on_mousedown = Some(Rc::new(RefCell::new(move |e: &PointerEvent| {
        if e.phase == EventPhase::Capture {
            return;
        }
        p.set(p.get() + 1);
    })));
    root.children.push(leaf);

    let path = hit_path(&root, 5.0, 5.0);
    assert!(path.iter().any(|e| e.focus_id.as_deref() == Some("child")));
    assert!(path.iter().any(|e| e.focus_id.as_deref() == Some("parent")));
    let mut ev = PointerEvent::new(5.0, 5.0, MouseButton::Left, 1);
    dispatch_pointer(&path, &mut ev, PointerKind::Down);
    assert_eq!(
        parent_bubble.get(),
        0,
        "child stopPropagation must not let a bubble-only parent handler run"
    );
}

#[test]
fn hit_path_bounds_are_visual_under_scroll() {
    let mut leaf = Element::new();
    leaf.layout_bounds = [0.0, 200.0, 80.0, 80.0];
    leaf.focus_id = Some("plot".into());
    leaf.on_mousedown = Some(Rc::new(RefCell::new(|_e: &PointerEvent| {})));

    // Scroll contribution only applies for Scroll/Auto (not Hidden/Clip).
    let mut viewport = Element::new().overflow("scroll");
    viewport.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    viewport.scroll_offset = [0.0, 180.0];
    viewport.children.push(leaf);

    // Visual leaf: y = 200 - 180 = 20.
    let path = hit_path(&viewport, 40.0, 40.0);
    let leaf_e = path
        .iter()
        .find(|e| e.focus_id.as_deref() == Some("plot"))
        .expect("scrolled leaf on path");
    assert!(
        (leaf_e.bounds[1] - 20.0).abs() < 0.5,
        "expected visual y=20, got {:?}",
        leaf_e.bounds
    );
    assert!((leaf_e.bounds[0] - 0.0).abs() < 0.5);
    assert!((leaf_e.bounds[2] - 80.0).abs() < 0.5);
}

#[test]
fn hit_path_bounds_follow_paint_aabb_after_rotate_90() {
    let mut leaf = Element::new().on_click(|| {});
    leaf.layout_bounds = [0.0, 0.0, 100.0, 20.0];
    leaf.focus_id = Some("rot".into());
    leaf.style.rotate_deg = 90.0;
    let paint = crate::core::tree::visual_paint_aabb(&leaf, [0.0, 0.0]);
    let world = crate::core::tree::layout_local_to_paint_world(
        &leaf,
        [90.0, 10.0],
        [0.0, 0.0],
        crate::style::MAT2D_IDENTITY,
        [1.0, 1.0],
    );
    let path = hit_path(&leaf, world[0], world[1]);
    let e = path
        .iter()
        .find(|e| e.focus_id.as_deref() == Some("rot"))
        .expect("painted rotate leaf on path");
    assert!(
        (e.bounds[0] - paint[0]).abs() < 0.5
            && (e.bounds[1] - paint[1]).abs() < 0.5
            && (e.bounds[2] - paint[2]).abs() < 0.5
            && (e.bounds[3] - paint[3]).abs() < 0.5,
        "hit_path bounds must be paint AABB, got {:?} vs {paint:?}",
        e.bounds
    );
    assert!(
        hit_path(&leaf, 90.0, 10.0)
            .iter()
            .all(|e| e.focus_id.as_deref() != Some("rot")),
        "old layout-box point must miss the rotated leaf"
    );
}

#[test]
fn hit_path_clip_path_rotate_90_misses_layout_center() {
    let mut el = Element::new().on_click(|| {});
    el.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    el.focus_id = Some("clip".into());
    el.style.rotate_deg = 90.0;
    el.style.clip_path = crate::style::parse_clip_path("circle(25% at 75% 50%)").unwrap();
    assert!(
        hit_path(&el, 50.0, 75.0)
            .iter()
            .any(|e| e.focus_id.as_deref() == Some("clip")),
        "painted clip-path ink must be on the hit path"
    );
    assert!(
        hit_path(&el, 75.0, 50.0)
            .iter()
            .all(|e| e.focus_id.as_deref() != Some("clip")),
        "old layout-space clip circle must miss after 90°"
    );
}

#[test]
fn label_for_resolves_compound_checkbox_control_not_row() {
    use std::cell::Cell;

    let toggled = Rc::new(Cell::new(0u32));
    let t = toggled.clone();

    // Mirrors `<Checkbox>"caption"</Checkbox>`: outer row shares template_id with control focus_id.
    let mut control = Element::new();
    control.focus_id = Some("terms".into());
    control.template_id = Some("terms".into());
    control = control.on_click(move || {
        t.set(t.get() + 1);
    });

    let mut caption = Element::new().text("Accept");
    caption.attrs.insert("for".to_string(), "terms".to_string());

    let mut row = Element::new().child(control).child(caption);
    row.template_id = Some("terms".into());

    let found = find_by_focus_id(&row, "terms").expect("control");
    assert!(
        found.on_click.is_some(),
        "must resolve focused control, not template_id-only row"
    );
    assert_eq!(found.focus_id.as_deref(), Some("terms"));

    let path = [HitPathEntry {
        label_for: Some("terms".into()),
        ..Default::default()
    }];
    let (cb, fid) = resolve_click_from_path(&row, &path);
    assert_eq!(fid.as_deref(), Some("terms"));
    (*cb.expect("forwarded click").borrow_mut())();
    assert_eq!(toggled.get(), 1);
}

#[test]
fn mouse_event_exposes_dom_fields() {
    let mut ev = MouseEvent::new(40.0, 18.0, MouseButton::Left, 1);
    ev.current_bounds = [10.0, 8.0, 100.0, 32.0];
    ev.alt_key = true;
    ev.buttons = 1;
    ev.screen_x = 80.0;
    ev.movement_x = 3.0;
    assert_eq!(ev.client_x(), 40.0);
    assert_eq!(ev.client_y(), 18.0);
    assert!((ev.offset_x() - 30.0).abs() < 0.01);
    assert!((ev.offset_y() - 10.0).abs() < 0.01);
    assert_eq!(ev.button_index(), 0);
    assert_eq!(ev.which(), 1);
    assert!(ev.get_modifier_state("Alt"));
    assert!(!ev.get_modifier_state("Shift"));
    assert_eq!(ev.buttons, 1);
    ev.prevent_default();
    assert!(ev.default_prevented());
}

#[test]
fn mousedown_and_mouseover_dispatch() {
    let downs = Rc::new(Cell::new(0u8));
    let overs = Rc::new(Cell::new(0u8));
    let d = downs.clone();
    let o = overs.clone();

    let mut leaf = Element::new();
    leaf.layout_bounds = [0.0, 0.0, 20.0, 20.0];
    leaf = leaf
        .on_mousedown(move |e| {
            assert_eq!(e.event_type, "mousedown");
            d.set(d.get() + 1);
        })
        .on_mouseover(move |e| {
            assert_eq!(e.related_target.as_deref(), Some("prev"));
            o.set(o.get() + 1);
        });

    let mut root = Element::new();
    root.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    root.children.push(leaf);

    let path = hit_path(&root, 5.0, 5.0);
    let mut down = MouseEvent::new(5.0, 5.0, MouseButton::Left, 1);
    down.event_type = "mousedown";
    dispatch_pointer(&path, &mut down, PointerKind::Down);
    assert_eq!(downs.get(), 1);

    let mut over = MouseEvent::new(5.0, 5.0, MouseButton::Left, 0);
    over.event_type = "mouseover";
    over.related_target = Some("prev".into());
    dispatch_pointer(&path, &mut over, PointerKind::Over);
    assert_eq!(overs.get(), 1);
}

#[test]
fn mousedown_is_alias_of_pointer_down_single_fire() {
    let n = Rc::new(Cell::new(0u8));
    let a = n.clone();
    let b = n.clone();
    let mut leaf = Element::new();
    leaf.layout_bounds = [0.0, 0.0, 20.0, 20.0];
    leaf = leaf
        .on_pointer_down(move |_e| {
            a.set(a.get() + 1);
        })
        .on_mousedown(move |_e| {
            b.set(b.get() + 10);
        });
    let mut root = Element::new();
    root.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    root.children.push(leaf);
    let path = hit_path(&root, 5.0, 5.0);
    let mut ev = MouseEvent::new(5.0, 5.0, MouseButton::Left, 1);
    ev.event_type = "mousedown";
    dispatch_pointer(&path, &mut ev, PointerKind::Down);
    // Alias: later `on_mousedown` replaces `on_pointer_down`; still one dispatch.
    assert_eq!(n.get(), 10);
}

fn click_tree() -> (Element, Rc<Cell<u32>>) {
    let n = Rc::new(Cell::new(0u32));
    let n2 = n.clone();
    let mut btn = Element::new();
    btn.layout_bounds = [50.0, 0.0, 40.0, 40.0];
    btn.focus_id = Some("btn".into());
    btn = btn.on_click(move || n2.set(n2.get() + 1));

    let mut pad = Element::new();
    pad.layout_bounds = [0.0, 0.0, 40.0, 40.0];
    pad.focus_id = Some("pad".into());

    let mut root = Element::new();
    root.layout_bounds = [0.0, 0.0, 100.0, 40.0];
    root.focus_id = Some("root".into());
    root.children.push(pad);
    root.children.push(btn);
    (root, n)
}

#[test]
fn click_ignores_press_outside_release_inside() {
    let (root, n) = click_tree();
    let down = hit_path(&root, 10.0, 10.0);
    let up = hit_path(&root, 60.0, 10.0);
    assert_eq!(down.last().unwrap().focus_id.as_deref(), Some("pad"));
    assert_eq!(up.last().unwrap().focus_id.as_deref(), Some("btn"));
    let click_path = click_dispatch_path(&down, &up);
    let (cb, _) = resolve_click_from_path(&root, &click_path);
    assert!(
        cb.is_none(),
        "DOM click target is the LCA; outer press + inner release must not fire the button"
    );
    assert_eq!(n.get(), 0);
}

#[test]
fn click_fires_when_press_and_release_share_the_button() {
    let (root, n) = click_tree();
    let down = hit_path(&root, 60.0, 10.0);
    let up = hit_path(&root, 70.0, 12.0);
    let click_path = click_dispatch_path(&down, &up);
    let (cb, _) = resolve_click_from_path(&root, &click_path);
    (*cb.expect("button click").borrow_mut())();
    assert_eq!(n.get(), 1);
}

#[test]
fn mousemove_on_parent_fires_when_label_child_is_target() {
    let n = Rc::new(Cell::new(0u32));
    let n2 = n.clone();
    let mut child = Element::new();
    child.layout_bounds = [10.0, 10.0, 80.0, 20.0];
    child.focus_id = Some("hint".into());
    child = child.text("hint");

    let mut pad = Element::new();
    pad.layout_bounds = [0.0, 0.0, 100.0, 40.0];
    pad.focus_id = Some("pad".into());
    pad = pad.on_mousemove(move |_e| {
        n2.set(n2.get() + 1);
    });
    pad.children.push(child);

    let mut root = Element::new();
    root.layout_bounds = [0.0, 0.0, 100.0, 40.0];
    root.children.push(pad);

    let path = hit_path(&root, 50.0, 20.0);
    assert_eq!(path.last().unwrap().focus_id.as_deref(), Some("hint"));
    assert!(path.iter().any(|e| e.focus_id.as_deref() == Some("pad")));
    let mut ev = MouseEvent::new(50.0, 20.0, MouseButton::Left, 0);
    ev.event_type = "mousemove";
    dispatch_pointer(&path, &mut ev, PointerKind::Move);
    assert!(
        n.get() >= 1,
        "interactive parent on_mousemove must fire while the pointer slides the child label"
    );
}

#[test]
fn click_fires_when_release_is_on_child_of_press_target() {
    let n = Rc::new(Cell::new(0u32));
    let n2 = n.clone();
    let mut child = Element::new();
    child.layout_bounds = [55.0, 5.0, 20.0, 20.0];
    child.focus_id = Some("label".into());

    let mut btn = Element::new();
    btn.layout_bounds = [50.0, 0.0, 40.0, 40.0];
    btn.focus_id = Some("btn".into());
    btn = btn.on_click(move || n2.set(n2.get() + 1));
    btn.children.push(child);

    let mut root = Element::new();
    root.layout_bounds = [0.0, 0.0, 100.0, 40.0];
    root.focus_id = Some("root".into());
    root.children.push(btn);

    let down = hit_path(&root, 60.0, 10.0);
    let up = hit_path(&root, 62.0, 12.0);
    assert!(down.iter().any(|e| e.focus_id.as_deref() == Some("btn")));
    let click_path = click_dispatch_path(&down, &up);
    let (cb, _) = resolve_click_from_path(&root, &click_path);
    (*cb.expect("ancestor button").borrow_mut())();
    assert_eq!(n.get(), 1);
}

#[test]
fn click_survives_pointer_capture_short_up_path() {
    let (root, n) = click_tree();
    let down = hit_path(&root, 60.0, 10.0);
    let captured = down
        .iter()
        .find(|e| e.focus_id.as_deref() == Some("btn"))
        .cloned()
        .expect("btn on press path");
    let up = vec![captured];
    let click_path = click_dispatch_path(&down, &up);
    let (cb, _) = resolve_click_from_path(&root, &click_path);
    (*cb.expect("captured button").borrow_mut())();
    assert_eq!(n.get(), 1);
}

#[test]
fn still_pointer_click_keeps_press_target_when_live_hit_slides() {
    let (root, n) = click_tree();
    let down = hit_path(&root, 60.0, 10.0);
    let up = hit_path(&root, 10.0, 10.0);
    assert_eq!(down.last().unwrap().focus_id.as_deref(), Some("btn"));
    assert_eq!(up.last().unwrap().focus_id.as_deref(), Some("pad"));
    let moved = click_dispatch_path(&down, &up);
    let (cb_moved, _) = resolve_click_from_path(&root, &moved);
    assert!(
        cb_moved.is_none(),
        "user motion off the button must not click it"
    );
    let still = click_gesture_path(&down, &up, false, false);
    let (cb, _) = resolve_click_from_path(&root, &still);
    (*cb.expect("still pointer keeps the press button")
        .borrow_mut())();
    assert_eq!(n.get(), 1);
}

#[test]
fn user_scroll_after_press_cancels_click_even_if_pointer_still() {
    let (root, n) = click_tree();
    let down = hit_path(&root, 60.0, 10.0);
    let up = hit_path(&root, 60.0, 10.0);
    assert_eq!(down.last().unwrap().focus_id.as_deref(), Some("btn"));
    let click_path = click_gesture_path(&down, &up, false, true);
    let (cb, _) = resolve_click_from_path(&root, &click_path);
    assert!(
        cb.is_none(),
        "wheel / pan after mousedown must not fire on_click"
    );
    assert_eq!(n.get(), 0);
}
