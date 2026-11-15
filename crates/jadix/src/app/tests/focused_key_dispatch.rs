use super::*;
use crate::core::reactivity::use_signal;
use crate::primitives::interactive::interactive;
use std::cell::RefCell;
use std::rc::Rc;

/// Regression for `RefCell already borrowed`: extract handlers under an outer borrow,
/// release it, then run — handlers may `signal.set` / nest `borrow_mut`.
#[test]
fn key_handlers_run_after_outer_borrow_released() {
    let state = Rc::new(RefCell::new(0u32));
    let state_cb = state.clone();
    let open = use_signal("tree_hit_key_reentry_open", || false);
    let root: Element = interactive(Element::new())
        .focus_id("k")
        .focusable(true)
        .on_click(move || {
            *state_cb.borrow_mut() += 1;
            open.set(true);
        })
        .into();

    // Outer immutable borrow (like input_controller before the fix).
    let outer = state.borrow();
    let handlers = focused_key_handlers(&root, "k").expect("handlers");
    assert!(handlers.focusable);
    let click = handlers.on_click.clone().expect("click");
    drop(outer); // must release before running
    click.borrow_mut()();
    assert_eq!(*state.borrow(), 1);
    assert!(open.peek());
}

#[test]
fn focused_activation_precedes_window_shortcuts_and_preserves_modified_keys() {
    use std::cell::Cell;
    use winit::keyboard::{Key, KeyCode, KeyLocation, ModifiersState, NamedKey, PhysicalKey};
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut root: Element = interactive(Element::new())
        .focus_id("reject")
        .focusable(true)
        .on_click({
            let calls = calls.clone();
            move || calls.borrow_mut().push("reject")
        })
        .into();
    crate::authoring::keys::attach(&mut root, {
        let calls = calls.clone();
        move |_| {
            calls.borrow_mut().push("allow");
            true
        }
    });
    let mut event = KeyEvent {
        physical_key: PhysicalKey::Code(KeyCode::Enter),
        logical_key: Key::Named(NamedKey::Enter),
        text: None,
        location: KeyLocation::Standard,
        state: ElementState::Pressed,
        repeat: false,
        text_with_all_modifiers: None,
        key_without_modifiers: Key::Named(NamedKey::Enter),
    };
    let mut focused = focused_key_handlers(&root, "reject").unwrap();
    let dispatch = |focused: Option<&FocusedKeyHandlers>, editing, event: &KeyEvent, modifiers| {
        run_window_key_handlers(
            crate::authoring::keys::handlers(&root),
            focused,
            editing,
            event,
            modifiers,
        )
    };
    for key in [Key::Named(NamedKey::Enter), Key::Character(" ".into())] {
        event.logical_key = key;
        assert!(dispatch(
            Some(&focused),
            false,
            &event,
            ModifiersState::SHIFT
        ));
        assert_eq!(calls.borrow_mut().drain(..).collect::<Vec<_>>(), ["reject"]);
    }
    event.repeat = true;
    assert!(dispatch(
        Some(&focused),
        false,
        &event,
        ModifiersState::empty()
    ));
    assert!(calls.borrow().is_empty());
    event.repeat = false;
    for modifiers in [
        ModifiersState::CONTROL,
        ModifiersState::ALT,
        ModifiersState::META,
    ] {
        assert!(dispatch(Some(&focused), false, &event, modifiers));
        assert_eq!(calls.borrow_mut().drain(..).collect::<Vec<_>>(), ["allow"]);
        assert!(!run_focused_key_handlers(&focused, &event, modifiers));
    }
    assert!(dispatch(
        Some(&focused),
        true,
        &event,
        ModifiersState::empty()
    ));
    assert!(dispatch(None, false, &event, ModifiersState::empty()));
    assert_eq!(
        calls.borrow_mut().drain(..).collect::<Vec<_>>(),
        ["allow", "allow"]
    );
    let keys = Rc::new(Cell::new(0));
    focused.on_key_down = Some(Rc::new(RefCell::new({
        let keys = keys.clone();
        move |_| {
            keys.set(keys.get() + 1);
            true
        }
    })));
    assert!(dispatch(
        Some(&focused),
        false,
        &event,
        ModifiersState::empty()
    ));
    assert_eq!(keys.get(), 1);
    assert!(calls.borrow().is_empty());
}
