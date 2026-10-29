use super::*;
use crate::app::portal_overlay::clear_overlay_stack;
use crate::core::reactivity::create_root;
use crate::layout::layout_tree::{
    hoist_portal_subtrees, LayoutTree, ATTR_HOISTED_PORTAL, ATTR_PORTAL_KEY, PORTAL_LAYER_ID,
};

#[test]
fn command_action_holds_label() {
    let a = CommandAction::new("ping", "Ping host", || {}).shortcut("⌘P");
    assert_eq!(a.id, "ping");
    assert_eq!(a.label, "Ping host");
    assert_eq!(a.shortcut.as_deref(), Some("⌘P"));
}

#[test]
fn builder_item_style() {
    let (_o, el): (_, Element) = create_root(|| {
        command_paint(
            "cmd-chrome".into(),
            Signal::new(true),
            "search".into(),
            CommandSurface::Inline,
            Rc::new(|| {
                command_list()
                    .child(
                        command_item("cal")
                            .style(|b| b.text_size(20.0))
                            .text("Calendar"),
                    )
                    .into()
            }),
        )
    });
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(&el, 20.0),
        "command_item .style(|b| b.text_size(20)) must land on the item tree"
    );
}

#[test]
fn command_item_caption_inherits_text_sm_leading() {
    let (_o, mut el): (_, Element) = create_root(|| {
        command_paint(
            "cmd-leading".into(),
            Signal::new(true),
            "search".into(),
            CommandSurface::Inline,
            Rc::new(|| {
                let mut cap = Element::new().text("Calendar");
                cap.text_style.set_line_height_specified(true);
                cap.text_style.line_height_scale = Some(1.0);
                command_list().child(command_item("cal").child(cap)).into()
            }),
        )
    });
    fn find_item(el: &Element) -> Option<&Element> {
        if el.attrs.get("data-slot").map(String::as_str) == Some("command-item") {
            return Some(el);
        }
        el.children.iter().find_map(find_item)
    }
    fn find_caption(el: &Element) -> Option<&Element> {
        if el.text.as_deref() == Some("Calendar") {
            return Some(el);
        }
        el.children.iter().find_map(find_caption)
    }
    let item = find_item(&el).expect("command-item");
    assert_eq!(
        item.text_style.line_height_scale,
        Some(TAILWIND_TEXT_SM_LEADING_SCALE),
        "item root carries text-sm leading"
    );
    let caption = find_caption(&el).expect("caption");
    assert!(
        !caption.text_style.line_height_specified(),
        "paint must release Label leading-none so the item token can inherit"
    );
    assert_ne!(caption.text_style.line_height_scale, Some(1.0));
    crate::layout::layout_tree::cascade_inheritable_text_style_for_test(&mut el);
    let caption = find_caption(&el).expect("caption after cascade");
    assert_eq!(
        caption.text_style.line_height_scale,
        Some(TAILWIND_TEXT_SM_LEADING_SCALE),
        "caption copies item-root text-sm leading"
    );
}

fn find_portal_layer(root: &Element) -> &Element {
    root.children
        .iter()
        .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
        .expect("portal layer")
}

fn find_hoisted_panel<'a>(root: &'a Element, key: &str) -> &'a Element {
    find_portal_layer(root)
        .children
        .iter()
        .find(|c| {
            c.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) == Some("1")
                && c.attrs.get(ATTR_PORTAL_KEY).map(String::as_str) == Some(key)
        })
        .expect("hoisted command stage")
}

fn overlay_palette_panel(stage: &Element) -> &Element {
    stage.children.first().expect("palette panel")
}

fn command_overlay_frame_open(overlay_id: &str, inner: Element) -> Element {
    let open = Signal::new(true);
    let overlay_opts = ShellOverlayOpts::from_content_parts(None, Some(0.0), true, Some(0.0));
    viewport_modal_layer(
        overlay_id,
        open,
        &overlay_opts,
        ViewportModalLayerOpts::dialog_fade(),
        |_| Some(command_overlay_panel(inner, overlay_id)),
    )
}

#[test]
fn command_overlay_list_inset_from_panel_edges() {
    clear_overlay_stack();
    let (_owner, mut root) = create_root(|| {
        let inner = command_paint(
            "cmd-pad-reg".into(),
            Signal::new(true),
            "Type a command or search...".into(),
            CommandSurface::Overlay,
            gallery_overlay_body(),
        );
        let frame = command_overlay_frame_open("cmd-pad-reg-t", inner);
        Element::new().w(800.0).h(600.0).child(frame)
    });
    let _ = hoist_portal_subtrees(&mut root);
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
    let stage = find_hoisted_panel(&root, "panel:1001");
    let panel = overlay_palette_panel(stage);
    let [px, _py, pw, _ph] = panel.layout_bounds;
    let item = find_cmd_item(panel);
    let item_inset_l = item.layout_bounds[0] - px;
    let item_inset_r = (px + pw) - (item.layout_bounds[0] + item.layout_bounds[2]);
    clear_overlay_stack();
    assert!(
            item_inset_l >= 3.0,
            "command-item should be inset from panel left by CommandGroup p-1 (>=4px), got {item_inset_l}"
        );
    assert!(
            item_inset_r >= 3.0,
            "command-item should be inset from panel right by CommandGroup p-1 (>=4px), got {item_inset_r}"
        );
}

#[test]
fn command_overlay_frame_hoists_viewport_stage() {
    clear_overlay_stack();
    let (_owner, mut root) = create_root(|| {
        let inner = Element::new().w(200.0).h(48.0).bg([0.2, 0.2, 0.2, 1.0]);
        let frame = command_overlay_frame_open("cmd-overlay-frame-t", inner);
        Element::new().w(800.0).h(600.0).child(frame)
    });

    let lifted = hoist_portal_subtrees(&mut root);
    assert!(lifted >= 2, "underlay + stage must hoist; lifted={lifted}");

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);

    let stage = find_hoisted_panel(&root, "panel:1001");
    assert!(
        stage.layout_bounds[2] > 700.0 && stage.layout_bounds[3] > 500.0,
        "command stage must fill the viewport after hoist, got {:?}",
        stage.layout_bounds
    );
    let panel = overlay_palette_panel(stage);
    let stage_h = stage.layout_bounds[3];
    assert!(
        panel.layout_bounds[3] > 40.0,
        "palette panel must have visible height, got {:?}",
        panel.layout_bounds
    );
    assert!(
        (panel.layout_bounds[2] - 512.0).abs() < 8.0 || panel.layout_bounds[2] > 400.0,
        "palette width is max-w-lg (32rem), got {}",
        panel.layout_bounds[2]
    );
    let panel_y = panel.layout_bounds[1];
    let panel_h = panel.layout_bounds[3];
    let centered_y = (stage_h - panel_h) * 0.5;
    assert!(
        (panel_y - centered_y).abs() < 12.0,
        "palette must be vertically centered in viewport, got y={panel_y} want≈{centered_y}"
    );
    clear_overlay_stack();
}

