use super::*;

#[test]
fn group_label_centers_navigation_on_both_axes() {
    let (_owner, mut root) = crate::core::reactivity::create_root(|| {
        SidebarGroupLabel(SidebarGroupLabelProps {
            children: Some(Element::from("导航")),
            base: None,
        })
        .w(240.0)
    });
    let mut layout = LayoutTree::new();
    let _ = layout.build_and_layout(&mut root, 240.0, 32.0, [-1.0, -1.0]);
    let bounds = root.layout_bounds;
    let label = root.children.first().expect("navigation label");
    let text = label.layout_bounds;
    assert!((bounds[0] + bounds[2] * 0.5 - text[0] - text[2] * 0.5).abs() < 0.5);
    assert!((bounds[1] + bounds[3] * 0.5 - text[1] - text[3] * 0.5).abs() < 0.5);
}

fn responsive_frame(root: &mut Element, layout: &mut LayoutTree, width: f32) -> LayoutStatus {
    crate::utils::modal_motion::set_modal_viewport_size(width, 600.0);
    let mut impact = LayoutStatus::Clean;
    impact = impact.merge(crate::authoring::presentation::sync_tree(
        root,
        layout,
        crate::authoring::presentation::Context::new(
            crate::authoring::presentation::Phase::State,
            [width, 600.0],
        ),
    ));
    crate::layout::layout_tree::hoist_portal_subtrees(root);
    crate::style::sync_class_variants_subtree(
        root,
        [-1.0, -1.0],
        "",
        false,
        width,
        600.0,
        layout,
        &mut impact,
    );
    layout.build_and_layout(root, width, 600.0, [-1.0, -1.0]);
    impact
}

