use super::*;

#[test]
fn detached_and_secondary_windows_keep_their_full_content_viewport() {
    crate::core::reactivity::create_root(|| {
        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);
        DevToolsState::set_panel_width(640.0);
        super::super::device::disable();
        crate::utils::modal_motion::set_modal_viewport_size(1200.0, 900.0);
        assert_eq!(
            crate::utils::modal_motion::modal_viewport_size(),
            (560.0, 900.0)
        );
        SESSION.with(|session| *session.borrow_mut() = Some(WindowSession::new(())));
        assert_eq!(
            crate::utils::modal_motion::modal_viewport_size(),
            (1200.0, 900.0)
        );
        SESSION.with(|session| session.borrow_mut().take());
        {
            let _scope = window_scope::enter(window_scope::WindowScope::Secondary(73));
            assert_eq!(
                crate::utils::modal_motion::modal_viewport_size(),
                (1200.0, 900.0)
            );
        }
        assert_eq!(
            crate::utils::modal_motion::modal_viewport_size(),
            (560.0, 900.0)
        );
        DevToolsState::set_open(false);
    });
}

#[test]
fn resize_drag_captures_pointer_updates_width_and_releases() {
    crate::core::reactivity::create_root(|| {
        use crate::app::pointer_events::{dispatch_pointer, hit_path, PointerEvent, PointerKind};
        use winit::event::MouseButton;
        DevToolsState::set_panel_width(640.0);
        let mut handle = resize_handle();
        handle.layout_bounds = [800.0, 0.0, 6.0, 400.0];
        let mut root = Element::new().child(handle);
        root.layout_bounds = [0.0, 0.0, 1440.0, 700.0];
        let mut event = PointerEvent::new(800.0, 100.0, MouseButton::Left, 1);
        event.buttons = 1;
        dispatch_pointer(
            &hit_path(&root, event.x, event.y),
            &mut event,
            PointerKind::Down,
        );
        assert_eq!(
            crate::app::pointer_capture_id().as_deref(),
            Some("__jadix_devtools_resize")
        );
        event.x = 660.0;
        assert!(!hit_path(&root, event.x, event.y)
            .iter()
            .any(|entry| { entry.focus_id.as_deref() == Some("__jadix_devtools_resize") }));
        let path = crate::app::pointer_events::captured_path(&root).unwrap();
        let mut event = PointerEvent::new(660.0, 100.0, MouseButton::Left, 0);
        event.buttons = 1;
        dispatch_pointer(&path, &mut event, PointerKind::Move);
        assert_eq!(DevToolsState::panel_width(), 780.0);
        let mut event = PointerEvent::new(660.0, 100.0, MouseButton::Left, 0);
        dispatch_pointer(&path, &mut event, PointerKind::Up);
        assert!(crate::app::pointer_capture_id().is_none());
        assert!(crate::app::pointer_events::captured_path(&root).is_none());
        assert!(RESIZE_START.get().is_none());
    });
}
