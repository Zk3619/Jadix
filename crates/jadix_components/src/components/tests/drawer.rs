use super::*;

#[test]
fn background_removal_and_owner_disposal_restore_authored_scale_once() {
    use crate::authoring::presentation::{self, Context, Phase};
    for dispose_owner in [false, true] {
        DRAWER_SCALE_BG.with(|state| *state.borrow_mut() = None);
        DRAWER_SCALE_BG_APPLIED.set(false);
        let (owner, layer) = crate::core::reactivity::create_root(|| {
            let lifetime = std::rc::Rc::new(());
            register_drawer_scale_background("retiring-drawer", true, 1.0, &lifetime);
            let mut layer = Element::new();
            attach_drawer_background(&mut layer, "retiring-drawer", &lifetime);
            layer
        });
        let mut page = Element::new();
        page.style.scale = [0.8, 0.9];
        let mut root = Element::new().child(page).child(layer);
        let mut layout = crate::layout::LayoutTree::new();
        let context = Context::new(Phase::BeforeLayout, [800.0, 600.0]);
        presentation::sync_tree(&mut root, &mut layout, context);
        assert_eq!(root.children[0].style.scale, [0.8 * 0.96, 0.9 * 0.96]);
        presentation::sync_tree(&mut root, &mut layout, context);
        assert_eq!(root.children[0].style.scale, [0.8 * 0.96, 0.9 * 0.96]);
        if dispose_owner {
            owner.dispose();
        } else {
            root.children.pop();
        }
        presentation::sync_tree(&mut root, &mut layout, context);
        assert_eq!(root.children[0].style.scale, [0.8, 0.9]);
        presentation::sync_tree(&mut root, &mut layout, context);
        assert_eq!(root.children[0].style.scale, [0.8, 0.9]);
        assert!(DRAWER_SCALE_BG.with(|state| state.borrow().is_none()));
    }
}

#[test]
fn responsive_drawer_caps_sides_and_short_viewport_metrics() {
    use crate::core::tree::LayoutTree;
    use crate::style::{resolve_class_variants, VariantContext};
    for direction in [DrawerDirection::Left, DrawerDirection::Right] {
        for width in [200.0, 320.0, 639.5, 640.0, 1024.0] {
            let mut panel =
                apply_drawer_shell_surface(Element::new(), direction, Some("50rem"), None, None)
                    .class("shadow-none");
            resolve_class_variants(
                &mut panel,
                &VariantContext {
                    viewport_w: width,
                    ..Default::default()
                },
            );
            let mut root = Element::new().relative().w(width).h(600.0).child(panel);
            LayoutTree::new().build_and_layout(&mut root, width, 600.0, [-1.0, -1.0]);
            let bounds = root.children[0].layout_bounds;
            assert!(
                bounds[0] >= -0.5 && bounds[0] + bounds[2] <= width + 0.5,
                "{direction:?} {width}: {bounds:?}"
            );
        }
    }
    for height in [20.0, 80.0, 120.0, 320.0, 800.0] {
        for fraction in [0.0, 0.3, 1.0] {
            assert!(drawer_y_max_h(height) <= height);
            assert!(drawer_y_visible_h(fraction, height) <= height);
            for direction in [DrawerDirection::Up, DrawerDirection::Down] {
                let panel = apply_drawer_shell_surface(
                    Element::new(),
                    direction,
                    None,
                    None,
                    Some(drawer_y_visible_h(fraction, height)),
                );
                let mut root = Element::new().relative().w(320.0).h(height).child(panel);
                LayoutTree::new().build_and_layout(&mut root, 320.0, height, [-1.0, -1.0]);
                let bounds = root.children[0].layout_bounds;
                assert!(
                    bounds[3] > 0.0 && bounds[1] >= -0.5 && bounds[1] + bounds[3] <= height + 0.5,
                    "{direction:?} height={height}: {bounds:?}"
                );
            }
        }
    }
}

