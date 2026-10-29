use super::*;
use crate::core::tree::Element;
use crate::layout::layout_tree::PORTAL_LAYER_ID;

fn wrap_like_devtools(app: Element) -> Element {
    panel::wrap_app(app)
}

#[test]
fn pick_ignores_portal_layer_and_hits_app_content() {
    let mut leaf = Element::new().focus_id("real-btn");
    leaf.layout_bounds = [20.0, 20.0, 40.0, 20.0];

    let mut chrome = Element::new().focus_id("chrome").child(leaf);
    chrome.layout_bounds = [0.0, 0.0, 200.0, 200.0];

    let mut toast = Element::new().focus_id("toast");
    toast.layout_bounds = [150.0, 0.0, 40.0, 40.0];

    let mut portal = Element::new();
    portal.template_id = Some(PORTAL_LAYER_ID.into());
    // Full-window stacking shell — must not steal inspector picks.
    portal.layout_bounds = [0.0, 0.0, 400.0, 300.0];

    let mut app = Element::new()
        .w("100%")
        .h("100%")
        .children(vec![chrome, toast, portal]);
    app.layout_bounds = [0.0, 0.0, 400.0, 300.0];

    let mut root = wrap_like_devtools(app);
    // Approximate layout for wrap_app: host[0] + panel_slot[1]
    root.layout_bounds = [0.0, 0.0, 400.0, 300.0];
    if let Some(host) = root.children.get_mut(0) {
        host.layout_bounds = [0.0, 0.0, 400.0, 300.0];
        if let Some(app_el) = host.children.get_mut(0) {
            app_el.layout_bounds = [0.0, 0.0, 400.0, 300.0];
        }
        if let Some(hl) = host.children.get_mut(1) {
            hl.layout_bounds = [0.0, 0.0, 400.0, 300.0];
        }
    }

    let path = pick_element_path(&root, 30.0, 30.0, [0.0, 0.0]).expect("pick");
    let hit = find_element_by_path(&root, &path).expect("resolve");
    assert_eq!(hit.focus_id.as_deref(), Some("real-btn"));
    assert_ne!(hit.template_id.as_deref(), Some(PORTAL_LAYER_ID));
}

#[test]
fn pick_mode_closes_after_successful_click() {
    DevToolsState::enable_for_window();
    DevToolsState::set_open(true);
    DevToolsState::set_pick(true);

    let mut leaf = Element::new().focus_id("target");
    leaf.layout_bounds = [10.0, 10.0, 30.0, 20.0];
    let mut app = Element::new().child(leaf);
    app.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    let mut root = wrap_like_devtools(app);
    root.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    if let Some(host) = root.children.get_mut(0) {
        host.layout_bounds = [0.0, 0.0, 100.0, 100.0];
        if let Some(app_el) = host.children.get_mut(0) {
            app_el.layout_bounds = [0.0, 0.0, 100.0, 100.0];
        }
    }

    assert!(try_pick_click(&root, 15.0, 15.0));
    assert!(
        !DevToolsState::is_pick(),
        "pick mode must turn off after select"
    );
    assert_eq!(DevToolsState::active_tab(), DevToolsTab::Elements);
    let selected = DevToolsState::selected_path();
    let hit = find_element_by_path(&root, &selected).expect("selected");
    assert_eq!(hit.focus_id.as_deref(), Some("target"));
}

