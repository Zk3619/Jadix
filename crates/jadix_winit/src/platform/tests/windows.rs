use super::*;
use crate::application::ApplicationHandler;
use crate::chrome::{WindowControlArea, WindowControlRegion};
use crate::event::WindowEvent;
use crate::event_loop::{ActiveEventLoop, EventLoop};
use crate::raw_window_handle::HasWindowHandle;
use crate::window::{WindowAttributes, WindowId};
use windows_sys::Win32::Foundation::{HWND as WinHwnd, LPARAM, POINT};
use windows_sys::Win32::UI::Input::Ime::{
    ImmGetContext, ImmGetOpenStatus, ImmReleaseContext, ImmSetOpenStatus,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

#[test]
#[ignore = "real HWND; exercises the installed Windows IME with system keyboard input"]
fn native_real_ime_composition_reaches_preedit_and_commit_events() {
    use crate::event::Ime;
    use std::time::{Duration, Instant};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        ActivateKeyboardLayout, GetKeyboardLayout, LoadKeyboardLayoutW, UnloadKeyboardLayout,
        SetActiveWindow, SetFocus, HKL, KLF_ACTIVATE, VK_SPACE, VK_RETURN,
    };
    use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow, ShowWindow, SW_SHOW,
    };

    struct ImeRestore {
        hwnd: WinHwnd,
        chinese_layout: HKL,
        previous_layout: HKL,
        was_open: bool,
    }
    impl Drop for ImeRestore {
        fn drop(&mut self) {
            unsafe {
                let ime = ImmGetContext(self.hwnd);
                if !ime.is_null() {
                    ImmSetOpenStatus(ime, i32::from(self.was_open));
                    ImmReleaseContext(self.hwnd, ime);
                }
                ActivateKeyboardLayout(self.previous_layout, KLF_ACTIVATE);
                UnloadKeyboardLayout(self.chinese_layout);
            }
        }
    }

    struct Harness {
        events: std::sync::Arc<std::sync::Mutex<Vec<Ime>>>,
        _restore: Option<ImeRestore>,
        _window: Option<Box<dyn crate::window::Window>>,
        started: Instant,
    }
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            let window = event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_surface_size(crate::dpi::LogicalSize::new(420, 240)),
                )
                .unwrap();
            let handle = window.window_handle().unwrap();
            let crate::raw_window_handle::RawWindowHandle::Win32(raw) = handle.as_raw() else {
                panic!("Win32 window expected")
            };
            let hwnd = raw.hwnd.get() as WinHwnd;

            let layout_name = [
                b'0' as u16,
                b'0' as u16,
                b'0' as u16,
                b'0' as u16,
                b'0' as u16,
                b'8' as u16,
                b'0' as u16,
                b'4' as u16,
                0,
            ];
            let previous_layout = unsafe { GetKeyboardLayout(0) };
            let chinese_layout = unsafe { LoadKeyboardLayoutW(layout_name.as_ptr(), KLF_ACTIVATE) };
            assert!(!chinese_layout.is_null(), "Microsoft Chinese IME layout is required");
            unsafe { ActivateKeyboardLayout(chinese_layout, KLF_ACTIVATE) };

            #[allow(deprecated)]
            window.set_ime_allowed(true);
            let ime = unsafe { ImmGetContext(hwnd) };
            assert!(!ime.is_null(), "the window must have a real IME context");
            let was_open = unsafe { ImmGetOpenStatus(ime) } != 0;
            assert_ne!(unsafe { ImmSetOpenStatus(ime, 1) }, 0, "open the installed IME");
            unsafe { ImmReleaseContext(hwnd, ime) };
            self._restore = Some(ImeRestore {
                hwnd,
                chinese_layout,
                previous_layout,
                was_open,
            });
            unsafe { ShowWindow(hwnd, SW_SHOW) };
            unsafe {
                let foreground = GetForegroundWindow();
                let foreground_thread = GetWindowThreadProcessId(foreground, std::ptr::null_mut());
                let current_thread = GetCurrentThreadId();
                let attached = AttachThreadInput(current_thread, foreground_thread, 1) != 0;
                SetForegroundWindow(hwnd);
                SetActiveWindow(hwnd);
                SetFocus(hwnd);
                if attached {
                    AttachThreadInput(current_thread, foreground_thread, 0);
                }
            }
            assert_eq!(unsafe { GetForegroundWindow() }, hwnd);

            let keys = [b'N' as u16, b'I' as u16, b'H' as u16, b'A' as u16, b'O' as u16];
            let mut input = Vec::with_capacity(keys.len() * 2 + 4);
            for key in keys.into_iter().chain([VK_SPACE, VK_RETURN]) {
                input.push(INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: key,
                            wScan: 0,
                            dwFlags: 0,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                });
                input.push(INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: key,
                            wScan: 0,
                            dwFlags: KEYEVENTF_KEYUP,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                });
            }
            assert_eq!(
                unsafe { SendInput(input.len() as u32, input.as_ptr(), std::mem::size_of::<INPUT>() as i32) },
                input.len() as u32,
                "send real keyboard input through the active Chinese IME"
            );
            self._window = Some(window);
        }

        fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, _: WindowId, event: WindowEvent) {
            if let WindowEvent::Ime(ime) = event {
                let committed = matches!(&ime, Ime::Commit(text) if !text.is_empty());
                self.events.lock().unwrap().push(ime);
                if committed {
                    event_loop.exit();
                }
            }
        }

        fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
            if self.started.elapsed() >= Duration::from_secs(12) {
                event_loop.exit();
            } else {
                event_loop.set_control_flow(crate::event_loop::ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_millis(25),
                ));
            }
        }
    }

    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let harness = Harness {
        events: events.clone(),
        _restore: None,
        _window: None,
        started: Instant::now(),
    };
    builder.build().unwrap().run_app(harness).unwrap();
    let events = events.lock().unwrap();
    assert!(
        events.iter().any(|event| matches!(event, Ime::Preedit(text, _) if !text.is_empty())),
        "real IME must emit non-empty preedit, got {:?}",
        *events
    );
    assert!(
        events.iter().any(|event| matches!(event, Ime::Commit(text) if !text.is_empty())),
        "real IME must commit a candidate, got {:?}",
        *events
    );
}