#[test]
#[cfg(feature = "animation-base")]
fn responsive_sidebar_breakpoint_preserves_exit_start_width() {
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    crate::utils::modal_motion::set_modal_viewport_size(1000.0, 600.0);
    let (_owner, _) = crate::core::reactivity::create_root(|| {
        let mut root: Element = sidebar("breakpoint-nav")
            .provider(true)
            .collapsible("icon")
            .item(sidebar_menu_button("breakpoint-action").text("Navigation"))
            .into();
        let mut layout = LayoutTree::new();
        responsive_frame(&mut root, &mut layout, 1000.0);
        let width = sidebar_node_mut(&mut root, ATTR_SLOT, SLOT_ROOT)
            .unwrap()
            .layout_bounds[2];
        assert!(width > 200.0);
        responsive_frame(&mut root, &mut layout, 740.0);
        let rail = sidebar_node_mut(&mut root, ATTR_SLOT, SLOT_ROOT).unwrap();
        assert!(
            (rail.layout_bounds[2] - width).abs() < 0.1,
            "breakpoint exit must start at the previous width, not disappear: {:?}",
            rail.layout_bounds
        );
        assert_eq!(rail.style.display, taffy::style::Display::Flex);
        crate::style::advance_css_transition_timeline_ms(100.0);
        responsive_frame(&mut root, &mut layout, 740.0);
        let middle = sidebar_node_mut(&mut root, ATTR_SLOT, SLOT_ROOT)
            .unwrap()
            .layout_bounds[2];
        assert!(middle > 0.0 && middle < width);
        responsive_frame(&mut root, &mut layout, 1000.0);
        let reversed = sidebar_node_mut(&mut root, ATTR_SLOT, SLOT_ROOT)
            .unwrap()
            .layout_bounds[2];
        assert!((reversed - middle).abs() < 0.1);
        crate::style::advance_css_transition_timeline_ms(100.0);
        responsive_frame(&mut root, &mut layout, 1000.0);
        let returning = sidebar_node_mut(&mut root, ATTR_SLOT, SLOT_ROOT)
            .unwrap()
            .layout_bounds[2];
        assert!(returning > middle && returning < width);
        responsive_frame(&mut root, &mut layout, 740.0);
        assert!(
            (sidebar_node_mut(&mut root, ATTR_SLOT, SLOT_ROOT)
                .unwrap()
                .layout_bounds[2]
                - returning)
                .abs()
                < 0.1
        );
        crate::style::advance_css_transition_timeline_ms(200.0);
        responsive_frame(&mut root, &mut layout, 740.0);
        assert_eq!(
            sidebar_node_mut(&mut root, ATTR_SLOT, SLOT_ROOT)
                .unwrap()
                .style
                .display,
            taffy::style::Display::None
        );
        assert!(sidebar_state("breakpoint-nav__provider").open.peek());
        responsive_frame(&mut root, &mut layout, 1000.0);
        let rail = sidebar_node_mut(&mut root, ATTR_SLOT, SLOT_ROOT).unwrap();
        assert_eq!(rail.style.display, taffy::style::Display::Flex);
        assert!(rail.layout_bounds[2] < 4.0);
        crate::style::advance_css_transition_timeline_ms(200.0);
        responsive_frame(&mut root, &mut layout, 1000.0);
        assert!(
            (sidebar_node_mut(&mut root, ATTR_SLOT, SLOT_ROOT)
                .unwrap()
                .layout_bounds[2]
                - width)
                .abs()
                < 0.1
        );
        crate::style::set_prefers_reduced_motion(Some(true));
        responsive_frame(&mut root, &mut layout, 740.0);
        assert_eq!(
            sidebar_node_mut(&mut root, ATTR_SLOT, SLOT_ROOT)
                .unwrap()
                .style
                .display,
            taffy::style::Display::None
        );
        responsive_frame(&mut root, &mut layout, 1000.0);
        assert!(
            (sidebar_node_mut(&mut root, ATTR_SLOT, SLOT_ROOT)
                .unwrap()
                .layout_bounds[2]
                - width)
                .abs()
                < 0.1
        );
        crate::style::set_prefers_reduced_motion(None);
    });
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
fn responsive_sidebar_sheet_keeps_labels_after_desktop_exit() {
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    crate::utils::modal_motion::set_modal_viewport_size(1000.0, 600.0);
    crate::app::portal_overlay::clear_overlay_stack();
    let (_owner, _) = crate::core::reactivity::create_root(|| {
        let mut root: Element = sidebar("breakpoint-sheet")
            .provider(true)
            .collapsible("icon")
            .item(sidebar_menu_button("breakpoint-sheet-action").text("Navigation"))
            .into();
        let mut layout = LayoutTree::new();
        responsive_frame(&mut root, &mut layout, 1000.0);
        responsive_frame(&mut root, &mut layout, 740.0);
        crate::style::advance_css_transition_timeline_ms(200.0);
        responsive_frame(&mut root, &mut layout, 740.0);
        let ctx = sidebar_state("breakpoint-sheet__provider");
        ctx.open_mobile.set(true);
        for _ in 0..3 {
            responsive_frame(&mut root, &mut layout, 740.0);
            let panel =
                sidebar_node_mut(&mut root, ATTR_MOBILE_PANEL, "breakpoint-sheet__mobile").unwrap();
            let mut found = false;
            crate::core::tree::walk_elements(panel, |node| {
                if node.text.as_deref() == Some("Navigation") {
                    found = true;
                    assert!(
                        node.style.opacity > 0.99,
                        "desktop width must not hide sheet labels"
                    );
                    assert!(
                        node.style.size.width.is_auto(),
                        "sheet labels must not retain zero width"
                    );
                }
            });
            assert!(found);
        }
        ctx.open_mobile.set(false);
        responsive_frame(&mut root, &mut layout, 1000.0);
    });
    crate::style::set_css_transition_virtual_timeline(false);
    crate::app::portal_overlay::clear_overlay_stack();
}

#[test]
#[cfg(feature = "animation-base")]
fn responsive_sidebar_inset_margin_has_intermediate_layout() {
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let (_owner, _) = crate::core::reactivity::create_root(|| {
        let mut root = SidebarInset(SidebarInsetProps {
            variant: Some("inset".into()),
            children: None,
            base: None,
        });
        let mut layout = LayoutTree::new();
        responsive_frame(&mut root, &mut layout, 1000.0);
        assert!((root.layout_bounds[0] - 8.0).abs() < 0.1);
        responsive_frame(&mut root, &mut layout, 740.0);
        assert!((root.layout_bounds[0] - 8.0).abs() < 0.1);
        crate::style::advance_css_transition_timeline_ms(100.0);
        responsive_frame(&mut root, &mut layout, 740.0);
        assert!(root.layout_bounds[0] > 0.0 && root.layout_bounds[0] < 8.0);
        crate::style::advance_css_transition_timeline_ms(100.0);
        responsive_frame(&mut root, &mut layout, 740.0);
        assert!(root.layout_bounds[0].abs() < 0.1);
        responsive_frame(&mut root, &mut layout, 1000.0);
        assert!(root.layout_bounds[0].abs() < 0.1);
        crate::style::advance_css_transition_timeline_ms(200.0);
        responsive_frame(&mut root, &mut layout, 1000.0);
        assert!((root.layout_bounds[0] - 8.0).abs() < 0.1);
    });
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
fn responsive_sidebar_animation_finishes_through_class_resolution() {
    use crate::utils::modal_motion::{flush_modal_shell_motion_subtree, tick_hoisted_modal_css};
    crate::app::portal_overlay::clear_overlay_stack();
    crate::style::utility::set_frame_dt(1.0 / 60.0);
    let (_owner, _) = crate::core::reactivity::create_root(|| {
        let mut root: Element = sidebar("animated-mobile")
            .provider(true)
            .collapsible("icon")
            .header(sidebar_menu_button("animated-title").text("Jadix Gallery"))
            .item(sidebar_menu_button("animated-nav").text("Navigation"))
            .into();
        let ctx = sidebar_state("animated-mobile__provider");
        let mut layout = LayoutTree::new();
        responsive_frame(&mut root, &mut layout, 600.0);
        ctx.open_mobile.set(true);
        for _ in 0..40 {
            responsive_frame(&mut root, &mut layout, 600.0);
            flush_modal_shell_motion_subtree(&mut root);
            tick_hoisted_modal_css(&mut root);
        }
        let panel =
            sidebar_node_mut(&mut root, ATTR_MOBILE_PANEL, "animated-mobile__mobile").unwrap();
        assert!(
            panel.style.translate_pct[0].abs() < 0.001,
            "open must reach the viewport instead of restarting every frame: {:?}",
            panel.style.translate_pct
        );
        let (close_bounds, close_position) = {
            let close = sidebar_node_mut(panel, "data-sidebar-mobile-close", "true").unwrap();
            (close.layout_bounds, close.style.position)
        };
        let title = sidebar_node_mut(panel, ATTR_SLOT, SLOT_MENU_BUTTON).unwrap();
        let title_bounds = title.layout_bounds;
        assert_eq!(close_position, taffy::style::Position::Relative);
        assert!(close_bounds[0] > title_bounds[0]);
        assert!(close_bounds[1] < 48.0, "close control: {close_bounds:?}");
        assert!(
            (close_bounds[1] - title_bounds[1]).abs() < 1.0,
            "close and title must share a row: close={close_bounds:?}, title={title_bounds:?}"
        );
        assert!(
                close_bounds[0] + close_bounds[2] > title_bounds[0] + title_bounds[2],
                "close must stay to the right of the title: close={close_bounds:?}, title={title_bounds:?}"
            );
        ctx.open_mobile.set(false);
        for _ in 0..40 {
            responsive_frame(&mut root, &mut layout, 600.0);
            flush_modal_shell_motion_subtree(&mut root);
            tick_hoisted_modal_css(&mut root);
        }
        if let Some(panel) =
            sidebar_node_mut(&mut root, ATTR_MOBILE_PANEL, "animated-mobile__mobile")
        {
            assert!(
                panel.style.translate_pct[0] < -0.999,
                "exit must finish sliding offscreen"
            );
        }
        dismiss_viewport_modal("animated-mobile__mobile", ctx.open_mobile);
    });
    crate::app::portal_overlay::clear_overlay_stack();
}

#[test]
fn responsive_sidebar_moves_live_content_and_preserves_desktop_preference() {
    use crate::core::reactivity::create_root;
    use std::rc::Rc;
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    crate::app::portal_overlay::clear_overlay_stack();
    crate::utils::modal_motion::set_modal_viewport_size(1024.0, 600.0);
    let (_owner, _) = create_root(|| {
        let desktop = Signal::new(false);
        let clicks = Rc::new(Cell::new(0));
        let count = clicks.clone();
        let mut root = sidebar_provider_scope(
            "responsive-provider",
            Some(desktop),
            true,
            Some("inset"),
            || {
                let nav: Element = sidebar("responsive-nav")
                    .collapsible("icon")
                    .item(
                        sidebar_menu_button("responsive-action")
                            .text("Navigation")
                            .on_click(move || count.set(count.get() + 1)),
                    )
                    .into();
                let trigger = SidebarTrigger(SidebarTriggerProps {
                    id: "responsive-trigger".into(),
                    on_click: None,
                    children: None,
                    base: None,
                });
                let inset = SidebarInset(SidebarInsetProps {
                    variant: Some("inset".into()),
                    children: Some(trigger),
                    base: None,
                });
                let mut fragment = Element::new();
                fragment.set_attr("jadix_fragment", "1");
                fragment.children(vec![nav, inset])
            },
        );
        let ctx = sidebar_state("responsive-provider");
        let mut layout = LayoutTree::new();
        responsive_frame(&mut root, &mut layout, 1024.0);
        crate::utils::modal_motion::set_modal_viewport_size(767.5, 600.0);
        assert!(
            !ctx.is_mobile.peek(),
            "resize has not reached frame sync yet"
        );
        ctx.toggle();
        assert!(
            ctx.open_mobile.peek(),
            "the first narrow-screen click opens the sheet"
        );
        assert!(
            !desktop.peek(),
            "mobile toggle must preserve desktop preference"
        );
        ctx.open_mobile.set(false);
        crate::utils::modal_motion::set_modal_viewport_size(1024.0, 600.0);
        responsive_frame(&mut root, &mut layout, 1024.0);
        let id = "responsive-nav__mobile";
        let action = sidebar_node_mut(&mut root, "data-slot", SLOT_MENU_BUTTON).unwrap();
        let original_handler = action.on_click.clone().unwrap();
        for width in [767.5, 320.0, 639.5, 640.0, 767.99] {
            responsive_frame(&mut root, &mut layout, width);
            crate::style::advance_css_transition_timeline_ms(200.0);
            responsive_frame(&mut root, &mut layout, width);
            assert!(ctx.is_mobile.peek());
            assert!(!ctx.open_mobile.peek());
            assert_eq!(
                sidebar_node_mut(&mut root, ATTR_MOBILE_ID, id)
                    .unwrap()
                    .style
                    .display,
                taffy::style::Display::None
            );
            let inset = sidebar_node_mut(&mut root, ATTR_SLOT, SLOT_INSET).unwrap();
            assert!(
                inset.layout_bounds[0].abs() < 0.5 && (inset.layout_bounds[2] - width).abs() < 0.5
            );
            let trigger = sidebar_node_mut(&mut root, ATTR_SLOT, SLOT_TRIGGER).unwrap();
            (trigger.on_click.as_ref().unwrap().borrow_mut())();
            assert_eq!(
                responsive_frame(&mut root, &mut layout, width),
                LayoutStatus::StructureChanged
            );
            assert!(
                !desktop.peek(),
                "mobile toggle must not expand the desktop rail"
            );
            let panel = sidebar_node_mut(&mut root, ATTR_MOBILE_PANEL, id).unwrap();
            let mut close_count = 0;
            crate::core::tree::walk_elements(panel, |node| {
                if node
                    .attrs
                    .get("data-sidebar-mobile-close")
                    .map(String::as_str)
                    == Some("true")
                {
                    close_count += 1;
                }
            });
            assert_eq!(close_count, 1, "each opening has exactly one close control");
            assert!((panel.layout_bounds[2] - 288.0).abs() < 0.5);
            assert!(
                panel.layout_bounds[0] >= -0.5
                    && panel.layout_bounds[0] + panel.layout_bounds[2] <= width + 0.5
            );
            let action = sidebar_node_mut(panel, ATTR_SLOT, SLOT_MENU_BUTTON).unwrap();
            assert!(
                action.layout_bounds[1] >= 48.0,
                "content must clear the close control"
            );
            assert!(Rc::ptr_eq(
                action.on_click.as_ref().unwrap(),
                &original_handler
            ));
            (action.on_click.as_ref().unwrap().borrow_mut())();
            assert!(!crate::utils::focus_trap::focus_trap_ring_ids().is_empty());
            if width == 767.5 {
                let parent_ring = crate::utils::focus_trap::focus_trap_ring_ids();
                let nested = Signal::new(true);
                crate::utils::modal_shell::modal_sync("responsive-nested", nested);
                crate::utils::sync_focus_trap("nested", true, "dialog-control");
                responsive_frame(&mut root, &mut layout, width);
                assert_eq!(
                    crate::utils::focus_trap::focus_trap_ring_ids(),
                    vec!["dialog-control"]
                );
                nested.set(false);
                crate::utils::modal_shell::modal_sync("responsive-nested", nested);
                crate::utils::sync_focus_trap("nested", false, "");
                responsive_frame(&mut root, &mut layout, width);
                assert_eq!(crate::utils::focus_trap::focus_trap_ring_ids(), parent_ring);
            }
            if width == 320.0 {
                crate::utils::popup::arm_underlay_close();
                let overlay = crate::core::tree::find_by_template_id_mut(
                    &mut root,
                    &format!("{id}__modal_overlay"),
                )
                .unwrap();
                (overlay.on_click.as_ref().unwrap().borrow_mut())();
            } else {
                assert!(crate::app::portal_overlay::try_close_top());
            }
            responsive_frame(&mut root, &mut layout, width);
            assert!(!ctx.open_mobile.peek());
            assert!(
                sidebar_node_mut(&mut root, ATTR_MOBILE_PANEL, id).is_some()
                    == cfg!(feature = "animation-base"),
                "only animated exit retains mobile content"
            );
            crate::utils::modal_motion::finish_modal_exit_for_test(
                &crate::utils::modal_motion::use_modal_presence(id),
            );
            responsive_frame(&mut root, &mut layout, width);
            assert!(sidebar_node_mut(&mut root, ATTR_MOBILE_PANEL, id).is_none());
            let rail = sidebar_node_mut(&mut root, ATTR_MOBILE_ID, id).unwrap();
            assert!(
                sidebar_node_mut(rail, "data-sidebar-mobile-header", "true").is_none(),
                "generated mobile header must not remain in the rail"
            );
            assert!(crate::utils::focus_trap::focus_trap_ring_ids().is_empty());
        }
        assert_eq!(clicks.get(), 5);
        ctx.toggle();
        responsive_frame(&mut root, &mut layout, 375.0);
        responsive_frame(&mut root, &mut layout, 768.0);
        crate::style::advance_css_transition_timeline_ms(200.0);
        responsive_frame(&mut root, &mut layout, 768.0);
        assert!(!ctx.is_mobile.peek() && !ctx.open_mobile.peek());
        assert!(!desktop.peek());
        assert!(sidebar_node_mut(&mut root, ATTR_MOBILE_PANEL, id).is_none());
        assert!(
            !crate::utils::viewport_modal::viewport_modal_motion_busy(id),
            "returning to desktop must cancel pending enter motion"
        );
        assert!(crate::app::portal_overlay::top_stacked_panel_z().is_none());
        let rail = sidebar_node_mut(&mut root, ATTR_MOBILE_ID, id).unwrap();
        assert!(
            (rail.layout_bounds[2] - 48.0).abs() < 0.5,
            "rail bounds={:?}, width={:?}, display={:?}",
            rail.layout_bounds,
            rail.style.size.width,
            rail.style.display
        );
        ctx.toggle();
        assert!(
            desktop.peek(),
            "desktop toggle must use the externally controlled signal"
        );
    });
    crate::style::set_css_transition_virtual_timeline(false);
    crate::utils::modal_motion::set_modal_viewport_size(800.0, 600.0);
    crate::app::portal_overlay::clear_overlay_stack();
}

#[test]
fn responsive_sidebar_builder_provider_right_side_and_fixed_escape_hatch() {
    use crate::core::reactivity::create_root;
    crate::utils::modal_motion::set_modal_viewport_size(320.0, 600.0);
    let (_owner, _) = create_root(|| {
        let mut root: Element = sidebar("responsive-builder")
            .provider(true)
            .side("right")
            .collapsible("offcanvas")
            .header(SidebarRail(SidebarRailProps {
                on_click: None,
                children: None,
                base: None,
            }))
            .item(sidebar_menu_button("builder-item").text("Item"))
            .into();
        let ctx = sidebar_state("responsive-builder__provider");
        assert!(ctx.is_mobile.peek());
        ctx.toggle();
        responsive_frame(&mut root, &mut LayoutTree::new(), 320.0);
        assert!(!crate::utils::focus_trap::focus_trap_ring_ids()
            .iter()
            .any(|id| id.ends_with("__sidebar_rail")));
        let panel =
            sidebar_node_mut(&mut root, ATTR_MOBILE_PANEL, "responsive-builder__mobile").unwrap();
        assert!(
            (panel.layout_bounds[0] - 32.0).abs() < 0.5,
            "panel bounds={:?}",
            panel.layout_bounds
        );
        let close = sidebar_node_mut(
            panel,
            "aria-label",
            &crate::tr_default_zh("jadix.sidebar.close", "Close sidebar", "关闭侧栏"),
        )
        .unwrap();
        (close.on_click.as_ref().unwrap().borrow_mut())();
        assert!(!ctx.open_mobile.peek());
        let mut fixed: Element = sidebar("responsive-fixed")
            .provider(true)
            .collapsible("none")
            .into();
        responsive_frame(&mut fixed, &mut LayoutTree::new(), 320.0);
        assert!(sidebar_node_mut(&mut fixed, ATTR_MOBILE_ID, "responsive-fixed__mobile").is_none());
        let rail = sidebar_node_mut(&mut fixed, ATTR_SLOT, SLOT_ROOT).unwrap();
        assert!((rail.layout_bounds[2] - 256.0).abs() < 0.5);
    });
    crate::utils::modal_motion::set_modal_viewport_size(800.0, 600.0);
    crate::app::portal_overlay::clear_overlay_stack();
}

fn sidebar_width_lerp(from: f32, to: f32, elapsed_ms: f32) -> f32 {
    let t = (elapsed_ms / SIDEBAR_WIDTH_TRANSITION_MS).clamp(0.0, 1.0);
    from + (to - from) * t
}

fn collapse_t_from_anim(
    from_px: f32,
    to_px: f32,
    elapsed_ms: f32,
    expanded_px: f32,
    collapsed_px: f32,
) -> f32 {
    collapse_t_from_width(
        sidebar_width_lerp(from_px, to_px, elapsed_ms),
        expanded_px,
        collapsed_px,
    )
}

#[test]
fn sidebar_default_width_is_16rem() {
    let el: Element = sidebar("t").into();
    assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
    assert_eq!(el.template_id.as_deref(), Some("t"));
}

#[test]
fn builder_header_and_menu_button_style() {
    let el: Element = sidebar("t-chrome")
        .header_with(|h| {
            h.class("px-2").style(|b| b.gap(8.0)).child(
                sidebar_menu_button("nav-a")
                    .text("Hi")
                    .style(|b| b.text_size(20.0)),
            )
        })
        .into();
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(&el, 20.0),
        "menu button .style(|b| b.text_size(20)) must land on the tree"
    );
}

#[test]
fn menu_button_text_uses_tailwind_text_sm_line_height() {
    use crate::style::TAILWIND_TEXT_SM_LEADING_SCALE;
    fn find_label<'a>(el: &'a Element) -> Option<&'a Element> {
        if el.text.as_deref() == Some("Gallery") {
            return Some(el);
        }
        el.children.iter().find_map(find_label)
    }

    let mut el: Element = sidebar_menu_button("nav-a").text("Gallery").into();
    assert_eq!(
        el.text_style.line_height_scale,
        Some(TAILWIND_TEXT_SM_LEADING_SCALE),
        "token lives on the button root (`text-sm`)"
    );
    assert!(el.text_style.line_height_specified());
    let label = find_label(&el).expect("label");
    assert!(
        (label.text_style.size_resolved() - 14.0).abs() < 0.01,
        "text-sm size"
    );
    assert!(
        !label.text_style.line_height_specified(),
        "caption leaf must inherit, not stamp, line-height"
    );

    crate::layout::layout_tree::cascade_inheritable_text_style_for_test(&mut el);
    let label = find_label(&el).expect("label after cascade");
    assert_eq!(
        label.text_style.line_height_scale,
        Some(TAILWIND_TEXT_SM_LEADING_SCALE),
        "ShadCN menu button is `text-sm` (line-height 1.25rem), not Label leading-none"
    );
}