#[test]
fn pick_commit_temporarily_highlights_without_clearing_selection() {
    use super::overlay::{refresh_highlight, stamped_highlight_abs_rect, HIGHLIGHT_SLOT_ID};
    use super::snapshot::find_element_by_template_mut;

    DevToolsState::enable_for_window();
    DevToolsState::set_open(true);
    DevToolsState::set_pick(true);

    let mut leaf = Element::new().focus_id("target");
    leaf.layout_bounds = [10.0, 10.0, 80.0, 24.0];
    let mut app = Element::new().child(leaf);
    app.layout_bounds = [0.0, 0.0, 200.0, 200.0];
    let mut root = wrap_like_devtools(app);
    root.layout_bounds = [0.0, 0.0, 400.0, 300.0];
    if let Some(host) = root.children.get_mut(0) {
        host.layout_bounds = [0.0, 0.0, 400.0, 300.0];
        if let Some(app_el) = host.children.get_mut(0) {
            app_el.layout_bounds = [0.0, 0.0, 400.0, 300.0];
        }
        if let Some(slot) = host.children.get_mut(1) {
            slot.layout_bounds = [0.0, 0.0, 400.0, 300.0];
        }
    }

    assert!(try_pick_hover(&root, 15.0, 15.0));
    assert!(refresh_highlight(&mut root));
    assert!(DevToolsState::has_hover_highlight());
    {
        let slot = find_element_by_template_mut(&mut root, HIGHLIGHT_SLOT_ID).unwrap();
        assert!(stamped_highlight_abs_rect(&slot.children[4]).is_some());
    }

    assert!(try_pick_click(&root, 15.0, 15.0));
    assert!(!DevToolsState::is_pick());
    assert!(!DevToolsState::selected_path_key().is_empty());
    assert_eq!(
        DevToolsState::overlay_path_key(),
        DevToolsState::selected_path_key()
    );
    let _ = sync_highlight_after_pick_commit(&mut root);
    let _ = refresh_highlight(&mut root);
    let slot = find_element_by_template_mut(&mut root, HIGHLIGHT_SLOT_ID).unwrap();
    assert!(stamped_highlight_abs_rect(&slot.children[4]).is_some());
    let selected = DevToolsState::selected_path_key();
    let deadline = DevToolsState::pick_highlight_deadline().unwrap();
    assert!(!DevToolsState::expire_pick_highlight(
        deadline - std::time::Duration::from_millis(1)
    ));
    assert!(DevToolsState::expire_pick_highlight(deadline));
    assert!(!DevToolsState::has_hover_highlight());
    assert_eq!(DevToolsState::selected_path_key(), selected);
    refresh_highlight(&mut root);
    let slot = find_element_by_template_mut(&mut root, HIGHLIGHT_SLOT_ID).unwrap();
    assert!(stamped_highlight_abs_rect(&slot.children[4]).is_none());
}

#[test]
fn pick_hover_follows_pointer_and_clears_on_miss() {
    DevToolsState::enable_for_window();
    DevToolsState::set_open(true);
    DevToolsState::set_pick(true);

    let mut leaf = Element::new().focus_id("hover-target");
    leaf.layout_bounds = [10.0, 10.0, 30.0, 20.0];
    let mut app = Element::new().child(leaf);
    app.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    let mut root = wrap_like_devtools(app);
    root.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    if let Some(host) = root.children.get_mut(0) {
        host.layout_bounds = [0.0, 0.0, 100.0, 100.0];
        if let Some(app_el) = host.children.get_mut(0) {
            app_el.layout_bounds = [0.0, 0.0, 100.0, 100.0];
        }
    }

    assert!(try_pick_hover(&root, 15.0, 15.0));
    let hover = DevToolsState::highlight_path();
    let hit = find_element_by_path(&root, &hover).expect("hover path");
    assert_eq!(hit.focus_id.as_deref(), Some("hover-target"));

    // Same target again → no change.
    assert!(!try_pick_hover(&root, 15.0, 15.0));

    // Simulate Elements-tree row leave: must not wipe pick hover while pick is on.
    DevToolsState::clear_hover_highlight_if_path(&hover);
    // (panel skips this call when pick is on; API itself still clears — re-apply pick)
    assert!(try_pick_hover(&root, 15.0, 15.0));
    assert!(DevToolsState::has_hover_highlight());

    assert!(try_pick_hover(&root, 500.0, 500.0));
    assert!(!DevToolsState::has_hover_highlight());

    DevToolsState::set_pick(false);
    assert!(!DevToolsState::has_hover_highlight());
}

