//! Runtime Inspector (B1): Elements pick/highlight + Reactivity graph + Layout/CSSOM.

pub(crate) mod controls;
mod device;
mod overlay;
mod locale;
mod panel;
mod presentation;
mod reactivity_panel;
mod snapshot;
mod source;
mod state;
mod style_edit;

pub(crate) fn content_viewport_size(window_size: (f32, f32)) -> (f32, f32) {
    if !DevToolsState::is_enabled()
        || !DevToolsState::is_open()
        || crate::app::window_scope::active() != crate::app::window_scope::WindowScope::Primary
    {
        return window_size;
    }
    if let Some(size) = device::viewport_size() {
        return size;
    }
    if presentation::is_detached() {
        return window_size;
    }
    (
        (window_size.0 - DevToolsState::panel_width()).max(1.0),
        window_size.1,
    )
}

pub use snapshot::{
    box_model_snapshot, cssom_text_from_element, cssom_text_from_style, find_element_by_path,
    find_element_by_path_mut, inspector_authored_style, key_to_path, node_label, path_to_key,
    pick_element_path, snapshot_subtree, snapshot_tree, source_location_from_template_id,
    style_snapshot, style_snapshot_for_element, BoxModelSnapshot, ElementPath,
    InspectorNodeSnapshot, SourceLocation, StyleSnapshot,
};
pub use state::{DevToolsState, DevToolsTab, HighlightLayer};
pub use style_edit::{apply_cssom_text, apply_style_patch, patch_element_at_path, StylePatch};
pub(crate) use style_edit::{
    commit_inspector_cssom, queue_inspector_cssom, reapply_inspector_overrides,
    take_pending_inspector_edits,
};

pub(crate) use device::scope_portals as scope_preview_portals;
#[cfg(test)]
pub(crate) use device::PORTAL_HOST as DEVICE_PORTAL_HOST;
pub(crate) use overlay::{
    begin_highlight_collect, clear_highlight_collect_root, finalize_highlight_after_collect,
    highlight_paint_gen, note_element_paint_visual, note_element_paint_visual_from_abs,
    refresh_highlight, refresh_highlight_slot,
    set_highlight_collect_root, stamped_highlight_abs_rect, HIGHLIGHT_SLOT_ID,
};
pub(crate) use panel::{
    sync_content_viewport, sync_inspector_applied_cssom, sync_overlay_layout, wrap_app,
};
pub(crate) use presentation::is_detached_window;
pub(crate) use presentation::window_resized as detached_window_resized;
pub use presentation::{is_detached, toggle_detached};

#[cfg(test)]
fn set_test_scroll_offset(element: &mut crate::core::tree::Element, offset: [f32; 2]) {
    element.scroll_offset = offset;
    if let Some(rest) = element
        .focus_id
        .as_deref()
        .and_then(|id| id.strip_prefix("scroll:"))
    {
        use crate::core::{reactivity::use_signal, scroll_viewport};
        use_signal(&scroll_viewport::scroll_offset_x_signal_key(rest), || {
            0.0_f32
        })
        .set_silent(offset[0]);
        use_signal(&scroll_viewport::scroll_offset_y_signal_key(rest), || {
            0.0_f32
        })
        .set_silent(offset[1]);
    }
}

pub(crate) fn try_toggle_from_key(
    event: &winit::event::KeyEvent,
    modifiers: winit::keyboard::ModifiersState,
) -> bool {
    if event.state != winit::event::ElementState::Pressed || event.repeat {
        return false;
    }
    if !DevToolsState::is_enabled() {
        return false;
    }
    use winit::keyboard::{Key, NamedKey};
    match &event.logical_key {
        Key::Named(NamedKey::F12) => {
            DevToolsState::toggle_open();
            true
        }
        Key::Character(c)
            if c.eq_ignore_ascii_case("i") && modifiers.control_key() && modifiers.shift_key() =>
        {
            DevToolsState::toggle_open();
            true
        }
        Key::Character(c)
            if c.eq_ignore_ascii_case("c") && modifiers.control_key() && modifiers.shift_key() =>
        {
            DevToolsState::toggle_pick();
            if !DevToolsState::is_open() {
                DevToolsState::set_open(true);
            }
            true
        }
        _ => false,
    }
}