#[test]
fn nested_label_leading_none_inherits_menu_button_text_sm() {
    use crate::style::TAILWIND_TEXT_SM_LEADING_SCALE;
    use crate::utils::text_node::apply_label_defaults;
    fn find_label<'a>(el: &'a Element) -> Option<&'a Element> {
        if el.text.as_deref() == Some("Gallery") {
            return Some(el);
        }
        el.children.iter().find_map(find_label)
    }

    let mut el: Element = sidebar_menu_button("nav-a")
        .child(apply_label_defaults(
            Element::new().text("Gallery".to_string()).text_size(14.0),
        ))
        .into();
    let label = find_label(&el).expect("label");
    assert!(
        !label.text_style.line_height_specified(),
        "paint must release Label leading-none so the button token can inherit"
    );

    crate::layout::layout_tree::cascade_inheritable_text_style_for_test(&mut el);
    let label = find_label(&el).expect("label after cascade");
    assert_eq!(
        label.text_style.line_height_scale,
        Some(TAILWIND_TEXT_SM_LEADING_SCALE),
        "nested Label defaults must not keep a 1em strut under `text-sm`"
    );
}

#[test]
fn sidebar_uses_sidebar_semantic_bg_not_muted() {
    let el: Element = sidebar("t").into();
    assert_eq!(el.style.bg_color, parse_color("sidebar"));
}

