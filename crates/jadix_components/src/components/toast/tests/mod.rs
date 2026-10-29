use super::host::{paint_toast_host, toaster, TOAST_HOST_VIEW};
use super::motion::{
    exit_target, stack_lift_visual, toast_enter_offset_y, toast_exit_offset_y, toast_motion_pose,
    tone_shadow_stacked, ToastStackMotion,
};
use super::view::tone_chrome;
use super::*;
use crate::layout::layout_tree::ATTR_PORTAL_KEY;
use crate::style::{parse_color, parse_text_color, SemanticTone};

#[test]
fn registered_swipe_moves_and_settles_through_host_routing() {
    crate::core::reactivity::create_root(|| {
        crate::utils::drag::routing::finish_all();
        let id = 997_143;
        begin_toast_drag(ToastDrag {
            id,
            start_x: 100.0,
            start_y: 100.0,
            dirs: ToastSwipeDirs::for_edge(true, false, false, true),
        });
        assert!(crate::utils::drag::routing::prepare_move(None, 130.0, 80.0).dispatch());
        assert_eq!(toast_drag_xy(id).peek(), [30.0, -20.0]);
        crate::utils::drag::routing::finish_all();
        assert_eq!(toast_drag_xy(id).peek(), [0.0, 0.0]);
        assert!(!is_toast_dragging());
        assert!(!crate::utils::drag::pointer::is_active());
    });
}

fn toast_tone_palette(tone: ToastTone) -> ([f32; 4], [u8; 4], [f32; 4], Option<&'static str>) {
    let chrome = tone_chrome(tone);
    let border = chrome
        .border
        .map(|b| b.color)
        .unwrap_or([0.0, 0.0, 0.0, 0.0]);
    let icon = SemanticTone::default_icon(tone.to_semantic());
    (chrome.bg, chrome.fg, border, icon)
}

fn settle_toast_exits() {
    let mut q = toast_queue().get();
    let mut p = toast_pending().get();
    let past = Instant::now() - Duration::from_secs(1);
    for t in &mut q {
        if t.exiting {
            t.exit_started = Some(past);
        }
    }
    let _ = finalize_exits(&mut q, &mut p);
    toast_queue().set(q);
    toast_pending().set(p);
    promote_pending();
}

#[test]
fn stack_limit_three_keeps_three_active() {
    toast_set_overflow("displace");
    toast_set_limit(3);
    toast_queue().set(vec![]);
    toast_pending().set(vec![]);
    let _a = toast("A").duration(10_000).show();
    let _b = toast("B").duration(10_000).show();
    let _c = toast("C").duration(10_000).show();
    assert_eq!(active_count(&toast_queue().get()), 3);
    let _d = toast("D").duration(10_000).show();
    let q = toast_queue().get();
    assert_eq!(active_count(&q), 3, "displace must free a slot for D");
    assert_eq!(
        q.iter().filter(|t| t.exiting).count(),
        1,
        "oldest should be exiting"
    );
    assert!(q.iter().any(|t| t.id == _d && !t.exiting));
    settle_toast_exits();
    toast_set_limit(3);
}

#[test]
fn toast_portal_sits_above_stacked_modals() {
    assert!(TOAST_PORTAL_Z > 1000 + 10 * 20);
}

#[test]
fn exit_moves_back_toward_enter_edge() {
    // 顶栏：入场从上方（负 Y）滑入 → 退场继续向负 Y（回入场边）。
    assert!(toast_enter_offset_y(false) < 0.0);
    assert!(toast_exit_offset_y(false) < 0.0);
    // 底栏：入场从下方（正 Y）滑入 → 退场继续向正 Y。
    assert!(toast_enter_offset_y(true) > 0.0);
    assert!(toast_exit_offset_y(true) > 0.0);
    // 无拖拽时 front 终点。
    assert_eq!(
        exit_target([0.0, 0.0], false)[1],
        toast_exit_offset_y(false)
    );
    assert_eq!(exit_target([0.0, 0.0], true)[1], toast_exit_offset_y(true));
}

#[test]
fn behind_exit_stays_in_place() {
    let (x, y, op, _) = toast_motion_pose(
        1,
        true,
        false,
        true,
        Some(&ToastStackMotion {
            target_lift: 14.0,
            target_scale: 0.95,
            behind_fade: true,
            dim_shadow: true,
            live_rank: None,
        }),
        [0.0, 0.0],
    );
    let lift = stack_lift_visual(false, 14.0, 0.95);
    assert_eq!(x, 0.0);
    assert!(
        (y - lift).abs() < 0.01,
        "behind exit must not nudge, y={y} lift={lift}"
    );
    assert_eq!(op, 0.0);
}

