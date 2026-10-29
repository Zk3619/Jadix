//! Thread-local access to the active window [`AppState`] for fine-grained UI updates
//! (e.g. `reconcile_subtree` + redraw), set in [`crate::app::lifecycle`] after `resumed`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Weak;
use std::sync::Arc;

use winit::window::{Window, WindowId};

use crate::core::tree::{Element, LayoutStatus};

use super::secondary_window::SecondaryWindowState;
use super::window_scope::{self, TreeHome};

thread_local! {
    static APP_RUNTIME: RefCell<Option<AppRuntime>> = const { RefCell::new(None) };
    // OS operations must work even while input dispatch borrows a UI tree.
    static WINDOWS: RefCell<HashMap<WindowId, super::multi_window::WindowHandle>> = RefCell::new(HashMap::new());
    static PRIMARY: std::cell::Cell<Option<WindowId>> = const { std::cell::Cell::new(None) };
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
                            target: "jadix::menu_sub",
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
                let mut secs = secs.try_borrow_mut().ok()?;
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
                let mut secs = secs.try_borrow_mut().ok()?;
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
    super::invalidation::reset();
    APP_RUNTIME.with(|c| *c.borrow_mut() = None);
    for window in window_handles() {
        super::entry::forget_window_config(window.id());
        super::frame_stats::forget_window_presentations(window.id());
    }
    WINDOWS.with(|c| c.borrow_mut().clear());
    PRIMARY.with(|c| c.set(None));
}

pub(crate) fn register_window(window: &Arc<dyn Window>, primary: bool) {
    WINDOWS.with(|c| {
        c.borrow_mut()
            .insert(window.id(), super::multi_window::WindowHandle::new(window))
    });
    if primary {
        PRIMARY.with(|c| c.set(Some(window.id())));
    }
}

