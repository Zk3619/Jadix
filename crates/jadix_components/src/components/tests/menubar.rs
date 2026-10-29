use super::*;

#[test]
fn menubar_scope_exposes_template() {
    let el = menubar_scope("mb-scope", || {
        menubar_menu(Some("file".into()), None, || {
            let trigger = MenubarTrigger(MenubarTriggerProps {
                id: String::new(),
                children: Some(Element::new().text("File")),
                base: None,
            });
            let content = menubar_content(MenubarContentOpts::default(), || {
                MenubarItem(MenubarItemProps {
                    disabled: Some(false),
                    inset: Some(false),
                    variant: None,
                    on_click: None,
                    children: Some(Element::new().text("New")),
                    base: None,
                })
            });
            let mut frag = Element::new();
            frag.set_attr("jadix_fragment", "1");
            frag.children(vec![trigger, content])
        })
    });
    assert_eq!(
        el.template_id.as_deref(),
        Some("mb-scope__menubar"),
        "scope must expose a stable menubar template id"
    );
    assert_eq!(
        el.attrs.get("data-slot").map(String::as_str),
        Some("menubar")
    );
}

#[test]
fn builder_builds_compound() {
    let el: Element = menubar("mb-b")
        .section(MenuSection::new(
            "file",
            "File",
            vec![MenuItem::new("n", "New", || {})],
        ))
        .into();
    assert!(el.template_id.as_deref().is_some());
}

fn has_mb_trig_view(el: &Element) -> bool {
    el.template_id
        .as_deref()
        .is_some_and(|t| t.contains("__mb_trig_view"))
        || el.children.iter().any(has_mb_trig_view)
}

fn find_menubar_trigger(el: &Element) -> Option<&Element> {
    if el.attrs.get("data-slot").map(String::as_str) == Some(SLOT_TRIGGER) {
        return Some(el);
    }
    el.children.iter().find_map(find_menubar_trigger)
}