#[test]
fn list_displace_exit_moves_one_slot_along_enter_axis() {
    toast_set_layout("list");
    toast_set_overflow("displace");
    toast_set_limit(2);
    toast_queue().set(vec![]);
    toast_pending().set(vec![]);
    let _a = toast("A").duration(10_000).show();
    let _b = toast("B").duration(10_000).show();
    let _c = toast("C").duration(10_000).show();
    let q = toast_queue().get();
    let a = q.iter().find(|t| t.title == "A").expect("A");
    assert!(a.exiting);
    let vis = toast_exit_vis(a.id).expect("vis");
    assert!(vis.to_pending);
    assert_eq!(vis.rank, 2);
    // 底栏 list：rank↑ → lift 更负（向上 +1 档）。
    let step = list_step();
    let lift = -(vis.rank as f32 * step);
    let (x, y, op, sc) = toast_motion_pose(
        a.id,
        true,
        true,
        true,
        Some(&ToastStackMotion {
            target_lift: lift,
            target_scale: 1.0,
            behind_fade: true,
            dim_shadow: false,
            live_rank: None,
        }),
        [0.0, 0.0],
    );
    assert_eq!(x, 0.0);
    assert!((y - lift).abs() < 0.01, "y={y} lift={lift}");
    assert_eq!(op, 0.0);
    assert!((sc - 1.0).abs() < 0.001);
    toast_set_layout("stack");
}

#[test]
fn displace_exit_targets_limit_plus_one_slot() {
    toast_set_overflow("displace");
    toast_set_limit(3);
    toast_queue().set(vec![]);
    toast_pending().set(vec![]);
    let _a = toast("A").duration(10_000).show();
    let _b = toast("B").duration(10_000).show();
    let _c = toast("C").duration(10_000).show();
    let _d = toast("D").duration(10_000).show();
    let q = toast_queue().get();
    let a = q.iter().find(|t| t.title == "A").expect("A");
    assert!(a.exiting, "oldest should be displace-exiting");
    let vis = toast_exit_vis(a.id).expect("exit vis");
    assert!(vis.to_pending);
    assert_eq!(
        vis.rank, 3,
        "displace rank must be visible_limit (= stack cap + 1 slot, 0-based)"
    );
    let scale = (1.0 - 3.0_f32 * 0.05).max(0.90);
    let lift_raw = 3.0 * STACK_PEEK;
    let (x, y, op, sc) = toast_motion_pose(
        a.id,
        true,
        false,
        true,
        Some(&ToastStackMotion {
            target_lift: lift_raw,
            target_scale: scale,
            behind_fade: true,
            dim_shadow: true,
            live_rank: None,
        }),
        [0.0, 0.0],
    );
    let lift = stack_lift_visual(false, lift_raw, scale);
    assert_eq!(x, 0.0);
    assert!((y - lift).abs() < 0.01);
    assert_eq!(op, 0.0);
    assert!((sc - scale).abs() < 0.001);
}

fn host_has_toast_portal(el: &Element) -> bool {
    fn walk(el: &Element) -> bool {
        if el.attrs.get("jadix_portal").map(String::as_str) == Some("1")
            && el.attrs.get(ATTR_PORTAL_KEY).map(String::as_str) == Some("toast-host")
        {
            return true;
        }
        el.children.iter().any(walk)
    }
    el.attrs.get("jadix_portal").map(String::as_str) != Some("1") && walk(el)
}

fn find_tid(el: &Element, id: &str) -> bool {
    el.template_id.as_deref() == Some(id) || el.children.iter().any(|c| find_tid(c, id))
}

fn collect_node_texts(el: &Element, out: &mut Vec<String>) {
    if let Some(t) = &el.text {
        if !t.is_empty() {
            out.push(t.clone());
        }
    }
    for c in &el.children {
        collect_node_texts(c, out);
    }
}

fn find_toast_cards<'a>(el: &'a Element, out: &mut Vec<&'a Element>) {
    if el
        .template_id
        .as_deref()
        .is_some_and(|id| id.starts_with("toast-card-"))
    {
        out.push(el);
    }
    for c in &el.children {
        find_toast_cards(c, out);
    }
}

fn seed_collapsed_stack() {
    toast_set_layout("stack");
    toast_set_overflow("displace");
    toast_set_limit(3);
    toast_queue().set(vec![]);
    toast_pending().set(vec![]);
    expanded_signal().set(false);
    let _ = toast("Stack-A")
        .description("body-A")
        .duration(10_000)
        .show();
    let _ = toast("Stack-B")
        .description("body-B")
        .duration(10_000)
        .show();
    let _ = toast("Stack-C")
        .description("body-C")
        .duration(10_000)
        .show();
}

