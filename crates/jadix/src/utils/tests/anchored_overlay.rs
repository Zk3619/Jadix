use super::*;
use crate::app::portal_overlay::clear_overlay_stack;
use crate::core::reactivity::{create_render_effect, create_root};
use crate::layout::layout_tree::ATTR_PORTAL_KEY;
use crate::utils::popup::{ATTR_FLOATING, ATTR_FLOATING_POINT, ATTR_FLOATING_TRIGGER};

fn is_zero_len(el: &Element) -> bool {
    crate::layout::dim_length_px(el.style.size.width).is_some_and(|w| w == 0.0)
        && crate::layout::dim_length_px(el.style.size.height).is_some_and(|h| h == 0.0)
}

fn find_attr<'a>(el: &'a Element, key: &str) -> Option<&'a str> {
    el.attrs
        .get(key)
        .map(String::as_str)
        .or_else(|| el.children.iter().find_map(|c| find_attr(c, key)))
}

#[test]
fn retained_builder_registers_from_untracked_tree_construction() {
    use crate::app::reactive::{registered_anchor_count, reset_registered_anchors};

    reset_registered_anchors();
    let before = registered_anchor_count();
    let (_owner, tree) = create_root(|| {
        let open = use_signal("retained-overlay-test-open", || false);
        let tree = anchored_overlay("retained-overlay-test")
            .open(open)
            .retained()
            .placement(Placement::trigger(
                "retained-overlay-test-trigger",
                PopupPlacement::BottomStart,
                4.0,
                FloatingStrategy::Auto,
            ))
            .child(|| Element::new().text("Option"));
        assert_eq!(registered_anchor_count(), before + 1);
        tree
    });
    assert!(find_attr(&tree, "jadix_fragment").is_some());
    reset_registered_anchors();
    clear_overlay_stack();
}

#[test]
fn builder_and_opts_share_scope_template() {
    let (_o, (a, b)) = create_root(|| {
        let open = use_signal("ao-dual-open", || false);
        let a = anchored_overlay("ao-dual")
            .open(open)
            .placement(Placement::Unanchored)
            .child(|| Element::new().text("a"));
        let b = anchored_overlay_scope("ao-dual-opts", Some(open), || {
            let mut frag = Element::new();
            frag.set_attr("jadix_fragment", "1");
            frag.children(vec![anchored_overlay_content(
                AnchoredOverlayContentOpts::default().placement(Placement::Unanchored),
                || Element::new().text("b"),
            )])
        });
        (a, b)
    });
    assert_eq!(a.template_id.as_deref(), Some("ao-dual__anchored_overlay"));
    assert_eq!(
        b.template_id.as_deref(),
        Some("ao-dual-opts__anchored_overlay")
    );
    assert!(is_zero_len(&a) || a.children.iter().any(is_zero_len));
    fn find_view(el: &Element, suffix: &str) -> bool {
        el.template_id
            .as_deref()
            .is_some_and(|t| t.ends_with(suffix))
            || el.children.iter().any(|c| find_view(c, suffix))
    }
    assert!(find_view(&a, "__view"), "one-shot must mount 0×0 view");
    assert!(find_view(&b, "__view"), "opts content must mount 0×0 view");
}

#[test]
fn placement_trigger_marks_floating() {
    create_root(|| {
        let mut panel = Element::new().text("p");
        apply_placement(
            &mut panel,
            "ao-t",
            &Placement::Trigger {
                focus_id: "btn".into(),
                placement: PopupPlacement::BottomStart,
                gap: FLOATING_TRIGGER_GAP,
                strategy: FloatingStrategy::Fixed,
            },
            Signal::new(String::new()),
            "",
            false,
            Some("24rem"),
            None,
        );
        assert_eq!(find_attr(&panel, ATTR_FLOATING), Some("ao-t"));
        assert_eq!(find_attr(&panel, ATTR_FLOATING_TRIGGER), Some("btn"));
        assert_eq!(find_attr(&panel, ATTR_FLOATING_STRATEGY), Some("fixed"));
        assert_eq!(find_attr(&panel, ATTR_FLOATING_MAX_H), Some("24rem"));
        assert!(find_attr(&panel, ATTR_FLOATING_POINT).is_none());
    });
}

#[test]
fn placement_width_auto_does_not_seed_explicit_w() {
    create_root(|| {
        let mut panel = Element::new().text("p");
        apply_placement(
            &mut panel,
            "ao-auto-w",
            &Placement::Trigger {
                focus_id: "btn".into(),
                placement: PopupPlacement::BottomStart,
                gap: FLOATING_TRIGGER_GAP,
                strategy: FloatingStrategy::Fixed,
            },
            Signal::new(String::new()),
            "",
            false,
            None,
            Some("auto"),
        );
        assert_eq!(find_attr(&panel, ATTR_FLOATING_MATCH_TRIGGER), Some("0"));
        assert!(
            find_attr(&panel, ATTR_FLOATING_W).is_none(),
            "width=auto must not write jadix_floating_w (sync would parse it as trigger px)"
        );
    });
}

#[test]
fn placement_preserves_author_definite_width_as_floating_w() {
    create_root(|| {
        let mut panel = Element::new().w(256.0).text("shell");
        apply_placement(
            &mut panel,
            "ao-author-w",
            &Placement::Trigger {
                focus_id: "btn".into(),
                placement: PopupPlacement::BottomStart,
                gap: FLOATING_TRIGGER_GAP,
                strategy: FloatingStrategy::Auto,
            },
            Signal::new(String::new()),
            "",
            false,
            None,
            None,
        );
        assert_eq!(find_attr(&panel, ATTR_FLOATING_MATCH_TRIGGER), Some("0"));
        let w = find_attr(&panel, ATTR_FLOATING_W).expect("author width stamped");
        let px: f32 = w.trim_end_matches("px").parse().expect("px length");
        assert!(
            (px - 256.0).abs() < 0.5,
            "shell .w(16rem) must stamp jadix_floating_w, got {w}"
        );
    });
}