#[test]
#[ignore = "real HWND; moves an owned foreign-procedure window across a foreground switch"]
fn native_owned_foreign_window_never_reads_winit_window_data() {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::AttachThreadInput;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, GetForegroundWindow, GetWindow,
        GetWindowThreadProcessId, GW_OWNER, GWLP_USERDATA, SetForegroundWindow,
        SetWindowLongPtrW, SetWindowPos, ShowWindow, SWP_NOACTIVATE, SWP_NOSIZE,
        SWP_NOZORDER, SW_SHOW, WS_POPUP,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SetActiveWindow, SetFocus};

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    struct Harness;
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            let window = event_loop.create_window(WindowAttributes::default()).unwrap();
            let handle = window.window_handle().unwrap();
            let crate::raw_window_handle::RawWindowHandle::Win32(raw) = handle.as_raw() else {
                panic!("Win32 window expected")
            };
            let hwnd = raw.hwnd.get() as WinHwnd;
            unsafe { SetForegroundWindow(hwnd) };
            let foreign = unsafe {
                CreateWindowExW(
                    0,
                    wide("STATIC").as_ptr(),
                    wide("foreign helper").as_ptr(),
                    WS_POPUP,
                    10,
                    10,
                    80,
                    40,
                    hwnd,
                    std::ptr::null_mut(),
                    GetModuleHandleW(std::ptr::null()),
                    std::ptr::null(),
                )
            };
            assert!(!foreign.is_null());
            assert_eq!(unsafe { GetWindow(foreign, GW_OWNER) }, hwnd);
            unsafe { SetWindowLongPtrW(foreign, GWLP_USERDATA, 1) };
            unsafe {
                ShowWindow(foreign, SW_SHOW);
                let foreground = GetForegroundWindow();
                let foreground_thread = GetWindowThreadProcessId(foreground, std::ptr::null_mut());
                let current_thread = GetWindowThreadProcessId(hwnd, std::ptr::null_mut());
                let attached = AttachThreadInput(current_thread, foreground_thread, 1) != 0;
                SetForegroundWindow(foreign);
                SetActiveWindow(foreign);
                SetFocus(foreign);
                if attached {
                    AttachThreadInput(current_thread, foreground_thread, 0);
                }
            }
            assert_eq!(unsafe { GetForegroundWindow() }, foreign, "switch foreground to foreign HWND");
            assert_ne!(
                unsafe {
                    SetWindowPos(
                        hwnd,
                        std::ptr::null_mut(),
                        24,
                        24,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                    )
                },
                0,
                "move parent while the foreign owned HWND is foreground (last error {})",
                unsafe { GetLastError() }
            );
            assert_eq!(unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(foreign, GWLP_USERDATA) }, 1);
            unsafe {
                SetForegroundWindow(hwnd);
                DestroyWindow(foreign);
            }
            event_loop.exit();
        }

        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }

    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness).unwrap();
}

