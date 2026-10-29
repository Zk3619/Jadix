use super::*;
use crate::core::reactivity::create_root;
use crate::core::tree::Element;
use std::rc::Rc;

fn hit(id: &str, bounds: [f32; 4], drop: bool) -> DragHit {
    DragHit {
        id: id.into(),
        scope_id: "s".into(),
        is_draggable: !drop,
        is_droppable: drop,
        rect: Rect::from_bounds(bounds),
        item_type: "card".into(),
        accept: String::new(),
        list_id: Some("list".into()),
        index: None,
        group: String::new(),
        disabled: false,
        list_axis: None,
        skip_shift: false,
        skip_follow: false,
        pull_clone: false,
    }
}

#[test]
fn restricted_source_and_drop_follow_parent_scroll() {
    use crate::core::reactivity::use_signal;
    use crate::core::scroll_viewport::{
        scroll_offset_x_signal_key, scroll_offset_y_signal_key, SCROLL_VIEWPORT_FOCUS_PREFIX,
    };

    let (_owner, ()) = create_root(|| {
        let committed = Signal::new([0.0, 0.0]);
        let _scope = super::super::drag_scope("scroll-drop")
            .threshold(0.0)
            .restrict(Restrict::Parent)
            .on_drag_end(move |ev| committed.set(ev.delta))
            .child(Element::new);
        let sx = use_signal(&scroll_offset_x_signal_key("scroll-drop-page"), || 0.0_f32);
        let sy = use_signal(&scroll_offset_y_signal_key("scroll-drop-page"), || 0.0_f32);
        let mut card = Element::new();
        card.layout_bounds = [80.0, 240.0, 40.0, 24.0];
        card.set_attr(ATTR_DRAG_DRAGGABLE, "card");
        card.set_attr(ATTR_DRAG_SCOPE, "scroll-drop");
        let mut arena = Element::new();
        arena.layout_bounds = [10.0, 200.0, 200.0, 120.0];
        arena.set_attr("id", "scroll-drop-arena");
        arena = arena.child(card);
        let mut page = Element::new();
        page.focus_id = Some(format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}scroll-drop-page"));
        page.layout_bounds = [0.0, 0.0, 800.0, 600.0];
        page = page.child(arena);

        for axis in [Axis::Xy, Axis::X, Axis::Y] {
            for scroll in [[160.0, 180.0], [40.0, 20.0]] {
                for pointer in [[-500.0, -500.0], [900.0, 900.0]] {
                    sx.set_silent(100.0);
                    sy.set_silent(100.0);
                    SCOPES.with(|scopes| {
                        scopes.borrow_mut().get_mut("scroll-drop").unwrap().axis = axis;
                    });
                    let hits = collect_hits(&page);
                    let start = hits.iter().find(|h| h.id == "card").unwrap().rect;
                    begin(BeginDrag {
                        scope_id: "scroll-drop".into(),
                        draggable_id: "card".into(),
                        pointer: [start.x + 20.0, start.y + 12.0],
                        grab_offset: [20.0, 12.0],
                        start_bounds: start.to_bounds(),
                        from_list: None,
                        from_index: None,
                        item_type: "chip".into(),
                        group: String::new(),
                        capture_id: "cap".into(),
                    });
                    assert!(tick(pointer[0], pointer[1], &hits, [800.0, 600.0]));
                    sx.set_silent(scroll[0]);
                    sy.set_silent(scroll[1]);
                    // The page scrolls while the pointer stays at the edge.
                    resync_overlay_follow_after_layout(&mut page);
                    let ghost = peek_overlay_pos("scroll-drop").unwrap();
                    let hits = collect_hits(&page);
                    let slot = hits.iter().find(|h| h.id == "card").unwrap().rect;
                    let translate = active_source_translate().unwrap();
                    assert_eq!(
                            [slot.x + translate[0], slot.y + translate[1]],
                            ghost,
                            "scrolled source must paint at the restricted origin: axis={axis:?}, scroll={scroll:?}"
                        );
                    let painted = crate::core::tree::find_in_tree(&page, |el| {
                        el.attrs.get(ATTR_DRAG_DRAGGABLE).map(String::as_str) == Some("card")
                    })
                    .unwrap()
                    .style
                    .translate;
                    assert_eq!(
                        [slot.x + painted[0], slot.y + painted[1]],
                        ghost,
                        "post-layout sync must update the source before paint"
                    );
                    let parent = last_parent_bounds("scroll-drop", "card").unwrap();
                    assert!(ghost[0] >= parent[0] && ghost[1] >= parent[1]);
                    assert!(ghost[0] + slot.width <= parent[0] + parent[2]);
                    assert!(ghost[1] + slot.height <= parent[1] + parent[3]);
                    assert!(tick(pointer[0], pointer[1], &hits, [800.0, 600.0]));
                    let ghost = peek_overlay_pos("scroll-drop").unwrap();
                    assert!(end(&hits));
                    let delta = committed.peek();
                    assert_eq!(
                            [slot.x + delta[0], slot.y + delta[1]],
                            ghost,
                            "persisted position must match preview after scroll: axis={axis:?}, scroll={scroll:?}"
                        );
                }
            }
        }
    });
}

#[test]
fn threshold_then_activate_and_end() {
    let (_o, _) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([0.0_f32, 0.0]);
        register_scope(
            "s".into(),
            ScopeRuntime {
                collision: CollisionAlg::PointerWithin,
                axis: Axis::Xy,
                start_axis: Axis::Xy,
                threshold: 8.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: false,
                source_opacity: 1.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "a".into(),
            pointer: [10.0, 10.0],
            grab_offset: [2.0, 2.0],
            start_bounds: [8.0, 8.0, 40.0, 24.0],
            from_list: Some("list".into()),
            from_index: Some(0),
            item_type: "card".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let drops = vec![hit("zone", [50.0, 50.0, 80.0, 80.0], true)];
        assert!(!tick(12.0, 10.0, &drops, [400.0, 300.0]));
        assert!(!is_active());
        assert!(tick(30.0, 10.0, &drops, [400.0, 300.0]));
        assert!(is_active());
        assert_eq!(active_draggable.peek().as_deref(), Some("a"));
        assert_eq!(overlay_pos.peek(), [28.0, 8.0]);
        assert_eq!(last_hits().len(), 1);
        assert_eq!(last_hits_in_scope("s")[0].id, "zone");
        assert!(last_hits_in_scope("other").is_empty());
        let _ = end(&drops);
        assert!(!is_dragging());
        assert!(active_draggable.peek().is_none());
    });
}