pub(crate) fn unregister_window(id: WindowId) {
    if PRIMARY.with(|p| p.get()) == Some(id) {
        super::invalidation::forget(window_scope::WindowScope::Primary);
        PRIMARY.with(|p| p.set(None));
    }
    WINDOWS.with(|c| c.borrow_mut().remove(&id));
    super::entry::forget_window_config(id);
    super::frame_stats::forget_window_presentations(id);
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
        // Match the real AppState path: pointer handlers and drag-end callers
        // may still hold the live tree; defer rather than panicking on remount.
        let mut root = root.try_borrow_mut().ok()?;
        let want = anchor_id.replace('\\', "/").to_ascii_lowercase();
        let mut new_fragment = new_fragment;
        let Some(needs_portal_hoist) = find_template_id_mut(&mut root, &want).map(|slot| {
            // Match AppState::reconcile_subtree: identity-reconcile then assign.
            // Wholesale `*slot = fresh` hid slot↔portal pairing (equal child len).
            let _ = crate::app::test_support::reconcile_elements(&mut new_fragment, slot);
            let needs = element_contains_jadix_portal(&new_fragment);
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
fn element_contains_jadix_portal(el: &Element) -> bool {
    crate::core::tree::find_by_attr(el, "jadix_portal", "1").is_some()
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

pub(super) fn apply_frame_invalidation(
    present: bool,
    caches: bool,
    repaint: bool,
    nodes: &HashMap<String, bool>,
) {
    APP_RUNTIME.with(|slot| {
        let rt = slot.borrow().clone();
        let Some(rt) = rt else { return };
        match window_scope::tree_home_for(window_scope::active()) {
            TreeHome::AppState => {
                let Some(state) = rt.state.upgrade() else {
                    return;
                };
                let mut state = state.borrow_mut();
                let Some(state) = state.as_mut() else { return };
                state.force_present |= present;
                if repaint {
                    super::invalidation::patch_node(&mut state.root_element, false);
                    state.layout_status = state.layout_status.merge(LayoutStatus::StyleChanged);
                    state.mark_full_redraw();
                }
                if caches {
                    state.bust_text_caches();
                }
                for (id, layout) in nodes {
                    state.update_element_by_template_id(id, |el| {
                        super::invalidation::patch_node(el, *layout)
                    });
                    if *layout {
                        let want = id.replace('\\', "/").to_ascii_lowercase();
                        state.mark_requested_layout(&want);
                    }
                }
            }
            TreeHome::SecondarySlot => {
                let Some(id) = AppRuntime::secondary_slot_id() else {
                    return;
                };
                let Some(secs) = rt.secondaries.upgrade() else {
                    return;
                };
                let mut secs = secs.borrow_mut();
                let Some(sec) = secs.get_mut(&id) else { return };
                if repaint {
                    super::invalidation::patch_node(&mut sec.root_element, false);
                    sec.layout_status = sec.layout_status.merge(LayoutStatus::StyleChanged);
                    sec.layout_tree.invalidate_collect_skip();
                }
                if caches {
                    sec.bust_text_caches();
                }
                for (id, layout) in nodes {
                    sec.update_element_by_template_id(id, |el| {
                        super::invalidation::patch_node(el, *layout)
                    });
                    if *layout {
                        let want = id.replace('\\', "/").to_ascii_lowercase();
                        if let Some(el) =
                            crate::core::tree::find_by_template_id_mut(&mut sec.root_element, &want)
                        {
                            sec.layout_tree.mark_subtree_layout_dirty(el);
                        }
                    }
                }
            }
        }
    });
}

/// Called after style invalidation while no window tree is borrowed.
pub(crate) fn sync_window_theme() {
    APP_RUNTIME.with(|slot| {
        let rt = slot.borrow().clone();
        let Some(rt) = rt else { return };
        if let Some(state) = rt.state.upgrade() {
            if let Ok(mut state) = state.try_borrow_mut() {
                if let Some(state) = state.as_mut() {
                    state.sync_window_theme();
                }
            }
        }
        if let Some(secondaries) = rt.secondaries.upgrade() {
            if let Ok(mut secondaries) = secondaries.try_borrow_mut() {
                for secondary in secondaries.values_mut() {
                    secondary.sync_window_theme();
                }
            }
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
    let window = active_window()?;
    Some(f(window.as_ref()))
}

fn active_window() -> Option<Arc<dyn Window>> {
    current_window_handle()?.upgrade().ok()
}

pub(crate) fn current_window_handle() -> Option<super::multi_window::WindowHandle> {
    let id = match window_scope::tree_home_for(window_scope::active()) {
        TreeHome::AppState => PRIMARY.with(|c| c.get()),
        TreeHome::SecondarySlot => window_scope::window_id_for_active_token(),
    }?;
    WINDOWS.with(|c| c.borrow().get(&id).filter(|w| w.is_alive()).cloned())
}

pub(super) fn window_handle(id: WindowId) -> Option<super::multi_window::WindowHandle> {
    WINDOWS.with(|c| c.borrow().get(&id).filter(|w| w.is_alive()).cloned())
}

pub(crate) fn window_handles() -> Vec<super::multi_window::WindowHandle> {
    let mut windows = WINDOWS.with(|c| {
        c.borrow()
            .values()
            .filter(|w| w.is_alive())
            .cloned()
            .collect::<Vec<_>>()
    });
    let primary = PRIMARY.with(|c| c.get());
    windows.sort_by_key(|w| (Some(w.id()) != primary, w.id()));
    windows
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

#[cfg(all(debug_assertions, feature = "devtools"))]
pub(crate) fn replace_secondary_root(id: WindowId, root: Element) {
    let secondaries = APP_RUNTIME.with(|runtime| {
        runtime
            .borrow()
            .as_ref()
            .and_then(|runtime| runtime.secondaries.upgrade())
    });
    if let Some(secondaries) = secondaries {
        if let Ok(mut windows) = secondaries.try_borrow_mut() {
            if let Some(window) = windows.get_mut(&id) {
                window.replace_root(root);
                window.window.request_redraw();
            }
        }
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
#[cfg(all(debug_assertions, feature = "devtools"))]
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
#[cfg(all(debug_assertions, feature = "devtools"))]
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
#[path = "tests/runtime.rs"]
mod tests;