#[test]
fn menu_button_active_sets_accent_slot() {
    let el: Element = sidebar_menu_button("nav-a")
        .text("Basics")
        .is_active(true)
        .into();
    assert_eq!(
        el.attrs.get("data-active").map(String::as_str),
        Some("true")
    );
    assert_eq!(el.style.bg_color, parse_color("sidebar-accent"));
}

#[test]
fn menu_button_active_keeps_stable_font_weight() {
    fn text_weight(el: &Element) -> u16 {
        if el.text.is_some() {
            return el.text_style.weight_resolved();
        }
        el.children
            .iter()
            .map(text_weight)
            .next()
            .unwrap_or(el.text_style.weight_resolved())
    }
    let idle: Element = sidebar_menu_button("w-idle").text("Basics").into();
    let active: Element = sidebar_menu_button("w-active")
        .text("Basics")
        .is_active(true)
        .into();
    assert_eq!(
        text_weight(&idle),
        text_weight(&active),
        "active must not change font-weight (glyph metrics jump the label down)"
    );
    assert_eq!(text_weight(&idle), MENU_WEIGHT);
}

#[test]
fn menu_button_active_does_not_shift_label_y() {
    fn find_text_y(result: &crate::layout::layout_tree::RenderResult, s: &str) -> f32 {
        result
            .text_nodes
            .iter()
            .find(|n| n.text == s)
            .map(|n| n.y)
            .expect("text")
    }
    let (_handle, _) = crate::core::reactivity::create_root(|| {
        let mut idle: Element = sidebar_menu_button("y-idle").text("HelloNav").into();
        let mut layout = LayoutTree::new();
        let first = layout.build_and_layout(&mut idle, 240.0, 48.0, [-1.0, -1.0]);
        let y0 = find_text_y(&first, "HelloNav");
        let _ = apply_bound_sidebar_menu_button_active(&mut idle, true);
        let second = layout.build_and_layout(&mut idle, 240.0, 48.0, [-1.0, -1.0]);
        let y1 = find_text_y(&second, "HelloNav");
        assert!(
            (y0 - y1).abs() < 0.5,
            "selecting a menu item must not move the label (idle y={y0}, active y={y1})"
        );
    });
}

#[test]
fn sidebar_width_lerp_is_linear_200ms() {
    assert!((sidebar_width_lerp(256.0, 48.0, 0.0) - 256.0).abs() < 0.01);
    assert!((sidebar_width_lerp(256.0, 48.0, 100.0) - 152.0).abs() < 0.01);
    assert!((sidebar_width_lerp(256.0, 48.0, 200.0) - 48.0).abs() < 0.01);
    assert!((sidebar_width_lerp(48.0, 256.0, 200.0) - 256.0).abs() < 0.01);
}

#[test]
fn builder_items_wrap_in_menu_and_content() {
    let el: Element = sidebar("shell")
        .w("13rem")
        .item(sidebar_menu_button("a").text("One"))
        .into();
    fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_slot(c, slot))
    }
    assert!(find_slot(&el, SLOT_CONTENT).is_some());
    assert!(find_slot(&el, SLOT_MENU).is_some());
    assert!(find_slot(&el, SLOT_MENU_BUTTON).is_some());
}

#[test]
fn content_host_survives_take_packed_beside_header() {
    let el: Element = sidebar("shell-host")
        .w("13rem")
        .header(Element::new().text("HeaderChrome"))
        .item(sidebar_menu_button("a").text("One"))
        .footer(Element::new().text("FooterChrome"))
        .into();
    fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_slot(c, slot))
    }
    let inner = find_slot(&el, SLOT_INNER).expect("inner");
    assert!(
        inner
            .children
            .iter()
            .any(|c| c.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_HEADER)),
        "header must remain a sibling of the content host"
    );
    assert!(
        inner.children.iter().all(|c| c
            .focus_id
            .as_deref()
            .is_none_or(|f| !f.starts_with("scrollbar-track:"))),
        "take_packed must not hoist the overlay track next to Header"
    );
    let host = find_slot(&el, SLOT_CONTENT_HOST).expect("content host");
    assert!(find_slot(host, SLOT_CONTENT).is_some());
    assert!(host.children.iter().any(|c| {
        c.focus_id
            .as_deref()
            .is_some_and(|f| f.starts_with("scrollbar-track:"))
    }));
}