#[test]
fn threshold_zero_stays_pending_on_press_activates_on_first_move() {
    let (_o, _) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([40.0_f32, 24.0]);
        register_scope(
            "s".into(),
            ScopeRuntime {
                collision: CollisionAlg::PointerWithin,
                axis: Axis::Xy,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: true,
                source_opacity: 0.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "a".into(),
            pointer: [20.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [10.0, 8.0, 40.0, 24.0],
            from_list: None,
            from_index: None,
            item_type: String::new(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(!is_active(), "begin stays pending until tick");
        assert!(!overlay_open.peek());
        assert!(
            !tick(20.0, 16.0, &[], [400.0, 300.0]),
            "press must not activate: source_opacity/stack chrome would fire with no ghost"
        );
        assert!(!is_active());
        assert!(
            !overlay_open.peek(),
            "overlay stays closed on press so a 0-opacity source is not erased"
        );
        assert!(
            tick(21.0, 16.0, &[], [400.0, 300.0]),
            "first movement is the drag attempt — overlay opens immediately"
        );
        assert!(is_active());
        assert!(overlay_open.peek());
        let _ = end(&[]);
    });
}

fn seed_velocity(vx: f32, vy: f32) {
    VELOCITY.with(|v| {
        let mut t = v.borrow_mut();
        t.reset();
        let t0 = Instant::now();
        t.sample([0.0, 0.0], t0);
        t.sample(
            [vx * 0.02, vy * 0.02],
            t0 + std::time::Duration::from_millis(20),
        );
    });
}

#[test]
fn free_move_inertia_coasts_then_commits() {
    let (_o, _) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([0.0_f32, 0.0]);
        register_scope(
            "s".into(),
            ScopeRuntime {
                collision: CollisionAlg::PointerWithin,
                axis: Axis::Xy,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: false,
                source_opacity: 1.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        set_scope_inertia(
            "s",
            crate::utils::pointer_action::InertiaOpts {
                enabled: true,
                min_speed: 50.0,
                ..Default::default()
            },
        );
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "a".into(),
            pointer: [10.0, 10.0],
            grab_offset: [0.0, 0.0],
            start_bounds: [10.0, 10.0, 40.0, 24.0],
            from_list: None,
            from_index: None,
            item_type: String::new(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let drops: Vec<DragHit> = vec![];
        assert!(tick(20.0, 10.0, &drops, [400.0, 300.0]));
        seed_velocity(400.0, 0.0);
        assert!(end(&drops));
        assert!(is_coasting());
        assert!(is_dragging());
        let dummy = Element::new();
        let mut frames = 0;
        while is_coasting() && frames < 200 {
            let _ = pump_inertia(&dummy, [400.0, 300.0]);
            frames += 1;
        }
        assert!(!is_coasting(), "frames={frames}");
        assert!(!is_dragging());
    });
}

#[test]
fn sortable_does_not_coast() {
    let (_o, _) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([0.0_f32, 0.0]);
        register_scope(
            "s".into(),
            ScopeRuntime {
                collision: CollisionAlg::PointerWithin,
                axis: Axis::Xy,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: false,
                source_opacity: 1.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        set_scope_inertia(
            "s",
            crate::utils::pointer_action::InertiaOpts {
                enabled: true,
                min_speed: 1.0,
                ..Default::default()
            },
        );
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "a".into(),
            pointer: [10.0, 10.0],
            grab_offset: [0.0, 0.0],
            start_bounds: [10.0, 10.0, 40.0, 24.0],
            from_list: Some("list".into()),
            from_index: Some(0),
            item_type: "card".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let drops = vec![hit("zone", [50.0, 50.0, 80.0, 80.0], true)];
        let _ = tick(40.0, 10.0, &drops, [400.0, 300.0]);
        seed_velocity(400.0, 0.0);
        assert!(end(&drops));
        assert!(!is_coasting());
        assert!(!is_dragging());
    });
}

#[test]
fn lock_axis_start_pins_secondary_after_first_move() {
    let (_o, _) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([0.0_f32, 0.0]);
        register_scope(
            "s-start".into(),
            ScopeRuntime {
                collision: CollisionAlg::PointerWithin,
                axis: Axis::Start,
                start_axis: Axis::Xy,
                threshold: 4.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: false,
                source_opacity: 1.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        begin(BeginDrag {
            scope_id: "s-start".into(),
            draggable_id: "a".into(),
            pointer: [10.0, 10.0],
            grab_offset: [2.0, 2.0],
            start_bounds: [8.0, 8.0, 40.0, 24.0],
            from_list: None,
            from_index: None,
            item_type: "card".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let drops = vec![hit("zone", [50.0, 50.0, 80.0, 80.0], true)];
        assert!(tick(30.0, 18.0, &drops, [400.0, 300.0]));
        let pos = overlay_pos.peek();
        assert!(
            (pos[0] - 28.0).abs() < 1e-3 && (pos[1] - 8.0).abs() < 1e-3,
            "first move dx>dy must lock X (Y stays at grab origin); got {pos:?}"
        );
        let _ = end(&drops);
    });
}

#[test]
fn start_axis_x_stays_pending_on_vertical_then_activates_on_horizontal() {
    let (_o, _) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([0.0_f32, 0.0]);
        register_scope(
            "s-sa".into(),
            ScopeRuntime {
                collision: CollisionAlg::PointerWithin,
                axis: Axis::Xy,
                start_axis: Axis::X,
                threshold: 4.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: false,
                source_opacity: 1.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        begin(BeginDrag {
            scope_id: "s-sa".into(),
            draggable_id: "a".into(),
            pointer: [10.0, 10.0],
            grab_offset: [0.0, 0.0],
            start_bounds: [10.0, 10.0, 40.0, 24.0],
            from_list: None,
            from_index: None,
            item_type: "card".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let drops = vec![hit("zone", [50.0, 50.0, 80.0, 80.0], true)];
        assert!(!tick(10.0, 30.0, &drops, [400.0, 300.0]));
        assert!(
            !is_active(),
            "vertical first move must not activate start_axis=x"
        );
        assert!(tick(30.0, 12.0, &drops, [400.0, 300.0]));
        assert!(is_active());
        let _ = end(&drops);
    });
}

#[test]
fn snap_end_only_applies_on_end_not_tick() {
    let (_o, _) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([0.0_f32, 0.0]);
        register_scope(
            "s-snap".into(),
            ScopeRuntime {
                collision: CollisionAlg::PointerWithin,
                axis: Axis::Xy,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: Some(Snap::grid(SnapGrid::new(10.0, 10.0).end_only(true))),
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: false,
                source_opacity: 1.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        begin(BeginDrag {
            scope_id: "s-snap".into(),
            draggable_id: "a".into(),
            pointer: [0.0, 0.0],
            grab_offset: [0.0, 0.0],
            start_bounds: [0.0, 0.0, 20.0, 20.0],
            from_list: None,
            from_index: None,
            item_type: "card".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let drops = vec![hit("zone", [0.0, 0.0, 400.0, 300.0], true)];
        assert!(tick(14.0, 0.0, &drops, [400.0, 300.0]));
        let mid = overlay_pos.peek();
        assert!(
            (mid[0] - 14.0).abs() < 1e-3,
            "end_only must not snap during tick; got {mid:?}"
        );
        let _ = end(&drops);
        let done = overlay_pos.peek();
        assert!(
            (done[0] - 10.0).abs() < 1e-3,
            "end_only must snap on end; got {done:?}"
        );
    });
}

#[test]
fn axis_y_overlay_x_follows_scrolled_layout_slot() {
    let (_o, _) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([0.0_f32, 0.0]);
        register_scope(
            "s".into(),
            ScopeRuntime {
                collision: CollisionAlg::PointerWithin,
                axis: Axis::Y,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: true,
                source_opacity: 0.4,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "a".into(),
            pointer: [110.0, 50.0],
            grab_offset: [10.0, 10.0],
            start_bounds: [100.0, 40.0, 80.0, 24.0],
            from_list: Some("list".into()),
            from_index: Some(0),
            item_type: "card".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let mut src = hit("a", [40.0, 40.0, 80.0, 24.0], false);
        src.index = Some(0);
        let hits = vec![src];
        assert!(tick(200.0, 90.0, &hits, [400.0, 300.0]));
        let pos = overlay_pos.peek();
        assert!(
                (pos[0] - 40.0).abs() < 0.5,
                "axis=y must pin overlay X to the scrolled layout slot, not press-time start_bounds; got {:?}",
                pos
            );
        assert!(
            (pos[1] - 80.0).abs() < 0.5,
            "axis=y free axis still follows the pointer (50+40-10); got {:?}",
            pos
        );
        let _ = end(&hits);
    });
}

#[test]
fn axis_xy_overlay_does_not_pin_to_scrolled_slot() {
    let (_o, _) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([0.0_f32, 0.0]);
        register_scope(
            "s".into(),
            ScopeRuntime {
                collision: CollisionAlg::PointerWithin,
                axis: Axis::Xy,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: true,
                source_opacity: 0.4,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "a".into(),
            pointer: [110.0, 50.0],
            grab_offset: [10.0, 10.0],
            start_bounds: [100.0, 40.0, 80.0, 24.0],
            from_list: Some("list".into()),
            from_index: Some(0),
            item_type: "card".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let hits = vec![hit("a", [40.0, 40.0, 80.0, 24.0], false)];
        assert!(tick(200.0, 90.0, &hits, [400.0, 300.0]));
        assert_eq!(
            overlay_pos.peek(),
            [190.0, 80.0],
            "unlocked axis still follows pointer − grab, not the scrolled slot"
        );
        let _ = end(&hits);
    });
}

#[test]
fn pump_autoscroll_resyncs_axis_y_after_parent_scroll() {
    use crate::core::reactivity::use_signal;
    use crate::core::scroll_viewport::{scroll_offset_x_signal_key, SCROLL_VIEWPORT_FOCUS_PREFIX};

    let (_o, _) = create_root(|| {
        use_signal(&scroll_offset_x_signal_key("drag-axis-scroll"), || 0.0_f32).set_silent(0.0);
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([0.0_f32, 0.0]);
        register_scope(
            "s".into(),
            ScopeRuntime {
                collision: CollisionAlg::PointerWithin,
                axis: Axis::Y,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: true,
                source_opacity: 0.4,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        let mut chip = Element::new();
        chip.layout_bounds = [100.0, 40.0, 80.0, 24.0];
        chip.set_attr(ATTR_DRAG_DRAGGABLE, "a");
        chip.set_attr(ATTR_DRAG_SCOPE, "s");
        let mut page = Element::new();
        page.focus_id = Some(format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}drag-axis-scroll"));
        page.layout_bounds = [0.0, 0.0, 400.0, 300.0];
        page = page.child(chip);

        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "a".into(),
            pointer: [110.0, 50.0],
            grab_offset: [10.0, 10.0],
            start_bounds: [100.0, 40.0, 80.0, 24.0],
            from_list: Some("list".into()),
            from_index: Some(0),
            item_type: "card".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let hits0 = collect_hits(&page);
        assert!(tick(110.0, 80.0, &hits0, [400.0, 300.0]));
        assert!(
            (overlay_pos.peek()[0] - 100.0).abs() < 0.5,
            "precondition: overlay X at unscrolled slot; {:?}",
            overlay_pos.peek()
        );

        // Parent ScrollArea panned (autoscroll / smooth chase) with no mousemove.
        use_signal(&scroll_offset_x_signal_key("drag-axis-scroll"), || 0.0_f32).set_silent(60.0);
        assert!(
            pump_autoscroll(&page, [400.0, 300.0]),
            "held-still pump must resync overlay after parent scroll"
        );
        let pos = overlay_pos.peek();
        assert!(
            (pos[0] - 40.0).abs() < 1.0,
            "axis=y overlay X must follow the scrolled slot (100-60); got {:?}",
            pos
        );
        let _ = end(&collect_hits(&page));
    });
}

#[test]
fn end_clears_preview_before_on_end() {
    let (_o, _) = create_root(|| {
        let overlay_open = Signal::new(true);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(Some("a".into()));
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(Some("list".into()));
        let from_index = Signal::new(Some(0usize));
        let hover_list = Signal::new(Some("list".into()));
        let hover_index = Signal::new(Some(1usize));
        let drag_extent = Signal::new([40.0_f32, 20.0]);
        let saw_stale = std::rc::Rc::new(std::cell::Cell::new(false));
        let flag = saw_stale.clone();
        let hover_index_cb = hover_index;
        let overlay_open_cb = overlay_open;
        register_scope(
            "s".into(),
            ScopeRuntime {
                collision: CollisionAlg::PointerWithin,
                axis: Axis::Xy,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: true,
                source_opacity: 1.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: Some(std::rc::Rc::new(std::cell::RefCell::new(
                    move |_: &DragEvent| {
                        if overlay_open_cb.peek() || hover_index_cb.peek().is_some() {
                            flag.set(true);
                        }
                    },
                ))),
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "a".into(),
            pointer: [10.0, 10.0],
            grab_offset: [0.0, 0.0],
            start_bounds: [8.0, 8.0, 40.0, 24.0],
            from_list: Some("list".into()),
            from_index: Some(0),
            item_type: "card".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let drops = vec![hit("zone", [50.0, 50.0, 80.0, 80.0], true)];
        assert!(tick(20.0, 10.0, &drops, [400.0, 300.0]));
        assert!(end(&drops));
        assert!(
            !saw_stale.get(),
            "on_drag_end must not see leftover overlay/hover (rebuild would keep a gap)"
        );
    });
}

#[test]
fn tick_resolves_insert_index_when_collision_hits_container() {
    let (_o, _) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([0.0_f32, 0.0]);
        register_scope(
            "s".into(),
            ScopeRuntime {
                collision: CollisionAlg::MostIntersecting,
                axis: Axis::Y,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: false,
                source_opacity: 1.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "a".into(),
            pointer: [10.0, 5.0],
            grab_offset: [0.0, 0.0],
            start_bounds: [0.0, 0.0, 40.0, 20.0],
            from_list: Some("list".into()),
            from_index: Some(0),
            item_type: "card".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let mut a = hit("a", [0.0, 0.0, 40.0, 20.0], true);
        a.index = Some(0);
        a.is_draggable = true;
        let mut b = hit("b", [0.0, 20.0, 40.0, 20.0], true);
        b.index = Some(1);
        b.is_draggable = true;
        let container = hit("list", [0.0, 0.0, 40.0, 80.0], true);
        let hits = vec![container, a, b];
        assert!(tick(10.0, 28.0, &hits, [400.0, 300.0]));
        assert_eq!(hover_index.peek(), Some(1));
        assert_eq!(hover_list.peek().as_deref(), Some("list"));
        let _ = end(&hits);
    });
}

#[test]
fn cancel_does_not_fire_end() {
    let (_o, _) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([0.0_f32, 0.0]);
        let ended = std::rc::Rc::new(std::cell::Cell::new(false));
        let ended_cb = ended.clone();
        register_scope(
            "s".into(),
            ScopeRuntime {
                collision: CollisionAlg::PointerWithin,
                axis: Axis::Xy,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: false,
                source_opacity: 1.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: Some(std::rc::Rc::new(std::cell::RefCell::new(
                    move |_: &DragEvent| ended_cb.set(true),
                ))),
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "a".into(),
            pointer: [10.0, 10.0],
            grab_offset: [0.0, 0.0],
            start_bounds: [8.0, 8.0, 40.0, 24.0],
            from_list: Some("list".into()),
            from_index: Some(0),
            item_type: "card".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let drops = vec![hit("zone", [50.0, 50.0, 80.0, 80.0], true)];
        assert!(tick(20.0, 10.0, &drops, [400.0, 300.0]));
        assert!(cancel());
        assert!(!is_dragging());
        assert!(!ended.get());
    });
}

#[test]
fn collect_skips_overlay_and_reads_attrs() {
    let mut leaf = Element::new();
    leaf.layout_bounds = [1.0, 2.0, 10.0, 12.0];
    leaf.set_attr(ATTR_DRAG_DROPPABLE, "z");
    leaf.set_attr(ATTR_DRAG_SCOPE, "s");
    let mut ghost = Element::new();
    ghost.set_attr(ATTR_DRAG_OVERLAY, "1");
    ghost.set_attr(ATTR_DRAG_DROPPABLE, "ghost");
    let mut nested = Element::new();
    nested.set_attr(ATTR_DRAG_DROPPABLE, "nested-in-ghost");
    nested.layout_bounds = [0.0, 0.0, 4.0, 4.0];
    ghost = ghost.child(nested);
    let mut root = Element::new();
    root = root.child(leaf).child(ghost);
    let hits = collect_hits(&root);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "z");
}

#[test]
fn collect_droppable_rect_is_clipped_to_visible_ancestor() {
    let mut target = Element::new();
    target.layout_bounds = [-10.0, 10.0, 100.0, 50.0];
    target.set_attr(ATTR_DRAG_DROPPABLE, "dock-body-clipped");
    target.set_attr(ATTR_DRAG_SCOPE, "dock-clipped");
    let mut root = Element::new().overflow("hidden");
    root.layout_bounds = [0.0, 0.0, 80.0, 40.0];
    root = root.child(target);
    let hits = collect_hits(&root);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].rect.to_bounds(), [0.0, 10.0, 80.0, 30.0]);
    assert_eq!(
        last_draggable_bounds("dock-clipped", "dock-body-clipped"),
        Some([-10.0, 10.0, 100.0, 50.0])
    );
}

/// `restrict="parent"` must skip the kit hug-content shell and bind the
/// author container — no id required on that parent.
#[test]
fn collect_hits_records_author_parent_skipping_item_shell() {
    let mut island = Element::new();
    island.layout_bounds = [8.0, 8.0, 40.0, 24.0];
    island.set_attr(ATTR_DRAG_DRAGGABLE, "card");
    island.set_attr(ATTR_DRAG_SCOPE, "s");
    let mut shell = Element::new();
    shell.layout_bounds = [8.0, 8.0, 40.0, 24.0];
    shell.set_attr(ATTR_DRAG_ITEM_SHELL, "1");
    shell = shell.child(island);
    let mut parent = Element::new();
    parent.layout_bounds = [0.0, 0.0, 200.0, 120.0];
    parent = parent.child(shell);
    let hits = collect_hits(&parent);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "card");
    assert_eq!(
        last_parent_bounds("s", "card"),
        Some([0.0, 0.0, 200.0, 120.0]),
        "must skip hug shell; got {:?}",
        last_parent_bounds("s", "card")
    );
}

#[test]
fn singleton_cross_list_hole_keeps_author_gap_through_item_shell() {
    for axis in [Axis::X, Axis::Y] {
        let mut item = Element::new();
        item.layout_bounds = [10.0, 20.0, 40.0, 24.0];
        item.set_attr(ATTR_DRAG_DRAGGABLE, "singleton-gap-card");
        item.set_attr(ATTR_DRAG_SCOPE, "singleton-gap-scope");
        item.set_attr(ATTR_DRAG_LIST, "singleton-gap-list");
        item.set_attr(ATTR_DRAG_INDEX, "0");
        let mut shell = Element::new().child(item);
        shell.set_attr(ATTR_DRAG_ITEM_SHELL, "1");
        let mut parent = Element::new().child(shell);
        parent.style.gap.width = taffy::LengthPercentage::length(4.0);
        parent.style.gap.height = taffy::LengthPercentage::length(6.0);
        let chip = if axis == Axis::X { 40.0 } else { 24.0 };
        let gap = if axis == Axis::X { 4.0 } else { 6.0 };
        let hits = collect_hits(&parent);
        LAST_HITS.with(|h| *h.borrow_mut() = hits.clone());
        DRAG_PACK_PITCH.set(0.0);
        assert_eq!(
            pack_insert_extent(Some("singleton-gap-list"), axis, chip),
            chip + gap
        );
        let hole = insert_hole_origin(&hits, "singleton-gap-list", Some(1), axis, chip).unwrap();
        let advance = if axis == Axis::X {
            hole[0] - 10.0
        } else {
            hole[1] - 20.0
        };
        assert_eq!(
            advance,
            chip + gap,
            "preview and layout must use the same packed row"
        );

        // A subsequent collect replaces authored spacing, including a genuine zero gap.
        parent.style.gap = taffy::Size::zero();
        let hits = collect_hits(&parent);
        assert_eq!(
            row_pitch_along(&hits, Some("singleton-gap-list"), axis, chip),
            chip
        );
    }
}

#[test]
fn insert_index_uses_item_slots_not_container() {
    let mut a = hit("a", [0.0, 0.0, 40.0, 20.0], true);
    a.index = Some(0);
    let mut b = hit("b", [0.0, 20.0, 40.0, 20.0], true);
    b.index = Some(1);
    let container = hit("list", [0.0, 0.0, 40.0, 40.0], true);
    let hits = vec![container, a, b];
    assert_eq!(
        insert_index_along_axis(&hits, "list", [10.0, 5.0], Axis::Y),
        Some(0)
    );
    assert_eq!(
        insert_index_along_axis(&hits, "list", [10.0, 25.0], Axis::Y),
        Some(1)
    );
    assert_eq!(
        insert_index_along_axis(&hits, "list", [10.0, 38.0], Axis::Y),
        Some(2),
        "lower half of the last row inserts after it"
    );
    assert_eq!(
        insert_index_along_axis(&hits, "list", [10.0, 41.0], Axis::Y),
        Some(2),
        "past the last slot bottom appends"
    );
}

#[test]
fn insert_index_past_last_slot_appends() {
    let mut a = hit("a", [0.0, 0.0, 40.0, 20.0], true);
    a.index = Some(0);
    let hits = vec![a];
    assert_eq!(
        insert_index_along_axis(&hits, "list", [10.0, 4.0], Axis::Y),
        Some(0)
    );
    assert_eq!(
        insert_index_along_axis(&hits, "list", [10.0, 8.0], Axis::Y),
        Some(0),
        "upper half of the lone row stays on index 0"
    );
    assert_eq!(
        insert_index_along_axis(&hits, "list", [10.0, 16.0], Axis::Y),
        Some(1),
        "lower half of the lone row appends"
    );
    assert_eq!(
        insert_index_along_axis(&hits, "list", [10.0, 21.0], Axis::Y),
        Some(1)
    );
}

#[test]
fn insert_index_at_source_center_stays_from() {
    let mut a = hit("a", [0.0, 0.0, 80.0, 32.0], true);
    a.index = Some(0);
    let mut b = hit("b", [0.0, 40.0, 80.0, 32.0], true);
    b.index = Some(1);
    let hits = vec![a, b];
    assert_eq!(
        insert_index_along_axis(&hits, "list", [40.0, 16.0], Axis::Y),
        Some(0),
        "grab at row centre must not report hover=1"
    );
    assert_eq!(
        insert_index_along_axis(&hits, "list", [40.0, 41.0], Axis::Y),
        Some(1),
        "entering the next layout slot opens the hole"
    );
}

/// Gallery `also_droppable` island is `w=100%`. Layout can report each row's
/// height as the whole list; using that as `end` swallows every probe into
/// index 0 (or, with a page-tall moving box, past-the-end append).
#[test]
fn insert_index_uses_slot_pitch_not_stretched_island_height() {
    let hits: Vec<DragHit> = (0..8)
        .map(|i| {
            let mut h = hit(
                [
                    "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus", "Neptune",
                ][i],
                [0.0, i as f32 * 40.0, 80.0, 320.0],
                true,
            );
            h.index = Some(i);
            h
        })
        .collect();
    assert_eq!(
        insert_index_along_axis(&hits, "list", [40.0, 16.0], Axis::Y),
        Some(0),
        "pointer in the first pitch must stay on from"
    );
    assert_eq!(
        insert_index_along_axis(&hits, "list", [40.0, 50.0], Axis::Y),
        Some(1),
        "stretched paint height must not keep hover on row 0; got {:?}",
        insert_index_along_axis(&hits, "list", [40.0, 50.0], Axis::Y)
    );
    assert_eq!(
        insert_index_along_axis(&hits, "list", [40.0, 90.0], Axis::Y),
        Some(2)
    );
    assert_eq!(
        insert_index_along_axis(&hits, "list", [40.0, 300.0], Axis::Y),
        Some(7),
        "pointer still in the last pitch must not append"
    );
    assert_eq!(
        insert_index_along_axis(&hits, "list", [40.0, 321.0], Axis::Y),
        Some(8)
    );
}

#[test]
fn insert_index_clustered_slots_do_not_append() {
    let hits: Vec<DragHit> = (0..8)
        .map(|i| {
            let mut h = hit(
                [
                    "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus", "Neptune",
                ][i],
                [0.0, 0.0, 80.0, 32.0],
                true,
            );
            h.index = Some(i);
            h
        })
        .collect();
    assert_eq!(
        insert_index_along_axis(&hits, "list", [40.0, 16.0], Axis::Y),
        None,
        "clustered slots cannot infer pitch; session keeps from_index"
    );
    assert_eq!(
        insert_index_along_axis(&hits, "list", [40.0, 400.0], Axis::Y),
        None,
        "page-offset pointer vs local-Y slots must not report hover=len; got {:?}",
        insert_index_along_axis(&hits, "list", [40.0, 400.0], Axis::Y)
    );
}

/// Gallery dimmed Sortable: auto-height chips can report 2px origin jitter
/// while opacity 0 keeps a real ~40px pitch. Walking a 2px stride always
/// appends — even when the pointer is still on the first 32px row.
#[test]
fn insert_index_jitter_pitch_does_not_append() {
    let hits: Vec<DragHit> = (0..8)
        .map(|i| {
            let mut h = hit(
                [
                    "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus", "Neptune",
                ][i],
                [0.0, i as f32 * 2.0, 80.0, 32.0],
                true,
            );
            h.index = Some(i);
            h
        })
        .collect();
    assert_eq!(
        insert_index_along_axis(&hits, "list", [40.0, 16.0], Axis::Y),
        None,
        "jittered origins cannot infer pitch; session keeps from_index"
    );
    assert_eq!(
        insert_index_along_axis(&hits, "list", [40.0, 50.0], Axis::Y),
        None,
        "jittered origins must not walk to len; got {:?}",
        insert_index_along_axis(&hits, "list", [40.0, 50.0], Axis::Y)
    );
}

#[test]
fn insert_index_skips_collapsed_zero_height_slots() {
    let hits: Vec<DragHit> = (0..8)
        .map(|i| {
            let mut h = hit("x", [0.0, 0.0, 80.0, 0.0], true);
            h.index = Some(i);
            h
        })
        .collect();
    assert_eq!(
        insert_index_along_axis(&hits, "list", [40.0, 16.0], Axis::Y),
        None,
        "0-height rows must not fall through to append (Gallery: 松手永远进队尾)"
    );
}

#[test]
fn slot_stride_includes_gap_between_consecutive_items() {
    let mut a = hit("a", [0.0, 0.0, 40.0, 20.0], true);
    a.index = Some(0);
    let mut b = hit("b", [0.0, 28.0, 40.0, 20.0], true);
    b.index = Some(1);
    let hits = vec![a, b];
    assert_eq!(
        slot_stride_along_axis(&hits, "list", Axis::Y, 20.0),
        28.0,
        "preview shift must use slot pitch (20 item + 8 gap), not box height"
    );
    assert_eq!(
        slot_stride_along_axis(&hits, "missing", Axis::Y, 20.0),
        20.0
    );
}

#[test]
fn cap_stretched_keeps_naturally_unequal_folder_heights() {
    let mut inbox = hit("folder:Inbox", [0.0, 0.0, 200.0, 200.0], true);
    inbox.index = Some(0);
    inbox.list_id = Some("tree-root".into());
    let mut later = hit("folder:Later", [0.0, 208.0, 200.0, 80.0], true);
    later.index = Some(1);
    later.list_id = Some("tree-root".into());
    let mut hits = vec![inbox, later];
    cap_stretched_indexed_rects(&mut hits);
    assert!(
        (hits[0].rect.height - 200.0).abs() < 0.5,
        "tall folder must keep paint height, got {}",
        hits[0].rect.height
    );
    assert!(
        (hits[1].rect.height - 80.0).abs() < 0.5,
        "short last folder must not inherit the previous stride, got {}",
        hits[1].rect.height
    );
}

#[test]
fn unchanged_insert_slot_ignores_vacated_layout_hits() {
    for axis in [Axis::X, Axis::Y] {
        let mut source = hit("Move me", [0.0, 0.0, 40.0, 16.0], true);
        source.index = Some(0);
        let mut peer = hit("Locked", [0.0, 0.0, 40.0, 16.0], true);
        peer.index = Some(1);
        for index in 0..2 {
            assert_eq!(
                pack_item_translate(
                    &[source.clone(), peer.clone()],
                    Some("list"),
                    Some(index),
                    Some("list"),
                    Some(0),
                    Some("list"),
                    Some(0),
                    axis,
                ),
                [0.0, 0.0],
                "unchanged order must not introduce a gap from stale hits"
            );
        }
    }
}

#[test]
fn pack_variable_height_reorder_preserves_span() {
    let mut inbox = hit("folder:Inbox", [0.0, 0.0, 200.0, 200.0], true);
    inbox.index = Some(0);
    inbox.list_id = Some("tree-root".into());
    let mut later = hit("folder:Later", [0.0, 208.0, 200.0, 80.0], true);
    later.index = Some(1);
    later.list_id = Some("tree-root".into());
    let hits = vec![inbox, later];
    let later_up = pack_item_translate(
        &hits,
        Some("tree-root"),
        Some(0),
        Some("tree-root"),
        Some(1),
        Some("tree-root"),
        Some(0),
        Axis::Y,
    );
    assert!(
        (later_up[1] - 88.0).abs() < 1.0,
        "Inbox must shift by Later's slot (80+8 gap), not Inbox's 208 stride; got {}",
        later_up[1]
    );
    let later_self = pack_item_translate(
        &hits,
        Some("tree-root"),
        Some(1),
        Some("tree-root"),
        Some(1),
        Some("tree-root"),
        Some(0),
        Axis::Y,
    );
    assert!(
        (later_self[1] + 208.0).abs() < 1.0,
        "Later moves from 208 to 0; got {}",
        later_self[1]
    );
    let packed = pack_same_list_origins(&hits, "tree-root", 1, 0, Axis::Y);
    let later_y = packed.get(&1).unwrap()[1];
    let inbox_y = packed.get(&0).unwrap()[1];
    assert!((later_y - 0.0).abs() < 0.5);
    assert!(
        (inbox_y - 88.0).abs() < 1.0,
        "Inbox packed under Later+gap, got {inbox_y}"
    );
    let span = inbox_y + 200.0;
    assert!(
        (span - 288.0).abs() < 1.0,
        "reorder must not grow the list; span={span}"
    );
}

#[test]
fn slot_extent_uses_each_row() {
    let mut a = hit("a", [0.0, 0.0, 40.0, 200.0], true);
    a.index = Some(0);
    let mut b = hit("b", [0.0, 208.0, 40.0, 80.0], true);
    b.index = Some(1);
    let hits = vec![a, b];
    assert!((slot_extent_along_axis(&hits, "list", Some(0), Axis::Y, 80.0) - 208.0).abs() < 0.5);
    assert!((slot_extent_along_axis(&hits, "list", Some(1), Axis::Y, 80.0) - 88.0).abs() < 0.5);
}

#[test]
fn cross_list_source_translate_lands_on_target_hole() {
    let mut src = hit("src", [0.0, 0.0, 80.0, 32.0], true);
    src.list_id = Some("a".into());
    src.index = Some(0);
    let mut dst = hit("dst", [200.0, 0.0, 80.0, 32.0], true);
    dst.list_id = Some("b".into());
    dst.index = Some(0);
    let mut col_b = hit("col-b", [200.0, 0.0, 80.0, 100.0], true);
    col_b.list_id = Some("b".into());
    col_b.index = None;
    let hits = [src, dst, col_b];
    assert_eq!(
        source_translate_to_insert_hole(&hits, "a", 0, "b", Some(0), Axis::Y),
        [200.0, 0.0]
    );
    assert_eq!(
        source_translate_to_insert_hole(&hits, "a", 0, "b", Some(1), Axis::Y),
        [200.0, 32.0],
        "append hole is last origin + fallback extent"
    );
}

/// `restrict="corners-arena"`: ghost stays inside the named droppable, not
/// the window — Gallery 四角旋转 must not leave the dashed box.
#[test]
fn tick_restrict_element_clamps_ghost_to_named_box() {
    let (_o, _) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([40.0_f32, 24.0]);
        register_scope(
            "s".into(),
            ScopeRuntime {
                collision: CollisionAlg::PointerWithin,
                axis: Axis::Xy,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::Element("arena".into()),
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: true,
                source_opacity: 0.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "card".into(),
            pointer: [100.0, 52.0],
            grab_offset: [20.0, 12.0],
            start_bounds: [80.0, 40.0, 40.0, 24.0],
            from_list: None,
            from_index: None,
            item_type: "chip".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let mut card = hit("card", [80.0, 40.0, 40.0, 24.0], false);
        card.scope_id = "s".into();
        card.is_draggable = true;
        let mut arena = hit("arena", [0.0, 0.0, 200.0, 120.0], true);
        arena.scope_id = "s".into();
        let hits = vec![card, arena];
        assert!(tick(500.0, 500.0, &hits, [800.0, 600.0]));
        let pos = overlay_pos.peek();
        assert!(
            (pos[0] - 160.0).abs() < 0.5 && (pos[1] - 96.0).abs() < 0.5,
            "ghost must clamp to arena (200-40, 120-24), not the 800x600 viewport; got {pos:?}"
        );
        let _ = end(&hits);
        remember_item_restrict("s", "card", Some(Restrict::Viewport));
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "card".into(),
            pointer: [100.0, 52.0],
            grab_offset: [20.0, 12.0],
            start_bounds: [80.0, 40.0, 40.0, 24.0],
            from_list: None,
            from_index: None,
            item_type: "chip".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(tick(900.0, 900.0, &hits, [800.0, 600.0]));
        let pos = overlay_pos.peek();
        remember_item_restrict("s", "card", None);
        let _ = end(&hits);
        assert!(
            (pos[0] - 760.0).abs() < 0.5 && (pos[1] - 576.0).abs() < 0.5,
            "item restrict=viewport overrides the arena and uses the window; got {pos:?}"
        );
    });
}

#[test]
fn restrict_overflow_hidden_records_clip_box() {
    let (_o, _) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([40.0_f32, 24.0]);
        register_scope(
            "s".into(),
            ScopeRuntime {
                collision: CollisionAlg::PointerWithin,
                axis: Axis::Xy,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::Element("arena".into()),
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Hidden,
                overlay_registered: true,
                source_opacity: 0.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        let mut card = hit("card", [80.0, 40.0, 40.0, 24.0], false);
        card.scope_id = "s".into();
        card.is_draggable = true;
        let mut arena = hit("arena", [10.0, 20.0, 200.0, 120.0], true);
        arena.scope_id = "s".into();
        let hits = vec![card, arena];
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "card".into(),
            pointer: [100.0, 52.0],
            grab_offset: [20.0, 12.0],
            start_bounds: [80.0, 40.0, 40.0, 24.0],
            from_list: None,
            from_index: None,
            item_type: "chip".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(tick(500.0, 500.0, &hits, [800.0, 600.0]));
        let clip = peek_restrict_clip().expect("hidden overflow stores the restrict box");
        assert!(
            (clip[0] - 10.0).abs() < 0.5
                && (clip[1] - 20.0).abs() < 0.5
                && (clip[2] - 200.0).abs() < 0.5
                && (clip[3] - 120.0).abs() < 0.5,
            "clip must be the arena box, got {clip:?}"
        );
        let _ = end(&hits);
        assert!(peek_restrict_clip().is_none());
    });
}

#[test]
fn restrict_overflow_hidden_clip_follows_scrolled_arena() {
    let (_o, _) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([40.0_f32, 24.0]);
        register_scope(
            "s".into(),
            ScopeRuntime {
                collision: CollisionAlg::PointerWithin,
                axis: Axis::Xy,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::Element("arena".into()),
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Hidden,
                overlay_registered: true,
                source_opacity: 0.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        let mut card = hit("card", [80.0, 40.0, 40.0, 24.0], false);
        card.scope_id = "s".into();
        card.is_draggable = true;
        let mut arena = hit("arena", [10.0, 20.0, 200.0, 120.0], true);
        arena.scope_id = "s".into();
        let hits = vec![card.clone(), arena];
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "card".into(),
            pointer: [100.0, 52.0],
            grab_offset: [20.0, 12.0],
            start_bounds: [80.0, 40.0, 40.0, 24.0],
            from_list: None,
            from_index: None,
            item_type: "chip".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(tick(110.0, 62.0, &hits, [800.0, 600.0]));
        let clip0 = peek_restrict_clip().expect("hidden overflow stores the restrict box");
        assert!(
            (clip0[1] - 20.0).abs() < 0.5,
            "initial clip y must be the arena, got {clip0:?}"
        );

        let mut arena_scrolled = hit("arena", [10.0, -60.0, 200.0, 120.0], true);
        arena_scrolled.scope_id = "s".into();
        let mut card_scrolled = card.clone();
        card_scrolled.rect = Rect::from_bounds([80.0, -40.0, 40.0, 24.0]);
        let scrolled = vec![card_scrolled, arena_scrolled];
        assert!(tick(110.0, 62.0, &scrolled, [800.0, 600.0]));
        let clip1 = peek_restrict_clip().expect("clip must survive page scroll");
        assert!(
            (clip1[0] - 10.0).abs() < 0.5
                && (clip1[1] + 60.0).abs() < 0.5
                && (clip1[2] - 200.0).abs() < 0.5
                && (clip1[3] - 120.0).abs() < 0.5,
            "clip shell must track the scrolled arena, got {clip1:?}"
        );
        let pos = overlay_pos.peek();
        assert!(
            pos[1] + 0.5 >= clip1[1] && pos[1] <= clip1[1] + clip1[3] + 0.5,
            "ghost origin must stay inside the scrolled restrict box, pos={pos:?} clip={clip1:?}"
        );

        assert!(tick(110.0, 62.0, &[], [800.0, 600.0]));
        let clip_kept = peek_restrict_clip().expect("failed lookup must keep last clip");
        assert_eq!(clip_kept, clip1, "stale hits must not drop Hidden clip");

        assert!(tick(110.0, 62.0, &hits, [800.0, 600.0]));
        let clip2 = peek_restrict_clip().expect("clip restores after scroll-back");
        assert!(
            (clip2[1] - 20.0).abs() < 0.5,
            "scroll-back must restore the arena clip, got {clip2:?}"
        );
        let _ = end(&hits);
    });
}

#[test]
fn restrict_overflow_hidden_clip_intersects_ancestor_scroll() {
    use crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX;
    let (_o, _) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([40.0_f32, 24.0]);
        register_scope(
            "s".into(),
            ScopeRuntime {
                collision: CollisionAlg::PointerWithin,
                axis: Axis::Xy,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::Element("arena".into()),
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Hidden,
                overlay_registered: true,
                source_opacity: 0.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        let mut card = Element::new();
        card.layout_bounds = [80.0, 100.0, 40.0, 24.0];
        card.set_attr(ATTR_DRAG_DRAGGABLE, "card");
        card.set_attr(ATTR_DRAG_SCOPE, "s");
        let mut arena = Element::new();
        arena.layout_bounds = [10.0, 20.0, 200.0, 240.0];
        arena.set_attr(ATTR_DRAG_DROPPABLE, "arena");
        arena.set_attr(ATTR_DRAG_SCOPE, "s");
        arena = arena.child(card);
        let mut vp = Element::new().overflow("hidden");
        vp.layout_bounds = [0.0, 80.0, 400.0, 200.0];
        vp.focus_id = Some(format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}page"));
        vp = vp.child(arena);
        let hits = collect_hits(&vp);
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "card".into(),
            pointer: [100.0, 110.0],
            grab_offset: [20.0, 10.0],
            start_bounds: [80.0, 100.0, 40.0, 24.0],
            from_list: None,
            from_index: None,
            item_type: "chip".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(tick(110.0, 120.0, &hits, [800.0, 600.0]));
        let clip = peek_restrict_clip().expect("hidden clip");
        assert!(
            (clip[0] - 10.0).abs() < 0.5 && (clip[1] - 20.0).abs() < 0.5,
            "clip shell must stay on the arena, not the scroll viewport; got {clip:?}"
        );
        let ancestor = peek_restrict_ancestor_clip().expect("page overflow");
        assert!(
            (ancestor[1] - 80.0).abs() < 0.5,
            "ancestor clip must be the scroll viewport, got {ancestor:?}"
        );
        let visible = intersect_xywh(clip, ancestor);
        assert!(
            (visible[1] - 80.0).abs() < 0.5 && visible[0] >= 10.0 - 0.5,
            "visible scissor must start at the viewport and keep the arena left, got {visible:?}"
        );
        assert!(
            visible[0] + 0.5 >= 10.0,
            "rotation must not paint in the padding left of the arena; visible={visible:?}"
        );
        let _ = end(&hits);
    });
}

/// `restrict="parent"`: ghost stays inside the author box recorded by
/// [`collect_hits`], not the window and not a named id.
#[test]
fn tick_restrict_parent_clamps_ghost_to_author_box() {
    let (_o, _) = create_root(|| {
        let committed_delta = Rc::new(RefCell::new(None));
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([40.0_f32, 24.0]);
        register_scope(
            "s".into(),
            ScopeRuntime {
                collision: CollisionAlg::PointerWithin,
                axis: Axis::Xy,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::Parent,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: true,
                source_opacity: 0.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: Some(Rc::new(RefCell::new({
                    let committed_delta = committed_delta.clone();
                    move |ev: &DragEvent| *committed_delta.borrow_mut() = Some(ev.delta)
                }))),
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        let mut island = Element::new();
        island.layout_bounds = [80.0, 40.0, 40.0, 24.0];
        island.set_attr(ATTR_DRAG_DRAGGABLE, "card");
        island.set_attr(ATTR_DRAG_SCOPE, "s");
        let mut shell = Element::new();
        shell.layout_bounds = [80.0, 40.0, 40.0, 24.0];
        shell.set_attr(ATTR_DRAG_ITEM_SHELL, "1");
        shell = shell.child(island);
        let mut parent = Element::new();
        parent.layout_bounds = [0.0, 0.0, 200.0, 120.0];
        parent = parent.child(shell);
        let hits = collect_hits(&parent);
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "card".into(),
            pointer: [100.0, 52.0],
            grab_offset: [20.0, 12.0],
            start_bounds: [80.0, 40.0, 40.0, 24.0],
            from_list: None,
            from_index: None,
            item_type: "chip".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(tick(200.0, 108.0, &hits, [800.0, 600.0]));
        assert!(tick_follow(500.0, 500.0, [800.0, 600.0]));
        let pos = overlay_pos.peek();
        let _ = end(&hits);
        assert!(
                (pos[0] - 160.0).abs() < 0.5 && (pos[1] - 96.0).abs() < 0.5,
                "ghost must clamp to author parent (200-40, 120-24), not the window or hug shell; got {pos:?}"
            );
        let delta = committed_delta.borrow().expect("active drag must commit");
        assert_eq!(
            [80.0 + delta[0], 40.0 + delta[1]],
            pos,
            "drop must persist the restricted preview after dragging beyond the edge"
        );

        // Offset edges deliberately fall between snap points. A second
        // snap in a drop handler could push the committed tile outside.
        use super::super::geometry::SnapTargets;
        let arena = [5.0, 7.0, 200.0, 120.0];
        parent.layout_bounds = arena;
        parent.set_attr("id", "arena");
        let hits = collect_hits(&parent);
        for (restrict, bounds) in [
            (Restrict::Parent, arena),
            (Restrict::Rect(arena), arena),
            (Restrict::Element("arena".into()), arena),
            (Restrict::Viewport, [0.0, 0.0, 205.0, 127.0]),
        ] {
            for (kind, snap) in [
                ("none", None),
                (
                    "grid",
                    Some(Snap::grid(SnapGrid::new(32.0, 32.0).offset([16.0, 16.0]))),
                ),
                (
                    "end-grid",
                    Some(Snap::grid(
                        SnapGrid::new(32.0, 32.0)
                            .offset([16.0, 16.0])
                            .end_only(true),
                    )),
                ),
                (
                    "targets",
                    Some(Snap::targets(
                        SnapTargets::new([
                            SnapGrid::new(8.0, 8.0),
                            SnapGrid::new(32.0, 32.0).offset([16.0, 16.0]),
                        ])
                        .range(14.0),
                    )),
                ),
                (
                    "fn",
                    Some(Snap::targets(
                        SnapTargets::default().with_fn(Rc::new(|x, _| Some([x, 72.0]))),
                    )),
                ),
            ] {
                for overlay_registered in [false, true] {
                    for pointer in [
                        [-500.0, -500.0],
                        [-500.0, 500.0],
                        [500.0, -500.0],
                        [500.0, 500.0],
                    ] {
                        SCOPES.with(|scopes| {
                            let mut scopes = scopes.borrow_mut();
                            let rt = scopes.get_mut("s").unwrap();
                            rt.restrict = restrict.clone();
                            rt.snap = snap.clone();
                            rt.overlay_registered = overlay_registered;
                        });
                        begin(BeginDrag {
                            scope_id: "s".into(),
                            draggable_id: "card".into(),
                            pointer: [100.0, 52.0],
                            grab_offset: [20.0, 12.0],
                            start_bounds: [80.0, 40.0, 40.0, 24.0],
                            from_list: None,
                            from_index: None,
                            item_type: "chip".into(),
                            group: String::new(),
                            capture_id: "cap".into(),
                        });
                        assert!(tick(101.0, 53.0, &hits, [205.0, 127.0]));
                        assert!(tick_follow(pointer[0], pointer[1], [205.0, 127.0]));
                        let expected = [
                            if pointer[0] < 0.0 {
                                bounds[0]
                            } else {
                                bounds[0] + bounds[2] - 40.0
                            },
                            if kind == "fn" {
                                72.0
                            } else if pointer[1] < 0.0 {
                                bounds[1]
                            } else {
                                bounds[1] + bounds[3] - 24.0
                            },
                        ];
                        if kind != "end-grid" {
                            assert_eq!(
                                overlay_pos.peek(),
                                expected,
                                "restricted preview: {restrict:?}, {kind}, {pointer:?}"
                            );
                        }
                        assert!(end(&hits));
                        let delta = committed_delta.borrow().unwrap();
                        assert_eq!(
                                [80.0 + delta[0], 40.0 + delta[1]],
                                expected,
                                "restricted drop: {restrict:?}, {kind}, {pointer:?}, overlay={overlay_registered}"
                            );
                    }
                }
            }
        }
    });
}

#[test]
fn empty_target_hole_uses_container_origin() {
    let mut src = hit("src", [0.0, 0.0, 80.0, 32.0], true);
    src.list_id = Some("a".into());
    src.index = Some(0);
    let mut col_b = hit("col-b", [200.0, 8.0, 80.0, 100.0], true);
    col_b.list_id = Some("b".into());
    let hits = [src, col_b];
    assert_eq!(
        source_translate_to_insert_hole(&hits, "a", 0, "b", Some(0), Axis::Y),
        [200.0, 8.0]
    );
}

#[test]
fn midpoint_hover_apply_lands_in_opened_slot() {
    use crate::utils::drag_reorder::apply_reorder_vec;
    let mk = |id: &str, y: f32, i: usize| {
        let mut h = hit(id, [0.0, y, 40.0, 20.0], true);
        h.index = Some(i);
        h
    };
    let hits = vec![
        mk("a", 0.0, 0),
        mk("b", 20.0, 1),
        mk("c", 40.0, 2),
        mk("d", 60.0, 3),
    ];
    // Inside `c` (40..60). Past `c`'s bottom (61) appends toward `d`.
    let hover = insert_index_along_axis(&hits, "list", [10.0, 61.0], Axis::Y);
    assert_eq!(hover, Some(3));
    let mut v = vec![0, 1, 2, 3];
    apply_reorder_vec(&mut v, 0, hover.unwrap());
    assert_eq!(
        v,
        vec![1, 2, 3, 0],
        "commit must occupy the preview hole at hover, not hover-1"
    );
    let hover2 = insert_index_along_axis(&hits, "list", [10.0, 25.0], Axis::Y);
    assert_eq!(hover2, Some(1));
    apply_reorder_vec(&mut v, 3, hover2.unwrap());
    assert_eq!(v, vec![1, 0, 2, 3]);
    let mut sorted = v.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, vec![0, 1, 2, 3]);
}

/// Gallery hosts Drag in `ScrollArea`. Pointer / `current_bounds` are visual;
/// `layout_bounds` are pre-scroll. Hits used for overlay_pos + collision must
/// match the pointer, or the ghost is pinned below the fold (user: 浮层丢失).
#[test]
fn collect_hits_rect_is_visual_inside_scrolled_viewport() {
    use crate::core::reactivity::use_signal;
    use crate::core::scroll_viewport::{scroll_offset_y_signal_key, SCROLL_VIEWPORT_FOCUS_PREFIX};

    let (_o, (layout_y, hit_y, grab_y, start_y)) = create_root(|| {
        use_signal(&scroll_offset_y_signal_key("drag-scroll"), || 0.0_f32).set_silent(400.0);
        let mut chip = Element::new()
            .w(80.0)
            .h(32.0)
            .min_h(32.0)
            .flex_shrink(0.0)
            .bg([0.9, 0.9, 0.9, 1.0]);
        chip.set_attr(ATTR_DRAG_DRAGGABLE, "chip");
        chip.set_attr(ATTR_DRAG_SCOPE, "s");
        let mut vp = Element::new()
            .w(200.0)
            .h(120.0)
            .flex_col()
            .overflow_y("hidden")
            .overflow_x("hidden")
            .child(
                Element::new()
                    .w(80.0)
                    .h(400.0)
                    .min_h(400.0)
                    .flex_shrink(0.0),
            )
            .child(chip);
        vp.focus_id = Some(format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}drag-scroll"));
        vp.scroll_offset = [0.0, 400.0];
        let mut root = Element::new().w(200.0).h(120.0).child(vp);
        let mut tree = crate::layout::LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 200.0, 120.0, [0.0, 0.0]);

        fn find_chip(el: &Element) -> Option<&Element> {
            if el.attrs.get(ATTR_DRAG_DRAGGABLE).map(String::as_str) == Some("chip") {
                Some(el)
            } else {
                el.children.iter().find_map(find_chip)
            }
        }
        let chip = find_chip(&root).expect("chip");
        let layout_y = chip.layout_bounds[1];
        let hits = collect_hits(&root);
        let hit = hits.iter().find(|h| h.id == "chip").expect("hit");
        let pointer = [hit.rect.x + 10.0, hit.rect.y + 8.0];
        let (grab, start) = start_geometry("s", "chip", pointer, [0.0; 4]);
        (layout_y, hit.rect.y, grab[1], start[1])
    });
    assert!(
        layout_y > 300.0,
        "precondition: item layout_bounds stay in content space; layout_y={layout_y}"
    );
    assert!(
            (hit_y - (layout_y - 400.0)).abs() < 3.0,
            "collect_hits must use visual box (layout - scroll), not layout_bounds; layout_y={layout_y} hit_y={hit_y}"
        );
    assert!(
        (start_y - hit_y).abs() < 1.0,
        "start_geometry cache must match the visual hit box; start_y={start_y} hit_y={hit_y}"
    );
    assert!(
        (grab_y - 8.0).abs() < 2.0,
        "grab offset vs visual pointer must not include scroll; grab_y={grab_y}"
    );
}

/// Sortable preview translates siblings to open a gap. Collision / insert
/// index must keep the **layout slots** (scroll-adjusted, no CSS translate),
/// or `hover_index` chases the moving boxes and live reordering vanishes.
#[test]
fn collect_hits_ignores_preview_translate() {
    let mut item = Element::new();
    item.layout_bounds = [0.0, 40.0, 80.0, 32.0];
    item.style.translate = [0.0, -32.0];
    item.set_attr(ATTR_DRAG_DRAGGABLE, "venus");
    item.set_attr(ATTR_DRAG_DROPPABLE, "venus");
    item.set_attr(ATTR_DRAG_LIST, "planets");
    item.set_attr(ATTR_DRAG_INDEX, "1");
    let hits = collect_hits(&item);
    assert_eq!(hits.len(), 1);
    assert!(
        (hits[0].rect.y - 40.0).abs() < 0.5,
        "sortable collision slot must ignore preview translate; got y={}",
        hits[0].rect.y
    );
    assert!(
        (hits[0].rect.height - 32.0).abs() < 0.5,
        "slot height must stay the laid-out size; got h={}",
        hits[0].rect.height
    );
}

#[test]
fn tick_hover_index_uses_slot_not_translated_paint() {
    let (_o, hover) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([0.0_f32, 0.0]);
        register_scope(
            "s".into(),
            ScopeRuntime {
                collision: CollisionAlg::ClosestCenter,
                axis: Axis::Y,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: false,
                source_opacity: 1.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "a".into(),
            pointer: [10.0, 5.0],
            grab_offset: [0.0, 0.0],
            start_bounds: [0.0, 0.0, 40.0, 20.0],
            from_list: Some("list".into()),
            from_index: Some(0),
            item_type: "card".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let mut a = Element::new();
        a.layout_bounds = [0.0, 0.0, 40.0, 20.0];
        a.set_attr(ATTR_DRAG_DRAGGABLE, "a");
        a.set_attr(ATTR_DRAG_DROPPABLE, "a");
        a.set_attr(ATTR_DRAG_LIST, "list");
        a.set_attr(ATTR_DRAG_INDEX, "0");
        a.set_attr(ATTR_DRAG_SCOPE, "s");
        a.set_attr(ATTR_DRAG_TYPE, "card");
        let mut b = Element::new();
        b.layout_bounds = [0.0, 20.0, 40.0, 20.0];
        b.style.translate = [0.0, -20.0];
        b.set_attr(ATTR_DRAG_DRAGGABLE, "b");
        b.set_attr(ATTR_DRAG_DROPPABLE, "b");
        b.set_attr(ATTR_DRAG_LIST, "list");
        b.set_attr(ATTR_DRAG_INDEX, "1");
        b.set_attr(ATTR_DRAG_SCOPE, "s");
        b.set_attr(ATTR_DRAG_TYPE, "card");
        let mut root = Element::new();
        root = root.child(a).child(b);
        let hits = collect_hits(&root);
        assert!(tick(10.0, 28.0, &hits, [400.0, 300.0]));
        let hover = hover_index.peek();
        let _ = end(&hits);
        hover
    });
    assert_eq!(
        hover,
        Some(1),
        "pointer over b's layout slot must keep hover=1 even if b has already shifted"
    );
}

fn drop_hit(id: &str, scope: &str, list: &str, index: Option<usize>, bounds: [f32; 4]) -> DragHit {
    DragHit {
        id: id.into(),
        scope_id: scope.into(),
        is_draggable: index.is_some(),
        is_droppable: true,
        rect: Rect::from_bounds(bounds),
        item_type: "planet".into(),
        accept: "planet".into(),
        list_id: Some(list.into()),
        index,
        group: String::new(),
        disabled: false,
        list_axis: None,
        skip_shift: false,
        skip_follow: false,
        pull_clone: false,
    }
}

#[test]
fn tick_single_column_closest_center_does_not_jump_hover_to_mid_list() {
    let (_o, (at_lift, at_half, at_tail)) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([80.0_f32, 320.0]);
        register_scope(
            "gal-drag-sort".into(),
            ScopeRuntime {
                collision: CollisionAlg::ClosestCenter,
                axis: Axis::Y,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: true,
                source_opacity: 1.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        begin(BeginDrag {
            scope_id: "gal-drag-sort".into(),
            draggable_id: "Mercury".into(),
            pointer: [40.0, 16.0],
            grab_offset: [40.0, 16.0],
            // Stretched island / list box (not the 32px row). Using this
            // center for insert-index was Gallery: 拖起塌陷, 拖到一半才出现.
            start_bounds: [0.0, 0.0, 80.0, 320.0],
            from_list: Some("planets".into()),
            from_index: Some(0),
            item_type: "planet".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let names = [
            "Mercury", "Venus", "Earth", "Jupiter", "Uranus", "Mars", "Neptune", "Saturn",
        ];
        let mut hits: Vec<DragHit> = names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                drop_hit(
                    name,
                    "gal-drag-sort",
                    "planets",
                    Some(i),
                    [0.0, i as f32 * 40.0, 80.0, 32.0],
                )
            })
            .collect();
        hits.push(drop_hit(
            "planets",
            "gal-drag-sort",
            "planets",
            None,
            [0.0, 0.0, 80.0, 320.0],
        ));
        assert!(tick(41.0, 16.0, &hits, [800.0, 600.0]));
        let at_lift = hover_index.peek();
        assert!(tick(40.0, 176.0, &hits, [800.0, 600.0]));
        let at_half = hover_index.peek();
        assert!(tick(40.0, 300.0, &hits, [800.0, 600.0]));
        let at_tail = hover_index.peek();
        let _ = end(&hits);
        (at_lift, at_half, at_tail)
    });
    assert_eq!(
        at_lift,
        Some(0),
        "lift on row 0 must not report hover≈len/2 from a stretched start box; got {at_lift:?}"
    );
    assert_eq!(
        at_half,
        Some(4),
        "pointer at mid-list should open the mid slot, not stay stuck or append; got {at_half:?}"
    );
    assert_eq!(
        at_tail,
        Some(7),
        "pointer still inside the last slot must not drop the body (append); got {at_tail:?}"
    );
}

/// Gallery: each `also_droppable` island can report the list height. Probing
/// `dragged.center()` (origin + 160) then always sits past the last 32px
/// sibling — hover=len, origin closes, dimmed body is translated below the
/// fold, drop appends (user: 半透明本体丢失, 松手永远进队尾).
#[test]
fn tick_stretched_island_hits_follow_pointer_not_box_center() {
    let (_o, (at_lift, at_earth, at_tail)) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([80.0_f32, 320.0]);
        register_scope(
            "gal-drag-sort".into(),
            ScopeRuntime {
                collision: CollisionAlg::ClosestCenter,
                axis: Axis::Y,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: true,
                source_opacity: 1.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        begin(BeginDrag {
            scope_id: "gal-drag-sort".into(),
            draggable_id: "Mercury".into(),
            pointer: [40.0, 16.0],
            grab_offset: [40.0, 16.0],
            start_bounds: [0.0, 0.0, 80.0, 320.0],
            from_list: Some("planets".into()),
            from_index: Some(0),
            item_type: "planet".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let names = [
            "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus", "Neptune",
        ];
        let mut hits: Vec<DragHit> = names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                drop_hit(
                    name,
                    "gal-drag-sort",
                    "planets",
                    Some(i),
                    [0.0, i as f32 * 40.0, 80.0, 320.0],
                )
            })
            .collect();
        hits.push(drop_hit(
            "planets",
            "gal-drag-sort",
            "planets",
            None,
            [0.0, 0.0, 80.0, 320.0],
        ));
        assert!(tick(41.0, 16.0, &hits, [800.0, 600.0]));
        let at_lift = hover_index.peek();
        assert!(tick(40.0, 90.0, &hits, [800.0, 600.0]));
        let at_earth = hover_index.peek();
        assert!(tick(40.0, 300.0, &hits, [800.0, 600.0]));
        let at_tail = hover_index.peek();
        let _ = end(&hits);
        (at_lift, at_earth, at_tail)
    });
    assert_eq!(
        at_lift,
        Some(0),
        "lift on row 0 must keep hover=from; got {at_lift:?}"
    );
    assert_eq!(
        at_earth,
        Some(2),
        "pointer on Earth must open slot 2, not stay on 0 or append; got {at_earth:?}"
    );
    assert_eq!(
        at_tail,
        Some(7),
        "pointer in the last pitch must not append; got {at_tail:?}"
    );
}

/// Gallery: pointer is in window space; SortableItem slots can all report
/// the island origin (local Y = 0). Walking those slots then always
/// appends (`hover=len`), the dimmed body is translated out of the
/// ScrollArea, and `on_drag_end` parks the row at the tail.
#[test]
fn tick_clustered_local_slots_do_not_append_or_drop_body() {
    let committed = std::rc::Rc::new(std::cell::Cell::new(None::<Option<usize>>));
    let committed_end = committed.clone();
    let (_o, (at_lift, at_move)) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([80.0_f32, 32.0]);
        register_scope(
            "gal-drag-sort".into(),
            ScopeRuntime {
                collision: CollisionAlg::ClosestCenter,
                axis: Axis::Y,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: true,
                source_opacity: 1.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: Some(std::rc::Rc::new(std::cell::RefCell::new(
                    move |ev: &DragEvent| {
                        committed_end.set(Some(ev.to_index));
                    },
                ))),
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        begin(BeginDrag {
            scope_id: "gal-drag-sort".into(),
            draggable_id: "Mercury".into(),
            pointer: [40.0, 416.0],
            grab_offset: [40.0, 16.0],
            start_bounds: [0.0, 400.0, 80.0, 32.0],
            from_list: Some("planets".into()),
            from_index: Some(0),
            item_type: "planet".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let names = [
            "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus", "Neptune",
        ];
        let mut hits: Vec<DragHit> = names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                drop_hit(
                    name,
                    "gal-drag-sort",
                    "planets",
                    Some(i),
                    [0.0, 0.0, 80.0, 32.0],
                )
            })
            .collect();
        hits.push(drop_hit(
            "planets",
            "gal-drag-sort",
            "planets",
            None,
            [0.0, 0.0, 80.0, 176.0],
        ));
        assert!(tick(41.0, 416.0, &hits, [800.0, 600.0]));
        let at_lift = hover_index.peek();
        assert!(tick(40.0, 480.0, &hits, [800.0, 600.0]));
        let at_move = hover_index.peek();
        assert!(end(&hits));
        (at_lift, at_move)
    });
    assert_eq!(
        at_lift,
        Some(0),
        "lift must keep hover=from, not len; got {at_lift:?}"
    );
    assert_eq!(
        at_move,
        Some(0),
        "moving a page-offset pointer vs local slots must not append; got {at_move:?}"
    );
    assert_eq!(
        committed.get(),
        Some(Some(0)),
        "commit must stay on from, not the tail; got {:?}",
        committed.get()
    );
}

#[test]
fn foreign_scope_hits_do_not_steal_sortable_hover() {
    let (_o, (at_lift, at_earth)) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([80.0_f32, 32.0]);
        register_scope(
            "gal-drag-sort".into(),
            ScopeRuntime {
                collision: CollisionAlg::ClosestCenter,
                axis: Axis::Y,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: true,
                source_opacity: 1.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        begin(BeginDrag {
            scope_id: "gal-drag-sort".into(),
            draggable_id: "Mercury".into(),
            pointer: [40.0, 416.0],
            grab_offset: [40.0, 16.0],
            start_bounds: [0.0, 400.0, 80.0, 32.0],
            from_list: Some("planets".into()),
            from_index: Some(0),
            item_type: "planet".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        // Hide-source list stacked above (same planet names). Its last row is
        // closer to the pointer than planets[1] if the session does not
        // isolate scopes — that was Gallery 半透明本体 collapsing.
        let hits = vec![
            drop_hit(
                "Mercury",
                "gal-drag-sort-hide",
                "planets-hide",
                Some(0),
                [0.0, 0.0, 80.0, 32.0],
            ),
            drop_hit(
                "Earth",
                "gal-drag-sort-hide",
                "planets-hide",
                Some(2),
                [0.0, 80.0, 80.0, 32.0],
            ),
            drop_hit(
                "planets-hide",
                "gal-drag-sort-hide",
                "planets-hide",
                None,
                [0.0, 0.0, 80.0, 320.0],
            ),
            drop_hit(
                "Mercury",
                "gal-drag-sort",
                "planets",
                Some(0),
                [0.0, 400.0, 80.0, 32.0],
            ),
            drop_hit(
                "Venus",
                "gal-drag-sort",
                "planets",
                Some(1),
                [0.0, 440.0, 80.0, 32.0],
            ),
            drop_hit(
                "Earth",
                "gal-drag-sort",
                "planets",
                Some(2),
                [0.0, 480.0, 80.0, 32.0],
            ),
            drop_hit(
                "planets",
                "gal-drag-sort",
                "planets",
                None,
                [0.0, 400.0, 80.0, 320.0],
            ),
        ];
        assert!(tick(41.0, 416.0, &hits, [800.0, 600.0]));
        let at_lift = (hover_list.peek(), hover_index.peek());
        assert!(tick(40.0, 496.0, &hits, [800.0, 600.0]));
        let at_earth = (hover_list.peek(), hover_index.peek());
        let _ = end(&hits);
        (at_lift, at_earth)
    });
    assert_eq!(
        at_lift.0.as_deref(),
        Some("planets"),
        "stacked opacity-0 list must not become hover_list; got {:?}",
        at_lift.0
    );
    assert_eq!(
        at_lift.1,
        Some(0),
        "grab in the source slot must keep hover=from so the dimmed body stays; got {:?}",
        at_lift.1
    );
    assert_eq!(
        at_earth.0.as_deref(),
        Some("planets"),
        "duplicate planet ids must resolve drop_hit in the session scope; got {:?}",
        at_earth.0
    );
    assert_eq!(
        at_earth.1,
        Some(2),
        "pointer on dimmed Earth must not append via opacity-0 Earth; got {:?}",
        at_earth.1
    );
}

#[test]
fn empty_scope_hit_does_not_join_named_session() {
    let (_o, hover_list) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([80.0_f32, 32.0]);
        register_scope(
            "named".into(),
            ScopeRuntime {
                collision: CollisionAlg::ClosestCenter,
                axis: Axis::Y,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: false,
                source_opacity: 1.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        begin(BeginDrag {
            scope_id: "named".into(),
            draggable_id: "Mercury".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [0.0, 8.0, 80.0, 32.0],
            from_list: Some("planets".into()),
            from_index: Some(0),
            item_type: "planet".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let hits = vec![
            drop_hit("Venus", "", "planets-hide", Some(1), [0.0, 0.0, 80.0, 80.0]),
            drop_hit(
                "Earth",
                "named",
                "planets",
                Some(1),
                [0.0, 48.0, 80.0, 32.0],
            ),
        ];
        assert!(tick(40.0, 20.0, &hits, [800.0, 600.0]));
        let hover = hover_list.peek();
        let _ = end(&hits);
        hover
    });
    assert_eq!(
        hover_list.as_deref(),
        Some("planets"),
        "hit with empty scope_id must not join a named DragProvider; got {hover_list:?}"
    );
}

/// Gallery Kanban: first also_droppable island can report the column height.
/// A uniform `start + stride` loop then swallows every probe into index 0.
#[test]
fn insert_index_stretched_first_row_does_not_swallow_column() {
    let mut a = hit("a", [0.0, 0.0, 80.0, 88.0], true);
    a.index = Some(0);
    let mut b = hit("b", [0.0, 40.0, 80.0, 32.0], true);
    b.index = Some(1);
    let hits = vec![a, b];
    assert_eq!(
        insert_index_along_axis(&hits, "list", [40.0, 56.0], Axis::Y),
        Some(1),
        "pointer on row 1 must not fall into row 0 when row 0 paint height is stretched"
    );
    assert_eq!(
        insert_index_along_axis(&hits, "list", [40.0, 16.0], Axis::Y),
        Some(0)
    );
}

/// Tail row: pitch < chip height — pointer on the chip must stay on from_index.
#[test]
fn pointer_on_short_pitch_tail_row_stays_from_index() {
    let mut a = hit("a", [0.0, 0.0, 80.0, 32.0], true);
    a.index = Some(0);
    a.scope_id = "gal-drag-board".into();
    a.list_id = Some("list".into());
    let mut b = hit("b", [0.0, 38.0, 80.0, 48.0], true);
    b.index = Some(1);
    b.scope_id = "gal-drag-board".into();
    b.list_id = Some("list".into());
    b.is_draggable = true;
    let mut hits = vec![a, b];
    cap_stretched_indexed_rects(&mut hits);
    remember_draggable_bounds("gal-drag-board", "b", [0.0, 38.0, 80.0, 48.0]);
    let sess = DragSession {
        scope_id: "gal-drag-board".into(),
        draggable_id: "b".into(),
        phase: DragPhase::Active,
        start_pointer: [40.0, 82.0],
        last_pointer: [40.0, 82.0],
        grab_offset: [0.0, 0.0],
        start_bounds: [0.0, 38.0, 80.0, 48.0],
        from_list: Some("list".into()),
        from_index: Some(1),
        item_type: String::new(),
        group: String::new(),
        axis: Axis::Y,
        motion_axis: Axis::Y,
        start_axis: Axis::Xy,
        threshold: 0.0,
        restrict: Restrict::None,
        restrict_end_only: false,
        restrict_anchor: RestrictAnchor::FULL,
        snap: None,
        capture_id: String::new(),
        delay_ms: 0.0,
        activation: Activation::And,
        pressed_at: Instant::now(),
        last_viewport: [0.0, 0.0],
        pull_clone: false,
        data: None,
        sort_mode: SortMode::Insert,
        items: Vec::new(),
    };
    assert_eq!(
        insert_index_along_axis(&hits, "list", [40.0, 82.0], Axis::Y),
        Some(2),
        "slot math alone may append when pitch < chip height"
    );
    assert_eq!(
        resolve_sortable_hover_index(&hits, &sess, "list", [40.0, 82.0], None),
        Some(1),
        "pointer still on source layout bounds must keep from_index"
    );
}

#[test]
fn tail_append_clamped_when_dragging_last_item() {
    let sess = DragSession {
        scope_id: "gal-drag-board".into(),
        draggable_id: "todo:Review PR".into(),
        phase: DragPhase::Active,
        start_pointer: [40.0, 120.0],
        last_pointer: [40.0, 120.0],
        grab_offset: [0.0, 0.0],
        start_bounds: [0.0, 80.0, 80.0, 32.0],
        from_list: Some("todo".into()),
        from_index: Some(1),
        item_type: String::new(),
        group: String::new(),
        axis: Axis::Y,
        motion_axis: Axis::Y,
        start_axis: Axis::Xy,
        threshold: 0.0,
        restrict: Restrict::None,
        restrict_end_only: false,
        restrict_anchor: RestrictAnchor::FULL,
        snap: None,
        capture_id: String::new(),
        delay_ms: 0.0,
        activation: Activation::And,
        pressed_at: Instant::now(),
        last_viewport: [0.0, 0.0],
        pull_clone: false,
        data: None,
        sort_mode: SortMode::Insert,
        items: Vec::new(),
    };
    let mut draft = hit("todo:Draft RFC", [0.0, 0.0, 80.0, 32.0], true);
    draft.list_id = Some("todo".into());
    draft.index = Some(0);
    let mut review = hit("todo:Review PR", [0.0, 40.0, 80.0, 32.0], true);
    review.list_id = Some("todo".into());
    review.index = Some(1);
    let hits = vec![draft, review];
    assert_eq!(
        clamp_tail_append_hover(&sess, "todo", &hits, 2),
        1,
        "2-item list: append after last row is phantom slot while dragging that row"
    );
    let mut sess0 = sess.clone();
    sess0.from_index = Some(0);
    sess0.draggable_id = "todo:Draft RFC".into();
    assert_eq!(
        clamp_tail_append_hover(&sess0, "todo", &hits, 2),
        1,
        "same-list has no slot past last; first row must not open phantom append"
    );
}

/// Same-list reorder occupies the N layout slots. `insert_index_along_axis`
/// reports `max+1` once the pointer is a pitch past the last row; that
/// phantom index makes siblings vacate the last slot while the dimmed body
/// is painted one pitch below it (user: 拖到最后再向下一个项高 → 末槽空白).
#[test]
fn same_list_past_last_row_keeps_body_in_last_slot() {
    let hits: Vec<DragHit> = (0..8)
        .map(|i| {
            let mut h = hit(
                [
                    "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus", "Neptune",
                ][i],
                [0.0, i as f32 * 40.0, 80.0, 32.0],
                true,
            );
            h.list_id = Some("planets".into());
            h.index = Some(i);
            h
        })
        .collect();
    let sess = DragSession {
        scope_id: "gal-drag-sort".into(),
        draggable_id: "Mercury".into(),
        phase: DragPhase::Active,
        start_pointer: [40.0, 16.0],
        last_pointer: [40.0, 16.0],
        grab_offset: [0.0, 0.0],
        start_bounds: [0.0, 0.0, 80.0, 32.0],
        from_list: Some("planets".into()),
        from_index: Some(0),
        item_type: "planet".into(),
        group: String::new(),
        axis: Axis::Y,
        motion_axis: Axis::Y,
        start_axis: Axis::Xy,
        threshold: 0.0,
        restrict: Restrict::None,
        restrict_end_only: false,
        restrict_anchor: RestrictAnchor::FULL,
        snap: None,
        capture_id: String::new(),
        delay_ms: 0.0,
        activation: Activation::And,
        pressed_at: Instant::now(),
        last_viewport: [0.0, 0.0],
        pull_clone: false,
        data: None,
        sort_mode: SortMode::Insert,
        items: Vec::new(),
    };
    assert_eq!(
        insert_index_along_axis(&hits, "planets", [40.0, 340.0], Axis::Y),
        Some(8),
        "raw midpoint past last+pitch is Sortable append (len)"
    );
    assert_eq!(
        resolve_sortable_hover_index(&hits, &sess, "planets", [40.0, 300.0], None),
        Some(7),
        "pointer in the last slot opens the last real hole"
    );
    assert_eq!(
        resolve_sortable_hover_index(&hits, &sess, "planets", [40.0, 340.0], Some(7)),
        Some(7),
        "one pitch past the last row must keep hover on the last slot, not max+1"
    );
    let hover =
        resolve_sortable_hover_index(&hits, &sess, "planets", [40.0, 340.0], Some(7)).unwrap();
    let src = source_translate_to_insert_hole(&hits, "planets", 0, "planets", Some(hover), Axis::Y);
    assert_eq!(
        src,
        [0.0, 280.0],
        "dimmed body must sit on Neptune's layout slot, not one pitch below the list"
    );
    assert_eq!(
        source_translate_to_insert_hole(&hits, "planets", 0, "planets", Some(8), Axis::Y),
        [0.0, 320.0],
        "unclamped hover=len is the blank-slot translate (last origin + pitch)"
    );
}

/// Gallery semi-transparent Sortable: Mars (index 3) dragged below Jupiter,
/// then nudged one row — hover must not advance and leave a blank slot.
#[test]
fn mars_below_jupiter_pins_hover_when_nudged_one_row() {
    let hits: Vec<DragHit> = (0..8)
        .map(|i| {
            let mut h = hit(
                [
                    "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus", "Neptune",
                ][i],
                [0.0, i as f32 * 40.0, 80.0, 320.0],
                true,
            );
            h.list_id = Some("planets".into());
            h.index = Some(i);
            h
        })
        .collect();
    let mut hits = hits;
    cap_stretched_indexed_rects(&mut hits);
    let sess = DragSession {
        scope_id: "gal-drag-sort".into(),
        draggable_id: "Mars".into(),
        phase: DragPhase::Active,
        start_pointer: [40.0, 136.0],
        last_pointer: [40.0, 136.0],
        grab_offset: [0.0, 0.0],
        start_bounds: [0.0, 120.0, 80.0, 32.0],
        from_list: Some("planets".into()),
        from_index: Some(3),
        item_type: "planet".into(),
        group: String::new(),
        axis: Axis::Y,
        motion_axis: Axis::Y,
        start_axis: Axis::Xy,
        threshold: 0.0,
        restrict: Restrict::None,
        restrict_end_only: false,
        restrict_anchor: RestrictAnchor::FULL,
        snap: None,
        capture_id: String::new(),
        delay_ms: 0.0,
        activation: Activation::And,
        pressed_at: Instant::now(),
        last_viewport: [0.0, 0.0],
        pull_clone: false,
        data: None,
        sort_mode: SortMode::Insert,
        items: Vec::new(),
    };
    assert_eq!(
        resolve_sortable_hover_index(&hits, &sess, "planets", [40.0, 190.0], None),
        Some(5),
        "lower half of Jupiter row opens insert after Jupiter"
    );
    assert_eq!(
        resolve_sortable_hover_index(&hits, &sess, "planets", [40.0, 230.0], Some(5)),
        Some(5),
        "one pitch below the opened hole must not bump hover"
    );
    assert_eq!(
        resolve_sortable_hover_index(&hits, &sess, "planets", [40.0, 250.0], Some(5)),
        Some(6),
        "past hole+pitch band may advance"
    );
    let hole = insert_hole_origin(&hits, "planets", Some(5), Axis::Y, 40.0).unwrap();
    let src = source_translate_to_insert_hole(&hits, "planets", 3, "planets", Some(5), Axis::Y);
    assert_eq!(
        src,
        [hole[0] - 0.0, hole[1] - 120.0],
        "dimmed body translate must match layout hole origin"
    );
}

#[test]
fn kanban_past_todo_content_resolves_doing_column() {
    let sess = DragSession {
        scope_id: "gal-drag-board".into(),
        draggable_id: "todo:Review PR".into(),
        phase: DragPhase::Active,
        start_pointer: [120.0, 120.0],
        last_pointer: [120.0, 120.0],
        grab_offset: [0.0, 0.0],
        start_bounds: [0.0, 40.0, 80.0, 32.0],
        from_list: Some("todo".into()),
        from_index: Some(1),
        item_type: "task".into(),
        group: "board".into(),
        axis: Axis::Y,
        motion_axis: Axis::Y,
        start_axis: Axis::Xy,
        threshold: 0.0,
        restrict: Restrict::None,
        restrict_end_only: false,
        restrict_anchor: RestrictAnchor::FULL,
        snap: None,
        capture_id: String::new(),
        delay_ms: 0.0,
        activation: Activation::And,
        pressed_at: Instant::now(),
        last_viewport: [0.0, 0.0],
        pull_clone: false,
        data: None,
        sort_mode: SortMode::Insert,
        items: Vec::new(),
    };
    let mut draft = hit("todo:Draft RFC", [0.0, 0.0, 80.0, 32.0], true);
    draft.scope_id = "gal-drag-board".into();
    draft.list_id = Some("todo".into());
    draft.index = Some(0);
    draft.item_type = "task".into();
    draft.group = "board".into();
    let mut review = hit("todo:Review PR", [0.0, 40.0, 80.0, 32.0], true);
    review.scope_id = "gal-drag-board".into();
    review.list_id = Some("todo".into());
    review.index = Some(1);
    review.item_type = "task".into();
    review.group = "board".into();
    let mut todo_shell = hit("todo", [0.0, 0.0, 80.0, 200.0], true);
    todo_shell.scope_id = "gal-drag-board".into();
    todo_shell.list_id = Some("todo".into());
    todo_shell.item_type = "task".into();
    todo_shell.accept = "task".into();
    todo_shell.group = "board".into();
    todo_shell.item_type = "task".into();
    todo_shell.accept = "task".into();
    todo_shell.group = "board".into();
    let mut doing_shell = hit("doing", [88.0, 0.0, 80.0, 200.0], true);
    doing_shell.scope_id = "gal-drag-board".into();
    doing_shell.list_id = Some("doing".into());
    doing_shell.item_type = "task".into();
    doing_shell.accept = "task".into();
    doing_shell.group = "board".into();
    let mut task = hit("todo:Write tests", [88.0, 0.0, 80.0, 32.0], true);
    task.scope_id = "gal-drag-board".into();
    task.list_id = Some("doing".into());
    task.index = Some(0);
    task.item_type = "task".into();
    task.group = "board".into();
    let hits = vec![draft, review, todo_shell, doing_shell, task];
    let pointer = [120.0, 120.0];
    assert!(
        pointer_past_list_indexed_content(&hits, "todo", pointer, Axis::Y),
        "pointer is below todo indexed rows"
    );
    assert_eq!(
        resolve_sortable_hover_list(&hits, &sess, pointer, None).as_deref(),
        Some("doing"),
        "past todo padding + doing column band must target doing, not phantom todo slot"
    );
}

fn nest_file_drag_session() -> DragSession {
    DragSession {
        scope_id: "s".into(),
        draggable_id: "tree-inbox:Beta".into(),
        phase: DragPhase::Active,
        start_pointer: [110.0, 72.0],
        last_pointer: [110.0, 72.0],
        grab_offset: [0.0, 0.0],
        start_bounds: [8.0, 60.0, 204.0, 24.0],
        from_list: Some("tree-inbox".into()),
        from_index: Some(1),
        item_type: "file".into(),
        group: "nest".into(),
        axis: Axis::Y,
        motion_axis: Axis::Y,
        start_axis: Axis::Xy,
        threshold: 0.0,
        restrict: Restrict::None,
        restrict_end_only: false,
        restrict_anchor: RestrictAnchor::FULL,
        snap: None,
        capture_id: String::new(),
        delay_ms: 0.0,
        activation: Activation::And,
        pressed_at: Instant::now(),
        last_viewport: [0.0, 0.0],
        pull_clone: false,
        data: None,
        sort_mode: SortMode::Insert,
        items: Vec::new(),
    }
}

/// Gallery nested folders: Inbox (Alpha, Beta) stacked above Later (Gamma).
fn nest_folder_file_hits() -> Vec<DragHit> {
    let mut root = hit("tree-root", [0.0, 0.0, 220.0, 280.0], true);
    root.list_id = Some("tree-root".into());
    root.accept = "folder".into();
    root.item_type = String::new();
    let mut inbox_card = hit("folder:Inbox", [0.0, 0.0, 220.0, 88.0], true);
    inbox_card.list_id = Some("tree-root".into());
    inbox_card.index = Some(0);
    inbox_card.item_type = "folder".into();
    let mut inbox_shell = hit("tree-inbox", [8.0, 28.0, 204.0, 52.0], true);
    inbox_shell.list_id = Some("tree-inbox".into());
    inbox_shell.accept = "file".into();
    inbox_shell.group = "nest".into();
    inbox_shell.item_type = String::new();
    let mut alpha = hit("tree-inbox:Alpha", [8.0, 32.0, 204.0, 24.0], true);
    alpha.list_id = Some("tree-inbox".into());
    alpha.index = Some(0);
    alpha.item_type = "file".into();
    alpha.group = "nest".into();
    let mut beta = hit("tree-inbox:Beta", [8.0, 60.0, 204.0, 24.0], true);
    beta.list_id = Some("tree-inbox".into());
    beta.index = Some(1);
    beta.item_type = "file".into();
    beta.group = "nest".into();
    beta.is_draggable = true;
    let mut later_card = hit("folder:Later", [0.0, 108.0, 220.0, 60.0], true);
    later_card.list_id = Some("tree-root".into());
    later_card.index = Some(1);
    later_card.item_type = "folder".into();
    let mut later_shell = hit("tree-later", [8.0, 136.0, 204.0, 28.0], true);
    later_shell.list_id = Some("tree-later".into());
    later_shell.accept = "file".into();
    later_shell.group = "nest".into();
    later_shell.item_type = String::new();
    let mut gamma = hit("tree-later:Gamma", [8.0, 140.0, 204.0, 24.0], true);
    gamma.list_id = Some("tree-later".into());
    gamma.index = Some(0);
    gamma.item_type = "file".into();
    gamma.group = "nest".into();
    vec![
        root,
        inbox_card,
        inbox_shell,
        alpha,
        beta,
        later_card,
        later_shell,
        gamma,
    ]
}

/// Pointer in the gap between stacked nested lists must not flip columns
/// every frame (that grows/shrinks both lists). Stick to the closer slot.
#[test]
fn nested_lists_gap_sticky_nearest_slot() {
    let sess = nest_file_drag_session();
    let hits = nest_folder_file_hits();
    let gap = [110.0, 112.0];
    assert!(
        !list_shell_contains(&hits, "tree-inbox", gap)
            && !list_shell_contains(&hits, "tree-later", gap),
        "fixture pointer sits in the folder gap, not inside either inner shell"
    );
    let first = resolve_sortable_hover_list(&hits, &sess, gap, None);
    assert!(
        first.as_deref() == Some("tree-inbox") || first.as_deref() == Some("tree-later"),
        "gap must pick a nested file list, not tree-root; got {first:?}"
    );
    assert_eq!(
        resolve_sortable_hover_list(&hits, &sess, gap, first.as_deref()),
        first,
        "sticky nearest-slot must not oscillate while the pointer stays in the gap"
    );
    assert_eq!(
        resolve_sortable_hover_list(&hits, &sess, gap, Some("tree-inbox")).as_deref(),
        Some("tree-inbox"),
        "12px sticky keeps Inbox when the pointer is between the two folders"
    );
    assert_eq!(
        resolve_sortable_hover_list(&hits, &sess, gap, Some("tree-later")).as_deref(),
        Some("tree-later"),
        "12px sticky keeps Later when the pointer is between the two folders"
    );
}

/// Dragging the last inbox file above the first: hover must be slot 0 even
/// when the uncapped `also_droppable` island still covers Alpha.
#[test]
fn nested_same_list_last_item_above_first_opens_head_slot() {
    let sess = nest_file_drag_session();
    let hits = nest_folder_file_hits();
    remember_draggable_bounds("s", "tree-inbox:Beta", [8.0, 28.0, 204.0, 200.0]);
    let above_alpha = [110.0, 40.0];
    assert!(
        hits.iter()
            .any(|h| h.id == "tree-inbox:Alpha" && h.rect.contains_point(above_alpha)),
        "fixture pointer sits on Alpha"
    );
    assert_eq!(
        resolve_sortable_hover_list(&hits, &sess, above_alpha, None).as_deref(),
        Some("tree-inbox")
    );
    assert_eq!(
        resolve_sortable_hover_index(&hits, &sess, "tree-inbox", above_alpha, None),
        Some(0),
        "Beta dragged above Alpha must open slot 0, not stay from_index=1"
    );
}

/// Clustered local slots (all y=0) vs page-space press box: pointer on the
/// folder title is above Beta's press origin, so hover is the head slot.
#[test]
fn nested_clustered_slots_last_item_above_press_opens_head() {
    let mut sess = nest_file_drag_session();
    sess.start_bounds = [8.0, 60.0, 204.0, 24.0];
    let mut alpha = hit("tree-inbox:Alpha", [8.0, 0.0, 204.0, 24.0], true);
    alpha.list_id = Some("tree-inbox".into());
    alpha.index = Some(0);
    alpha.item_type = "file".into();
    alpha.group = "nest".into();
    let mut beta = hit("tree-inbox:Beta", [8.0, 0.0, 204.0, 24.0], true);
    beta.list_id = Some("tree-inbox".into());
    beta.index = Some(1);
    beta.item_type = "file".into();
    beta.group = "nest".into();
    beta.is_draggable = true;
    let mut shell = hit("tree-inbox", [8.0, 0.0, 204.0, 24.0], true);
    shell.list_id = Some("tree-inbox".into());
    shell.accept = "file".into();
    shell.group = "nest".into();
    shell.item_type = String::new();
    let hits = vec![shell, alpha, beta];
    let title = [110.0, 12.0];
    assert_eq!(
        resolve_sortable_hover_index(&hits, &sess, "tree-inbox", title, None),
        Some(0),
        "pointer above Beta's press box must open slot 0 even when local slots share y=0"
    );
}

/// Pointer below Later: nearest *insert* slot is after Gamma (`len`), not
/// Gamma's own index (which opens a hole under the Later title).
#[test]
fn nested_lists_outside_second_list_uses_last_real_slot() {
    let sess = nest_file_drag_session();
    let hits = nest_folder_file_hits();
    let below = [110.0, 220.0];
    assert!(
        !list_shell_contains(&hits, "tree-later", below),
        "pointer is outside Later's list shell"
    );
    assert_eq!(
        resolve_sortable_hover_list(&hits, &sess, below, None).as_deref(),
        Some("tree-later"),
        "below both folders the nearest file list is Later"
    );
    assert_eq!(
        insert_index_along_axis(&hits, "tree-later", below, Axis::Y),
        None,
        "raw midpoint is far past last+2*pitch"
    );
    assert_eq!(
        resolve_sortable_hover_index(&hits, &sess, "tree-later", below, None),
        Some(1),
        "below Gamma the nearest insert slot is after Gamma, not slot 0"
    );
    let hover = resolve_sortable_hover_index(&hits, &sess, "tree-later", below, None).unwrap();
    let src =
        source_translate_to_insert_hole(&hits, "tree-inbox", 1, "tree-later", Some(hover), Axis::Y);
    assert_eq!(
        src,
        [0.0, 104.0],
        "dimmed body sits in the append hole (Gamma origin + pitch)"
    );
    assert_eq!(
        source_translate_to_insert_hole(&hits, "tree-inbox", 1, "tree-later", Some(0), Axis::Y),
        [0.0, 80.0],
        "hover=0 is the title-gap hole (Gamma's layout origin)"
    );
}

/// Activate snapshots Inbox Alpha→Beta stride (item+gap). A one-item Later
/// has no neighbour stride, so live `slot_extent(Gamma)` is chip-only and
/// the dimmed body sits flush on Gamma (Gallery: Beta 移到 Gamma 之下).
#[test]
fn nested_cross_list_append_under_gamma_keeps_packed_gap() {
    let hits = nest_folder_file_hits();
    let chip = 24.0;
    let source_pitch = row_pitch_along(&hits, Some("tree-inbox"), Axis::Y, chip);
    assert!(
        (source_pitch - 28.0).abs() < 0.5,
        "Inbox Alpha→Beta pitch is item+gap; got {source_pitch}"
    );
    DRAG_PACK_PITCH.set(source_pitch);
    DRAG_PACK_GAP.set(4.0);
    let src =
        source_translate_to_insert_hole(&hits, "tree-inbox", 1, "tree-later", Some(1), Axis::Y);
    let hole = insert_hole_origin(&hits, "tree-later", Some(1), Axis::Y, chip).unwrap();
    DRAG_PACK_PITCH.set(0.0);
    DRAG_PACK_GAP.set(0.0);
    assert_eq!(
        src,
        [0.0, 108.0],
        "append under Gamma must be origin + packed pitch (28), not chip (24); got {src:?}"
    );
    let gamma = hits.iter().find(|h| h.id == "tree-later:Gamma").unwrap();
    let gap = hole[1] - (gamma.rect.y + gamma.rect.height);
    assert!(
        (gap - 4.0).abs() < 0.5,
        "dimmed Beta must keep author gap under Gamma; gap={gap} hole={hole:?} gamma={:?}",
        gamma.rect
    );
}

/// After the origin list shrinks, live hits may drop the absolute source.
/// The target hole origin is still well-defined from the destination list.
#[test]
fn cross_list_hole_origin_without_src_hit() {
    let hits = nest_folder_file_hits()
        .into_iter()
        .filter(|h| h.list_id.as_deref() != Some("tree-inbox") || h.index.is_none())
        .collect::<Vec<_>>();
    assert!(
        hits.iter()
            .all(|h| !(h.list_id.as_deref() == Some("tree-inbox") && h.index == Some(1))),
        "precondition: Beta hit is gone"
    );
    DRAG_PACK_PITCH.set(28.0);
    DRAG_PACK_GAP.set(4.0);
    let origin = [8.0, 34.0];
    let hole = insert_hole_origin(&hits, "tree-later", Some(1), Axis::Y, 24.0).unwrap();
    let got = source_translate_to_insert_hole_at(
        &hits,
        "tree-inbox",
        1,
        "tree-later",
        Some(1),
        Axis::Y,
        Some(origin),
    );
    DRAG_PACK_PITCH.set(0.0);
    DRAG_PACK_GAP.set(0.0);
    assert_eq!(
        got,
        Some([hole[0] - origin[0], hole[1] - origin[1]]),
        "missing src hit must not collapse to [0,0]; hole={hole:?} origin={origin:?}"
    );
    assert!(
        got.unwrap()[1].abs() > 8.0,
        "must still move the dimmed body into Later; got {got:?}"
    );
}

#[test]
fn kanban_drag_last_item_below_rows_keeps_hover_index() {
    let (_o, (hover_list, hover_index)) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([80.0_f32, 32.0]);
        register_scope(
            "gal-drag-board".into(),
            ScopeRuntime {
                collision: CollisionAlg::MostIntersecting,
                axis: Axis::Y,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: true,
                source_opacity: 0.4,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        begin(BeginDrag {
            scope_id: "gal-drag-board".into(),
            draggable_id: "todo:Review PR".into(),
            pointer: [40.0, 56.0],
            grab_offset: [40.0, 16.0],
            start_bounds: [0.0, 40.0, 80.0, 32.0],
            from_list: Some("todo".into()),
            from_index: Some(1),
            item_type: "task".into(),
            group: "board".into(),
            capture_id: "cap".into(),
        });
        let mut draft = drop_hit(
            "todo:Draft RFC",
            "gal-drag-board",
            "todo",
            Some(0),
            [0.0, 0.0, 80.0, 32.0],
        );
        let mut review = drop_hit(
            "todo:Review PR",
            "gal-drag-board",
            "todo",
            Some(1),
            [0.0, 40.0, 80.0, 32.0],
        );
        review.is_draggable = true;
        review.item_type = "task".into();
        review.group = "board".into();
        draft.is_draggable = true;
        draft.item_type = "task".into();
        draft.group = "board".into();
        let mut todo_shell = drop_hit(
            "todo",
            "gal-drag-board",
            "todo",
            None,
            [0.0, 0.0, 80.0, 200.0],
        );
        todo_shell.item_type = "task".into();
        todo_shell.accept = "task".into();
        todo_shell.group = "board".into();
        let hits = vec![draft, review, todo_shell];
        assert!(tick(40.0, 100.0, &hits, [800.0, 600.0]));
        let list = hover_list.peek();
        let idx = hover_index.peek();
        let _ = end(&hits);
        (list, idx)
    });
    assert_eq!(hover_list.as_deref(), Some("todo"));
    assert_eq!(
        hover_index,
        Some(1),
        "below rows in todo padding must not open phantom index 2"
    );
}

#[test]
fn pointer_indexed_insert_beats_tail_append_on_stretched_column() {
    let (_o, hover_index) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([80.0_f32, 32.0]);
        register_scope(
            "gal-drag-board".into(),
            ScopeRuntime {
                collision: CollisionAlg::MostIntersecting,
                axis: Axis::Y,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: true,
                source_opacity: 0.4,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        begin(BeginDrag {
            scope_id: "gal-drag-board".into(),
            draggable_id: "todo:Review PR".into(),
            pointer: [40.0, 82.0],
            grab_offset: [40.0, 42.0],
            start_bounds: [0.0, 40.0, 80.0, 48.0],
            from_list: Some("todo".into()),
            from_index: Some(1),
            item_type: "task".into(),
            group: "board".into(),
            capture_id: "cap".into(),
        });
        let mut draft = drop_hit(
            "todo:Draft RFC",
            "gal-drag-board",
            "todo",
            Some(0),
            [0.0, 0.0, 80.0, 88.0],
        );
        let mut review = drop_hit(
            "todo:Review PR",
            "gal-drag-board",
            "todo",
            Some(1),
            [0.0, 40.0, 80.0, 48.0],
        );
        review.is_draggable = true;
        draft.is_draggable = true;
        let hits = vec![draft, review];
        assert!(tick(41.0, 82.0, &hits, [800.0, 600.0]));
        let idx = hover_index.peek();
        let _ = end(&hits);
        idx
    });
    assert_eq!(
        hover_index,
        Some(1),
        "lift on Review PR must stay from_index, not append"
    );
}

#[test]
fn pointer_indexed_row_beats_ghost_column_collision() {
    let (_o, (hover_list, hover_index)) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([80.0_f32, 32.0]);
        register_scope(
            "gal-drag-board".into(),
            ScopeRuntime {
                collision: CollisionAlg::MostIntersecting,
                axis: Axis::Y,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: true,
                source_opacity: 0.4,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        begin(BeginDrag {
            scope_id: "gal-drag-board".into(),
            draggable_id: "todo:Review PR".into(),
            pointer: [40.0, 56.0],
            grab_offset: [40.0, 16.0],
            start_bounds: [0.0, 40.0, 80.0, 32.0],
            from_list: Some("todo".into()),
            from_index: Some(1),
            item_type: "task".into(),
            group: "board".into(),
            capture_id: "cap".into(),
        });
        let mut draft = drop_hit(
            "todo:Draft RFC",
            "gal-drag-board",
            "todo",
            Some(0),
            [0.0, 0.0, 80.0, 32.0],
        );
        let mut review = drop_hit(
            "todo:Review PR",
            "gal-drag-board",
            "todo",
            Some(1),
            [0.0, 40.0, 80.0, 32.0],
        );
        review.is_draggable = true;
        draft.is_draggable = true;
        // Column shell shorter than rows (Kanban label-only container hit).
        let hits = vec![
            draft,
            review,
            drop_hit(
                "todo",
                "gal-drag-board",
                "todo",
                None,
                [0.0, 0.0, 80.0, 24.0],
            ),
            drop_hit(
                "doing",
                "gal-drag-board",
                "doing",
                None,
                [88.0, 0.0, 80.0, 88.0],
            ),
        ];
        assert!(tick(41.0, 56.0, &hits, [800.0, 600.0]));
        let list = hover_list.peek();
        let idx = hover_index.peek();
        let _ = end(&hits);
        (list, idx)
    });
    assert_eq!(
        hover_list.as_deref(),
        Some("todo"),
        "hover_list must follow the indexed row under the pointer, not ghost collision"
    );
    assert_eq!(
        hover_index,
        Some(1),
        "insert index must stay on the lifted row"
    );
}

#[test]
fn nested_list_shell_beats_parent_folder_row() {
    let (_o, hover) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([40.0_f32, 20.0]);
        register_scope(
            "s".into(),
            ScopeRuntime {
                collision: CollisionAlg::MostIntersecting,
                axis: Axis::Y,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: false,
                source_opacity: 1.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 8.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "file".into(),
            pointer: [10.0, 310.0],
            grab_offset: [0.0, 0.0],
            start_bounds: [0.0, 300.0, 40.0, 20.0],
            from_list: Some("inbox".into()),
            from_index: Some(0),
            item_type: "file".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let mut folder = hit("folder-later", [0.0, 0.0, 200.0, 200.0], true);
        folder.list_id = Some("root".into());
        folder.index = Some(1);
        folder.is_draggable = true;
        folder.item_type = "folder".into();
        let mut nested = hit("later-shell", [10.0, 40.0, 180.0, 140.0], true);
        nested.list_id = Some("later".into());
        nested.index = None;
        nested.accept = "file".into();
        nested.item_type = String::new();
        let mut src = hit("file", [0.0, 300.0, 40.0, 20.0], true);
        src.list_id = Some("inbox".into());
        src.index = Some(0);
        src.is_draggable = true;
        src.item_type = "file".into();
        let hits = vec![folder, nested, src];
        assert!(tick(20.0, 80.0, &hits, [400.0, 400.0]));
        let list = hover_list.peek();
        let _ = end(&hits);
        list
    });
    assert_eq!(
        hover.as_deref(),
        Some("later"),
        "empty nested list padding must win over the wrapping folder row"
    );
}

#[test]
fn empty_nested_inbox_header_accepts_file_drop() {
    let (_o, hover) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([160.0_f32, 32.0]);
        register_scope(
            "s".into(),
            ScopeRuntime {
                collision: CollisionAlg::MostIntersecting,
                axis: Axis::Y,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: false,
                source_opacity: 1.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 8.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "tree-later:Beta".into(),
            pointer: [20.0, 96.0],
            grab_offset: [0.0, 0.0],
            start_bounds: [0.0, 80.0, 200.0, 32.0],
            from_list: Some("tree-later".into()),
            from_index: Some(0),
            item_type: "file".into(),
            group: "nest".into(),
            capture_id: "cap".into(),
        });
        let mut folder = hit("folder:Inbox", [0.0, 0.0, 200.0, 48.0], true);
        folder.list_id = Some("tree-root".into());
        folder.index = Some(0);
        folder.is_draggable = true;
        folder.item_type = "folder".into();
        folder.accept = "folder".into();
        folder.group = "nest".into();
        let mut inbox = hit("tree-inbox", [8.0, 40.0, 184.0, 0.0], true);
        inbox.list_id = Some("tree-inbox".into());
        inbox.index = None;
        inbox.accept = "file".into();
        inbox.item_type = String::new();
        inbox.group = "nest".into();
        let mut later_shell = hit("tree-later", [0.0, 80.0, 200.0, 40.0], true);
        later_shell.list_id = Some("tree-later".into());
        later_shell.index = None;
        later_shell.accept = "file".into();
        later_shell.group = "nest".into();
        let mut src = hit("tree-later:Beta", [0.0, 80.0, 200.0, 32.0], true);
        src.list_id = Some("tree-later".into());
        src.index = Some(0);
        src.is_draggable = true;
        src.item_type = "file".into();
        src.accept = "file".into();
        src.group = "nest".into();
        let hits = vec![folder, inbox, later_shell, src];
        assert!(tick(20.0, 16.0, &hits, [400.0, 400.0]));
        let list = hover_list.peek();
        let index = hover_index.peek();
        let _ = end(&hits);
        (list, index)
    });
    assert_eq!(
            hover.0.as_deref(),
            Some("tree-inbox"),
            "collapsed Inbox list must borrow the folder box so a file over the header selects it; hover_list={:?}",
            hover.0
        );
    assert_eq!(
        hover.1,
        Some(0),
        "empty Inbox insert index must be 0, not the origin list's from_index; hover_index={:?}",
        hover.1
    );
}

#[test]
fn origin_empty_shell_does_not_steal_hover_from_dest() {
    let (_o, hover) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([160.0_f32, 32.0]);
        register_scope(
            "s".into(),
            ScopeRuntime {
                collision: CollisionAlg::MostIntersecting,
                axis: Axis::Y,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: false,
                source_opacity: 1.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 8.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "tree-inbox:Beta".into(),
            pointer: [20.0, 40.0],
            grab_offset: [0.0, 0.0],
            start_bounds: [8.0, 20.0, 184.0, 32.0],
            from_list: Some("tree-inbox".into()),
            from_index: Some(0),
            item_type: "file".into(),
            group: "nest".into(),
            capture_id: "cap".into(),
        });
        let mut folder = hit("folder:Inbox", [0.0, 0.0, 200.0, 80.0], true);
        folder.list_id = Some("tree-root".into());
        folder.index = Some(0);
        folder.item_type = "folder".into();
        folder.accept = "folder".into();
        folder.group = "nest".into();
        let mut inbox = hit("tree-inbox", [8.0, 40.0, 184.0, 0.0], true);
        inbox.list_id = Some("tree-inbox".into());
        inbox.index = None;
        inbox.accept = "file".into();
        inbox.group = "nest".into();
        let mut later = hit("tree-later", [0.0, 30.0, 200.0, 80.0], true);
        later.list_id = Some("tree-later".into());
        later.index = None;
        later.accept = "file".into();
        later.group = "nest".into();
        let mut gamma = hit("tree-later:Gamma", [8.0, 38.0, 184.0, 32.0], true);
        gamma.list_id = Some("tree-later".into());
        gamma.index = Some(0);
        gamma.item_type = "file".into();
        gamma.accept = "file".into();
        gamma.group = "nest".into();
        let hits = vec![folder, inbox, later, gamma];
        assert!(tick(20.0, 70.0, &hits, [400.0, 400.0]));
        let list = hover_list.peek();
        let _ = end(&hits);
        list
    });
    assert_eq!(
        hover.as_deref(),
        Some("tree-later"),
        "vacated origin Inbox must not borrow the folder box and steal Later; hover_list={:?}",
        hover
    );
}

#[test]
fn list_row_extent_uses_packed_stride_not_stretched_chip() {
    let mut a = hit("a", [0.0, 0.0, 40.0, 24.0], true);
    a.index = Some(0);
    a.list_id = Some("list".into());
    let mut b = hit("b", [0.0, 28.0, 40.0, 24.0], true);
    b.index = Some(1);
    b.list_id = Some("list".into());
    LAST_HITS.with(|h| *h.borrow_mut() = vec![a, b]);
    assert!(
        (list_row_extent(Some("list"), Axis::Y, 200.0) - 28.0).abs() < 0.5,
        "stretched island height must not become the vacate row; got {}",
        list_row_extent(Some("list"), Axis::Y, 200.0)
    );
}

/// Cross-list padding looks up the *target* list. A one-item Later has no
/// neighbour stride, so live `list_row_extent` is `chip` and drops the
/// author gap. `pack_insert_extent` uses the snapshotted source pitch.
#[test]
fn list_row_extent_singleton_target_must_keep_source_row_pitch() {
    let mut alpha = hit("tree-inbox:Alpha", [0.0, 0.0, 160.0, 32.0], true);
    alpha.index = Some(0);
    alpha.list_id = Some("tree-inbox".into());
    let mut beta = hit("tree-inbox:Beta", [0.0, 38.0, 160.0, 32.0], true);
    beta.index = Some(1);
    beta.list_id = Some("tree-inbox".into());
    let mut gamma = hit("tree-later:Gamma", [0.0, 80.0, 160.0, 32.0], true);
    gamma.index = Some(0);
    gamma.list_id = Some("tree-later".into());
    LAST_HITS.with(|h| *h.borrow_mut() = vec![alpha, beta, gamma]);
    let source_pitch = list_row_extent(Some("tree-inbox"), Axis::Y, 32.0);
    let live_target = list_row_extent(Some("tree-later"), Axis::Y, 32.0);
    assert!(
        (source_pitch - 38.0).abs() < 0.5,
        "Inbox Alpha→Beta pitch is item+gap; got {source_pitch}"
    );
    assert!(
        (live_target - 32.0).abs() < 0.5,
        "live Later stride is chip-only; got {live_target}"
    );
    DRAG_PACK_PITCH.set(source_pitch);
    DRAG_PACK_GAP.set(6.0);
    let target_pad = pack_insert_extent(Some("tree-later"), Axis::Y, 32.0);
    let gap = pack_gap(32.0);
    DRAG_PACK_PITCH.set(0.0);
    DRAG_PACK_GAP.set(0.0);
    assert!(
        (target_pad - source_pitch).abs() < 0.5,
        "Later pad must open the same packed row, not chip-only; \
             target_pad={target_pad} source_pitch={source_pitch}"
    );
    assert!(
        (gap - 6.0).abs() < 0.5,
        "pack_gap is pitch - chip; got {gap}"
    );
}

/// Cross-list: source shell leaves the pack (`position: absolute`). Origin
/// siblings already reflow into the hole — a second `-extent` FLIP walks
/// them into the folder header (Gallery: Alpha 拖到 Later 时 Beta 盖住 Inbox).
/// Target siblings must open the snapshotted packed row, not the live
/// collapsed source slot (0 gap between dimmed body and Gamma).
#[test]
fn cross_list_origin_sibling_stays_put_target_opens_packed_row() {
    let mut alpha = hit("tree-inbox:Alpha", [8.0, 40.0, 160.0, 32.0], true);
    alpha.index = Some(0);
    alpha.list_id = Some("tree-inbox".into());
    let mut beta = hit("tree-inbox:Beta", [8.0, 40.0, 160.0, 32.0], true);
    beta.index = Some(1);
    beta.list_id = Some("tree-inbox".into());
    let mut gamma = hit("tree-later:Gamma", [8.0, 200.0, 160.0, 32.0], true);
    gamma.index = Some(0);
    gamma.list_id = Some("tree-later".into());
    let hits = vec![alpha, beta, gamma];
    DRAG_PACK_PITCH.set(38.0);
    DRAG_PACK_GAP.set(6.0);
    let origin_beta = pack_item_translate(
        &hits,
        Some("tree-inbox"),
        Some(1),
        Some("tree-inbox"),
        Some(0),
        Some("tree-later"),
        Some(0),
        Axis::Y,
    );
    let target_gamma = pack_item_translate(
        &hits,
        Some("tree-later"),
        Some(0),
        Some("tree-inbox"),
        Some(0),
        Some("tree-later"),
        Some(0),
        Axis::Y,
    );
    DRAG_PACK_PITCH.set(0.0);
    DRAG_PACK_GAP.set(0.0);
    assert_eq!(
        origin_beta,
        [0.0, 0.0],
        "origin sibling must not FLIP up after the source left the pack; got {origin_beta:?}"
    );
    assert!(
            (target_gamma[1] - 38.0).abs() < 0.5,
            "target sibling must open the snapshotted packed row (item+gap), not the collapsed live slot; got {}",
            target_gamma[1]
        );
}

/// Later is a singleton, so activate snapshots chip-only. Inbox already
/// has Alpha→Beta author gap — the hole must use that stride, not 32.
#[test]
fn cross_list_occupied_target_uses_its_own_packed_row() {
    let mut alpha = hit("tree-inbox:Alpha", [8.0, 40.0, 160.0, 32.0], true);
    alpha.index = Some(0);
    alpha.list_id = Some("tree-inbox".into());
    let mut beta = hit("tree-inbox:Beta", [8.0, 78.0, 160.0, 32.0], true);
    beta.index = Some(1);
    beta.list_id = Some("tree-inbox".into());
    let mut gamma = hit("tree-later:Gamma", [8.0, 200.0, 160.0, 32.0], true);
    gamma.index = Some(0);
    gamma.list_id = Some("tree-later".into());
    let hits = vec![alpha, beta, gamma];
    LAST_HITS.with(|h| *h.borrow_mut() = hits.clone());
    DRAG_PACK_PITCH.set(32.0);
    DRAG_PACK_GAP.set(0.0);
    let above_alpha = pack_item_translate(
        &hits,
        Some("tree-inbox"),
        Some(0),
        Some("tree-later"),
        Some(0),
        Some("tree-inbox"),
        Some(0),
        Axis::Y,
    );
    let above_beta = pack_item_translate(
        &hits,
        Some("tree-inbox"),
        Some(1),
        Some("tree-later"),
        Some(0),
        Some("tree-inbox"),
        Some(1),
        Axis::Y,
    );
    let alpha_stays = pack_item_translate(
        &hits,
        Some("tree-inbox"),
        Some(0),
        Some("tree-later"),
        Some(0),
        Some("tree-inbox"),
        Some(1),
        Axis::Y,
    );
    assert!(!list_is_append_hover("tree-inbox", Some(0)));
    assert!(!list_is_append_hover("tree-inbox", Some(1)));
    assert!(list_is_append_hover("tree-inbox", Some(2)));
    DRAG_PACK_PITCH.set(0.0);
    DRAG_PACK_GAP.set(0.0);
    LAST_HITS.with(|h| h.borrow_mut().clear());
    assert!(
        (above_alpha[1] - 38.0).abs() < 0.5,
        "insert above Alpha must open Inbox pitch (32+6), not Later chip; got {}",
        above_alpha[1]
    );
    assert!(
        (above_beta[1] - 38.0).abs() < 0.5,
        "insert above Beta must open Inbox pitch; got {}",
        above_beta[1]
    );
    assert_eq!(
        alpha_stays,
        [0.0, 0.0],
        "Alpha stays when the hole is below it; got {alpha_stays:?}"
    );
}

#[test]
fn folder_drag_over_inner_files_stays_on_outer_list() {
    let (_o, hover) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([200.0_f32, 80.0]);
        register_scope(
            "s".into(),
            ScopeRuntime {
                collision: CollisionAlg::MostIntersecting,
                axis: Axis::Y,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: false,
                source_opacity: 1.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 8.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            },
        );
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "folder:Later".into(),
            pointer: [20.0, 220.0],
            grab_offset: [0.0, 0.0],
            start_bounds: [0.0, 208.0, 200.0, 80.0],
            from_list: Some("tree-root".into()),
            from_index: Some(1),
            item_type: "folder".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let mut root = hit("tree-root", [0.0, 0.0, 200.0, 400.0], true);
        root.list_id = Some("tree-root".into());
        root.index = None;
        root.accept = "folder".into();
        root.item_type = String::new();
        let mut inbox = hit("folder:Inbox", [0.0, 0.0, 200.0, 200.0], true);
        inbox.list_id = Some("tree-root".into());
        inbox.index = Some(0);
        inbox.is_draggable = true;
        inbox.item_type = "folder".into();
        let mut inbox_shell = hit("tree-inbox", [8.0, 40.0, 184.0, 150.0], true);
        inbox_shell.list_id = Some("tree-inbox".into());
        inbox_shell.index = None;
        inbox_shell.accept = "file".into();
        inbox_shell.item_type = String::new();
        inbox_shell.group = "nest".into();
        let mut alpha = hit("tree-inbox:Alpha", [8.0, 48.0, 184.0, 32.0], true);
        alpha.list_id = Some("tree-inbox".into());
        alpha.index = Some(0);
        alpha.is_draggable = true;
        alpha.item_type = "file".into();
        alpha.group = "nest".into();
        let mut later = hit("folder:Later", [0.0, 208.0, 200.0, 80.0], true);
        later.list_id = Some("tree-root".into());
        later.index = Some(1);
        later.is_draggable = true;
        later.item_type = "folder".into();
        let hits = vec![root, inbox, inbox_shell, alpha, later];
        assert!(tick(20.0, 60.0, &hits, [400.0, 400.0]));
        let list = hover_list.peek();
        let _ = end(&hits);
        list
    });
    assert_eq!(
        hover.as_deref(),
        Some("tree-root"),
        "folder drag over inner file rows must stay on the outer list, not tree-inbox"
    );
}

#[test]
fn author_modifier_shifts_overlay_origin() {
    use crate::utils::drag::modifier::{DragDelta, DragModifier, MODIFIER_ORDER_AUTHOR};
    let (_o, pos) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([20.0_f32, 20.0]);
        register_scope(
            "s".into(),
            ScopeRuntime {
                collision: CollisionAlg::PointerWithin,
                axis: Axis::Xy,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: false,
                source_opacity: 1.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: vec![DragModifier::new("nudge", MODIFIER_ORDER_AUTHOR, |t, _| {
                    DragDelta {
                        x: t.x + 8.0,
                        y: t.y,
                    }
                })],
                sensors: Vec::new(),
            },
        );
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "a".into(),
            pointer: [10.0, 10.0],
            grab_offset: [0.0, 0.0],
            start_bounds: [10.0, 10.0, 20.0, 20.0],
            from_list: None,
            from_index: None,
            item_type: String::new(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let hits = vec![hit("a", [10.0, 10.0, 20.0, 20.0], false)];
        assert!(tick(10.0, 30.0, &hits, [400.0, 300.0]));
        let pos = overlay_pos.peek();
        let _ = end(&hits);
        pos
    });
    assert!(
        (pos[0] - 18.0).abs() < 0.5 && (pos[1] - 30.0).abs() < 0.5,
        "author modifier must shift origin after sensor delta, got {pos:?}"
    );
}

#[test]
fn add_modifier_survives_register_scope() {
    use crate::utils::drag::modifier::{DragDelta, DragModifier, MODIFIER_ORDER_AUTHOR};
    use crate::utils::drag::session::{add_modifier, remove_modifier};
    let (_o, (kept, after_remove)) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([20.0_f32, 20.0]);
        let mk = |overlay_open,
                  overlay_pos,
                  active_draggable,
                  active_droppable,
                  from_list,
                  from_index,
                  hover_list,
                  hover_index,
                  drag_extent| ScopeRuntime {
            collision: CollisionAlg::PointerWithin,
            axis: Axis::Xy,
            start_axis: Axis::Xy,
            threshold: 0.0,
            restrict: Restrict::None,
            restrict_end_only: false,
            restrict_anchor: RestrictAnchor::FULL,
            snap: None,
            restrict_overflow: RestrictOverflow::Visible,
            overlay_registered: false,
            source_opacity: 1.0,
            autoscroll: AutoScroll {
                enabled: false,
                ..Default::default()
            },
            empty_insert: 0.0,
            delay_ms: 0.0,
            activation: Activation::And,
            collision_fn: None,
            overlay_align: OverlayAlign::Grab,
            can_drop: None,
            on_start: None,
            on_move: None,
            on_over: None,
            on_end: None,
            overlay_open,
            overlay_pos,
            active_draggable,
            active_droppable,
            from_list,
            from_index,
            hover_list,
            hover_index,
            drag_extent,
            stack_seq: 0,
            modifiers: Vec::new(),
            sensors: Vec::new(),
        };
        register_scope(
            "s-mod-rt".into(),
            mk(
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
            ),
        );
        assert!(add_modifier(
            "s-mod-rt",
            DragModifier::new("nudge", MODIFIER_ORDER_AUTHOR, |t, _| DragDelta {
                x: t.x + 8.0,
                y: t.y,
            }),
        ));
        // Provider rebuild replaces builder modifiers; runtime extra must stay.
        register_scope(
            "s-mod-rt".into(),
            mk(
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
            ),
        );
        begin(BeginDrag {
            scope_id: "s-mod-rt".into(),
            draggable_id: "a".into(),
            pointer: [10.0, 10.0],
            grab_offset: [0.0, 0.0],
            start_bounds: [10.0, 10.0, 20.0, 20.0],
            from_list: None,
            from_index: None,
            item_type: String::new(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let mut src = hit("a", [10.0, 10.0, 20.0, 20.0], false);
        src.scope_id = "s-mod-rt".into();
        let hits = vec![src];
        assert!(tick(10.0, 30.0, &hits, [400.0, 300.0]));
        let kept = overlay_pos.peek();
        assert!(remove_modifier("s-mod-rt", "nudge"));
        assert!(tick(10.0, 30.0, &hits, [400.0, 300.0]));
        let after_remove = overlay_pos.peek();
        let _ = end(&hits);
        (kept, after_remove)
    });
    assert!(
        (kept[0] - 18.0).abs() < 0.5 && (kept[1] - 30.0).abs() < 0.5,
        "runtime modifier must survive register_scope, got {kept:?}"
    );
    assert!(
        (after_remove[0] - 10.0).abs() < 0.5 && (after_remove[1] - 30.0).abs() < 0.5,
        "remove_modifier must drop the extra stage, got {after_remove:?}"
    );
}

fn scope_rt(
    collision: CollisionAlg,
    axis: Axis,
    threshold: f32,
    delay_ms: f32,
    activation: Activation,
    collision_fn: Option<CollisionFn>,
    can_drop: Option<DragPredicate>,
    overlay_align: OverlayAlign,
    overlay_open: Signal<bool>,
    overlay_pos: Signal<[f32; 2]>,
    active_draggable: Signal<Option<String>>,
    active_droppable: Signal<Option<String>>,
    from_list: Signal<Option<String>>,
    from_index: Signal<Option<usize>>,
    hover_list: Signal<Option<String>>,
    hover_index: Signal<Option<usize>>,
    drag_extent: Signal<[f32; 2]>,
) -> ScopeRuntime {
    ScopeRuntime {
        collision,
        axis,
        start_axis: Axis::Xy,
        threshold,
        restrict: Restrict::None,
        restrict_end_only: false,
        restrict_anchor: RestrictAnchor::FULL,
        snap: None,
        restrict_overflow: RestrictOverflow::Visible,
        overlay_registered: true,
        source_opacity: 1.0,
        autoscroll: AutoScroll {
            enabled: false,
            ..Default::default()
        },
        empty_insert: 0.0,
        delay_ms,
        activation,
        collision_fn,
        overlay_align,
        can_drop,
        on_start: None,
        on_move: None,
        on_over: None,
        on_end: None,
        overlay_open,
        overlay_pos,
        active_draggable,
        active_droppable,
        from_list,
        from_index,
        hover_list,
        hover_index,
        drag_extent,
        stack_seq: 0,
        modifiers: Vec::new(),
        sensors: Vec::new(),
    }
}

#[test]
fn collision_fn_session_picks_non_enum_winner() {
    let (_o, _) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([0.0_f32, 0.0]);
        let f: CollisionFn = Rc::new(|_| Some("far".into()));
        register_scope(
            "s".into(),
            scope_rt(
                CollisionAlg::ClosestCenter,
                Axis::Xy,
                0.0,
                0.0,
                Activation::And,
                Some(f),
                None,
                OverlayAlign::Grab,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
            ),
        );
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "a".into(),
            pointer: [10.0, 10.0],
            grab_offset: [2.0, 2.0],
            start_bounds: [8.0, 8.0, 40.0, 24.0],
            from_list: Some("list".into()),
            from_index: Some(0),
            item_type: "card".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let drops = vec![
            hit("near", [12.0, 0.0, 10.0, 10.0], true),
            hit("far", [80.0, 80.0, 10.0, 10.0], true),
        ];
        assert!(tick(85.0, 85.0, &drops, [400.0, 300.0]));
        assert_eq!(active_droppable.peek().as_deref(), Some("far"));
        let _ = end(&drops);
    });
}

#[test]
fn collision_fn_then_can_drop_vetoes() {
    let (_o, _) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([0.0_f32, 0.0]);
        let f: CollisionFn = Rc::new(|_| Some("far".into()));
        let pred: DragPredicate = Rc::new(|ev| ev.droppable_id.as_deref() != Some("far"));
        register_scope(
            "s".into(),
            scope_rt(
                CollisionAlg::ClosestCenter,
                Axis::Xy,
                0.0,
                0.0,
                Activation::And,
                Some(f),
                Some(pred),
                OverlayAlign::Grab,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
            ),
        );
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "a".into(),
            pointer: [10.0, 10.0],
            grab_offset: [2.0, 2.0],
            start_bounds: [8.0, 8.0, 40.0, 24.0],
            from_list: Some("list".into()),
            from_index: Some(0),
            item_type: "card".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let drops = vec![
            hit("near", [12.0, 0.0, 10.0, 10.0], true),
            hit("far", [80.0, 80.0, 10.0, 10.0], true),
        ];
        assert!(tick(12.0, 5.0, &drops, [400.0, 300.0]));
        assert!(
            active_droppable.peek().is_none(),
            "can_drop must veto after collision_fn selects"
        );
        let _ = end(&drops);
    });
}

#[test]
fn activation_or_fires_on_delay_without_distance() {
    let (_o, _) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([0.0_f32, 0.0]);
        register_scope(
            "s".into(),
            scope_rt(
                CollisionAlg::PointerWithin,
                Axis::Xy,
                999.0,
                5.0,
                Activation::Or,
                None,
                None,
                OverlayAlign::Grab,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
            ),
        );
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "a".into(),
            pointer: [10.0, 10.0],
            grab_offset: [2.0, 2.0],
            start_bounds: [8.0, 8.0, 40.0, 24.0],
            from_list: Some("list".into()),
            from_index: Some(0),
            item_type: "card".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let drops = vec![hit("zone", [50.0, 50.0, 80.0, 80.0], true)];
        assert!(!tick(10.0, 10.0, &drops, [400.0, 300.0]));
        assert!(!is_active());
        assert!(
            activation_next_wake_at().is_none(),
            "press-only must not arm activation"
        );
        std::thread::sleep(std::time::Duration::from_millis(12));
        assert!(
            !tick(10.0, 10.0, &drops, [400.0, 300.0]),
            "delay without movement is still a press — do not raise/dim the source"
        );
        assert!(!is_active());
        assert!(
            tick(11.0, 10.0, &drops, [400.0, 300.0]),
            "Or + delay already met: first movement must activate immediately"
        );
        assert!(is_active());
        assert!(
            activation_next_wake_at().is_none(),
            "active drag must clear its deadline"
        );
        let _ = end(&drops);
    });
}

#[test]
fn activation_and_needs_distance_even_after_delay() {
    let (_o, _) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([0.0_f32, 0.0]);
        register_scope(
            "s".into(),
            scope_rt(
                CollisionAlg::PointerWithin,
                Axis::Xy,
                999.0,
                5.0,
                Activation::And,
                None,
                None,
                OverlayAlign::Grab,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
            ),
        );
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "a".into(),
            pointer: [10.0, 10.0],
            grab_offset: [2.0, 2.0],
            start_bounds: [8.0, 8.0, 40.0, 24.0],
            from_list: Some("list".into()),
            from_index: Some(0),
            item_type: "card".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let drops = vec![hit("zone", [50.0, 50.0, 80.0, 80.0], true)];
        std::thread::sleep(std::time::Duration::from_millis(12));
        assert!(!tick(10.0, 10.0, &drops, [400.0, 300.0]));
        assert!(!is_active());
        assert!(!tick(11.0, 10.0, &drops, [400.0, 300.0]));
        assert!(
            activation_next_wake_at().is_none(),
            "And still needs the distance threshold"
        );
        let _ = end(&drops);
    });
}

