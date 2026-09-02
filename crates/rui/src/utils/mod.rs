pub mod activate_registry;
pub mod block_on;
pub mod clipboard;
pub mod controlled;
pub mod drag_reorder;
pub mod element_pack;
pub mod file_dialog;
pub mod focus_trap;
pub mod font;
pub mod icon_texture;
pub mod input_hit;
pub mod instance_id;
pub mod keyboard;
pub mod keyed_overlay;
pub mod layout_group;
pub mod list_nav;
pub mod lucide;
pub mod lucide_animated;
pub mod motion;
pub mod pointer_drag;
pub mod popup;
pub mod presence;
pub mod roving;
pub mod table_sort;
pub mod text_field;
pub mod text_node;
pub mod text_texture;
pub mod text_vector_spike;
pub mod toggle_state;
pub mod tree_model;
pub mod virtual_list;
pub mod virtual_masonry;

pub use block_on::block_on;
pub use controlled::{resolve_f32_signal, resolve_open_signal, resolve_string_signal};
pub use element_pack::{children_none_if_vacuous, is_text_content_leaf, is_vacuous_element, take_fragment, take_packed};
pub use file_dialog::{
    file_dialog_paths, file_dialog_paths_signal_key, pick_file, pick_files, pick_files_into,
    pick_folder, pick_folder_into,
};
pub use focus_trap::{
    activate_focus_trap, deactivate_focus_trap, focus_trap_ring_ids, sync_focus_trap,
    FOCUS_TRAP_OWNER_KEY, FOCUS_TRAP_RING_KEY,
};
pub use instance_id::resolve_instance_id;
pub use keyed_overlay::{
    adopt_keyed_overlay, is_keyed_motion_item, item_template_id, mount_reactive_anchor,
    preserve_keyed_motion_on_reconcile, viewport_portal_stage, KEYED_OVERLAY_PREFIX,
    TOAST_CARD_SCOPE,
};
pub use layout_group::{
    linear_slot_offset, slot_list_pose, slot_visual_offset, LayoutGroup, SlotListConfig, SlotPose,
};
pub use lucide::{apply_bound_icon_name, lucide_glyph, lucide_svg_inner, ATTR_ICON_NAME};
pub use lucide_animated::{
    animated_icon, animated_icon_names, animated_icon_recipe, ensure_animated_icon_texture,
    start_animated_icon, stop_animated_icon, subtree_needs_animated_icon_frame,
    sync_animated_icon_hover, tick_animated_icons_subtree, AnimatedIcon, AnimatedIconBuilder,
    ATTR_ANIMATED_ICON, ATTR_ANIMATED_ICON_STATE,
};
pub use motion::{
    clear_motion, ensure_motion_transitions, motion_is_animating, motion_sync_status,
    prepare_motion_sync, snap_motion_baseline, MotionBootstrap, MotionEpsilon, MotionStyle,
    MotionTransform,
};
pub use popup::{
    apply_stored_floating_place, arm_underlay_close, compute_anchored_placement,
    compute_cursor_menu_placement, disarm_underlay_close, floating_portal_anchor, mark_floating,
    mark_floating_at_point, mark_floating_with_gap, mark_portal, mark_portal_mount,
    mount_anchored_overlay, mount_modal_layer, mount_viewport_portal_stage, panel_block_clicks,
    path_hits_floating_trigger, portal_panel, portal_panel_mount, swallow_clicks,
    sync_floating_placement, underlay_close_is_armed, underlay_close_on_click,
    visit_floating_panels, PopupPlacement, ATTR_FLOATING, ATTR_FLOATING_H,
    ATTR_FLOATING_MATCH_TRIGGER, ATTR_FLOATING_MAX_H, ATTR_FLOATING_MIN_H, ATTR_FLOATING_MIN_W,
    ATTR_FLOATING_PLACEMENT, ATTR_FLOATING_POINT, ATTR_FLOATING_STRATEGY, ATTR_FLOATING_TRIGGER,
    ATTR_FLOATING_W, FLOATING_TRIGGER_GAP, FLOATING_VIEWPORT_MARGIN,
};
pub use presence::{exit_elapsed, exit_elapsed_at, ExitGate, PresencePhase};
pub use roving::{roving_edge, roving_move};
pub use text_field::{
    bind_controlled_value, caret_line_and_local, commit_value, commit_value_sig,
    ensure_caret_visible_scroll_x, ensure_caret_visible_scroll_y,
    ensure_caret_visible_scroll_y_at_line, focus_id_for_text_edit, input_focus_id,
    input_scroll_x_signal_key, input_value_signal_key, pull_controlled_into_store, text_edit_rest,
    textarea_default_height, textarea_focus_id, textarea_height_signal_key,
    textarea_resize_focus_id, textarea_scroll_rest, textarea_wrap_mode_signal_key,
    textarea_wrap_width_signal_key, unbind_controlled_value, TextareaCaretScrollInsets,
    TextareaWrapMode, WrapMode, INPUT_FOCUS_PREFIX, TEXTAREA_FOCUS_PREFIX,
    TEXTAREA_RESIZE_FOCUS_PREFIX,
};
pub use text_node::{
    apply_label_defaults, build_icon_text_node, build_lucide_icon_text_node,
    build_lucide_icon_text_node_or, build_named_icon_text_node, build_text_node, TextNodeConfig,
};
pub use toggle_state::{
    bool_on_click, bool_slot_key, resolve_bool_signal, write_bool_toggle, SLOT_CHECKBOX,
    SLOT_SWITCH, SLOT_TOGGLE,
};
pub use virtual_list::{item_height_for_text, visible_item_range};
pub use virtual_masonry::{
    height_from_cache, masonry_column_width, pack_masonry, visible_masonry_indices,
    MasonryItemFrame, MasonryLayout,
};
