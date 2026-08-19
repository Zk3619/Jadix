use std::sync::Arc;

use accesskit::{Node, NodeId, Role, Tree, TreeId, TreeUpdate};
use accesskit_winit::Adapter;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, TouchPhase, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::ModifiersState,
    window::WindowId,
};

use crate::core::tree::Element;

fn collect_scroll_rests(el: &crate::core::tree::Element, out: &mut std::collections::HashSet<String>) {
    if let Some(fid) = el.focus_id.as_deref() {
        if let Some(rest) = crate::core::scroll_viewport::strip_scroll_viewport_focus_id(fid) {
            out.insert(rest.to_string());
        }
    }
    for child in &el.children {
        collect_scroll_rests(child, out);
    }
}

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
        super::tray::dispose_trays_for_shutdown();

        // 1) Pull secondary UI/GPU state out of the map *before* disposing owners, so
        //    OwnerHandle Drop → effect cleanup cannot re-enter `secondaries` (RefCell panic)
        //    and leave the process wedged with a live HWND (Windows file lock on gallery.exe).
        let mut secs = std::mem::take(&mut *self.secondaries.borrow_mut());
        for (id, mut sec) in secs.drain() {
            if let Some(token) = super::window_scope::forget_secondary(id) {
                super::portal_overlay::drop_overlay_stack(
                    super::window_scope::WindowScope::Secondary(token),
                );
            }
            sec.hide_window();
            sec.trim_before_drop();
            drop(sec.take_owner());
            drop(sec);
        }
        self.secondary_accesskit.clear();
        self.ime_owner_window = None;
        // 2) Drop registry Arcs so OS windows are destroyed (not only SecondaryWindowState).
        drop(self.window_registry.drain_secondaries());

        let Some(state) = self.state.borrow_mut().take() else {
            return;
        };
        // End the `borrow_mut` guard before `state` drops. Dropping `AppState` runs
        // element `UnmountHook`s that may touch `self.state` via the shared Rc still held
        // by effect closures until `clear_effects` runs.
        crate::app::runtime::clear_app_runtime();
        self.accesskit_adapter = None;
        // Clear shared FontSystem pointer before dropping primary text_renderer.
        crate::text::clear_shared_font_system();
        crate::text::shutdown_layout_text();
        // Drop effect closures before GPU/UI state so they cannot rebuild into a dead cell.
        crate::core::reactivity::clear_effects();
        drop(state);
        self.pointer_window = None;
    }

    fn drop_secondary(&mut self, id: WindowId) {
        if let Some(token) = super::window_scope::forget_secondary(id) {
            super::portal_overlay::drop_overlay_stack(super::window_scope::WindowScope::Secondary(
                token,
            ));
        }
        if let Some(mut sec) = self.secondaries.borrow_mut().remove(&id) {
            let mut scroll_rests = std::collections::HashSet::new();
            collect_scroll_rests(&sec.root_element, &mut scroll_rests);
            for rest in &scroll_rests {
                crate::core::scroll_viewport::remove_scroll_viewport_signals(rest);
                super::scroll_support::cancel_scroll_item_into_view(rest);
            }
            sec.hide_window();
            sec.trim_before_drop();
            drop(sec.take_owner());
            drop(sec);
        }
        self.secondary_accesskit.remove(&id);
        if self.ime_owner_window == Some(id) {
            self.ime_owner_window = None;
        }
        if self.pointer_window == Some(id) {
            self.pointer_window = None;
        }
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
        if self.is_secondary_window(window_id) {
            self.handle_secondary_mouse_button(
                window_id,
                ElementState::Pressed,
                MouseButton::Right,
            );
        } else {
            self.with_primary_input(window_id, |app| {
                app.handle_mouse_button(window_id, ElementState::Pressed, MouseButton::Right, None);
            });
        }
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
            tree_id: TreeId::ROOT,
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
                crate::app::runtime::request_redraw_all();
            }
            super::RuiUserEvent::OpenWindow { options, builder } => {
                let wants_glass = options.compositor_alpha();
                let mut composition = wants_glass;
                let Ok(created) = event_loop.create_window(options.window_attributes(composition))
                else {
                    log::warn!("multi-window: create_window failed");
                    return;
                };
                let mut window = Arc::new(created);
                super::window_chrome::apply_created_window(&window, &options, composition);
                let placeholder = crate::core::tree::Element::new().w("100%").h("100%");

                let secondary = if wants_glass {
                    if let Some(gpu) = super::glass_gpu::try_acquire(&window) {
                        super::secondary_window::SecondaryWindowState::from_compositor(
                            window.clone(),
                            gpu,
                            placeholder,
                            options.clone(),
                        )
                    } else {
                        log::warn!(
                            "glass: no PreMultiplied swapchain; falling back to the primary GPU"
                        );
                        composition = false;
                        drop(window);
                        let Ok(created) =
                            event_loop.create_window(options.window_attributes(false))
                        else {
                            log::warn!("multi-window: create_window failed");
                            return;
                        };
                        window = Arc::new(created);
                        super::window_chrome::apply_created_window(&window, &options, false);
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
                            &primary.adapter,
                            &primary.config,
                            placeholder,
                            options.clone(),
                        ) {
                            Ok(s) => s,
                            Err(e) => {
                                log::warn!("multi-window: secondary init failed: {e}");
                                return;
                            }
                        }
                    }
                } else {
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
                        &primary.adapter,
                        &primary.config,
                        placeholder,
                        options.clone(),
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
                self.secondaries.borrow_mut().insert(id, secondary);

                let builder_rc = std::rc::Rc::new(std::cell::RefCell::new(builder.0));
                let secondaries = self.secondaries.clone();
                let token = super::window_scope::token_for_secondary(id);
                let (owner, _) = crate::core::reactivity::create_root(move || {
                    // Stamp the owner tree so nested Dialog/Select effects keep Secondary
                    // scope on re-run (without this, portal_sync / reconcile hit the primary).
                    crate::core::reactivity::bind_current_owner_window_key_scope(token);
                    let builder_rc = builder_rc.clone();
                    let secondaries = secondaries.clone();
                    crate::core::reactivity::create_effect(move || {
                        let _scope = super::window_scope::enter_secondary_id(id);
                        let new_root = (builder_rc.borrow_mut())();
                        if let Some(sec) = secondaries.borrow_mut().get_mut(&id) {
                            sec.replace_root(new_root);
                            sec.window.request_redraw();
                        }
                    });
                });
                if let Some(sec) = self.secondaries.borrow_mut().get_mut(&id) {
                    sec.attach_owner(owner);
                }

                let adapter = Adapter::with_mixed_handlers(
                    event_loop,
                    &window,
                    AccessKitBootstrap,
                    self.event_loop_proxy.clone(),
                );
                self.secondary_accesskit.insert(id, adapter);

                window.set_visible(true);
                super::window_chrome::apply_created_window(&window, &options, composition);
                window.request_redraw();
            }
            super::RuiUserEvent::CloseWindow { id } => {
                let is_primary = self.window_registry.primary_id() == Some(id)
                    || (*self.state.borrow())
                        .as_ref()
                        .is_some_and(|s| s.window.id() == id);
                if is_primary {
                    self.shutdown_renderer();
                    event_loop.exit();
                    return;
                }
                self.drop_secondary(id);
                if let Some(slot) = self.window_registry.remove_secondary(id) {
                    slot.window.set_visible(false);
                }
            }
            super::RuiUserEvent::Exit => {
                self.shutdown_renderer();
                event_loop.exit();
            }
            super::RuiUserEvent::AccessKit(ev) => {
                use accesskit_winit::WindowEvent as AkWindowEvent;
                let wid = ev.window_id;
                let is_secondary = self.is_secondary_window(wid);
                match ev.window_event {
                    AkWindowEvent::ActionRequested(req) => {
                        if is_secondary {
                            super::window_scope::force_secondary_id(wid);
                            if let Some(sec) = self.secondaries.borrow_mut().get_mut(&wid) {
                                if crate::a11y::dispatch_accesskit_action(&sec.root_element, &req) {
                                    sec.window.request_redraw();
                                }
                            }
                        } else {
                            super::window_scope::force_primary();
                            if let Some(state) = (*self.state.borrow_mut()).as_mut() {
                                if crate::a11y::dispatch_accesskit_action(&state.root_element, &req)
                                {
                                    state.window.request_redraw();
                                }
                            }
                        }
                    }
                    AkWindowEvent::InitialTreeRequested => {
                        if is_secondary {
                            super::window_scope::force_secondary_id(wid);
                            if let Some(adapter) = self.secondary_accesskit.get_mut(&wid) {
                                if let Some(sec) = self.secondaries.borrow().get(&wid) {
                                    let focused = crate::core::reactivity::use_signal(
                                        "__rui_button_focus_id",
                                        String::new,
                                    )
                                    .get();
                                    let fid = if focused.is_empty() {
                                        None
                                    } else {
                                        Some(focused.as_str())
                                    };
                                    let update = crate::a11y::tree_update_from_elements(
                                        &sec.root_element,
                                        fid,
                                    );
                                    adapter.update_if_active(|| update);
                                }
                            }
                        } else if let Some(adapter) = &mut self.accesskit_adapter {
                            super::window_scope::force_primary();
                            if let Some(state) = (*self.state.borrow()).as_ref() {
                                let update = state.accesskit_tree_update();
                                adapter.update_if_active(|| update);
                            }
                        }
                    }
                    AkWindowEvent::AccessibilityDeactivated => {
                        // No visual change; do not pump redraw.
                        log::debug!("accesskit: accessibility deactivated window={wid:?}");
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

        // Scrollbar auto-hide: `WaitUntil` the hold deadline, then one redraw starts fade-out.
        // Expired keys MUST be removed — leaving them makes every later `about_to_wait`
        // see `until <= now` and `request_redraw()` forever (~vsync idle CPU).
        if self.prune_expired_scrollbar_visibility(now) {
            if let Some(state) = (*self.state.borrow()).as_ref() {
                state.window.request_redraw();
            }
        }
        for &until in self.scrollbar_visible_until.values() {
            next_wake = Some(next_wake.map_or(until, |t| t.min(until)));
        }

        if let Some(wake) = self.caret_wake_at {
            if wake <= now {
                self.caret_wake_at = None;
                if let Some(state) = (*self.state.borrow()).as_ref() {
                    state.window.request_redraw();
                }
                for sec in self.secondaries.borrow().values() {
                    if sec.has_input_caret {
                        sec.window.request_redraw();
                    }
                }
            } else {
                next_wake = Some(next_wake.map_or(wake, |t| t.min(wake)));
            }
        }

        // Deferred IME hard-release after OS blur (see `ime_os_blur_at`).
        if let Some(at) = self.ime_os_blur_at {
            if at <= now {
                if let Some(state) = (*self.state.borrow()).as_ref() {
                    state.window.request_redraw();
                }
            } else {
                next_wake = Some(next_wake.map_or(at, |t| t.min(at)));
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

        // Toast 自动退场 / 退场动画结束：到期唤醒并推进队列。
        if crate::components::toast::tick_toast_lifecycle() {
            if let Some(state) = (*self.state.borrow()).as_ref() {
                state.window.request_redraw();
            }
        }
        if let Some(wake) = crate::components::toast::toast_next_wake_at() {
            if wake <= now {
                if let Some(state) = (*self.state.borrow()).as_ref() {
                    state.window.request_redraw();
                }
            } else {
                next_wake = Some(next_wake.map_or(wake, |t| t.min(wake)));
            }
        }

        for sec in self.secondaries.borrow().values() {
            if let Some(at) = sec.ime_os_blur_at {
                if at <= now {
                    sec.window.request_redraw();
                } else {
                    next_wake = Some(next_wake.map_or(at, |t| t.min(at)));
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
            let attrs = self.window_options.to_window_attributes();
            let window = event_loop.create_window(attrs).unwrap();
            super::window_chrome::apply_created_window(
                &window,
                &self.window_options,
                self.window_options.compositor_alpha(),
            );
            let window = Arc::new(window);

            // Initialize GPU/runtime state with a private bootstrap root. The user app tree must be
            // built exactly once, inside the tracked root effect below; otherwise startup creates
            // a throwaway tree whose Drop/on_unmount side effects can invalidate resources still
            // referenced by the tracked tree.
            let state = pollster::block_on(super::state::AppState::new_bootstrap(
                window.clone(),
                self.window_options.clone(),
            ));
            *self.state.borrow_mut() = Some(state);
            self.window_registry.insert_primary(window.clone());

            let state_ref = self.state.clone();
            crate::app::runtime::set_app_runtime(crate::app::runtime::AppRuntime::new(
                std::rc::Rc::downgrade(&state_ref),
                std::rc::Rc::downgrade(&self.secondaries),
            ));
            crate::app::reactive::reset_registered_anchors();

            crate::debug_stack::reset_crumb_file();
            crate::debug_stack::crumb("lifecycle: before root create_effect");
            // Enable Inspector before the root effect so wrap_app sees the flag on first run.
            #[cfg(feature = "devtools")]
            if self.window_options.devtools {
                crate::app::devtools::DevToolsState::enable_for_window();
            }
            // create_effect runs synchronously on registration. By the time it returns, the real
            // root has been reconciled and deferred fine-grained reactive anchors have mounted, so
            // making the window visible cannot expose the bootstrap root.
            let builder_ref = self.app_builder.clone();
            crate::core::reactivity::mark_next_effect_as_ui_root();
            crate::core::reactivity::create_effect(move || {
                crate::debug_stack::crumb("root_effect: before app_builder");
                let new_root = (builder_ref.borrow_mut())();
                #[cfg(feature = "devtools")]
                let new_root = crate::app::devtools::wrap_app(new_root);
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
            let composition = self
                .state
                .borrow()
                .as_ref()
                .is_some_and(|s| s.composition_swapchain);
            super::window_chrome::apply_created_window(&window, &self.window_options, composition);
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
        if let Some(win) = self.secondaries.borrow().get(&id).map(|s| s.window.clone()) {
            if let Some(adapter) = self.secondary_accesskit.get_mut(&id) {
                adapter.process_event(&win, &event);
            }
        }
        match event {
            WindowEvent::CloseRequested => {
                let is_primary = self.window_registry.primary_id() == Some(id)
                    || (*self.state.borrow())
                        .as_ref()
                        .is_some_and(|s| s.window.id() == id);
                if !is_primary {
                    self.drop_secondary(id);
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
                    self.drop_secondary(id);
                    let _ = self.window_registry.remove_secondary(id);
                    return;
                }
                self.shutdown_renderer();
            }
            WindowEvent::CursorMoved { position, .. } => {
                if self.is_secondary_window(id) {
                    self.handle_secondary_cursor_moved(id, position, event_loop);
                } else {
                    self.with_primary_input(id, |app| {
                        app.handle_cursor_moved(id, position, event_loop)
                    });
                }
            }
            WindowEvent::CursorLeft { .. } => {
                if self.is_secondary_window(id) {
                    self.handle_secondary_cursor_left(id, event_loop);
                } else {
                    self.with_primary_input(id, |app| app.handle_cursor_left(id, event_loop));
                }
            }
            WindowEvent::MouseInput {
                state: element_state,
                button,
                ..
            } => {
                if self.is_secondary_window(id) {
                    self.handle_secondary_mouse_button(id, element_state, button);
                } else {
                    self.with_primary_input(id, |app| {
                        app.handle_mouse_button(id, element_state, button, Some(event_loop))
                    });
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if self.is_secondary_window(id) {
                    self.handle_secondary_mouse_wheel(id, delta);
                } else {
                    self.with_primary_input(id, |app| app.handle_mouse_wheel(id, delta));
                }
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
                            if self.is_secondary_window(id) {
                                self.handle_secondary_cursor_moved(id, touch.location, event_loop);
                                self.handle_secondary_mouse_button(
                                    id,
                                    ElementState::Pressed,
                                    MouseButton::Left,
                                );
                            } else {
                                self.with_primary_input(id, |app| {
                                    app.handle_cursor_moved(id, touch.location, event_loop);
                                    app.handle_mouse_button(
                                        id,
                                        ElementState::Pressed,
                                        MouseButton::Left,
                                        Some(event_loop),
                                    );
                                });
                            }
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
                            if self.is_secondary_window(id) {
                                self.handle_secondary_cursor_moved(id, touch.location, event_loop);
                            } else {
                                self.with_primary_input(id, |app| {
                                    app.handle_cursor_moved(id, touch.location, event_loop);
                                });
                            }
                        }
                    }
                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        self.active_touches.remove(&touch.id);
                        if self.active_touches.len() < 2 {
                            self.pinch_prev_dist = None;
                        }
                        if self.primary_touch_id == Some(touch.id) {
                            self.cursor_pos = touch.location;
                            if self.is_secondary_window(id) {
                                self.handle_secondary_cursor_moved(id, touch.location, event_loop);
                                self.handle_secondary_mouse_button(
                                    id,
                                    ElementState::Released,
                                    MouseButton::Left,
                                );
                            } else {
                                self.with_primary_input(id, |app| {
                                    app.handle_cursor_moved(id, touch.location, event_loop);
                                    app.handle_mouse_button(
                                        id,
                                        ElementState::Released,
                                        MouseButton::Left,
                                        Some(event_loop),
                                    );
                                });
                            }
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
                if self.is_secondary_window(id) {
                    super::window_scope::force_secondary_id(id);
                    self.handle_ime_event(ime, id);
                } else {
                    self.with_primary_input(id, |app| app.handle_ime_event(ime, id));
                }
            }
            WindowEvent::Focused(focused) => {
                let is_primary = (*self.state.borrow())
                    .as_ref()
                    .is_some_and(|state| state.window.id() == id);
                if !is_primary {
                    if focused {
                        self.bind_pointer_window(id);
                        super::window_scope::force_secondary_id(id);
                        if let Some(sec) = self.secondaries.borrow_mut().get_mut(&id) {
                            sec.os_focused = true;
                            sec.ime_os_blur_at = None;
                            sec.window.request_redraw();
                        }
                    } else if let Some(sec) = self.secondaries.borrow_mut().get_mut(&id) {
                        // Same TIP-candidate soft-blur as primary: do not drop IME immediately.
                        sec.os_focused = false;
                        let at = std::time::Instant::now() + std::time::Duration::from_millis(200);
                        sec.ime_os_blur_at = Some(at);
                        event_loop.set_control_flow(ControlFlow::WaitUntil(at));
                    }
                } else if focused {
                    self.bind_pointer_window(id);
                    // Must not use enter_primary() guard here — its Drop would restore a
                    // stuck secondary SIGNAL_KEY_SCOPE and freeze primary input routing.
                    super::window_scope::force_primary();
                    self.os_window_focused = true;
                    self.ime_os_blur_at = None;
                } else {
                    // Defer hard release: TIP candidate UI may briefly steal focus.
                    self.os_window_focused = false;
                    self.ime_os_blur_at =
                        Some(std::time::Instant::now() + std::time::Duration::from_millis(200));
                    event_loop
                        .set_control_flow(ControlFlow::WaitUntil(self.ime_os_blur_at.unwrap()));
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if self.is_secondary_window(id) {
                    self.handle_secondary_keyboard(id, event);
                } else {
                    self.with_primary_input(id, |app| {
                        let _ = app.handle_keyboard_input(event);
                    });
                }
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
                if let Some(sec) = self.secondaries.borrow_mut().get_mut(&id) {
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
                if let Some(sec) = self.secondaries.borrow_mut().get_mut(&id) {
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
                if self.secondaries.borrow().contains_key(&id) {
                    self.handle_secondary_redraw(id, event_loop);
                } else {
                    self.handle_redraw_requested(id, event_loop);
                }
            }
            _ => {}
        }
    }
}

pub(super) fn run<F: FnMut() -> Element + 'static>(
    window_options: super::WindowOptions,
    app_builder: F,
) {
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
        mouse_buttons: 0,
        last_client_pos: None,
        last_mouse_target: None,
        last_mouse_path: None,
        click_press_path: None,
        last_click_at: None,
        last_click_pos: [0.0, 0.0],
        click_count: 0,
        scrollbar_drag: None,
        content_scroll_drag: None,
        textarea_resize_drag: None,
        primary_touch_id: None,
        active_touches: std::collections::HashMap::new(),
        pinch_prev_dist: None,
        pinch_scale: 1.0,
        long_press: None,
        pointer_window: None,
        window_registry: super::multi_window::WindowRegistry::new(),
        secondaries: std::rc::Rc::new(std::cell::RefCell::new(std::collections::HashMap::new())),
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
        ime_allowed_latched: false,
        os_window_focused: true,
        ime_os_blur_at: None,
        event_loop_proxy: event_loop_proxy.clone(),
        accesskit_adapter: None,
        secondary_accesskit: std::collections::HashMap::new(),
        ime_owner_window: None,
        custom_cursors: crate::style::CustomCursorCache::new(),
        window_options,
        #[cfg(debug_assertions)]
        hot_reload: super::hot_reload::HotReloadState::new(event_loop_proxy),
    };
    let _ = event_loop.run_app(&mut app);
    // Belt-and-suspenders: if exit skipped `exiting` / CloseRequested, still drop GPU
    // before thread-local reactive Runtime destructors run.
    app.shutdown_renderer();
}
