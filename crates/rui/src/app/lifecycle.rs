use std::sync::Arc;

use accesskit::{Node, NodeId, Role, Tree, TreeUpdate};
use accesskit_winit::Adapter;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::ModifiersState,
    window::{Window, WindowId},
};

use crate::core::tree::Element;

impl super::App {
    /// Drop GPU resources while the window / event loop are still valid.
    ///
    /// The root `create_effect` captures `Rc<RefCell<Option<AppState>>>`. If we leave
    /// teardown to thread-local `Runtime` destructors after `main` returns, winit has
    /// already torn down and wgpu-hal's GLES/WGL path panics inside `make_current`
    /// (`unwrap` on OS error 2004).
    fn shutdown_renderer(&mut self) {
        crate::app::runtime::clear_app_runtime();
        self.accesskit_adapter = None;
        // Take AppState out first so wgpu Surface/Device drop while the HWND is still
        // alive (field drop order keeps `window` until after `surface`).
        let _ = self.state.borrow_mut().take();
        // Drop effect closures that captured the state `Rc` (root UI rebuild effect).
        crate::core::reactivity::clear_effects();
    }
}

struct AccessKitBootstrap;

impl accesskit::ActivationHandler for AccessKitBootstrap {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        const RID: NodeId = NodeId(1);
        Some(TreeUpdate {
            nodes: vec![(RID, Node::new(Role::Window))],
            tree: Some(Tree::new(RID)),
            focus: RID,
        })
    }
}