#[test]
fn command_overlay_empty_sides_hit_underlay() {
    use crate::app::pointer_events::hit_path;

    clear_overlay_stack();
    let (_owner, mut root) = create_root(|| {
        let inner = Element::new().w(200.0).h(48.0).bg([0.2, 0.2, 0.2, 1.0]);
        let frame = command_overlay_frame_open("cmd-overlay-sides-t", inner);
        Element::new().w(800.0).h(600.0).child(frame)
    });

    let _ = hoist_portal_subtrees(&mut root);
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);

    let stage = find_hoisted_panel(&root, "panel:1001");
    let panel = overlay_palette_panel(stage);
    let [px, py, pw, ph] = panel.layout_bounds;
    assert!(
        px > 8.0,
        "palette must not span the viewport; left={px} w={pw}"
    );
    let left_x = (px - 24.0).max(4.0);
    let right_x = (px + pw + 24.0).min(796.0);
    let mid_y = py + ph * 0.5;
    for (name, x) in [("left", left_x), ("right", right_x)] {
        let path = hit_path(&root, x, mid_y);
        assert!(
            path.iter().any(|e| {
                e.portal_key
                    .as_deref()
                    .is_some_and(|k| k.starts_with("underlay:"))
            }),
            "{name} of command palette must hit underlay so outside click closes; path keys={:?}",
            path.iter()
                .filter_map(|e| e.portal_key.as_deref())
                .collect::<Vec<_>>()
        );
    }
    clear_overlay_stack();
}

#[test]
fn command_overlay_close_then_toast_does_not_leave_underlay() {
    use crate::components::toast::host::paint_toast_host;
    use crate::components::toast::toast_push;
    clear_overlay_stack();
    let (_owner, mut root) = create_root(|| {
        let frame =
            command_overlay_frame_open("cmd-overlay-toast-t", Element::new().w(200.0).h(48.0));
        Element::new()
            .w(800.0)
            .h(600.0)
            .child(frame)
            .child(paint_toast_host())
    });
    let _ = hoist_portal_subtrees(&mut root);
    assert!(
        find_portal_layer(&root).children.iter().any(|c| c
            .attrs
            .get(ATTR_PORTAL_KEY)
            .map(String::as_str)
            == Some("underlay:1000")),
        "precondition: command underlay is hoisted"
    );

    batch(|| {
        let _ = toast_push("Hello from Command", 2500);
    });
    // Overlay effect returns empty; toast host keeps a non-portal shell + child portal.
    let (_owner2, mut closed) = create_root(|| {
        Element::new()
            .w(800.0)
            .h(600.0)
            .child(Element::new().w(0.0).h(0.0))
            .child(paint_toast_host())
    });
    let _ = hoist_portal_subtrees(&mut closed);
    let layer = find_portal_layer(&closed);
    assert!(
        !layer.children.iter().any(|c| {
            c.attrs
                .get(ATTR_PORTAL_KEY)
                .map(|s| s.starts_with("underlay:"))
                .unwrap_or(false)
        }),
        "closed command must not leave a full-screen underlay that swallows later toast clicks"
    );
    assert!(
        layer.children.iter().any(|c| {
            c.attrs.get(ATTR_PORTAL_KEY).map(String::as_str) == Some("toast-host")
                && c.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) == Some("1")
        }),
        "toast must sit on the portal layer after command close"
    );
    let _ = root;
    clear_overlay_stack();
}

fn gallery_overlay_body() -> Rc<dyn Fn() -> Element> {
    Rc::new(|| {
        let mk_item = |value: &str, label: &str| {
            CommandItem(CommandItemProps {
                value: Some(value.into()),
                disabled: None,
                checked: None,
                on_click: None,
                children: Some(Element::new().text(label)),
                base: None,
            })
        };
        let mut grp_kids = Element::new();
        grp_kids.set_attr("jadix_fragment", "1");
        let group = CommandGroup(CommandGroupProps {
            heading: Some("Suggestions".into()),
            children: Some(grp_kids.children(vec![
                mk_item("toast", "Show toast"),
                mk_item("log", "Log ping"),
            ])),
            base: None,
        });
        let mut list_kids = Element::new();
        list_kids.set_attr("jadix_fragment", "1");
        let mut root = Element::new();
        root.set_attr("jadix_fragment", "1");
        root.children(vec![
            CommandInput(CommandInputProps {
                placeholder: None,
                children: None,
                base: None,
            }),
            CommandList(CommandListProps {
                children: Some(list_kids.children(vec![group])),
                base: None,
            }),
        ])
    })
}

#[test]
fn command_overlay_keeps_left_radius_and_list_items() {
    clear_overlay_stack();
    let (_owner, mut root) = create_root(|| {
        let inner = command_paint(
            "gal-cmd".into(),
            Signal::new(true),
            "Type a command or search...".into(),
            CommandSurface::Overlay,
            gallery_overlay_body(),
        );
        let frame = command_overlay_frame_open("cmd-overlay-radius-t", inner);
        let host = Element::new()
            .w(0.0)
            .h(0.0)
            .pointer_events_box_none()
            .child(frame);
        let side = Element::new().w(208.0).h("100%").bg([0.1, 0.1, 0.1, 1.0]);
        let body: Element = scroll_area("cmd-overlay-main")
            .overflow_y("auto")
            .flex_grow(1.0)
            .min_w(0.0)
            .h("100%")
            .child(host)
            .into();
        Element::new().w(800.0).h(600.0).child(
            Element::new()
                .w("100%")
                .h("100%")
                .flex_row()
                .child(side)
                .child(body),
        )
    });

    let _ = hoist_portal_subtrees(&mut root);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);

    let stage = find_hoisted_panel(&root, "panel:1001");
    let panel = overlay_palette_panel(stage);
    assert!(
        panel.style.radii.iter().all(|r| *r >= 7.0),
        "overlay panel must keep all four corner radii, got {:?}",
        panel.style.radii
    );
    assert!(
        panel.layout_bounds[0] > 8.0,
        "panel must not be flush-clipped on the left (sidebar), got x={}",
        panel.layout_bounds[0]
    );
    let px = panel.layout_bounds[0];
    let pw = panel.layout_bounds[2];
    let overlay = &result.instances[result.overlay_split.instance_start..];
    let panel_inst = overlay
        .iter()
        .find(|i| {
            i.color[3] > 0.5
                && (i.rect_pos[0] - px).abs() < 2.0
                && (i.rect_size[0] - pw).abs() < 4.0
                && i.radii.iter().any(|r| *r >= 7.0)
        })
        .expect("panel chrome in overlay paint suffix");
    let clip_cuts_left = panel_inst.clip_size[0] > 0.0
        && panel_inst.clip_size[1] > 0.0
        && panel_inst.clip_pos[0] > px + 1.0;
    assert!(
            !clip_cuts_left,
            "main ScrollArea must not scissor overlay left radii (sidebar edge); clip_pos={:?} clip_size={:?} panel_x={}",
            panel_inst.clip_pos, panel_inst.clip_size, px
        );
    let mut items = 0usize;
    walk_cmd(panel, &mut |el| {
        if el.attrs.get("data-slot").map(String::as_str) == Some("command-item") {
            items += 1;
            assert!(
                el.layout_bounds[3] > 10.0,
                "command item must have visible height, got {:?}",
                el.layout_bounds
            );
        }
    });
    assert!(items >= 2, "overlay list items must paint, got {items}");
    assert!(
        panel.layout_bounds[3] > 80.0,
        "overlay must be taller than the search row, got h={}",
        panel.layout_bounds[3]
    );
    clear_overlay_stack();
}

