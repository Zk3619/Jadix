use super::*;

#[test]
fn resizing_preview_keeps_one_frame_and_preserves_control_nodes() {
    crate::core::reactivity::create_root(|| {
        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);
        ENABLED.set(true);
        let mut root = super::super::panel::wrap_app(Element::new().w("100%").h("100%"));
        root.children[0].layout_bounds = [0.0, 0.0, 500.0, 900.0];
        let mut tree = LayoutTree::new();
        sync(&mut root, &mut tree);
        let control = super::super::snapshot::find_element_by_template(&root, "dt-device-rotate")
            .expect("rotate control")
            .on_click
            .clone()
            .unwrap();
        for width in [390.0, 360.0, 420.0, 300.0] {
            VIEWPORT.set(Viewport {
                width,
                ..VIEWPORT.get()
            });
            DevToolsState::bump();
            sync(&mut root, &mut tree);
            assert_eq!(
                root.children[0]
                    .children
                    .iter()
                    .filter(|node| node.template_id.as_deref() == Some(FRAME))
                    .count(),
                1,
                "resizing must update the existing frame rather than append grips"
            );
            let current =
                super::super::snapshot::find_element_by_template(&root, "dt-device-rotate")
                    .expect("rotate control");
            assert!(std::rc::Rc::ptr_eq(
                &control,
                current.on_click.as_ref().unwrap()
            ));
        }
        disable();
        DevToolsState::set_open(false);
        VIEWPORT.set(Viewport {
            width: 390.0,
            height: 844.0,
            zoom: 0.0,
        });
    });
}

#[test]
fn preview_toolbar_portals_stay_outside_the_scaled_host() {
    crate::core::reactivity::create_root(|| {
        use crate::layout::layout_tree::ATTR_PORTAL_MOUNT;
        disable();
        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);
        toggle();
        let mut toolbar_popup = Element::new().template_id("toolbar-menu");
        toolbar_popup.set_attr("jadix_portal", "1");
        let mut app_popup = Element::new().template_id("app-menu");
        app_popup.set_attr("jadix_portal", "1");
        let mut root = Element::new()
            .template_id("__jadix_app_host")
            .child(Element::new().child(app_popup))
            .child(Element::new().template_id(TOOLBAR).child(toolbar_popup));
        assert!(scope_portals(&mut root));
        assert_eq!(
            root.children[0].children[0]
                .attrs
                .get(ATTR_PORTAL_MOUNT)
                .map(String::as_str),
            Some(PORTAL_HOST)
        );
        assert_eq!(
            root.children[1].children[0]
                .attrs
                .get(ATTR_PORTAL_MOUNT)
                .map(String::as_str),
            Some("__jadix_app_host")
        );
        assert!(!scope_portals(&mut root));
        disable();
        DevToolsState::set_open(false);
    });
}

