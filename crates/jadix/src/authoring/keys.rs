//! Node-owned window shortcuts, dispatched after releasing host tree borrows.

use crate::app::window_scope::{self, WindowScope};
use crate::core::reactivity::{current_owner, on_cleanup, with_owner, OwnerId};
use crate::core::tree::Element;
use std::cell::RefCell;
use std::rc::Rc;
use winit::event::ElementState;
use winit::keyboard::{Key, ModifiersState};

#[derive(Clone, Copy)]
pub struct Context<'key> {
    pub key: &'key Key,
    pub modifiers: ModifiersState,
    pub state: ElementState,
    pub repeat: bool,
}

type Callback = Rc<dyn Fn(Context<'_>) -> bool>;

#[derive(Clone)]
pub struct Handler {
    callback: Rc<RefCell<Option<Callback>>>,
    owner: Option<OwnerId>,
    window: WindowScope,
}

impl Handler {
    /// A consumed shortcut stops dispatch. Disposed Owners invalidate prepared handlers.
    pub fn dispatch(&self, context: Context<'_>) -> bool {
        let Some(callback) = self.callback.borrow().clone() else {
            return false;
        };
        let _scope = window_scope::enter(self.window);
        let run = || callback(context);
        self.owner.map_or_else(run, |owner| with_owner(owner, run))
    }
}

/// Append a shortcut to this subtree's root. Window dispatch visits nodes in tree order
/// and callbacks in attachment order, before focused-widget commands.
pub fn attach(element: &mut Element, callback: impl Fn(Context<'_>) -> bool + 'static) {
    let callback = Rc::new(RefCell::new(Some(Rc::new(callback) as Callback)));
    let owner = current_owner();
    if owner.is_some() {
        let weak = Rc::downgrade(&callback);
        on_cleanup(move || {
            if let Some(callback) = weak.upgrade() {
                let released = callback.borrow_mut().take();
                drop(released);
            }
        });
    }
    element.window_key_handlers.push(Handler {
        callback,
        owner,
        window: window_scope::active(),
    });
}

/// Snapshot mounted shortcuts without keeping a host tree borrow across user callbacks.
pub fn handlers(root: &Element) -> Vec<Handler> {
    let mut handlers = Vec::new();
    let mut stack = vec![root];
    while let Some(element) = stack.pop() {
        handlers.extend(element.window_key_handlers.iter().cloned());
        stack.extend(element.children.iter().rev());
    }
    handlers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::create_root;
    use std::cell::Cell;

    #[test]
    fn prepared_shortcuts_release_tree_borrows_and_expire_on_owner_disposal() {
        let tree = Rc::new(RefCell::new(Element::new()));
        let calls = Rc::new(Cell::new(0));
        let (owner, ()) = create_root(|| {
            let tree_callback = tree.clone();
            let calls = calls.clone();
            attach(&mut tree.borrow_mut(), move |_| {
                tree_callback.borrow_mut().set_attr("shortcut", "called");
                calls.set(calls.get() + 1);
                true
            });
        });
        let prepared = handlers(&tree.borrow());
        let key = Key::Character("b".into());
        let context = Context {
            key: &key,
            modifiers: ModifiersState::CONTROL,
            state: ElementState::Pressed,
            repeat: false,
        };
        assert!(prepared[0].dispatch(context));
        assert_eq!(calls.get(), 1);
        owner.dispose();
        assert!(!prepared[0].dispatch(context));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn shortcuts_follow_tree_and_attachment_order() {
        let order = Rc::new(RefCell::new(Vec::new()));
        let mut root = Element::new();
        for index in 0..3 {
            let mut child = Element::new();
            let order = order.clone();
            attach(&mut child, move |_| {
                order.borrow_mut().push(index);
                index == 1
            });
            root.children.push(child);
        }
        let key = Key::Character("b".into());
        let context = Context {
            key: &key,
            modifiers: ModifiersState::CONTROL,
            state: ElementState::Pressed,
            repeat: false,
        };
        assert!(handlers(&root)
            .into_iter()
            .any(|handler| handler.dispatch(context)));
        assert_eq!(*order.borrow(), vec![0, 1]);
    }
}