fn find_tid(el: &Element, id: &str) -> bool {
    el.template_id.as_deref() == Some(id) || el.children.iter().any(|c| find_tid(c, id))
}

fn collect_tids(el: &Element) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(el: &Element, out: &mut Vec<String>) {
        if let Some(t) = &el.template_id {
            out.push(t.clone());
        }
        for c in &el.children {
            walk(c, out);
        }
    }
    walk(el, &mut out);
    out
}

/// Gallery: `let cmd_el = ui! { <CommandOverlay/> }` stamps `file:line:column` on the
/// **root**. The reactive anchor id must still be findable underneath.
#[test]
fn command_overlay_anchor_survives_ui_root_template_stamp() {
    clear_overlay_stack();
    crate::app::reactive::reset_registered_anchors();
    let (_owner, stamped) = create_root(|| {
        let open = use_signal("stamp_cmd_open", || false);
        let host = command_overlay_scope("gal-cmd", Some(open), "", || {
            Element::new().w(200.0).h(48.0)
        });
        host.template_id("examples/gallery/src/main.rs:2718:9")
    });
    assert_eq!(
        stamped.template_id.as_deref(),
        Some("examples/gallery/src/main.rs:2718:9"),
        "ui! stamps the wrapper"
    );
    assert!(
            find_tid(&stamped, "gal-cmd__cmd_overlay"),
            "overlay reactive placeholder must remain a descendant after ui! root stamp; tree tids: {:?}",
            collect_tids(&stamped)
        );
    clear_overlay_stack();
    crate::app::reactive::reset_registered_anchors();
}

#[test]
fn command_overlay_in_gapped_row_does_not_occupy_flex_gap() {
    use crate::app::reactive::reset_registered_anchors;
    use crate::primitives::hstack;

    fn find_tid<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
        if el.template_id.as_deref() == Some(id) {
            Some(el)
        } else {
            el.children.iter().find_map(|c| find_tid(c, id))
        }
    }

    clear_overlay_stack();
    reset_registered_anchors();
    let (_owner, mut row) = create_root(|| {
        let open = use_signal("cmd_gap_open", || false);
        let mut a = Element::new().w(40.0).h(20.0);
        a.template_id = Some("cmd-gap-a".into());
        let mut b = Element::new().w(40.0).h(20.0);
        b.template_id = Some("cmd-gap-b".into());
        let host = command_overlay_scope("cmd-gap", Some(open), "", || {
            Element::new().w(200.0).h(48.0)
        });
        hstack().gap(8.0).child(a).child(host).child(b).into()
    });
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut row, 400.0, 80.0, [0.0, 0.0]);
    let aa = find_tid(&row, "cmd-gap-a")
        .unwrap_or_else(|| panic!("missing cmd-gap-a"))
        .layout_bounds;
    let bb = find_tid(&row, "cmd-gap-b")
        .unwrap_or_else(|| panic!("missing cmd-gap-b"))
        .layout_bounds;
    let gap = bb[0] - (aa[0] + aa[2]);
    assert!(
        (gap - 8.0).abs() < 1.0,
        "CommandOverlay leftover host must not occupy parent gap, got {gap}"
    );
    clear_overlay_stack();
    reset_registered_anchors();
}

/// Gallery shape: overlay host sits in an auto-height card inside a ScrollArea
/// (percent height collapses). Auto-height command body must still paint.
#[test]
fn command_overlay_visible_in_gallery_scroll_card() {
    clear_overlay_stack();
    let (_owner, mut root) = create_root(|| {
        let inner: Element = vstack()
            .w("100%")
            .child(Element::new().w("100%").h(36.0).bg([0.2, 0.2, 0.2, 1.0]))
            .child(Element::new().w("100%").h(80.0).bg([0.3, 0.3, 0.3, 1.0]))
            .into();
        let frame = command_overlay_frame_open("cmd-overlay-gallery-t", inner);
        let host = Element::new()
            .w(0.0)
            .h(0.0)
            .pointer_events_box_none()
            .child(frame);
        let card: Element = vstack()
            .w("100%")
            .child(Element::new().w("100%").h(40.0))
            .child(host)
            .into();
        Element::new().w(800.0).h(600.0).child(
            Element::new()
                .w("100%")
                .h("100%")
                .overflow_y("auto")
                .child(card),
        )
    });

    let lifted = hoist_portal_subtrees(&mut root);
    assert!(lifted >= 2, "underlay + stage must hoist; lifted={lifted}");

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);

    let stage = find_hoisted_panel(&root, "panel:1001");
    assert!(
        stage.layout_bounds[2] > 700.0 && stage.layout_bounds[3] > 500.0,
        "gallery host is 0×0; stage must still fill the viewport, got {:?}",
        stage.layout_bounds
    );
    let panel = overlay_palette_panel(stage);
    assert!(
        panel.layout_bounds[3] > 100.0,
        "auto-height command body must not collapse in gallery card, got {:?}",
        panel.layout_bounds
    );
    assert!(
        panel.layout_bounds[2] > 200.0,
        "command panel must have visible width, got {:?}",
        panel.layout_bounds
    );
    clear_overlay_stack();
}

#[test]
fn command_filter_lets_editing_keys_through() {
    assert!(!command_filter_consumes_key(&Key::Named(
        NamedKey::Backspace
    )));
    assert!(!command_filter_consumes_key(&Key::Named(NamedKey::Delete)));
    assert!(!command_filter_consumes_key(&Key::Named(
        NamedKey::ArrowLeft
    )));
    assert!(!command_filter_consumes_key(&Key::Named(NamedKey::Home)));
    assert!(!command_filter_consumes_key(&Key::Named(NamedKey::End)));
    assert!(!command_filter_consumes_key(&Key::Character("a".into())));
    assert!(command_filter_consumes_key(&Key::Named(
        NamedKey::ArrowDown
    )));
    assert!(command_filter_consumes_key(&Key::Named(NamedKey::Enter)));
    assert!(command_filter_consumes_key(&Key::Named(NamedKey::Escape)));
}

