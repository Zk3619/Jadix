use super::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, KeyCode, KeyLocation, NamedKey, PhysicalKey};
use winit::window::{Window, WindowId};

struct Guard {
    veto: AtomicBool,
    exits: AtomicUsize,
    closing: AtomicUsize,
}

impl WindowLifecycleHook for Guard {
    fn on_created(&self, _: &dyn Window, _: &str) {}
    fn on_event(&self, _: &dyn Window, _: &WindowEvent) {}
    fn on_close_requested(&self, _: &dyn Window) -> bool {
        !self.veto.load(Ordering::Relaxed)
    }
    fn on_closing(&self, _: &dyn Window) {
        self.closing.fetch_add(1, Ordering::Relaxed);
    }
    fn on_exit(&self) {
        self.exits.fetch_add(1, Ordering::Relaxed);
    }
}

fn key(named: NamedKey, repeat: bool) -> KeyEvent {
    KeyEvent {
        physical_key: PhysicalKey::Code(KeyCode::F11),
        logical_key: Key::Named(named),
        text: None,
        location: KeyLocation::Standard,
        state: ElementState::Pressed,
        repeat,
        text_with_all_modifiers: None,
        key_without_modifiers: Key::Named(named),
    }
}

struct Harness {
    app: App,
    guard: Arc<Guard>,
    passed: Rc<Cell<bool>>,
}

impl Harness {
    fn open(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        title: &str,
        independent: bool,
    ) -> WindowHandle {
        self.app.handle_user_event(
            event_loop,
            JadixUserEvent::OpenWindow {
                options: Box::new(
                    WindowOptions::new()
                        .title(title)
                        .size(240.0, 160.0)
                        .close_with_primary(!independent),
                ),
                builder: secondary_window::SecondaryUiBuilder(Box::new(|| {
                    Element::new()
                        .w("100%")
                        .h("100%")
                        .bg("#ffffff")
                        .text("独立窗口")
                })),
            },
        );
        windows()
            .into_iter()
            .find(|window| window.upgrade().unwrap().title() == title)
            .unwrap()
    }
}

impl ApplicationHandler for Harness {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.app.can_create_surfaces(event_loop);
        let primary = current_window().unwrap();
        let coupled = self.open(event_loop, "coupled", false);
        let independent = self.open(event_loop, "independent", true);
        assert!(!independent.closes_with_primary().unwrap());
        independent.set_close_with_primary(true).unwrap();
        assert!(independent.closes_with_primary().unwrap());
        independent.set_close_with_primary(false).unwrap();
        independent.set_always_on_top(true).unwrap();
        independent.set_always_on_top(false).unwrap();
        independent.set_fullscreen_shortcuts(false).unwrap();
        assert!(!self
            .app
            .handle_fullscreen_shortcut(independent.id(), &key(NamedKey::F11, false)));
        independent.set_fullscreen_shortcuts(true).unwrap();
        assert!(self
            .app
            .handle_fullscreen_shortcut(independent.id(), &key(NamedKey::F11, false)));
        assert!(independent.is_fullscreen().unwrap());
        assert!(!primary.is_fullscreen().unwrap());
        assert!(self
            .app
            .handle_fullscreen_shortcut(independent.id(), &key(NamedKey::F11, true)));
        assert!(independent.is_fullscreen().unwrap());
        self.app
            .handle_fullscreen_shortcut(independent.id(), &key(NamedKey::Escape, false));
        assert!(!independent.is_fullscreen().unwrap());
        let fonts = crate::text::shared_font_system_ptr().unwrap();
        self.guard.veto.store(true, Ordering::Relaxed);
        self.app
            .window_event(event_loop, primary.id(), WindowEvent::CloseRequested);
        assert_eq!(windows().len(), 3);
        self.guard.veto.store(false, Ordering::Relaxed);
        self.app
            .window_event(event_loop, primary.id(), WindowEvent::CloseRequested);
        assert_eq!(windows().len(), 1);
        assert!(!primary.is_alive());
        assert!(!coupled.is_alive());
        assert!(independent.is_alive());
        assert!(current_window().is_none());
        assert!(!event_loop.exiting());
        assert!(self.app.state.borrow().is_none());
        assert_eq!(crate::text::shared_font_system_ptr().unwrap(), fonts);
        self.app.can_create_surfaces(event_loop);
        assert_eq!(windows().len(), 1, "primary must not respawn");
        let seen = Rc::new(RefCell::new(None));
        let received = seen.clone();
        set_second_instance_handler(move |launch| {
            *received.borrow_mut() = Some((current_window().unwrap().id(), launch))
        });
        let launch = InstanceLaunch {
            args: vec!["document.txt".into()],
            working_directory: std::env::current_dir().unwrap(),
        };
        self.app
            .handle_user_event(event_loop, JadixUserEvent::SecondInstance(launch.clone()));
        assert_eq!(*seen.borrow(), Some((independent.id(), launch)));
        let later = self.open(event_loop, "after primary", true);
        for window in [&independent, &later] {
            self.app
                .window_event(event_loop, window.id(), WindowEvent::RedrawRequested);
        }
        self.app
            .handle_user_event(event_loop, JadixUserEvent::CloseWindow { id: primary.id() });
        assert_eq!(windows().len(), 2, "stale close is harmless");
        self.guard.veto.store(true, Ordering::Relaxed);
        self.app.handle_user_event(event_loop, JadixUserEvent::Exit);
        assert_eq!(
            windows().len(),
            2,
            "Quit can be vetoed after the primary closes"
        );
        self.guard.veto.store(false, Ordering::Relaxed);
        self.app
            .window_event(event_loop, later.id(), WindowEvent::Destroyed);
        assert_eq!(windows().len(), 1);
        self.app
            .window_event(event_loop, independent.id(), WindowEvent::CloseRequested);
        assert!(event_loop.exiting());
        assert!(!independent.is_alive());
        assert!(!later.is_alive());
        assert!(windows().is_empty());
        assert!(crate::text::shared_font_system_ptr().is_none());
        assert_eq!(self.guard.closing.load(Ordering::Relaxed), 4);
        assert_eq!(self.guard.exits.load(Ordering::Relaxed), 1);
        self.passed.set(true);
    }

    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.app.window_event(event_loop, id, event);
    }
}

#[test]
#[ignore = "real Windows GPU/window test; run separately from other event-loop tests"]
fn independent_windows_survive_primary_and_last_close_exits() {
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let _lock = ui_bridge::UI_QUEUE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    crate::init();
    let mut builder = winit::event_loop::EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let (_sender, receiver) = std::sync::mpsc::channel();
    let app = lifecycle::create_app(
        WindowOptions::new().title("primary").size(320.0, 200.0),
        || {
            Element::new()
                .w("100%")
                .h("100%")
                .bg("#ffffff")
                .text("Primary")
        },
        event_loop.create_proxy(),
        receiver,
    );
    let guard = Arc::new(Guard {
        veto: AtomicBool::new(false),
        exits: AtomicUsize::new(0),
        closing: AtomicUsize::new(0),
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