#[test]
fn placement_point_drops_trigger_attr() {
    create_root(|| {
        let pos = Signal::new([120.0_f32, 80.0]);
        let mut panel = Element::new().text("p");
        apply_placement(
            &mut panel,
            "ao-p",
            &Placement::Point { pos, gap: 0.0 },
            Signal::new(String::new()),
            "",
            false,
            None,
            None,
        );
        assert_eq!(find_attr(&panel, ATTR_FLOATING), Some("ao-p"));
        assert_eq!(find_attr(&panel, ATTR_FLOATING_POINT), Some("120,80"));
        assert!(
            find_attr(&panel, ATTR_FLOATING_TRIGGER).is_none(),
            "Point must not look like a trigger hit-target"
        );
    });
}

#[test]
fn placement_unanchored_identity_floating() {
    create_root(|| {
        let mut panel = Element::new().text("p");
        apply_placement(
            &mut panel,
            "ao-u",
            &Placement::Unanchored,
            Signal::new("hud-btn".into()),
            "",
            false,
            None,
            None,
        );
        assert_eq!(find_attr(&panel, ATTR_FLOATING), Some("ao-u"));
        assert_eq!(find_attr(&panel, ATTR_FLOATING_TRIGGER), Some("hud-btn"));
        assert!(find_attr(&panel, ATTR_FLOATING_POINT).is_none());
        assert!(
            find_attr(&panel, crate::utils::popup::ATTR_FLOATING_PLACEMENT).is_none(),
            "Unanchored must not set placement (sync would move author inset)"
        );
    });
}

#[test]
fn pinned_point_toggle_closes_once_without_outside_press_reopening() {
    clear_overlay_stack();
    create_root(|| {
        let open = Signal::new(true);
        let presence = use_floating_presence("ao-point-toggle", FloatingMotionKind::Fade);
        let opts = AnchoredOverlayContentOpts::default()
            .placement(Placement::Point {
                pos: Signal::new([120.0, 80.0]),
                gap: 0.0,
            })
            .kind(OverlayKind::Popover)
            .pinned(true);
        assert_eq!(
            anchored_overlay("point-builder").pinned(true).mode,
            opts.mode
        );
        assert_eq!(
            anchored_overlay("point-builder")
                .pinned(true)
                .pinned(false)
                .mode,
            OverlayMode::Stacked
        );
        let mut panel = assemble_panel(
            "ao-point-toggle",
            open,
            Signal::new("unused-default".into()),
            &presence,
            &opts,
            Element::new().w(100.0).h(60.0),
        );
        assert_eq!(find_attr(&panel, ATTR_FLOATING_POINT), Some("120,80"));
        panel.set_attr(crate::layout::layout_tree::ATTR_HOISTED_PORTAL, "1");
        let mut button = Element::new()
            .focus_id("point-toggle-button")
            .on_click(move || open.set(!open.peek()));
        button.layout_bounds = [0.0, 0.0, 100.0, 30.0];
        let mut root = Element::new().child(button).child(panel);
        root.layout_bounds = [0.0, 0.0, 640.0, 480.0];
        let path = crate::app::pointer_events::hit_path(&root, 20.0, 15.0);
        crate::utils::popup::arm_underlay_close();
        assert!(!crate::app::portal_overlay::try_close_top());
        if !crate::utils::popup::path_hits_floating_trigger(&root, &path) {
            crate::app::portal_overlay::try_dismiss_outside_press();
        }
        assert!(
            open.peek(),
            "toggle pointer-down must not dismiss the Point panel"
        );
        let (click, _) = crate::app::pointer_events::resolve_click_from_path(&root, &path);
        click.expect("toggle callback").borrow_mut()();
        assert!(!open.peek(), "pointer-up must close, not reopen");
        assert!(!crate::app::portal_overlay::try_dismiss_outside_press());
        assert!(!open.peek(), "close must stay closed after outside-dismiss");
    });
    clear_overlay_stack();
}

#[test]
fn placement_unanchored_prefers_content_focus_id() {
    create_root(|| {
        let mut panel = Element::new().text("p");
        apply_placement(
            &mut panel,
            "ao-u2",
            &Placement::Unanchored,
            Signal::new("default-trig".into()),
            "rsx-btn",
            false,
            None,
            None,
        );
        assert_eq!(find_attr(&panel, ATTR_FLOATING_TRIGGER), Some("rsx-btn"));
    });
}

#[test]
fn open_unanchored_uses_zero_anchor_without_viewport_stage() {
    clear_overlay_stack();
    create_root(|| {
        let open = Signal::new(true);
        let presence = use_floating_presence("ao-open-u", FloatingMotionKind::Fade);
        let opts = AnchoredOverlayContentOpts::default()
            .placement(Placement::Unanchored)
            .kind(OverlayKind::Popover);
        let mounted = assemble_panel(
            "ao-open-u",
            open,
            Signal::new("hud-btn".into()),
            &presence,
            &opts,
            Element::new().text("hud").fixed().top("1rem").right("1rem"),
        );
        assert!(
            is_zero_len(&mounted),
            "Unanchored keeps the same 0×0 document-flow mount as Trigger"
        );
        assert_eq!(
            find_attr(&mounted, ATTR_PORTAL_KEY),
            Some("panel:ao-open-u"),
            "portal key must be instance-unique, not panel:{{z}}"
        );
        assert_eq!(
            find_attr(&mounted, ATTR_FLOATING),
            Some("ao-open-u"),
            "identity ATTR_FLOATING so panel hits are not outside-press"
        );
        assert_eq!(
            find_attr(&mounted, ATTR_FLOATING_TRIGGER),
            Some("hud-btn"),
            "toggle focus_id must be registered for outside-press exemption"
        );
        assert!(
            find_attr(&mounted, crate::utils::popup::ATTR_FLOATING_PLACEMENT).is_none(),
            "must not place against the trigger"
        );
    });
    clear_overlay_stack();
}