#[test]
fn reset_command_session_clears_filter_and_active() {
    let _owner = create_root(|| {
        let id = "cmd-reset-t";
        let filter_id = format!("{id}__filter");
        use_signal(input_value_signal_key(&filter_id).as_str(), String::new).set("hello".into());
        use_signal(format!("{id}__cmd_active").as_str(), || 0usize).set(3);
        reset_command_session(id);
        assert_eq!(
            use_signal(input_value_signal_key(&filter_id).as_str(), String::new).get(),
            ""
        );
        assert_eq!(
            use_signal(format!("{id}__cmd_active").as_str(), || 0usize).get(),
            NO_ACTIVE_COMMAND
        );
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn command_dialog_session_reset_waits_for_unmount() {
    use crate::utils::presence::PresencePhase;
    use crate::utils::viewport_modal::{
        viewport_modal_force_unmount, viewport_modal_is_mounted, viewport_modal_phase,
    };

    clear_overlay_stack();
    let _owner = create_root(|| {
        let id = "cmd-reset-wait";
        let filter_id = format!("{id}__filter");
        let open = use_signal(format!("{id}__open").as_str(), || true);
        let active = use_signal(format!("{id}__cmd_active").as_str(), || 0usize);
        use_signal(input_value_signal_key(&filter_id).as_str(), String::new).set("query".into());
        active.set(2);

        assert_eq!(viewport_modal_phase(id, open), Some(PresencePhase::Present));
        assert!(viewport_modal_is_mounted(id, open));

        open.set(false);
        assert_eq!(viewport_modal_phase(id, open), Some(PresencePhase::Exiting));
        assert!(
            viewport_modal_is_mounted(id, open),
            "dialog must stay mounted during exit animation"
        );
        assert_eq!(active.get(), 2, "active must not reset while still mounted");
        assert_eq!(
            use_signal(input_value_signal_key(&filter_id).as_str(), String::new).get(),
            "query",
            "filter must not clear while still mounted"
        );

        viewport_modal_force_unmount(id);
        reset_command_session(id);
        assert_eq!(active.get(), NO_ACTIVE_COMMAND);
        assert_eq!(
            use_signal(input_value_signal_key(&filter_id).as_str(), String::new).get(),
            ""
        );
    });
    clear_overlay_stack();
}

fn walk_cmd<'a>(el: &'a Element, f: &mut impl FnMut(&'a Element)) {
    f(el);
    for c in &el.children {
        walk_cmd(c, f);
    }
}

fn find_cmd_item(root: &Element) -> &Element {
    let mut found = None;
    walk_cmd(root, &mut |el| {
        if el.attrs.get("data-slot").map(String::as_str) == Some("command-item") {
            found = Some(el);
        }
    });
    found.expect("command-item")
}

fn three_item_command_body() -> Rc<dyn Fn() -> Element> {
    Rc::new(|| {
        let mk = |value: &str, label: &str| {
            CommandItem(CommandItemProps {
                value: Some(value.into()),
                disabled: None,
                checked: None,
                on_click: None,
                children: Some(Element::new().text(label)),
                base: None,
            })
        };
        let mut list_kids = Element::new();
        list_kids.set_attr("jadix_fragment", "1");
        let mut frag = Element::new();
        frag.set_attr("jadix_fragment", "1");
        frag.children(vec![
            CommandInput(CommandInputProps {
                placeholder: None,
                children: None,
                base: None,
            }),
            CommandList(CommandListProps {
                children: Some(list_kids.children(vec![
                    mk("a", "Alpha"),
                    mk("b", "Beta"),
                    mk("c", "Gamma"),
                ])),
                base: None,
            }),
        ])
    })
}

#[test]
fn command_has_no_active_item_until_hover_and_clears_on_leave() {
    let (_owner, root) = create_root(|| {
        command_paint(
            "cmd-no-active".into(),
            Signal::new(true),
            "q".into(),
            CommandSurface::Dialog,
            three_item_command_body(),
        )
    });
    let active = use_signal("cmd-no-active__cmd_active", || NO_ACTIVE_COMMAND);
    assert_eq!(active.peek(), NO_ACTIVE_COMMAND);
    let items = collect_cmd_items(&root);
    assert!(items.iter().all(|item| item.style.bg_color[3] == 0.0));
    for frame in 0..120 {
        let index = frame % items.len();
        (items[index].on_hover.as_ref().unwrap().borrow_mut())(true);
        assert_eq!(active.peek(), index);
        (items[index].on_hover.as_ref().unwrap().borrow_mut())(false);
        assert_eq!(
            active.peek(),
            NO_ACTIVE_COMMAND,
            "pointer leave at sample {frame}"
        );
    }
}

fn collect_cmd_items(root: &Element) -> Vec<&Element> {
    let mut out = Vec::new();
    walk_cmd(root, &mut |el| {
        if el.attrs.get("data-slot").map(String::as_str) == Some("command-item") {
            out.push(el);
        }
    });
    out
}

#[test]
fn command_paint_rebuild_does_not_force_first_item_active() {
    let body = three_item_command_body();
    let (_owner, root) = create_root({
        let body = body.clone();
        move || {
            let open = Signal::new(true);
            let id = "cmd-keep-ix";
            let _ = command_paint(
                id.into(),
                open,
                "q".into(),
                CommandSurface::Dialog,
                body.clone(),
            );
            use_signal(format!("{id}__cmd_active").as_str(), || 0usize).set_silent(2);
            command_paint(id.into(), open, "q".into(), CommandSurface::Dialog, body)
        }
    });
    assert_eq!(
        use_signal("cmd-keep-ix__cmd_active", || 0usize).peek(),
        2,
        "close/rebuild must keep the hovered index; resetting to 0 paints the first row accent"
    );
    let items = collect_cmd_items(&root);
    assert_eq!(items.len(), 3);
    let accent = parse_color("accent");
    assert_ne!(
        items[0].style.bg_color, accent,
        "first item must not take hover accent after rebuild"
    );
    assert_eq!(
        items[2].style.bg_color, accent,
        "active index 2 must keep accent through rebuild"
    );
}

#[test]
fn command_dialog_list_scrollbar_hover_is_not_area() {
    use crate::core::scroll_viewport::scroll_hover_mode_signal_key;
    let (_owner, _) = create_root(|| {
        let _ = command_paint(
            "cmd-hover-mode".into(),
            Signal::new(true),
            "q".into(),
            CommandSurface::Dialog,
            three_item_command_body(),
        );
        let mode = use_signal(&scroll_hover_mode_signal_key("cmd-hover-mode:list"), || {
            "missing".to_string()
        })
        .peek();
        assert_eq!(
            mode, "scrollbar",
            "CommandDialog list must not use hover_mode=area (item hover would latch the bar)"
        );
    });
}

#[test]
fn command_dialog_does_not_focus_filter_during_enter() {
    use crate::utils::viewport_modal::{
        viewport_modal_clear_pending_enter, viewport_modal_motion_busy, viewport_modal_phase,
    };
    clear_overlay_stack();
    let (_owner, _) = create_root(|| {
        let id = "cmd-focus-enter";
        let open = Signal::new(true);
        let _ = viewport_modal_phase(id, open);
        assert!(
            viewport_modal_motion_busy(id),
            "opening a dialog must arm enter motion"
        );
        use_signal("__jadix_button_focus_id", String::new).set(String::new());
        let mut root = command_paint(
            id.into(),
            open,
            "q".into(),
            CommandSurface::Dialog,
            three_item_command_body(),
        );
        let want = input_focus_id(&format!("{id}__filter"));
        assert_ne!(
            use_signal("__jadix_button_focus_id", String::new).peek(),
            want,
            "command_paint must not focus the filter on the enter frame"
        );
        let mut tree = LayoutTree::new();
        let mut impact = crate::core::tree::LayoutStatus::Clean;
        sync_command_item_paint_subtree(&mut root, &mut tree, &mut impact);
        assert_ne!(
            use_signal("__jadix_button_focus_id", String::new).peek(),
            want,
            "paint sync must not focus the filter while enter is busy"
        );
        viewport_modal_clear_pending_enter(id);
        sync_command_item_paint_subtree(&mut root, &mut tree, &mut impact);
        assert_eq!(
            use_signal("__jadix_button_focus_id", String::new).peek(),
            want,
            "filter focuses only after enter motion is idle"
        );
    });
    clear_overlay_stack();
}

#[test]
fn command_overlay_does_not_focus_filter_during_enter() {
    use crate::utils::viewport_modal::{
        viewport_modal_clear_pending_enter, viewport_modal_motion_busy, viewport_modal_phase,
    };
    clear_overlay_stack();
    let (_owner, _) = create_root(|| {
        let id = "cmd-focus-overlay";
        let open = Signal::new(true);
        let _ = viewport_modal_phase(id, open);
        assert!(
            viewport_modal_motion_busy(id),
            "opening an overlay must arm enter motion"
        );
        use_signal("__jadix_button_focus_id", String::new).set(String::new());
        let mut root = command_paint(
            id.into(),
            open,
            "q".into(),
            CommandSurface::Overlay,
            three_item_command_body(),
        );
        let want = input_focus_id(&format!("{id}__filter"));
        assert_ne!(
            use_signal("__jadix_button_focus_id", String::new).peek(),
            want,
            "command_paint must not focus the overlay filter on the enter frame"
        );
        let mut tree = LayoutTree::new();
        let mut impact = crate::core::tree::LayoutStatus::Clean;
        sync_command_item_paint_subtree(&mut root, &mut tree, &mut impact);
        assert_ne!(
            use_signal("__jadix_button_focus_id", String::new).peek(),
            want,
            "paint sync must not focus the overlay filter while enter is busy"
        );
        viewport_modal_clear_pending_enter(id);
        sync_command_item_paint_subtree(&mut root, &mut tree, &mut impact);
        assert_eq!(
            use_signal("__jadix_button_focus_id", String::new).peek(),
            want,
            "overlay filter focuses only after enter motion is idle"
        );
    });
    clear_overlay_stack();
}

#[test]
fn command_dialog_list_scroll_caps_height() {
    let mk_item = |label: &str| {
        CommandItem(CommandItemProps {
            value: Some(label.into()),
            disabled: None,
            checked: None,
            on_click: None,
            children: Some(Element::new().text(label)),
            base: None,
        })
    };
    let (_owner, root) = create_root(|| {
        command_paint(
            "cmd-dlg-scroll".into(),
            Signal::new(true),
            "search".into(),
            CommandSurface::Dialog,
            Rc::new(move || {
                let mut items = Vec::new();
                for i in 0..12 {
                    items.push(mk_item(&format!("Item {i}")));
                }
                let mut grp_kids = Element::new();
                grp_kids.set_attr("jadix_fragment", "1");
                let group = CommandGroup(CommandGroupProps {
                    heading: Some("Suggestions".into()),
                    children: Some(grp_kids.children(items)),
                    base: None,
                });
                let mut list_kids = Element::new();
                list_kids.set_attr("jadix_fragment", "1");
                let mut frag = Element::new();
                frag.set_attr("jadix_fragment", "1");
                frag.children(vec![
                    CommandInput(CommandInputProps {
                        placeholder: None,
                        children: None,
                        base: None,
                    }),
                    CommandList(CommandListProps {
                        children: Some(list_kids.children(vec![group])),
                        base: None,
                    }),
                ])
            }),
        )
    });
    let mut host = Element::new().w(800.0).h(600.0).child(root);
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut host, 800.0, 600.0, [-1.0, -1.0]);
    let mut list_h = 0.0f32;
    walk_cmd(&host, &mut |el| {
        if el.attrs.get("data-slot").map(String::as_str) == Some("command-list") {
            list_h = el.layout_bounds[3];
        }
    });
    assert!(
        list_h > 0.0,
        "command-list must layout with non-zero height"
    );
    assert!(
        list_h <= 305.0,
        "command-list must cap at max-h 18.75rem (300px) and scroll, got h={list_h}"
    );
}

fn gallery_like_command_body() -> Rc<dyn Fn() -> Element> {
    Rc::new(|| {
        let mk_item = |value: &str, label: &str| {
            CommandItem(CommandItemProps {
                value: Some(value.into()),
                disabled: None,
                checked: None,
                on_click: None,
                children: Some(Element::new().text(label)),
                base: None,
            })
        };
        let group = |heading: &str, items: Vec<Element>| {
            let mut grp_kids = Element::new();
            grp_kids.set_attr("jadix_fragment", "1");
            CommandGroup(CommandGroupProps {
                heading: Some(heading.into()),
                children: Some(grp_kids.children(items)),
                base: None,
            })
        };
        let mut list_kids = Element::new();
        list_kids.set_attr("jadix_fragment", "1");
        let mut frag = Element::new();
        frag.set_attr("jadix_fragment", "1");
        frag.children(vec![
            CommandInput(CommandInputProps {
                placeholder: None,
                children: None,
                base: None,
            }),
            CommandList(CommandListProps {
                children: Some(list_kids.children(vec![
                    group(
                        "Suggestions",
                        vec![
                            mk_item("toast", "Show toast"),
                            mk_item("calendar", "Calendar"),
                            mk_item("emoji", "Search Emoji"),
                            mk_item("calc", "Calculator"),
                        ],
                    ),
                    CommandSeparator(CommandSeparatorProps {
                        children: None,
                        base: None,
                    }),
                    group(
                        "Settings",
                        vec![
                            mk_item("profile", "Profile"),
                            mk_item("billing", "Billing"),
                            mk_item("settings", "Settings"),
                        ],
                    ),
                    CommandSeparator(CommandSeparatorProps {
                        children: None,
                        base: None,
                    }),
                    group(
                        "File",
                        vec![
                            mk_item("new-file", "New File"),
                            mk_item("open", "Open..."),
                            mk_item("save", "Save"),
                        ],
                    ),
                ])),
                base: None,
            }),
        ])
    })
}

/// Gallery CommandDialog body: CommandEmpty registrar + shortcut pills on Settings/File.
fn gallery_command_dialog_body() -> Rc<dyn Fn() -> Element> {
    Rc::new(|| {
        let mk_item = |value: &str, label: &str| {
            CommandItem(CommandItemProps {
                value: Some(value.into()),
                disabled: None,
                checked: None,
                on_click: None,
                children: Some(Element::new().text(label)),
                base: None,
            })
        };
        let mk_shortcut_item = |value: &str, label: &str, keys: &str| {
            let mut kids = Element::new();
            kids.set_attr("jadix_fragment", "1");
            CommandItem(CommandItemProps {
                value: Some(value.into()),
                disabled: None,
                checked: None,
                on_click: None,
                children: Some(kids.children(vec![
                    Element::new().text(label),
                    CommandShortcut(CommandShortcutProps {
                        children: Some(Element::new().text(keys)),
                        base: None,
                    }),
                ])),
                base: None,
            })
        };
        let group = |heading: &str, items: Vec<Element>| {
            let mut grp_kids = Element::new();
            grp_kids.set_attr("jadix_fragment", "1");
            CommandGroup(CommandGroupProps {
                heading: Some(heading.into()),
                children: Some(grp_kids.children(items)),
                base: None,
            })
        };
        let mut list_kids = Element::new();
        list_kids.set_attr("jadix_fragment", "1");
        let mut frag = Element::new();
        frag.set_attr("jadix_fragment", "1");
        frag.children(vec![
            CommandInput(CommandInputProps {
                placeholder: None,
                children: None,
                base: None,
            }),
            CommandList(CommandListProps {
                children: Some(list_kids.children(vec![
                    CommandEmpty(CommandEmptyProps {
                        children: Some(Element::new().text("No results found.")),
                        base: None,
                    }),
                    group(
                        "Suggestions",
                        vec![
                            mk_item("toast", "Show toast"),
                            mk_item("calendar", "Calendar"),
                            mk_item("emoji", "Search Emoji"),
                            mk_item("calc", "Calculator"),
                        ],
                    ),
                    CommandSeparator(CommandSeparatorProps {
                        children: None,
                        base: None,
                    }),
                    group(
                        "Settings",
                        vec![
                            mk_shortcut_item("profile", "Profile", "⌘P"),
                            mk_shortcut_item("billing", "Billing", "⌘B"),
                            mk_shortcut_item("settings", "Settings", "⌘S"),
                        ],
                    ),
                    CommandSeparator(CommandSeparatorProps {
                        children: None,
                        base: None,
                    }),
                    group(
                        "File",
                        vec![
                            mk_shortcut_item("new-file", "New File", "⌘N"),
                            mk_shortcut_item("open", "Open...", "⌘O"),
                            mk_shortcut_item("save", "Save", "⌘S"),
                        ],
                    ),
                ])),
                base: None,
            }),
        ])
    })
}

fn layout_command_dialog_host(modal_id: &str, body: Rc<dyn Fn() -> Element>) -> Element {
    use crate::components::base::SlotChrome;
    use crate::components::dialog::dialog_shell_frame;
    use crate::utils::modal_shell::{default_modal_overlay_color, ShellOverlayOpts};

    let open = Signal::new(true);
    let overlay_opts = ShellOverlayOpts {
        color: default_modal_overlay_color(None, Some(0.5)),
        close_on_click: true,
        backdrop_blur: 0.0,
    };
    let inner = command_paint(
        modal_id.into(),
        open,
        "search".into(),
        CommandSurface::Dialog,
        body,
    );
    let shell = dialog_shell_frame(
        modal_id,
        open,
        inner,
        &overlay_opts,
        None,
        true,
        None,
        None,
        None,
        None,
        SlotChrome::default(),
    );
    Element::new().w("100%").h("100%").child(shell)
}

fn item_bounds_by_value(root: &Element, value: &str) -> Option<[f32; 4]> {
    let mut found = None;
    walk_cmd(root, &mut |el| {
        if el.attrs.get("data-slot").map(String::as_str) != Some("command-item") {
            return;
        }
        // CommandItem doesn't stamp `value` on the node; match by first text leaf.
        if crate::core::tree::first_text_leaf(el) == value {
            found = Some(el.layout_bounds);
        }
    });
    found
}

#[test]
fn command_dialog_gallery_settings_items_stable_across_layouts() {
    clear_overlay_stack();
    let modal_id = "cmd-dlg-settings-jump";
    let (_owner, mut host) =
        create_root(|| layout_command_dialog_host(modal_id, gallery_command_dialog_body()));
    assert!(hoist_portal_subtrees(&mut host) >= 2);

    let mut empty_stub_h = None;
    walk_cmd(&host, &mut |el| {
        if el.attrs.get("jadix_command_empty_stub").map(String::as_str) == Some("1") {
            empty_stub_h = Some(el.layout_bounds[3]);
        }
    });
    assert!(
        empty_stub_h.is_none(),
        "CommandEmpty registrar must not stay in the list layout (would push groups down)"
    );

    let layout_once = |host: &mut Element| {
        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(host, 800.0, 600.0, [-1.0, -1.0]);
        (
            item_bounds_by_value(host, "Profile").expect("Profile"),
            item_bounds_by_value(host, "Billing").expect("Billing"),
            item_bounds_by_value(host, "Settings").expect("Settings item"),
            item_bounds_by_value(host, "New File").expect("New File"),
        )
    };

    let first = layout_once(&mut host);
    let second = layout_once(&mut host);
    for (name, a, b) in [
        ("Profile", first.0, second.0),
        ("Billing", first.1, second.1),
        ("Settings", first.2, second.2),
        ("New File", first.3, second.3),
    ] {
        assert!(
            (a[1] - b[1]).abs() < 0.5,
            "{name} y jumped across layouts: {a:?} -> {b:?}"
        );
    }
    let profile_gap = first.1[1] - (first.0[1] + first.0[3]);
    let billing_to_settings = first.2[1] - (first.1[1] + first.1[3]);
    assert!(
            (profile_gap - billing_to_settings).abs() < 1.0,
            "Settings-group rows must share the same inter-item gap; got profile→billing={profile_gap} billing→settings={billing_to_settings}"
        );
    let settings_to_file = first.3[1] - (first.2[1] + first.2[3]);
    assert!(
        settings_to_file > 0.0,
        "File group must sit below Settings, gap={settings_to_file}"
    );
    clear_overlay_stack();
}

/// Enter-settle must not move CommandDialog rows in layout or Coverage Y.
/// Remaining screen-space travel while fade is already visible is the
/// `__modal_zoom` layer scaling about the card center (lower rows move down).
#[test]
#[cfg(feature = "animation-base")]
fn command_dialog_item_y_stable_across_enter_settle() {
    use crate::utils::modal_motion::{flush_modal_shell_motion_subtree, tick_hoisted_modal_css};
    use crate::utils::text_field::input_focus_id;
    use crate::utils::viewport_modal::viewport_modal_motion_busy;

    crate::style::utility::set_frame_dt(1.0 / 60.0);
    clear_overlay_stack();
    let modal_id = "cmd-dlg-enter-y";
    let (_owner, mut host) =
        create_root(|| layout_command_dialog_host(modal_id, gallery_command_dialog_body()));
    assert!(hoist_portal_subtrees(&mut host) >= 2);

    let item_ys = |host: &Element| {
        (
            item_bounds_by_value(host, "Show toast").expect("Show toast")[1],
            item_bounds_by_value(host, "Profile").expect("Profile")[1],
        )
    };

    let mut last_comp_ys = None;
    let mut last_blit_profile_y = None;
    let mut last_comp_live_also = false;
    let mut settle_ys = None;

    for frame in 0..24 {
        let _ = flush_modal_shell_motion_subtree(&mut host);
        let _ = tick_hoisted_modal_css(&mut host);
        let busy = viewport_modal_motion_busy(modal_id);
        let _ = crate::renderer::take_capture_jobs();
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut host, 800.0, 600.0, [-1.0, -1.0]);
        let caps = crate::renderer::take_capture_jobs();
        let cap = caps.iter().find(|c| c.key.starts_with("jadix_zoom_cap:"));
        let ys = item_ys(&host);
        if let Some(cap) = cap {
            last_comp_ys = Some(ys);
            last_comp_live_also = result.text_nodes.iter().any(|t| t.text == "Profile");
            if let (Some(ty), Some(blit)) = (
                cap.text_nodes
                    .iter()
                    .find(|t| t.text == "Profile")
                    .map(|t| t.y),
                result
                    .textured_draws
                    .iter()
                    .find(|d| d.texture_key.starts_with("jadix_zoom_cap:")),
            ) {
                let b = cap.bounds;
                last_blit_profile_y = Some(blit.rect[1] + (ty - b[1]) * (blit.rect[3] / b[3]));
            }
        }
        if !busy {
            settle_ys = Some(ys);
            let live_y = result
                .text_nodes
                .iter()
                .find(|t| t.text == "Profile")
                .map(|t| t.y);
            let last_c = last_comp_ys.expect("enter must capture __modal_zoom");
            assert!(
                !last_comp_live_also,
                "1:1 blit must not also emit live Coverage (row pop)"
            );
            assert!(
                (last_c.0 - ys.0).abs() < 0.5 && (last_c.1 - ys.1).abs() < 0.5,
                "item layout y jumped compositor→settle: {last_c:?} -> {ys:?}"
            );
            if let (Some(blit_y), Some(live_y)) = (last_blit_profile_y, live_y) {
                assert!(
                    (blit_y - live_y).abs() < 1.0,
                    "Profile blit-mapped y {blit_y} != Coverage {live_y}"
                );
            }
            break;
        }
        assert!(frame < 23, "enter must settle within 24 frames");
    }

    let settle = settle_ys.expect("enter must settle");
    let mut tree = LayoutTree::new();
    let mut impact = crate::core::tree::LayoutStatus::Clean;
    sync_command_item_paint_subtree(&mut host, &mut tree, &mut impact);
    let _ = tree.build_and_layout(&mut host, 800.0, 600.0, [-1.0, -1.0]);
    let focused = item_ys(&host);
    assert!(
        (settle.0 - focused.0).abs() < 0.5 && (settle.1 - focused.1).abs() < 0.5,
        "item layout y jumped settle→filter-focus: {settle:?} -> {focused:?}"
    );
    assert_eq!(
        use_signal("__jadix_button_focus_id", String::new).peek(),
        input_focus_id(&format!("{modal_id}__filter")),
        "filter focuses only after enter motion is idle"
    );
    clear_overlay_stack();
}

#[test]
fn command_dialog_list_max_scroll_reveals_last_item() {
    use crate::components::base::SlotChrome;
    use crate::components::dialog::dialog_shell_frame;
    use crate::utils::modal_shell::{default_modal_overlay_color, ShellOverlayOpts};

    clear_overlay_stack();
    let modal_id = "cmd-dlg-scroll-end";
    let (_owner, mut host) = create_root(|| {
        let open = Signal::new(true);
        let overlay_opts = ShellOverlayOpts {
            color: default_modal_overlay_color(None, Some(0.5)),
            close_on_click: true,
            backdrop_blur: 0.0,
        };
        let inner = command_paint(
            modal_id.into(),
            open,
            "search".into(),
            CommandSurface::Dialog,
            gallery_like_command_body(),
        );
        let shell = dialog_shell_frame(
            modal_id,
            open,
            inner,
            &overlay_opts,
            None,
            true,
            None,
            None,
            None,
            None,
            SlotChrome::default(),
        );
        Element::new().w("100%").h("100%").child(shell)
    });
    assert!(hoist_portal_subtrees(&mut host) >= 2);

    for (vw, vh) in [(800.0, 600.0), (400.0, 400.0)] {
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut host, vw, vh, [-1.0, -1.0]);
        let zoom_id = format!("{modal_id}__modal_zoom");
        let mut card_bounds = [0.0f32; 4];
        let mut last_item: Option<[f32; 4]> = None;
        let mut list_bounds = [0.0f32; 4];
        let mut scrollbar_bounds: Option<[f32; 4]> = None;
        let track_fid = format!("scrollbar-track:{modal_id}:list:y");
        walk_cmd(&host, &mut |el| {
            if el.template_id.as_deref() == Some(zoom_id.as_str()) {
                card_bounds = el.layout_bounds;
            }
            if el.attrs.get("data-slot").map(String::as_str) == Some("command-list") {
                list_bounds = el.layout_bounds;
            }
            if el.attrs.get("data-slot").map(String::as_str) == Some("command-item") {
                last_item = Some(el.layout_bounds);
            }
            if el.focus_id.as_deref() == Some(track_fid.as_str()) {
                scrollbar_bounds = Some(el.layout_bounds);
            }
        });
        let item = last_item.expect("command-item");
        let item_bottom = item[1] + item[3];
        let list_bottom = list_bounds[1] + list_bounds[3];
        let card_bottom = card_bounds[1] + card_bounds[3];
        let header_h = (list_bounds[1] - card_bounds[1]).max(0.0);
        let rest = format!("{modal_id}:list");
        let (_id, vp, max_scroll, content) = result
            .scroll_metrics
            .iter()
            .find(|(id, ..)| id == &rest)
            .unwrap_or_else(|| {
                panic!(
                    "missing scroll metrics for {rest} at {vw}x{vh}; have {:?}",
                    result
                        .scroll_metrics
                        .iter()
                        .map(|(id, ..)| id.as_str())
                        .collect::<Vec<_>>()
                )
            });
        let content_bottom = list_bounds[1] + content[1];
        let visible_bottom_at_end = list_bottom + max_scroll[1];
        let pad_after_last = content_bottom - item_bottom;
        let painted_item_bottom = item_bottom - max_scroll[1];
        let max_card = vh * 0.85;
        assert!(
            card_bounds[3] <= max_card + 1.0,
            "card must honor max_h 85dvh at {vw}x{vh}: card_h={} cap={max_card}",
            card_bounds[3]
        );
        assert!(
            list_bounds[3] > 8.0,
            "list must keep a usable viewport (not crushed) at {vw}x{vh}, list_h={}",
            list_bounds[3]
        );
        assert!(
                list_bottom <= card_bottom + 1.0,
                "list must stay inside the card (header+list must fit max_h): list_bottom={list_bottom} card_bottom={card_bottom} header_h={header_h} list_h={} card_h={} {vw}x{vh}",
                list_bounds[3],
                card_bounds[3]
            );
        if let Some(sb) = scrollbar_bounds {
            let sb_bottom = sb[1] + sb[3];
            assert!(
                    sb_bottom <= card_bottom + 1.0,
                    "scrollbar must stay inside the card at {vw}x{vh}: sb_bottom={sb_bottom} card_bottom={card_bottom} sb={sb:?}"
                );
        }
        assert!(
                content[1] > vp[3] + 1.0,
                "list content must overflow viewport so it can scroll at {vw}x{vh} (content_h={} vp_h={})",
                content[1],
                vp[3]
            );
        assert!(
                item_bottom <= content_bottom + 1.0,
                "content_h must cover last item at {vw}x{vh}: item_bottom={item_bottom} content_bottom={content_bottom} content_h={} list_y={} item={:?}",
                content[1],
                list_bounds[1],
                item
            );
        assert!(
                pad_after_last >= 3.0,
                "content_h must include CommandList trailing inset after last item at {vw}x{vh}, got pad_after_last={pad_after_last}"
            );
        assert!(
                item_bottom <= visible_bottom_at_end + 1.0,
                "last command-item must be fully visible at max scroll ({vw}x{vh}): item_bottom={item_bottom} visible_end={visible_bottom_at_end} max_scroll_y={} list_bottom={list_bottom} item_h={}",
                max_scroll[1],
                item[3]
            );
        assert!(
                painted_item_bottom <= card_bottom + 1.0,
                "at max scroll last item must sit inside the card clip ({vw}x{vh}): painted_bottom={painted_item_bottom} card_bottom={card_bottom} list_bottom={list_bottom} max_scroll_y={} item={:?}",
                max_scroll[1],
                item
            );
    }
    clear_overlay_stack();
}