fn assert_collapsed_stack_paint(
    label: &str,
    root: &Element,
    result: &crate::layout::layout_tree::RenderResult,
) {
    let mut cards = Vec::new();
    find_toast_cards(root, &mut cards);
    assert_eq!(
        cards.len(),
        3,
        "{label}: expected 3 toast-card nodes, got {}",
        cards.len()
    );

    for card in &cards {
        let mut texts = Vec::new();
        collect_node_texts(card, &mut texts);
        assert!(
                texts.iter().any(|t| t.starts_with("Stack-")),
                "{label}: {} lost inner text (got {texts:?}), bg={:?} radii={:?} overflow={:?} z={} scale={:?} bounds={:?}",
                card.template_id.as_deref().unwrap_or("?"),
                card.style.bg_color,
                card.style.radii,
                card.style.overflow,
                card.style.z_index,
                card.style.scale,
                card.layout_bounds
            );
        assert!(
            card.style.bg_color[3] > 0.5,
            "{label}: {} lost opaque bg {:?}",
            card.template_id.as_deref().unwrap_or("?"),
            card.style.bg_color
        );
        assert!(
            card.style.radii.iter().any(|r| *r > 0.5),
            "{label}: {} lost radii {:?}",
            card.template_id.as_deref().unwrap_or("?"),
            card.style.radii
        );
        let bw = card.style.resolved_border_widths();
        assert!(
            bw.iter().any(|w| *w > 0.0),
            "{label}: {} lost border stroke {bw:?} style={:?}",
            card.template_id.as_deref().unwrap_or("?"),
            card.style.border_style
        );

        let [cx, cy, cw, ch] = card.layout_bounds;
        let painted = result.instances[result.overlay_split.instance_start..]
            .iter()
            .find(|inst| {
                let ix = inst.rect_pos[0];
                let iy = inst.rect_pos[1];
                let iw = inst.rect_size[0];
                let ih = inst.rect_size[1];
                ix < cx + cw && ix + iw > cx && iy < cy + ch && iy + ih > cy && inst.color[3] > 0.5
            });
        let Some(painted) = painted else {
            panic!(
                "{label}: {} has no opaque overlay instance at bounds {:?}; overlay inst={:?}",
                card.template_id.as_deref().unwrap_or("?"),
                card.layout_bounds,
                result.instances[result.overlay_split.instance_start..]
                    .iter()
                    .map(|i| (i.rect_pos, i.rect_size, i.color, i.radii, i.clip_size))
                    .collect::<Vec<_>>()
            );
        };
        assert!(
            painted.radii[0] > 0.5 && painted.border_width > 0.0,
            "{label}: {} overlay instance lost chrome radii={:?} border_w={} size={:?} clip={:?}",
            card.template_id.as_deref().unwrap_or("?"),
            painted.radii,
            painted.border_width,
            painted.rect_size,
            painted.clip_size
        );
    }

    let overlay_texts: Vec<&str> = result.text_nodes[result.overlay_split.text_start..]
        .iter()
        .map(|t| t.text.as_str())
        .collect();
    for title in ["Stack-A", "Stack-B", "Stack-C"] {
        let node = result.text_nodes[result.overlay_split.text_start..]
            .iter()
            .find(|t| t.text == title);
        let Some(node) = node else {
            panic!(
                    "{label}: {title} missing from overlay glyphon text (collapsed stack must not drop labels).\n\
                     overlay={overlay_texts:?} bands={} page_stack={}",
                    result.overlay_split.bands.len(),
                    result.overlay_split.page_stack_bands.len()
                );
        };
        if let Some(clip) = node.clip_bounds {
            assert!(
                clip[2] > 1.0 && clip[3] > 1.0,
                "{label}: {title} clip collapsed to {clip:?}"
            );
        }
    }
}

#[test]
fn toast_host_root_is_a_box_none_shell_not_the_portal() {
    toast_queue().set(vec![]);
    toast_pending().set(vec![]);
    let (_owner, el) = crate::core::reactivity::create_root(paint_toast_host);
    assert_ne!(
        el.attrs.get("jadix_portal").map(String::as_str),
        Some("1"),
        "reactive toast root must stay in-tree so Command close can still find it"
    );
}

#[test]
fn toaster_anchor_survives_ui_root_template_stamp() {
    crate::app::reactive::reset_registered_anchors();
    toast_queue().set(vec![]);
    toast_pending().set(vec![]);
    let (_owner, stamped) = crate::core::reactivity::create_root(|| {
        let host: Element = toaster().into();
        host.template_id("examples/gallery/src/main.rs:3729:9")
    });
    assert_eq!(
        stamped.template_id.as_deref(),
        Some("examples/gallery/src/main.rs:3729:9"),
        "ui! stamps the wrapper"
    );
    assert!(
        find_tid(&stamped, TOAST_HOST_VIEW),
        "toast reactive placeholder must remain a descendant after ui! root stamp"
    );
    crate::app::reactive::reset_registered_anchors();
}

