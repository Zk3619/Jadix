//! Optional window-level key handler for apps that need global shortcuts
//! (e.g. calculator digits) without stealing focus from every control.

use std::cell::RefCell;
use std::rc::Rc;

use winit::event::KeyEvent;

type KeyHandler = Rc<RefCell<dyn FnMut(&KeyEvent) -> bool>>;

thread_local! {
    static HANDLER: RefCell<Option<KeyHandler>> = const { RefCell::new(None) };
}

/// Register a handler invoked from the window key path before focused-widget dispatch.
/// Return `true` if the event was consumed.
pub fn set_window_key_handler(f: impl FnMut(&KeyEvent) -> bool + 'static) {
    HANDLER.with(|h| {
        *h.borrow_mut() = Some(Rc::new(RefCell::new(f)));
    });
}

pub fn clear_window_key_handler() {
    HANDLER.with(|h| *h.borrow_mut() = None);
}

pub(super) fn try_dispatch_window_key(event: &KeyEvent) -> bool {
    HANDLER.with(|h| {
        let handler = h.borrow().clone();
        match handler {
            Some(handler) => handler.borrow_mut()(event),
            None => false,
        }
    })
}

fn fullscreen_action(
    key: &winit::keyboard::Key,
    modifiers: winit::keyboard::ModifiersState,
    macos: bool,
    windows: bool,
    fullscreen: bool,
) -> Option<bool> {
    use winit::keyboard::{Key, ModifiersState, NamedKey};
    if !macos && modifiers.is_empty() && *key == Key::Named(NamedKey::F11) {
        return Some(!fullscreen);
    }
    if windows && modifiers == ModifiersState::ALT && *key == Key::Named(NamedKey::Enter) {
        return Some(!fullscreen);
    }
    if macos
        && modifiers == (ModifiersState::CONTROL | ModifiersState::META)
        && matches!(key, Key::Character(text) if text.eq_ignore_ascii_case("f"))
    {
        return Some(!fullscreen);
    }
    if fullscreen && modifiers.is_empty() && *key == Key::Named(NamedKey::Escape) {
        return Some(false);
    }
    None
}

impl super::App {
    pub(super) fn handle_fullscreen_shortcut(
        &mut self,
        id: winit::window::WindowId,
        event: &KeyEvent,
    ) -> bool {
        if !cfg!(any(
            windows,
            target_os = "macos",
            all(target_os = "linux", not(target_env = "ohos"))
        )) || event.state != winit::event::ElementState::Pressed
        {
            return false;
        }
        let Some(window) = super::runtime::window_handle(id) else {
            return false;
        };
        if !window
            .chrome
            .fullscreen_shortcuts
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return false;
        }
        let modifiers = self.window_modifiers.get(&id).copied().unwrap_or_default();
        let Some(fullscreen) = fullscreen_action(
            &event.logical_key,
            modifiers,
            cfg!(target_os = "macos"),
            cfg!(windows),
            window.is_fullscreen().unwrap_or(false),
        ) else {
            return false;
        };
        if event.repeat {
            return true;
        }
        let _scope = if self.window_registry.primary_id() == Some(id) {
            super::window_scope::enter_primary()
        } else {
            super::window_scope::enter_secondary_id(id)
        };
        if event.logical_key == winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape)
            && super::portal_overlay::try_close_top()
        {
            if let Ok(native) = window.upgrade() {
                native.request_redraw();
            }
            return true;
        }
        if let Err(error) = window.set_fullscreen(fullscreen) {
            log::warn!("fullscreen shortcut: {error}");
        }
        true
    }
}

#[cfg(test)]
#[path = "tests/window_keys.rs"]
mod tests;