#[test]
fn command_dialog_item_click_runs_on_click_then_closes() {
    use crate::core::reactivity::use_signal;
    use std::cell::Cell;
    let ran = Rc::new(Cell::new(false));
    let (_owner, (root, open)) = create_root({
        let ran = ran.clone();
        move || {
            let open = use_signal("cmd-cal-dlg-open", || true);
            let ran = ran.clone();
            let root = command_paint(
                "cmd-cal-dlg".into(),
                open,
                "search".into(),
                CommandSurface::Dialog,
                Rc::new(move || {
                    let ran = ran.clone();
                    let item = CommandItem(CommandItemProps {
                        value: Some("calendar".into()),
                        disabled: None,
                        checked: None,
                        on_click: Some(Handler::new(move |_| ran.set(true))),
                        children: Some(Element::new().text("Calendar")),
                        base: None,
                    });
                    let mut grp_kids = Element::new();
                    grp_kids.set_attr("jadix_fragment", "1");
                    let group = CommandGroup(CommandGroupProps {
                        heading: Some("Suggestions".into()),
                        children: Some(grp_kids.children(vec![item])),
                        base: None,
                    });
                    let mut list_kids = Element::new();
                    list_kids.set_attr("jadix_fragment", "1");
                    let mut frag = Element::new();
                    frag.set_attr("jadix_fragment", "1");
                    frag.children(vec![
                        CommandInput(CommandInputProps {
                            placeholder: None,
                            children: None,
                            base: None,
                        }),
                        CommandList(CommandListProps {
                            children: Some(list_kids.children(vec![group])),
                            base: None,
                        }),
                    ])
                }),
            );
            (root, open)
        }
    });
    find_cmd_item(&root)
        .on_click
        .as_ref()
        .expect("item click")
        .borrow_mut()();
    assert!(ran.get(), "Calendar on_click must run before dismiss");
    assert!(
        !open.peek(),
        "dismissible CommandDialog closes after activate"
    );
}

