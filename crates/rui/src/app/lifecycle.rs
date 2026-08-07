use std::sync::Arc;

use accesskit::{Node, NodeId, Role, Tree, TreeUpdate};
use accesskit_winit::Adapter;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, TouchPhase, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::ModifiersState,
    window::WindowId,
};

use crate::core::tree::Element;

impl super::App {
    /// Drop GPU resources while the window / event loop are still valid.
    ///
    /// The root `create_effect` captures `Rc<RefCell<Option<AppState>>>`. If we leave
    /// teardown to thread-local `Runtime` destructors after `main` returns, winit has
    /// already torn down and wgpu-hal's GLES/WGL path panics inside `make_current`
    /// (`unwrap` on OS error 2004). With `panic = "abort"` that becomes Windows
    /// `STATUS_STACK_BUFFER_OVERRUN` (0xC0000409).
    fn shutdown_renderer(&mut self) {
        // Idempotent: CloseRequested / Destroyed / exiting / post-run_app may all call us.
        let Some(state) = self.state.borrow_mut().take() else {
            // Still drop secondary GPU surfaces if primary was already cleared.
            self.secondaries.clear();
            return;
        };
        // End the `borrow_mut` guard before `state` drops. Dropping `AppState` runs
        // element `UnmountHook`s that may touch `self.state` via the shared Rc still held
        // by effect closures until `clear_effects` runs.
        crate::app::runtime::clear_app_runtime();
        self.accesskit_adapter = None;
        // Drop secondaries while primary device/window are still alive (shared device clones).
        self.secondaries.clear();
        // Clear shared FontSystem pointer before dropping primary text_renderer.
        crate::text::clear_shared_font_system();
        crate::text::shutdown_layout_text();
        // Drop effect closures before GPU/UI state so they cannot rebuild into a dead cell.
        crate::core::reactivity::clear_effects();
        drop(state);
    }

    fn touch_pinch_distance(
        touches: &std::collections::HashMap<u64, winit::dpi::PhysicalPosition<f64>>,
    ) -> Option<f32> {
        if touches.len() < 2 {
            return None;
        }
        let mut iter = touches.values();
        let a = *iter.next()?;
        let b = *iter.next()?;
        let dx = (a.x - b.x) as f32;
        let dy = (a.y - b.y) as f32;
        let d = (dx * dx + dy * dy).sqrt();
        if d > 1.0 {
            Some(d)
        } else {
            None
        }
    }

    fn update_pinch_scale(&mut self) {
        let Some(dist) = Self::touch_pinch_distance(&self.active_touches) else {
            return;
        };
        if let Some(prev) = self.pinch_prev_dist {
            if prev > 1.0 {
                let factor = (dist / prev).clamp(0.5, 2.0);
                self.pinch_scale = (self.pinch_scale * factor).clamp(0.25, 4.0);
                // Documented MVP signal: apps can read `__rui_pinch_scale` (no scroll zoom yet).
                crate::core::reactivity::use_signal("__rui_pinch_scale", || 1.0_f32)
                    .set(self.pinch_scale);
                if let Some(state) = (*self.state.borrow()).as_ref() {
                    state.window.request_redraw();
                }
            }
        }
        self.pinch_prev_dist = Some(dist);
    }

    /// After 500ms hold without >8px move, fire the same path as right-click once.
    pub(super) fn try_fire_long_press(&mut self) {
        let Some(lp) = self.long_press.as_mut() else {
            return;
        };
        if lp.fired {
            return;
        }
        if lp.start.elapsed() < std::time::Duration::from_millis(500) {
            return;
        }
        lp.fired = true;
        let window_id = lp.window_id;
        let pos = lp.pos;
        // Restore cursor to press position for hit-test, then reuse right-click path.
        self.cursor_pos = winit::dpi::PhysicalPosition::new(pos[0] as f64, pos[1] as f64);
        self.handle_mouse_button(window_id, ElementState::Pressed, MouseButton::Right);
    }

    pub(super) fn begin_long_press(&mut self, window_id: WindowId, pos: [f32; 2]) {
        self.long_press = Some(super::LongPressTrack {
            window_id,
            start: std::time::Instant::now(),
            pos,
            fired: false,
        });
    }

    pub(super) fn cancel_long_press(&mut self) {
        self.long_press = None;
    }

