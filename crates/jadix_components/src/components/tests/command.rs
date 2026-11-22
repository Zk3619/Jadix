use super::*;
use crate::app::portal_overlay::clear_overlay_stack;
use crate::core::reactivity::create_root;
use crate::layout::layout_tree::{
    ATTR_HOISTED_PORTAL, ATTR_PORTAL_KEY, LayoutTree, PORTAL_LAYER_ID, hoist_portal_subtrees,
};

#[test]
fn command_action_holds_label() {
    let a = CommandAction::new("ping", "Ping host", || {})
        .keywords(["server", "remote"])
        .shortcut("⌘P");
    assert_eq!(a.id, "ping");
    assert_eq!(a.label, "Ping host");
    assert_eq!(a.keywords, ["server", "remote"]);
    assert_eq!(a.shortcut.as_deref(), Some("⌘P"));
}

#[test]
fn command_search_matches_keywords_case_insensitively() {
    assert!(command_item_matches_query(
        "Open settings",
        "settings",
        &["preferences".into(), "configuration".into()],
        "PREFER"
    ));
    assert!(command_item_matches_query(
        "Open settings",
        "settings",
        &["preferences".into()],
        "setting"
    ));
    assert!(!command_item_matches_query(
        "Open settings",
        "settings",
        &["preferences".into()],
        "unrelated"
    ));
}

#[test]
fn command_cached_search_tracks_dynamic_fields_across_surfaces() {
    for surface in [
        CommandSurface::Inline,
        CommandSurface::Dialog,
        CommandSurface::Overlay,
    ] {
        let (_owner, ()) = create_root(|| {
            let id = "cmd-cached-fields";
            let query = use_signal(
                input_value_signal_key(&format!("{id}__filter")).as_str(),
                String::new,
            );
            let rows = Rc::new(RefCell::new(vec![
                (
                    "a".to_string(),
                    "ÉCOLE".to_string(),
                    vec!["learn".to_string()],
                ),
                (
                    "b".to_string(),
                    "Beta".to_string(),
                    vec!["prefs".to_string()],
                ),
            ]));
            let body: Rc<dyn Fn() -> Element> = {
                let rows = rows.clone();
                Rc::new(move || {
                    Element::new().children(
                        rows.borrow()
                            .iter()
                            .map(|(value, label, keywords)| {
                                CommandItem(CommandItemProps {
                                    value: Some(value.clone()),
                                    children: Some(Element::new().text(label.clone())),
                                    keywords: Some(keywords.clone()),
                                    ..Default::default()
                                })
                            })
                            .collect::<Vec<_>>(),
                    )
                })
            };
            let open = Signal::new(true);
            let paint = |q: &str| {
                query.set(q.to_owned());
                let root = command_paint(id.into(), open, "Search".into(), surface, body.clone());
                if q == "Éc" {
                    let label = crate::core::tree::find_in_tree(&root, |el| {
                        el.text.as_deref() == Some("ÉCOLE")
                    })
                    .expect("matching Unicode label");
                    assert_eq!(
                        label
                            .text_spans
                            .as_ref()
                            .unwrap()
                            .iter()
                            .filter(|span| span.color.is_some())
                            .map(|span| span.text.as_str())
                            .collect::<Vec<_>>(),
                        ["ÉC"]
                    );
                }
                use_signal(
                    format!("{id}__cmd_visible").as_str(),
                    Vec::<CommandVisibleItem>::new,
                )
                .with_untracked(|list| {
                    list.iter()
                        .map(|item| item.value.clone())
                        .collect::<Vec<_>>()
                })
            };
            assert_eq!(paint("Éc"), ["a"]);
            assert_eq!(paint("PREF"), ["b"]);
            assert_eq!(paint("PREF"), ["b"]);
            rows.borrow_mut()[0].2 = vec!["preferences".into()];
            rows.borrow_mut().swap(0, 1);
            assert_eq!(paint("PREFE"), ["a"]);
            rows.borrow_mut()[1].0 = "new-id".into();
            rows.borrow_mut()[1].1 = "Renamed".into();
            rows.borrow_mut()[1].2.clear();
            assert_eq!(paint("new-id"), ["new-id"]);
            assert_eq!(paint("RENAME"), ["new-id"]);
            rows.borrow_mut().truncate(1);
            assert!(paint("RENAME").is_empty());
            assert_eq!(paint(""), ["b"]);
        });
    }
}

#[test]
fn command_accessibility_tracks_silent_highlight_filtering_and_dismissal() {
    use crate::authoring::presentation::{self, Context, Phase};

    for surface in [
        CommandSurface::Inline,
        CommandSurface::Dialog,
        CommandSurface::Overlay,
    ] {
        let (_owner, ()) = create_root(|| {
            let id = format!("cmd-semantics-{surface:?}");
            let open = Signal::new(true);
            let active = use_signal(&format!("{id}__cmd_active"), || NO_ACTIVE_COMMAND);
            let query = use_signal(
                &input_value_signal_key(&format!("{id}__filter")),
                String::new,
            );
            let body: Rc<dyn Fn() -> Element> = Rc::new(|| {
                let field = CommandInput(CommandInputProps::default());
                let items = [
                    ("a", "Alpha", false),
                    ("b", "Beta", true),
                    ("c", "Charlie", false),
                ]
                .into_iter()
                .map(|(value, label, disabled)| {
                    CommandItem(CommandItemProps {
                        value: Some(value.into()),
                        children: Some(Element::new().text(label)),
                        disabled: Some(disabled),
                        ..Default::default()
                    })
                })
                .collect::<Vec<_>>();
                let mut fragment = Element::new();
                fragment.set_attr("jadix_fragment", "1");
                Element::new()
                    .child(field)
                    .child(CommandList(CommandListProps {
                        children: Some(fragment.children(items)),
                        ..Default::default()
                    }))
            });
            let paint = || {
                command_paint(
                    id.clone(),
                    open,
                    "Search commands".into(),
                    surface,
                    body.clone(),
                )
            };
            let mut root = paint();
            let mut layout = LayoutTree::new();
            let sync = |root: &mut Element, layout: &mut LayoutTree| {
                crate::app::test_support::sync_reactive_elements_subtree(
                    root,
                    layout,
                    &mut LayoutStatus::Clean,
                );
                presentation::sync_tree(root, layout, Context::new(Phase::State, [800.0, 600.0]));
            };
            let inspect = |root: &Element, expected: Option<usize>, expanded: bool| {
                let size = use_signal(
                    &format!("{id}__cmd_visible"),
                    Vec::<CommandVisibleItem>::new,
                )
                .with_untracked(Vec::len)
                .to_string();
                let mut found_input = false;
                let mut found_list = false;
                let mut selected = Vec::new();
                walk_cmd(root, &mut |el| {
                    if el.focus_id.as_deref()
                        == Some(input_focus_id(&format!("{id}__filter")).as_str())
                    {
                        found_input = true;
                        assert_eq!(el.attrs["accesskit_role"], "combobox");
                        assert_eq!(el.attrs["aria-autocomplete"], "list");
                        assert_eq!(el.attrs["accesskit_name"], "Search commands");
                        assert_eq!(el.attrs["aria-expanded"], expanded.to_string());
                        assert_eq!(
                            el.attrs.get("aria-controls"),
                            expanded.then(|| format!("{id}:listbox")).as_ref()
                        );
                        assert_eq!(
                            el.attrs.get("aria-activedescendant"),
                            expected
                                .filter(|_| expanded)
                                .map(|ix| format!("{id}:item:{ix}"))
                                .as_ref()
                        );
                    }
                    if el.attrs.get("data-slot").map(String::as_str) == Some("command-list") {
                        found_list = true;
                        assert_eq!(el.attrs["accesskit_role"], "listbox");
                        assert_eq!(el.attrs["id"], format!("{id}:listbox"));
                        assert_eq!(el.attrs["accesskit_size_of_set"], size);
                    }
                    if el.attrs.get("data-slot").map(String::as_str) == Some("command-item") {
                        let ix = el.attrs["jadix_command_ix"].parse::<usize>().unwrap();
                        assert_eq!(el.attrs["accesskit_role"], "listboxoption");
                        assert_eq!(el.attrs["id"], format!("{id}:item:{ix}"));
                        assert_eq!(el.attrs["aria-posinset"], (ix + 1).to_string());
                        assert_eq!(el.attrs["aria-setsize"], size);
                        if el.attrs["aria-selected"] == "true" {
                            selected.push(ix);
                        }
                        if el.attrs.get("jadix_command_disabled").is_some() {
                            assert_eq!(el.attrs["aria-disabled"], "true");
                            assert_eq!(el.attrs["aria-selected"], "false");
                        }
                    }
                });
                assert!(found_input && found_list);
                assert_eq!(selected, expected.into_iter().collect::<Vec<_>>());
            };
            // Input is authored before rows; paint resolves the completed registry.
            sync(&mut root, &mut layout);
            inspect(&root, None, true);
            set_command_active(active, 2);
            sync(&mut root, &mut layout);
            inspect(&root, Some(2), true);
            set_command_active(active, 1);
            sync(&mut root, &mut layout);
            inspect(&root, None, true);
            query.set("Charlie".into());
            root = paint();
            set_command_active(active, 0);
            sync(&mut root, &mut layout);
            inspect(&root, Some(0), true);
            open.set(false);
            sync(&mut root, &mut layout);
            inspect(&root, Some(0), false);
            open.set(true);
            query.set("no match".into());
            root = paint();
            sync(&mut root, &mut layout);
            inspect(&root, None, true);
        });
    }
}