#[test]
fn command_palette_matches_shadcn_chrome() {
    let (_owner, root) = create_root(|| {
        command_paint(
            "cmd-style".into(),
            Signal::new(true),
            "Type a command or search...".into(),
            CommandSurface::Overlay,
            Rc::new(|| {
                let mut item_kids = Element::new();
                item_kids.set_attr("jadix_fragment", "1");
                let item = CommandItem(CommandItemProps {
                    value: Some("home".into()),
                    disabled: None,
                    checked: None,
                    on_click: None,
                    children: Some(item_kids.children(vec![
                        Element::new().text("Home"),
                        CommandShortcut(CommandShortcutProps {
                            children: Some(Element::new().text("⌘H")),
                            base: None,
                        }),
                    ])),
                    base: None,
                });
                let mut grp_kids = Element::new();
                grp_kids.set_attr("jadix_fragment", "1");
                let group = CommandGroup(CommandGroupProps {
                    heading: Some("Navigation".into()),
                    children: Some(grp_kids.children(vec![item])),
                    base: None,
                });
                let mut list_kids = Element::new();
                list_kids.set_attr("jadix_fragment", "1");
                let mut root = Element::new();
                root.set_attr("jadix_fragment", "1");
                root.children(vec![
                    CommandInput(CommandInputProps {
                        placeholder: None,
                        children: None,
                        base: None,
                    }),
                    CommandList(CommandListProps {
                        children: Some(list_kids.children(vec![group])),
                        base: None,
                    }),
                ])
            }),
        )
    });

    let mut plain = false;
    let mut item_radius = 0.0f32;
    let mut item_text = 0.0f32;
    let mut item_hover = false;
    let mut shortcut_tracking = 99.0f32;
    walk_cmd(&root, &mut |el| {
        if el.input_paint_sync.as_ref().is_some_and(|s| s.plain_chrome) {
            plain = true;
        }
        if el.attrs.get("data-slot").map(String::as_str) == Some("command-item") {
            item_radius = el.style.radii[0];
            item_hover = el.on_hover.is_some();
            walk_cmd(el, &mut |n| {
                if n.text.as_deref() == Some("Home") {
                    item_text = n.text_style.size_resolved();
                }
                if n.text.as_deref() == Some("⌘H") {
                    shortcut_tracking = n.text_style.letter_spacing;
                }
            });
        }
    });
    assert!(plain, "command filter must suppress the Input focus ring");
    assert!(
        item_radius >= 5.0,
        "command item highlight must be rounded-md, got {item_radius}"
    );
    assert!(item_hover, "command items must highlight on pointer hover");
    assert!(
        (item_text - 13.0).abs() < 0.01,
        "command item label must be 13px, got {item_text}"
    );
    assert!(
        shortcut_tracking < 1.0,
        "shortcut tracking must stay readable, got {shortcut_tracking}"
    );
}

