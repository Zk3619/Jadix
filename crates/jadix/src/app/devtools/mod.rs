//! Runtime Inspector (B1): Elements pick/highlight + Reactivity graph + Layout/CSSOM.

pub(crate) mod controls;
mod device;
mod locale;
mod overlay;
mod panel;
mod presentation;
mod reactivity_panel;
mod snapshot;
mod source;
mod state;
mod style_edit;
#[cfg(all(test, windows, feature = "images"))]
pub(super) fn hr6_save(updated: String) {
    let location = source_location_from_template_id("source.rs:1:13").unwrap();
    let (document, _) = source::document(&location).unwrap();
    source::save(&document, updated.clone());
    assert_eq!(std::fs::read_to_string(&document.path).unwrap(), updated);
}
#[cfg(all(test, windows, feature = "images"))]
pub(super) fn hr6_writes_enabled() -> bool {
    source::writes_enabled()
}
#[cfg(all(test, windows, feature = "images"))]
pub(super) fn hr6_enable_writes() {
    source::toggle_writes();
}
#[cfg(all(test, windows, feature = "images"))]
pub(super) fn hr6_draft() -> String {
    let location = source_location_from_template_id("source.rs:1:13").unwrap();
    source::document(&location).unwrap().0.text
}
pub(crate) use style_edit::forget_instance;

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
    refresh_highlight, refresh_highlight_slot, set_highlight_collect_root,
    stamped_highlight_abs_rect, HIGHLIGHT_SLOT_ID,
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
#[path = "tests/mod.rs"]
mod tests;
