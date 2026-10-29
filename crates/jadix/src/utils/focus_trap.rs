//! Explicit Tab focus trap (keyboard nav capability — not a product component).
//!
//! When [`FOCUS_TRAP_RING_KEY`] is non-empty, [`crate::app::input_controller`] cycles Tab
//! only among the listed `focus_id`s. Prefer this over guessing from portal z-order when
//! the ring must be an explicit set (custom panels, demos).
//!
//! ## Usage
//! ```ignore
//! jadix::sync_focus_trap("dialog-1", open.get(), "input:name,btn-ok,btn-cancel");
//! // or:
//! jadix::activate_focus_trap("dialog-1", "input:name,btn-ok");
//! jadix::deactivate_focus_trap("dialog-1");
//! ```
//!
//! `owner` identifies who installed the trap so only that owner may clear it when
//! another trap has taken over.

use crate::core::reactivity::use_signal;

/// Comma-separated `focus_id` list; when non-empty, Tab cycles only inside this set.
pub const FOCUS_TRAP_RING_KEY: &str = "__jadix_focus_trap_ring";
/// Current trap owner id; used so deactivate only clears if still owned by the caller.
pub const FOCUS_TRAP_OWNER_KEY: &str = "__jadix_focus_trap_owner";

/// Install a Tab focus trap owned by `owner` with comma-separated `focus_id` ring.
pub fn activate_focus_trap(owner: impl Into<String>, order: impl AsRef<str>) {
    let owner = owner.into();
    let order = order.as_ref().trim();
    use_signal(FOCUS_TRAP_RING_KEY, String::new).set(order.to_string());
    use_signal(FOCUS_TRAP_OWNER_KEY, String::new).set(owner);
}

/// Clear the trap if `owner` still owns it.
pub fn deactivate_focus_trap(owner: &str) {
    let owner_sig = use_signal(FOCUS_TRAP_OWNER_KEY, String::new);
    if owner_sig.peek() == owner {
        use_signal(FOCUS_TRAP_RING_KEY, String::new).set(String::new());
        owner_sig.set(String::new());
    }
}

/// Activate when `active`, otherwise [`deactivate_focus_trap`] for `owner`.
pub fn sync_focus_trap(owner: &str, active: bool, order: impl AsRef<str>) {
    if active {
        activate_focus_trap(owner, order);
    } else {
        deactivate_focus_trap(owner);
    }
}

/// Parse the current ring into trimmed non-empty `focus_id`s (empty if trap inactive).
pub fn focus_trap_ring_ids() -> Vec<String> {
    let ring = use_signal(FOCUS_TRAP_RING_KEY, String::new).peek();
    if ring.is_empty() {
        return Vec::new();
    }
    ring.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
#[path = "tests/focus_trap.rs"]
mod tests;