#[test]
fn toaster_singleton_registers_anchor_after_tracked_mount() {
    crate::app::reactive::reset_registered_anchors();
    toast_queue().set(vec![]);
    toast_pending().set(vec![]);
    let (_owner, (before, after_first, after_second)) =
        crate::core::reactivity::create_root(|| {
            let pair = std::rc::Rc::new(std::cell::Cell::new((false, false, false)));
            let pair2 = pair.clone();
            crate::core::reactivity::create_effect(move || {
                let before = crate::app::reactive::reactive_anchor_is_live(TOAST_HOST_VIEW);
                let first: Element = toaster().into();
                let after_first = crate::app::reactive::reactive_anchor_is_live(TOAST_HOST_VIEW);
                let _second: Element = toaster().into();
                let after_second = crate::app::reactive::reactive_anchor_is_live(TOAST_HOST_VIEW);
                let _keep = first;
                pair2.set((before, after_first, after_second));
            });
            pair.get()
        });
    assert!(!before, "host must not be live before the first toaster()");
    assert!(
        after_first,
        "first toaster() must keep the host anchor alive"
    );
    assert!(
        after_second,
        "a second toaster() in the same tree still shares the live host"
    );
    crate::app::reactive::reset_registered_anchors();
}

#[test]
fn toast_stage_hoists_to_window_portal_layer() {
    use crate::layout::layout_tree::{
        hoist_portal_subtrees, LayoutTree, ATTR_HOISTED_PORTAL, PORTAL_LAYER_ID,
    };
    toast_queue().set(vec![]);
    toast_pending().set(vec![]);
    let (_owner, mut root) = crate::core::reactivity::create_root(|| {
        let _ = toast("Toast from Gallery")
            .description("Hover the stack to expand.")
            .duration(5000)
            .show();
        Element::new().w(800.0).h(600.0).child(paint_toast_host())
    });
    let _ = hoist_portal_subtrees(&mut root);
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
    let layer = root
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
        .expect("window portal layer");
    let stage = layer
        .children
        .iter()
        .find(|c| {
            c.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) == Some("1")
                && c.attrs.get(ATTR_PORTAL_KEY).map(String::as_str) == Some("toast-host")
        })
        .expect("toast stage on window root");
    assert!(
        stage.layout_bounds[2] > 700.0 && stage.layout_bounds[3] > 500.0,
        "toast stage must fill the window portal layer, got {:?}",
        stage.layout_bounds
    );
}

#[test]
fn collapsed_stack_keeps_chrome_and_text_on_plain_host() {
    use crate::layout::layout_tree::LayoutTree;
    let (_owner, mut root) = crate::core::reactivity::create_root(|| {
        seed_collapsed_stack();
        Element::new().w(800.0).h(600.0).child(paint_toast_host())
    });
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
    assert_collapsed_stack_paint("plain-host", &root, &result);
}