#[test]
#[ignore = "real HWND; run in a separate interactive Windows process"]
fn native_hidden_maximize_waits_for_explicit_show() {
    use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};
    static ORIGINAL: AtomicIsize = AtomicIsize::new(0);
    static SHOWS: AtomicUsize = AtomicUsize::new(0);
    static NORMAL_SHOWS: AtomicUsize = AtomicUsize::new(0);
    unsafe extern "system" fn record_show(
        hwnd: WinHwnd,
        message: u32,
        wparam: usize,
        lparam: isize,
    ) -> isize {
        if unsafe { IsWindowVisible(hwnd) } != 0 {
            SHOWS.fetch_add(1, Ordering::Relaxed);
            if message == WM_WINDOWPOSCHANGED && unsafe { IsZoomed(hwnd) } == 0 {
                NORMAL_SHOWS.fetch_add(1, Ordering::Relaxed);
            }
        }
        unsafe {
            CallWindowProcW(
                std::mem::transmute::<isize, WNDPROC>(ORIGINAL.load(Ordering::Relaxed)),
                hwnd,
                message,
                wparam,
                lparam,
            )
        }
    }
    struct Harness;
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            let window = event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_visible(false)
                        .with_surface_size(crate::dpi::LogicalSize::new(320, 240)),
                )
                .unwrap();
            let handle = window.window_handle().unwrap();
            let crate::raw_window_handle::RawWindowHandle::Win32(raw) = handle.as_raw() else {
                panic!("Win32")
            };
            let hwnd = raw.hwnd.get() as WinHwnd;
            ORIGINAL.store(
                unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, record_show as *const () as isize) },
                Ordering::Relaxed,
            );
            assert_ne!(ORIGINAL.load(Ordering::Relaxed), 0);
            SHOWS.store(0, Ordering::Relaxed);
            NORMAL_SHOWS.store(0, Ordering::Relaxed);
            assert_eq!(window.is_visible(), Some(false));
            window.set_maximized(true);
            assert_eq!(
                window.is_visible(),
                Some(false),
                "restoring maximize must not show an unpainted window"
            );
            window.set_decorations(false);
            assert_eq!(window.is_visible(), Some(false));
            assert_eq!(
                SHOWS.load(Ordering::Relaxed),
                0,
                "maximize must not briefly show then hide the startup window"
            );
            window.set_visible(true);
            assert_eq!(
                NORMAL_SHOWS.load(Ordering::Relaxed),
                0,
                "restoring maximize must never expose the normal-size window first"
            );
            assert_eq!(window.is_visible(), Some(true));
            assert!(
                SHOWS.load(Ordering::Relaxed) > 0,
                "native observer must see explicit show"
            );
            assert!(window.is_maximized());
            let mut bounds = unsafe { std::mem::zeroed() };
            unsafe {
                GetWindowRect(hwnd, &mut bounds);
            }
            let monitor = unsafe {
                windows_sys::Win32::Graphics::Gdi::MonitorFromWindow(
                    hwnd,
                    windows_sys::Win32::Graphics::Gdi::MONITOR_DEFAULTTONEAREST,
                )
            };
            let mut info: windows_sys::Win32::Graphics::Gdi::MONITORINFO =
                unsafe { std::mem::zeroed() };
            info.cbSize = std::mem::size_of_val(&info) as u32;
            unsafe {
                windows_sys::Win32::Graphics::Gdi::GetMonitorInfoW(monitor, &mut info);
            }
            assert!(
                bounds.right - bounds.left >= info.rcWork.right - info.rcWork.left,
                "maximized style must also restore maximized geometry"
            );
            assert!(bounds.bottom - bounds.top >= info.rcWork.bottom - info.rcWork.top);
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_WNDPROC, ORIGINAL.load(Ordering::Relaxed));
            }
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness).unwrap();
}

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