pub(crate) fn try_pick_click(root: &crate::core::tree::Element, x: f32, y: f32) -> bool {
    if !DevToolsState::is_enabled() || !DevToolsState::is_open() || !DevToolsState::is_pick() {
        return false;
    }
    let Some(path) = resolve_pick_path(root, x, y) else {
        return false;
    };
    DevToolsState::set_highlight_layer(HighlightLayer::All);
    DevToolsState::set_tab(DevToolsTab::Elements);
    let (identity, size_hint) = find_element_by_path(root, &path)
        .map(|el| {
            let b = el.layout_bounds;
            (
                snapshot::element_hover_identity(el),
                (b[2].max(0.0), b[3].max(0.0)),
            )
        })
        .unwrap_or_default();
    DevToolsState::set_selected(path.clone(), identity, Vec::new(), size_hint);
    DevToolsState::request_tree_reveal(&path);
    DevToolsState::set_pick(false);
    DevToolsState::set_hover_highlight_path(path);
    DevToolsState::defer_pick_highlight_clear();
    true
}

/// Flush the page overlay after pick selection (hover cleared).
pub(crate) fn sync_highlight_after_pick_commit(root: &mut crate::core::tree::Element) -> bool {
    refresh_highlight(root)
}

/// While pick mode is active, follow the pointer and drive the page overlay highlight.
///
/// Must run **after** `update_hovers` callbacks: tree-row `on_hover(false)` clears the same
/// `HOVER_PATH` key when the pointer is over the app (rows are not the hover target), which
/// would erase pick hover if applied earlier in the same frame.
///
/// Returns `true` when the hover highlight key changed (caller should stamp + paint).
pub(crate) fn try_pick_hover(root: &crate::core::tree::Element, x: f32, y: f32) -> bool {
    if !DevToolsState::is_enabled() || !DevToolsState::is_open() || !DevToolsState::is_pick() {
        return false;
    }
    let before = DevToolsState::hover_path_key();
    match pick_element_path(root, x, y, [0.0, 0.0]) {
        // Pointer over the docked Inspector — leave tree-row hover alone.
        Some(path) if path_is_devtools_chrome(root, &path) => {}
        Some(path) => {
            // Tree rows only exist on the Elements tab; switch so auto-scroll can find them.
            if DevToolsState::active_tab() != DevToolsTab::Elements {
                DevToolsState::set_tab(DevToolsTab::Elements);
            }
            let (identity, size_hint) = find_element_by_path(root, &path)
                .map(|el| {
                    let b = el.layout_bounds;
                    (
                        snapshot::element_hover_identity(el),
                        (b[2].max(0.0), b[3].max(0.0)),
                    )
                })
                .unwrap_or_default();
            DevToolsState::set_hover_highlight(path, identity, Vec::new(), size_hint);
        }
        None => DevToolsState::clear_hover_highlight(),
    }
    DevToolsState::hover_path_key() != before
}

fn resolve_pick_path(root: &crate::core::tree::Element, x: f32, y: f32) -> Option<ElementPath> {
    let path = pick_element_path(root, x, y, [0.0, 0.0])?;
    if path_is_devtools_chrome(root, &path) {
        return None;
    }
    Some(path)
}

fn path_is_devtools_chrome(root: &crate::core::tree::Element, path: &[usize]) -> bool {
    let mut el = root;
    if el
        .template_id
        .as_deref()
        .is_some_and(|id| id.starts_with("__jadix_devtools"))
    {
        return true;
    }
    for &idx in path {
        el = match el.children.get(idx) {
            Some(c) => c,
            None => return false,
        };
        if el
            .template_id
            .as_deref()
            .is_some_and(|id| id.starts_with("__jadix_devtools"))
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
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
        assert_eq!(DevToolsState::overlay_path_key(), DevToolsState::selected_path_key());
        let _ = sync_highlight_after_pick_commit(&mut root);
        let _ = refresh_highlight(&mut root);
        let slot = find_element_by_template_mut(&mut root, HIGHLIGHT_SLOT_ID).unwrap();
        assert!(stamped_highlight_abs_rect(&slot.children[4]).is_some());
        let selected = DevToolsState::selected_path_key();
        let deadline = DevToolsState::pick_highlight_deadline().unwrap();
        assert!(!DevToolsState::expire_pick_highlight(deadline - std::time::Duration::from_millis(1)));
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
}
