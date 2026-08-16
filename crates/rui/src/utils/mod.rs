pub mod clipboard;
pub mod drag_reorder;
pub mod file_dialog;
pub mod focus_trap;
pub mod font;
pub mod input_hit;
pub mod instance_id;
pub mod keyboard;
pub mod lucide;
pub mod popup;
pub mod roving;
pub mod text_field;
pub mod text_node;
pub mod virtual_list;

pub use file_dialog::{
    file_dialog_paths, file_dialog_paths_signal_key, pick_file, pick_files, pick_files_into,
    pick_folder, pick_folder_into,
};
pub use focus_trap::{
    activate_focus_trap, deactivate_focus_trap, focus_trap_ring_ids, sync_focus_trap,
    FOCUS_TRAP_OWNER_KEY, FOCUS_TRAP_RING_KEY,
};
pub use instance_id::resolve_instance_id;
pub use lucide::{apply_bound_icon_name, lucide_glyph};
pub use popup::{
    apply_stored_floating_place, arm_underlay_close, compute_anchored_placement,
    disarm_underlay_close, floating_portal_anchor, mark_floating, mark_portal, mark_portal_mount,
    mount_anchored_overlay, mount_modal_layer, mount_viewport_portal_stage, panel_block_clicks,
    path_hits_floating_trigger, portal_panel, portal_panel_mount, swallow_clicks,
    sync_floating_placement, underlay_close_is_armed, underlay_close_on_click,
    visit_floating_panels, PopupPlacement, ATTR_FLOATING, ATTR_FLOATING_H,
    ATTR_FLOATING_MATCH_TRIGGER, ATTR_FLOATING_MAX_H, ATTR_FLOATING_MIN_H, ATTR_FLOATING_MIN_W,
    ATTR_FLOATING_PLACEMENT, ATTR_FLOATING_STRATEGY, ATTR_FLOATING_TRIGGER, ATTR_FLOATING_W,
    FLOATING_TRIGGER_GAP, FLOATING_VIEWPORT_MARGIN,
};
pub use roving::{roving_edge, roving_move};
pub use text_field::{
    bind_controlled_value, caret_line_and_local, commit_value, commit_value_sig,
    ensure_caret_visible_scroll_x, ensure_caret_visible_scroll_y, focus_id_for_text_edit,
    input_focus_id, input_scroll_x_signal_key, input_value_signal_key, pull_controlled_into_store,
    text_edit_rest, textarea_default_height, textarea_focus_id, textarea_height_signal_key,
    textarea_resize_focus_id, textarea_scroll_rest, TextareaCaretScrollInsets, INPUT_FOCUS_PREFIX,
    TEXTAREA_FOCUS_PREFIX, TEXTAREA_RESIZE_FOCUS_PREFIX,
};
pub use text_node::{apply_label_defaults, build_icon_text_node, build_text_node, TextNodeConfig};
pub use virtual_list::{item_height_for_text, visible_item_range};
