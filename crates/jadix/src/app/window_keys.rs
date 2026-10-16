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