#[test]
fn collapsed_stack_keeps_chrome_and_text_inside_ui_relative_box() {
    // Gallery 根：`ui! { <Box relative> 壳 + Toaster }`。wrap_ui_expansion 会给
    // Box 盖 template_id/hash；壳上的 z-index 会打开 page_stack_bands。
    // 本 crate 内不能写 `ui!`（宏展开走 `jadix::`），用等价 builder + stamp。
    use crate::layout::layout_tree::LayoutTree;
    let (_owner, mut root) = crate::core::reactivity::create_root(|| {
        seed_collapsed_stack();
        let host: Element = r#box()
            .w(800.0)
            .h(600.0)
            .relative(true)
            .child(r#box().w("100%").h("100%").z_index(1).child(text("shell")))
            .child(paint_toast_host())
            .into();
        host
    });
    root = root
        .template_hash("gallery-root-hash")
        .template_id("examples/gallery/src/app.rs:1:1");
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
    assert_collapsed_stack_paint("ui-relative-box", &root, &result);
}

#[test]
fn toast_builder_and_push_share_the_same_host_portal() {
    toast_queue().set(vec![]);
    toast_pending().set(vec![]);
    let (_owner, first) = crate::core::reactivity::create_root(|| {
        let _ = toast("Toast from Gallery")
            .description("Hover the stack to expand.")
            .duration(5000)
            .show();
        paint_toast_host()
    });
    assert!(host_has_toast_portal(&first));
    let (_owner, from_command) = crate::core::reactivity::create_root(|| {
        let _ = toast("Hello from Command")
            .description("Hover the stack to expand.")
            .duration(5000)
            .show();
        paint_toast_host()
    });
    assert!(
        host_has_toast_portal(&from_command),
        "command-triggered toasts must reuse the Toaster portal, not a second root"
    );
    let q = toast_queue().get();
    assert!(q.iter().any(|t| t.title == "Toast from Gallery"));
    assert!(q.iter().any(|t| t.title == "Hello from Command"));
}

#[test]
fn toast_host_lifts_cards_as_child_portal() {
    toast_queue().set(vec![]);
    toast_pending().set(vec![]);
    let (_owner, el) = crate::core::reactivity::create_root(|| {
        let _ = toast_push("Hello from Command", 2500);
        paint_toast_host()
    });
    assert!(
        host_has_toast_portal(&el),
        "toast cards must hoist from a child portal, not replace the host root"
    );
}

#[test]
fn toast_still_paints_after_command_modal_closes() {
    use crate::app::portal_overlay::{clear_overlay_stack, portal_sync, OverlayKind, OverlayMode};
    clear_overlay_stack();
    toast_queue().set(vec![]);
    toast_pending().set(vec![]);
    let (_owner, _) = crate::core::reactivity::create_root(|| {
        let cmd = Signal::new(true);
        assert!(portal_sync("gal-cmd", cmd, OverlayKind::Modal, OverlayMode::Stacked).is_some());
        let first = toast_push("Hello from Command", 2500);
        cmd.set(false);
        let _ = portal_sync("gal-cmd", cmd, OverlayKind::Modal, OverlayMode::Stacked);
        let painted = paint_toast_host();
        assert!(
            host_has_toast_portal(&painted),
            "first command toast must still paint after the modal leaves the overlay stack"
        );
        let second = toast_push("Copy", 1500);
        let painted = paint_toast_host();
        assert!(
            host_has_toast_portal(&painted),
            "follow-up toasts must keep painting"
        );
        assert!(toast_queue().get().iter().any(|t| t.id == first));
        assert!(toast_queue().get().iter().any(|t| t.id == second));
        clear_overlay_stack();
    });
}

#[test]
fn toast_push_and_dismiss() {
    let id = toast_push("hi", 5000);
    assert!(toast_queue().get().iter().any(|t| t.id == id));
    toast_dismiss(id);
    assert!(
        toast_queue().get().iter().any(|t| t.id == id && t.exiting),
        "dismiss starts exit animation"
    );
    settle_toast_exits();
    assert!(!toast_queue().get().iter().any(|t| t.id == id));
}

#[test]
fn toast_builder_tone_and_description() {
    let id = toast("Saved")
        .description("ok")
        .variant("success")
        .duration(2000)
        .show();
    let item = toast_queue()
        .get()
        .into_iter()
        .find(|t| t.id == id)
        .expect("queued");
    assert_eq!(item.title, "Saved");
    assert_eq!(item.description, "ok");
    assert_eq!(item.tone, ToastTone::Success);
    toast_dismiss(id);
    settle_toast_exits();
}

#[test]
fn layout_parse_defaults_to_stack() {
    assert_eq!(ToastLayout::parse("stack"), ToastLayout::Stack);
    assert_eq!(ToastLayout::parse("list"), ToastLayout::List);
    assert_eq!(ToastLayout::parse("sonner"), ToastLayout::Stack);
    assert_eq!(ToastLayout::parse(""), ToastLayout::Stack);
}

#[test]
fn toast_tone_unknown_falls_back_and_success_uses_soft_accent() {
    assert_eq!(ToastTone::parse("nope"), ToastTone::Default);
    assert_eq!(ToastTone::parse("info"), ToastTone::Info);
    assert_eq!(ToastTone::parse("error"), ToastTone::Destructive);
    assert_eq!(ToastTone::parse("warn"), ToastTone::Warning);
    let (bg, fg, border, icon) = toast_tone_palette(ToastTone::Success);
    assert_eq!(bg, tokens::popover());
    assert_eq!(border, parse_color("success/50"));
    assert_eq!(fg, parse_text_color("success"));
    assert_eq!(icon, Some("circle-check"));
    let (_, _, _, info_icon) = toast_tone_palette(ToastTone::Info);
    assert_eq!(info_icon, Some("info"));
    let (_, _, wborder, wicon) = toast_tone_palette(ToastTone::Warning);
    assert_eq!(wborder, parse_color("warning/50"));
    assert_eq!(wicon, Some("circle-alert"));
    let (_, _, _, xicon) = toast_tone_palette(ToastTone::Destructive);
    assert_eq!(xicon, Some("circle-x"));
    let (sc, _, _) = tone_shadow_stacked(ToastTone::Success, false);
    assert!(
        sc[1] > sc[0] && sc[1] > sc[2] && (sc[3] - 0.12).abs() < 0.001,
        "success shadow must be green-tinted at default strength, got {sc:?}"
    );
    let (ic, _, _) = tone_shadow_stacked(ToastTone::Info, false);
    assert!(
        ic[2] > ic[0] && ic[2] > ic[1] && (ic[3] - 0.12).abs() < 0.001,
        "info shadow must be blue-tinted at default strength, got {ic:?}"
    );
}

#[test]
fn toast_set_layout_switches_signal() {
    toast_set_layout("list");
    assert_eq!(
        ToastLayout::parse(&layout_signal().peek()),
        ToastLayout::List
    );
    toast_set_layout("stack");
    assert_eq!(
        ToastLayout::parse(&layout_signal().peek()),
        ToastLayout::Stack
    );
}

#[test]
#[cfg(feature = "animation-base")]
fn overflow_displace_caps_concurrent_exiting() {
    toast_set_overflow("displace");
    toast_set_limit(2);
    toast_queue().set(vec![]);
    toast_pending().set(vec![]);

    let a = toast("A").duration(10_000).show();
    let b = toast("B").duration(10_000).show();
    let c = toast("C").duration(10_000).show();
    assert_eq!(
        toast_queue().get().iter().filter(|t| t.exiting).count(),
        1,
        "first displace starts one exit"
    );
    assert!(
        toast_queue().get().iter().any(|t| t.id == a && t.exiting),
        "A should be exiting"
    );

    let d = toast("D").duration(10_000).show();
    let q = toast_queue().get();
    let p = toast_pending().get();
    // 挤出淡出与入场同步：允许并发两段 to_pending 退场，勿 snap 掉 A 的动画。
    assert_eq!(
        q.iter().filter(|t| t.exiting).count(),
        2,
        "A and B should both be fading (displace exits are not snapped early)"
    );
    assert!(
        p.iter().all(|t| t.id != a),
        "A must still be on-queue exiting, not snapped to pending"
    );
    assert!(q.iter().any(|t| t.id == a && t.exiting));
    assert!(q.iter().any(|t| t.id == b && t.exiting));
    assert!(q.iter().any(|t| t.id == c && !t.exiting));
    assert!(q.iter().any(|t| t.id == d && !t.exiting));

    settle_toast_exits();
    toast_set_limit(3);
}

#[test]
fn overflow_displace_requeues_oldest_at_end() {
    toast_set_overflow("displace");
    toast_set_limit(2);
    toast_queue().set(vec![]);
    toast_pending().set(vec![]);

    let a = toast("A").duration(10_000).show();
    let b = toast("B").duration(10_000).show();
    let c = toast("C").duration(10_000).show();
    let q = toast_queue().get();
    assert_eq!(active_count(&q), 2);
    assert!(
        q.iter().any(|t| t.id == a && t.exiting),
        "A fades out synchronously with C entering"
    );
    assert!(q.iter().any(|t| t.id == b && !t.exiting));
    assert!(q.iter().any(|t| t.id == c && !t.exiting));
    assert!(
        toast_pending().get().is_empty(),
        "A moves to pending only after fade completes"
    );

    settle_toast_exits();
    assert!(!toast_queue().get().iter().any(|t| t.id == a));
    let p = toast_pending().get();
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].id, a, "A held in pending after fade");

    toast_dismiss(b);
    settle_toast_exits();
    let q = toast_queue().get();
    assert_eq!(active_count(&q), 2);
    assert!(q.iter().any(|t| t.id == c));
    assert_eq!(
        q.iter().filter(|t| !t.exiting).next().map(|t| t.id),
        Some(a),
        "A reappears at stack back (queue oldest) after a slot frees"
    );
    assert_eq!(
        q.iter().filter(|t| !t.exiting).last().map(|t| t.id),
        Some(c),
        "C stays front after pending promote"
    );
    assert!(toast_pending().get().is_empty());
    for id in [c, a] {
        toast_dismiss(id);
    }
    settle_toast_exits();
}

