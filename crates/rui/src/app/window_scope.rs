//! Active window scope for overlay stacks, focus keys, and reactive patches.
//!
//! Primary (default) keeps historical global TLS / `__rui_*` keys so tests stay unchanged.
//! Secondary windows get a stable numeric token used as [`crate::core::reactivity::set_signal_key_scope`].
//!
//! [`active`] is derived from the reactivity signal-key scope so nested `create_effect` /
//! `reactive_view` bodies keep the owning window even when no explicit
//! [`enter_secondary_id`] guard is held (Owner carries the token; `run_effect` restores it).
//!
//! Each window has its own input channel and UI tree home — there is no AppState tree-swap.
//! Cross-window collaboration uses [`crate::app::multi_window::shared_slot`] /
//! [`crate::app::ui_bridge::run_on_ui_thread`], not a shared hit-test tree.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use winit::window::WindowId;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub(crate) enum WindowScope {
    #[default]
    Primary,
    Secondary(u64),
}

thread_local! {
    static ACTIVE_WINDOW_ID: Cell<Option<WindowId>> = const { Cell::new(None) };
    static NEXT_TOKEN: Cell<u64> = const { Cell::new(1) };
    static TOKENS: RefCell<HashMap<WindowId, u64>> = RefCell::new(HashMap::new());
    static TOKEN_TO_WINDOW: RefCell<HashMap<u64, WindowId>> = RefCell::new(HashMap::new());
}

pub(crate) fn active() -> WindowScope {
    let t = crate::core::reactivity::signal_key_scope();
    if t == 0 {
        WindowScope::Primary
    } else {
        WindowScope::Secondary(t)
    }
}

pub(crate) fn active_window_id() -> Option<WindowId> {
    ACTIVE_WINDOW_ID.get().or_else(window_id_for_active_token)
}

pub(crate) fn window_id_for_active_token() -> Option<WindowId> {
    let t = crate::core::reactivity::signal_key_scope();
    if t == 0 {
        None
    } else {
        TOKEN_TO_WINDOW.with(|m| m.borrow().get(&t).copied())
    }
}

/// Where the *logical* primary / secondary UI tree lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TreeHome {
    AppState,
    SecondarySlot,
}

pub(crate) fn tree_home_for(scope: WindowScope) -> TreeHome {
    match scope {
        WindowScope::Primary => TreeHome::AppState,
        WindowScope::Secondary(_) => TreeHome::SecondarySlot,
    }
}

/// Allocate (or reuse) the secondary-window token for `id`.
pub(crate) fn token_for_secondary(id: WindowId) -> u64 {
    TOKENS.with(|m| {
        *m.borrow_mut().entry(id).or_insert_with(|| {
            let n = NEXT_TOKEN.get();
            NEXT_TOKEN.set(n + 1);
            TOKEN_TO_WINDOW.with(|rev| {
                rev.borrow_mut().insert(n, id);
            });
            n
        })
    })
}

pub(crate) fn forget_secondary(id: WindowId) -> Option<u64> {
    let token = TOKENS.with(|m| m.borrow_mut().remove(&id));
    if let Some(t) = token {
        TOKEN_TO_WINDOW.with(|rev| {
            rev.borrow_mut().remove(&t);
        });
    }
    token
}

pub(crate) struct WindowScopeGuard {
    prev_suffix: u64,
    prev_window: Option<WindowId>,
}

impl Drop for WindowScopeGuard {
    fn drop(&mut self) {
        ACTIVE_WINDOW_ID.set(self.prev_window);
        let _ = crate::core::reactivity::set_signal_key_scope(self.prev_suffix);
    }
}

pub(crate) fn enter(scope: WindowScope) -> WindowScopeGuard {
    let prev_window = ACTIVE_WINDOW_ID.get();
    if matches!(scope, WindowScope::Primary) {
        ACTIVE_WINDOW_ID.set(None);
    }
    let suffix = match scope {
        WindowScope::Primary => 0,
        WindowScope::Secondary(n) => n,
    };
    let prev_suffix = crate::core::reactivity::set_signal_key_scope(suffix);
    WindowScopeGuard {
        prev_suffix,
        prev_window,
    }
}

pub(crate) fn enter_primary() -> WindowScopeGuard {
    enter(WindowScope::Primary)
}

pub(crate) fn enter_secondary_id(id: WindowId) -> WindowScopeGuard {
    let token = token_for_secondary(id);
    let prev_window = ACTIVE_WINDOW_ID.replace(Some(id));
    let prev_suffix = crate::core::reactivity::set_signal_key_scope(token);
    WindowScopeGuard {
        prev_suffix,
        prev_window,
    }
}

/// Permanently switch to primary scope (no restore-on-drop). Use on primary `Focused(true)`
/// so a prior secondary enter cannot leave `SIGNAL_KEY_SCOPE` stuck non-zero.
pub(crate) fn force_primary() {
    ACTIVE_WINDOW_ID.set(None);
    let _ = crate::core::reactivity::set_signal_key_scope(0);
}

/// Permanently switch to a secondary scope (no restore-on-drop). Use on secondary focus.
pub(crate) fn force_secondary_id(id: WindowId) {
    let token = token_for_secondary(id);
    ACTIVE_WINDOW_ID.set(Some(id));
    let _ = crate::core::reactivity::set_signal_key_scope(token);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::{
        bind_current_owner_window_key_scope, create_effect, create_root, create_signal,
        set_signal_key_scope, use_signal,
    };

    #[test]
    fn tree_home_maps_scope_without_swap() {
        assert_eq!(tree_home_for(WindowScope::Primary), TreeHome::AppState);
        assert_eq!(
            tree_home_for(WindowScope::Secondary(1)),
            TreeHome::SecondarySlot
        );
    }

    #[test]
    fn window_local_signal_keys_split_by_scope() {
        let _g = enter(WindowScope::Primary);
        let primary = use_signal("__rui_button_focus_id", String::new);
        primary.set("main".into());

        let _g2 = enter(WindowScope::Secondary(42));
        let secondary = use_signal("__rui_button_focus_id", String::new);
        assert_eq!(secondary.peek(), "");
        secondary.set("sec".into());
        drop(_g2);

        assert_eq!(primary.peek(), "main");
        let _ = set_signal_key_scope(0);
    }

    #[test]
    fn nested_effect_rerun_keeps_owner_window_scope() {
        let (_owner, seen) = create_root(|| {
            bind_current_owner_window_key_scope(99);
            let seen = create_signal(Vec::<WindowScope>::new());
            let tick = create_signal(0u32);
            let seen2 = seen;
            create_effect(move || {
                let _ = tick.get();
                seen2.update(|v| {
                    v.push(active());
                });
            });
            // Initial run under create_root still has signal scope 0 unless we enter —
            // bind only stamps the Owner; run_effect restores it on (re)run.
            tick.set(1);
            seen
        });
        let scopes = seen.peek();
        assert!(
            scopes.iter().any(|s| *s == WindowScope::Secondary(99)),
            "effect re-run must see Secondary(99), got {scopes:?}"
        );
        let _ = set_signal_key_scope(0);
    }
}