#[test]
fn sequential_snap_crosses_neighbor_midpoint() {
    let points = [0.2, 0.5, 1.0];
    assert_eq!(sequential_snap_target(&points, 1.0, -0.3), 0.5);
    assert_eq!(sequential_snap_target(&points, 0.5, 0.3), 1.0);
    assert_eq!(sequential_snap_target(&points, 0.5, -0.2), 0.2);
    assert_eq!(sequential_snap_target(&points, 1.0, -0.1), 1.0);
}

#[test]
fn configured_low_snap_is_not_a_dismiss_gesture() {
    assert!(!should_dismiss_drawer(0.2, 0.0, 0.2));
    assert!(!should_dismiss_drawer(0.15, 0.0, 0.2));
    assert!(should_dismiss_drawer(0.04, 0.0, 0.2));
}

#[test]
fn stationary_drag_does_not_schedule_another_layout() {
    crate::core::reactivity::create_root(|| {
        set_drawer_viewport_size(800.0, 800.0);
        let open = Signal::new(true);
        begin_drawer_drag(
            "stationary".into(),
            DrawerDirection::Down,
            400.0,
            200.0,
            &[DrawerSnapPoint::Fraction(1.0)],
            false,
            open,
            "stationary-handle".into(),
            true,
        );
        let mut root = Element::new();
        assert!(tick_drawer_drag(&mut root, 400.0, 230.0));
        PENDING_DRAWER_LAYOUT_BUMP.with(|q| q.borrow_mut().clear());
        assert!(!tick_drawer_drag(&mut root, 400.0, 230.0));
        assert!(PENDING_DRAWER_LAYOUT_BUMP.with(|q| q.borrow().is_empty()));
        DRAWER_DRAG.with(|d| *d.borrow_mut() = None);
    });
}

#[test]
fn secondary_button_does_not_start_drawer_drag() {
    crate::core::reactivity::create_root(|| {
        DRAWER_DRAG.with(|d| *d.borrow_mut() = None);
        let open = Signal::new(true);
        let el = bind_drawer_drag_region(
            Element::new(),
            "right-click",
            "right-click-handle",
            open,
            DrawerDirection::Down,
            &[DrawerSnapPoint::Fraction(1.0)],
            false,
            true,
        );
        let e = PointerEvent::new(400.0, 200.0, winit::event::MouseButton::Right, 1);
        el.on_mousedown.as_ref().unwrap().borrow_mut()(&e);
        assert!(!is_drawer_dragging("right-click"));
        DRAWER_DRAG.with(|d| *d.borrow_mut() = None);
    });
}

#[test]
fn explicit_handle_inherits_content_swipe_configuration() {
    crate::core::reactivity::create_root(|| {
        DRAWER_DRAG.with(|d| *d.borrow_mut() = None);
        let open = Signal::new(true);
        provide_context(
            drawer_context(),
            DrawerCtx {
                open,
                id: "explicit-handle".into(),
            },
        );
        provide_context(
            drawer_swipe_context(),
            DrawerSwipeConfig {
                direction: DrawerDirection::Left,
                snap_points: vec![
                    DrawerSnapPoint::Fraction(0.25),
                    DrawerSnapPoint::Fraction(1.0),
                ],
                sequential: true,
            },
        );
        let handle = crate::ui! { <DrawerSwipeHandle /> };
        let e = PointerEvent::new(100.0, 200.0, winit::event::MouseButton::Left, 1);
        handle.on_mousedown.as_ref().unwrap().borrow_mut()(&e);
        let drag = DRAWER_DRAG.with(|d| d.borrow().clone()).unwrap();
        assert_eq!(drag.direction, DrawerDirection::Left);
        assert_eq!(drag.snap_fracs, [0.25, 1.0]);
        assert!(drag.snap_to_sequential);
        DRAWER_DRAG.with(|d| *d.borrow_mut() = None);
        release_pointer_capture();
    });
}