#[test]
fn hover_switch_keeps_successor_open_after_previous_menu_exits() {
    use crate::app::portal_overlay::clear_overlay_stack;
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_effect, create_root};
    use crate::core::tree::{find_in_tree, LayoutTree};
    use crate::utils::floating_motion::{
        drain_pending_floating_unmounts, flush_floating_motion_subtree, tick_floating_presence,
        tick_hoisted_floating_css,
    };

    clear_overlay_stack();
    reset_registered_anchors();
    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(live.clone());
    let (owner, ()) = create_root(|| {
        let live = live.clone();
        create_effect(move || {
            *live.borrow_mut() = crate::ui! {
                <Menubar id="mb-hover-switch">
                    <MenubarMenu id="file">
                        <MenubarTrigger>"File"</MenubarTrigger>
                        <MenubarContent><MenubarItem>"New File"</MenubarItem></MenubarContent>
                    </MenubarMenu>
                    <MenubarMenu id="edit">
                        <MenubarTrigger>"Edit"</MenubarTrigger>
                        <MenubarContent><MenubarItem>"Undo"</MenubarItem></MenubarContent>
                    </MenubarMenu>
                    <MenubarMenu id="view">
                        <MenubarTrigger>"View"</MenubarTrigger>
                        <MenubarContent><MenubarItem>"Zoom"</MenubarItem></MenubarContent>
                    </MenubarMenu>
                </Menubar>
            };
        });
    });
    drain_deferred_view_remounts();
    let trigger = |name: &str| {
        let fid = format!("mb-hover-switch__{name}__dd_trigger");
        let tree = live.borrow();
        let el = find_in_tree(&tree, |el| el.focus_id.as_deref() == Some(&fid))
            .expect("menubar trigger");
        (el.on_click.clone().unwrap(), el.on_hover.clone().unwrap())
    };
    let (click_file, hover_file) = trigger("file");
    let (_, hover_edit) = trigger("edit");
    let (_, hover_view) = trigger("view");
    let menus = menus_of("mb-hover-switch");
    let mut layout = LayoutTree::new();
    let mut advance_frames = |frames| {
        crate::style::utility::set_frame_dt(0.016);
        for _ in 0..frames {
            tick_floating_presence();
            drain_pending_floating_unmounts();
            drain_deferred_view_remounts();
            let mut tree = live.borrow_mut();
            layout.build_and_layout(&mut tree, 800.0, 600.0, [0.0, 0.0]);
            crate::utils::popup::sync_floating_placement(&mut tree, 800.0, 600.0);
            flush_floating_motion_subtree(&mut tree);
            tick_hoisted_floating_css(&mut tree);
        }
    };
    click_file.borrow_mut()();
    advance_frames(24);
    assert!(menus[0].open.peek(), "File must stay open before switching");
    crate::utils::popup::arm_underlay_close();
    for (index, hover) in [
        (1, &hover_edit),
        (2, &hover_view),
        (1, &hover_edit),
        (0, &hover_file),
    ] {
        hover.borrow_mut()(true);
        assert!(menus[index].open.peek(), "hover must open menu {index}");
        advance_frames(24);
        assert!(
            menus[index].open.peek(),
            "menu {index} must stay open after its predecessor exits"
        );
        assert_eq!(menus.iter().filter(|menu| menu.open.peek()).count(), 1);
        let panel_id = format!("{}__floating_panel", menus[index].id);
        assert!(
            find_in_tree(&live.borrow(), |el| el.template_id.as_deref()
                == Some(&panel_id))
            .is_some(),
            "menu {index} must retain its visible panel"
        );
        hover.borrow_mut()(false);
    }
    // Change direction again before any of the outgoing menus finishes exiting.
    for (index, hover) in [
        (1, &hover_edit),
        (2, &hover_view),
        (0, &hover_file),
        (1, &hover_edit),
    ] {
        hover.borrow_mut()(true);
        advance_frames(1);
        assert!(
            menus[index].open.peek(),
            "rapid hover must keep menu {index} open"
        );
        assert_eq!(
            menus.iter().filter(|menu| menu.open.peek()).count(),
            1,
            "rapid switching must leave only the hovered menu open"
        );
        hover.borrow_mut()(false);
    }
    advance_frames(24);
    assert!(
        menus[1].open.peek(),
        "Edit must survive all preceding exits"
    );
    assert!(crate::utils::popup::underlay_close_is_armed());
    assert!(crate::app::portal_overlay::try_dismiss_outside_press());
    assert!(!any_menu_open("mb-hover-switch"));
    crate::style::utility::set_frame_dt(0.0);
    clear_test_reconcile_root();
    drop(live);
    owner.dispose();
    clear_overlay_stack();
    reset_registered_anchors();
}

#[test]
fn menubar_trigger_open_patches_chrome_without_island() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::app::reactive::reset_registered_anchors;
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_effect, create_root, take_flushed_effects};

    reset_registered_anchors();
    clear_test_reconcile_root();
    let tree: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&tree));

    let (_owner, open) = create_root(|| {
        let open = use_signal("mb_trig_chrome__file__dd_open", || false);
        let tree = Rc::clone(&tree);
        create_effect({
            let open = open;
            move || {
                let el = menubar_scope("mb_trig_chrome", || {
                    menubar_menu(Some("file".into()), Some(open), || {
                        let trigger = MenubarTrigger(MenubarTriggerProps {
                            id: String::new(),
                            children: Some(Element::new().text("File")),
                            base: None,
                        });
                        let mut frag = Element::new();
                        frag.set_attr("jadix_fragment", "1");
                        frag.children(vec![trigger])
                    })
                });
                *tree.borrow_mut() = el;
            }
        });
        open
    });

    assert!(
        !has_mb_trig_view(&tree.borrow()),
        "MenubarTrigger must not mount a reactive_view placeholder"
    );
    let tid = find_menubar_trigger(&tree.borrow())
        .and_then(|t| t.template_id.clone())
        .expect("trigger template_id");
    assert_eq!(tid, "mb_trig_chrome__file__dd_trigger");
    assert_eq!(
        find_menubar_trigger(&tree.borrow())
            .and_then(|t| t.attrs.get("data-state"))
            .map(String::as_str),
        Some("closed")
    );

    let _ = take_flushed_effects();
    open.set(true);
    let flushed = take_flushed_effects();
    assert!(
        flushed
            .iter()
            .any(|(_, phase)| *phase == crate::core::reactivity::EffectPhase::Render),
        "open.set must run trigger chrome bind Render effects, got {flushed:?}"
    );
    assert!(
        !has_mb_trig_view(&tree.borrow()),
        "open.set must not grow a trigger island"
    );
    let (after_tid, after_state, after_bg, after_hover_none) = {
        let tree = tree.borrow();
        let after = find_menubar_trigger(&tree).expect("trigger after set");
        (
            after.template_id.clone(),
            after.attrs.get("data-state").cloned(),
            after.style.bg_color,
            after
                .authoring_state
                .get::<crate::components::button::ButtonHoverPaint>()
                .is_none(),
        )
    };
    assert_eq!(after_tid.as_deref(), Some(tid.as_str()));
    assert_eq!(after_state.as_deref(), Some("open"));
    assert_eq!(after_bg, parse_color("accent"));
    assert!(
        after_hover_none,
        "open chrome must disable hover paint so sync cannot overwrite accent"
    );
}