#[test]
fn builder_tree_hosts_tree_view_in_content() {
    use crate::components::tree_view::TreeNode;
    use crate::core::reactivity::create_root;

    let (_o, el): (_, Element) = create_root(|| {
        sidebar("shell-tree")
            .tree(sidebar_tree("nav-tree").roots(vec![TreeNode {
                id: "a".into(),
                label: "A".into(),
                children: vec![],
                chrome: Default::default(),
            }]))
            .into()
    });
    fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_slot(c, slot))
    }
    assert!(find_slot(&el, SLOT_CONTENT).is_some());
    assert!(find_slot(&el, "tree-view").is_some());
    assert!(find_slot(&el, SLOT_MENU).is_none());
}

#[test]
fn menu_button_rest_is_transparent_with_sidebar_fg() {
    let el: Element = sidebar_menu_button("nav-a")
        .text("Basics")
        .is_active(false)
        .into();
    fn first_text_color(el: &Element) -> Option<[u8; 4]> {
        if el.text.is_some() {
            return Some(el.text_style.color_resolved());
        }
        el.children.iter().find_map(first_text_color)
    }
    assert_eq!(el.style.bg_color[3], 0.0);
    assert_eq!(
        first_text_color(&el),
        Some(parse_text_color("sidebar-foreground"))
    );
}

fn rsx_string_menu_button(id: &str, label: &str) -> Element {
    SidebarMenuButton(SidebarMenuButtonProps {
        id: id.into(),
        is_active: None,
        variant: None,
        size: None,
        on_click: None,
        children: Some(Element::from(label)),
        base: None,
    })
}

fn first_text_leaf(el: &Element) -> Option<&Element> {
    if el.text.is_some() {
        return Some(el);
    }
    el.children.iter().find_map(first_text_leaf)
}

#[test]
fn rsx_string_menu_button_fg_survives_full_cascade() {
    // Gallery Header/Footer: `<SidebarMenuButton>"Jadix Gallery"</SidebarMenuButton>`
    // has no `text_color` attr. Overlay enter runs a full cascade; without the
    // inherit-lock stamp, `TextStyle::default` white overwrites sidebar-foreground.
    let mut btn = rsx_string_menu_button("gal-workspace", "Jadix Gallery");
    let fg = parse_text_color("sidebar-foreground");
    let leaf = first_text_leaf(&btn).expect("text leaf");
    assert_eq!(leaf.text_style.color_resolved(), fg);
    assert!(
        leaf.attrs.contains_key("text_color"),
        "paint must lock color against cascade"
    );

    crate::layout::layout_tree::cascade_inheritable_text_style_for_test(&mut btn);
    let leaf = first_text_leaf(&btn).expect("text leaf after cascade");
    assert_eq!(
        leaf.text_style.color_resolved(),
        fg,
        "cascade must not reset RSX string leaves to default white"
    );
    assert_ne!(leaf.text_style.color_resolved(), [255, 255, 255, 255]);
}

#[test]
fn header_slot_inherits_sidebar_foreground() {
    let el = SidebarHeader(SidebarHeaderProps {
        children: Some(Element::from("Jadix Gallery")),
        base: None,
    });
    assert_eq!(
        el.text_style.color,
        Some(parse_text_color("sidebar-foreground"))
    );
}

#[test]
fn menu_button_outline_ring_only_when_active() {
    let rest: Element = sidebar_menu_button("nav-o")
        .text("Outline")
        .variant("outline")
        .into();
    assert_eq!(rest.style.border_width, 0.0);
    assert_eq!(rest.style.shadow_spread, 0.0);

    let active: Element = sidebar_menu_button("nav-o-active")
        .text("Outline")
        .variant("outline")
        .is_active(true)
        .into();
    assert_eq!(active.style.border_width, 0.0);
    assert!(active.style.shadow_spread > 0.0);
}

#[test]
fn inset_rail_has_content_side_border() {
    let el: Element = Sidebar(SidebarProps {
        id: "inset-rail".into(),
        rail_w: None,
        side: Some("left".into()),
        variant: Some("inset".into()),
        collapsible: None,
        children: None,
        base: None,
    });
    assert_eq!(el.style.bg_color, parse_color("sidebar"));
    assert!(
        el.style.border_width > 0.0 || el.style.border_width_sides.is_some(),
        "inset rail must draw content-side border"
    );
    fn find_inner<'a>(el: &'a Element) -> Option<&'a Element> {
        if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_INNER) {
            return Some(el);
        }
        el.children.iter().find_map(find_inner)
    }
    let inner = find_inner(&el).expect("sidebar-inner");
    assert!(
        inner.style.radii.iter().all(|r| *r < 0.01),
        "inset inner must not be wrapped in a rounded shell"
    );
    assert_eq!(inner.style.border_width, 0.0);
    assert_eq!(
        inner.style.overflow.y,
        taffy::style::Overflow::Visible,
        "inset inner must not clip Header/Footer glyphs (overflow lives on SidebarContent)"
    );
}

#[test]
fn header_and_footer_do_not_clip_overflow() {
    let header = SidebarHeader(SidebarHeaderProps {
        children: None,
        base: None,
    });
    let footer = SidebarFooter(SidebarFooterProps {
        children: None,
        base: None,
    });
    assert_eq!(header.style.overflow.x, taffy::style::Overflow::Visible);
    assert_eq!(header.style.overflow.y, taffy::style::Overflow::Visible);
    assert_eq!(footer.style.overflow.x, taffy::style::Overflow::Visible);
    assert_eq!(footer.style.overflow.y, taffy::style::Overflow::Visible);
}

#[test]
fn floating_inner_still_clips_to_rounded_shell() {
    let el: Element = sidebar("float").variant(SidebarVariant::Floating).into();
    fn find_inner<'a>(el: &'a Element) -> Option<&'a Element> {
        if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_INNER) {
            return Some(el);
        }
        el.children.iter().find_map(find_inner)
    }
    let inner = find_inner(&el).expect("sidebar-inner");
    assert_eq!(inner.style.overflow.y, taffy::style::Overflow::Hidden);
}

#[test]
fn rail_layout_offcanvas_collapses_width_when_closed() {
    let mut rail = paint_sidebar_rail(
        "16rem",
        SidebarSide::Left,
        SidebarVariant::Inset,
        SidebarCollapsible::Offcanvas,
        None,
    );
    let status = apply_sidebar_rail_layout(&mut rail, false, "16rem");
    assert_eq!(status, LayoutStatus::LayoutChanged);
    assert_eq!(
        crate::layout::dim_length_px(rail.style.size.width),
        Some(0.0)
    );
    assert_eq!(
        rail.attrs.get(ATTR_STATE).map(String::as_str),
        Some("collapsed")
    );
}

#[test]
fn rail_layout_icon_mode_uses_icon_width_when_closed() {
    let mut rail = paint_sidebar_rail(
        "16rem",
        SidebarSide::Left,
        SidebarVariant::Inset,
        SidebarCollapsible::Icon,
        None,
    );
    let status = apply_sidebar_rail_layout(&mut rail, false, "16rem");
    assert_eq!(status, LayoutStatus::LayoutChanged);
    assert_eq!(
        crate::layout::dim_length_px(rail.style.size.width),
        Some(48.0),
        "icon collapse must keep a 3rem rail, not width 0"
    );
    assert_eq!(
        crate::layout::dim_length_px(rail.style.min_size.width),
        Some(48.0)
    );
    assert_eq!(
        rail.attrs.get(ATTR_STATE).map(String::as_str),
        Some("collapsed")
    );
}