#[test]
fn invalid_snap_fraction_cannot_poison_nearest_snap() {
    assert_eq!(
        resolve_snap_fractions(&[DrawerSnapPoint::Fraction(f32::NAN)], 400.0),
        [1.0]
    );
    assert_eq!(
        resolve_snap_fractions(
            &[
                DrawerSnapPoint::Fraction(f32::NAN),
                DrawerSnapPoint::Fraction(0.5)
            ],
            400.0
        ),
        [0.5]
    );
}

#[test]
fn releasing_vertical_drawer_keeps_live_height_before_settling() {
    crate::core::reactivity::create_root(|| {
        set_drawer_viewport_size(800.0, 800.0);
        let open = Signal::new(true);
        let id = "release-height";
        drawer_frac_signal(id).set(1.0);
        begin_drawer_drag(
            id.into(),
            DrawerDirection::Down,
            400.0,
            100.0,
            &[
                DrawerSnapPoint::Fraction(0.5),
                DrawerSnapPoint::Fraction(1.0),
            ],
            true,
            open,
            "release-height-handle".into(),
            true,
        );
        let mut root = Element::new();
        tick_drawer_drag(&mut root, 400.0, 100.0 + drawer_y_max_h(800.0) * 0.3);
        let before = drawer_open_fraction(id);
        clear_drawer_drag_if_any();
        assert!(
            (drawer_open_fraction(id) - before).abs() < 0.01,
            "release must start at the dragged height, not jump to the snap target"
        );
        let started = DRAWER_Y_SETTLING.with(|s| s.borrow()[id].started);
        assert!(tick_drawer_snaps_at(
            started + std::time::Duration::from_millis(80)
        ));
        assert!(drawer_open_fraction(id) < before && drawer_open_fraction(id) > 0.5);
        assert!(!tick_drawer_snaps_at(
            started + std::time::Duration::from_millis(300)
        ));
        assert!((drawer_open_fraction(id) - 0.5).abs() < 0.001);
        assert!(crate::app::pointer_capture_id().is_none());
    });
}

#[test]
fn nested_control_keeps_its_pointer_gesture() {
    use crate::app::pointer_events::{dispatch_pointer, HitPathEntry, PointerKind};
    crate::core::reactivity::create_root(|| {
        DRAWER_DRAG.with(|d| *d.borrow_mut() = None);
        let panel = bind_drawer_drag_region(
            Element::new(),
            "controls",
            "controls-panel",
            Signal::new(true),
            DrawerDirection::Down,
            &[DrawerSnapPoint::Fraction(1.0)],
            false,
            false,
        );
        for focus in ["input-link", "scroll:controls__body"] {
            let child = Element::new().focus_id(focus);
            let path = [
                HitPathEntry::from_element(&panel, [0.0, 0.0, 800.0, 700.0]),
                HitPathEntry::from_element(&child, [10.0, 100.0, 300.0, 60.0]),
            ];
            let mut e = PointerEvent::new(30.0, 120.0, winit::event::MouseButton::Left, 1);
            dispatch_pointer(&path, &mut e, PointerKind::Down);
            assert!(
                !is_drawer_dragging("controls"),
                "nested {focus} must own dragging"
            );
        }
    });
}

#[test]
fn drawer_direction_parses_shadcn_tokens() {
    assert_eq!(DrawerDirection::from("down"), DrawerDirection::Down);
    assert_eq!(DrawerDirection::from("bottom"), DrawerDirection::Down);
    assert_eq!(DrawerDirection::from("UP"), DrawerDirection::Up);
    assert_eq!(DrawerDirection::from("left"), DrawerDirection::Left);
    assert_eq!(DrawerDirection::from("right"), DrawerDirection::Right);
}

#[test]
fn snap_points_normalize_and_nearest() {
    let pts = resolve_snap_fractions(
        &[
            DrawerSnapPoint::Fraction(0.35),
            DrawerSnapPoint::Fraction(1.0),
        ],
        800.0,
    );
    assert!((nearest_snap(&pts, 0.4) - 0.35).abs() < 0.01);
}

#[test]
fn snap_point_pixels_convert_to_fraction() {
    let pt = DrawerSnapPoint::Pixels(400.0);
    assert!((pt.to_open_fraction(800.0) - 0.5).abs() < 0.01);
}