fn axis_hit(
    id: &str,
    list: &str,
    bounds: [f32; 4],
    index: Option<usize>,
    list_axis: Axis,
) -> DragHit {
    DragHit {
        id: id.into(),
        scope_id: "s".into(),
        is_draggable: index.is_some(),
        is_droppable: true,
        rect: Rect::from_bounds(bounds),
        item_type: "card".into(),
        accept: String::new(),
        list_id: Some(list.into()),
        index,
        group: String::new(),
        disabled: false,
        list_axis: Some(list_axis),
        skip_shift: false,
        skip_follow: false,
        pull_clone: false,
    }
}

#[test]
fn multi_drag_preview_and_commit_use_the_same_boundary() {
    use crate::utils::drag::{apply_event, drag_scope, use_drag};
    let (_owner, ()) = create_root(|| {
        let items = Signal::new(vec!["a", "b", "c", "d", "e"]);
        let _scope = drag_scope("s")
            .multi_drag(true)
            .threshold(0.0)
            .on_drag_end(move |ev| {
                let mut v = items.peek();
                apply_event(&mut v, None, ev);
                items.set(v);
            })
            .child(|| {
                use_drag()
                    .unwrap()
                    .selected
                    .set(vec!["a".into(), "b".into(), "c".into()]);
                Element::new()
            });
        for (pointer_y, expected) in [
            (145.0, vec!["d", "e", "a", "b", "c"]),
            (1.0, vec!["a", "b", "c", "d", "e"]),
        ] {
            let mut hits = vec![axis_hit(
                "list",
                "list",
                [0.0, 0.0, 80.0, 160.0],
                None,
                Axis::Y,
            )];
            hits.extend(items.peek().iter().enumerate().map(|(i, id)| {
                axis_hit(
                    id,
                    "list",
                    [0.0, i as f32 * 24.0, 80.0, 20.0],
                    Some(i),
                    Axis::Y,
                )
            }));
            let primary = hits.iter().find(|h| h.id == "a").unwrap();
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "a".into(),
                pointer: [40.0, primary.rect.y + 10.0],
                grab_offset: [40.0, 10.0],
                start_bounds: primary.rect.to_bounds(),
                from_list: Some("list".into()),
                from_index: primary.index,
                item_type: "card".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            assert!(tick(40.0, pointer_y, &hits, [800.0, 600.0]));
            assert_eq!(active_drag_ids(), vec!["a", "b", "c"]);
            let hover = SCOPES
                .with(|scopes| scopes.borrow()["s"].hover_index.peek())
                .unwrap();
            for row in hits.iter().filter(|h| h.index.is_some()) {
                let shift = pack_item_translate(
                    &hits,
                    Some("list"),
                    row.index,
                    Some("list"),
                    primary.index,
                    Some("list"),
                    Some(hover),
                    Axis::Y,
                );
                let expected_index = expected.iter().position(|id| *id == row.id).unwrap();
                assert_eq!(
                    row.rect.y + shift[1],
                    expected_index as f32 * 24.0,
                    "{} must occupy its committed slot",
                    row.id
                );
            }
            assert!(end(&hits));
            assert_eq!(items.peek(), expected);
        }
    });
}

