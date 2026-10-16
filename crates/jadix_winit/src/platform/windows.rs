//! Win32 extensions plus native caption hit regions (including Snap hover).
use crate::error::RequestError;
use crate::window::Window as CoreWindow;
use std::sync::Arc;
pub use winit_win32::*;

/// Publish logical caption rectangles after layout. The backend owns their
/// lifetime and checks current native flags during WM_NCHITTEST. Empty clears.
/// Safe from any thread, like other Winit setters. Windows owns caption actions;
/// do not also activate the same rectangles in client pointer handlers.
/// HTMAXBUTTON enables the Snap integration point; flyout visibility is OS policy.
pub fn set_window_control_regions(
    window: &Arc<dyn CoreWindow>,
    regions: Vec<crate::chrome::WindowControlRegion>,
) -> Result<(), RequestError> {
    let regions = regions
        .into_iter()
        .map(|r| {
            use crate::chrome::WindowControlArea as A;
            let area = match r.area {
                A::Client => CaptionArea::Client,
                A::Drag => CaptionArea::Drag,
                A::Minimize => CaptionArea::Minimize,
                A::Maximize => CaptionArea::Maximize,
                A::Close => CaptionArea::Close,
            };
            CaptionRegion {
                bounds: r.bounds,
                area,
            }
        })
        .collect();
    window.set_caption_regions(regions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ApplicationHandler;
    use crate::chrome::{WindowControlArea, WindowControlRegion};
    use crate::event::WindowEvent;
    use crate::event_loop::{ActiveEventLoop, EventLoop};
    use crate::raw_window_handle::HasWindowHandle;
    use crate::window::{WindowAttributes, WindowId};
    use windows_sys::Win32::Foundation::{HWND as WinHwnd, LPARAM, POINT};
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    #[test]
    #[ignore = "real HWND; run in a separate interactive Windows process"]
    fn native_caption_regions_follow_policy_and_window_lifetime() {
        struct Harness;
        impl ApplicationHandler for Harness {
            fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
                let window: Arc<dyn CoreWindow> = event_loop
                    .create_window(
                        WindowAttributes::default()
                            .with_decorations(false)
                            .with_surface_size(crate::dpi::LogicalSize::new(320, 240)),
                    )
                    .unwrap()
                    .into();
                let handle = window.window_handle().unwrap();
                let crate::raw_window_handle::RawWindowHandle::Win32(raw) = handle.as_raw() else {
                    panic!("Win32")
                };
                let hwnd = raw.hwnd.get() as WinHwnd;
                let hit = |x: f64, y: f64| unsafe {
                    let scale = window.scale_factor();
                    let mut point = POINT {
                        x: (x * scale) as i32,
                        y: (y * scale) as i32,
                    };
                    assert_ne!(
                        windows_sys::Win32::Graphics::Gdi::ClientToScreen(hwnd, &mut point),
                        0
                    );
                    let pos = ((point.y as u16 as u32) << 16) | (point.x as u16 as u32);
                    SendMessageW(hwnd, WM_NCHITTEST, 0, pos as LPARAM)
                };
                let regions = vec![
                    WindowControlRegion {
                        bounds: [0.0, 0.0, 320.0, 40.0],
                        area: WindowControlArea::Drag,
                    },
                    WindowControlRegion {
                        bounds: [100.0, 0.0, 40.0, 40.0],
                        area: WindowControlArea::Client,
                    },
                    WindowControlRegion {
                        bounds: [200.0, 0.0, 40.0, 40.0],
                        area: WindowControlArea::Maximize,
                    },
                    WindowControlRegion {
                        bounds: [240.0, 0.0, 40.0, 40.0],
                        area: WindowControlArea::Minimize,
                    },
                    WindowControlRegion {
                        bounds: [280.0, 0.0, 40.0, 40.0],
                        area: WindowControlArea::Close,
                    },
                ];
                set_window_control_regions(&window, regions.clone()).unwrap();
                assert_eq!(Arc::weak_count(&window), 0);
                assert_eq!(hit(20.0, 20.0), HTCAPTION as isize);
                assert_eq!(hit(110.0, 20.0), HTCLIENT as isize);
                assert_eq!(hit(210.0, 20.0), HTMAXBUTTON as isize);
                assert_eq!(hit(250.0, 20.0), HTMINBUTTON as isize);
                assert_eq!(hit(290.0, 20.0), HTCLOSE as isize);
                let buttons = window.enabled_buttons();
                window.set_enabled_buttons(crate::window::WindowButtons::empty());
                for x in [210.0, 250.0, 290.0] {
                    assert_eq!(hit(x, 20.0), HTCLIENT as isize);
                }
                window.set_enabled_buttons(buttons);
                window.set_resizable(false);
                assert_eq!(hit(210.0, 20.0), HTCLIENT as isize);
                unsafe {
                    SendMessageW(hwnd, WM_NCLBUTTONDBLCLK, HTCAPTION as usize, 0);
                    SendMessageW(hwnd, WM_SYSCOMMAND, SC_MAXIMIZE as usize, 0);
                }
                assert!(
                    !window.is_maximized(),
                    "fixed custom captions block native double-click and system maximize"
                );
                window.set_resizable(true);
                assert_eq!(hit(210.0, 20.0), HTMAXBUTTON as isize);
                unsafe {
                    SendMessageW(hwnd, WM_NCLBUTTONDBLCLK, HTCAPTION as usize, 0);
                }
                assert!(
                    window.is_maximized(),
                    "resizable custom captions keep native double-click behavior"
                );
                window.set_maximized(false);
                // Updating is atomic and never adds a second weak reference/hook.
                set_window_control_regions(&window, regions).unwrap();
                assert_eq!(Arc::weak_count(&window), 0);
                assert!(
                    set_window_control_regions(
                        &window,
                        vec![WindowControlRegion {
                            bounds: [f64::NAN; 4],
                            area: WindowControlArea::Drag
                        }]
                    )
                    .is_err()
                );
                assert_eq!(hit(20.0, 20.0), HTCAPTION as isize);
                let other = window.clone();
                assert!(
                    std::thread::spawn(move || set_window_control_regions(&other, vec![]).is_ok())
                        .join()
                        .unwrap()
                );
                set_window_control_regions(&window, vec![]).unwrap();
                assert_eq!(hit(20.0, 20.0), HTCLIENT as isize);
                assert_eq!(Arc::weak_count(&window), 0);
                set_window_control_regions(
                    &window,
                    vec![WindowControlRegion {
                        bounds: [0.0, 0.0, 300.0, 40.0],
                        area: WindowControlArea::Drag,
                    }],
                )
                .unwrap();
                let weak = Arc::downgrade(&window);
                drop(window);
                assert!(weak.upgrade().is_none());
                event_loop.exit();
            }
            fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
        }
        let mut builder = EventLoop::builder();
        builder.with_any_thread(true);
        builder.build().unwrap().run_app(Harness).unwrap();
    }
}
