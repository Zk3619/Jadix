//! Thread-local access to the active window [`AppState`] for fine-grained UI updates
//! (e.g. `reconcile_subtree` + redraw), set in [`crate::app::lifecycle`] after `resumed`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Weak;
use std::sync::Arc;

use winit::window::{Window, WindowId};

use crate::core::tree::Element;

use super::secondary_window::SecondaryWindowState;
use super::window_scope::{self, TreeHome};

thread_local! {
    static APP_RUNTIME: RefCell<Option<AppRuntime>> = const { RefCell::new(None) };
}

/// Weak handle to primary [`AppState`] plus secondary trees.
#[derive(Clone)]
pub struct AppRuntime {
    state: Weak<RefCell<Option<super::state::AppState>>>,
    secondaries: Weak<RefCell<HashMap<WindowId, SecondaryWindowState>>>,
}

impl AppRuntime {
    pub(in crate::app) fn new(
        state: Weak<RefCell<Option<super::state::AppState>>>,
        secondaries: Weak<RefCell<HashMap<WindowId, SecondaryWindowState>>>,
    ) -> Self {
        Self { state, secondaries }
    }

    fn secondary_slot_id() -> Option<WindowId> {
        window_scope::active_window_id()
    }

    pub fn try_reconcile_subtree(
        &self,
        anchor_id: &str,
        #[allow(unused_mut)] mut new_fragment: Element,
    ) -> Option<crate::core::tree::LayoutStatus> {
        match window_scope::tree_home_for(window_scope::active()) {
            TreeHome::AppState => {
                let state_rc = self.state.upgrade()?;
                // Root rebuild / input handlers may already hold this RefCell
                // (Inspector overlay used to panic here via nested reactive_view).
                let mut opt = match state_rc.try_borrow_mut() {
                    Ok(b) => b,
                    Err(_) => {
                        log::debug!(
                            target: "rui::menu_sub",
                            "reconcile BORROW_FAIL anchor={anchor_id}"
                        );
                        return None;
                    }
                };
                let state = opt.as_mut()?;
                #[cfg(debug_assertions)]
                state.apply_hot_reload_overrides_to_fragment(&mut new_fragment);
                state.reconcile_subtree(anchor_id, new_fragment)
            }
            TreeHome::SecondarySlot => {
                let id = Self::secondary_slot_id()?;
                let secs = self.secondaries.upgrade()?;
                let mut secs = secs.borrow_mut();
                let sec = secs.get_mut(&id)?;
                sec.reconcile_subtree(anchor_id, new_fragment)
            }
        }
    }