#[test]
fn shift_selection_uses_live_list_order_and_skips_disabled_rows() {
    let (_owner, ()) = create_root(|| {
        let selected = Signal::new(Vec::new());
        register_scope_product("s".into(), SortMode::Insert, true, selected);
        let mut hits: Vec<_> = ["a", "b", "c", "d", "e"]
            .iter()
            .enumerate()
            .map(|(i, id)| {
                axis_hit(
                    id,
                    "list",
                    [0.0, i as f32 * 24.0, 80.0, 20.0],
                    Some(i),
                    Axis::Y,
                )
            })
            .collect();
        hits[3].disabled = true;
        selection_press("s", false);
        try_click_select("s", "b", &hits);
        selection_press("s", true);
        try_click_select("s", "e", &hits);
        assert_eq!(selected.peek(), vec!["b", "c", "e"]);
        selection_press("s", false);
        try_click_select("s", "c", &hits);
        assert_eq!(selected.peek(), vec!["b", "e"]);
    });
}

#[test]
fn swap_hover_picks_item_not_insert_slot() {
    let hits = vec![
        axis_hit("a", "list", [0.0, 0.0, 40.0, 20.0], Some(0), Axis::Y),
        axis_hit("b", "list", [0.0, 24.0, 40.0, 20.0], Some(1), Axis::Y),
        axis_hit("c", "list", [0.0, 48.0, 40.0, 20.0], Some(2), Axis::Y),
    ];
    assert_eq!(
        resolve_swap_hover_index(&hits, "list", [20.0, 34.0]),
        Some(1),
        "pointer inside B"
    );
    assert_eq!(
        resolve_swap_hover_index(&hits, "list", [20.0, 100.0]),
        Some(2),
        "past last row still the last item, not append"
    );
    assert_eq!(
        resolve_swap_hover_index(&hits, "list", [20.0, 10.0]),
        Some(0)
    );
}