#[test]
fn snap_sequential_steps_from_start_not_nearest() {
    let pts = vec![0.2_f32, 0.5, 1.0];
    // Dragged past nearest (0.5) toward closed, but sequential should step 1.0 → 0.5.
    let target = sequential_snap_target(&pts, 1.0, -0.6);
    assert!((target - 0.5).abs() < 0.01);
    // Tiny move stays on start snap.
    let stay = sequential_snap_target(&pts, 1.0, -0.1);
    assert!((stay - 1.0).abs() < 0.01);
    let half = sequential_snap_target(&pts, 0.2, -0.02);
    assert!(
        (half - 0.2).abs() < 0.01,
        "small drag from the lowest snap must stay, got {half}"
    );
}

#[test]
fn dismiss_from_half_snap_requires_close_threshold_not_flick() {
    assert!(!should_dismiss_drawer(0.5, 2000.0, 0.5));
    assert!(!should_dismiss_drawer(0.48, 2000.0, 0.5));
    assert!(!should_dismiss_drawer(0.40, 2000.0, 0.5));
    assert!(should_dismiss_drawer(0.20, 0.0, 0.5));
    assert!(should_dismiss_drawer(0.30, 2000.0, 0.5));
}

#[test]
fn release_snap_extra_is_target_pose_not_fully_open() {
    let axis = 700.0;
    let from = drawer_drag_translate(DrawerDirection::Down, 0.42, axis);
    let to = drawer_drag_translate(DrawerDirection::Down, 0.5, axis);
    assert!(from[1] > to[1]);
    assert!(
        to[1] > 1.0,
        "0.5 snap must keep a closed-axis offset, got {}",
        to[1]
    );
    let full = drawer_drag_translate(DrawerDirection::Down, 1.0, axis);
    assert!(full[1].abs() < 0.01);
}

#[test]
fn drag_translate_maps_open_frac() {
    let t = drawer_drag_translate(DrawerDirection::Down, 0.5, 800.0);
    assert!((t[1] - 400.0).abs() < 0.01);
    assert!(t[0].abs() < 0.01);
    let left = drawer_drag_translate(DrawerDirection::Left, 0.0, 400.0);
    assert!((left[0] + 400.0).abs() < 0.01);
    let right = drawer_drag_translate(DrawerDirection::Right, 0.0, 400.0);
    assert!((right[0] - 400.0).abs() < 0.01);
}

#[test]
fn drawer_shell_stays_absolute_on_dock_edge() {
    use taffy::style::Position;
    let down = apply_drawer_shell_surface(Element::new(), DrawerDirection::Down, None, None, None);
    assert_eq!(down.style.position, Position::Absolute);
    assert!(swallow_clicks(down).style.position == Position::Absolute);
    let right =
        apply_drawer_shell_surface(Element::new(), DrawerDirection::Right, None, None, None);
    assert_eq!(right.style.position, Position::Absolute);
}

#[test]
fn drawer_y_drag_translate_follows_open_frac() {
    let axis = drawer_y_max_h(800.0);
    let half = drawer_drag_translate(DrawerDirection::Down, 0.5, axis);
    assert!(
        (half[1] - axis * 0.5).abs() < 0.01,
        "0.5 snap is half of max-h via translate, got {}",
        half[1]
    );
    let closed = drawer_drag_translate(DrawerDirection::Down, 0.0, axis);
    assert!((closed[1] - axis).abs() < 0.01);
    let open = drawer_drag_translate(DrawerDirection::Down, 1.0, axis);
    assert!(open[1].abs() < 0.01);
}

#[test]
fn drawer_y_shell_uses_max_h_at_full_open() {
    use crate::layout::dimension::{dim_kind, DimKind};
    let max_h = drawer_y_max_h(800.0);
    let panel = apply_drawer_shell_surface(
        Element::new(),
        DrawerDirection::Down,
        None,
        None,
        Some(max_h),
    );
    match dim_kind(panel.style.size.height) {
        DimKind::Length(h) => assert!((h - max_h).abs() < 0.5, "got {h}, want {max_h}"),
        other => panic!("expected pixel height, got {other:?}"),
    }
}

