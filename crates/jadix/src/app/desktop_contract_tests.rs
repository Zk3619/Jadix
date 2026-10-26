//! Opt-in real Windows loop/GPU regression: no screenshots or manual timing.
//! Run with --ignored --test-threads=1 on an interactive Windows desktop.

use super::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

struct CloseGuard {
    block_secondary: AtomicBool,
    closing: AtomicUsize,
    exits: AtomicUsize,
}

impl WindowLifecycleHook for CloseGuard {
    fn on_created(&self, _: &dyn Window, _: &str) {}
    fn on_event(&self, _: &dyn Window, _: &WindowEvent) {}
    fn on_closing(&self, _: &dyn Window) {
        self.closing.fetch_add(1, Ordering::SeqCst);
    }
    fn on_exit(&self) {
        self.exits.fetch_add(1, Ordering::SeqCst);
    }
    fn on_close_requested(&self, window: &dyn Window) -> bool {
        assert_eq!(current_window().unwrap().id(), window.id());
        window.title() != "Contract secondary" || !self.block_secondary.load(Ordering::SeqCst)
    }
}

// Use a real WM_COMMAND, not a call to our callback dispatcher.
fn select_first_command(window: &WindowHandle) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetMenu, GetMenuItemID, GetSubMenu, SendMessageW, WM_COMMAND,
    };
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let window = window.upgrade().unwrap();
    let RawWindowHandle::Win32(raw) = window.window_handle().unwrap().as_raw() else {
        panic!("Win32")
    };
    unsafe {
        let hwnd = raw.hwnd.get() as _;
        let menu = GetMenu(hwnd);
        assert!(!menu.is_null());
        let command = GetMenuItemID(GetSubMenu(menu, 0), 0);
        assert_ne!(command, u32::MAX);
        SendMessageW(hwnd, WM_COMMAND, command as usize, 0);
    }
}

fn press_save_accelerator(window: &WindowHandle) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyboardState, SetKeyboardState, VK_CONTROL,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{MSG, WM_KEYDOWN};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    struct RestoreKeys([u8; 256]);
    impl Drop for RestoreKeys {
        fn drop(&mut self) {
            unsafe {
                SetKeyboardState(self.0.as_ptr());
            }
        }
    }
    let window = window.upgrade().unwrap();
    let RawWindowHandle::Win32(raw) = window.window_handle().unwrap().as_raw() else {
        panic!("Win32")
    };
    unsafe {
        let mut keys = [0; 256];
        assert_ne!(GetKeyboardState(keys.as_mut_ptr()), 0);
        let _restore = RestoreKeys(keys);
        keys[VK_CONTROL as usize] = 0x80;
        assert_ne!(SetKeyboardState(keys.as_ptr()), 0);
        let msg = MSG {
            hwnd: raw.hwnd.get() as _,
            message: WM_KEYDOWN,
            wParam: b'S' as usize,
            ..std::mem::zeroed()
        };
        assert!(
            native_menu::translate_accelerator((&msg as *const MSG).cast()),
            "Ctrl+S must be consumed by the menu accelerator table"
        );
    }
}

struct Harness {
    app: App,
    guard: Arc<CloseGuard>,
    passed: Rc<Cell<bool>>,
}

fn assert_resize_disabled(window: &WindowHandle) {
    use winit::window::ResizeDirection::*;
    assert!(!window.is_resizable().unwrap());
    let size = window.upgrade().unwrap().surface_size();
    for direction in [
        North, South, East, West, NorthEast, NorthWest, SouthEast, SouthWest,
    ] {
        assert_eq!(
            window.start_resize(direction).unwrap_err(),
            "window is not resizable",
            "fixed windows must reject every resize direction before OS dispatch"
        );
    }
    assert_eq!(window.upgrade().unwrap().surface_size(), size);
}