#[test]
fn command_item_keywords_keep_matching_item_visible() {
    let (_owner, root) = create_root(|| {
        let id = "cmd-keyword";
        use_signal(
            input_value_signal_key(&format!("{id}__filter")).as_str(),
            String::new,
        )
        .set("PREFER".into());

        command_paint(
            id.into(),
            Signal::new(true),
            "Search".into(),
            CommandSurface::Inline,
            Rc::new(|| {
                CommandItem(CommandItemProps {
                    value: Some("settings".into()),
                    disabled: None,
                    checked: None,
                    keywords: Some(vec!["preferences".into()]),
                    on_click: None,
                    children: Some(Element::new().text("Settings")),
                    base: None,
                })
            }),
        )
    });

    let mut visible = false;
    walk_cmd(&root, &mut |el| {
        visible |= el.attrs.get("data-slot").map(String::as_str) == Some("command-item");
    });
    assert!(
        visible,
        "keyword-matched CommandItem should remain rendered"
    );
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
                keywords: None,
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
                keywords: None,
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
fn command_query_shares_reactive_input_edits_and_escape_reset() {
    use crate::app::test_support::sync_reactive_elements_subtree;
    use crate::core::reactivity::create_effect;
    use winit::event::{ElementState, KeyEvent};
    use winit::keyboard::{KeyCode, KeyLocation, PhysicalKey};

    for surface in [
        CommandSurface::Inline,
        CommandSurface::Dialog,
        CommandSurface::Overlay,
    ] {
        let (_owner, ()) = create_root(|| {
            let id = format!("cmd-query-{surface:?}");
            // Exercise the facade export, including a query seeded before mount.
            let query = crate::components::command_query(&id);
            query.set("Beta");
            let observed = Rc::new(RefCell::new(Vec::new()));
            create_effect({
                let query = query.clone();
                let observed = observed.clone();
                move || observed.borrow_mut().push(query.get())
            });
            let open = Signal::new(true);
            let body = three_item_command_body();
            let rendered = Rc::new(RefCell::new(Element::new()));
            create_effect({
                let id = id.clone();
                let rendered = rendered.clone();
                move || {
                    *rendered.borrow_mut() =
                        command_paint(id.clone(), open, "Search".into(), surface, body.clone())
                }
            });
            let mut layout = LayoutTree::new();
            let sync = |root: &mut Element, layout: &mut LayoutTree| {
                sync_reactive_elements_subtree(root, layout, &mut LayoutStatus::Clean);
                layout.build_and_layout(root, 800.0, 600.0, [-1.0, -1.0]);
            };
            let visible = use_signal(
                &format!("{id}__cmd_visible"),
                Vec::<CommandVisibleItem>::new,
            );
            sync(&mut rendered.borrow_mut(), &mut layout);
            assert_eq!(visible.peek()[0].label, "Beta");
            assert_eq!(visible.peek().len(), 1);
            // Recreating a handle must address the same store and rebuild filtering.
            command_query(&id).set("Gamma");
            sync(&mut rendered.borrow_mut(), &mut layout);
            assert_eq!(query.get(), "Gamma");
            assert_eq!(visible.peek()[0].label, "Gamma");
            assert_eq!(visible.peek().len(), 1);
            // This is the same commit entry used by Input editing, not a shadow signal.
            crate::utils::text_field::commit_value(&format!("{id}__filter"), "Alpha".into());
            sync(&mut rendered.borrow_mut(), &mut layout);
            assert_eq!(query.get(), "Alpha");
            assert_eq!(visible.peek()[0].label, "Alpha");
            let mut handler = None;
            walk_cmd(&rendered.borrow(), &mut |el| {
                if el.focus_id.as_deref() == Some(input_focus_id(&format!("{id}__filter")).as_str())
                {
                    handler = el.on_key_down.clone();
                }
            });
            assert!((handler.expect("mounted CommandInput").borrow_mut())(
                KeyEvent {
                    physical_key: PhysicalKey::Code(KeyCode::Escape),
                    logical_key: Key::Named(NamedKey::Escape),
                    text: None,
                    location: KeyLocation::Standard,
                    state: ElementState::Pressed,
                    repeat: false,
                    text_with_all_modifiers: None,
                    key_without_modifiers: Key::Named(NamedKey::Escape),
                }
            ));
            sync(&mut rendered.borrow_mut(), &mut layout);
            assert_eq!(query.get(), "");
            assert!(open.peek(), "first Escape clears without dismissing");
            assert_eq!(visible.peek().len(), 3);
            assert_eq!(*observed.borrow(), ["Beta", "Gamma", "Alpha", ""]);
        });
    }
}

#[test]
fn command_pointer_motion_preserves_keyboard_highlight_across_surfaces() {
    use crate::app::pointer_events::{
        HitPathEntry, MouseButton, PointerEvent, PointerKind, dispatch_pointer,
    };
    use crate::app::scroll_support::resolve_pending_descendant_bounds;
    use winit::event::{ElementState, KeyEvent};
    use winit::keyboard::{KeyCode, KeyLocation, PhysicalKey};

    for surface in [
        CommandSurface::Inline,
        CommandSurface::Dialog,
        CommandSurface::Overlay,
    ] {
        let _owner = create_root(|| {
            let id = format!("cmd-pointer-{surface:?}");
            let active = use_signal(&format!("{id}__cmd_active"), || NO_ACTIVE_COMMAND);
            let open = Signal::new(true);
            let body: Rc<dyn Fn() -> Element> = Rc::new(|| {
                let items = (0..4).map(|index| {
                    CommandItem(CommandItemProps {
                        value: Some(format!("item-{index}")),
                        disabled: Some(index == 2),
                        checked: None,
                        keywords: None,
                        on_click: None,
                        children: Some(Element::new().text(format!("Item {index}"))),
                        base: None,
                    })
                });
                let mut fragment = Element::new();
                fragment.set_attr("jadix_fragment", "1");
                let list = CommandList(CommandListProps {
                    children: Some(fragment.children(items.collect::<Vec<_>>())),
                    base: None,
                });
                let input = CommandInput(CommandInputProps {
                    placeholder: None,
                    children: None,
                    base: None,
                });
                let mut fragment = Element::new();
                fragment.set_attr("jadix_fragment", "1");
                fragment.children(vec![input, list])
            });
            let paint = || command_paint(id.clone(), open, "Search".into(), surface, body.clone());
            let mut root = paint();
            let mut tree = LayoutTree::new();
            tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
            let items = collect_cmd_items(&root);
            let pointer = |index: usize, kind, x, buttons| {
                let path = [HitPathEntry::from_element(
                    items[index],
                    [0.0, 0.0, 100.0, 32.0],
                )];
                let mut event = PointerEvent::new(x, 16.0, MouseButton::Left, 0);
                event.buttons = buttons;
                dispatch_pointer(&path, &mut event, kind);
            };
            let hover = |index: usize, on| {
                if let Some(handler) = &items[index].on_hover {
                    (handler.borrow_mut())(on);
                }
            };
            assert_eq!(active.peek(), NO_ACTIVE_COMMAND);
            hover(0, true);
            assert_eq!(
                active.peek(),
                NO_ACTIVE_COMMAND,
                "layout hover is not pointer motion"
            );
            pointer(0, PointerKind::Move, 10.0, 0);
            assert_eq!(active.peek(), 0);
            let mut key_handler = None;
            walk_cmd(&root, &mut |el| {
                if el.focus_id.as_deref() == Some(format!("input:{id}__filter").as_str()) {
                    key_handler = el.on_key_down.clone();
                }
            });
            let key_handler = key_handler.unwrap();
            for expected in [1, 3] {
                assert!((key_handler.borrow_mut())(KeyEvent {
                    physical_key: PhysicalKey::Code(KeyCode::ArrowDown),
                    logical_key: Key::Named(NamedKey::ArrowDown),
                    text: None,
                    location: KeyLocation::Standard,
                    state: ElementState::Pressed,
                    repeat: false,
                    text_with_all_modifiers: None,
                    key_without_modifiers: Key::Named(NamedKey::ArrowDown),
                }));
                assert_eq!(active.peek(), expected);
            }
            assert!(!resolve_pending_descendant_bounds(&root).is_empty());
            hover(0, false);
            hover(3, true);
            pointer(3, PointerKind::Leave, 10.0, 0);
            pointer(0, PointerKind::Move, 10.0, 0);
            assert_eq!(
                active.peek(),
                3,
                "stationary pointer must preserve keyboard selection"
            );
            pointer(0, PointerKind::Move, 11.0, 1);
            pointer(2, PointerKind::Move, 12.0, 0);
            pointer(0, PointerKind::Move, 12.0, 0);
            assert_eq!(active.peek(), 3, "drag and disabled rows cannot select");
            pointer(0, PointerKind::Move, 13.0, 0);
            assert_eq!(active.peek(), 0, "actual pointer motion retakes selection");
            assert!(
                resolve_pending_descendant_bounds(&root).is_empty(),
                "pointer takeover cancels pending keyboard reveal"
            );
            pointer(0, PointerKind::Leave, 14.0, 0);
            assert_eq!(
                active.peek(),
                NO_ACTIVE_COMMAND,
                "actual pointer leave clears highlight"
            );
            pointer(1, PointerKind::Move, 14.0, 0);
            assert_eq!(
                active.peek(),
                1,
                "leave cannot consume the next row's move sample"
            );
            let rebuilt = paint();
            let items = collect_cmd_items(&rebuilt);
            let path = [HitPathEntry::from_element(
                items[0],
                [0.0, 0.0, 100.0, 32.0],
            )];
            let mut event = PointerEvent::new(14.0, 16.0, MouseButton::Left, 0);
            dispatch_pointer(&path, &mut event, PointerKind::Move);
            assert_eq!(active.peek(), 1, "rebuild preserves last pointer position");
            reset_command_session(&id);
            dispatch_pointer(&path, &mut event, PointerKind::Move);
            assert_eq!(
                active.peek(),
                0,
                "new session accepts its first pointer sample"
            );
        });
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
fn command_keyboard_skips_disabled_items_across_surfaces_and_filter_changes() {
    use std::cell::Cell;
    use winit::event::{ElementState, KeyEvent};
    use winit::keyboard::{KeyCode, KeyLocation, PhysicalKey};

    for (name, surface) in [
        ("inline", CommandSurface::Inline),
        ("dialog", CommandSurface::Dialog),
        ("overlay", CommandSurface::Overlay),
    ] {
        let _owner = create_root(|| {
            let id = format!("cmd-disabled-nav-{name}");
            let filter_id = format!("{id}__filter");
            let active = use_signal(&format!("{id}__cmd_active"), || NO_ACTIVE_COMMAND);
            let query = use_signal(&input_value_signal_key(&filter_id), String::new);
            let disabled = Signal::new(vec![true, false, true, false, true]);
            let open = Signal::new(true);
            let activated = Rc::new(Cell::new(None));
            let body: Rc<dyn Fn() -> Element> = Rc::new({
                let activated = activated.clone();
                move || {
                    let items: Vec<Element> = disabled
                        .peek()
                        .into_iter()
                        .enumerate()
                        .map(|(index, disabled)| {
                            let activated = activated.clone();
                            CommandItem(CommandItemProps {
                                value: Some(format!("item-{index}")),
                                disabled: Some(disabled),
                                checked: None,
                                keywords: None,
                                on_click: Some(Handler::new(move |_| {
                                    assert!(open.peek(), "activation must precede dismissal");
                                    activated.set(Some(index));
                                })),
                                children: Some(Element::new().text(format!("Item {index}"))),
                                base: None,
                            })
                        })
                        .collect();
                    let mut list = Element::new();
                    list.set_attr("jadix_fragment", "1");
                    let mut root = Element::new();
                    root.set_attr("jadix_fragment", "1");
                    root.children(vec![
                        CommandInput(CommandInputProps {
                            placeholder: None,
                            children: None,
                            base: None,
                        }),
                        CommandList(CommandListProps {
                            children: Some(list.children(items)),
                            base: None,
                        }),
                    ])
                }
            });
            let paint = || command_paint(id.clone(), open, "Search".into(), surface, body.clone());
            let dispatch = |root: &Element, key: NamedKey, state: ElementState| {
                let mut handler = None;
                walk_cmd(root, &mut |el| {
                    if el.focus_id.as_deref() == Some(format!("input:{filter_id}").as_str()) {
                        handler = el.on_key_down.clone();
                    }
                });
                (handler.unwrap().borrow_mut())(KeyEvent {
                    physical_key: PhysicalKey::Code(KeyCode::Enter),
                    logical_key: Key::Named(key),
                    text: None,
                    location: KeyLocation::Standard,
                    state,
                    repeat: false,
                    text_with_all_modifiers: None,
                    key_without_modifiers: Key::Named(key),
                })
            };
            let press = |root: &Element, key: NamedKey| dispatch(root, key, ElementState::Pressed);
            let root = paint();
            assert_eq!(active.peek(), NO_ACTIVE_COMMAND);
            for (key, expected) in [
                (NamedKey::ArrowDown, 1),
                (NamedKey::ArrowDown, 3),
                (NamedKey::ArrowDown, 1),
                (NamedKey::ArrowUp, 3),
            ] {
                assert!(press(&root, key));
                assert_eq!(active.peek(), expected, "{name}: {key:?}");
            }
            set_command_active(active, NO_ACTIVE_COMMAND);
            assert!(press(&root, NamedKey::ArrowUp));
            assert_eq!(active.peek(), 3, "first Up starts at last enabled item");

            query.set("Item 1".into());
            let _ = paint();
            assert_eq!(active.peek(), 0, "filter clamps to an enabled visible item");
            query.set("Item 2".into());
            let root = paint();
            assert_eq!(
                active.peek(),
                NO_ACTIVE_COMMAND,
                "disabled replacement clears highlight"
            );
            assert!(press(&root, NamedKey::ArrowDown));
            assert_eq!(active.peek(), NO_ACTIVE_COMMAND);
            press(&root, NamedKey::Enter);
            assert_eq!(activated.get(), None);
            assert!(open.peek());

            query.set("missing".into());
            let root = paint();
            assert!(press(&root, NamedKey::ArrowUp));
            assert_eq!(active.peek(), NO_ACTIVE_COMMAND);
            query.set(String::new());
            let root = paint();
            assert!(press(&root, NamedKey::ArrowDown));
            disabled.set(vec![true; 5]);
            let root = paint();
            assert_eq!(
                active.peek(),
                NO_ACTIVE_COMMAND,
                "newly disabled active item clears"
            );
            assert!(press(&root, NamedKey::ArrowUp));
            assert_eq!(active.peek(), NO_ACTIVE_COMMAND);

            disabled.set(vec![true, false, true, false, true]);
            query.set("Item 1".into());
            let root = paint();
            let caret = use_signal(&input_caret_signal_key(&filter_id), || None::<usize>);
            let selection = use_signal(&input_selection_signal_key(&filter_id), || {
                None::<(usize, usize)>
            });
            caret.set(Some(6));
            selection.set(Some((0, 6)));
            use_signal("__jadix_button_focus_id", String::new).set(format!("input:{filter_id}"));
            assert!(press(&root, NamedKey::Escape));
            assert_eq!(query.peek(), "", "first Escape clears query on {name}");
            assert!(open.peek());
            assert!(!dispatch(&root, NamedKey::Escape, ElementState::Released));
            assert!(open.peek(), "Escape release must not dismiss on {name}");
            assert_eq!(caret.peek(), None);
            assert_eq!(selection.peek(), None);
            assert_eq!(active.peek(), NO_ACTIVE_COMMAND);
            assert_eq!(
                use_signal("__jadix_button_focus_id", String::new).peek(),
                format!("input:{filter_id}")
            );
            let root = paint();
            assert_eq!(press(&root, NamedKey::Escape), surface.dismissible());
            assert_eq!(open.peek(), !surface.dismissible());
            open.set(true);
            let root = paint();
            assert!(press(&root, NamedKey::ArrowDown));
            assert!(press(&root, NamedKey::Enter));
            assert_eq!(activated.get(), Some(1));
            assert_eq!(open.peek(), !surface.dismissible());
        });
    }
}

#[test]
fn command_keyboard_reveals_variable_height_rows_across_surfaces() {
    use crate::app::scroll_support::{
        apply_pending_scroll_align, resolve_pending_descendant_bounds,
    };
    use crate::core::scroll_viewport::{scroll_offset_y_signal_key, scroll_target_y_signal_key};
    use winit::event::{ElementState, KeyEvent};
    use winit::keyboard::{KeyCode, KeyLocation, PhysicalKey};

    for surface in [
        CommandSurface::Inline,
        CommandSurface::Dialog,
        CommandSurface::Overlay,
    ] {
        let _owner = create_root(|| {
            let id = format!("cmd-reveal-{surface:?}");
            let rest = format!("{id}:list");
            let filter_id = input_focus_id(&format!("{id}__filter"));
            let active = use_signal(&format!("{id}__cmd_active"), || NO_ACTIVE_COMMAND);
            let body: Rc<dyn Fn() -> Element> = Rc::new(|| {
                let items: Vec<_> = (0..18)
                    .map(|index| {
                        CommandItem(CommandItemProps {
                            value: Some(format!("item-{index}")),
                            disabled: Some(matches!(index, 0 | 7 | 17)),
                            checked: None,
                            keywords: None,
                            on_click: None,
                            children: Some(
                                Element::new()
                                    .text(format!("Item {index}"))
                                    .h(if index % 3 == 0 { 48.0 } else { 16.0 }),
                            ),
                            base: None,
                        })
                    })
                    .collect();
                let fragment = || {
                    let mut element = Element::new();
                    element.set_attr("jadix_fragment", "1");
                    element
                };
                let group = CommandGroup(CommandGroupProps {
                    heading: Some("Commands".into()),
                    children: Some(fragment().children(items)),
                    base: None,
                });
                fragment().children(vec![
                    CommandInput(CommandInputProps {
                        placeholder: None,
                        children: None,
                        base: None,
                    }),
                    CommandList(CommandListProps {
                        children: Some(fragment().child(group)),
                        base: None,
                    }),
                ])
            });
            let open = Signal::new(true);
            let root = command_paint(id.clone(), open, "Search".into(), surface, body.clone());
            let mut host = Element::new().w(800.0).h(600.0).child(root);
            let mut tree = LayoutTree::new();
            let result = tree.build_and_layout(&mut host, 800.0, 600.0, [-1.0, -1.0]);
            let (_, viewport, _, content) = result
                .scroll_metrics
                .iter()
                .find(|(key, ..)| key == &rest)
                .expect("Command scroll metrics");
            let height_key =
                crate::core::scroll_viewport::ScrollViewportSignalKeys::new(&rest).viewport_h;
            use_signal(&height_key, || 0.0_f32).set_silent(viewport[3]);
            let mut impact = crate::core::tree::LayoutStatus::Clean;
            sync_command_item_paint_subtree(&mut host, &mut tree, &mut impact);
            assert!(content[1] > viewport[3] + 100.0);
            let mut handler = None;
            let mut rows = Vec::new();
            walk_cmd(&host, &mut |el| {
                if el.focus_id.as_deref() == Some(filter_id.as_str()) {
                    handler = el.on_key_down.clone();
                }
                if el.attrs.get("data-slot").map(String::as_str) == Some("command-item") {
                    rows.push(el.layout_bounds);
                }
            });
            assert!(
                rows.windows(2)
                    .any(|pair| (pair[0][3] - pair[1][3]).abs() > 10.0)
            );
            let handler = handler.unwrap();
            let press = |key| {
                assert!((handler.borrow_mut())(KeyEvent {
                    physical_key: PhysicalKey::Code(KeyCode::ArrowDown),
                    logical_key: Key::Named(key),
                    text: None,
                    location: KeyLocation::Standard,
                    state: ElementState::Pressed,
                    repeat: false,
                    text_with_all_modifiers: None,
                    key_without_modifiers: Key::Named(key),
                }));
            };
            let offset = use_signal(&scroll_offset_y_signal_key(&rest), || 0.0_f32);
            let target = use_signal(&scroll_target_y_signal_key(&rest), || 0.0_f32);
            // Initial Up and both wrap directions, then every enabled row back to the top.
            for key in [NamedKey::ArrowUp, NamedKey::ArrowDown, NamedKey::ArrowUp]
                .into_iter()
                .chain(std::iter::repeat_n(NamedKey::ArrowUp, 14))
            {
                let previous = offset.peek();
                press(key);
                let index = active.peek();
                assert!(!matches!(index, 0 | 7 | 17));
                let row = rows[index];
                let bounds = resolve_pending_descendant_bounds(&host);
                assert!(
                    apply_pending_scroll_align(
                        &rest,
                        *viewport,
                        content[0],
                        content[1],
                        bounds.get(&rest).copied(),
                    ),
                    "{surface:?}: keyboard navigation must schedule its actual row"
                );
                let destination = target.peek();
                assert_eq!(
                    offset.peek(),
                    previous,
                    "default smooth scrolling must not snap"
                );
                let top = row[1] - destination;
                assert!(
                    top >= viewport[1] - 0.5 && top + row[3] <= viewport[1] + viewport[3] + 0.5,
                    "{surface:?}: active row {index} must be fully revealed"
                );
                if row[1] - previous >= viewport[1]
                    && row[1] + row[3] - previous <= viewport[1] + viewport[3]
                {
                    assert_eq!(destination, previous, "visible rows must not recenter");
                }
                // Settle the shared smooth chase; retain the same mounted tree.
                offset.set_silent(destination);
                crate::core::tree::walk_elements_mut_pruned(&mut host, |el| {
                    if el.focus_id.as_deref()
                        == Some(
                            crate::core::scroll_viewport::focus_id_for_scroll_viewport(&rest)
                                .as_str(),
                        )
                    {
                        el.scroll_offset[1] = destination;
                    }
                    true
                });
            }
            assert_eq!(active.peek(), 1);
            press(NamedKey::PageDown);
            assert!(
                (3..16).contains(&active.peek()),
                "a measured page must move beyond one row"
            );
            assert!(resolve_pending_descendant_bounds(&host).contains_key(&rest));
            for _ in 0..3 {
                press(NamedKey::PageDown);
                assert!(!matches!(active.peek(), 0 | 7 | 17));
            }
            assert_eq!(active.peek(), 16, "PageDown clamps to the last enabled row");
            press(NamedKey::PageDown);
            assert_eq!(active.peek(), 16, "PageDown must not wrap");
            press(NamedKey::PageUp);
            assert!(
                (2..15).contains(&active.peek()),
                "PageUp uses the same measured viewport"
            );
            for _ in 0..3 {
                press(NamedKey::PageUp);
                assert!(!matches!(active.peek(), 0 | 7 | 17));
            }
            assert_eq!(active.peek(), 1, "PageUp clamps to the first enabled row");
            press(NamedKey::PageUp);
            assert_eq!(active.peek(), 1, "PageUp must not wrap");
            press(NamedKey::ArrowUp);
            assert!(resolve_pending_descendant_bounds(&host).contains_key(&rest));
            reset_command_session(&id);
            assert!(
                !resolve_pending_descendant_bounds(&host).contains_key(&rest),
                "session reset must cancel the outgoing reveal"
            );
            press(NamedKey::ArrowUp);
            assert!(resolve_pending_descendant_bounds(&host).contains_key(&rest));
            let _ = command_paint(id.clone(), open, "Search".into(), surface, body);
            assert!(
                !resolve_pending_descendant_bounds(&host).contains_key(&rest),
                "list rebuild must discard targets from the previous visible index"
            );
            if surface.dismissible() {
                press(NamedKey::ArrowDown);
                assert!(resolve_pending_descendant_bounds(&host).contains_key(&rest));
                press(NamedKey::Escape);
                assert!(!open.peek());
                assert!(
                    !resolve_pending_descendant_bounds(&host).contains_key(&rest),
                    "close must cancel reveal before the exit animation finishes"
                );
            }
        });
    }
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
            keywords: None,
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
                keywords: None,
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
                keywords: None,
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
                keywords: None,
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
    use crate::utils::modal_shell::{ShellOverlayOpts, default_modal_overlay_color};

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
    use crate::utils::modal_shell::{ShellOverlayOpts, default_modal_overlay_color};

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
                        keywords: None,
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
                    keywords: None,
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
            item_hover = el.on_mousemove.is_some();
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
                    keywords: None,
                    on_click: None,
                    children: Some(Element::new().text("Alpha")),
                    base: None,
                });
                let b = CommandItem(CommandItemProps {
                    value: Some("b".into()),
                    disabled: None,
                    checked: None,
                    keywords: None,
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

#[test]
fn command_builder_virtualizes_grouped_rows_without_losing_offscreen_actions() {
    use crate::app::test_support::sync_reactive_elements_subtree;
    use crate::core::scroll_viewport::{scroll_offset_y_signal_key, scroll_target_y_signal_key};
    use winit::event::{ElementState, KeyEvent};
    use winit::keyboard::{KeyCode, KeyLocation, PhysicalKey};

    for surface in [
        CommandSurface::Inline,
        CommandSurface::Dialog,
        CommandSurface::Overlay,
    ] {
        let (_owner, ()) = create_root(|| {
            let id = format!("cmd-virtual-{surface:?}");
            let activated = Rc::new(std::cell::Cell::new(None));
            let actions = |range: std::ops::Range<usize>| {
                range
                    .map(|i| {
                        let activated = activated.clone();
                        CommandAction::new(format!("item-{i}"), format!("Item {i}"), move || {
                            activated.set(Some(i))
                        })
                        .keywords([if i == 4999 { "ÉCOLE" } else { "ordinary" }])
                        .disabled(i % 7 == 0)
                    })
                    .collect()
            };
            let entries = Rc::new(vec![
                CommandEntry::Separator,
                CommandEntry::Group(CommandGroupData {
                    heading: "First".into(),
                    items: actions(0..2500),
                }),
                CommandEntry::Separator,
                CommandEntry::Separator,
                CommandEntry::Group(CommandGroupData::new("Empty group")),
                CommandEntry::Group(CommandGroupData {
                    heading: "Second".into(),
                    items: actions(2500..5000),
                }),
                CommandEntry::Separator,
            ]);
            let body: Rc<dyn Fn() -> Element> =
                Rc::new(move || builder_palette_body("Search".into(), entries.clone()));
            let open = Signal::new(true);
            let active = use_signal(&format!("{id}__cmd_active"), || NO_ACTIVE_COMMAND);
            let visible = use_signal(
                &format!("{id}__cmd_visible"),
                Vec::<CommandVisibleItem>::new,
            );
            let query = command_query(&id);
            let paint = || command_paint(id.clone(), open, "Search".into(), surface, body.clone());
            let mounted = |root: &Element| {
                let mut indices = Vec::new();
                walk_cmd(root, &mut |el| {
                    if el.attrs.get("data-slot").map(String::as_str) == Some("command-item") {
                        indices.push(el.attrs["jadix_command_ix"].parse::<usize>().unwrap());
                    }
                });
                indices
            };
            let press = |root: &Element, key| {
                let mut handler = None;
                walk_cmd(root, &mut |el| {
                    if el.focus_id.as_deref()
                        == Some(input_focus_id(&format!("{id}__filter")).as_str())
                    {
                        handler = el.on_key_down.clone();
                    }
                });
                assert!((handler.unwrap().borrow_mut())(KeyEvent {
                    physical_key: PhysicalKey::Code(KeyCode::ArrowDown),
                    logical_key: Key::Named(key),
                    text: None,
                    location: KeyLocation::Standard,
                    state: ElementState::Pressed,
                    repeat: false,
                    text_with_all_modifiers: None,
                    key_without_modifiers: Key::Named(key),
                }));
            };
            let mut host = Element::new().w(800.0).h(600.0).child(paint());
            assert_eq!(visible.peek().len(), 5000);
            assert_eq!(activate_registry::count(&id), 5000);
            assert!(
                (1..30).contains(&mounted(&host).len()),
                "build only a viewport window"
            );
            assert!(!mounted(&host).contains(&4999));
            let mut layout = LayoutTree::new();
            layout.build_and_layout(&mut host, 800.0, 600.0, [-1.0, -1.0]);
            sync_reactive_elements_subtree(&mut host, &mut layout, &mut LayoutStatus::Clean);
            fn stamp_tall_rows(el: &mut Element) {
                if el.attrs.contains_key("__jadix_vlist_row_key") {
                    el.layout_bounds[3] = 100.0;
                }
                for child in &mut el.children {
                    stamp_tall_rows(child);
                }
            }
            stamp_tall_rows(&mut host);
            sync_reactive_elements_subtree(&mut host, &mut layout, &mut LayoutStatus::Clean);
            assert_eq!(command_page_enabled(&id, 1, 200.0, true), Some(3));
            assert_eq!(command_page_enabled(&id, 3, 200.0, false), Some(1));
            // Keyboard state and actions must cover data that has never been mounted.
            press(&host, NamedKey::ArrowUp);
            assert_eq!(active.peek(), 4999);
            press(&host, NamedKey::Enter);
            assert_eq!(activated.get(), Some(4999));
            assert_eq!(open.peek(), !surface.dismissible());
            open.set(true);
            for _ in 0..3 {
                sync_reactive_elements_subtree(&mut host, &mut layout, &mut LayoutStatus::Clean);
                let result = layout.build_and_layout(&mut host, 800.0, 600.0, [-1.0, -1.0]);
                // AppState publishes collected metrics between frames. This headless
                // harness must do the same before consuming deferred navigation.
                for (rest, bounds, _, content) in result.scroll_metrics {
                    use crate::core::scroll_viewport::ScrollViewportSignalKeys;
                    let keys = ScrollViewportSignalKeys::new(&rest);
                    for (key, value) in [
                        (&keys.viewport_w, bounds[2]),
                        (&keys.viewport_h, bounds[3]),
                        (&keys.content_w, content[0]),
                        (&keys.content_h, content[1]),
                    ] {
                        use_signal(key, || 0.0_f32).set_silent(value);
                    }
                }
            }
            assert!(
                mounted(&host).contains(&4999),
                "the focused offscreen row must mount"
            );
            assert!(mounted(&host).len() < 30);
            crate::authoring::presentation::sync_tree(
                &mut host,
                &mut layout,
                crate::authoring::presentation::Context::new(
                    crate::authoring::presentation::Phase::State,
                    [800.0, 600.0],
                ),
            );
            walk_cmd(&host, &mut |el| {
                if el.attrs.get("data-slot").map(String::as_str) == Some("command-list") {
                    assert_eq!(el.attrs["accesskit_role"], "listbox");
                    assert_eq!(el.attrs["id"], format!("{id}:listbox"));
                    assert_eq!(el.attrs["accesskit_size_of_set"], "5000");
                }
                if el.focus_id.as_deref() == Some(input_focus_id(&format!("{id}__filter")).as_str())
                {
                    assert_eq!(el.attrs["aria-activedescendant"], format!("{id}:item:4999"));
                }
                if el.attrs.get("jadix_command_ix").map(String::as_str) == Some("4999") {
                    assert_eq!(el.attrs["id"], format!("{id}:item:4999"));
                    assert_eq!(el.attrs["aria-selected"], "true");
                    assert_eq!(el.attrs["aria-posinset"], "5000");
                    assert_eq!(el.attrs["aria-setsize"], "5000");
                }
            });
            #[cfg(feature = "a11y")]
            {
                let update = crate::a11y::bridge::tree_update_from_elements(&host, None);
                assert!(
                    update
                        .nodes
                        .iter()
                        .any(|(_, node)| node.role() == accesskit::Role::ListBox
                            && node.size_of_set() == Some(5000))
                );
                assert!(
                    update
                        .nodes
                        .iter()
                        .any(|(_, node)| node.role() == accesskit::Role::ListBoxOption
                            && node.position_in_set() == Some(4999))
                );
            }
            assert!(
                use_signal(&scroll_target_y_signal_key(&format!("{id}:list")), || {
                    0.0_f32
                })
                .peek()
                    > 1000.0
            );
            // A stable keyboard highlight must not pull wheel/drag scrolling back.
            use_signal(&scroll_offset_y_signal_key(&format!("{id}:list")), || {
                0.0_f32
            })
            .set_silent(0.0);
            use_signal(&scroll_target_y_signal_key(&format!("{id}:list")), || {
                0.0_f32
            })
            .set_silent(0.0);
            sync_reactive_elements_subtree(&mut host, &mut layout, &mut LayoutStatus::Clean);
            assert!(!mounted(&host).contains(&4999));
            assert!(mounted(&host).len() < 30);
            press(&host, NamedKey::PageDown);
            assert_eq!(active.peek(), 4999, "paging clamps instead of wrapping");
            press(&host, NamedKey::PageUp);
            assert!((4980..4999).contains(&active.peek()));
            assert_ne!(active.peek() % 7, 0, "paging skips disabled commands");
            press(&host, NamedKey::ArrowDown);
            assert_ne!(active.peek() % 7, 0);

            query.set("école");
            let filtered = paint();
            assert_eq!(visible.peek().len(), 1);
            assert_eq!(visible.peek()[0].value, "item-4999");
            assert_eq!(mounted(&filtered), vec![0]);
            let mut texts = Vec::new();
            walk_cmd(&filtered, &mut |el| {
                if let Some(text) = &el.text {
                    texts.push(text.clone());
                    if text == "ÉCOLE" {
                        assert_eq!(
                            el.text_spans
                                .as_ref()
                                .unwrap()
                                .iter()
                                .filter(|span| span.color.is_some())
                                .map(|span| span.text.as_str())
                                .collect::<Vec<_>>(),
                            ["ÉCOLE"]
                        );
                    }
                }
            });
            assert!(texts.contains(&"Second".to_owned()));
            assert!(!texts.contains(&"First".to_owned()));
            assert!(!texts.contains(&"Empty group".to_owned()));
            activated.set(None);
            press(&filtered, NamedKey::ArrowDown);
            press(&filtered, NamedKey::Enter);
            assert_eq!(
                activated.get(),
                Some(4999),
                "filtered indices must map to original callbacks"
            );
            query.set("no-such-command");
            let empty = paint();
            assert!(visible.peek().is_empty());
            assert!(mounted(&empty).is_empty());
            assert_eq!(active.peek(), NO_ACTIVE_COMMAND);
            let message = use_signal(&format!("{id}__cmd_empty"), String::new).peek();
            let mut found_empty = false;
            walk_cmd(&empty, &mut |el| {
                found_empty |= el.text.as_deref() == Some(message.as_str())
            });
            assert!(found_empty);
        });
    }
}

#[test]
fn command_external_results_and_loading_share_ordinary_and_virtual_lists() {
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::app::test_support::sync_reactive_elements_subtree;
    use crate::core::reactivity::{create_effect, mark_next_effect_as_ui_root};
    for count in [3, 100] {
        let (_owner, ()) = create_root(|| {
            let id = format!("cmd-external-{count}");
            let busy = Signal::new(true);
            let results = Signal::new(Vec::<usize>::new());
            let settings = command(&id).filterable(false).loading_signal(busy);
            provide_context(command_behavior_context(), settings.behavior);
            command_query(&id).set("server-ranked-query-with-no-local-match");
            let open = Signal::new(true);
            let rendered = Rc::new(RefCell::new(Element::new()));
            install_test_reconcile_root(rendered.clone());
            mark_next_effect_as_ui_root();
            create_effect({
                let id = id.clone();
                let rendered = rendered.clone();
                move || {
                    *rendered.borrow_mut() = command(&id)
                        .inline()
                        .open_signal(open)
                        .filterable(false)
                        .loading_signal(busy)
                        .child(move || {
                            let entries = Rc::new(
                                results
                                    .get()
                                    .into_iter()
                                    .map(|i| {
                                        CommandEntry::Item(CommandAction::new(
                                            i.to_string(),
                                            format!("Result {i}"),
                                            || {},
                                        ))
                                    })
                                    .collect(),
                            );
                            builder_palette_body("Search".into(), entries)
                        });
                }
            });
            let mut layout = LayoutTree::new();
            let mut inspect = |busy: bool, loading: bool| {
                sync_reactive_elements_subtree(
                    &mut rendered.borrow_mut(),
                    &mut layout,
                    &mut LayoutStatus::Clean,
                );
                crate::authoring::presentation::sync_tree(
                    &mut rendered.borrow_mut(),
                    &mut layout,
                    crate::authoring::presentation::Context::new(
                        crate::authoring::presentation::Phase::State,
                        [800.0, 600.0],
                    ),
                );
                let mut lists = 0;
                let mut statuses = 0;
                let mut empty_text = false;
                let message = use_signal(&format!("{id}__cmd_empty"), String::new).peek();
                walk_cmd(&rendered.borrow(), &mut |el| {
                    if el.attrs.get("data-slot").map(String::as_str) == Some("command-list") {
                        lists += 1;
                        assert_eq!(el.attrs["aria-busy"], busy.to_string(), "{count} results");
                    }
                    if el.attrs.get("data-slot").map(String::as_str) == Some("command-loading") {
                        statuses += 1;
                        assert_eq!(el.attrs["accesskit_role"], "status");
                    }
                    empty_text |= el.text.as_deref() == Some(message.as_str());
                });
                assert_eq!(lists, 1);
                assert_eq!(statuses, usize::from(loading));
                assert_eq!(empty_text, !busy && results.peek().is_empty());
            };
            inspect(true, true);
            results.set((0..count).rev().collect());
            inspect(true, false);
            let visible = use_signal(
                &format!("{id}__cmd_visible"),
                Vec::<CommandVisibleItem>::new,
            )
            .peek();
            assert_eq!(visible.len(), count);
            assert_eq!(
                visible[0].value,
                (count - 1).to_string(),
                "preserve external ranking"
            );
            busy.set(false);
            inspect(false, false);
            results.set(Vec::new());
            inspect(false, false);
            // Compound rows use the same bypass, including the noncached fallback.
            let body = three_item_command_body();
            let _ = command_paint(
                id.clone(),
                open,
                "Search".into(),
                CommandSurface::Inline,
                body,
            );
            assert_eq!(
                use_signal(
                    &format!("{id}__cmd_visible"),
                    Vec::<CommandVisibleItem>::new
                )
                .peek()
                .len(),
                3
            );
            clear_test_reconcile_root();
        });
    }
}

#[test]
fn command_searchless_keyboard_reveals_and_activates_enabled_results() {
    use crate::app::test_support::sync_reactive_elements_subtree;
    use winit::event::{ElementState, KeyEvent};
    use winit::keyboard::{KeyCode, KeyLocation, PhysicalKey};
    for count in [4, 100] {
        for surface in [
            CommandSurface::Inline,
            CommandSurface::Dialog,
            CommandSurface::Overlay,
        ] {
            let (_owner, ()) = create_root(|| {
                let id = format!("cmd-searchless-{count}-{surface:?}");
                let settings = command(&id).searchable(false);
                provide_context(command_behavior_context(), settings.behavior);
                command_query(&id).set("hidden query must not filter or capture Escape");
                let selected = Rc::new(std::cell::Cell::new(None));
                let entries: Rc<Vec<CommandEntry>> = Rc::new(
                    (0..count)
                        .map(|i| {
                            let selected = selected.clone();
                            CommandEntry::Item(
                                CommandAction::new(
                                    i.to_string(),
                                    if i == count - 1 {
                                        "Zebra".into()
                                    } else if i == 2 {
                                        "Bravo".into()
                                    } else {
                                        format!("Alpha {i}")
                                    },
                                    move || selected.set(Some(i)),
                                )
                                .disabled(i == 0),
                            )
                        })
                        .collect(),
                );
                let body: Rc<dyn Fn() -> Element> =
                    Rc::new(move || builder_palette_body("Actions".into(), entries.clone()));
                let open = Signal::new(true);
                let active = use_signal(&format!("{id}__cmd_active"), || NO_ACTIVE_COMMAND);
                let mut root = command_paint(id.clone(), open, "Actions".into(), surface, body);
                let mut list_handler = None;
                let mut inputs = 0;
                walk_cmd(&root, &mut |el| {
                    inputs += usize::from(
                        el.attrs.get("accesskit_role").map(String::as_str) == Some("combobox"),
                    );
                    if el.focus_id.as_deref() == Some(format!("{id}:listbox").as_str()) {
                        assert!(el.focusable);
                        list_handler = el.on_key_down.clone();
                    }
                });
                assert_eq!(inputs, 0);
                let handler = list_handler.expect("focusable list");
                let press = |key: Key| {
                    let text = if let Key::Character(text) = &key {
                        Some(text.clone())
                    } else {
                        None
                    };
                    assert!((handler.borrow_mut())(KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::ArrowDown),
                        logical_key: key.clone(),
                        text: text.clone(),
                        location: KeyLocation::Standard,
                        state: ElementState::Pressed,
                        repeat: false,
                        text_with_all_modifiers: text,
                        key_without_modifiers: key,
                    }));
                };
                press(Key::Named(NamedKey::Home));
                assert_eq!(active.peek(), 1, "skip disabled first row");
                press(Key::Character("a".into()));
                assert_eq!(active.peek(), if count == 4 { 1 } else { 3 });
                press(Key::Named(NamedKey::Home));
                press(Key::Character("b".into()));
                press(Key::Character("r".into()));
                assert_eq!(
                    active.peek(),
                    2,
                    "multi-letter prefix includes current match"
                );
                press(Key::Named(NamedKey::End));
                assert_eq!(active.peek(), count - 1);
                let mut layout = LayoutTree::new();
                for _ in 0..3 {
                    layout.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
                    sync_reactive_elements_subtree(
                        &mut root,
                        &mut layout,
                        &mut LayoutStatus::Clean,
                    );
                }
                crate::authoring::presentation::sync_tree(
                    &mut root,
                    &mut layout,
                    crate::authoring::presentation::Context::new(
                        crate::authoring::presentation::Phase::State,
                        [800.0, 600.0],
                    ),
                );
                let mut mounted = false;
                walk_cmd(&root, &mut |el| {
                    if el.focus_id.as_deref() == Some(format!("{id}:listbox").as_str()) {
                        assert_eq!(
                            el.attrs["aria-activedescendant"],
                            format!("{id}:item:{}", count - 1)
                        );
                    }
                    mounted |= el.attrs.get("id") == Some(&format!("{id}:item:{}", count - 1));
                });
                assert!(mounted, "End must reveal the virtual row");
                press(Key::Named(NamedKey::Enter));
                assert_eq!(selected.get(), Some(count - 1));
                assert_eq!(open.peek(), !surface.dismissible());
                open.set(true);
                if surface.dismissible() {
                    press(Key::Named(NamedKey::Escape));
                    assert!(!open.peek(), "hidden query must not swallow dismissal");
                }
            });
        }
    }
}

#[test]
fn command_controlled_selection_and_confirmation_share_all_surfaces_and_virtual_rows() {
    use crate::app::test_support::sync_reactive_elements_subtree;
    use winit::event::{ElementState, KeyEvent};
    use winit::keyboard::{KeyCode, KeyLocation, PhysicalKey};

    let event = |key: Key| KeyEvent {
        physical_key: PhysicalKey::Code(KeyCode::Enter),
        logical_key: key.clone(),
        text: None,
        location: KeyLocation::Standard,
        state: ElementState::Pressed,
        repeat: false,
        text_with_all_modifiers: None,
        key_without_modifiers: key,
    };
    let sync = |root: &mut Element, layout: &mut LayoutTree| {
        for _ in 0..3 {
            layout.build_and_layout(root, 800.0, 600.0, [-1.0, -1.0]);
            sync_reactive_elements_subtree(root, layout, &mut LayoutStatus::Clean);
        }
        crate::authoring::presentation::sync_tree(
            root,
            layout,
            crate::authoring::presentation::Context::new(
                crate::authoring::presentation::Phase::State,
                [800.0, 600.0],
            ),
        );
    };
    for count in [4, 100] {
        for surface in [
            CommandSurface::Inline,
            CommandSurface::Dialog,
            CommandSurface::Overlay,
        ] {
            for searchable in [false, true] {
                let (_owner, ()) = create_root(|| {
                    let id = format!("cmd-multi-{count}-{surface:?}-{searchable}");
                    let selection = Signal::new(vec!["0".to_string(), "1".to_string()]);
                    let open = Signal::new(true);
                    let calls = Rc::new(RefCell::new(Vec::new()));
                    let entries = Rc::new(
                        (0..count)
                            .map(|i| {
                                let calls = calls.clone();
                                CommandEntry::Item(
                                    CommandAction::new(
                                        i.to_string(),
                                        format!("Item {i}"),
                                        move || {
                                            calls.borrow_mut().push((
                                                i,
                                                selection.peek(),
                                                open.peek(),
                                            ));
                                        },
                                    )
                                    .disabled(i == 0)
                                    .shortcut("Ctrl+K"),
                                )
                            })
                            .collect::<Vec<_>>(),
                    );
                    let builds = Rc::new(std::cell::Cell::new(0));
                    let body: Rc<dyn Fn() -> Element> = {
                        let builds = builds.clone();
                        Rc::new(move || {
                            builds.set(builds.get() + 1);
                            builder_palette_body("Select actions".into(), entries.clone())
                        })
                    };
                    let settings = command(&id)
                        .searchable(searchable)
                        .selection_signal(selection);
                    provide_context(command_behavior_context(), settings.behavior);
                    let mut root = command_paint(
                        id.clone(),
                        open,
                        "Select actions".into(),
                        surface,
                        body.clone(),
                    );
                    let handler_of = |root: &Element| {
                        let mut handler = None;
                        walk_cmd(root, &mut |el| {
                            if (searchable
                                && el.attrs.get("accesskit_role").map(String::as_str)
                                    == Some("combobox"))
                                || (!searchable
                                    && el.focus_id.as_deref()
                                        == Some(format!("{id}:listbox").as_str()))
                            {
                                handler = el.on_key_down.clone();
                            }
                        });
                        handler.expect("Command keyboard target")
                    };
                    let handler = handler_of(&root);
                    let mut layout = LayoutTree::new();
                    sync(&mut root, &mut layout);
                    let mut click_disabled = None;
                    let mut click_one = None;
                    walk_cmd(&root, &mut |el| {
                        if el.attrs.get("data-slot").map(String::as_str) == Some("command-list") {
                            assert_eq!(el.attrs["aria-multiselectable"], "true");
                        }
                        if el.attrs.get("id") == Some(&format!("{id}:item:0")) {
                            assert_eq!(
                                el.attrs["aria-selected"], "true",
                                "external selection includes disabled rows"
                            );
                            click_disabled = el.on_click.clone();
                        }
                        if el.attrs.get("id") == Some(&format!("{id}:item:1")) {
                            click_one = el.on_click.clone();
                            let mut check = false;
                            walk_cmd(el, &mut |child| {
                                if child.attrs.get("data-slot").map(String::as_str)
                                    == Some("command-selection-check")
                                {
                                    check = true;
                                    assert_eq!(child.style.opacity, 1.0);
                                }
                            });
                            assert!(check, "selection remains visible alongside a shortcut");
                        }
                    });
                    click_disabled.expect("disabled row").borrow_mut()();
                    assert!(calls.borrow().is_empty());
                    click_one.expect("selected row").borrow_mut()();
                    assert_eq!(selection.peek(), ["0"]);
                    assert_eq!(calls.borrow()[0], (1, vec!["0".into()], true));
                    assert!(open.peek());
                    assert!(handler.borrow_mut()(event(Key::Named(NamedKey::Enter))));
                    assert_eq!(selection.peek(), ["0", "1"]);
                    assert!(
                        open.peek(),
                        "Enter toggles without closing in selection mode"
                    );
                    if searchable {
                        // Text fields retain ordinary typing; navigation still reaches offscreen items.
                        for _ in 2..count {
                            assert!(handler.borrow_mut()(event(Key::Named(NamedKey::ArrowDown))));
                        }
                    } else {
                        assert!(handler.borrow_mut()(event(Key::Named(NamedKey::End))));
                    }
                    assert!(handler.borrow_mut()(event(if searchable {
                        Key::Named(NamedKey::Enter)
                    } else {
                        Key::Character(" ".into())
                    })));
                    assert_eq!(
                        selection.peek(),
                        vec!["0".to_string(), "1".into(), (count - 1).to_string()]
                    );
                    assert!(open.peek());
                    selection.set(vec!["2".into(), "absent-value".into()]);
                    sync(&mut root, &mut layout);
                    let mut last = false;
                    walk_cmd(&root, &mut |el| {
                        if el.attrs.get("id") == Some(&format!("{id}:item:{}", count - 1)) {
                            last = true;
                            assert_eq!(
                                el.attrs["aria-selected"], "false",
                                "highlight must not overwrite selection"
                            );
                            walk_cmd(el, &mut |child| {
                                if child.attrs.get("data-slot").map(String::as_str)
                                    == Some("command-selection-check")
                                {
                                    assert_eq!(child.style.opacity, 0.0);
                                }
                            });
                        }
                    });
                    assert!(last, "selected offscreen action must be revealable");
                    assert_eq!(
                        builds.get(),
                        1,
                        "selection does not rebuild/filter the full Command body"
                    );
                    let confirmed = Rc::new(RefCell::new(None));
                    let settings = command(&id)
                        .searchable(searchable)
                        .selection_signal(selection)
                        .on_confirm({
                            let confirmed = confirmed.clone();
                            move |values| *confirmed.borrow_mut() = Some((values, open.peek()))
                        });
                    provide_context(command_behavior_context(), settings.behavior);
                    root = command_paint(id.clone(), open, "Select actions".into(), surface, body);
                    let calls_before = calls.borrow().len();
                    assert!(handler_of(&root).borrow_mut()(event(Key::Named(
                        NamedKey::Enter
                    ))));
                    assert_eq!(
                        *confirmed.borrow(),
                        Some((vec!["2".into(), "absent-value".into()], true))
                    );
                    assert_eq!(
                        calls.borrow().len(),
                        calls_before,
                        "confirmation must not re-run the active item"
                    );
                    assert_eq!(open.peek(), !surface.dismissible());
                });
            }
        }
    }
}

#[test]
fn command_compound_selection_survives_filtering_and_confirms_empty_results() {
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::app::test_support::sync_reactive_elements_subtree;
    use crate::core::reactivity::{create_effect, mark_next_effect_as_ui_root};
    use winit::event::{ElementState, KeyEvent};
    use winit::keyboard::{KeyCode, KeyLocation, PhysicalKey};
    let (_owner, ()) = create_root(|| {
        let id = "cmd-compound-multi";
        let selection = Signal::new(Vec::<String>::new());
        let open = Signal::new(true);
        let confirmed = Rc::new(RefCell::new(Vec::new()));
        let builds = Rc::new(std::cell::Cell::new(0));
        let rendered = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(rendered.clone());
        mark_next_effect_as_ui_root();
        create_effect({
            let rendered = rendered.clone();
            let confirmed = confirmed.clone();
            let builds = builds.clone();
            move || {
                let confirmed = confirmed.clone();
                let builds = builds.clone();
                *rendered.borrow_mut() = command(id)
                    .inline()
                    .open_signal(open)
                    .selection_signal(selection)
                    .on_confirm(move |values| confirmed.borrow_mut().push(values))
                    .child(move || {
                        builds.set(builds.get() + 1);
                        let children = Element::new().children(
                            ["Alpha", "Beta"]
                                .into_iter()
                                .map(|name| {
                                    CommandItem(CommandItemProps {
                                        value: Some(name.into()),
                                        children: Some(Element::new().text(name)),
                                        ..Default::default()
                                    })
                                })
                                .collect::<Vec<_>>(),
                        );
                        Element::new().children(vec![
                            CommandInput(CommandInputProps::default()),
                            CommandList(CommandListProps {
                                children: Some(children),
                                base: None,
                            }),
                        ])
                    });
            }
        });
        let mut layout = LayoutTree::new();
        let mut sync = || {
            sync_reactive_elements_subtree(
                &mut rendered.borrow_mut(),
                &mut layout,
                &mut LayoutStatus::Clean,
            );
            crate::authoring::presentation::sync_tree(
                &mut rendered.borrow_mut(),
                &mut layout,
                crate::authoring::presentation::Context::new(
                    crate::authoring::presentation::Phase::State,
                    [800.0, 600.0],
                ),
            );
        };
        sync();
        let mut first = None;
        walk_cmd(&rendered.borrow(), &mut |el| {
            if el.attrs.get("id").map(String::as_str) == Some("cmd-compound-multi:item:0") {
                first = el.on_click.clone();
            }
        });
        first.expect("Alpha action").borrow_mut()();
        assert_eq!(selection.peek(), ["Alpha"]);
        assert!(open.peek());
        command_query(id).set("Beta");
        sync();
        assert_eq!(
            find_cmd_item(&rendered.borrow()).attrs["aria-selected"],
            "false"
        );
        let click = find_cmd_item(&rendered.borrow()).on_click.clone().unwrap();
        click.borrow_mut()();
        assert_eq!(selection.peek(), ["Alpha", "Beta"]);
        command_query(id).set("");
        sync();
        let selected_count = || {
            let mut selected = 0;
            walk_cmd(&rendered.borrow(), &mut |el| {
                selected += usize::from(
                    el.attrs.get("data-slot").map(String::as_str) == Some("command-item")
                        && el.attrs.get("aria-selected").map(String::as_str) == Some("true"),
                );
            });
            selected
        };
        assert_eq!(selected_count(), 2);
        let before = builds.get();
        selection.set(Vec::new());
        sync();
        assert_eq!(selected_count(), 0);
        selection.set(vec!["Beta".into()]);
        sync();
        assert_eq!(selected_count(), 1);
        assert_eq!(
            builds.get(),
            before,
            "external selection paints retained rows without rebuilding the body"
        );
        selection.set(Vec::new());
        command_query(id).set("no results");
        sync();
        let mut handler = None;
        walk_cmd(&rendered.borrow(), &mut |el| {
            if el.attrs.get("accesskit_role").map(String::as_str) == Some("combobox") {
                handler = el.on_key_down.clone();
            }
        });
        assert!(handler.unwrap().borrow_mut()(KeyEvent {
            physical_key: PhysicalKey::Code(KeyCode::Enter),
            logical_key: Key::Named(NamedKey::Enter),
            text: None,
            location: KeyLocation::Standard,
            state: ElementState::Pressed,
            repeat: false,
            text_with_all_modifiers: None,
            key_without_modifiers: Key::Named(NamedKey::Enter),
        }));
        assert_eq!(*confirmed.borrow(), vec![Vec::<String>::new()]);
        assert!(
            open.peek(),
            "inline confirmation keeps the embedded list open"
        );
        clear_test_reconcile_root();
    });
}

#[test]
fn controlled_command_results_preserve_identity_and_refresh_activation() {
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::create_effect;
    use winit::event::{ElementState, KeyEvent};
    use winit::keyboard::{KeyCode, KeyLocation, PhysicalKey};

    let key = |key: NamedKey| KeyEvent {
        physical_key: PhysicalKey::Code(KeyCode::Enter),
        logical_key: Key::Named(key),
        text: None,
        location: KeyLocation::Standard,
        state: ElementState::Pressed,
        repeat: false,
        text_with_all_modifiers: None,
        key_without_modifiers: Key::Named(key),
    };
    let settle = |live: &Rc<RefCell<Element>>, layout: &mut LayoutTree| {
        crate::app::reactive::drain_deferred_view_remounts();
        for _ in 0..3 {
            let mut root = live.borrow_mut();
            hoist_portal_subtrees(&mut root);
            layout.build_and_layout(&mut root, 800.0, 600.0, [-1.0; 2]);
            crate::app::test_support::sync_reactive_elements_subtree(
                &mut root,
                layout,
                &mut LayoutStatus::Clean,
            );
        }
        crate::authoring::presentation::sync_tree(
            &mut live.borrow_mut(),
            layout,
            crate::authoring::presentation::Context::new(
                crate::authoring::presentation::Phase::State,
                [800.0, 600.0],
            ),
        );
    };
    for count in [4, 100] {
        for surface in [
            CommandSurface::Inline,
            CommandSurface::Dialog,
            CommandSurface::Overlay,
        ] {
            for searchable in [false, true] {
                crate::app::reactive::reset_registered_anchors();
                clear_overlay_stack();
                let id = format!("cmd-controlled-results-{count}-{surface:?}-{searchable}");
                let live = Rc::new(RefCell::new(Element::new()));
                install_test_reconcile_root(live.clone());
                let (owner, (results, loading, selection, open, calls, builds, active, visible)) =
                    create_root(|| {
                        let calls = Rc::new(RefCell::new(Vec::new()));
                        let results = Signal::new(
                            (0..count)
                                .map(|i| {
                                    let calls = calls.clone();
                                    CommandAction::new(
                                        i.to_string(),
                                        format!("Result {i}"),
                                        move || {
                                            calls.borrow_mut().push((i, 0));
                                        },
                                    )
                                })
                                .collect::<Vec<_>>(),
                        );
                        let loading = Signal::new(false);
                        let selection = Signal::new(vec!["external".to_owned()]);
                        let open = Signal::new(true);
                        let builds = Rc::new(std::cell::Cell::new(0));
                        create_effect({
                            let live = live.clone();
                            let id = id.clone();
                            let builds = builds.clone();
                            move || {
                                builds.set(builds.get() + 1);
                                let mut builder = command(&id)
                                    .open_signal(open)
                                    .searchable(searchable)
                                    .filterable(false)
                                    .loading_signal(loading)
                                    .selection_signal(selection)
                                    .action(CommandAction::new("ignored", "Static ignored", || {}))
                                    .actions_signal(results);
                                builder = match surface {
                                    CommandSurface::Inline => builder.inline(),
                                    CommandSurface::Dialog => builder.dialog(),
                                    CommandSurface::Overlay => builder.overlay(),
                                };
                                *live.borrow_mut() = builder.into();
                            }
                        });
                        (
                            results,
                            loading,
                            selection,
                            open,
                            calls,
                            builds,
                            use_signal(&format!("{id}__cmd_active"), || NO_ACTIVE_COMMAND),
                            use_signal(
                                &format!("{id}__cmd_visible"),
                                Vec::<CommandVisibleItem>::new,
                            ),
                        )
                    });
                let mut layout = LayoutTree::new();
                settle(&live, &mut layout);
                let handler = {
                    let mut handler = None;
                    walk_cmd(&live.borrow(), &mut |el| {
                        if (searchable
                            && el.attrs.get("accesskit_role").map(String::as_str)
                                == Some("combobox"))
                            || (!searchable
                                && el.focus_id.as_deref() == Some(format!("{id}:listbox").as_str()))
                        {
                            handler = el.on_key_down.clone();
                        }
                    });
                    handler.unwrap_or_else(|| panic!("keyboard target {id}"))
                };
                assert_eq!(visible.peek().len(), count);
                assert_eq!(active.peek(), NO_ACTIVE_COMMAND);
                command_query(&id).set("remote-ranked-query");
                settle(&live, &mut layout);
                assert_eq!(visible.peek().len(), count, "external filtering bypass");
                if searchable {
                    for _ in 0..count {
                        assert!(handler.borrow_mut()(key(NamedKey::ArrowDown)));
                    }
                } else {
                    assert!(handler.borrow_mut()(key(NamedKey::End)));
                }
                settle(&live, &mut layout);
                assert_eq!(active.peek(), count - 1);
                let mut mounted = 0;
                let mut last_mounted = false;
                walk_cmd(&live.borrow(), &mut |el| {
                    if el.attrs.get("data-slot").map(String::as_str) == Some("command-item") {
                        mounted += 1;
                        last_mounted |=
                            el.attrs.get("id") == Some(&format!("{id}:item:{}", count - 1));
                    }
                });
                assert!(
                    mounted <= 30 && last_mounted,
                    "bounded rows reveal logical end: {id}"
                );
                let mut reordered = results.peek();
                reordered.reverse();
                results.set(reordered);
                settle(&live, &mut layout);
                assert_eq!(
                    active.peek(),
                    0,
                    "highlight follows stable ID after reordering"
                );
                assert_eq!(visible.peek()[0].value, (count - 1).to_string());
                assert_eq!(command_query(&id).get(), "remote-ranked-query");
                // Equal display metadata with a different callback must invalidate activation.
                let mut refreshed = results.peek();
                let latest = calls.clone();
                refreshed[0].run = Rc::new(move || latest.borrow_mut().push((count - 1, 1)));
                results.set(refreshed);
                settle(&live, &mut layout);
                assert!(handler.borrow_mut()(key(NamedKey::Enter)));
                assert_eq!(&*calls.borrow(), &[(count - 1, 1)]);
                assert_eq!(
                    selection.peek(),
                    ["external".to_owned(), (count - 1).to_string()]
                );
                assert!(open.peek(), "multi-selection retains palette");
                let mut disabled = results.peek();
                disabled[0].disabled = true;
                results.set(disabled);
                settle(&live, &mut layout);
                assert_eq!(
                    active.peek(),
                    NO_ACTIVE_COMMAND,
                    "disabled highlight clears"
                );
                handler.borrow_mut()(key(NamedKey::Enter));
                assert_eq!(calls.borrow().len(), 1, "no stale activation after disable");
                assert!(handler.borrow_mut()(key(NamedKey::ArrowDown)));
                assert_eq!(active.peek(), 1);
                let mut removed = results.peek();
                removed.remove(1);
                results.set(removed);
                settle(&live, &mut layout);
                assert_eq!(
                    active.peek(),
                    NO_ACTIVE_COMMAND,
                    "removed highlight cannot retarget neighbour"
                );
                handler.borrow_mut()(key(NamedKey::Enter));
                assert_eq!(calls.borrow().len(), 1);
                loading.set(true);
                results.set(Vec::new());
                settle(&live, &mut layout);
                assert!(visible.peek().is_empty());
                assert!(
                    crate::core::tree::find_in_tree(&live.borrow(), |el| {
                        el.attrs.get("data-slot").map(String::as_str) == Some("command-loading")
                    })
                    .is_some()
                );
                assert_eq!(
                    selection.peek(),
                    ["external".to_owned(), (count - 1).to_string()]
                );
                open.set(false);
                results.set(vec![CommandAction::new("new", "New result", || {})]);
                loading.set(false);
                open.set(true);
                settle(&live, &mut layout);
                assert_eq!(visible.peek()[0].value, "new");
                assert_eq!(
                    builds.get(),
                    1,
                    "source changes must stay in component anchors: {id}"
                );
                clear_test_reconcile_root();
                drop(live);
                drop(owner);
                crate::app::reactive::reset_registered_anchors();
            }
        }
    }
}

#[test]
fn query_signal_drives_all_command_surfaces_without_rebuilding_owner() {
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::create_effect;
    use winit::event::{ElementState, KeyEvent};
    use winit::keyboard::{KeyCode, KeyLocation, PhysicalKey};
    let settle = |live: &Rc<RefCell<Element>>, layout: &mut LayoutTree| {
        crate::app::reactive::drain_deferred_view_remounts();
        for _ in 0..3 {
            let mut root = live.borrow_mut();
            hoist_portal_subtrees(&mut root);
            layout.build_and_layout(&mut root, 800.0, 600.0, [-1.0; 2]);
            crate::app::test_support::sync_reactive_elements_subtree(
                &mut root,
                layout,
                &mut LayoutStatus::Clean,
            );
        }
        crate::authoring::presentation::sync_tree(
            &mut live.borrow_mut(),
            layout,
            crate::authoring::presentation::Context::new(
                crate::authoring::presentation::Phase::State,
                [800.0, 600.0],
            ),
        );
    };
    for count in [0, 4, 100] {
        for surface in [
            CommandSurface::Inline,
            CommandSurface::Dialog,
            CommandSurface::Overlay,
        ] {
            for searchable in [true, false] {
                crate::app::reactive::reset_registered_anchors();
                clear_overlay_stack();
                let id = format!("cmd-query-signal-{count}-{surface:?}-{searchable}");
                let first = if count == 0 {
                    "Beta".into()
                } else {
                    format!("Entry {:04}", count - 1)
                };
                let next: String = if count == 0 {
                    "Gamma".into()
                } else {
                    "Entry 0000".into()
                };
                let expected: String = if count == 0 { "c".into() } else { "0".into() };
                let live = Rc::new(RefCell::new(Element::new()));
                install_test_reconcile_root(live.clone());
                let (owner, (query, open, loading, builds, visible, focus)) = create_root(|| {
                    let query = Signal::new(first.clone());
                    let open = Signal::new(true);
                    let loading = Signal::new(false);
                    let builds = Rc::new(std::cell::Cell::new(0));
                    create_effect({
                        let id = id.clone();
                        let live = live.clone();
                        let builds = builds.clone();
                        move || {
                            builds.set(builds.get() + 1);
                            let mut builder = command(&id)
                                .open_signal(open)
                                .query_signal(query)
                                .loading_signal(loading)
                                .searchable(searchable);
                            builder = match surface {
                                CommandSurface::Inline => builder.inline(),
                                CommandSurface::Dialog => builder.dialog(),
                                CommandSurface::Overlay => builder.overlay(),
                            };
                            *live.borrow_mut() = if count == 0 {
                                let body = three_item_command_body();
                                builder.child(move || body())
                            } else {
                                for i in 0..count {
                                    builder = builder.action(CommandAction::new(
                                        i.to_string(),
                                        format!("Entry {i:04}"),
                                        || {},
                                    ));
                                }
                                builder.into()
                            };
                        }
                    });
                    (
                        query,
                        open,
                        loading,
                        builds,
                        use_signal(
                            &format!("{id}__cmd_visible"),
                            Vec::<CommandVisibleItem>::new,
                        ),
                        use_signal("__jadix_button_focus_id", String::new),
                    )
                });
                let mut layout = LayoutTree::new();
                settle(&live, &mut layout);
                assert_eq!(command_query(&id).get(), first, "mount seed {id}");
                focus.set(String::new());
                query.set(next.clone());
                settle(&live, &mut layout);
                if searchable {
                    assert_eq!(visible.peek().len(), 1, "external filters {id}");
                    assert_eq!(visible.peek()[0].value, expected);
                } else {
                    assert_eq!(visible.peek().len(), if count == 0 { 3 } else { count });
                }
                focus.set(crate::utils::text_field::input_focus_id(&format!(
                    "{id}__filter"
                )));
                query.set("pending remote".into());
                loading.set(true);
                loading.set(false);
                settle(&live, &mut layout);
                assert_eq!(command_query(&id).get(), next, "no reseed on remount {id}");
                command_query(&id).set(first.clone());
                assert_eq!(query.peek(), first, "local commit wins {id}");
                let handler = crate::core::tree::find_in_tree(&live.borrow(), |el| {
                    if searchable {
                        el.attrs.get("accesskit_role").map(String::as_str) == Some("combobox")
                    } else {
                        el.focus_id.as_deref() == Some(format!("{id}:listbox").as_str())
                    }
                })
                .unwrap()
                .on_key_down
                .clone()
                .unwrap();
                if searchable {
                    assert!(handler.borrow_mut()(KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::Escape),
                        logical_key: Key::Named(NamedKey::Escape),
                        text: None,
                        location: KeyLocation::Standard,
                        state: ElementState::Pressed,
                        repeat: false,
                        text_with_all_modifiers: None,
                        key_without_modifiers: Key::Named(NamedKey::Escape),
                    }));
                    assert_eq!(query.peek(), "", "session clear publishes {id}");
                    assert!(open.peek(), "first Escape only clears query {id}");
                } else {
                    command_query(&id).set(String::new());
                    assert_eq!(query.peek(), "");
                }
                focus.set(String::new());
                open.set(false);
                settle(&live, &mut layout);
                open.set(true);
                settle(&live, &mut layout);
                focus.set(String::new());
                query.set(next.clone());
                settle(&live, &mut layout);
                assert_eq!(
                    command_query(&id).get(),
                    next,
                    "binding survives reopen {id}"
                );
                let mut mounted = 0;
                walk_cmd(&live.borrow(), &mut |el| {
                    mounted += usize::from(
                        el.attrs.get("data-slot").map(String::as_str) == Some("command-item"),
                    );
                });
                assert!(mounted <= 30, "virtual rows bounded {id}: {mounted}");
                assert_eq!(builds.get(), 1, "query stays inside component {id}");
                clear_test_reconcile_root();
                drop(live);
                drop(owner);
                crate::app::reactive::reset_registered_anchors();
            }
        }
    }
}
