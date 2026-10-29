use super::*;

#[test]
#[cfg(feature = "animation-base")]
fn rsx_dropdown_keeps_content_height_placement_and_shadow_through_expansion() {
    use crate::app::portal_overlay::clear_overlay_stack;
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::Signal;
    use crate::core::reactivity::{create_effect, create_root};
    use crate::layout::LayoutTree;

    fn find_bar<'a>(el: &'a Element, prefix: &str) -> Option<&'a Element> {
        if el
            .focus_id
            .as_deref()
            .is_some_and(|id| id.starts_with(prefix) && id.ends_with(":y"))
        {
            return Some(el);
        }
        el.children.iter().find_map(|child| find_bar(child, prefix))
    }

    clear_overlay_stack();
    reset_registered_anchors();
    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(live.clone());
    let (owner, (open, trigger_y)) = create_root(|| {
        let open = Signal::new(false);
        let trigger_y = Signal::new(1400.0_f32);
        let live = live.clone();
        create_effect(move || {
            *live.borrow_mut() = vstack().w(2500.0).h(1500.0).p(0.0).child(Element::new().h(trigger_y.get()).flex_shrink(0.0)).child(crate::ui! {
                    <DropdownMenu id="height-dropdown" open={open}>
                        <DropdownMenuTrigger>"Open"</DropdownMenuTrigger>
                        <DropdownMenuContent>
                            <DropdownMenuLabel>"Actions"</DropdownMenuLabel>
                            <DropdownMenuItem>"Copy"</DropdownMenuItem>
                            <DropdownMenuItem>"Paste"</DropdownMenuItem>
                            <DropdownMenuSeparator />
                            <DropdownMenuCheckboxItem defaultChecked={true}>"Show Bookmarks Bar"</DropdownMenuCheckboxItem>
                            <DropdownMenuCheckboxItem>"Show Full URLs"</DropdownMenuCheckboxItem>
                            <DropdownMenuSeparator />
                            <DropdownMenuRadioGroup defaultValue="pedro">
                                <DropdownMenuLabel>"People"</DropdownMenuLabel>
                                <DropdownMenuRadioItem value="pedro">"Pedro Duarte"</DropdownMenuRadioItem>
                                <DropdownMenuRadioItem value="colm">"Colm Tuite"</DropdownMenuRadioItem>
                            </DropdownMenuRadioGroup>
                            <DropdownMenuSeparator />
                            <DropdownMenuSub>
                                <DropdownMenuSubTrigger>"More Tools"</DropdownMenuSubTrigger>
                                <DropdownMenuSubContent>
                                    <DropdownMenuItem>"Save Page As..."</DropdownMenuItem>
                                    <DropdownMenuItem>"Developer Tools"</DropdownMenuItem>
                                </DropdownMenuSubContent>
                            </DropdownMenuSub>
                            <DropdownMenuSeparator />
                            <DropdownMenuItem variant="destructive">"Delete"</DropdownMenuItem>
                        </DropdownMenuContent>
                    </DropdownMenu>
                }).into();
        });
        (open, trigger_y)
    });
    let mut layout = LayoutTree::new();
    layout.build_and_layout(&mut live.borrow_mut(), 2500.0, 1500.0, [0.0, 0.0]);
    crate::style::utility::set_frame_dt(0.016);
    let viewport_h = std::cell::Cell::new(1500.0);
    let mut layout_frame = || {
        let mut root = live.borrow_mut();
        let mut rendered = layout.build_and_layout_fast_ex(
            &mut root,
            2500.0,
            viewport_h.get(),
            [0.0, 0.0],
            true,
            false,
            None,
        );
        crate::utils::floating_motion::flush_floating_motion_subtree(&mut root);
        crate::utils::floating_motion::tick_hoisted_floating_css(&mut root);
        if crate::utils::floating_motion::floating_motion_needs_relayout() {
            crate::utils::floating_motion::mark_floating_panels_layout_dirty(&root, &mut layout);
            rendered = layout.relayout_overlay_follow_and_collect(
                &mut root,
                2500.0,
                viewport_h.get(),
                [0.0, 0.0],
                true,
                false,
            );
        }
        rendered
    };

    for (y, height, expected_h, above) in [
        (1400.0, 1500.0, 326.0, true),
        (100.0, 1500.0, 326.0, false),
        (250.0, 300.0, 238.0, true),
        (1400.0, 1500.0, 326.0, true),
        (100.0, 1500.0, 326.0, false),
    ] {
        open.set(false);
        drain_deferred_view_remounts();
        let mut exit_h = 326.0;
        for frame in 0..20 {
            layout_frame();
            let root = live.borrow();
            crate::utils::popup::visit_floating_panels(&root, |panel| {
                if panel.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_CONTENT)
                    && panel.style.opacity >= 0.01
                {
                    let h = crate::layout::dim_length_px(panel.style.max_size.height)
                        .unwrap_or(panel.layout_bounds[3]);
                    assert!(
                        h <= exit_h + 0.5,
                        "closing must shrink smoothly, frame={frame}: h={h}, previous={exit_h}"
                    );
                    exit_h = h;
                    let track = find_bar(
                        panel,
                        crate::components::scrollbar::SCROLLBAR_TRACK_FOCUS_PREFIX,
                    )
                    .expect("menu scrollbar track during collapse");
                    assert!(track.layout_bounds[3] < 0.5,
                            "collapse must not expose a scrollbar hit slab, frame={frame}, h={h}, track={:?}, attrs={:?}, animations={:?}", track.layout_bounds, track.attrs, panel.style.animations);
                }
            });
            drop(root);
            drain_deferred_view_remounts();
        }
        viewport_h.set(height);
        trigger_y.set(y);
        open.set(true);
        drain_deferred_view_remounts();
        // Simulate the pointer showing the auto scrollbar while the menu opens.
        crate::style::use_ui_transition_cubic("scrollbar_spring_height-dropdown__dd_scroll:y", 0.0)
            .set(1.0);
        crate::style::tick_animations(0.08);
        crate::style::tick_animations(0.08);
        let mut previous_h = 0.0;
        for frame in 0..64 {
            if frame == 24 {
                take_activate("height-dropdown", 6)
                    .expect("submenu trigger")
                    .as_ref()();
                drain_deferred_view_remounts();
            }
            let rendered = layout_frame();
            let root = live.borrow();
            let mut panels = 0;
            let mut sub_panels = 0;
            crate::utils::popup::visit_floating_panels(&root, |panel| {
                if panel.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_CONTENT) {
                    panels += 1;
                    if panel.style.opacity >= 0.01 {
                        let h = crate::layout::dim_length_px(panel.style.max_size.height)
                            .unwrap_or(panel.layout_bounds[3])
                            .min(panel.layout_bounds[3]);
                        assert!(h <= expected_h + 0.5 && h + 0.5 >= previous_h,
                            "height reveal must grow smoothly to the rows, y={y}, frame={frame}, h={h}, previous={previous_h}");
                        previous_h = h;
                        let track = find_bar(
                            panel,
                            crate::components::scrollbar::SCROLLBAR_TRACK_FOCUS_PREFIX,
                        )
                        .expect("menu scrollbar track");
                        if h < expected_h - 0.5 {
                            assert!(track.layout_bounds[3] < 0.5,
                                    "animation clipping must not expose a scrollbar hit slab, frame={frame}, h={h}, track={:?}", track.layout_bounds);
                        }
                        let seam = if above {
                            panel.layout_bounds[1] + h
                        } else {
                            panel.layout_bounds[1]
                        };
                        let expected_seam = if above { y - 4.0 } else { y + 44.0 };
                        assert!((seam - expected_seam).abs() < 0.5,
                            "expansion must stay attached to the trigger, y={y}, frame={frame}: {:?} clip={h}", panel.layout_bounds);
                    }
                    if frame >= 20 {
                        assert!((panel.layout_bounds[3] - expected_h).abs() < 0.5,
                            "dropdown must hug its rows after expansion, frame={frame}: {:?} content={:?}",
                            panel.layout_bounds, panel.attrs.get(crate::utils::popup::ATTR_FLOATING_CONTENT_H));
                        let track = find_bar(
                            panel,
                            crate::components::scrollbar::SCROLLBAR_TRACK_FOCUS_PREFIX,
                        )
                        .unwrap();
                        if expected_h < 326.0 {
                            assert!(
                                track.layout_bounds[3] > 1.0,
                                "genuine overflow must restore the scrollbar after expansion"
                            );
                            let thumb = find_bar(
                                panel,
                                crate::components::scrollbar::SCROLLBAR_THUMB_FOCUS_PREFIX,
                            )
                            .unwrap();
                            assert!(
                                rendered.instances.iter().any(|instance| {
                                    (instance.rect_pos[0] - thumb.layout_bounds[0]).abs() < 2.5
                                        && (instance.rect_pos[1] - thumb.layout_bounds[1]).abs()
                                            < 2.5
                                        && (instance.rect_size[1] - thumb.layout_bounds[3]).abs()
                                            < 2.5
                                        && instance.rect_size[0] > 1.0
                                        && instance.color[3] > 0.5
                                }),
                                "scrollbar must paint again for viewport-constrained menus"
                            );
                        } else {
                            assert!(
                                track.layout_bounds[3] < 0.5,
                                "fitting menus must stay free of scrollbars"
                            );
                        }
                    }
                }
                if panel.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_SUB_CONTENT) {
                    sub_panels += 1;
                    if frame >= 44 {
                        assert!(
                            (panel.layout_bounds[3] - 66.0).abs() < 0.5,
                            "submenu must hug its own rows: {:?}",
                            panel.layout_bounds
                        );
                        assert!(
                            panel.layout_bounds[0] >= 180.0
                                && panel.layout_bounds[1] >= 8.0
                                && panel.layout_bounds[1] + panel.layout_bounds[3] <= height - 7.5,
                            "submenu must remain beside its trigger and inside the viewport: {:?}",
                            panel.layout_bounds
                        );
                    }
                    if frame == 28 {
                        let shadow = rendered
                            .instances
                            .iter()
                            .find(|instance| {
                                (instance.rect_pos[0] - panel.layout_bounds[0]).abs() < 0.5
                                    && (instance.rect_pos[1] - panel.layout_bounds[1]).abs() < 0.5
                                    && instance.shadow_color[3] > 0.0
                            })
                            .expect("submenu shadow must be painted during expansion");
                        assert!(
                            shadow.shadow_color[3] < panel.style.shadow_color[3],
                            "painted shadow must fade in instead of appearing at full strength"
                        );
                        assert!(
                            panel.style.opacity > 0.0 && panel.style.opacity < 0.99,
                            "submenu shadow must fade in with the expansion, opacity={}",
                            panel.style.opacity
                        );
                    }
                }
            });
            assert_eq!(panels, 1);
            assert_eq!(sub_panels, usize::from(frame >= 24));
        }
    }
    crate::style::utility::set_frame_dt(0.0);
    clear_test_reconcile_root();
    drop(live);
    owner.dispose();
    reset_registered_anchors();
    clear_overlay_stack();
}