impl ApplicationHandler for Harness {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.app.can_create_surfaces(event_loop);
        let primary = current_window().unwrap();
        assert_resize_disabled(&primary);
        assert!(!window_is_resizable());
        self.app.handle_user_event(
            event_loop,
            JadixUserEvent::OpenWindow {
                options: Box::new(
                    WindowOptions::new()
                        .title("Contract secondary")
                        .frameless()
                        .resizable(false)
                        .resize_border(5.0)
                        .min_size(220.0, 150.0)
                        .size(300.0, 200.0),
                ),
                builder: secondary_window::SecondaryUiBuilder(Box::new(|| {
                    Element::new().text("secondary")
                })),
            },
        );
        let secondary = windows()
            .into_iter()
            .find(|w| w.id() != primary.id())
            .unwrap();
        assert_resize_disabled(&secondary);
        assert_eq!(secondary.resize_direction_at(1.0, 1.0, 5.0).unwrap(), None);
        assert!(secondary.toggle_maximized().is_err());
        let region = {
            let _scope = window_scope::enter_secondary_id(secondary.id());
            crate::components::window_drag_region(Element::new())
        };
        {
            let _scope = window_scope::enter_secondary_id(secondary.id());
            let props = || crate::components::AppProps {
                title: Some("Requested title".into()),
                resizable: Some(false),
                min_width: Some(240.0),
                min_height: Some(180.0),
                ..Default::default()
            };
            let _ = crate::components::App(props());
            assert_resize_disabled(&secondary);
            assert!(!window_is_resizable());
            secondary.set_resizable(true).unwrap();
            assert!(secondary.is_resizable().unwrap());
            assert!(window_is_resizable());
            assert_eq!(
                secondary.resize_direction_at(1.0, 1.0, 5.0).unwrap(),
                Some(winit::window::ResizeDirection::NorthWest)
            );
            let scale = secondary.upgrade().unwrap().scale_factor();
            let motion = WindowEvent::PointerMoved {
                device_id: None,
                position: winit::dpi::PhysicalPosition::new(scale, scale),
                primary: true,
                source: winit::event::PointerSource::Mouse,
            };
            assert!(self
                .app
                .handle_native_resize_border(secondary.id(), &motion, event_loop));
            assert!(secondary.chrome.hovered.load(Ordering::Relaxed));
            // A resize may consume its release. The next content gesture must
            // still receive both its press and release.
            secondary.chrome.pressed.store(true, Ordering::Relaxed);
            for state in [
                winit::event::ElementState::Pressed,
                winit::event::ElementState::Released,
            ] {
                let click = WindowEvent::PointerButton {
                    device_id: None,
                    position: winit::dpi::PhysicalPosition::new(50.0 * scale, 50.0 * scale),
                    primary: true,
                    button: winit::event::ButtonSource::Mouse(winit::event::MouseButton::Left),
                    state,
                    is_macos_activation_click: false,
                };
                assert!(!self
                    .app
                    .handle_native_resize_border(secondary.id(), &click, event_loop));
                assert!(!secondary.chrome.pressed.load(Ordering::Relaxed));
            }
            assert!(self
                .app
                .handle_native_resize_border(secondary.id(), &motion, event_loop));
            secondary.set_resizable(false).unwrap();
            assert!(!secondary.chrome.hovered.load(Ordering::Relaxed));
            assert!(!self
                .app
                .handle_native_resize_border(secondary.id(), &motion, event_loop));
            secondary.set_resizable(true).unwrap();
            secondary.set_title("Native title").unwrap();
            let _ = crate::components::App(props());
            assert_eq!(secondary.upgrade().unwrap().title(), "Native title");
            assert!(
                secondary.upgrade().unwrap().is_resizable(),
                "unchanged RSX patch must preserve a later native change"
            );
            secondary.set_title("Contract secondary").unwrap();
            secondary.set_resizable(false).unwrap();
            assert_resize_disabled(&secondary);
            assert!(!window_is_resizable());
        }
        assert!(!window_is_resizable(), "scope must return to the primary");
        // A title region retains its owning window even when invoked in another scope.
        let double_press =
            pointer_events::MouseEvent::new(20.0, 20.0, winit::event::MouseButton::Left, 2);
        region.on_mousedown.as_ref().unwrap().borrow_mut()(&double_press);
        assert!(
            !secondary.is_maximized().unwrap(),
            "fixed title bar must not maximize"
        );
        secondary.set_resizable(true).unwrap();
        region.on_mousedown.as_ref().unwrap().borrow_mut()(&double_press);
        assert!(secondary.is_maximized().unwrap());
        assert!(!primary.is_maximized().unwrap());
        assert_eq!(secondary.resize_direction_at(1.0, 1.0, 5.0).unwrap(), None);
        assert!(secondary
            .start_resize(winit::window::ResizeDirection::East)
            .is_err());
        secondary.set_maximized(false).unwrap();
        secondary.set_resizable(false).unwrap();
        // A held UI-tree borrow must not disable chrome/handle access.
        {
            let _tree_borrow = self.app.state.borrow_mut();
            assert_eq!(current_window().unwrap().id(), primary.id());
        }
        let seen = Rc::new(RefCell::new(Vec::new()));
        for window in [&primary, &secondary] {
            let seen = seen.clone();
            native_menu::set_window_menu(
                window,
                vec![native_menu::NativeMenuItem::submenu(
                    "File",
                    vec![
                        native_menu::NativeMenuItem::command("scope", "Scope", move || {
                            seen.borrow_mut().push(current_window().unwrap().id());
                        })
                        .accelerator("Ctrl+S"),
                    ],
                )],
            )
            .unwrap();
        }
        window_scope::force_secondary_id(secondary.id());
        select_first_command(&primary);
        select_first_command(&secondary);
        ui_bridge::drain_ui_tasks();
        assert_eq!(*seen.borrow(), vec![primary.id(), secondary.id()]);
        assert_eq!(
            current_window().unwrap().id(),
            secondary.id(),
            "dispatch restores caller scope"
        );
        press_save_accelerator(&primary);
        ui_bridge::drain_ui_tasks();
        assert_eq!(
            *seen.borrow(),
            vec![primary.id(), secondary.id(), primary.id()]
        );