#[test]
fn rail_layout_none_keeps_width_when_closed() {
    let mut rail = paint_sidebar_rail(
        "16rem",
        SidebarSide::Left,
        SidebarVariant::Inset,
        SidebarCollapsible::None,
        None,
    );
    let _ = apply_sidebar_rail_layout(&mut rail, false, "16rem");
    assert_eq!(
        crate::layout::dim_length_px(rail.style.size.width),
        Some(256.0),
        "collapsible=none must not shrink the rail"
    );
}

#[test]
fn icon_collapse_hides_labels_and_keeps_icons() {
    use crate::components::label::label;
    use crate::utils::icon_provider::build_icon_named;

    fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_slot(c, slot))
    }
    fn find_icon<'a>(el: &'a Element) -> Option<&'a Element> {
        if is_sidebar_icon_node(el) {
            return Some(el);
        }
        el.children.iter().find_map(find_icon)
    }
    fn find_text_label<'a>(el: &'a Element) -> Option<&'a Element> {
        if el.text.as_deref() == Some("Basics") {
            return Some(el);
        }
        el.children.iter().find_map(find_text_label)
    }

    let mut kids = Element::new();
    kids.set_attr("jadix_fragment", "1");
    let mut btn_kids = Element::new();
    btn_kids.set_attr("jadix_fragment", "1");
    btn_kids = btn_kids.children(vec![
        build_icon_named("layout-dashboard"),
        label("").text("Basics").into(),
    ]);
    kids = kids.children(vec![
        SidebarGroupLabel(SidebarGroupLabelProps {
            children: Some(Element::from("Nav")),
            base: None,
        }),
        SidebarMenuButton(SidebarMenuButtonProps {
            id: "icon-nav".into(),
            is_active: None,
            variant: None,
            size: None,
            on_click: None,
            children: Some(btn_kids),
            base: None,
        }),
    ]);
    let mut rail: Element = Sidebar(SidebarProps {
        id: "icon-rail".into(),
        rail_w: Some("16rem".into()),
        side: None,
        variant: Some("inset".into()),
        collapsible: Some("icon".into()),
        children: Some(kids),
        base: None,
    });
    let status = apply_sidebar_rail_layout(&mut rail, false, "16rem");
    assert_eq!(status, LayoutStatus::LayoutChanged);
    assert_eq!(
        crate::layout::dim_length_px(rail.style.size.width),
        Some(48.0)
    );
    let group_label = find_slot(&rail, SLOT_GROUP_LABEL).expect("group label");
    assert!(
        group_label.style.opacity < 0.01,
        "group label must fade like ShadCN opacity-0"
    );
    assert_eq!(
        crate::layout::dim_length_px(group_label.style.size.height),
        Some(0.0),
        "group label must collapse height (ShadCN -mt-8)"
    );
    let icon = find_icon(&rail).expect("lucide icon");
    assert!(icon.style.opacity > 0.99);
    let text = find_text_label(&rail).expect("menu label");
    assert!(
        text.style.opacity < 0.01,
        "non-icon text must hide, not Display::None"
    );
    assert_eq!(
        crate::layout::dim_length_px(text.style.size.width),
        Some(0.0),
        "collapsed labels must drop out of flex so the 16px icon sits in p-2"
    );
    let btn = find_slot(&rail, SLOT_MENU_BUTTON).expect("menu button");
    assert_eq!(
        crate::layout::dim_length_px(btn.style.size.width),
        Some(32.0),
        "collapsed menu button must be size-8"
    );
    assert_eq!(
        crate::layout::lp_length_px(btn.style.padding.left),
        Some(8.0),
        "collapsed menu button keeps p-2 (ShadCN size-8 + p-2 → 16px icon box)"
    );
}

#[test]
fn collapse_t_tracks_width_not_elapsed_direction() {
    let expanded = 256.0;
    let collapsed = 48.0;
    assert!((collapse_t_from_width(expanded, expanded, collapsed) - 0.0).abs() < 1e-4);
    assert!((collapse_t_from_width(collapsed, expanded, collapsed) - 1.0).abs() < 1e-4);
    assert!((collapse_t_from_width(152.0, expanded, collapsed) - 0.5).abs() < 0.01);

    // Collapse 256→48 at 100ms (halfway): t=0.5
    let mid = collapse_t_from_anim(expanded, collapsed, 100.0, expanded, collapsed);
    assert!((mid - 0.5).abs() < 0.01, "mid-collapse t={mid}");

    // Reverse from that pixel width toward expanded: elapsed 0 must stay t=0.5,
    // not snap to 1.0 (old formula used 1-p on expand, which jumped).
    let reversed = collapse_t_from_anim(152.0, expanded, 0.0, expanded, collapsed);
    assert!(
        (reversed - 0.5).abs() < 0.01,
        "reverse must keep chrome t continuous, got {reversed}"
    );
    let reversed_end = collapse_t_from_anim(152.0, expanded, 200.0, expanded, collapsed);
    assert!(
        (reversed_end - 0.0).abs() < 0.01,
        "reverse end must be expanded, got {reversed_end}"
    );
}

#[test]
fn icon_text_collapse_nowrap_and_stashes_grow_mid_tween() {
    use crate::style::WhiteSpace;

    let mut el = Element::new();
    el.text = Some("指针事件".into());
    el.style.flex_grow = 1.0;
    let changed = apply_icon_text_collapse(&mut el, 0.5);
    assert!(changed);
    assert_eq!(el.text_style.white_space, WhiteSpace::Nowrap);
    assert_eq!(el.text_style.wrap_mode, crate::text::WrapMode::None);
    assert!(
        (el.style.flex_grow).abs() < 0.001,
        "grow must stash as soon as collapse starts so labels clip instead of wrapping"
    );
    assert!(
        crate::layout::dim_length_px(el.style.size.width).is_none(),
        "mid-tween labels keep auto width (clipped); width 0 only at rest"
    );
    assert!((el.style.opacity - 0.5).abs() < 0.02);
}

#[cfg(feature = "animation-base")]
fn paint_icon_rail_with_provider(provider_id: &str, width: &str) -> Element {
    let mut rail = paint_sidebar_rail(
        width,
        SidebarSide::Left,
        SidebarVariant::Inset,
        SidebarCollapsible::Icon,
        None,
    );
    rail.set_attr(ATTR_PROVIDER_ID, provider_id);
    rail.set_attr(ATTR_WIDTH_STORE, width);
    rail
}