    pub fn request_redraw(&self) {
        match window_scope::tree_home_for(window_scope::active()) {
            TreeHome::AppState => {
                if let Some(state_rc) = self.state.upgrade() {
                    // Layout/collect already holds `borrow_mut`; skip rather than panic.
                    if let Ok(opt) = state_rc.try_borrow() {
                        if let Some(state) = opt.as_ref() {
                            state.window.request_redraw();
                        }
                    }
                }
            }
            TreeHome::SecondarySlot => {
                if let Some(id) = Self::secondary_slot_id() {
                    if let Some(secs) = self.secondaries.upgrade() {
                        if let Ok(secs) = secs.try_borrow() {
                            if let Some(sec) = secs.get(&id) {
                                sec.window.request_redraw();
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn request_redraw_all(&self) {
        if let Some(state_rc) = self.state.upgrade() {
            if let Ok(opt) = state_rc.try_borrow() {
                if let Some(state) = opt.as_ref() {
                    state.window.request_redraw();
                }
            }
        }
        if let Some(secs) = self.secondaries.upgrade() {
            if let Ok(secs) = secs.try_borrow() {
                for sec in secs.values() {
                    sec.window.request_redraw();
                }
            }
        }
    }

    pub fn try_update_element_by_template_id<F>(
        &self,
        template_id: &str,
        patch: F,
    ) -> Option<crate::core::tree::LayoutStatus>
    where
        F: FnOnce(&mut Element) -> crate::core::tree::LayoutStatus,
    {
        match window_scope::tree_home_for(window_scope::active()) {
            TreeHome::AppState => {
                let state_rc = self.state.upgrade()?;
                let mut opt = state_rc.try_borrow_mut().ok()?;
                let state = opt.as_mut()?;
                Some(state.update_element_by_template_id(template_id, patch))
            }
            TreeHome::SecondarySlot => {
                let id = Self::secondary_slot_id()?;
                let secs = self.secondaries.upgrade()?;
                let mut secs = secs.borrow_mut();
                let sec = secs.get_mut(&id)?;
                Some(sec.update_element_by_template_id(template_id, patch))
            }
        }
    }
}

pub fn set_app_runtime(rt: AppRuntime) {
    APP_RUNTIME.with(|c| *c.borrow_mut() = Some(rt));
}

pub fn clear_app_runtime() {
    APP_RUNTIME.with(|c| *c.borrow_mut() = None);
}

/// Returns [`None`] if no runtime is installed (e.g. before first `resumed` or after teardown).
pub fn try_reconcile_subtree(
    anchor_id: &str,
    new_fragment: Element,
) -> Option<crate::core::tree::LayoutStatus> {
    let has_app = APP_RUNTIME.with(|c| c.borrow().is_some());
    if has_app {
        return APP_RUNTIME.with(|c| {
            c.borrow()
                .as_ref()
                .and_then(|rt| rt.try_reconcile_subtree(anchor_id, new_fragment))
        });
    }
    #[cfg(test)]
    {
        return test_reconcile_subtree(anchor_id, new_fragment);
    }
    #[cfg(not(test))]
    {
        let _ = (anchor_id, new_fragment);
        None
    }
}

#[cfg(test)]
thread_local! {
    static TEST_RECONCILE_ROOT: RefCell<Option<std::rc::Rc<RefCell<Element>>>> =
        const { RefCell::new(None) };
}

/// Install an in-memory tree so [`try_reconcile_subtree`] / [`try_update_element_by_template_id`]
/// work without a live `AppState` (component overlay / bind harnesses).
#[cfg(test)]
pub fn install_test_reconcile_root(root: std::rc::Rc<RefCell<Element>>) {
    TEST_RECONCILE_ROOT.with(|c| *c.borrow_mut() = Some(root));
}

#[cfg(test)]
pub fn clear_test_reconcile_root() {
    TEST_RECONCILE_ROOT.with(|c| *c.borrow_mut() = None);
}

#[cfg(test)]
fn test_reconcile_subtree(
    anchor_id: &str,
    new_fragment: Element,
) -> Option<crate::core::tree::LayoutStatus> {
    TEST_RECONCILE_ROOT.with(|c| {
        let root = c.borrow().as_ref()?.clone();
        let mut root = root.borrow_mut();
        let want = anchor_id.replace('\\', "/").to_ascii_lowercase();
        let mut new_fragment = new_fragment;
        let Some(needs_portal_hoist) = find_template_id_mut(&mut root, &want).map(|slot| {
            // Match AppState::reconcile_subtree: identity-reconcile then assign.
            // Wholesale `*slot = fresh` hid slot↔portal pairing (equal child len).
            let _ = crate::app::test_support::reconcile_elements(&mut new_fragment, slot);
            let needs = element_contains_rui_portal(&new_fragment);
            *slot = new_fragment;
            needs
        }) else {
            // Match AppState: miss must not look like a successful apply.
            return None;
        };
        if needs_portal_hoist {
            let _ = crate::layout::layout_tree::hoist_portal_subtrees(&mut root);
        }
        if let Some(path) = crate::core::tree::find_path_in_tree(&root, |n| {
            n.template_id
                .as_deref()
                .is_some_and(|t| t.replace('\\', "/").to_ascii_lowercase() == want)
        }) {
            crate::layout::collect_cache::invalidate_collect_paint_hash_path(&mut root, &path);
        }
        Some(crate::core::tree::LayoutStatus::StructureChanged)
    })
}

#[cfg(test)]
fn element_contains_rui_portal(el: &Element) -> bool {
    crate::core::tree::find_by_attr(el, "rui_portal", "1").is_some()
}

#[cfg(test)]
fn find_template_id_mut<'a>(el: &'a mut Element, want: &str) -> Option<&'a mut Element> {
    crate::core::tree::find_in_tree_mut(el, |n| {
        n.template_id
            .as_deref()
            .is_some_and(|t| t.replace('\\', "/").to_ascii_lowercase() == want)
    })
}

pub fn request_redraw() {
    APP_RUNTIME.with(|c| {
        if let Some(rt) = c.borrow().as_ref() {
            rt.request_redraw();
        }
    });
}

/// Promote nested portals (e.g. menu submenu) while AppState is free to borrow.
/// Safe no-op when no runtime is installed or the tree is already borrowed.
pub fn try_hoist_portals_now() {
    if !crate::core::reactivity::runtime_tls_alive() {
        return;
    }
    APP_RUNTIME.with(|c| {
        let rt_slot = c.borrow();
        let Some(rt) = rt_slot.as_ref() else {
            return;
        };
        match window_scope::tree_home_for(window_scope::active()) {
            TreeHome::AppState => {
                let Some(state_rc) = rt.state.upgrade() else {
                    return;
                };
                drop(rt_slot);
                let Ok(mut opt) = state_rc.try_borrow_mut() else {
                    return;
                };
                let Some(state) = opt.as_mut() else {
                    return;
                };
                let n = crate::app::state::hoist_portal_subtrees_refreshing_scroll_flags(
                    &mut state.root_element,
                );
                if n > 0 {
                    state.mark_portal_membership_dirty();
                    state.window.request_redraw();
                }
            }
            TreeHome::SecondarySlot => {
                let Some(id) = window_scope::active_window_id() else {
                    return;
                };
                let Some(secs) = rt.secondaries.upgrade() else {
                    return;
                };
                drop(rt_slot);
                let Ok(mut secs) = secs.try_borrow_mut() else {
                    return;
                };
                let Some(sec) = secs.get_mut(&id) else {
                    return;
                };
                let n = crate::app::state::hoist_portal_subtrees_refreshing_scroll_flags(
                    &mut sec.root_element,
                );
                if n > 0 {
                    sec.window.request_redraw();
                }
            }
        }
    });
    #[cfg(test)]
    {
        TEST_RECONCILE_ROOT.with(|c| {
            if let Some(root) = c.borrow().as_ref() {
                if let Ok(mut tree) = root.try_borrow_mut() {
                    let _ = crate::layout::layout_tree::hoist_portal_subtrees(&mut tree);
                }
            }
        });
    }
}

pub fn request_redraw_all() {
    APP_RUNTIME.with(|c| {
        if let Some(rt) = c.borrow().as_ref() {
            rt.request_redraw_all();
        }
    });
}

/// Clone the active [`Window`] handle (primary or secondary) and run `f`.
///
/// Drops AppState / secondary borrows before calling `f` so OS APIs that re-enter
/// the event loop (`drag_window`, DWM) cannot `RefCell`-panic.
pub(crate) fn with_active_winit_window<R>(f: impl FnOnce(&dyn Window) -> R) -> Option<R> {
    let window: Arc<dyn Window> = APP_RUNTIME.with(|c| {
        let rt = c.borrow();
        let rt = rt.as_ref()?;
        match window_scope::tree_home_for(window_scope::active()) {
            TreeHome::AppState => {
                let state_rc = rt.state.upgrade()?;
                let opt = state_rc.try_borrow().ok()?;
                Some(opt.as_ref()?.window.clone())
            }
            TreeHome::SecondarySlot => {
                let id = window_scope::active_window_id()?;
                let secs = rt.secondaries.upgrade()?;
                let secs = secs.try_borrow().ok()?;
                Some(secs.get(&id)?.window.clone())
            }
        }
    })?;
    Some(f(window.as_ref()))
}

#[cfg(test)]
fn test_update_element_by_template_id<F>(
    template_id: &str,
    patch: F,
) -> Option<crate::core::tree::LayoutStatus>
where
    F: FnOnce(&mut Element) -> crate::core::tree::LayoutStatus,
{
    TEST_RECONCILE_ROOT.with(|c| {
        let root = c.borrow().as_ref()?.clone();
        // Match AppState: `try_borrow_mut` fail is `None` (idle retry), not a panic.
        let mut root = root.try_borrow_mut().ok()?;
        let want = template_id.replace('\\', "/").to_ascii_lowercase();
        match find_template_id_mut(&mut root, &want) {
            Some(slot) => Some(patch(slot)),
            None => {
                // Match AppState: node not in tree is Clean + miss, not idle retry.
                crate::app::reactive::note_bind_miss(template_id);
                Some(crate::core::tree::LayoutStatus::Clean)
            }
        }
    })
}

pub fn try_update_element_by_template_id<F>(
    template_id: &str,
    patch: F,
) -> Option<crate::core::tree::LayoutStatus>
where
    F: FnOnce(&mut Element) -> crate::core::tree::LayoutStatus,
{
    let has_app = APP_RUNTIME.with(|c| c.borrow().is_some());
    if has_app {
        return APP_RUNTIME.with(|c| {
            c.borrow()
                .as_ref()
                .and_then(|rt| rt.try_update_element_by_template_id(template_id, patch))
        });
    }
    #[cfg(test)]
    {
        return test_update_element_by_template_id(template_id, patch);
    }
    #[cfg(not(test))]
    {
        let _ = (template_id, patch);
        None
    }
}

/// Read-only access to the primary window UI root (Inspector snapshots).
pub fn with_primary_root<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&Element) -> R,
{
    APP_RUNTIME.with(|c| {
        let guard = c.borrow();
        let rt = guard.as_ref()?;
        let state_rc = rt.state.upgrade()?;
        let opt = state_rc.try_borrow().ok()?;
        let state = opt.as_ref()?;
        Some(f(&state.root_element))
    })
}

/// Apply Inspector style patch to a node by child-index path (P1).
/// Returns whether the patch was applied (false if runtime/state busy or path missing).
#[cfg(feature = "devtools")]
pub fn patch_devtools_style(path: &[usize], patch: crate::app::devtools::StylePatch) -> bool {
    APP_RUNTIME.with(|c| {
        let guard = c.borrow();
        let Some(rt) = guard.as_ref() else {
            return false;
        };
        let Some(state_rc) = rt.state.upgrade() else {
            return false;
        };
        let Ok(mut opt) = state_rc.try_borrow_mut() else {
            return false;
        };
        let Some(state) = opt.as_mut() else {
            return false;
        };
        if state.patch_devtools_element_at_path(path, patch) {
            state.window.request_redraw();
            true
        } else {
            false
        }
    })
}

/// Apply Inspector CSSOM text to a node by child-index path (P2).
/// Returns the canonical dump of the node's style after apply.
#[cfg(feature = "devtools")]
pub fn patch_devtools_cssom(path: &[usize], css: &str) -> Option<String> {
    APP_RUNTIME.with(|c| {
        let guard = c.borrow();
        let rt = guard.as_ref()?;
        let state_rc = rt.state.upgrade()?;
        let mut opt = state_rc.try_borrow_mut().ok()?;
        let state = opt.as_mut()?;
        let dump = state.patch_devtools_cssom_at_path(path, css)?;
        state.window.request_redraw();
        Some(dump)
    })
}

/// Install a dummy runtime whose `AppState` cell is already `borrow_mut`, then run `f`.
/// Used to lock the "layout collect must not `runtime::request_redraw`" seam.
#[cfg(test)]
pub(crate) fn call_while_app_state_mutably_borrowed(f: impl FnOnce()) {
    use std::rc::Rc;
    let state: Rc<RefCell<Option<super::state::AppState>>> = Rc::new(RefCell::new(None));
    let secondaries: Rc<RefCell<HashMap<WindowId, SecondaryWindowState>>> =
        Rc::new(RefCell::new(HashMap::new()));
    set_app_runtime(AppRuntime::new(
        Rc::downgrade(&state),
        Rc::downgrade(&secondaries),
    ));
    let _guard = state.borrow_mut();
    let _secs = secondaries.borrow_mut();
    f();
    drop(_secs);
    drop(_guard);
    clear_app_runtime();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_redraw_while_state_mutably_borrowed_does_not_panic() {
        call_while_app_state_mutably_borrowed(|| {
            request_redraw();
            request_redraw_all();
        });
    }
}