#[test]
fn preview_moves_open_portals_locally_and_restores_them_on_close() {
    crate::core::reactivity::create_root(|| {
        use super::super::snapshot::find_element_by_template;
        use crate::layout::layout_tree::{hoist_portal_subtrees, ATTR_HOISTED_MOUNT};
        disable();
        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);
        let mut portal = Element::new()
            .template_id("preview-test-popup")
            .w(100.0)
            .h(80.0);
        portal.set_attr("jadix_portal", "1");
        portal.set_attr("jadix_portal_key", "preview-test-popup");
        let app = Element::new().w("100%").h("100%").child(portal);
        let mut root = super::super::panel::wrap_app(app);
        let mut tree = LayoutTree::new();
        super::super::panel::sync_overlay_layout(&mut root, &mut tree);
        tree.build_and_layout(&mut root, 1200.0, 800.0, [-1.0; 2]);
        assert_eq!(
            find_element_by_template(&root, "preview-test-popup")
                .unwrap()
                .attrs
                .get(ATTR_HOISTED_MOUNT)
                .map(String::as_str),
            Some("__jadix_app_host")
        );
        toggle();
        super::super::panel::sync_overlay_layout(&mut root, &mut tree);
        assert!(hoist_portal_subtrees(&mut root) > 0);
        tree.build_and_layout(&mut root, 1200.0, 800.0, [-1.0; 2]);
        let mount = find_element_by_template(&root, PORTAL_HOST).unwrap();
        assert_eq!(mount.layout_bounds[2..], [390.0, 844.0]);
        let popup = find_element_by_template(mount, "preview-test-popup").unwrap();
        assert_eq!(
            popup.attrs.get(ATTR_HOISTED_MOUNT).map(String::as_str),
            Some(PORTAL_HOST)
        );
        assert!(!crate::layout::layout_tree::escapes_ancestor_overflow(
            popup
        ));
        assert!(find_element_by_template(&root, FRAME).is_some());
        disable();
        super::super::panel::sync_overlay_layout(&mut root, &mut tree);
        assert!(hoist_portal_subtrees(&mut root) > 0);
        let popup = find_element_by_template(&root, "preview-test-popup").unwrap();
        assert_eq!(
            popup.attrs.get(ATTR_HOISTED_MOUNT).map(String::as_str),
            Some("__jadix_app_host")
        );
        assert!(find_element_by_template(&root, FRAME).is_none());
        DevToolsState::set_open(false);
        assert!(hoist_portal_subtrees(&mut root) > 0);
        let popup = find_element_by_template(&root, "preview-test-popup").unwrap();
        assert!(popup.style.position_fixed);
        assert!(!popup.attrs.contains_key(ATTR_HOISTED_MOUNT));
    });
}

#[test]
fn preview_resize_capture_updates_dimensions_in_logical_pixels() {
    crate::core::reactivity::create_root(|| {
        use crate::app::pointer_events::{dispatch_pointer, hit_path, PointerEvent, PointerKind};
        use winit::event::MouseButton;
        VIEWPORT.set(Viewport {
            width: 390.0,
            height: 844.0,
            zoom: 0.5,
        });
        PAINT_SCALE.set(0.5);
        RESIZE_LIMIT.set([500.0, 910.0]);
        let mut grip = resize_grip("dt-device-resize-test", true, true)
            .w(14.0)
            .h(14.0);
        grip.layout_bounds = [400.0, 500.0, 14.0, 14.0];
        let mut root = Element::new().child(grip);
        root.layout_bounds = [0.0, 0.0, 1200.0, 900.0];
        let mut event = PointerEvent::new(405.0, 505.0, MouseButton::Left, 1);
        event.buttons = 1;
        dispatch_pointer(
            &hit_path(&root, event.x, event.y),
            &mut event,
            PointerKind::Down,
        );
        let path = crate::app::pointer_events::captured_path(&root).unwrap();
        let mut event = PointerEvent::new(430.0, 535.0, MouseButton::Left, 0);
        event.buttons = 1;
        dispatch_pointer(&path, &mut event, PointerKind::Move);
        assert_eq!(VIEWPORT.get().width, 490.0);
        assert_eq!(VIEWPORT.get().height, 904.0);
        let mut event = PointerEvent::new(5000.0, 5000.0, MouseButton::Left, 0);
        event.buttons = 1;
        dispatch_pointer(&path, &mut event, PointerKind::Move);
        assert_eq!(VIEWPORT.get().width, 500.0);
        assert_eq!(VIEWPORT.get().height, 910.0);
        dispatch_pointer(&path, &mut event, PointerKind::Up);
        RESIZE_LIMIT.set([7680.0, 4320.0]);
        assert!(RESIZE.get().is_none());
        assert!(crate::app::pointer_capture_id().is_none());
        VIEWPORT.set(Viewport {
            width: 390.0,
            height: 844.0,
            zoom: 0.0,
        });
    });
}