#[test]
fn overflow_wait_queues_then_promotes() {
    toast_set_overflow("wait");
    toast_set_limit(1);
    toast_queue().set(vec![]);
    toast_pending().set(vec![]);

    let a = toast("A").duration(10_000).show();
    let b = toast("B").duration(10_000).show();
    assert_eq!(toast_queue().get().len(), 1);
    assert_eq!(toast_queue().get()[0].id, a);
    assert_eq!(toast_pending().get().len(), 1);
    assert_eq!(toast_pending().get()[0].id, b);

    toast_dismiss(a);
    settle_toast_exits();
    let q = toast_queue().get();
    assert_eq!(q.len(), 1);
    assert_eq!(q[0].id, b, "B promoted after A exit finishes");
    assert!(toast_pending().get().is_empty());
    toast_dismiss(b);
    settle_toast_exits();
    toast_set_overflow("displace");
    toast_set_limit(3);
}

#[test]
fn timer_pauses_without_expiring() {
    toast_queue().set(vec![]);
    toast_pending().set(vec![]);
    set_stack_timers_paused(false);

    let id = toast("hold").duration(500).show();
    assert!(!toast_timer_expired(id));
    set_item_timer_paused(id, true);
    std::thread::sleep(Duration::from_millis(600));
    assert!(
        !toast_timer_expired(id),
        "paused toast must not expire while frozen"
    );
    set_item_timer_paused(id, false);
    std::thread::sleep(Duration::from_millis(600));
    assert!(toast_timer_expired(id), "resumed toast eventually expires");
    toast_dismiss(id);
    settle_toast_exits();
}

