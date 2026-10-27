//! Viewport-modal motion: author kinds + crate-private engine / runtime pump.
//!
//! - **Author kind / viewport**: [`modal_surface`] (this module re-exports it).
//! - **Engine**: [`engine`] (seed / hoist flush / follow-hand). Not author API.
//! - **Runtime pump** (crate-private): [`flush_modal_shell_motion_subtree`] /
//!   [`tick_hoisted_modal_css`] / [`tick_modal_presence`]. Used by `app/state`,
//!   `app/window_events`, and Command/Dialog enter-pump tests — not product
//!   orchestration. Authors should use [`crate::utils::viewport_modal`].

mod engine;

pub use crate::utils::modal_surface::{
    default_slide_axis_px, modal_viewport_size, set_modal_viewport_size, ModalSurfaceMotion,
};

pub(crate) use engine::{
    any_modal_css_needs_frame, apply_modal_shell_motion, bind_modal_open,
    drain_pending_modal_unmounts, finish_modal_exit_immediate, flush_modal_shell_motion_subtree,
    hoisted_should_retain, modal_shell_defer_css_tick, mounted_signal, sync_modal_presence,
    take_modal_force_immediate_wake, tick_hoisted_modal_css, tick_modal_presence,
    use_modal_presence, ModalPresenceHandle,
};

#[cfg(test)]
pub(crate) use engine::{
    clear_pending_modal_enter, finish_modal_exit_for_test, set_modal_css_needs_frame_for_test,
};

pub use engine::{
    arm_modal_slide_settle, note_modal_shell_drag, patch_modal_overlay_opacity,
    patch_modal_panel_transform,
};