    pub(super) fn note_long_press_move(&mut self, x: f32, y: f32) {
        const SLOP_PX: f32 = 8.0;
        let Some(lp) = self.long_press.as_ref() else {
            return;
        };
        if lp.fired {
            return;
        }
        let dx = x - lp.pos[0];
        let dy = y - lp.pos[1];
        if dx * dx + dy * dy > SLOP_PX * SLOP_PX {
            self.long_press = None;
        }
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
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: super::RuiUserEvent) {
        match event {
            #[cfg(debug_assertions)]
            super::RuiUserEvent::HotReloadWake => {
                if let Some(state) = (*self.state.borrow_mut()).as_mut() {
                    state.window.request_redraw();
                    self.force_redraw_count = self.force_redraw_count.max(2);
                }
            }
            super::RuiUserEvent::UiWake => {
                super::ui_bridge::drain_ui_tasks();
                if let Some(state) = (*self.state.borrow_mut()).as_mut() {
                    state.window.request_redraw();
                }
            }
            super::RuiUserEvent::OpenWindow { title, mut builder } => {
                let attrs = super::multi_window::default_window_attributes_with_title(title);
                let Ok(window) = event_loop.create_window(attrs) else {
                    log::warn!("multi-window: create_window failed");
                    return;
                };
                let window = Arc::new(window);
                let root = (builder.0)();

                let secondary = {
                    let state_borrow = self.state.borrow();
                    let Some(primary) = state_borrow.as_ref() else {
                        log::warn!("multi-window: OpenWindow before primary AppState ready");
                        return;
                    };
                    match super::secondary_window::SecondaryWindowState::new(
                        window.clone(),
                        Arc::clone(&primary.device),
                        Arc::clone(&primary.queue),
                        &primary.instance,
                        &primary.config,
                        root,
                    ) {
                        Ok(s) => s,
                        Err(e) => {
                            log::warn!("multi-window: secondary init failed: {e}");
                            return;
                        }
                    }
                };

                let id = window.id();
                self.window_registry.insert_secondary_slot(window.clone());
                self.secondaries.insert(id, secondary);
                window.set_visible(true);
                window.request_redraw();
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
                    AkWindowEvent::AccessibilityDeactivated => {
                        // No visual change; do not pump redraw.
                        log::debug!("accesskit: accessibility deactivated");
                    }
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(debug_assertions)]
        if self.hot_reload.has_pending() {
            if let Some(state) = (*self.state.borrow()).as_ref() {
                state.window.request_redraw();
            }
        }

        let now = std::time::Instant::now();
        let mut next_wake: Option<std::time::Instant> = None;

        // Scrollbar auto-hide timer: wake once when `visible_until` expires so fade-out can start,
        // instead of continuous request_redraw for the whole 1.5s hold.
        let mut visibility_expired = false;
        for &until in self.scrollbar_visible_until.values() {
            if until <= now {
                visibility_expired = true;
            } else {
                next_wake = Some(next_wake.map_or(until, |t| t.min(until)));
            }
        }
        if visibility_expired {
            if let Some(state) = (*self.state.borrow()).as_ref() {
                state.window.request_redraw();
            }
        }

        if let Some(wake) = self.caret_wake_at {
            if wake <= now {
                self.caret_wake_at = None;
                if let Some(state) = (*self.state.borrow()).as_ref() {
                    state.window.request_redraw();
                }
            } else {
                next_wake = Some(next_wake.map_or(wake, |t| t.min(wake)));
            }
        }

        // Long-press: wake at the 500ms deadline so we can synthesize ContextMenu.
        if let Some(lp) = &self.long_press {
            if !lp.fired {
                let deadline = lp.start + std::time::Duration::from_millis(500);
                if now >= deadline {
                    self.try_fire_long_press();
                } else {
                    next_wake = Some(next_wake.map_or(deadline, |t| t.min(deadline)));
                }
            }
        }

        if let Some(deadline) = next_wake {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.borrow().is_none() {
            let attrs = super::multi_window::default_window_attributes();
            let window = Arc::new(event_loop.create_window(attrs).unwrap());

            // Initialize GPU/runtime state with a private bootstrap root. The user app tree must be
            // built exactly once, inside the tracked root effect below; otherwise startup creates
            // a throwaway tree whose Drop/on_unmount side effects can invalidate resources still
            // referenced by the tracked tree.
            let state = pollster::block_on(super::state::AppState::new_bootstrap(window.clone()));
            *self.state.borrow_mut() = Some(state);
            self.window_registry.insert_primary(window.clone());

            let state_ref = self.state.clone();
            crate::app::runtime::set_app_runtime(crate::app::runtime::AppRuntime::new(
                std::rc::Rc::downgrade(&state_ref),
            ));
            crate::app::reactive::reset_registered_anchors();

            crate::debug_stack::reset_crumb_file();
            crate::debug_stack::crumb("lifecycle: before root create_effect");
            // create_effect runs synchronously on registration. By the time it returns, the real
            // root has been reconciled and deferred fine-grained reactive anchors have mounted, so
            // making the window visible cannot expose the bootstrap root.
            let builder_ref = self.app_builder.clone();
            crate::core::reactivity::create_effect(move || {
                crate::debug_stack::crumb("root_effect: before app_builder");
                let new_root = (builder_ref.borrow_mut())();
                crate::debug_stack::crumb(&format!(
                    "root_effect: after app_builder children={}",
                    new_root.children.len()
                ));
                if let Some(st) = (*state_ref.borrow_mut()).as_mut() {
                    crate::debug_stack::crumb("root_effect: before rebuild_ui");
                    st.rebuild_ui(new_root);
                    crate::debug_stack::crumb("root_effect: after rebuild_ui");
                    st.window.request_redraw();
                }
                crate::debug_stack::crumb("root_effect: body done (flush_deferred next)");
            });
            crate::debug_stack::crumb("lifecycle: after root create_effect");

            crate::debug_stack::crumb("lifecycle: before accesskit");
            let adapter = Adapter::with_mixed_handlers(
                event_loop,
                &window,
                AccessKitBootstrap,
                self.event_loop_proxy.clone(),
            );
            crate::debug_stack::crumb("lifecycle: after accesskit");
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
                let is_primary = self.window_registry.primary_id() == Some(id)
                    || (*self.state.borrow())
                        .as_ref()
                        .is_some_and(|s| s.window.id() == id);
                if !is_primary {
                    // Secondary close: drop GPU/UI state + slot; keep primary app running.
                    self.secondaries.remove(&id);
                    if let Some(slot) = self.window_registry.remove_secondary(id) {
                        slot.window.set_visible(false);
                    }
                    return;
                }
                self.shutdown_renderer();
                event_loop.exit();
            }
            WindowEvent::Destroyed => {
                let is_primary = self.window_registry.primary_id() == Some(id)
                    || (*self.state.borrow())
                        .as_ref()
                        .is_some_and(|s| s.window.id() == id);
                if !is_primary {
                    self.secondaries.remove(&id);
                    let _ = self.window_registry.remove_secondary(id);
                    return;
                }
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
            // Primary finger → pointer paths; second finger enables pinch scale.
            // Pinch scale is stored on App + signal `__rui_pinch_scale` (no scroll zoom yet).
            WindowEvent::Touch(touch) => {
                match touch.phase {
                    TouchPhase::Started => {
                        self.active_touches.insert(touch.id, touch.location);
                        if self.primary_touch_id.is_none() {
                            self.primary_touch_id = Some(touch.id);
                            self.cursor_pos = touch.location;
                            self.handle_cursor_moved(id, touch.location);
                            self.handle_mouse_button(id, ElementState::Pressed, MouseButton::Left);
                        }
                        if self.active_touches.len() >= 2 {
                            // Cancel content pan when a second finger lands (pinch takes over).
                            self.content_scroll_drag = None;
                            self.pinch_prev_dist = Self::touch_pinch_distance(&self.active_touches);
                        }
                    }
                    TouchPhase::Moved => {
                        self.active_touches.insert(touch.id, touch.location);
                        if self.active_touches.len() >= 2 {
                            self.content_scroll_drag = None;
                            self.update_pinch_scale();
                        } else if self.primary_touch_id == Some(touch.id) {
                            self.cursor_pos = touch.location;
                            self.handle_cursor_moved(id, touch.location);
                        }
                    }
                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        self.active_touches.remove(&touch.id);
                        if self.active_touches.len() < 2 {
                            self.pinch_prev_dist = None;
                        }
                        if self.primary_touch_id == Some(touch.id) {
                            self.cursor_pos = touch.location;
                            self.handle_cursor_moved(id, touch.location);
                            self.handle_mouse_button(id, ElementState::Released, MouseButton::Left);
                            self.primary_touch_id = None;
                            // Promote another finger to primary if still down.
                            if let Some((&nid, &npos)) = self.active_touches.iter().next() {
                                self.primary_touch_id = Some(nid);
                                self.cursor_pos = npos;
                            }
                        }
                    }
                }
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
                if let Some(sec) = self.secondaries.get_mut(&id) {
                    sec.resize(physical_size);
                    sec.window.request_redraw();
                } else if let Some(state) = (*self.state.borrow_mut()).as_mut() {
                    if state.window.id() == id {
                        state.note_winit_resized(physical_size);
                        state.resize(physical_size);
                        state.window.request_redraw();
                        self.force_redraw_count = self.force_redraw_count.max(1);
                    }
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(sec) = self.secondaries.get_mut(&id) {
                    sec.layout_status = crate::core::tree::LayoutStatus::LayoutChanged;
                    sec.window.request_redraw();
                } else if let Some(state) = (*self.state.borrow_mut()).as_mut() {
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
                if self.secondaries.contains_key(&id) {
                    self.handle_secondary_redraw(id);
                } else {
                    self.handle_redraw_requested(id, event_loop);
                }
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
    super::ui_bridge::install_proxy(event_loop_proxy.clone());
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
        content_scroll_drag: None,
        primary_touch_id: None,
        active_touches: std::collections::HashMap::new(),
        pinch_prev_dist: None,
        pinch_scale: 1.0,
        long_press: None,
        window_registry: super::multi_window::WindowRegistry::new(),
        secondaries: std::collections::HashMap::new(),
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
        caret_wake_at: None,
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
