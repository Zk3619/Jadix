//! Anchored-overlay motion: author kind + crate-private engine / runtime pump.
//!
//! - **Author kind**: [`crate::utils::floating_kind::FloatingMotionKind`].
//! - **Engine**: [`engine`] (portal hoist flush, Expand measure). Not author API.
//! - **Runtime pump** (crate-private): [`flush_floating_motion_subtree`] /
//!   [`tick_hoisted_floating_css`] / [`tick_floating_presence`]. Used by
//!   `app/state` and `app/window_events`. Authors should use
//!   [`crate::utils::anchored_overlay`].

mod engine;

pub use crate::utils::floating_kind::FloatingMotionKind;

pub(crate) use engine::{
    any_floating_motion_needs_frame, apply_floating_panel_motion, drain_pending_floating_unmounts,
    floating_motion_needs_relayout, floating_panel_defer_css_tick, floating_portal_phase,
    flush_floating_motion_subtree, hoisted_should_retain, mark_floating_panels_layout_dirty,
    tick_floating_presence, tick_hoisted_floating_css, use_floating_presence,
    FloatingPresenceHandle,
};

#[cfg(all(test, feature = "product-tests"))]
pub(crate) use engine::debug_floating_registry;
#[cfg(test)]
pub(crate) use engine::{floating_mounted_signal, sync_floating_presence};