/// Gallery symptom proxy: namespaced menubar parent open + sub_open must leave
/// parent open and push a nested Menu overlay (SubContent `portal_sync` path).
#[test]
fn menubar_sub_open_keeps_parent_and_pushes_sub_overlay() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::app::portal_overlay::{clear_overlay_stack, overlay_stack_depth_for_test};
    use crate::app::reactive::{registered_anchor_count, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_effect, create_root, use_signal};

    clear_overlay_stack();
    reset_registered_anchors();
    clear_test_reconcile_root();

    let root: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&root));

    let (handle, parent_open) = create_root(|| {
        let parent_open = use_signal("mb_sub_loop_bar__file__dd_open", || false);
        let root_e = Rc::clone(&root);
        create_effect(move || {
            let el = menubar_scope("mb_sub_loop_bar", || {
                menubar_menu(Some("file".into()), Some(parent_open), || {
                    let trigger = MenubarTrigger(MenubarTriggerProps {
                        id: String::new(),
                        children: Some(Element::new().text("File")),
                        base: None,
                    });
                    let content = menubar_content(MenubarContentOpts::default(), || {
                        menubar_sub(None, || {
                            let st = MenubarSubTrigger(MenubarSubTriggerProps {
                                disabled: Some(false),
                                inset: Some(false),
                                children: Some(Element::new().text("Share")),
                                base: None,
                            });
                            let sc = menubar_sub_content(Default::default(), || {
                                MenubarItem(MenubarItemProps {
                                    disabled: Some(false),
                                    inset: Some(false),
                                    variant: None,
                                    on_click: None,
                                    children: Some(Element::new().text("Email")),
                                    base: None,
                                })
                            });
                            let mut frag = Element::new();
                            frag.set_attr("jadix_fragment", "1");
                            frag.children(vec![st, sc])
                        })
                    });
                    let mut frag = Element::new();
                    frag.set_attr("jadix_fragment", "1");
                    frag.children(vec![trigger, content])
                })
            });
            *root_e.borrow_mut() = el;
        });
        parent_open
    });

    let anchors_before_open = registered_anchor_count();
    let depth_before = overlay_stack_depth_for_test();
    parent_open.set(true);
    let anchors_after_open = registered_anchor_count();
    let depth_after_open = overlay_stack_depth_for_test();
    assert!(parent_open.peek(), "parent_open.set(true) must stick");
    assert!(
            depth_after_open > depth_before,
            "Content open must portal_sync parent Menu (before={depth_before} after={depth_after_open}); anchors {anchors_before_open}->{anchors_after_open}"
        );
    assert!(
            anchors_after_open > anchors_before_open,
            "opening menubar content must register SubContent reactive_anchor (before={anchors_before_open} after={anchors_after_open}, depth={depth_after_open})"
        );

    // Gallery path: layout hoists the Content portal before the user can hover Share.
    {
        let mut tree = root.borrow_mut();
        let _ = crate::layout::layout_tree::hoist_portal_subtrees(&mut tree);
    }

    let sub_anchor_after_hoist = {
        let tree = root.borrow();
        fn find_tid(el: &Element, want: &str) -> bool {
            if el.template_id.as_deref() == Some(want) {
                return true;
            }
            el.children.iter().any(|c| find_tid(c, want))
        }
        find_tid(&tree, "mb_sub_loop_bar__file__sub0__view")
    };
    assert!(
        sub_anchor_after_hoist,
        "SubContent placeholder must remain findable after Content portal hoist"
    );

    let sub_trigger_fid = {
        let tree = root.borrow();
        fn find_sub_trigger_fid(el: &Element) -> Option<String> {
            if el.attrs.get("data-slot").map(String::as_str) == Some("dropdown-menu-sub-trigger") {
                return el.focus_id.clone();
            }
            for c in &el.children {
                if let Some(f) = find_sub_trigger_fid(c) {
                    return Some(f);
                }
            }
            None
        }
        find_sub_trigger_fid(&tree)
    };
    assert_eq!(
        sub_trigger_fid.as_deref(),
        Some("mb_sub_loop_bar__file__sub0__trigger"),
        "MenubarSubTrigger must expose the floating trigger focus_id after hoist"
    );

    let sub_open = use_signal("mb_sub_loop_bar__file__sub0__open", || false);
    assert!(!sub_open.peek(), "sub starts closed after parent open");
    sub_open.set(true);

    assert!(
        parent_open.peek(),
        "parent must stay open after sub opens (portal must not exclusive-dismiss)"
    );
    assert!(
        sub_open.peek(),
        "sub open signal must remain true after SubContent portal_sync"
    );
    assert!(
        overlay_stack_depth_for_test() >= 2,
        "SubContent open must push nested Menu overlay (got depth {})",
        overlay_stack_depth_for_test()
    );

    // Nested sub portal must hoist out of the parent menu (same as Select-in-Dialog).
    {
        let mut tree = root.borrow_mut();
        let _ = crate::layout::layout_tree::hoist_portal_subtrees(&mut tree);
    }

    let (has_sub_panel, sub_on_portal_layer, layer_len) = {
        let tree = root.borrow();
        fn find_slot(el: &Element, slot: &str) -> bool {
            if el.attrs.get("data-slot").map(String::as_str) == Some(slot) {
                return true;
            }
            el.children.iter().any(|c| find_slot(c, slot))
        }
        let layer = tree.children.iter().find(|c| {
            c.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID)
        });
        let layer_len = layer.map(|l| l.children.len()).unwrap_or(0);
        let sub_on_layer = layer.is_some_and(|l| {
            l.children
                .iter()
                .any(|c| find_slot(c, "dropdown-menu-sub-content"))
        });
        (
            find_slot(&tree, "dropdown-menu-sub-content"),
            sub_on_layer,
            layer_len,
        )
    };
    assert!(
        has_sub_panel,
        "SubContent open must reconcile a dropdown-menu-sub-content panel into the tree"
    );
    assert!(
        sub_on_portal_layer,
        "submenu must be a portal-layer sibling after hoist (layer_len={layer_len}); \
             staying under parent overflow:hidden clips RightStart panels — gallery Share symptom"
    );
    assert!(
        layer_len >= 2,
        "parent File menu + Share submenu must both live on the portal layer (got {layer_len})"
    );

    // Stuck-open recovery: clear panel chrome under the live `{sid}__view` island
    // without dropping its UnmountHook (that would dispose the SubContent effect).
    {
        let mut tree = root.borrow_mut();
        fn hollow_sub_view(el: &mut Element, want: &str) -> bool {
            if el.template_id.as_deref() == Some(want) {
                el.children.clear();
                el.attrs.remove("data-slot");
                el.text = None;
                return true;
            }
            for c in &mut el.children {
                if hollow_sub_view(c, want) {
                    return true;
                }
            }
            false
        }
        assert!(
            hollow_sub_view(&mut tree, "mb_sub_loop_bar__file__sub0__view"),
            "must find sub view island"
        );
    }
    let remount = use_signal("mb_sub_loop_bar__file__sub0__remount", || 0u32);
    remount.set(remount.peek().wrapping_add(1));
    assert!(
        {
            let tree = root.borrow();
            fn find_slot(el: &Element, slot: &str) -> bool {
                if el.attrs.get("data-slot").map(String::as_str) == Some(slot) {
                    return true;
                }
                el.children.iter().any(|c| find_slot(c, slot))
            }
            find_slot(&tree, "dropdown-menu-sub-content")
        },
        "remount bump must rebuild SubContent panel while sub_open stays true"
    );

    clear_test_reconcile_root();
    drop(root);
    handle.dispose();
    clear_overlay_stack();
    reset_registered_anchors();
}