#[test]
fn point_drag_updates_pos_signal() {
    create_root(|| {
        let pos = Signal::new([10.0_f32, 20.0]);
        begin_floating_panel_drag("ao-drag", pos, 10.0, 20.0);
        assert!(tick_floating_panel_drag(25.0, 40.0));
        assert_eq!(pos.peek(), [25.0, 40.0]);
        clear_floating_panel_drag();
        assert!(!is_floating_panel_dragging());
    });
}

#[test]
fn point_placement_does_not_subscribe_pos() {
    use std::cell::Cell;
    use std::rc::Rc;
    let (_o, _) = create_root(|| {
        let pos = Signal::new([10.0_f32, 20.0]);
        let runs = Rc::new(Cell::new(0));
        let runs2 = runs.clone();
        create_render_effect(move || {
            runs2.set(runs2.get() + 1);
            let mut panel = Element::new().text("p");
            apply_placement(
                &mut panel,
                "ao-p-sub",
                &Placement::Point { pos, gap: 0.0 },
                Signal::new(String::new()),
                "",
                false,
                None,
                None,
            );
        });
        assert_eq!(runs.get(), 1);
        pos.set([40.0, 60.0]);
        assert_eq!(
            runs.get(),
            1,
            "apply_placement must peek pos; .get remounted the overlay every tick"
        );
    });
}

#[test]
fn point_drag_patches_collect_origin() {
    create_root(|| {
        let pos = Signal::new([10.0_f32, 20.0]);
        let mut panel = Element::new().text("p");
        apply_placement(
            &mut panel,
            "ao-p-drag",
            &Placement::Point { pos, gap: 0.0 },
            Signal::new(String::new()),
            "",
            false,
            None,
            None,
        );
        panel.style.inset.left = Element::into_margin_auto(10.0);
        panel.style.inset.top = Element::into_margin_auto(20.0);
        begin_floating_panel_drag("ao-p-drag", pos, 10.0, 20.0);
        assert!(tick_floating_panel_drag(40.0, 55.0));
        sync_point_overlay_drag_insets(&mut panel);
        assert_eq!(find_attr(&panel, ATTR_FLOATING_POINT), Some("40,55"));
        assert_eq!(
            crate::core::tree::lpa_length_px(panel.style.inset.left),
            Some(10.0),
            "Taffy inset stays at grab origin during drag"
        );
        assert_eq!(
            crate::core::tree::lpa_length_px(panel.style.inset.top),
            Some(20.0)
        );
        assert_eq!(panel.style.translate, [0.0, 0.0]);
        let moved = adjust_point_overlay_collect_origin(&mut panel);
        assert_eq!(
            moved,
            Some([40.0, 55.0]),
            "collect must patch abs so occupancy / children follow"
        );
        assert!(take_ended_floating_panel_drag() == false);
        clear_floating_panel_drag();
        sync_point_overlay_drag_insets(&mut panel);
        assert_eq!(
            crate::core::tree::lpa_length_px(panel.style.inset.left),
            Some(40.0)
        );
        assert_eq!(
            crate::core::tree::lpa_length_px(panel.style.inset.top),
            Some(55.0)
        );
        assert_eq!(panel.style.translate, [0.0, 0.0]);
        assert_eq!(
            take_point_overlay_commit_dirty().as_deref(),
            Some("ao-p-drag")
        );
        assert!(
            take_ended_floating_panel_drag(),
            "mouseup must see the ended drag so outside-press does not close the panel"
        );
        assert!(!take_ended_floating_panel_drag());
    });
}

#[test]
fn point_drag_second_grab_keeps_pointer_offset() {
    create_root(|| {
        let pos = Signal::new([10.0_f32, 20.0]);
        let mut panel = Element::new().text("p");
        apply_placement(
            &mut panel,
            "ao-p-grab2",
            &Placement::Point { pos, gap: 0.0 },
            Signal::new(String::new()),
            "",
            false,
            None,
            None,
        );
        panel.style.inset.left = Element::into_margin_auto(18.0);
        panel.style.inset.top = Element::into_margin_auto(28.0);
        begin_floating_panel_drag("ao-p-grab2", pos, 10.0, 20.0);
        sync_point_overlay_drag_insets(&mut panel);
        assert!(tick_floating_panel_drag(80.0, 90.0));
        assert_eq!(
            adjust_point_overlay_collect_origin(&mut panel),
            Some([88.0, 98.0])
        );
        clear_floating_panel_drag();
        sync_point_overlay_drag_insets(&mut panel);
        let _ = take_point_overlay_commit_dirty();
        let _ = take_ended_floating_panel_drag();

        begin_floating_panel_drag("ao-p-grab2", pos, 90.0, 100.0);
        sync_point_overlay_drag_insets(&mut panel);
        assert_eq!(
            crate::core::tree::lpa_length_px(panel.style.inset.left),
            Some(88.0),
            "second grab must pin Taffy at the committed origin"
        );
        assert!(tick_floating_panel_drag(110.0, 120.0));
        assert_eq!(
            pos.peek(),
            [100.0, 110.0],
            "pos stays click/anchor + pointer delta"
        );
        assert_eq!(
            adjust_point_overlay_collect_origin(&mut panel),
            Some([108.0, 118.0]),
            "second grab must keep the pointer-to-panel offset"
        );
        clear_floating_panel_drag();
        let _ = take_point_overlay_commit_dirty();
        let _ = take_ended_floating_panel_drag();
    });
}

