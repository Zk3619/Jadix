use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::ModifiersState,
    window::{Window, WindowId},
};

use crate::core::tree::Element;

impl ApplicationHandler for super::App {
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: ()) {
        #[cfg(debug_assertions)]
        if let Some(state) = (&mut *self.state.borrow_mut()).as_mut() {
            state.window.request_redraw();
            self.force_redraw_count = self.force_redraw_count.max(2);
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
            let window =
                Arc::new(event_loop.create_window(Window::default_attributes().with_title("RUI Engine")).unwrap());

            // Build a real initial UI before the first frame so the first redraw paints content.
            let initial_root = (self.app_builder.borrow_mut())();
            let state = pollster::block_on(super::state::AppState::new(window.clone(), initial_root));
            *self.state.borrow_mut() = Some(state);

            let state_ref = self.state.clone();
            let builder_ref = self.app_builder.clone();
            crate::core::reactivity::create_effect(move || {
                let new_root = (builder_ref.borrow_mut())();
                if let Some(st) = (&mut *state_ref.borrow_mut()).as_mut() {
                    st.rebuild_ui(new_root);
                    st.window.request_redraw();
                }
            });
        }

        if let Some(state) = (*self.state.borrow()).as_ref() {
            state.window.request_redraw();
        }
        self.is_suspended = false;
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.is_suspended = true;
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::CursorMoved { position, .. } => self.handle_cursor_moved(id, position),
            WindowEvent::MouseInput {
                state: element_state,
                button,
                ..
            } => self.handle_non_left_mouse_input(id, element_state, button),
            WindowEvent::MouseWheel { delta, .. } => {
                self.handle_mouse_wheel(id, delta);
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::Ime(ime) => {
                self.handle_ime_event(ime);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let _ = self.handle_keyboard_input(event);
            }
            WindowEvent::Occluded(occluded) => {
                if !occluded {
                    if let Some(state) = (&mut *self.state.borrow_mut()).as_mut() {
                        if state.window.id() == id {
                            state.window.set_visible(true);
                            // One redraw; avoid extra `configure` / forced multi-frame redraw (flicker).
                            state.window.request_redraw();
                        }
                    }
                }
            }
            WindowEvent::Resized(physical_size) => {
                if physical_size.width > 0 && physical_size.height > 0 {
                    if let Some(state) = (&mut *self.state.borrow_mut()).as_mut() {
                        if state.window.id() == id {
                            state.note_winit_resized(physical_size);
                            state.resize(physical_size);
                            state.window.request_redraw();
                            self.force_redraw_count = self.force_redraw_count.max(1);
                        }
                    }
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(state) = (&mut *self.state.borrow_mut()).as_mut() {
                    if state.window.id() == id {
                        state.layout_status = crate::core::tree::LayoutStatus::LayoutChanged;
                        state.window.request_redraw();
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                self.handle_redraw_requested(id, event_loop);
            }
            _ => {}
        }
    }
}

pub(super) fn run<F: FnMut() -> Element + 'static>(app_builder: F) {
    env_logger::builder()
        .filter_level(log::LevelFilter::Warn)
        .filter_module("wgpu_hal::vulkan::instance", log::LevelFilter::Off)
        .filter_module("fontdb", log::LevelFilter::Error)
        .init();

    crate::init();

    let event_loop = EventLoop::new().unwrap();
    #[cfg(debug_assertions)]
    let hot_reload_wake = event_loop.create_proxy();
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
        scrollbar_drag: None,
        input_drag_rest: None,
        input_drag_anchor: 0,
        scroll_accum: std::collections::HashMap::new(),
        hover_scroll_rest: None,
        hover_scroll_axis: None,
        scrollbar_visible_until: std::collections::HashMap::new(),
        scroll_velocity: std::collections::HashMap::new(),
        smooth_active_until: std::collections::HashMap::new(),
        scroll_max_lowpass: std::collections::HashMap::new(),
        scroll_metrics_cache: std::collections::HashMap::new(),
        scroll_signal_key_cache: std::collections::HashMap::new(),
        #[cfg(debug_assertions)]
        hot_reload: super::hot_reload::HotReloadState::new(hot_reload_wake),
    };
    event_loop.run_app(&mut app).unwrap();
}
