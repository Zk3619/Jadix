use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::flags::StateFlags;
use jadix::app::WindowLifecycleHook;
use winit::event::WindowEvent;
use winit::window::{Window, WindowAttributes, WindowId};

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
    fn on_creating(&self, label: &str, attrs: &mut WindowAttributes) {
        let Some(state) = self.manager.initial_state(label) else {
            return;
        };
        let flags = self.manager.flags();
        // Persisted client dimensions are physical pixels. Supplying them to native
        // creation also gives DWM's opening animation the correct normal bounds.
        if flags.contains(StateFlags::SIZE) && state.width > 0 && state.height > 0 {
            attrs.surface_size =
                Some(winit::dpi::PhysicalSize::new(state.width, state.height).into());
        }
        if flags.contains(StateFlags::MAXIMIZED) {
            attrs.maximized = state.maximized;
        }
        if flags.contains(StateFlags::DECORATIONS) {
            attrs.decorations = state.decorated;
        }
        if flags.contains(StateFlags::FULLSCREEN) {
            attrs.fullscreen = state
                .fullscreen
                .then_some(winit::monitor::Fullscreen::Borderless(None));
        }
        // Monitor intersection checks still run in on_created, before first show.
    }
    fn on_created(&self, window: &dyn Window, label: &str) {
        if self.manager.effective_label(label).is_none() {
            return;
        }
        self.ids
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(window.id(), label.to_string());
        if let Err(e) = self.manager.restore_before_show(label, window) {
            log::warn!("jadix_window_state: restore `{label}`: {e}");
        }
    }

    fn on_event(&self, window: &dyn Window, event: &WindowEvent) {
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
            WindowEvent::SurfaceResized(size) => {
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

    fn on_closing(&self, window: &dyn Window) {
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
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_size_is_used_before_native_creation_and_respects_restore_policy() {
        let path = std::env::temp_dir().join(format!(
            "jadix-initial-attrs-{}-{}.toml",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(
            &path,
            "[windows.main]\nwidth = 713\nheight = 451\nmaximized = true\ndecorated = false\n",
        )
        .unwrap();
        let prepare = |manager| {
            let hook = JadixWindowStateHook {
                manager: Arc::new(manager),
                ids: Mutex::new(HashMap::new()),
            };
            let mut attrs = WindowAttributes::default().with_visible(false);
            hook.on_creating("main", &mut attrs);
            attrs
        };
        let attrs = prepare(Builder::new().with_path(&path).build().unwrap());
        assert_eq!(
            attrs.surface_size,
            Some(winit::dpi::PhysicalSize::new(713, 451).into())
        );
        assert!(attrs.maximized);
        assert!(!attrs.decorations);
        assert!(!attrs.visible);
        let size_only = prepare(
            Builder::new()
                .with_path(&path)
                .with_state_flags(StateFlags::SIZE)
                .build()
                .unwrap(),
        );
        assert_eq!(size_only.surface_size, attrs.surface_size);
        assert!(!size_only.maximized);
        assert!(size_only.decorations);
        let skipped = prepare(
            Builder::new()
                .with_path(&path)
                .skip_initial_state("main")
                .build()
                .unwrap(),
        );
        assert_eq!(
            skipped.surface_size,
            WindowAttributes::default().surface_size
        );
        let denied = prepare(
            Builder::new()
                .with_path(&path)
                .with_denylist(&["main"])
                .build()
                .unwrap(),
        );
        assert_eq!(
            denied.surface_size,
            WindowAttributes::default().surface_size
        );
        std::fs::remove_file(path).unwrap();
    }
}