/// Gallery root cause: sibling MenubarMenu scopes share one owner; without child-owner
/// isolation the last menu (View) overwrites DropdownMenuCtx. File→Share then binds as
/// `{bar}__view__sub0` while View.open is false — SubContent immediately closes it.
#[test]
fn menubar_sibling_menus_isolate_sub_parent_id() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::app::portal_overlay::clear_overlay_stack;
    use crate::app::reactive::reset_registered_anchors;
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_effect, create_root, use_signal};

    clear_overlay_stack();
    reset_registered_anchors();
    clear_test_reconcile_root();

    let root: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&root));

    let (handle, file_open) = create_root(|| {
        let file_open = use_signal("mb_iso_bar__file__dd_open", || false);
        let root_e = Rc::clone(&root);
        create_effect(move || {
            let el = menubar_scope("mb_iso_bar", || {
                let file = menubar_menu(Some("file".into()), Some(file_open), || {
                    let trigger = MenubarTrigger(MenubarTriggerProps {
                        id: String::new(),
                        children: Some(Element::new().text("File")),
                        base: None,
                    });
                    let content = menubar_content(MenubarContentOpts::default(), || {
                        menubar_sub(None, || {
                            let st = MenubarSubTrigger(MenubarSubTriggerProps {
                                disabled: Some(false),
                                inset: Some(false),
                                children: Some(Element::new().text("Share")),
                                base: None,
                            });
                            let sc = menubar_sub_content(Default::default(), || {
                                MenubarItem(MenubarItemProps {
                                    disabled: Some(false),
                                    inset: Some(false),
                                    variant: None,
                                    on_click: None,
                                    children: Some(Element::new().text("Email")),
                                    base: None,
                                })
                            });
                            let mut frag = Element::new();
                            frag.set_attr("jadix_fragment", "1");
                            frag.children(vec![st, sc])
                        })
                    });
                    let mut frag = Element::new();
                    frag.set_attr("jadix_fragment", "1");
                    frag.children(vec![trigger, content])
                });
                // Last sibling must not steal File's DropdownMenuCtx.
                let view = menubar_menu(Some("view".into()), None, || {
                    let trigger = MenubarTrigger(MenubarTriggerProps {
                        id: String::new(),
                        children: Some(Element::new().text("View")),
                        base: None,
                    });
                    let content = menubar_content(MenubarContentOpts::default(), || {
                        MenubarItem(MenubarItemProps {
                            disabled: Some(false),
                            inset: Some(false),
                            variant: None,
                            on_click: None,
                            children: Some(Element::new().text("Status Bar")),
                            base: None,
                        })
                    });
                    let mut frag = Element::new();
                    frag.set_attr("jadix_fragment", "1");
                    frag.children(vec![trigger, content])
                });
                let mut frag = Element::new();
                frag.set_attr("jadix_fragment", "1");
                frag.children(vec![file, view])
            });
            *root_e.borrow_mut() = el;
        });
        file_open
    });

    file_open.set(true);
    {
        let mut tree = root.borrow_mut();
        let _ = crate::layout::layout_tree::hoist_portal_subtrees(&mut tree);
    }

    let sub_trigger_fid = {
        let tree = root.borrow();
        fn find_sub_trigger_fid(el: &Element) -> Option<String> {
            if el.attrs.get("data-slot").map(String::as_str) == Some("dropdown-menu-sub-trigger")
                || el.attrs.get("data-slot").map(String::as_str) == Some("menubar-sub-trigger")
            {
                return el.focus_id.clone();
            }
            for c in &el.children {
                if let Some(f) = find_sub_trigger_fid(c) {
                    return Some(f);
                }
            }
            None
        }
        find_sub_trigger_fid(&tree)
    };
    assert_eq!(
        sub_trigger_fid.as_deref(),
        Some("mb_iso_bar__file__sub0__trigger"),
        "Share under File must keep File's id even after a sibling View menu is built \
             (regression: was mb_iso_bar__view__sub0__trigger)"
    );
    assert!(
        !use_signal("mb_iso_bar__view__sub0__open", || false).peek(),
        "View must not own a phantom Share sub signal"
    );

    clear_test_reconcile_root();
    drop(root);
    handle.dispose();
    clear_overlay_stack();
    reset_registered_anchors();
}

