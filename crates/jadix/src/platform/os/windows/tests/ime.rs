use super::*;

#[test]
#[ignore = "creates a native Windows window; run separately from other EventLoop tests"]
fn first_character_query_uses_idle_field_position() {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::ActiveEventLoop;
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::{WindowAttributes, WindowId};

    struct Harness;
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            let window = event_loop
                .create_window(WindowAttributes::default().with_visible(false))
                .unwrap();
            arm_ime_composition_watch(window.as_ref());
            let hwnd = hwnd_of(window.as_ref()).unwrap();
            // Query before any Preedit / composing paint: first focus, then a field switch.
            for (x, y, scroll) in [(80, 120, 0.0), (240, 320, 0.0), (240, 280, 40.0)] {
                apply_ime_cursor_area(
                    window.as_ref(),
                    x as f32,
                    y as f32,
                    2.0,
                    24.0,
                    1.0,
                    [0.0, scroll],
                    false,
                    true,
                );
                let mut pos: IMECHARPOSITION = unsafe { std::mem::zeroed() };
                pos.dwSize = std::mem::size_of::<IMECHARPOSITION>() as u32;
                let mut expected = POINT { x, y };
                unsafe {
                    ClientToScreen(hwnd, &mut expected);
                    assert_eq!(
                        SendMessageW(
                            hwnd,
                            WM_IME_REQUEST,
                            IMR_QUERYCHARPOSITION as WPARAM,
                            &mut pos as *mut IMECHARPOSITION as LPARAM,
                        ),
                        1
                    );
                }
                assert_eq!((pos.pt.x, pos.pt.y), (expected.x, expected.y));
                assert_eq!(pos.cLineHeight, 24);
            }
            end_composition_ui();
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = winit::event_loop::EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness).unwrap();
}

#[test]
fn lone_shift_reaches_the_ime() {
    assert!(shift_reaches_ime(WM_KEYDOWN, VK_SHIFT as WPARAM));
    assert!(shift_reaches_ime(WM_KEYUP, VK_LSHIFT as WPARAM));
    assert!(shift_reaches_ime(WM_SYSKEYDOWN, VK_RSHIFT as WPARAM));
    assert!(shift_reaches_ime(WM_SYSKEYUP, VK_SHIFT as WPARAM));
    // Left arrow, and Shift on a character message, stay with the editor.
    assert!(!shift_reaches_ime(WM_KEYDOWN, 0x25));
    assert!(!shift_reaches_ime(0x0102, VK_SHIFT as WPARAM));
}

#[test]
fn only_lone_shift_release_finishes_composition() {
    let mut gesture = ShiftGesture::default();
    assert!(!gesture.update(WM_KEYDOWN, VK_SHIFT as WPARAM));
    assert!(!gesture.update(WM_KEYDOWN, 0x54));
    assert!(!gesture.update(WM_KEYUP, 0x54));
    assert!(!gesture.update(WM_KEYUP, VK_SHIFT as WPARAM));
    assert!(!gesture.update(WM_KEYDOWN, VK_SHIFT as WPARAM));
    assert!(gesture.update(WM_KEYUP, VK_SHIFT as WPARAM));
    assert!(!gesture.update(WM_KEYUP, VK_SHIFT as WPARAM));
}
