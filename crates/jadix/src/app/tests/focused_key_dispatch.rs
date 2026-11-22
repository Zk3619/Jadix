use super::*;
use crate::core::reactivity::use_signal;
use crate::primitives::interactive::interactive;
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn semantic_link_shares_pointer_keyboard_activation_and_reactive_disable() {
    use crate::app::pointer_events::{
        HitPathEntry, MouseEvent, PointerKind, dispatch_pointer, resolve_click_from_path,
    };
    use crate::primitives::{apply_disabled, link};
    use winit::{
        event::MouseButton,
        keyboard::{Key, KeyCode, KeyLocation, ModifiersState, NamedKey, PhysicalKey},
    };
    let calls = Rc::new(RefCell::new(Vec::<String>::new()));
    let child: Element = link("semantic-link")
        .text("文档")
        .href("app:docs")
        .open_with({
            let calls = calls.clone();
            move |href| calls.borrow_mut().push(href.into())
        })
        .on_activate({
            let calls = calls.clone();
            move || calls.borrow_mut().push("activate".into())
        })
        .into();
    let mut root = Element::new()
        .on_click({
            let calls = calls.clone();
            move || calls.borrow_mut().push("parent".into())
        })
        .child(child);
    let pointer_click = |root: &Element| {
        let path = vec![
            HitPathEntry::from_element(root, [0.0; 4]),
            HitPathEntry::from_element(&root.children[0], [0.0; 4]),
        ];
        let mut up = MouseEvent::new(0.0, 0.0, MouseButton::Left, 1);
        dispatch_pointer(&path, &mut up, PointerKind::Up);
        if !up.is_consumed() {
            if let Some(callback) = resolve_click_from_path(root, &path).0 {
                callback.borrow_mut()();
            }
        }
        up.is_consumed()
    };
    assert!(!pointer_click(&root));
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
    for key in [Key::Named(NamedKey::Enter), Key::Character(" ".into())] {
        event.logical_key = key;
        assert!(run_focused_key_handlers(
            &focused_key_handlers(&root, "semantic-link").unwrap(),
            &event,
            ModifiersState::empty()
        ));
    }
    event.repeat = true;
    assert!(!run_focused_key_handlers(
        &focused_key_handlers(&root, "semantic-link").unwrap(),
        &event,
        ModifiersState::empty()
    ));
    assert_eq!(
        &*calls.borrow(),
        &[
            "app:docs", "activate", "app:docs", "activate", "app:docs", "activate"
        ]
    );
    calls.borrow_mut().clear();
    let retained = root.children[0].on_click.clone().unwrap();
    apply_disabled(&mut root.children[0], true);
    retained.borrow_mut()();
    assert!(pointer_click(&root));
    assert!(!root.children[0].focusable);
    assert!(root.children[0].on_click.is_none());
    assert!(calls.borrow().is_empty());
    apply_disabled(&mut root.children[0], false);
    assert!(root.children[0].focusable);
    assert!(!pointer_click(&root));
    assert_eq!(&*calls.borrow(), &["app:docs", "activate"]);
    let passive: Element = link("passive-link").href("https://example.test").into();
    assert!(passive.on_click.is_none());
    let mut untabbed: Element = link("untabbed-link").tab_stop(false).into();
    apply_disabled(&mut untabbed, true);
    apply_disabled(&mut untabbed, false);
    assert!(!untabbed.focusable);
}

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