#[test]
fn pull_clone_skips_sibling_shift() {
    use crate::utils::drag::registry::{set_item_flags, ItemFlags};
    set_item_flags(
        "s",
        "a",
        ItemFlags {
            pull_clone: true,
            ..Default::default()
        },
    );
    begin(BeginDrag {
        scope_id: "s".into(),
        draggable_id: "a".into(),
        pointer: [0.0, 0.0],
        grab_offset: [0.0, 0.0],
        start_bounds: [0.0, 0.0, 10.0, 10.0],
        from_list: Some("list".into()),
        from_index: Some(0),
        item_type: String::new(),
        group: String::new(),
        capture_id: "cap".into(),
    });
    assert!(active_pull_clone());
    let mut a = hit("a", [0.0, 0.0, 10.0, 10.0], true);
    a.list_id = Some("list".into());
    a.index = Some(0);
    let mut b = hit("b", [0.0, 10.0, 10.0, 10.0], true);
    b.list_id = Some("list".into());
    b.index = Some(1);
    let hits = vec![a, b];
    assert_eq!(
        pack_item_translate(
            &hits,
            Some("list"),
            Some(1),
            Some("list"),
            Some(0),
            Some("list"),
            Some(2),
            Axis::Y
        ),
        [0.0, 0.0]
    );
    let _ = cancel();
}

