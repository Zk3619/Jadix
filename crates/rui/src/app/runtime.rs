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
                let mut opt = state_rc.borrow_mut();
                let state = opt.as_mut()?;
                #[cfg(debug_assertions)]
                state.apply_hot_reload_overrides_to_fragment(&mut new_fragment);
                Some(state.reconcile_subtree(anchor_id, new_fragment))
            }
            TreeHome::SecondarySlot => {
                let id = Self::secondary_slot_id()?;
                let secs = self.secondaries.upgrade()?;
                let mut secs = secs.borrow_mut();
                let sec = secs.get_mut(&id)?;
                Some(sec.reconcile_subtree(anchor_id, new_fragment))
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
                let mut opt = state_rc.borrow_mut();
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
    APP_RUNTIME.with(|c| {
        c.borrow()
            .as_ref()
            .and_then(|rt| rt.try_reconcile_subtree(anchor_id, new_fragment))
    })
}

pub fn request_redraw() {
    APP_RUNTIME.with(|c| {
        if let Some(rt) = c.borrow().as_ref() {
            rt.request_redraw();
        }
    });
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
pub(crate) fn with_active_winit_window<R>(f: impl FnOnce(&Window) -> R) -> Option<R> {
    let window: Arc<Window> = APP_RUNTIME.with(|c| {
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
    Some(f(&window))
}

pub fn try_update_element_by_template_id<F>(
    template_id: &str,
    patch: F,
) -> Option<crate::core::tree::LayoutStatus>
where
    F: FnOnce(&mut Element) -> crate::core::tree::LayoutStatus,
{
    APP_RUNTIME.with(|c| {
        c.borrow()
            .as_ref()
            .and_then(|rt| rt.try_update_element_by_template_id(template_id, patch))
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