impl ApplicationHandler<super::RuiUserEvent> for super::App {
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: super::RuiUserEvent) {
        match event {
            #[cfg(debug_assertions)]
            super::RuiUserEvent::HotReloadWake => {
                if let Some(state) = (*self.state.borrow_mut()).as_mut() {
                    state.window.request_redraw();
                    self.force_redraw_count = self.force_redraw_count.max(2);
                }
            }
            super::RuiUserEvent::AccessKit(ev) => {
                use accesskit_winit::WindowEvent as AkWindowEvent;
                match ev.window_event {
                    AkWindowEvent::ActionRequested(req) => {
                        if let Some(state) = (*self.state.borrow_mut()).as_mut() {
                            if crate::accesskit_bridge::dispatch_accesskit_action(
                                &state.root_element,
                                &req,
                            ) {
                                state.window.request_redraw();
                            }
                        }
                    }
                    AkWindowEvent::InitialTreeRequested => {
                        if let Some(adapter) = &mut self.accesskit_adapter {
                            if let Some(state) = (*self.state.borrow()).as_ref() {
                                let update = state.accesskit_tree_update();
                                adapter.update_if_active(|| update);
                            }
                        }
                    }
                    _ => {
                        if let Some(state) = (*self.state.borrow()).as_ref() {
                            state.window.request_redraw();
                        }
                    }
                }
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        #[cfg(debug_assertions)]
        if self.hot_reload.has_pending() {
            if let Some(state) = (*self.state.borrow()).as_ref() {
                state.window.request_redraw();
            }
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.borrow().is_none() {
            let mut attrs = Window::default_attributes()
                .with_title(
                    std::env::var("RUI_WINDOW_TITLE").unwrap_or_else(|_| "RUI Engine".into()),
                )
                .with_visible(false);
            if let (Ok(w), Ok(h)) = (
                std::env::var("RUI_WINDOW_WIDTH"),
                std::env::var("RUI_WINDOW_HEIGHT"),
            ) {
                if let (Ok(w), Ok(h)) = (w.parse::<f64>(), h.parse::<f64>()) {
                    if w > 0.0 && h > 0.0 {
                        attrs = attrs.with_inner_size(winit::dpi::LogicalSize::new(w, h));
                    }
                }
            }
            let window = Arc::new(event_loop.create_window(attrs).unwrap());

            // Initialize GPU/runtime state with a private bootstrap root. The user app tree must be
            // built exactly once, inside the tracked root effect below; otherwise startup creates
            // a throwaway tree whose Drop/on_unmount side effects can invalidate resources still
            // referenced by the tracked tree.
            let state = pollster::block_on(super::state::AppState::new_bootstrap(window.clone()));
            *self.state.borrow_mut() = Some(state);

            let state_ref = self.state.clone();
            crate::app::runtime::set_app_runtime(crate::app::runtime::AppRuntime::new(
                std::rc::Rc::downgrade(&state_ref),
            ));
            crate::app::reactive::reset_registered_anchors();

            // create_effect runs synchronously on registration. By the time it returns, the real
            // root has been reconciled and deferred fine-grained reactive anchors have mounted, so
            // making the window visible cannot expose the bootstrap root.
            let builder_ref = self.app_builder.clone();
            crate::core::reactivity::create_effect(move || {
                let new_root = (builder_ref.borrow_mut())();
                if let Some(st) = (*state_ref.borrow_mut()).as_mut() {
                    st.rebuild_ui(new_root);
                    st.window.request_redraw();
                }
            });

            let adapter = Adapter::with_mixed_handlers(
                event_loop,
                &window,
                AccessKitBootstrap,
                self.event_loop_proxy.clone(),
            );
            window.set_visible(true);
            self.accesskit_adapter = Some(adapter);
        }

        if let Some(state) = (*self.state.borrow()).as_ref() {
            state.window.request_redraw();
        }
        self.is_suspended = false;
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.is_suspended = true;
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.shutdown_renderer();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let Some(adapter) = &mut self.accesskit_adapter {
            if let Some(st) = (*self.state.borrow()).as_ref() {
                if st.window.id() == id {
                    adapter.process_event(&st.window, &event);
                }
            }
        }
        match event {
            WindowEvent::CloseRequested => {
                self.shutdown_renderer();
                event_loop.exit();
            }
            WindowEvent::Destroyed => {
                self.shutdown_renderer();
            }
            WindowEvent::CursorMoved { position, .. } => self.handle_cursor_moved(id, position),
            WindowEvent::CursorLeft { .. } => self.handle_cursor_left(id),
            WindowEvent::MouseInput {
                state: element_state,
                button,
                ..
            } => self.handle_mouse_button(id, element_state, button),
            WindowEvent::MouseWheel { delta, .. } => {
                self.handle_mouse_wheel(id, delta);
            }
            WindowEvent::DroppedFile(path) => {
                super::push_dropped_file(path);
            }
            WindowEvent::HoveredFile(_) => {}
            WindowEvent::HoveredFileCancelled => {}
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::Ime(ime) => {
                self.handle_ime_event(ime);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let _ = self.handle_keyboard_input(event);
            }
            WindowEvent::Occluded(occluded) if !occluded => {
                if let Some(state) = (*self.state.borrow_mut()).as_mut() {
                    if state.window.id() == id {
                        state.window.set_visible(true);
                        // One redraw; avoid extra `configure` / forced multi-frame redraw (flicker).
                        state.window.request_redraw();
                    }
                }
            }
            WindowEvent::Resized(physical_size)
                if physical_size.width > 0 && physical_size.height > 0 =>
            {
                if let Some(state) = (*self.state.borrow_mut()).as_mut() {
                    if state.window.id() == id {
                        state.note_winit_resized(physical_size);
                        state.resize(physical_size);
                        state.window.request_redraw();
                        self.force_redraw_count = self.force_redraw_count.max(1);
                    }
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(state) = (*self.state.borrow_mut()).as_mut() {
                    if state.window.id() == id {
                        state.layout_status = crate::core::tree::LayoutStatus::LayoutChanged;
                        state.window.request_redraw();
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                // Flush any buffered file drops once per frame.
                let paths = super::take_dropped_files();
                if !paths.is_empty() {
                    super::fire_file_drop(paths);
                }
                self.handle_redraw_requested(id, event_loop);
            }
            _ => {}
        }
    }
}

pub(super) fn run<F: FnMut() -> Element + 'static>(app_builder: F) {
    #[cfg(feature = "runtime-logging")]
    env_logger::builder()
        .filter_level(log::LevelFilter::Warn)
        .filter_module("wgpu_hal::vulkan::instance", log::LevelFilter::Off)
        .filter_module("fontdb", log::LevelFilter::Error)
        .format(|buf, record| {
            use std::io::Write;
            let ts = buf.timestamp();
            writeln!(
                buf,
                "[{} {:<5} {}] {}",
                ts,
                record.level(),
                record.target(),
                record.args()
            )
        })
        .init();

    crate::init();

    let event_loop = EventLoop::<super::RuiUserEvent>::with_user_event()
        .build()
        .expect("event loop");
    let event_loop_proxy = event_loop.create_proxy();
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = super::App {
        state: std::rc::Rc::new(std::cell::RefCell::new(None)),
        is_suspended: false,
        force_redraw_count: 0,
        app_builder: std::rc::Rc::new(std::cell::RefCell::new(app_builder)),
        cursor_pos: winit::dpi::PhysicalPosition::new(0.0, 0.0),
        last_frame_time: std::time::Instant::now(),
        modifiers: ModifiersState::default(),
        left_button_down: false,
        last_click_at: None,
        last_click_pos: [0.0, 0.0],
        click_count: 0,
        scrollbar_drag: None,
        drag_reorder: None,
        input_drag_rest: None,
        input_drag_anchor: 0,
        label_drag_key: None,
        label_drag_anchor: 0,
        scroll_accum: std::collections::HashMap::new(),
        hover_scroll_rest: None,
        hover_scroll_axis: None,
        scrollbar_visible_until: std::collections::HashMap::new(),
        scroll_velocity: std::collections::HashMap::new(),
        smooth_active_until: std::collections::HashMap::new(),
        scroll_max_lowpass: std::collections::HashMap::new(),
        scroll_metrics_cache: std::collections::HashMap::new(),
        scroll_metric_duplicate_warned: std::collections::HashSet::new(),
        scroll_signal_key_cache: std::collections::HashMap::new(),
        scroll_diag_last_log: std::collections::HashMap::new(),
        event_loop_proxy: event_loop_proxy.clone(),
        accesskit_adapter: None,
        #[cfg(debug_assertions)]
        hot_reload: super::hot_reload::HotReloadState::new(event_loop_proxy),
    };
    let _ = event_loop.run_app(&mut app);
    // Belt-and-suspenders: if exit skipped `exiting` / CloseRequested, still drop GPU
    // before thread-local reactive Runtime destructors run.
    app.shutdown_renderer();
}