#[test]
fn point_drag_begin_keeps_unapplied_commit() {
    create_root(|| {
        let pos = Signal::new([10.0_f32, 20.0]);
        let mut panel = Element::new().text("p");
        apply_placement(
            &mut panel,
            "ao-p-pending",
            &Placement::Point { pos, gap: 0.0 },
            Signal::new(String::new()),
            "",
            false,
            None,
            None,
        );
        panel.style.inset.left = Element::into_margin_auto(18.0);
        panel.style.inset.top = Element::into_margin_auto(28.0);
        begin_floating_panel_drag("ao-p-pending", pos, 10.0, 20.0);
        sync_point_overlay_drag_insets(&mut panel);
        assert!(tick_floating_panel_drag(80.0, 90.0));
        assert_eq!(
            adjust_point_overlay_collect_origin(&mut panel),
            Some([88.0, 98.0])
        );
        clear_floating_panel_drag();
        begin_floating_panel_drag("ao-p-pending", pos, 90.0, 100.0);
        sync_point_overlay_drag_insets(&mut panel);
        assert_eq!(
            crate::core::tree::lpa_length_px(panel.style.inset.left),
            Some(88.0),
            "unapplied mouseup commit is the next grab origin"
        );
        assert!(tick_floating_panel_drag(110.0, 120.0));
        assert_eq!(
            adjust_point_overlay_collect_origin(&mut panel),
            Some([108.0, 118.0]),
            "follow is committed origin + pointer delta, not the first grab box"
        );
        clear_floating_panel_drag();
        let _ = take_point_overlay_commit_dirty();
        let _ = take_ended_floating_panel_drag();
    });
}

#[test]
fn point_drag_tick_queues_overlay_damage_before_render() {
    create_root(|| {
        let pos = Signal::new([10.0_f32, 20.0]);
        let mut panel = Element::new().w(160.0).h(72.0);
        apply_placement(
            &mut panel,
            "ao-p-tick",
            &Placement::Point { pos, gap: 0.0 },
            Signal::new(String::new()),
            "",
            false,
            None,
            None,
        );
        panel.style.inset.left = Element::into_margin_auto(10.0);
        panel.style.inset.top = Element::into_margin_auto(20.0);
        panel.layout_bounds = [10.0, 20.0, 160.0, 72.0];
        begin_floating_panel_drag("ao-p-tick", pos, 10.0, 20.0);
        sync_point_overlay_drag_insets(&mut panel);
        let _ = crate::app::damage::take_frame_paint_nodes();
        assert!(tick_floating_panel_drag(80.0, 90.0));
        let dirties = crate::app::damage::take_frame_paint_nodes();
        assert!(
            dirties.iter().any(|d| {
                d.kind == crate::app::damage::PaintDirtyKind::Overlay
                    && d.bounds[0] <= 80.5
                    && d.bounds[1] <= 90.5
                    && d.bounds[0] + d.bounds[2] >= 80.0 + 160.0 - 0.5
                    && d.bounds[1] + d.bounds[3] >= 90.0 + 72.0 - 0.5
            }),
            "CursorMoved must dest-reset the live origin (same-key union ok); got {dirties:?}"
        );
        clear_floating_panel_drag();
        let _ = take_point_overlay_commit_dirty();
    });
}

#[test]
fn point_drag_collect_keeps_full_panel_box() {
    use crate::layout::layout_tree::{hoist_portal_subtrees, hoisted_portal_stamps, LayoutTree};
    use crate::utils::popup::mark_portal;

    let (_o, mut root) = create_root(|| {
        let pos = Signal::new([40.0_f32, 50.0]);
        let mut panel = Element::new()
            .w(160.0)
            .h(72.0)
            .bg([1.0, 1.0, 1.0, 1.0])
            .child(Element::new().text("RSX Point").w(140.0).h(24.0));
        apply_placement(
            &mut panel,
            "ao-p-clip",
            &Placement::Point { pos, gap: 0.0 },
            Signal::new(String::new()),
            "",
            false,
            None,
            None,
        );
        panel.style.inset.left = Element::into_margin_auto(40.0);
        panel.style.inset.top = Element::into_margin_auto(50.0);
        panel = mark_portal(panel);
        begin_floating_panel_drag("ao-p-clip", pos, 40.0, 50.0);
        assert!(tick_floating_panel_drag(140.0, 160.0));
        Element::new().w(400.0).h(300.0).child(panel)
    });

    assert!(hoist_portal_subtrees(&mut root) >= 1);
    let mut tree = LayoutTree::new();
    let _result = tree.build_and_layout(&mut root, 400.0, 300.0, [0.0, 0.0]);

    fn find_point(el: &Element) -> Option<&Element> {
        if el.attrs.get(ATTR_FLOATING_POINT).is_some()
            && el.attrs.get(ATTR_FLOATING).map(String::as_str) == Some("ao-p-clip")
        {
            return Some(el);
        }
        el.children.iter().find_map(find_point)
    }
    let panel = find_point(&root).expect("point panel");
    assert!(
        (panel.layout_bounds[0] - 140.0).abs() < 1.0
            && (panel.layout_bounds[1] - 160.0).abs() < 1.0,
        "collect abs must be the live origin, not the grab box; bounds={:?}",
        panel.layout_bounds
    );
    assert!(
        panel.layout_bounds[2] > 150.0 && panel.layout_bounds[3] > 60.0,
        "panel size must not collapse during drag; bounds={:?}",
        panel.layout_bounds
    );
    assert_eq!(panel.style.translate, [0.0, 0.0]);

    let stamps = hoisted_portal_stamps(&root);
    let stamp = stamps
        .iter()
        .find(|(k, b)| {
            *k == crate::layout::layout_tree::overlay_portal_stamp_key(panel) && b[2] > 10.0
        })
        .map(|(_, b)| *b)
        .or_else(|| stamps.iter().find(|(_, b)| b[2] > 10.0).map(|(_, b)| *b))
        .expect("hoisted occupancy");
    assert!(
        (stamp[0] - 140.0).abs() < 1.5 && (stamp[1] - 160.0).abs() < 1.5,
        "portal occupancy must follow the panel, not stay on the grab box; stamp={stamp:?}"
    );
    assert!(
        stamp[2] > 150.0 && stamp[3] > 60.0,
        "occupancy must cover the full panel; stamp={stamp:?}"
    );
    clear_floating_panel_drag();
    let _ = take_point_overlay_commit_dirty();
}