#[test]
fn drawer_y_rest_height_follows_open_frac() {
    let vh = 800.0;
    let max_h = drawer_y_max_h(vh);
    let (full_h, full_extra) = drawer_y_shell_metrics(1.0, vh);
    assert!((full_h - max_h).abs() < 0.5);
    assert!(
        full_extra[1].abs() < 0.01,
        "rest extra is 0, got {}",
        full_extra[1]
    );
    let (half_h, half_extra) = drawer_y_shell_metrics(0.5, vh);
    assert!(
        (half_h - max_h * 0.5).abs() < 0.5,
        "0.5 snap rest height should be half max-h, got {half_h}"
    );
    assert!(half_extra[1].abs() < 0.01);
    assert!(
        half_h + 1.0 < max_h,
        "half-open must be shorter than full so body can scroll"
    );
    let (drag_h, drag_extra) = drawer_y_shell_metrics(0.5, vh);
    assert!(
        (drag_h - half_h).abs() < 0.5,
        "drag must resize height, not keep max-h, got {drag_h}"
    );
    assert!(
        drag_extra[1].abs() < 0.01,
        "drag extra must stay 0 (no max-h translate), got {}",
        drag_extra[1]
    );
}

#[test]
fn background_scale_shrinks_when_open_not_when_closing() {
    let open = drawer_background_scale(1.0);
    let closed = drawer_background_scale(0.0);
    let mid = drawer_background_scale(0.5);
    assert!(
        (closed - 1.0).abs() < 0.001,
        "closed page is unscaled, got {closed}"
    );
    assert!(
        open < closed && (open - (1.0 - SCALE_BG_MAX_SHRINK)).abs() < 0.001,
        "open page shrinks (Vaul), got {open}"
    );
    assert!(mid > open && mid < closed);
}

#[test]
fn rest_height_plus_max_h_translate_shoves_top_to_viewport_bottom() {
    let vh = 800.0;
    let max_h = drawer_y_max_h(vh);
    let (rest_h, rest_extra) = drawer_y_shell_metrics(0.5, vh);
    assert!(rest_extra[1].abs() < 0.01);
    let premature = drawer_drag_translate(DrawerDirection::Down, 0.5, max_h);
    let visual_top = vh - rest_h + premature[1];
    assert!(
            visual_top + 1.0 >= vh,
            "writing max-h pixel extra onto rest height pins the handle at the viewport bottom (got top={visual_top}, vh={vh})"
        );
}

#[test]
fn exclusive_redrag_from_half_snap_resizes_height() {
    use crate::core::reactivity::{create_root, use_signal};
    create_root(|| {
        DRAWER_DRAG.with(|d| *d.borrow_mut() = None);
        set_drawer_viewport_size(800.0, 800.0);
        let open = use_signal("drawer-redrag-open", || true);
        let id = "drawer-redrag";
        drawer_frac_signal(id).set(0.5);
        begin_drawer_drag(
            id.to_string(),
            DrawerDirection::Down,
            400.0,
            600.0,
            &[
                DrawerSnapPoint::Fraction(0.5),
                DrawerSnapPoint::Fraction(1.0),
            ],
            false,
            open,
            format!("{id}__handle"),
            true,
        );
        let drag = DRAWER_DRAG.with(|d| d.borrow().clone()).expect("drag");
        assert!(drag.committed, "handle drag starts committed");
        let mut root = Element::new();
        let moved = tick_drawer_drag(&mut root, 400.0, 640.0);
        assert!(moved);
        let live = drawer_open_fraction(id);
        let axis = drawer_y_max_h(800.0);
        let expected = (0.5 - 40.0 / axis).clamp(0.0, 1.15);
        assert!(
            (live - expected).abs() < 0.01,
            "live frac must follow the pointer, got {live} want {expected}"
        );
        let (h, extra) = drawer_y_shell_metrics(live, 800.0);
        assert!(
            extra[1].abs() < 0.01,
            "Y drag must not use max-h translate, extra={}",
            extra[1]
        );
        assert!(
            (h - expected * axis).abs() < 1.0,
            "Y drag height must track frac×max-h ({expected}*{axis}={}), got {h}",
            expected * axis
        );
        assert!(
            h + 1.0 < axis,
            "half-open drag must not grow to max-h, h={h} max={axis}"
        );
        DRAWER_DRAG.with(|d| *d.borrow_mut() = None);
    });
}