#[test]
fn sortable_list_direction_overrides_provider_axis() {
    let (_o, (on_vert, on_horiz)) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([80.0_f32, 32.0]);
        register_scope(
            "s".into(),
            scope_rt(
                CollisionAlg::PointerWithin,
                Axis::Y,
                0.0,
                0.0,
                Activation::And,
                None,
                None,
                OverlayAlign::Grab,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
            ),
        );
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "v0".into(),
            pointer: [40.0, 16.0],
            grab_offset: [40.0, 16.0],
            start_bounds: [0.0, 0.0, 80.0, 32.0],
            from_list: Some("vert".into()),
            from_index: Some(0),
            item_type: "card".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let hits = vec![
            axis_hit("v0", "vert", [0.0, 0.0, 80.0, 32.0], Some(0), Axis::Y),
            axis_hit("v1", "vert", [0.0, 40.0, 80.0, 32.0], Some(1), Axis::Y),
            axis_hit("v2", "vert", [0.0, 80.0, 80.0, 32.0], Some(2), Axis::Y),
            axis_hit("vert", "vert", [0.0, 0.0, 80.0, 120.0], None, Axis::Y),
            axis_hit("h0", "horiz", [0.0, 200.0, 40.0, 32.0], Some(0), Axis::X),
            axis_hit("h1", "horiz", [40.0, 200.0, 40.0, 32.0], Some(1), Axis::X),
            axis_hit("h2", "horiz", [80.0, 200.0, 40.0, 32.0], Some(2), Axis::X),
            axis_hit("horiz", "horiz", [0.0, 200.0, 120.0, 32.0], None, Axis::X),
        ];
        assert!(tick(40.0, 56.0, &hits, [400.0, 300.0]));
        let on_vert = hover_index.peek();
        assert_eq!(hover_list.peek().as_deref(), Some("vert"));
        assert!(tick(90.0, 216.0, &hits, [400.0, 300.0]));
        let on_horiz = hover_index.peek();
        assert_eq!(hover_list.peek().as_deref(), Some("horiz"));
        let _ = end(&hits);
        (on_vert, on_horiz)
    });
    assert_eq!(on_vert, Some(1), "vertical list still splits on Y");
    assert_eq!(
        on_horiz,
        Some(2),
        "horizontal list must split on X (Y would yield 0)"
    );
}