#[test]
fn escape_respects_top_overlay_focus_consumption_reentry_and_window_scope() {
    use crate::app::{
        portal_overlay as overlays,
        window_scope::{self, WindowScope},
    };
    use crate::core::reactivity::{Signal, create_root};
    use std::cell::Cell;
    use winit::keyboard::{Key, KeyCode, KeyLocation, NamedKey, PhysicalKey};

    for scope in [WindowScope::Primary, WindowScope::Secondary(83017)] {
        let _scope = window_scope::enter(scope);
        let _owner = create_root(|| {
            let parent = Signal::new(true);
            let child = Signal::new(true);
            let consume = Rc::new(Cell::new(true));
            let calls = Rc::new(Cell::new(0));
            let outer = Rc::new(RefCell::new(0));
            let parent_z = overlays::portal_sync(
                "escape-parent",
                parent,
                overlays::OverlayKind::Modal,
                overlays::OverlayMode::Stacked,
            )
            .unwrap();
            let mut panel = Element::new().on_key_down({
                let consume = consume.clone();
                let calls = calls.clone();
                let outer = outer.clone();
                move |_| {
                    *outer.borrow_mut() += 1;
                    calls.set(calls.get() + 1);
                    consume.get()
                }
            });
            panel.focus_id = Some("escape-input".into());
            panel.focusable = true;
            panel.style.z_index = parent_z.panel;
            let mut portals = Element::new().child(panel);
            portals.template_id = Some(crate::layout::layout_tree::PORTAL_LAYER_ID.into());
            let mut root = Element::new().child(portals);
            let mut event = KeyEvent {
                physical_key: PhysicalKey::Code(KeyCode::Escape),
                logical_key: Key::Named(NamedKey::Escape),
                text: None,
                location: KeyLocation::Standard,
                state: ElementState::Pressed,
                repeat: false,
                text_with_all_modifiers: None,
                key_without_modifiers: Key::Named(NamedKey::Escape),
            };
            let panel_z = overlays::top_stacked_panel_z();
            let handlers = {
                let _borrow = outer.borrow();
                focused_escape_handlers(&root, "escape-input", panel_z).unwrap()
            };
            assert!(run_focused_escape_or_close_overlay(Some(&handlers), &event));
            assert!(parent.peek(), "consumed Escape must leave overlay open");
            assert_eq!(calls.get(), 1);
            event.state = ElementState::Released;
            assert!(!run_focused_escape_or_close_overlay(
                Some(&handlers),
                &event
            ));
            assert_eq!(calls.get(), 1, "release must not invoke focused handler");
            assert!(parent.peek());
            event.state = ElementState::Pressed;

            let child_z = overlays::portal_sync(
                "escape-child",
                child,
                overlays::OverlayKind::Modal,
                overlays::OverlayMode::Stacked,
            )
            .unwrap();
            let mut child_panel = Element::new();
            child_panel.style.z_index = child_z.panel;
            root.children[0].children.push(child_panel);
            let handlers =
                focused_escape_handlers(&root, "escape-input", overlays::top_stacked_panel_z());
            assert!(
                handlers.is_none(),
                "lower layer focus must not consume Escape"
            );
            {
                let other = window_scope::enter(WindowScope::Secondary(83018));
                assert!(!run_focused_escape_or_close_overlay(None, &event));
                drop(other);
            }
            assert!(child.peek());
            event.state = ElementState::Released;
            assert!(!run_focused_escape_or_close_overlay(None, &event));
            assert!(child.peek());
            event.state = ElementState::Pressed;
            assert!(run_focused_escape_or_close_overlay(
                handlers.as_ref(),
                &event
            ));
            assert!(!child.peek());
            assert!(parent.peek());
            assert_eq!(calls.get(), 1);

            consume.set(false);
            let handlers =
                focused_escape_handlers(&root, "escape-input", overlays::top_stacked_panel_z())
                    .unwrap();
            assert!(run_focused_escape_or_close_overlay(Some(&handlers), &event));
            assert!(!parent.peek(), "unconsumed Escape dismisses top overlay");
            assert_eq!(calls.get(), 2);
            assert!(!run_focused_escape_or_close_overlay(
                Some(&handlers),
                &event
            ));
            assert_eq!(
                calls.get(),
                3,
                "one callback per dispatch without an overlay"
            );
            assert!(!run_focused_escape_or_close_overlay(None, &event));
        });
    }
}
