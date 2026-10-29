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