/// Gallery-faithful: Content body is a VStack (macro multi-child) with Separators around Sub.
/// Drive open via the real SubTrigger `on_hover` (set_dd_active + open_or_remount), then
/// hoist + layout + floating — assert the submenu has on-screen geometry.
#[test]
fn menubar_share_hover_opens_visible_submenu_panel() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::app::portal_overlay::clear_overlay_stack;
    use crate::app::reactive::reset_registered_anchors;
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_effect, create_root, use_signal};
    use crate::core::tree::LayoutTree;
    use crate::utils::popup::sync_floating_placement;

    clear_overlay_stack();
    reset_registered_anchors();
    clear_test_reconcile_root();

    let root: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&root));

    let (handle, parent_open) = create_root(|| {
        let parent_open = use_signal("mb_share_vis_bar__file__dd_open", || false);
        let root_e = Rc::clone(&root);
        create_effect(move || {
            let el = menubar_scope("mb_share_vis_bar", || {
                menubar_menu(Some("file".into()), Some(parent_open), || {
                    let trigger = MenubarTrigger(MenubarTriggerProps {
                        id: String::new(),
                        children: Some(Element::new().text("File")),
                        base: None,
                    });
                    // Mirror gallery RSX multi-child Content → VStack wrapping.
                    let content = menubar_content(MenubarContentOpts::default(), || {
                        let mut col = crate::primitives::vstack().w("100%").gap("0");
                        col = col.child(MenubarItem(MenubarItemProps {
                            disabled: Some(false),
                            inset: Some(false),
                            variant: None,
                            on_click: None,
                            children: Some(Element::new().text("New File")),
                            base: None,
                        }));
                        col = col.child(MenubarSeparator(MenubarSeparatorProps {
                            children: None,
                            base: None,
                        }));
                        col = col.child(menubar_sub(None, || {
                            let st = MenubarSubTrigger(MenubarSubTriggerProps {
                                disabled: Some(false),
                                inset: Some(false),
                                children: Some(Element::new().text("Share")),
                                base: None,
                            });
                            let sc = menubar_sub_content(Default::default(), || {
                                MenubarItem(MenubarItemProps {
                                    disabled: Some(false),
                                    inset: Some(false),
                                    variant: None,
                                    on_click: None,
                                    children: Some(Element::new().text("Email link")),
                                    base: None,
                                })
                            });
                            let mut frag = Element::new();
                            frag.set_attr("jadix_fragment", "1");
                            frag.children(vec![st, sc])
                        }));
                        col.into()
                    });
                    let mut frag = Element::new();
                    frag.set_attr("jadix_fragment", "1");
                    frag.children(vec![trigger, content])
                })
            });
            *root_e.borrow_mut() = el;
        });
        parent_open
    });

    parent_open.set(true);
    {
        let mut tree = root.borrow_mut();
        let _ = crate::layout::layout_tree::hoist_portal_subtrees(&mut tree);
    }

    // Fire the real SubTrigger hover handler (same path as gallery pointer enter).
    let hover_cb = {
        let tree = root.borrow();
        fn find_sub_hover(
            el: &Element,
        ) -> Option<std::rc::Rc<std::cell::RefCell<dyn FnMut(bool)>>> {
            if el.attrs.get("data-slot").map(String::as_str) == Some("dropdown-menu-sub-trigger") {
                return el.on_hover.clone();
            }
            for c in &el.children {
                if let Some(h) = find_sub_hover(c) {
                    return Some(h);
                }
            }
            None
        }
        find_sub_hover(&tree)
    };
    let hover_cb = hover_cb.expect("Share SubTrigger must expose on_hover after File open");
    hover_cb.borrow_mut()(true);

    // Same-frame contract: reconcile of SubContent must hoist the nested portal out of
    // File's overflow:hidden scroll — no extra hoist_portal_subtrees call required.
    {
        let tree = root.borrow();
        let layer = tree.children.iter().find(|c| {
            c.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID)
        });
        fn find_sub_panel(el: &Element) -> bool {
            el.attrs.get("data-slot").map(String::as_str) == Some("dropdown-menu-sub-content")
                || el.children.iter().any(find_sub_panel)
        }
        assert!(
                layer.is_some_and(|l| l.children.iter().any(find_sub_panel)),
                "Share submenu must sit on portal layer immediately after hover reconcile (same-frame hoist)"
            );
    }

    {
        let mut tree = root.borrow_mut();
        // Give the File trigger + Share row non-zero geometry so floating can place.
        fn stamp_bounds(el: &mut Element) {
            if el.focus_id.as_deref() == Some("mb_share_vis_bar__file__dd_trigger") {
                el.layout_bounds = [40.0, 40.0, 48.0, 28.0];
            }
            if el.focus_id.as_deref() == Some("mb_share_vis_bar__file__sub0__trigger") {
                el.layout_bounds = [48.0, 120.0, 160.0, 28.0];
            }
            for c in &mut el.children {
                stamp_bounds(c);
            }
        }
        stamp_bounds(&mut tree);
        let _ = sync_floating_placement(&mut tree, 800.0, 600.0);
        let mut layout = LayoutTree::new();
        let _ = layout.build_and_layout(&mut tree, 800.0, 600.0, [-1.0, -1.0]);
        let _ = crate::layout::layout_tree::hoist_portal_subtrees(&mut tree);
        let _ = sync_floating_placement(&mut tree, 800.0, 600.0);
        let _ = layout.build_and_layout(&mut tree, 800.0, 600.0, [-1.0, -1.0]);
    }

    let (sub_open, on_layer, panel_left, panel_w) = {
        let tree = root.borrow();
        let sub_open = use_signal("mb_share_vis_bar__file__sub0__open", || false).peek();
        let layer = tree.children.iter().find(|c| {
            c.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID)
        });
        fn find_sub_panel(el: &Element) -> Option<&Element> {
            if el.attrs.get("data-slot").map(String::as_str) == Some("dropdown-menu-sub-content") {
                return Some(el);
            }
            el.children.iter().find_map(find_sub_panel)
        }
        let panel = layer.and_then(|l| l.children.iter().find_map(find_sub_panel));
        let on_layer = panel.is_some();
        let left = panel
            .and_then(|p| crate::layout::lpa_length_px(p.style.inset.left))
            .unwrap_or(-1.0);
        let w = panel.map(|p| p.layout_bounds[2]).unwrap_or(0.0);
        (sub_open, on_layer, left, w)
    };

    assert!(sub_open, "Share hover must set sub_open true");
    assert!(
        on_layer,
        "Share submenu panel must sit on the portal layer after hover+hoist"
    );
    assert!(
        panel_left > 40.0,
        "submenu must be placed to the right of Share (left={panel_left}), not stuck at 0/clipped"
    );
    assert!(
            panel_w > 8.0 || panel_left > 40.0,
            "submenu must have visible width or at least a placed left edge (w={panel_w}, left={panel_left})"
        );

    clear_test_reconcile_root();
    drop(root);
    handle.dispose();
    clear_overlay_stack();
    reset_registered_anchors();
}