#[test]
fn content_press_does_not_grow_half_open_rest() {
    use crate::core::reactivity::{create_root, use_signal};
    create_root(|| {
        DRAWER_DRAG.with(|d| *d.borrow_mut() = None);
        set_drawer_viewport_size(800.0, 800.0);
        let open = use_signal("drawer-click-open", || true);
        let id = "drawer-click";
        drawer_frac_signal(id).set(0.5);
        begin_drawer_drag(
            id.to_string(),
            DrawerDirection::Down,
            400.0,
            500.0,
            &[
                DrawerSnapPoint::Fraction(0.5),
                DrawerSnapPoint::Fraction(1.0),
            ],
            false,
            open,
            format!("{id}__panel_drag"),
            false,
        );
        let drag = DRAWER_DRAG.with(|d| d.borrow().clone()).expect("drag");
        assert!(!drag.committed);
        DRAWER_DRAG.with(|d| *d.borrow_mut() = None);
    });
}

#[test]
fn drawer_drag_layout_bump_sets_after_tree_borrow_drop() {
    use crate::app::reactive::{bind_prop, drain_deferred_view_remounts};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_effect, create_root, use_signal};
    use crate::core::tree::LayoutStatus;
    use std::rc::Rc;

    crate::app::reactive::reset_registered_anchors();
    clear_test_reconcile_root();
    let tree = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&tree));
    let id = "drawer-bump-drop";
    let (_handle, _) = create_root({
        let tree = Rc::clone(&tree);
        move || {
            let open = use_signal("drawer-bump-open", || true);
            DRAWER_DRAG.with(|d| *d.borrow_mut() = None);
            set_drawer_viewport_size(800.0, 800.0);
            drawer_frac_signal(id).set(0.5);
            create_effect({
                let tree = Rc::clone(&tree);
                let id = id.to_string();
                move || {
                    let leaf = bind_prop(
                        Element::new().template_id("drawer-bump-leaf"),
                        "drawer-bump-leaf",
                        {
                            let id = id.clone();
                            move || drawer_layout_bump(&id).get()
                        },
                        |el, n| {
                            el.text = Some(n.to_string());
                            LayoutStatus::StyleChanged
                        },
                    );
                    *tree.borrow_mut() = leaf;
                }
            });
            begin_drawer_drag(
                id.to_string(),
                DrawerDirection::Down,
                400.0,
                600.0,
                &[
                    DrawerSnapPoint::Fraction(0.5),
                    DrawerSnapPoint::Fraction(1.0),
                ],
                false,
                open,
                format!("{id}__handle"),
                true,
            );
        }
    });
    drain_deferred_view_remounts();
    assert_eq!(tree.borrow().text.as_deref(), Some("0"));

    {
        let held = tree.borrow_mut();
        let mut dummy = Element::new();
        assert!(tick_drawer_drag(&mut dummy, 400.0, 640.0));
        assert_eq!(
            held.text.as_deref(),
            Some("0"),
            "layout bump must not Signal::update while the live tree is borrowed"
        );
        assert_eq!(drawer_layout_bump(id).peek(), 0);
    }
    drain_drawer_layout_bumps();
    assert_eq!(drawer_layout_bump(id).peek(), 1);
    drain_deferred_view_remounts();
    assert_eq!(
        tree.borrow().text.as_deref(),
        Some("1"),
        "leaf bind must land once try_borrow_mut succeeds"
    );
    DRAWER_DRAG.with(|d| *d.borrow_mut() = None);
}