#[test]
fn demoted_pending_keeps_running_timer() {
    toast_set_overflow("displace");
    toast_set_limit(1);
    toast_queue().set(vec![]);
    toast_pending().set(vec![]);
    expanded_signal().set(false);
    set_stack_timers_paused(false);

    let a = toast("A").duration(800).show();
    let _b = toast("B").duration(10_000).show();
    assert!(
        toast_queue().get().iter().any(|t| t.id == a && t.exiting),
        "A displace-fading before pending"
    );
    assert!(
        toast_timer_deadline(a).is_some(),
        "displaced toast must keep its timer armed during fade"
    );
    settle_toast_exits();
    assert!(
        toast_pending().get().iter().any(|t| t.id == a),
        "A demoted to pending after fade"
    );
    set_item_timer_paused(a, false);
    std::thread::sleep(Duration::from_millis(900));
    let mut p = toast_pending().peek();
    assert!(
        prune_expired_pending_into(&mut p),
        "pending toast expires without waiting to re-enter"
    );
    toast_set_limit(3);
    let q = toast_queue().get();
    for t in &q {
        toast_dismiss(t.id);
    }
    settle_toast_exits();
}

#[test]
fn collapse_resumes_a_timer_frozen_past_its_deadline() {
    // 指针正好在到期瞬间压住堆叠：暂停会把剩余削到 `timer::TOAST_PAUSE_FLOOR`。收起后必须
    // 重新起表——否则 `running_since` 永远是 None、`toast_timer_expired` 恒 false，
    // 这条通知再也不会自动退场（悬停展开后移开鼠标即卡死的主症状）。
    toast_set_layout("stack");
    toast_queue().set(vec![]);
    toast_pending().set(vec![]);
    expanded_signal().set(false);
    set_stack_timers_paused(false);

    let id = toast("stall").duration(500).show();
    std::thread::sleep(Duration::from_millis(560));
    assert!(
        toast_timer_expired(id),
        "500ms timer is due before the hover"
    );

    expanded_signal().set(true);
    let _ = tick_toast_lifecycle();
    assert!(
        !toast_timer_expired(id),
        "expanding must freeze even an already-due timer"
    );
    assert!(
        toast_timer_deadline(id).is_none(),
        "frozen timer has no deadline (nothing to wake the event loop for)"
    );

    expanded_signal().set(false);
    let _ = tick_toast_lifecycle();
    assert!(
        toast_timer_deadline(id).is_some(),
        "collapse must re-arm the frozen timer instead of leaving it dead"
    );
    std::thread::sleep(timer::TOAST_PAUSE_FLOOR + Duration::from_millis(60));
    assert!(
        toast_timer_expired(id),
        "resumed timer must expire so the toast leaves on its own"
    );
    toast_dismiss(id);
    settle_toast_exits();
}

#[test]
fn expanded_stack_freezes_displaced_pending_timers() {
    // displace 挤出的条目在退场动画期间被 `exiting` 跳过（计时继续跑），落进 pending
    // 后 `exiting` 又被清掉。若冻结按全局标志早退，它会在悬停期间悄悄到期，收起那一
    // 刻整批一起蒸发（「重新打开新通知，原通知全部消失」的来源）。
    toast_set_layout("stack");
    toast_set_overflow("displace");
    toast_set_limit(1);
    toast_queue().set(vec![]);
    toast_pending().set(vec![]);
    expanded_signal().set(false);
    set_stack_timers_paused(false);

    let a = toast("A").duration(10_000).show();
    let _b = toast("B").duration(10_000).show();
    settle_toast_exits();
    assert!(
        toast_pending().get().iter().any(|t| t.id == a),
        "A demoted to pending after the displace fade"
    );
    assert!(
        toast_timer_deadline(a).is_some(),
        "pending timer runs while the stack is collapsed"
    );

    expanded_signal().set(true);
    let _ = tick_toast_lifecycle();
    assert!(
        toast_timer_deadline(a).is_none(),
        "expanding must freeze pending timers too, not only visible ones"
    );

    expanded_signal().set(false);
    let _ = tick_toast_lifecycle();
    assert!(
        toast_timer_deadline(a).is_some(),
        "collapsing resumes the pending timer"
    );

    toast_set_limit(3);
    let q = toast_queue().get();
    for t in &q {
        toast_dismiss(t.id);
    }
    let p = toast_pending().get();
    for t in &p {
        toast_dismiss(t.id);
    }
    settle_toast_exits();
}