        select_first_command(&secondary); // Queue an event for the OLD menu.
        native_menu::set_window_menu(&secondary, vec![]).unwrap();
        ui_bridge::drain_ui_tasks();
        assert_eq!(
            seen.borrow().len(),
            3,
            "replaced menus discard pending commands"
        );

        // Native close and custom close must both honor the same veto; a Quit
        // must not tear down the primary before checking an unsaved secondary.
        self.app
            .window_event(event_loop, primary.id(), WindowEvent::CloseRequested);
        self.app.handle_user_event(
            event_loop,
            JadixUserEvent::CloseWindow { id: secondary.id() },
        );
        self.app.handle_user_event(event_loop, JadixUserEvent::Exit);
        assert_eq!(windows().len(), 2);
        assert!(primary.upgrade().unwrap().is_visible().unwrap_or(false));
        assert!(secondary.upgrade().unwrap().is_visible().unwrap_or(false));
        assert_eq!(self.guard.closing.load(Ordering::SeqCst), 0);
        self.guard.block_secondary.store(false, Ordering::SeqCst);
        self.app.handle_user_event(
            event_loop,
            JadixUserEvent::CloseWindow { id: secondary.id() },
        );
        assert_eq!(windows().len(), 1);
        assert!(
            !secondary.is_alive(),
            "weak handles cannot retain a closed HWND"
        );
        assert!(secondary.focus().is_err());
        assert!(secondary.is_resizable().is_err());
        assert_eq!(
            secondary
                .start_resize(winit::window::ResizeDirection::East)
                .unwrap_err(),
            "window has closed"
        );
        // A stale close must not accidentally quit the remaining primary.
        self.app.handle_user_event(
            event_loop,
            JadixUserEvent::CloseWindow { id: secondary.id() },
        );
        assert_eq!(windows().len(), 1);
        self.app.handle_user_event(event_loop, JadixUserEvent::Exit);
        self.app.shutdown_renderer();
        assert!(!primary.is_alive());
        assert_eq!(self.guard.closing.load(Ordering::SeqCst), 2);
        assert_eq!(
            self.guard.exits.load(Ordering::SeqCst),
            1,
            "shutdown hooks run exactly once"
        );
        self.passed.set(true);
    }

    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.app.window_event(event_loop, id, event);
    }
}