#[test]
fn smoke_phase_hides_presence_handle() {
    use crate::utils::presence::PresencePhase;
    clear_overlay_stack();
    create_root(|| {
        let open = Signal::new(true);
        let opts = AnchoredOverlayContentOpts::default()
            .kind(OverlayKind::Popover)
            .motion(FloatingMotionKind::Fade);
        assert_eq!(
            anchored_overlay_phase("ao-phase", open, &opts),
            Some(PresencePhase::Present)
        );
        open.set(false);
        assert_eq!(
            anchored_overlay_phase("ao-phase", open, &opts),
            cfg!(feature = "animation-base").then_some(PresencePhase::Exiting)
        );
        assert_eq!(
            anchored_overlay_is_mounted("ao-phase", &opts),
            cfg!(feature = "animation-base")
        );
    });
    clear_overlay_stack();
}

#[cfg(feature = "animation-base")]
fn has_portal(el: &Element) -> bool {
    el.attrs.get("jadix_portal").map(String::as_str) == Some("1")
        || el.children.iter().any(has_portal)
}

fn find_view(el: &Element, suffix: &str) -> bool {
    el.template_id
        .as_deref()
        .is_some_and(|t| t.ends_with(suffix))
        || el.children.iter().any(|c| find_view(c, suffix))
}

#[test]
fn wave2_canary_open_island_does_not_subscribe_ui_root() {
    use crate::app::reactive::reset_registered_anchors;
    use crate::core::reactivity::{
        clear_ui_root_effect, create_effect, dispose_effect, mark_next_effect_as_ui_root,
        ui_root_effect_dep_count,
    };
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    reset_registered_anchors();
    clear_overlay_stack();
    clear_ui_root_effect();
    let open = Signal::new(false);
    let runs = Rc::new(Cell::new(0));
    let kept = Rc::new(RefCell::new(None::<Element>));
    let runs_e = Rc::clone(&runs);
    let kept_e = Rc::clone(&kept);

    mark_next_effect_as_ui_root();
    let root = create_effect(move || {
        runs_e.set(runs_e.get() + 1);
        let el = anchored_overlay("ao-w2-root")
            .open(open)
            .placement(Placement::Unanchored)
            .child(|| Element::new().text("x").w(160.0).h(80.0));
        *kept_e.borrow_mut() = Some(el);
    });

    assert_eq!(runs.get(), 1);
    assert_eq!(
        ui_root_effect_dep_count(),
        0,
        "open must subscribe the structure island, not the UI root"
    );
    {
        let mounted = kept.borrow();
        let el = mounted.as_ref().expect("mounted");
        assert!(find_view(el, "__view"));
    }

    open.set(true);
    assert_eq!(runs.get(), 1, "open must not rebuild the UI root");
    assert_eq!(ui_root_effect_dep_count(), 0);

    drop(kept.borrow_mut().take());
    dispose_effect(root);
    clear_ui_root_effect();
    reset_registered_anchors();
    clear_overlay_stack();
}

#[test]
#[cfg(feature = "animation-base")]
fn wave2_canary_close_keeps_portal_while_exiting() {
    use crate::utils::presence::PresencePhase;

    clear_overlay_stack();
    create_root(|| {
        let open = Signal::new(true);
        let presence = use_floating_presence("ao-w2-exit", FloatingMotionKind::Fade);
        let opts = AnchoredOverlayContentOpts::default()
            .placement(Placement::Unanchored)
            .kind(OverlayKind::Popover);
        let trigger = Signal::new("hud-btn".into());
        let panel = || Element::new().text("hud").w(200.0).h(80.0);
        let mounted = assemble_panel("ao-w2-exit", open, trigger, &presence, &opts, panel());
        assert!(has_portal(&mounted), "open must portal the panel");
        assert_eq!(
            anchored_overlay_phase("ao-w2-exit", open, &opts),
            Some(PresencePhase::Present)
        );

        open.set(false);
        assert_eq!(
            anchored_overlay_phase("ao-w2-exit", open, &opts),
            Some(PresencePhase::Exiting)
        );
        assert!(anchored_overlay_is_mounted("ao-w2-exit", &opts));
        let exiting = assemble_panel("ao-w2-exit", open, trigger, &presence, &opts, panel());
        assert!(
            has_portal(&exiting),
            "exit must keep the portal until the gate completes"
        );

        presence.force_absent_flags();
        floating_mounted_signal(&presence).set(false);
        assert!(!anchored_overlay_is_mounted("ao-w2-exit", &opts));
        let gone = assemble_panel("ao-w2-exit", open, trigger, &presence, &opts, panel());
        assert!(
            !has_portal(&gone),
            "Absent must drop the portal after exit completes"
        );
    });
    clear_overlay_stack();
}

