use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use jadix::app::WindowLifecycleHook;
use winit::event::WindowEvent;
use winit::window::{Window, WindowId};

use crate::error::Result;
use crate::manager::{Builder, WindowStateManager};

struct JadixWindowStateHook {
    manager: Arc<WindowStateManager>,
    ids: Mutex<HashMap<WindowId, String>>,
}

impl JadixWindowStateHook {
    fn label_for(&self, id: WindowId) -> Option<String> {
        self.ids
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&id)
            .cloned()
    }

    fn forget(&self, id: WindowId) -> Option<String> {
        self.ids
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&id)
    }
}

impl WindowLifecycleHook for JadixWindowStateHook {
    fn on_created(&self, window: &Window, label: &str) {
        if self.manager.effective_label(label).is_none() {
            return;
        }
        self.ids
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(window.id(), label.to_string());
        if let Err(e) = self.manager.restore(label, window) {
            log::warn!("jadix_window_state: restore `{label}`: {e}");
        }
    }

    fn on_event(&self, window: &Window, event: &WindowEvent) {
        let Some(label) = self.label_for(window.id()) else {
            return;
        };
        match event {
            WindowEvent::Moved(pos) => {
                if window.is_minimized().unwrap_or(false) {
                    return;
                }
                self.manager.note_moved(&label, pos.x, pos.y);
            }
            WindowEvent::Resized(size) => {
                let maximized = if cfg!(target_os = "macos")
                    && (!window.is_decorated() || !window.is_resizable())
                {
                    false
                } else {
                    window.is_maximized()
                };
                let minimized = window.is_minimized().unwrap_or(false);
                self.manager
                    .note_resized(&label, size.width, size.height, maximized, minimized);
            }
            _ => {}
        }
    }

    fn on_closing(&self, window: &Window) {
        let Some(label) = self.forget(window.id()) else {
            return;
        };
        if let Err(e) = self.manager.capture(&label, window) {
            log::warn!("jadix_window_state: capture `{label}`: {e}");
        }
        if let Err(e) = self.manager.save_if_autosave() {
            log::warn!("jadix_window_state: save `{label}`: {e}");
        }
    }

    fn on_exit(&self) {
        if let Err(e) = self.manager.save() {
            log::warn!("jadix_window_state: save on exit: {e}");
        }
    }
}

pub fn install(builder: Builder) -> Result<Arc<WindowStateManager>> {
    let manager = Arc::new(builder.build()?);
    let hook = JadixWindowStateHook {
        manager: Arc::clone(&manager),
        ids: Mutex::new(HashMap::new()),
    };
    jadix::app::set_window_lifecycle_hook(Some(Arc::new(hook)));
    Ok(manager)
}