#[test]
fn dropdown_builder_sets_anchor() {
    let open = use_signal("dd-test-open", || false);
    let el: Element = dropdown(
        "dd-test",
        || Element::new().focus_id("dd-test__trigger").text("t"),
        || Element::new().text("menu"),
    )
    .open_signal(open)
    .anchor_focus_id("dd-test__trigger")
    .into();
    assert_eq!(
        el.template_id.as_deref(),
        Some("dd-test__dropdown_menu"),
        "dropdown must expose a stable dropdown_menu scope template"
    );
}

#[test]
fn compound_scope_exposes_template() {
    let el = dropdown_menu_scope("dd-scope", None, || {
        let trigger = DropdownMenuTrigger(DropdownMenuTriggerProps {
            id: String::new(),
            children: Some(Element::new().text("Open")),
            base: None,
        });
        let content = dropdown_menu_content(DropdownMenuContentOpts::default(), || {
            DropdownMenuItem(DropdownMenuItemProps {
                disabled: Some(false),
                inset: Some(false),
                variant: None,
                on_click: None,
                children: Some(Element::new().text("One")),
                base: None,
            })
        });
        let mut frag = Element::new();
        frag.set_attr("jadix_fragment", "1");
        frag.children(vec![trigger, content])
    });
    assert_eq!(
        el.template_id.as_deref(),
        Some("dd-scope__dropdown_menu"),
        "scope must expose a stable template id"
    );
}