#[test]
fn tick_follow_moves_ghost_without_crossing_hover_list() {
    let (_o, _) = create_root(|| {
        let overlay_open = Signal::new(false);
        let overlay_pos = Signal::new([0.0_f32, 0.0]);
        let active_draggable = Signal::new(None::<String>);
        let active_droppable = Signal::new(None::<String>);
        let from_list = Signal::new(None::<String>);
        let from_index = Signal::new(None::<usize>);
        let hover_list = Signal::new(None::<String>);
        let hover_index = Signal::new(None::<usize>);
        let drag_extent = Signal::new([80.0_f32, 32.0]);
        register_scope(
            "s".into(),
            scope_rt(
                CollisionAlg::PointerWithin,
                Axis::Y,
                0.0,
                0.0,
                Activation::And,
                None,
                None,
                OverlayAlign::Grab,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
            ),
        );
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "v0".into(),
            pointer: [40.0, 16.0],
            grab_offset: [40.0, 16.0],
            start_bounds: [0.0, 0.0, 80.0, 32.0],
            from_list: Some("vert".into()),
            from_index: Some(0),
            item_type: "card".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let hits = vec![
            axis_hit("v0", "vert", [0.0, 0.0, 80.0, 32.0], Some(0), Axis::Y),
            axis_hit("v1", "vert", [0.0, 40.0, 80.0, 32.0], Some(1), Axis::Y),
            axis_hit("vert", "vert", [0.0, 0.0, 80.0, 120.0], None, Axis::Y),
            axis_hit("h0", "horiz", [0.0, 200.0, 40.0, 32.0], Some(0), Axis::X),
            axis_hit("horiz", "horiz", [0.0, 200.0, 120.0, 32.0], None, Axis::X),
        ];
        assert!(tick(40.0, 56.0, &hits, [400.0, 300.0]));
        let grab_pos = overlay_pos.peek();
        assert_eq!(hover_list.peek().as_deref(), Some("vert"));
        assert!(tick_follow(90.0, 216.0, [400.0, 300.0]));
        let live = overlay_pos.peek();
        assert_ne!(
            live, grab_pos,
            "ghost origin must follow without collect_hits"
        );
        assert_eq!(
            hover_list.peek().as_deref(),
            Some("vert"),
            "跨区 hover_list must wait for vsync tick"
        );
        assert!(tick(90.0, 216.0, &hits, [400.0, 300.0]));
        assert_eq!(hover_list.peek().as_deref(), Some("horiz"));
        let _ = end(&hits);
    });
}
