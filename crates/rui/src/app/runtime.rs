//! Thread-local access to the active window [`AppState`] for fine-grained UI updates
//! (e.g. `reconcile_subtree` + redraw), set in [`crate::app::lifecycle`] after `resumed`.

use std::cell::RefCell;
use std::rc::Weak;

use crate::core::tree::Element;

thread_local! {
    static APP_RUNTIME: RefCell<Option<AppRuntime>> = const { RefCell::new(None) };
}

/// Weak handle to `Rc<RefCell<Option<AppState>>>` for the running application.
#[derive(Clone)]
pub struct AppRuntime {
    state: Weak<RefCell<Option<super::state::AppState>>>,
}

impl AppRuntime {
    pub(in crate::app) fn new(state: Weak<RefCell<Option<super::state::AppState>>>) -> Self {
        Self { state }
    }

    pub fn try_reconcile_subtree(
        &self,
        anchor_id: &str,
        new_fragment: Element,
    ) -> Option<crate::core::tree::LayoutStatus> {
        let state_rc = self.state.upgrade()?;
        let mut opt = state_rc.borrow_mut();
        let state = opt.as_mut()?;
        Some(state.reconcile_subtree(anchor_id, new_fragment))
    }

    pub fn request_redraw(&self) {
        if let Some(state_rc) = self.state.upgrade() {
            if let Some(state) = state_rc.borrow().as_ref() {
                state.window.request_redraw();
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
        let state_rc = self.state.upgrade()?;
        let mut opt = state_rc.borrow_mut();
        let state = opt.as_mut()?;
        Some(state.update_element_by_template_id(template_id, patch))
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