#[test]
fn same_frame_expiries_stagger_instead_of_snapping() {
    // 退场预算只有 1：一批同时到期时老路会 `snap_finalize_exit` 掉正在播的那张，
    // 前 N-1 张无动画瞬间抹掉——这就是「重新打开新通知，原通知全部消失」看到的样子。
    // 现在应只起一张退场，其余顺延到它播完。
    toast_set_overflow("displace");
    toast_set_limit(3);
    toast_queue().set(vec![]);
    toast_pending().set(vec![]);
    expanded_signal().set(false);
    set_stack_timers_paused(false);

    let a = toast("A").duration(500).show();
    let b = toast("B").duration(500).show();
    let c = toast("C").duration(500).show();
    std::thread::sleep(Duration::from_millis(560));

    let mut q = toast_queue().get();
    let mut p = toast_pending().get();
    assert_eq!(q.len(), 3, "all three visible before the batch expiry");
    assert!(prune_expired(&mut q, &mut p), "the oldest starts leaving");
    assert_eq!(
        q.len(),
        3,
        "no card may be snapped off-tree while another animates out"
    );
    assert_eq!(
        q.iter().filter(|t| t.exiting).count(),
        1,
        "exactly one exit animation at a time (MAX_CONCURRENT_EXITING)"
    );
    for id in [a, b, c] {
        if q.iter().any(|t| t.id == id && t.exiting) {
            continue;
        }
        assert!(
            !toast_timer_expired(id),
            "deferred card must stop being due, else it snaps the animating one"
        );
        assert!(
            toast_timer_deadline(id).is_some(),
            "deferred card keeps a running deadline so the loop wakes for it"
        );
    }

    toast_queue().set(q);
    toast_pending().set(p);
    for id in [a, b, c] {
        toast_dismiss(id);
    }
    settle_toast_exits();
}

#[test]
fn overflow_parse() {
    assert_eq!(ToastOverflow::parse("displace"), ToastOverflow::Displace);
    assert_eq!(ToastOverflow::parse("wait"), ToastOverflow::Wait);
    assert_eq!(ToastOverflow::parse("queue"), ToastOverflow::Wait);
    assert_eq!(ToastOverflow::parse(""), ToastOverflow::Displace);
}

#[test]
fn position_parse_six_corners() {
    assert_eq!(ToastPosition::parse("top-left"), ToastPosition::TopLeft);
    assert_eq!(ToastPosition::parse("top_center"), ToastPosition::TopCenter);
    assert_eq!(ToastPosition::parse("top-right"), ToastPosition::TopRight);
    assert_eq!(
        ToastPosition::parse("bottom-left"),
        ToastPosition::BottomLeft
    );
    assert_eq!(
        ToastPosition::parse("bottom-center"),
        ToastPosition::BottomCenter
    );
    assert_eq!(
        ToastPosition::parse("bottom_right"),
        ToastPosition::BottomRight
    );
    assert_eq!(ToastPosition::parse("nope"), ToastPosition::TopCenter);
    assert_eq!(ToastPosition::default(), ToastPosition::TopCenter);
    assert!(ToastPosition::BottomCenter.is_bottom());
    assert!(!ToastPosition::TopLeft.is_bottom());
}

#[test]
fn corner_swipe_allows_two_edges() {
    let tr = swipe_dirs_for_position(ToastPosition::TopRight);
    assert!(tr.up && tr.right && !tr.left && !tr.down);
    let bl = swipe_dirs_for_position(ToastPosition::BottomLeft);
    assert!(bl.down && bl.left && !bl.up && !bl.right);
    let tc = swipe_dirs_for_position(ToastPosition::TopCenter);
    assert!(tc.up && !tc.left && !tc.right && !tc.down);
    assert_eq!(clamp_swipe_offset(40.0, -30.0, tr), [40.0, -30.0]);
    assert_eq!(clamp_swipe_offset(-40.0, 30.0, tr), [0.0, 0.0]);
}