/// Select / DatePicker open: panel must not collect a visible silhouette at the
/// portal origin (0,0) for one frame before trigger placement lands.
#[test]
fn trigger_first_open_must_not_flash_visible_panel_at_portal_origin() {
    use crate::app::portal_overlay::{clear_overlay_stack, OverlayKind, OverlayMode};
    use crate::layout::layout_tree::{hoist_portal_subtrees, LayoutTree, ATTR_HOISTED_PORTAL};
    use crate::utils::floating_motion::{
        floating_motion_needs_relayout, flush_floating_motion_subtree,
        mark_floating_panels_layout_dirty, tick_hoisted_floating_css,
    };
    use crate::utils::popup::{PopupPlacement, FLOATING_TRIGGER_GAP};

    clear_overlay_stack();
    let (_owner, mut root) = create_root(|| {
        let open = Signal::new(true);
        let presence = use_floating_presence("ao-origin-flash", FloatingMotionKind::Fade);
        let opts = AnchoredOverlayContentOpts {
            placement: Some(Placement::trigger(
                "ao-origin-flash-trigger",
                PopupPlacement::BottomStart,
                FLOATING_TRIGGER_GAP,
                FloatingStrategy::Auto,
            )),
            kind: OverlayKind::Popover,
            motion: FloatingMotionKind::Fade,
            mode: OverlayMode::Stacked,
            match_trigger_width: false,
            ..Default::default()
        };
        let trigger = Element::new()
            .focus_id("ao-origin-flash-trigger")
            .w(120.0)
            .h(36.0)
            .absolute()
            .left(220.0)
            .top(180.0)
            .bg([0.2, 0.2, 0.25, 1.0])
            .text("Trigger");
        let mounted = assemble_panel(
            "ao-origin-flash",
            open,
            Signal::new("ao-origin-flash-trigger".into()),
            &presence,
            &opts,
            Element::new()
                .text("menu-body")
                .w(160.0)
                .h(120.0)
                .bg([1.0, 1.0, 1.0, 1.0]),
        );
        Element::new()
            .w(800.0)
            .h(600.0)
            .relative()
            .child(trigger)
            .child(mounted)
    });

    let _ = hoist_portal_subtrees(&mut root);
    let mut tree = LayoutTree::new();
    let mut out = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);

    fn find_panel<'a>(el: &'a Element) -> Option<&'a Element> {
        if el.template_id.as_deref() == Some("ao-origin-flash__floating_panel") {
            return Some(el);
        }
        el.children.iter().find_map(find_panel)
    }
    fn find_fid<'a>(el: &'a Element, fid: &str) -> Option<&'a Element> {
        if el.focus_id.as_deref() == Some(fid) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_fid(c, fid))
    }
    let panel_after_layout = find_panel(&root).expect("floating panel after layout");
    let trigger = find_fid(&root, "ao-origin-flash-trigger").expect("trigger");
    let left = crate::layout::lpa_length_px(panel_after_layout.style.inset.left);
    let top = crate::layout::lpa_length_px(panel_after_layout.style.inset.top);
    assert!(
        trigger.layout_bounds[2] > 1.0 && trigger.layout_bounds[3] > 1.0,
        "precondition: trigger must have layout bounds; got {:?}",
        trigger.layout_bounds
    );
    assert!(
            left.is_some_and(|x| x > 100.0) && top.is_some_and(|y| y > 100.0),
            "sync inside build_and_layout must write trigger-relative insets before collect; left={left:?} top={top:?} bounds={:?} trigger={:?}",
            panel_after_layout.layout_bounds,
            trigger.layout_bounds
        );

    // App present: flush motion then optional follow / animation collect.
    let _ = flush_floating_motion_subtree(&mut root);
    let floating_css = tick_hoisted_floating_css(&mut root);
    if floating_motion_needs_relayout() {
        mark_floating_panels_layout_dirty(&root, &mut tree);
        out = tree.relayout_overlay_follow_and_collect(
            &mut root,
            800.0,
            600.0,
            [0.0, 0.0],
            false,
            false,
        );
    } else if floating_css {
        out = tree.collect_animation_frame(&mut root, [0.0, 0.0], false);
    }

    let panel = find_panel(&root).expect("floating panel");
    assert_eq!(
        panel.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str),
        Some("1"),
        "panel must be hoisted"
    );
    let b = panel.layout_bounds;
    assert!(
        b[0] > 100.0 && b[1] > 100.0,
        "first open layout must sit near the trigger, not portal origin; bounds={b:?} opacity={}",
        panel.style.opacity
    );
    let visible_at_origin = out.instances.iter().any(|i| {
        i.color[3] > 0.05
            && i.rect_size[0] > 40.0
            && i.rect_size[1] > 40.0
            && i.rect_pos[0] < 40.0
            && i.rect_pos[1] < 40.0
    });
    assert!(
            !visible_at_origin,
            "must not collect a visible panel silhouette at portal origin; opacity={} bounds={b:?} instances={:?}",
            panel.style.opacity,
            out.instances
                .iter()
                .filter(|i| i.color[3] > 0.05 && i.rect_size[0] > 20.0)
                .map(|i| (i.rect_pos, i.rect_size, i.color[3]))
                .collect::<Vec<_>>()
        );
    clear_overlay_stack();
}

#[test]
fn wave2_canary_first_open_layouts_portal_panel() {
    use crate::layout::layout_tree::{
        hoist_portal_subtrees, LayoutTree, ATTR_HOISTED_PORTAL, PORTAL_LAYER_ID,
    };

    clear_overlay_stack();
    let (_owner, mut root) = create_root(|| {
        let open = Signal::new(true);
        let presence = use_floating_presence("ao-w2-first", FloatingMotionKind::Fade);
        let opts = AnchoredOverlayContentOpts::default()
            .placement(Placement::Unanchored)
            .kind(OverlayKind::Popover);
        let mounted = assemble_panel(
            "ao-w2-first",
            open,
            Signal::new("hud-btn".into()),
            &presence,
            &opts,
            Element::new()
                .text("wave2-ao")
                .w(200.0)
                .h(80.0)
                .bg([1.0, 1.0, 1.0, 1.0])
                .fixed()
                .top("1rem")
                .right("1rem"),
        );
        Element::new()
            .w(800.0)
            .h(600.0)
            .child(Element::new().text("page"))
            .child(mounted)
    });

    assert!(
        hoist_portal_subtrees(&mut root) >= 1,
        "first open must hoist the shell onto the portal layer"
    );
    assert!(
        root.children
            .iter()
            .any(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID)),
        "portal layer must exist after hoist"
    );
    assert_eq!(find_attr(&root, ATTR_HOISTED_PORTAL), Some("1"));

    let mut tree = LayoutTree::new();
    let _result = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
    fn find_hoisted(el: &Element) -> Option<&Element> {
        if el.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) == Some("1") {
            return Some(el);
        }
        el.children.iter().find_map(find_hoisted)
    }
    let hoisted = find_hoisted(&root).expect("hoisted portal panel");
    fn subtree_has_sized_box(el: &Element) -> bool {
        (el.layout_bounds[2] > 1.0 && el.layout_bounds[3] > 1.0)
            || el.children.iter().any(subtree_has_sized_box)
    }
    assert!(
        subtree_has_sized_box(hoisted),
        "first open must layout the portal panel, not skip as paint-only; hoisted bounds={:?}",
        hoisted.layout_bounds
    );
    clear_overlay_stack();
}