#[test]
fn pick_hover_ignores_inspector_chrome() {
    DevToolsState::enable_for_window();
    DevToolsState::set_open(true);
    DevToolsState::set_pick(true);

    let mut leaf = Element::new().focus_id("app-leaf");
    leaf.layout_bounds = [10.0, 10.0, 20.0, 20.0];
    let mut app = Element::new().child(leaf);
    app.layout_bounds = [0.0, 0.0, 200.0, 100.0];
    let mut root = wrap_like_devtools(app);
    root.layout_bounds = [0.0, 0.0, 400.0, 100.0];
    if let Some(host) = root.children.get_mut(0) {
        host.layout_bounds = [0.0, 0.0, 200.0, 100.0];
        if let Some(app_el) = host.children.get_mut(0) {
            app_el.layout_bounds = [0.0, 0.0, 200.0, 100.0];
        }
    }
    // Dock panel occupies the right half.
    if let Some(slot) = root.children.get_mut(1) {
        slot.layout_bounds = [200.0, 0.0, 200.0, 100.0];
        let mut panel = Element::new();
        panel.template_id = Some("__jadix_devtools_panel".into());
        panel.layout_bounds = [200.0, 0.0, 200.0, 100.0];
        slot.children.push(panel);
    }

    assert!(try_pick_hover(&root, 15.0, 15.0));
    assert!(DevToolsState::has_hover_highlight());
    let kept = DevToolsState::hover_path_key();

    // Over inspector chrome: must not clear pick hover (tree-row hover owns that region).
    assert!(!try_pick_hover(&root, 250.0, 50.0));
    assert_eq!(DevToolsState::hover_path_key(), kept);
}

#[test]
fn pick_uses_paint_visual_and_ignores_host_overflow_hidden_scroll() {
    use super::overlay::highlight_paint_gen;
    use super::panel::wrap_app;
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutTree;

    DevToolsState::enable_for_window();
    DevToolsState::set_open(true);
    DevToolsState::set_pick(true);

    create_root(|| {
        let target = Element::new()
            .w(100.0)
            .h(40.0)
            .bg([0.1, 0.5, 0.9, 1.0])
            .focus_id("pick-btn")
            .template_id("pick-btn");
        let spacer = Element::new().w(100.0).h(250.0).flex_shrink(0.0);
        let col = Element::new()
            .flex_col()
            .w(200.0)
            .h(1400.0)
            .flex_shrink(0.0)
            .children(vec![spacer, target]);

        let mut viewport = Element::new()
            .w("100%")
            .h("100%")
            .overflow_y("scroll")
            .focus_id("scroll:pick-main")
            .child(col);
        crate::app::devtools::set_test_scroll_offset(&mut viewport, [0.0, 200.0]);

        let app = Element::new().w("100%").h("100%").child(viewport);
        let mut root = wrap_app(app);
        if let Some(host) = root.children.get_mut(0) {
            host.scroll_offset = [0.0, 90.0];
        }

        let mut layout = LayoutTree::new();
        let _ = layout.build_and_layout(&mut root, 900.0, 600.0, [-1.0, -1.0]);

        let path = vec![0, 0, 0, 0, 1];
        let el = find_element_by_path(&root, &path).expect("target");
        assert_eq!(el.focus_id.as_deref(), Some("pick-btn"));
        let pb = el.paint_visual_bounds;
        assert_eq!(
            el.paint_visual_gen,
            highlight_paint_gen(),
            "collect must stamp paint visual"
        );
        let hx = pb[0] + pb[2] * 0.5;
        let hy = pb[1] + pb[3] * 0.5;
        assert!(
            try_pick_hover(&root, hx, hy),
            "pick must hit painted button at ({hx},{hy})"
        );
        let hover = DevToolsState::highlight_path();
        let hit = find_element_by_path(&root, &hover).expect("hover");
        assert_eq!(hit.focus_id.as_deref(), Some("pick-btn"));

        let abs_y = el.layout_bounds[1] + el.layout_bounds[3] * 0.5;
        assert!(
            (abs_y - hy).abs() > 50.0,
            "sanity: paint y must differ from abs y under scroll"
        );
    });

    DevToolsState::set_pick(false);
    DevToolsState::clear_hover_highlight();
}