#[test]
fn command_item_hover_paint_moves_without_rebuild() {
    let (_owner, mut root) = create_root(|| {
        command_paint(
            "cmd-hover".into(),
            Signal::new(true),
            "Type".into(),
            CommandSurface::Inline,
            Rc::new(|| {
                let a = CommandItem(CommandItemProps {
                    value: Some("a".into()),
                    disabled: None,
                    checked: None,
                    on_click: None,
                    children: Some(Element::new().text("Alpha")),
                    base: None,
                });
                let b = CommandItem(CommandItemProps {
                    value: Some("b".into()),
                    disabled: None,
                    checked: None,
                    on_click: None,
                    children: Some(Element::new().text("Beta")),
                    base: None,
                });
                let mut kids = Element::new();
                kids.set_attr("jadix_fragment", "1");
                CommandList(CommandListProps {
                    children: Some(kids.children(vec![a, b])),
                    base: None,
                })
            }),
        )
    });

    fn items(el: &Element) -> Vec<&Element> {
        let mut out = Vec::new();
        fn walk<'a>(el: &'a Element, out: &mut Vec<&'a Element>) {
            if el.attrs.get("data-slot").map(String::as_str) == Some("command-item") {
                out.push(el);
            }
            for c in &el.children {
                walk(c, out);
            }
        }
        walk(el, &mut out);
        out
    }

    assert_eq!(items(&root).len(), 2);
    use_signal("cmd-hover__cmd_active", || 0usize).set_silent(1);
    let mut tree = LayoutTree::new();
    let mut impact = crate::core::tree::LayoutStatus::Clean;
    sync_command_item_paint_subtree(&mut root, &mut tree, &mut impact);
    let painted = items(&root);
    let accent = parse_color("accent");
    assert_ne!(
        painted[0].style.bg_color, accent,
        "unhovered row must drop accent"
    );
    assert_eq!(
        painted[1].style.bg_color, accent,
        "active row must take accent via paint sync"
    );
}