#[test]
#[cfg(feature = "animation-base")]
fn rebuild_fragment_does_not_skip_expand_tween() {
    use crate::style::{set_css_transition_timeline_ms, set_css_transition_virtual_timeline};
    use crate::utils::keyed_overlay::preserve_keyed_motion_on_reconcile;

    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);

    let id = "rail-skip-expand";
    let mut mounted = paint_icon_rail_with_provider(id, "16rem");
    let _ = apply_sidebar_rail_layout(&mut mounted, false, "16rem");
    assert_eq!(
        crate::layout::dim_length_px(mounted.style.size.width),
        Some(48.0),
        "prime collapsed rest"
    );

    // Parent rebuild re-paints 16rem. bind_prop skips apply; reconcile copies used width.
    let mut fresh = paint_icon_rail_with_provider(id, "16rem");
    assert_eq!(
        crate::layout::dim_length_px(fresh.style.size.width),
        Some(256.0)
    );
    assert!(preserve_keyed_motion_on_reconcile(&mut fresh, &mounted));
    assert_eq!(
        crate::layout::dim_length_px(fresh.style.size.width),
        Some(48.0),
        "used collapsed width must survive paint 16rem"
    );

    let _ = apply_sidebar_rail_layout(&mut fresh, true, "16rem");
    let w0 = crate::layout::dim_length_px(fresh.style.size.width).unwrap_or(-1.0);
    assert!(
        w0 < 64.0,
        "expand tween must start from collapsed visual width, not paint 16rem; got {w0}"
    );
    assert!(
        rail_width_tween_running(&fresh),
        "expand tween must still be in flight after first tick"
    );

    set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn rebuild_fragment_does_not_skip_collapse_or_clobber_rest() {
    use crate::style::{set_css_transition_timeline_ms, set_css_transition_virtual_timeline};
    use crate::utils::keyed_overlay::preserve_keyed_motion_on_reconcile;

    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);

    let id = "rail-skip-collapse";
    let mut mounted = paint_icon_rail_with_provider(id, "16rem");
    let _ = apply_sidebar_rail_layout(&mut mounted, true, "16rem");
    assert_eq!(
        crate::layout::dim_length_px(mounted.style.size.width),
        Some(256.0)
    );
    let _ = apply_sidebar_rail_layout(&mut mounted, false, "16rem");
    assert!(
        rail_width_tween_running(&mounted),
        "collapse must start a tween"
    );

    set_css_transition_timeline_ms(200.0);
    let _ = mounted.tick_css_transitions(0.0);
    assert!(
        !rail_width_tween_running(&mounted),
        "collapse tween must settle"
    );
    assert_eq!(
        crate::layout::dim_length_px(mounted.style.size.width),
        Some(48.0)
    );

    let mut rest = paint_icon_rail_with_provider(id, "16rem");
    assert!(preserve_keyed_motion_on_reconcile(&mut rest, &mounted));
    assert_eq!(
        crate::layout::dim_length_px(rest.style.size.width),
        Some(48.0),
        "collapsed rest must overwrite paint 16rem so the next toggle can tween"
    );

    let _ = apply_sidebar_rail_layout(&mut rest, true, "16rem");
    let w = crate::layout::dim_length_px(rest.style.size.width).unwrap_or(-1.0);
    assert!(
        w < 64.0,
        "expand after collapsed rest must not skip; got {w}"
    );
    assert!(rail_width_tween_running(&rest));

    set_css_transition_virtual_timeline(false);
}

#[test]
fn provider_icon_collapse_lays_out_3rem_rail() {
    use crate::core::reactivity::create_root;

    fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_slot(c, slot))
    }

    let (_handle, _) = create_root(|| {
        let mut el = sidebar_provider_scope("icon-prov", None, false, Some("inset"), || {
            let mut frag = Element::new();
            frag.set_attr("jadix_fragment", "1");
            frag.children(vec![
                Sidebar(SidebarProps {
                    id: "icon-nav".into(),
                    rail_w: Some("16rem".into()),
                    side: None,
                    variant: Some("inset".into()),
                    collapsible: Some("icon".into()),
                    children: None,
                    base: None,
                }),
                SidebarInset(SidebarInsetProps {
                    variant: Some("inset".into()),
                    children: None,
                    base: None,
                }),
            ])
        });
        let mut layout = LayoutTree::new();
        let _ = layout.build_and_layout(&mut el, 800.0, 600.0, [-1.0, -1.0]);
        let rail = find_slot(&el, SLOT_ROOT).expect("rail");
        let w = rail.layout_bounds[2];
        assert!(
            (w - 48.0).abs() < 1.5,
            "closed icon sidebar must layout at 3rem; got {w}"
        );
    });
}

#[test]
fn horizontal_separator_has_inset_padding_not_full_bleed() {
    use crate::layout::{dim_length_px, lp_length_px};
    let el = sidebar_horizontal_separator_element();
    let pad_l = lp_length_px(el.style.padding.left).unwrap_or(0.0);
    let pad_r = lp_length_px(el.style.padding.right).unwrap_or(0.0);
    assert!(
        pad_l > 1.0 && pad_r > 1.0,
        "separator shell must px-inset from sidebar edges"
    );
    assert_eq!(el.children.len(), 1);
    assert!(
        dim_length_px(el.children[0].style.size.height).unwrap_or(0.0) <= 1.5,
        "inner line must be h-px"
    );
}

#[test]
fn provider_shell_lays_out_sidebar_and_inset_in_row() {
    let el = sidebar_provider_scope("prov", None, true, None, move || {
        let mut frag = Element::new();
        frag.set_attr("jadix_fragment", "1");
        frag.children(vec![
            sidebar("nav").into(),
            SidebarInset(SidebarInsetProps {
                variant: None,
                children: None,
                base: None,
            }),
        ])
    });
    assert_eq!(
        el.attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_PROVIDER)
    );
    assert_eq!(el.style.flex_direction, taffy::style::FlexDirection::Row);
    assert_eq!(el.children.len(), 2);
    assert_eq!(
        el.children[0].attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_ROOT)
    );
    assert_eq!(
        el.children[1].attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_INSET)
    );
}

#[test]
fn provider_wires_open_and_scroll_viewport_on_children() {
    let el = sidebar_provider_scope("prov-scroll", None, true, Some("inset"), move || {
        let mut frag = Element::new();
        frag.set_attr("jadix_fragment", "1");
        frag.children(vec![
            Sidebar(SidebarProps {
                id: "nav".into(),
                rail_w: None,
                side: None,
                variant: Some("inset".into()),
                collapsible: None,
                children: Some({
                    let mut inner = Element::new();
                    inner.set_attr("jadix_fragment", "1");
                    inner.children(vec![SidebarContent(SidebarContentProps {
                        children: None,
                        base: None,
                    })])
                }),
                base: None,
            }),
            SidebarInset(SidebarInsetProps {
                variant: Some("inset".into()),
                children: None,
                base: None,
            }),
        ])
    });
    fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_slot(c, slot))
    }
    let rail = find_slot(&el, SLOT_ROOT).expect("rail");
    assert_eq!(
        rail.attrs.get(ATTR_PROVIDER_ID).map(String::as_str),
        Some("prov-scroll")
    );
    let content = find_slot(&el, SLOT_CONTENT).expect("content");
    assert_eq!(
        content.focus_id.as_deref(),
        Some("scroll:prov-scroll__sidebar-content")
    );
    fn find_scrollbar_for(el: &Element, target: &str) -> bool {
        if el
            .focus_id
            .as_deref()
            .is_some_and(|f| f.starts_with(&format!("scrollbar-track:{target}:")))
        {
            return true;
        }
        el.children.iter().any(|c| find_scrollbar_for(c, target))
    }
    assert!(
        find_scrollbar_for(&el, "prov-scroll__sidebar-content"),
        "SidebarContent overflow must mount an overlay scrollbar"
    );
    let inner = find_slot(&el, SLOT_INNER).expect("inner");
    let track_is_inner_child = inner.children.iter().any(|c| {
        c.focus_id
            .as_deref()
            .is_some_and(|f| f.starts_with("scrollbar-track:"))
    });
    assert!(
        !track_is_inner_child,
        "overlay track must live on the content host, not as a Header sibling"
    );
    let host = find_slot(&el, SLOT_CONTENT_HOST).expect("content host");
    assert!(
        find_scrollbar_for(host, "prov-scroll__sidebar-content"),
        "overlay track must be a descendant of the content host"
    );
}

