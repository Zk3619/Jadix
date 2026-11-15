use jadix_winit::application::ApplicationHandler;
use jadix_winit::event::WindowEvent;
use jadix_winit::event_loop::{ActiveEventLoop, EventLoop};
use jadix_winit::window::{InputRegionRect, PointerPassthrough, WindowAttributes, WindowId};

#[cfg(windows)]
fn probe_region(window: &dyn jadix_winit::window::Window) {
    use jadix_winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Foundation::{POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        ClientToScreen, CreateRectRgn, DeleteObject, GetWindowRgn, NULLREGION, PtInRegion,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;
    let RawWindowHandle::Win32(handle) = window.window_handle().unwrap().as_raw() else {
        unreachable!()
    };
    let hwnd = handle.hwnd.get() as windows_sys::Win32::Foundation::HWND;
    let mut origin = POINT::default();
    let mut bounds = RECT::default();
    assert_ne!(unsafe { ClientToScreen(hwnd, &mut origin) }, 0);
    assert_ne!(unsafe { GetWindowRect(hwnd, &mut bounds) }, 0);
    let offset = (origin.x - bounds.left, origin.y - bounds.top);
    let query = unsafe { CreateRectRgn(0, 0, 0, 0) };
    assert!(!query.is_null());
    window
        .set_window_shape(Some(&[
            InputRegionRect::new(0, 0, 30, 30),
            InputRegionRect::new(50, 50, 20, 20),
        ]))
        .unwrap();
    assert_ne!(unsafe { GetWindowRgn(hwnd, query) }, 0);
    assert_ne!(
        unsafe { PtInRegion(query, offset.0 + 10, offset.1 + 10) },
        0
    );
    assert_ne!(
        unsafe { PtInRegion(query, offset.0 + 55, offset.1 + 55) },
        0
    );
    assert_eq!(
        unsafe { PtInRegion(query, offset.0 + 40, offset.1 + 40) },
        0
    );
    assert!(
        window
            .set_window_shape(Some(&[InputRegionRect::new(i32::MAX, 0, 10, 10)]))
            .is_err()
    );
    assert!(
        window
            .set_window_shape(Some(&[InputRegionRect::new(1 << 26, 0, 10, 10)]))
            .is_err()
    );
    assert_ne!(unsafe { GetWindowRgn(hwnd, query) }, 0);
    assert_ne!(
        unsafe { PtInRegion(query, offset.0 + 10, offset.1 + 10) },
        0
    );
    window.set_window_shape(Some(&[])).unwrap();
    assert_eq!(unsafe { GetWindowRgn(hwnd, query) }, NULLREGION);
    window.set_window_shape(None).unwrap();
    assert_eq!(unsafe { GetWindowRgn(hwnd, query) }, 0);
    // The legacy input-region entry point keeps its documented Windows side effect.
    window.set_input_region(Some(&[])).unwrap();
    assert_eq!(unsafe { GetWindowRgn(hwnd, query) }, NULLREGION);
    window.set_input_region(None).unwrap();
    unsafe {
        DeleteObject(query);
    }
    println!("Win32 region union, empty, restore and atomic rejection checks passed");
}

struct Probe;

impl ApplicationHandler for Probe {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        let window = event_loop
            .create_window(WindowAttributes::default().with_visible(false))
            .unwrap();
        let monitors: Vec<_> = window.available_monitors().collect();
        println!("monitors: {}", monitors.len());
        for monitor in monitors {
            println!("monitor: {monitor:?}");
        }
        println!("global cursor: {:?}", window.global_cursor_position());
        assert!(window.set_opacity(f32::NAN).is_err());
        assert!(window.set_opacity(-0.1).is_err());
        assert!(window.set_opacity(1.1).is_err());
        assert!(
            window
                .set_pointer_passthrough(PointerPassthrough::LEFT | PointerPassthrough::WHEEL)
                .is_err()
        );
        #[cfg(windows)]
        {
            use jadix_winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                GWL_EXSTYLE, GetLayeredWindowAttributes, GetWindowLongPtrW, LWA_ALPHA,
                WS_EX_LAYERED, WS_EX_TRANSPARENT,
            };
            let RawWindowHandle::Win32(handle) = window.window_handle().unwrap().as_raw() else {
                unreachable!()
            };
            let hwnd = handle.hwnd.get() as windows_sys::Win32::Foundation::HWND;
            window.set_opacity(0.5).unwrap();
            window
                .set_pointer_passthrough(PointerPassthrough::ALL)
                .unwrap();
            let mut alpha = 0;
            let mut flags = 0;
            assert_ne!(
                unsafe {
                    GetLayeredWindowAttributes(hwnd, std::ptr::null_mut(), &mut alpha, &mut flags)
                },
                0
            );
            assert_eq!(alpha, 128);
            assert_ne!(flags & LWA_ALPHA, 0);
            window
                .set_pointer_passthrough(PointerPassthrough::empty())
                .unwrap();
            let style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
            assert_ne!(style & WS_EX_LAYERED, 0);
            assert_eq!(style & WS_EX_TRANSPARENT, 0);
            window.set_opacity(1.0).unwrap();
            assert_eq!(
                unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32 & WS_EX_LAYERED,
                0
            );
            probe_region(window.as_ref());
            window.global_cursor_position().unwrap();
            println!("Win32 opacity, pass-through state and global cursor checks passed");
            let composition = event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_visible(false)
                        .with_platform_attributes(Box::new(
                            jadix_winit::platform::windows::WindowAttributesWindows::default()
                                .with_no_redirection_bitmap(true),
                        )),
                )
                .unwrap();
            probe_region(composition.as_ref());
            assert!(composition.set_opacity(0.5).is_err());
            assert!(
                composition
                    .set_pointer_passthrough(PointerPassthrough::ALL)
                    .is_err()
            );
            let RawWindowHandle::Win32(handle) = composition.window_handle().unwrap().as_raw()
            else {
                unreachable!()
            };
            let hwnd = handle.hwnd.get() as windows_sys::Win32::Foundation::HWND;
            assert_eq!(
                unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32 & WS_EX_LAYERED,
                0
            );
            println!("No-redirection window styles remain intact after unsupported requests");
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    EventLoop::new()?.run_app(Probe)?;
    Ok(())
}