#[test]
fn fade_popover_join_stamp_matches_band_above_leftover_datepicker() {
    use crate::app::damage::{
        overlay_band_encode_flags, overlay_membership_paint_stamps, push_frame_paint_element,
        take_frame_paint_nodes, PaintDirtyKind,
    };
    use crate::layout::layout_tree::{
        hoist_portal_subtrees, overlay_portal_stamp_key, LayoutTree, ATTR_HOISTED_PORTAL,
        PORTAL_LAYER_ID,
    };
    use crate::utils::floating_motion::{flush_floating_motion_subtree, tick_hoisted_floating_css};
    use crate::utils::popup::sync_floating_placement;

    clear_overlay_stack();
    let _ = take_frame_paint_nodes();
    let (_owner, mut root) = create_root(|| {
        let dp_open = Signal::new(true);
        let dp_presence = use_floating_presence("ao-dp-left", FloatingMotionKind::Expand);
        let dp_opts = AnchoredOverlayContentOpts::default()
            .placement(Placement::Unanchored)
            .kind(OverlayKind::Popover)
            .motion(FloatingMotionKind::Expand);
        let datepicker = assemble_panel(
            "ao-dp-left",
            dp_open,
            Signal::new("dp-btn".into()),
            &dp_presence,
            &dp_opts,
            Element::new()
                .text("calendar")
                .w(280.0)
                .h(320.0)
                .bg([1.0, 1.0, 1.0, 1.0])
                .fixed()
                .top(344.0)
                .left(64.0),
        );

        let cp_open = Signal::new(true);
        let cp_presence = use_floating_presence("ao-cp-fade", FloatingMotionKind::Fade);
        let cp_opts = AnchoredOverlayContentOpts::default()
            .placement(Placement::Unanchored)
            .kind(OverlayKind::Popover)
            .motion(FloatingMotionKind::Fade)
            .width("17.5rem");
        let mut knob = Element::new().w(12.0).h(12.0).bg([1.0, 0.0, 0.0, 1.0]);
        knob = knob.absolute();
        let colorpicker = assemble_panel(
            "ao-cp-fade",
            cp_open,
            Signal::new("cp-btn".into()),
            &cp_presence,
            &cp_opts,
            Element::new()
                .w(280.0)
                .h(360.0)
                .bg([1.0, 1.0, 1.0, 1.0])
                .child(knob)
                .fixed()
                .top(120.0)
                .left(640.0),
        );

        Element::new()
            .w(800.0)
            .h(600.0)
            .child(datepicker)
            .child(colorpicker)
    });

    assert!(hoist_portal_subtrees(&mut root) >= 1);
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
    if sync_floating_placement(&mut root, 800.0, 600.0) {
        let _ =
            tree.build_and_layout_fast_ex(&mut root, 800.0, 600.0, [0.0, 0.0], false, false, None);
    }
    let _ = flush_floating_motion_subtree(&mut root);
    let _ = tick_hoisted_floating_css(&mut root);
    let out =
        tree.build_and_layout_fast_ex(&mut root, 800.0, 600.0, [0.0, 0.0], false, false, None);

    fn find_tid<'a>(el: &'a Element, suffix: &str) -> Option<&'a Element> {
        if el
            .template_id
            .as_deref()
            .is_some_and(|t| t.ends_with(suffix))
        {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_tid(c, suffix))
    }
    let cp = find_tid(&root, "ao-cp-fade__floating_panel")
        .expect("ColorPicker Fade panel must keep __floating_panel tid after hoist");
    assert!(
        cp.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) == Some("1")
            || find_tid(&root, PORTAL_LAYER_ID).is_some(),
        "ColorPicker Fade must live on the portal layer"
    );
    assert!(
        cp.layout_bounds[2] > 8.0 && cp.layout_bounds[3] > 8.0,
        "ColorPicker Fade box must be the popover, not 0×0; bounds={:?} opacity={}",
        cp.layout_bounds,
        cp.style.opacity
    );

    let panel_key = overlay_portal_stamp_key(cp);
    let band_keys: Vec<u64> = out
        .overlay_split
        .band_ranges(
            out.instances.len(),
            out.textured_draws.len(),
            out.text_nodes.len(),
            0,
            0,
        )
        .iter()
        .map(|b| b.stamp_key)
        .collect();
    assert!(
            band_keys.iter().any(|k| *k == panel_key),
            "portal-layer band stamp must be overlay_portal_stamp_key of the Fade panel; panel_key={panel_key} bands={band_keys:?}"
        );

    let _ = take_frame_paint_nodes();
    push_frame_paint_element(cp, [0.0, 0.0]);
    let dirties = take_frame_paint_nodes();
    let overlay_dirties: Vec<u64> = dirties
        .iter()
        .filter(|d| d.kind == PaintDirtyKind::Overlay)
        .map(|d| d.key)
        .collect();
    assert_eq!(
        overlay_dirties,
        vec![panel_key],
        "Fade Overlay dirty must be the portal stamp, not tid/NodeId; dirties={dirties:?}"
    );
    let flags = overlay_band_encode_flags(&band_keys, &overlay_dirties, false, &[panel_key]);
    assert!(
        flags.iter().any(|e| *e),
        "ColorPicker Fade band must encode; bands={band_keys:?} flags={flags:?}"
    );

    let dp = find_tid(&root, "ao-dp-left__floating_panel");
    if let Some(dp) = dp {
        let dp_key = overlay_portal_stamp_key(dp);
        let color = cp.layout_bounds;
        let leftover = dp.layout_bounds;
        let joined = overlay_membership_paint_stamps(&[(dp_key, leftover)], &[(panel_key, color)]);
        assert_eq!(
            joined.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
            vec![panel_key],
            "opening ColorPicker must Overlay-paint ColorPicker, not leftover DatePicker"
        );
    }
    clear_overlay_stack();
}