#[test]
#[ignore = "creates real windows/GPU resources; requires an interactive Windows desktop"]
fn desktop_native_smoke() {
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let _lock = ui_bridge::UI_QUEUE_TEST_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    crate::init();
    let mut builder = winit::event_loop::EventLoop::builder();
    builder.with_any_thread(true);
    builder.with_msg_hook(|msg| unsafe { native_menu::translate_accelerator(msg) });
    let event_loop = builder.build().unwrap();
    let (_tx, rx) = std::sync::mpsc::channel();
    let app = lifecycle::create_app(
        WindowOptions::new()
            .title("Contract primary")
            .resizable(false)
            .size(320.0, 240.0),
        || {
            let window = current_window().unwrap().upgrade().unwrap();
            Element::new().text("primary").on_unmount(move || {
                assert!(!window.is_visible().unwrap_or(false), "hide before native/GPU teardown");
            })
        },
        event_loop.create_proxy(),
        rx,
    );
    let guard = Arc::new(CloseGuard {
        block_secondary: AtomicBool::new(true),
        closing: AtomicUsize::new(0),
        exits: AtomicUsize::new(0),
    });
    set_window_lifecycle_hook(Some(guard.clone()));
    let passed = Rc::new(Cell::new(false));
    event_loop
        .run_app(Harness {
            app,
            guard,
            passed: passed.clone(),
        })
        .unwrap();
    set_window_lifecycle_hook(None);
    assert!(passed.get());
}

#[test]
#[ignore = "real Windows proxy; run separately from other EventLoop tests"]
fn desktop_proxy_wake_batches() {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    const TOTAL: usize = 1025;
    struct WakeHarness {
        app: App,
        seen: Arc<Mutex<Vec<usize>>>,
        done: Arc<AtomicBool>,
        turns: usize,
        deadline: Instant,
    }
    impl ApplicationHandler for WakeHarness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            // No GPU/window needed: exercise the real OS proxy with the actual App drain.
            event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(self.deadline));
        }
        fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
            let before = self.seen.lock().unwrap().len();
            self.turns += 1;
            self.app.proxy_wake_up(event_loop);
            let after = self.seen.lock().unwrap().len();
            assert!(after - before <= ui_bridge::UI_TASK_BATCH);
            if self.done.load(Ordering::SeqCst) {
                assert_eq!(*self.seen.lock().unwrap(), (0..TOTAL).collect::<Vec<_>>());
                assert!(self.turns >= TOTAL.div_ceil(ui_bridge::UI_TASK_BATCH));
                assert!(
                    self.turns < TOTAL / 2,
                    "one wake per task was not coalesced"
                );
                event_loop.exit();
            }
        }
        fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
            assert!(
                Instant::now() < self.deadline,
                "lost wake: UI tasks never completed"
            );
            event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(self.deadline));
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let _lock = ui_bridge::UI_QUEUE_TEST_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let done = Arc::new(AtomicBool::new(false));
    // Before installation: startup must explicitly wake this queue.
    let prestart = seen.clone();
    ui_bridge::run_on_ui_thread(move || prestart.lock().unwrap().push(0));
    let mut builder = winit::event_loop::EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    ui_bridge::install_proxy(event_loop.create_proxy(), tx);
    let producer_seen = seen.clone();
    let producer_done = done.clone();
    std::thread::spawn(move || {
        for i in 1..TOTAL {
            let seen = producer_seen.clone();
            ui_bridge::run_on_ui_thread(move || seen.lock().unwrap().push(i));
        }
        ui_bridge::run_on_ui_thread(move || {
            // Nested work must get its own future turn, without losing its wake.
            ui_bridge::run_on_ui_thread(move || producer_done.store(true, Ordering::SeqCst));
        });
    })
    .join()
    .unwrap();
    let app = lifecycle::create_app(
        WindowOptions::default(),
        Element::new,
        event_loop.create_proxy(),
        rx,
    );
    event_loop
        .run_app(WakeHarness {
            app,
            seen,
            done: done.clone(),
            turns: 0,
            deadline: Instant::now() + Duration::from_secs(10),
        })
        .unwrap();
    assert!(done.load(Ordering::SeqCst));
}