#[test]
fn responsive_width_override_is_scoped_and_reversible() {
    let mut content = Element::new().child(Element::new().class("w-10 md:w-32"));
    content
        .attrs
        .insert("data-jadix-viewport-width".into(), "390".into());
    let mut root = Element::new().children(vec![content, Element::new().class("w-10 md:w-32")]);
    let mut tree = LayoutTree::new();
    let mut impact = crate::core::tree::LayoutStatus::Clean;
    crate::style::sync_class_variants_subtree(
        &mut root,
        [-1.0; 2],
        "",
        false,
        1200.0,
        800.0,
        &mut tree,
        &mut impact,
    );
    assert_eq!(
        root.children[0].children[0].style.size.width,
        taffy::style::Dimension::length(40.0)
    );
    assert_eq!(
        root.children[1].style.size.width,
        taffy::style::Dimension::length(128.0)
    );
    root.children[0].attrs.remove("data-jadix-viewport-width");
    crate::style::sync_class_variants_subtree(
        &mut root,
        [-1.0; 2],
        "",
        false,
        1200.0,
        800.0,
        &mut tree,
        &mut impact,
    );
    assert_eq!(
        root.children[0].children[0].style.size.width,
        taffy::style::Dimension::length(128.0)
    );
}

#[test]
fn device_preview_overrides_the_docked_content_viewport() {
    crate::core::reactivity::create_root(|| {
        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);
        DevToolsState::set_panel_width(640.0);
        crate::utils::modal_motion::set_modal_viewport_size(1500.0, 900.0);
        let app = Element::new()
            .w("100%")
            .h("100%")
            .child(Element::new().class("w-24 md:w-48"));
        let mut root = super::super::panel::wrap_app(app);
        let mut tree = LayoutTree::new();
        for (preview, expected_viewport, expected_width) in [
            (false, (860.0, 900.0), 192.0),
            (true, (390.0, 844.0), 96.0),
            (false, (860.0, 900.0), 192.0),
        ] {
            ENABLED.set(preview);
            VIEWPORT.set(Viewport {
                width: 390.0,
                height: 844.0,
                zoom: 0.0,
            });
            super::super::panel::sync_overlay_layout(&mut root, &mut tree);
            let mut impact = LayoutStatus::Clean;
            crate::style::sync_class_variants_subtree(
                &mut root,
                [-1.0; 2],
                "",
                false,
                1500.0,
                900.0,
                &mut tree,
                &mut impact,
            );
            assert_eq!(
                crate::utils::modal_motion::modal_viewport_size(),
                expected_viewport
            );
            assert_eq!(
                root.children[0].children[0].children[0].style.size.width,
                taffy::style::Dimension::length(expected_width)
            );
            tree.build_and_layout(&mut root, 1500.0, 900.0, [-1.0; 2]);
        }
        DevToolsState::set_open(false);
    });
}

#[test]
fn preview_uses_real_device_layout_and_close_restores_style_and_width() {
    crate::core::reactivity::create_root(|| {
        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);
        DevToolsState::set_panel_width(400.0);
        let app = Element::new().w("100%").h("100%").bg([0.2, 0.3, 0.4, 1.0]);
        let original = app.style.clone();
        let mut root = super::super::panel::wrap_app(app);
        let mut tree = LayoutTree::new();
        super::super::panel::sync_overlay_layout(&mut root, &mut tree);
        tree.build_and_layout(&mut root, 1200.0, 800.0, [-1.0; 2]);
        toggle();
        assert_ne!(
            super::super::panel::sync_overlay_layout(&mut root, &mut tree),
            LayoutStatus::Clean
        );
        tree.build_and_layout(&mut root, 1200.0, 800.0, [-1.0; 2]);
        let app = &root.children[0].children[0];
        assert!((app.layout_bounds[2] - 390.0).abs() < 1.0);
        assert!((app.layout_bounds[3] - 844.0).abs() < 1.0);
        assert!(app.style.scale[0] < 1.0);
        DevToolsState::set_open(false);
        assert_ne!(
            super::super::panel::sync_overlay_layout(&mut root, &mut tree),
            LayoutStatus::Clean
        );
        tree.build_and_layout_fast(&mut root, 1200.0, 800.0, [-1.0; 2], false, false);
        assert_eq!(root.children[0].children[0].style, original);
        assert!((root.children[0].children[0].layout_bounds[2] - 1200.0).abs() < 1.0);
        assert!(!root.children[0]
            .children
            .iter()
            .any(|child| child.template_id.as_deref() == Some(TOOLBAR)));
    });
}