/// TabsContent dispose kills the overlay owner. The hoisted panel must still
/// play Fade exit (`fade-out-0`) instead of being pruned on the same frame.
#[test]
#[cfg(feature = "animation-base")]
#[cfg(feature = "product-tests")]
fn tab_unmount_plays_configured_floating_exit() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::app::portal_overlay::clear_overlay_stack;
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::components::tabs::{tabs_content, tabs_scope};
    use crate::core::reactivity::{component_root, create_effect, create_root, use_signal};
    use crate::layout::layout_tree::{hoist_portal_subtrees, LayoutTree};
    use crate::utils::floating_motion::{
        drain_pending_floating_unmounts, flush_floating_motion_subtree,
        mark_floating_panels_layout_dirty, tick_floating_presence, tick_hoisted_floating_css,
    };
    use crate::utils::popup::sync_floating_placement;

    clear_overlay_stack();
    reset_registered_anchors();
    clear_test_reconcile_root();

    let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));

    let (owner, tab) = create_root(|| {
        let tab = use_signal("ao-tab-exit__tabs", || "a".to_string());
        let live_e = Rc::clone(&live);
        let tab_e = tab;
        create_effect(move || {
            *live_e.borrow_mut() =
                tabs_scope("ao-tab-exit", Some(tab_e), "a", "horizontal", false, || {
                    let mut frag = Element::new();
                    frag.set_attr("jadix_fragment", "1");
                    frag = frag.children(vec![
                        tabs_content("a", || {
                            component_root(|| {
                                anchored_overlay("ao-tab-a")
                                    .open(Signal::new(true))
                                    .placement(Placement::Unanchored)
                                    .motion(FloatingMotionKind::Fade)
                                    .child(|| {
                                        Element::new()
                                            .text("keep-me")
                                            .w(200.0)
                                            .h(80.0)
                                            .bg([1.0, 1.0, 1.0, 1.0])
                                    })
                            })
                        }),
                        tabs_content("b", || component_root(|| Element::new().text("other"))),
                    ]);
                    frag
                });
        });
        tab
    });
    drain_deferred_view_remounts();

    fn find_tid<'a>(el: &'a Element, tid: &str) -> Option<&'a Element> {
        if el.template_id.as_deref() == Some(tid) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_tid(c, tid))
    }
    fn has_anim(el: &Element, name: &str) -> bool {
        el.style.animations.iter().any(|a| a.name == name)
            || el.children.iter().any(|c| has_anim(c, name))
    }
    fn present_pass(root: &mut Element) {
        let _ = hoist_portal_subtrees(root);
        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(root, 800.0, 600.0, [0.0, 0.0]);
        if sync_floating_placement(root, 800.0, 600.0) {
            let _ =
                tree.build_and_layout_fast_ex(root, 800.0, 600.0, [0.0, 0.0], false, false, None);
        }
        let floating_needs = flush_floating_motion_subtree(root);
        let css_ticking = tick_hoisted_floating_css(root);
        if floating_needs || css_ticking {
            mark_floating_panels_layout_dirty(root, &mut tree);
            let _ =
                tree.build_and_layout_fast_ex(root, 800.0, 600.0, [0.0, 0.0], false, false, None);
        }
    }

    {
        let mut root = live.borrow_mut();
        present_pass(&mut root);
        present_pass(&mut root);
    }
    {
        let tree = live.borrow();
        assert!(
            find_tid(&tree, "ao-tab-a__floating_panel").is_some(),
            "open overlay must hoist the panel before the tab switch"
        );
    }

    tab.set("b".into());
    drain_deferred_view_remounts();
    crate::style::utility::set_frame_dt(0.016);
    {
        let _ = tick_floating_presence();
        drain_pending_floating_unmounts();
        drain_deferred_view_remounts();
        let mut root = live.borrow_mut();
        present_pass(&mut root);
    }
    {
        let tree = live.borrow();
        let panel = find_tid(&tree, "ao-tab-a__floating_panel")
            .expect("tab unmount must keep the hoisted panel for the configured exit");
        assert!(
            has_anim(panel, "fade-out-0"),
            "tab unmount must seed fade-out-0 instead of dropping the layer"
        );
    }

    for _ in 0..20 {
        let _ = tick_floating_presence();
        drain_pending_floating_unmounts();
        drain_deferred_view_remounts();
        let mut root = live.borrow_mut();
        present_pass(&mut root);
    }
    crate::style::utility::set_frame_dt(0.0);
    {
        let tree = live.borrow();
        assert!(
            find_tid(&tree, "ao-tab-a__floating_panel").is_none(),
            "panel must drop after the exit animation completes"
        );
    }

    clear_test_reconcile_root();
    drop(live);
    owner.dispose();
    clear_overlay_stack();
    reset_registered_anchors();
}