/// Gallery stuck path: Share hover opens while reconcile misses (borrow / missing slot).
/// Without deferred remount, highlight sticks and no further enter fires — panel never appears.
#[test]
fn menubar_share_hover_recovers_after_reconcile_miss() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::app::portal_overlay::clear_overlay_stack;
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_effect, create_root, use_signal};

    clear_overlay_stack();
    reset_registered_anchors();
    clear_test_reconcile_root();

    let root: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&root));

    let (handle, parent_open) = create_root(|| {
        let parent_open = use_signal("mb_miss_bar__file__dd_open", || false);
        let root_e = Rc::clone(&root);
        create_effect(move || {
            let el = menubar_scope("mb_miss_bar", || {
                menubar_menu(Some("file".into()), Some(parent_open), || {
                    let trigger = MenubarTrigger(MenubarTriggerProps {
                        id: String::new(),
                        children: Some(Element::new().text("File")),
                        base: None,
                    });
                    let content = menubar_content(MenubarContentOpts::default(), || {
                        menubar_sub(None, || {
                            let st = MenubarSubTrigger(MenubarSubTriggerProps {
                                disabled: Some(false),
                                inset: Some(false),
                                children: Some(Element::new().text("Share")),
                                base: None,
                            });
                            let sc = menubar_sub_content(Default::default(), || {
                                MenubarItem(MenubarItemProps {
                                    disabled: Some(false),
                                    inset: Some(false),
                                    variant: None,
                                    on_click: None,
                                    children: Some(Element::new().text("Email link")),
                                    base: None,
                                })
                            });
                            let mut frag = Element::new();
                            frag.set_attr("jadix_fragment", "1");
                            frag.children(vec![st, sc])
                        })
                    });
                    let mut frag = Element::new();
                    frag.set_attr("jadix_fragment", "1");
                    frag.children(vec![trigger, content])
                })
            });
            *root_e.borrow_mut() = el;
        });
        parent_open
    });

    parent_open.set(true);
    {
        let mut tree = root.borrow_mut();
        let _ = crate::layout::layout_tree::hoist_portal_subtrees(&mut tree);
    }

    let hover_cb = {
        let tree = root.borrow();
        fn find_sub_hover(
            el: &Element,
        ) -> Option<std::rc::Rc<std::cell::RefCell<dyn FnMut(bool)>>> {
            if el.attrs.get("data-slot").map(String::as_str) == Some("dropdown-menu-sub-trigger") {
                return el.on_hover.clone();
            }
            for c in &el.children {
                if let Some(h) = find_sub_hover(c) {
                    return Some(h);
                }
            }
            None
        }
        find_sub_hover(&tree)
    };
    let hover_cb = hover_cb.expect("Share SubTrigger on_hover");

    // Simulate reconcile miss during open (e.g. AppState borrowed): drop the test root.
    clear_test_reconcile_root();
    hover_cb.borrow_mut()(true);
    let sub_open_after_miss = use_signal("mb_miss_bar__file__sub0__open", || false).peek();
    assert!(
        sub_open_after_miss,
        "hover must still set sub_open even when reconcile misses"
    );

    fn find_sub_panel(el: &Element) -> bool {
        if el.attrs.get("data-slot").map(String::as_str) == Some("dropdown-menu-sub-content") {
            return true;
        }
        el.children.iter().any(find_sub_panel)
    }
    assert!(
        !find_sub_panel(&root.borrow()),
        "panel must be absent while reconcile root is cleared (miss path)"
    );

    // Restore tree + drain deferred remount (same as CursorMoved/redraw after hover).
    install_test_reconcile_root(Rc::clone(&root));
    drain_deferred_view_remounts();
    {
        let mut tree = root.borrow_mut();
        let _ = crate::layout::layout_tree::hoist_portal_subtrees(&mut tree);
    }

    assert!(
        find_sub_panel(&root.borrow()),
        "deferred remount after reconcile miss must mount Share submenu panel"
    );

    clear_test_reconcile_root();
    drop(root);
    handle.dispose();
    clear_overlay_stack();
    reset_registered_anchors();
}