#[test]
fn compound_sub_provides_context_before_children() {
    let _el = dropdown_menu_scope("dd-sub", None, || {
        dropdown_menu_content(DropdownMenuContentOpts::default(), || {
            dropdown_menu_sub(None, || {
                let st = DropdownMenuSubTrigger(DropdownMenuSubTriggerProps {
                    disabled: Some(false),
                    inset: Some(false),
                    children: Some(Element::new().text("More")),
                    base: None,
                });
                let sc = dropdown_menu_sub_content(Default::default(), || {
                    DropdownMenuItem(DropdownMenuItemProps {
                        disabled: Some(false),
                        inset: Some(false),
                        variant: None,
                        on_click: None,
                        children: Some(Element::new().text("Nested")),
                        base: None,
                    })
                });
                let mut frag = Element::new();
                frag.set_attr("jadix_fragment", "1");
                frag.children(vec![st, sc])
            })
        })
    });
}

/// Enter on SubTrigger opens the sub and highlights its first item; Esc/← close
/// restores parent-only highlighting (sub active cleared).
#[test]
fn keyboard_enter_sub_focuses_first_item_then_escape_clears_sub_active() {
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

    let (handle, parent_open) = create_root(|| {
        let parent_open = use_signal("dd_kb_sub__dd_open", || false);
        let root_e = Rc::clone(&root);
        create_effect(move || {
            let el = dropdown_menu_scope("dd_kb_sub", Some(parent_open), || {
                let trigger = DropdownMenuTrigger(DropdownMenuTriggerProps {
                    id: String::new(),
                    children: Some(Element::new().text("Open")),
                    base: None,
                });
                let content = dropdown_menu_content(DropdownMenuContentOpts::default(), || {
                    let _ = DropdownMenuItem(DropdownMenuItemProps {
                        disabled: Some(false),
                        inset: Some(false),
                        variant: None,
                        on_click: None,
                        children: Some(Element::new().text("Above")),
                        base: None,
                    });
                    dropdown_menu_sub(None, || {
                        let st = DropdownMenuSubTrigger(DropdownMenuSubTriggerProps {
                            disabled: Some(false),
                            inset: Some(false),
                            children: Some(Element::new().text("More")),
                            base: None,
                        });
                        let sc = dropdown_menu_sub_content(Default::default(), || {
                            DropdownMenuItem(DropdownMenuItemProps {
                                disabled: Some(false),
                                inset: Some(false),
                                variant: None,
                                on_click: None,
                                children: Some(Element::new().text("Nested")),
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
            *root_e.borrow_mut() = el;
        });
        parent_open
    });

    parent_open.set(true);
    {
        let mut tree = root.borrow_mut();
        let _ = crate::layout::layout_tree::hoist_portal_subtrees(&mut tree);
    }

    // Simulate Enter on SubTrigger (ix 1 after "Above").
    let parent_active = use_signal("dd_kb_sub__dd_active", || ACTIVE_NONE);
    set_dd_active(parent_active, 1);
    assert!(
        dropdown_menu_active_is_sub_trigger("dd_kb_sub", 1),
        "Share/More row must register as a sub trigger"
    );
    if let Some(cb) = take_activate("dd_kb_sub", 1) {
        cb();
    }
    assert_eq!(
        dropdown_menu_open_sub_id("dd_kb_sub").as_deref(),
        Some("dd_kb_sub__sub0")
    );
    let sub_active = use_signal("dd_kb_sub__sub0__dd_active", || ACTIVE_NONE).peek();
    assert_eq!(sub_active, 0, "Enter must highlight the submenu first item");
    assert_eq!(
        parent_active.peek(),
        1,
        "parent SubTrigger highlight must remain while sub is open"
    );

    close_open_submenu_keep_parent_highlight("dd_kb_sub__sub0");
    assert!(dropdown_menu_open_sub_id("dd_kb_sub").is_none());
    assert_eq!(
        use_signal("dd_kb_sub__sub0__dd_active", || ACTIVE_NONE).peek(),
        ACTIVE_NONE
    );
    assert_eq!(
        parent_active.peek(),
        1,
        "closing sub must keep parent highlight on the SubTrigger"
    );

    clear_test_reconcile_root();
    drop(root);
    handle.dispose();
    clear_overlay_stack();
    reset_registered_anchors();
}