#[cfg(feature = "theme-fluent")]
#[test]
fn fluent_selection_matches_card_material_and_hover_is_brighter() {
    use crate::style::{set_theme_mode, set_theme_preset, ThemeMode, ThemePreset};
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        set_theme_mode(mode);
        set_theme_preset(ThemePreset::Fluent);
        crate::core::reactivity::create_root(|| {
            let card: Element = crate::components::card().into();
            let mut button: Element = sidebar_menu_button("fluent-material-item")
                .text("Navigation")
                .is_active(true)
                .into();
            assert_eq!(button.style.bg_color, card.style.bg_color);
            assert_eq!(button.style.backdrop_blur, card.style.backdrop_blur);
            assert_eq!(button.style.glass, card.style.glass);
            let selected = button.style.bg_color;
            let mut layout = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            let hovered = use_signal("fluent-material-item_hover", || false);
            for active in [true, false] {
                hovered.set(true);
                super::apply_bound_sidebar_menu_button_active(&mut button, active);
                super::sync_sidebar_menu_button_paint_subtree(
                    &mut button,
                    &mut layout,
                    &mut impact,
                );
                let hover = button.style.bg_color;
                assert!(hover[3] > selected[3]);
                assert_eq!(&hover[..3], &selected[..3]);
                assert_eq!(button.style.backdrop_blur, card.style.backdrop_blur);
            }
            hovered.set(false);
            super::sync_sidebar_menu_button_paint_subtree(&mut button, &mut layout, &mut impact);
            assert_eq!(button.style.bg_color[3], 0.0);
            assert_eq!(button.style.backdrop_blur, 0.0);
            super::apply_bound_sidebar_menu_button_active(&mut button, true);
            assert_eq!(button.style.bg_color, selected);
            assert_eq!(button.style.backdrop_blur, card.style.backdrop_blur);
        });
    }
    set_theme_preset(ThemePreset::Shadcn);
    set_theme_mode(ThemeMode::Light);
}

#[test]
fn menu_button_wires_hover_handler() {
    let el: Element = sidebar_menu_button("nav-hover").text("Hover").into();
    assert!(
        el.on_hover.is_some(),
        "SidebarMenuButton must wire on_hover for sync paint"
    );
}

#[test]
fn menu_button_paint_sync_applies_hover_accent_from_signal() {
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutTree;
    let (_h, mut el): (_, Element) =
        create_root(|| sidebar_menu_button("nav-hover-sync").text("Hover").into());
    use_signal("nav-hover-sync_hover", || false).set(true);
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    super::sync_sidebar_menu_button_paint_subtree(&mut el, &mut tree, &mut impact);
    let want = parse_color("sidebar-accent");
    assert_eq!(
        el.style.bg_color, want,
        "paint sync must write sidebar-accent while `{{id}}_hover` is true"
    );
    assert_eq!(impact, LayoutStatus::StyleChanged);
}

#[test]
fn collapsible_menu_button_preserves_hover_chrome() {
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutTree;
    let (_owner, mut root): (_, Element) = create_root(|| {
        crate::components::collapsible("nav-branch-hover")
            .trigger(|| {
                sidebar_menu_button("nav-branch-hover-trigger")
                    .text("Group")
                    .into()
            })
            .into()
    });
    use_signal("nav-branch-hover-trigger_hover", || false).set(true);
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    super::sync_sidebar_menu_button_paint_subtree(&mut root, &mut tree, &mut impact);
    let trigger = root
        .children
        .iter()
        .find(|child| child.attrs.get(ATTR_SLOT).map(String::as_str) == Some("collapsible-trigger"))
        .unwrap();
    assert_eq!(trigger.style.bg_color, parse_color("sidebar-accent"));
    assert_eq!(impact, LayoutStatus::StyleChanged);
}

#[test]
fn bound_active_update_must_not_wipe_live_hover_chrome() {
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutTree;
    let (_h, mut el): (_, Element) =
        create_root(|| sidebar_menu_button("nav-hover-active").text("Item").into());
    use_signal("nav-hover-active_hover", || false).set(true);
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    super::sync_sidebar_menu_button_paint_subtree(&mut el, &mut tree, &mut impact);
    let before = el.style.bg_color;
    let _ = super::apply_bound_sidebar_menu_button_active(&mut el, true);
    assert_eq!(
        el.attrs.get("data-active").map(String::as_str),
        Some("true")
    );
    assert_eq!(
        el.style.bg_color, before,
        "isActive bind must keep accent while the pointer is still hovering"
    );
    assert_eq!(
        before,
        parse_color("sidebar-accent"),
        "precondition: hover sync must have applied accent"
    );
}

/// Gallery rail is an idle sibling while the inset page scrolls / settles.
/// Hover chrome is paint-only — without Button-parity damage + skip-hash bust,
/// ViewportsOnly `reuse_idle` skip-replays the rest bake and accent never shows
/// (basics page → hover 「指针事件」).
#[test]
fn menu_button_hover_must_collect_accent_under_idle_sidebar_skip() {
    use crate::app::damage::take_frame_paint_nodes;
    use crate::app::frame_stats::reset_collect_skip_counters;
    use crate::core::reactivity::create_root;
    use crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX;
    use crate::core::tree::LayoutTree;

    reset_collect_skip_counters();
    let _ = take_frame_paint_nodes();
    let accent = parse_color("sidebar-accent");

    let (_h, mut root): (_, Element) = create_root(|| {
        let basics: Element = sidebar_menu_button("gal-nav-basics")
            .text("基础")
            .is_active(true)
            .into();
        let events: Element = sidebar_menu_button("gal-nav-events")
            .text("指针事件")
            .into();
        let rail = Element::new()
            .flex_col()
            .w(160.0)
            .h(200.0)
            .flex_shrink(0.0)
            .bg(parse_color("sidebar"))
            .child(basics)
            .child(events);
        let rows = Element::new().flex_col().w(200.0).children((0..8).map(|i| {
            Element::new()
                .w(200.0)
                .h(40.0)
                .flex_shrink(0.0)
                .child(Element::new().text(format!("Row{i}")))
        }));
        let mut page = Element::new()
            .w(200.0)
            .h(200.0)
            .overflow_y("scroll")
            .overflow_x("hidden")
            .child(rows);
        page.focus_id = Some(format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}gal-nav-hover-skip"));
        Element::new()
            .flex_row()
            .w(360.0)
            .h(200.0)
            .child(rail)
            .child(page)
    });

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 360.0, 200.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 360.0, 200.0, [-1.0, -1.0], true, false);

    // Freeze an idle-rail fingerprint, then scroll the page (gallery main pane).
    root.children[1].scroll_offset[1] = 40.0;
    use_signal(
        &crate::core::scroll_viewport::scroll_offset_y_signal_key("gal-nav-hover-skip"),
        || 0.0_f32,
    )
    .set_silent(40.0);
    let _ = tree.build_and_layout_fast(&mut root, 360.0, 200.0, [-1.0, -1.0], true, false);
    let (skipped_before, _) = tree.last_collect_skip_counts();
    assert!(
            skipped_before >= 1,
            "precondition: idle rail must be skip-replayed while the page scrolls (skipped={skipped_before})"
        );

    let _ = take_frame_paint_nodes();
    use_signal("gal-nav-events_hover", || false).set(true);
    let mut impact = LayoutStatus::Clean;
    super::sync_sidebar_menu_button_paint_subtree(&mut root, &mut tree, &mut impact);
    assert_eq!(impact, LayoutStatus::StyleChanged);
    let dirties = take_frame_paint_nodes();
    assert!(
        !dirties.is_empty(),
        "hover chrome must queue named paint damage (Button parity); got {dirties:?}"
    );

    let events = root.children[0].children[1].style.bg_color;
    assert_eq!(
        events, accent,
        "sync must write sidebar-accent onto the hovered idle nav item"
    );

    let after = tree.build_and_layout_fast(&mut root, 360.0, 200.0, [-1.0, -1.0], true, false);
    let has_accent = after.instances.iter().any(|i| {
        i.rect_size[0] > 20.0
            && i.rect_size[1] > 10.0
            && (0..4).all(|c| (i.color[c] - accent[c]).abs() < 0.06)
    });
    assert!(
        has_accent,
        "idle-rail skip-replay must not keep rest bake after hover accent; instances={:?}",
        after.instances.iter().map(|i| i.color).collect::<Vec<_>>()
    );
}
