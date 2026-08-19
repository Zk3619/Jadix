//! Runtime Inspector (B1): Elements pick/highlight + Reactivity graph + Layout/CSSOM.

mod overlay;
mod panel;
mod reactivity_panel;
mod snapshot;
mod state;
mod style_edit;

pub use snapshot::{
    box_model_snapshot, cssom_text_from_style, find_element_by_path, find_element_by_path_mut,
    key_to_path, node_label, path_to_key, pick_element_path, snapshot_subtree, snapshot_tree,
    source_location_from_template_id, style_snapshot, BoxModelSnapshot, ElementPath,
    InspectorNodeSnapshot, SourceLocation, StyleSnapshot,
};
pub use state::{DevToolsState, DevToolsTab, HighlightLayer};
pub use style_edit::{apply_cssom_text, apply_style_patch, patch_element_at_path, StylePatch};

pub(crate) use overlay::{
    begin_highlight_collect, clear_highlight_collect_root, refresh_highlight_slot,
    set_highlight_collect_root, stamped_highlight_abs_rect, sync_box_model_visuals,
    sync_tree_row_hovers, HIGHLIGHT_SLOT_ID,
};
pub(crate) use panel::{sync_overlay, wrap_app};

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
            if c.eq_ignore_ascii_case("c")
                && modifiers.control_key()
                && modifiers.shift_key() =>
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
    if let Some(path) = pick_element_path(root, x, y, [0.0, 0.0]) {
        if path_is_devtools_chrome(root, &path) {
            return false;
        }
        DevToolsState::set_highlight_layer(HighlightLayer::All);
        DevToolsState::set_selected_path(path.clone());
        DevToolsState::set_hover_highlight_path(path);
        DevToolsState::set_pick(false);
        return true;
    }
    false
}

fn path_is_devtools_chrome(root: &crate::core::tree::Element, path: &[usize]) -> bool {
    let mut el = root;
    if el
        .template_id
        .as_deref()
        .is_some_and(|id| id.starts_with("__rui_devtools"))
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
            .is_some_and(|id| id.starts_with("__rui_devtools"))
        {
            return true;
        }
    }
    false
}
