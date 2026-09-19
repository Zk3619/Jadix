use std::sync::Arc;
use std::time::Instant;

#[cfg(feature = "a11y")]
use accesskit::{Node, NodeId, Role, Tree, TreeId, TreeUpdate};
#[cfg(feature = "a11y")]
use crate::a11y::winit_adapter::Adapter;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, PointerKind, TouchPhase, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow},
    keyboard::ModifiersState,
    window::{Window, WindowId},
};

use crate::core::tree::Element;

fn collect_scroll_rests(
    el: &crate::core::tree::Element,
    out: &mut std::collections::HashSet<String>,
) {
    crate::core::tree::walk_elements(el, |el| {
        if let Some(fid) = el.focus_id.as_deref() {
            if let Some(rest) = crate::core::scroll_viewport::strip_scroll_viewport_focus_id(fid) {
                out.insert(rest.to_string());
            }
        }
    });
}

/// Idle wait after `about_to_wait`. A future `WaitUntil` (caret ~520ms / long-press
/// 500ms) must not win over a Dialog that just opened: on Windows `request_redraw`
/// is often coalesced, so the loop would sleep until that timer — a first-open
/// empty wait, while later opens already have a live WM_PAINT pump.
pub(crate) fn idle_control_flow(next_wake: Option<Instant>, force_immediate: bool) -> ControlFlow {
    if force_immediate {
        ControlFlow::Poll
    } else if let Some(deadline) = next_wake {
        ControlFlow::WaitUntil(deadline)
    } else {
        ControlFlow::Wait
    }
}

/// `about_to_wait` must Poll while any of these are true.
///
/// `request_redraw` inside about_to_wait is coalesced on Windows. If we only
/// redraw and still `WaitUntil` caret/scrollbar, Dialog/Sheet enter/exit CSS
/// freezes mid-curve (plays a few frames → hitch → continues when the timer
/// fires). Page CSS already latches Poll; modal CSS must too — not only the
/// one-shot armed at `open.set`.
pub(crate) fn about_to_wait_force_immediate(
    modal_one_shot: bool,
    modal_css_needs_frame: bool,
    page_css_needs_frame: bool,
    text_edit_pump: bool,
    paint_clock_active: bool,
    force_redraw_pending: bool,
    resize_followup: bool,
    scrollbar_hide_wake: bool,
) -> bool {
    modal_one_shot
        || modal_css_needs_frame
        || page_css_needs_frame
        || text_edit_pump
        || paint_clock_active
        || force_redraw_pending
        || resize_followup
        || scrollbar_hide_wake
}

/// First idle frame after live scroll/drag. `request_redraw` inside
/// `RedrawRequested` is coalesced on Windows with caret `WaitUntil`, so that
/// frame must arm `force_redraw_count` (Poll) instead of sleeping.
pub(crate) fn scroll_settle_must_pump(was_live: bool, live: bool) -> bool {
    was_live && !live
}

impl super::App {
    /// Drop GPU resources while the window / event loop are still valid.
    ///
    /// The root `create_effect` captures `Rc<RefCell<Option<AppState>>>`. If we leave
    /// teardown to thread-local `Runtime` destructors after `main` returns, winit has
    /// already torn down and wgpu-hal's GLES/WGL path panics inside `make_current`
    /// (`unwrap` on OS error 2004). With `panic = "abort"` that becomes Windows
    /// `STATUS_STACK_BUFFER_OVERRUN` (0xC0000409).
    fn persist_window_state(&self) {
        if let Some(st) = self.state.borrow().as_ref() {
            super::window_lifecycle::emit_closing(st.window.as_ref());
        }
        for sec in self.secondaries.borrow().values() {
            super::window_lifecycle::emit_closing(sec.window.as_ref());
        }
        super::window_lifecycle::emit_exit();
    }

    fn lifecycle_window(&self, id: WindowId) -> Option<Arc<dyn Window>> {
        if let Some(st) = (*self.state.borrow()).as_ref() {
            if st.window.id() == id {
                return Some(st.window.clone());
            }
        }
        self.secondaries.borrow().get(&id).map(|s| s.window.clone())
    }

    fn shutdown_renderer(&mut self) {
        // Idempotent: CloseRequested / Destroyed / exiting / post-run_app may all call us.
        self.persist_window_state();
        super::tray::dispose_trays_for_shutdown();

        // 1) Pull secondary UI/GPU state out of the map *before* disposing owners, so
        //    OwnerHandle Drop → effect cleanup cannot re-enter `secondaries` (RefCell panic)
        //    and leave the process wedged with a live HWND (Windows file lock on gallery.exe).
        let mut secs = std::mem::take(&mut *self.secondaries.borrow_mut());
        for (id, mut sec) in secs.drain() {
            // Unmount hooks (portal_sync) must run under this window's scope, not Primary.
            let _scope = super::window_scope::enter_secondary_id(id);
            sec.hide_window();
            sec.trim_before_drop();
            let glass_window = sec.window.clone();
            drop(sec.take_owner());
            drop(sec);
            crate::platform::glass_detach(glass_window.as_ref());
            drop(_scope);
            if let Some(token) = super::window_scope::forget_secondary(id) {
                super::portal_overlay::drop_overlay_stack(
                    super::window_scope::WindowScope::Secondary(token),
                );
            }
        }
        #[cfg(feature = "a11y")]
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
        #[cfg(feature = "a11y")]
        {
            self.accesskit_adapter = None;
        }
        // Clear shared FontSystem pointer before dropping primary text_renderer.
        crate::text::clear_shared_font_system();
        crate::text::shutdown_layout_text();
        // Drop effect closures before GPU/UI state so they cannot rebuild into a dead cell.
        crate::core::reactivity::clear_effects();
        let window = state.window.clone();
        drop(state);
        crate::platform::glass_detach(window.as_ref());
        self.pointer_window = None;
    }

    fn drop_secondary(&mut self, id: WindowId) {
        if let Some(window) = self.secondaries.borrow().get(&id).map(|s| s.window.clone()) {
            super::window_lifecycle::emit_closing(window.as_ref());
        }
        if let Some(mut sec) = self.secondaries.borrow_mut().remove(&id) {
            // Keep SIGNAL_KEY_SCOPE on this window while Owner/unmount runs so
            // portal_sync cannot write the primary overlay stack.
            let _scope = super::window_scope::enter_secondary_id(id);
            let mut scroll_rests = std::collections::HashSet::new();
            collect_scroll_rests(&sec.root_element, &mut scroll_rests);
            for rest in &scroll_rests {
                crate::core::scroll_viewport::remove_scroll_viewport_signals(rest);
                super::scroll_support::cancel_scroll_item_into_view(rest);
            }
            sec.hide_window();
            sec.trim_before_drop();
            let glass_window = sec.window.clone();
            drop(sec.take_owner());
            drop(sec);
            crate::platform::glass_detach(glass_window.as_ref());
            drop(_scope);
        }
        if let Some(token) = super::window_scope::forget_secondary(id) {
            super::portal_overlay::drop_overlay_stack(super::window_scope::WindowScope::Secondary(
                token,
            ));
        }
        #[cfg(feature = "a11y")]
        self.secondary_accesskit.remove(&id);
        if self.ime_owner_window == Some(id) {
            self.ime_owner_window = None;
        }
        if self.pointer_window == Some(id) {
            self.pointer_window = None;
        }
    }

    fn touch_pinch_distance(
        touches: &std::collections::HashMap<
            winit::event::FingerId,
            winit::dpi::PhysicalPosition<f64>,
        >,
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

    fn sync_pointer_action_touches(&self) {
        let scale = (*self.state.borrow())
            .as_ref()
            .map(|s| s.window.scale_factor() as f32)
            .unwrap_or(1.0)
            .max(1.0);
        let pts: Vec<(u64, [f32; 2])> = self
            .active_touches
            .iter()
            .map(|(&id, p)| (id.into_raw() as u64, [p.x as f32 / scale, p.y as f32 / scale]))
            .collect();
        for &(id, pos) in &pts {
            if Some(id) != self.primary_touch_id.map(|f| f.into_raw() as u64) {
                crate::utils::pointer_drag::tick_pointer(id, pos[0], pos[1]);
            }
        }
        crate::utils::pointer_action::sync_touches(pts);
        if crate::utils::pointer_action::gesture_is_active() {
            if let Some(state) = (*self.state.borrow()).as_ref() {
                state.window.request_redraw();
            }
        }
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

#[cfg(feature = "a11y")]
struct AccessKitBootstrap;

#[cfg(feature = "a11y")]
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

/// AccessKit panics if the HWND was already shown (WGL/DComp init can do that).
#[cfg(feature = "a11y")]
fn create_accesskit_adapter(
    event_loop: &dyn ActiveEventLoop,
    window: &dyn Window,
    proxy: winit::event_loop::EventLoopProxy,
) -> Adapter {
    Adapter::with_mixed_handlers(event_loop, window, AccessKitBootstrap, proxy)
}

impl super::App {
    fn handle_user_event(&mut self, event_loop: &dyn ActiveEventLoop, event: super::RuiUserEvent) {
        match event {
            #[cfg(all(debug_assertions, feature = "hot-reload"))]
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
                if !crate::platform::host_capabilities().multi_window {
                    log::warn!(
                        "multi-window: secondary windows are not supported on {}",
                        crate::platform::host_kind()
                    );
                    return;
                }
                let wants_glass = options.compositor_alpha();
                let mut composition = options.wants_dcomp_swapchain();
                let Ok(created) = event_loop.create_window(options.window_attributes(composition))
                else {
                    log::warn!("multi-window: create_window failed");
                    return;
                };
                let mut window: Arc<dyn Window> = Arc::from(created);
                #[cfg(feature = "a11y")]
                let mut accesskit = Some(create_accesskit_adapter(
                    event_loop,
                    window.as_ref(),
                    self.event_loop_proxy.clone(),
                ));
                super::window_chrome::apply_created_window(window.as_ref(), &options, composition);
                super::window_lifecycle::emit_created(window.as_ref(), options.persistence_label(false));
                let placeholder = crate::core::tree::Element::new().w("100%").h("100%");

                let secondary = if wants_glass {
                    if let Some(gpu) = super::glass_gpu::try_acquire(&window, &options) {
                        super::secondary_window::SecondaryWindowState::from_compositor(
                            window.clone(),
                            gpu,
                            placeholder,
                            options.clone(),
                            composition,
                        )
                    } else {
                        if composition {
                            log::warn!(
                                "dcomp glass: DX12 composition GPU failed; recreating with AccentPolicy fallback"
                            );
                            composition = false;
                            #[cfg(feature = "a11y")]
                            {
                                drop(accesskit.take());
                            }
                            drop(window);
                            let Ok(created) =
                                event_loop.create_window(options.window_attributes(false))
                            else {
                                log::warn!("multi-window: create_window failed");
                                return;
                            };
                            window = Arc::from(created);
                            #[cfg(feature = "a11y")]
                            {
                                accesskit = Some(create_accesskit_adapter(
                                    event_loop,
                                    window.as_ref(),
                                    self.event_loop_proxy.clone(),
                                ));
                            }
                            super::window_chrome::apply_created_window(window.as_ref(), &options, false);
                            super::window_lifecycle::emit_created(
                                window.as_ref(),
                                options.persistence_label(false),
                            );
                        }
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

                #[cfg(feature = "a11y")]
                if let Some(adapter) = accesskit.take() {
                    self.secondary_accesskit.insert(id, adapter);
                }

                window.set_visible(true);
                crate::platform::glass_on_shown(window.as_ref());
                super::window_chrome::apply_created_window(window.as_ref(), &options, composition);
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
            #[cfg(feature = "a11y")]
            super::RuiUserEvent::AccessKit(ev) => {
                use crate::a11y::winit_adapter::AdapterWindowEvent as AkWindowEvent;
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
                }
            }
        }
    }

    fn create_primary_if_needed(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.state.borrow().is_none() {
            let attrs = self.window_options.to_window_attributes();
            let window = event_loop.create_window(attrs).expect("create window");
            #[cfg(feature = "a11y")]
            {
                log::debug!("lifecycle: before accesskit");
                self.accesskit_adapter = Some(create_accesskit_adapter(
                    event_loop,
                    window.as_ref(),
                    self.event_loop_proxy.clone(),
                ));
                log::debug!("lifecycle: after accesskit");
            }
            super::window_chrome::apply_created_window(
                window.as_ref(),
                &self.window_options,
                self.window_options.wants_dcomp_swapchain(),
            );
            super::window_lifecycle::emit_created(
                window.as_ref(),
                self.window_options.persistence_label(true),
            );
            let window: Arc<dyn Window> = Arc::from(window);

            let state = crate::utils::block_on::block_on(super::state::AppState::new_bootstrap(
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

            log::debug!("lifecycle: before root create_effect");
            #[cfg(feature = "devtools")]
            if self.window_options.devtools {
                crate::app::devtools::DevToolsState::enable_for_window();
            }
            let builder_ref = self.app_builder.clone();
            crate::core::reactivity::mark_next_effect_as_ui_root();
            crate::core::reactivity::create_effect(move || {
                log::debug!("root_effect: before app_builder");
                let new_root = (builder_ref.borrow_mut())();
                #[cfg(feature = "devtools")]
                let new_root = crate::app::devtools::wrap_app(new_root);
                log::debug!(
                    "root_effect: after app_builder children={}",
                    new_root.children.len()
                );
                if let Some(st) = (*state_ref.borrow_mut()).as_mut() {
                    log::debug!("root_effect: before rebuild_ui");
                    st.rebuild_ui(new_root);
                    log::debug!("root_effect: after rebuild_ui");
                    st.window.request_redraw();
                }
                log::debug!("root_effect: body done (flush_deferred next)");
            });
            log::debug!("lifecycle: after root create_effect");

            window.set_visible(true);
            crate::platform::glass_on_shown(window.as_ref());
            let composition = self
                .state
                .borrow()
                .as_ref()
                .is_some_and(|s| s.composition_swapchain);
            super::window_chrome::apply_created_window(window.as_ref(), &self.window_options, composition);
        }
    }
}

impl ApplicationHandler for super::App {
    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        while let Ok(event) = self.user_events.try_recv() {
            self.handle_user_event(event_loop, event);
        }
    }

    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.create_primary_if_needed(event_loop);
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
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
        // Windows coalesces `request_redraw` with `ControlFlow::Wait` (Dialog first-open).
        // Static pages have no page-CSS Poll, so without a latched Poll until
        // `RedrawRequested` the fade-start frame never runs and the overlay bar stays at cubic 1.
        if self.prune_expired_scrollbar_visibility(now) {
            if let Some(state) = (*self.state.borrow()).as_ref() {
                state.window.request_redraw();
            }
            self.scrollbar_hide_wake = true;
        }
        for &until in self.scrollbar_visible_until.values() {
            next_wake = Some(next_wake.map_or(until, |t| t.min(until)));
        }

        if let Some(wake) = self.caret_wake_at {
            let caret_wake_stale = (*self.state.borrow()).as_ref().is_some_and(|s| {
                let fid = crate::core::reactivity::use_signal("__rui_button_focus_id", String::new)
                    .peek();
                crate::app::ime_support::is_text_edit_focus(&fid)
                    && !crate::app::ime_support::painted_caret_matches_focus(&s.text_nodes, &fid)
            });
            if caret_wake_stale {
                // Root: drop the previous field's blink deadline so AboutToWait does
                // not `WaitUntil` ~520ms and swallow the click's already-queued redraw
                // (IME already follows `focus_id`; chrome just needed that frame).
                self.caret_wake_at = None;
                // FALLBACK — Windows Poll if chrome still freezes after dropping WaitUntil.
                // `request_redraw` inside about_to_wait is coalesced unless ControlFlow::Poll.
                // super::input_controller::arm_text_edit_force_immediate();
                // self.force_redraw_count = self.force_redraw_count.max(2);
                // if let Some(state) = (*self.state.borrow()).as_ref() {
                //     state.window.request_redraw();
                // }
            } else if wake <= now {
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

        // Carousel automatically rotates.
        if crate::components::carousel::tick_carousel_autoplay() {
            if let Some(state) = (*self.state.borrow()).as_ref() {
                state.window.request_redraw();
            }
        }
        if let Some(wake) = crate::components::carousel::carousel_next_wake_at() {
            if wake <= now {
                if let Some(state) = (*self.state.borrow()).as_ref() {
                    state.window.request_redraw();
                }
            } else {
                next_wake = Some(next_wake.map_or(wake, |t| t.min(wake)));
            }
        }

        // Drag / resize / gesture: keep edge auto-scroll alive while held still.
        if crate::utils::drag::is_active() || crate::utils::pointer_action::edge_scroll_active() {
            if let Some(state) = (*self.state.borrow()).as_ref() {
                let size = state.window.surface_size();
                let scale = state.window.scale_factor() as f32;
                let vp = [
                    size.width as f32 / scale.max(1.0),
                    size.height as f32 / scale.max(1.0),
                ];
                let scrolled = if crate::utils::drag::is_active() {
                    crate::utils::drag::pump_autoscroll(&state.root_element, vp)
                } else {
                    crate::utils::pointer_action::pump_edge_scroll(&state.root_element)
                };
                if scrolled {
                    state.window.request_redraw();
                }
            }
        }
        if crate::utils::drag::is_coasting() {
            if let Some(state) = (*self.state.borrow()).as_ref() {
                let size = state.window.surface_size();
                let scale = state.window.scale_factor() as f32;
                let vp = [
                    size.width as f32 / scale.max(1.0),
                    size.height as f32 / scale.max(1.0),
                ];
                let _ = crate::utils::drag::pump_inertia(&state.root_element, vp);
                state.window.request_redraw();
                let wake = now + std::time::Duration::from_millis(16);
                next_wake = Some(next_wake.map_or(wake, |t| t.min(wake)));
            }
        }

        // HoverCard / Tooltip openDelay / closeDelay（Solid 式 UI 线程定时器）。
        if crate::utils::hover_intent::tick_hover_intent() {
            if let Some(state) = (*self.state.borrow()).as_ref() {
                state.window.request_redraw();
            }
        }
        if let Some(wake) = crate::utils::hover_intent::hover_intent_next_wake_at() {
            if wake <= now {
                if let Some(state) = (*self.state.borrow()).as_ref() {
                    state.window.request_redraw();
                }
            } else {
                next_wake = Some(next_wake.map_or(wake, |t| t.min(wake)));
            }
        }
        if crate::utils::presence::tick_generic_presence() {
            if let Some(state) = (*self.state.borrow()).as_ref() {
                state.window.request_redraw();
            }
        }
        if let Some(wake) = crate::utils::presence::generic_presence_next_wake_at() {
            if wake <= now {
                if let Some(state) = (*self.state.borrow()).as_ref() {
                    state.window.request_redraw();
                }
            } else {
                next_wake = Some(next_wake.map_or(wake, |t| t.min(wake)));
            }
        }

        // Dialog/Sheet/Drawer: `request_redraw` inside RedrawRequested / about_to_wait
        // is coalesced on Windows. Opening arms a one-shot `Poll` so caret/long-press
        // `WaitUntil` (~500ms) cannot be the next wake. Later enter/exit frames must
        // keep Polling via `any_modal_css_needs_frame` — same latch as page CSS.
        // Hold-repeat typing uses the same Poll: otherwise caret WaitUntil lets the OS
        // queue many KeyDowns and they all land in one paint (batched glyphs).
        let text_edit_pump = super::input_controller::take_text_edit_force_immediate();
        crate::components::color_picker::drain_color_drag_apply();
        crate::components::drawer::drain_drawer_layout_bumps();
        let page_css_needs = crate::style::any_page_css_needs_frame();
        let modal_css_needs = crate::utils::modal_motion::any_modal_css_needs_frame();
        // Nested Select Expand (ColorPicker format menu) is not Dialog CSS. A one-shot
        // armed at floating Enter is consumed by the parent Fade; query live motion.
        let overlay_css_needs =
            modal_css_needs || crate::utils::floating_motion::any_floating_motion_needs_frame();
        let resize_followup = (*self.state.borrow())
            .as_ref()
            .is_some_and(|s| s.needs_resize_followup());
        let force_immediate = about_to_wait_force_immediate(
            crate::utils::modal_motion::take_modal_force_immediate_wake(),
            overlay_css_needs,
            page_css_needs,
            text_edit_pump,
            self.paint_clock_active,
            self.force_redraw_count > 0,
            resize_followup,
            self.scrollbar_hide_wake,
        );
        if force_immediate {
            if let Some(state) = (*self.state.borrow()).as_ref() {
                state.window.request_redraw();
            }
        }

        // Number input 增减触发器长按连发。
        if crate::components::input::tick_number_step_hold() {
            if let Some(state) = (*self.state.borrow()).as_ref() {
                state.window.request_redraw();
            }
        }
        if let Some(wake) = crate::components::input::number_step_hold_next_wake_at() {
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

        event_loop.set_control_flow(idle_control_flow(next_wake, force_immediate));
    }

    fn resumed(&mut self, _event_loop: &dyn ActiveEventLoop) {
        if let Some(state) = (*self.state.borrow()).as_ref() {
            state.window.request_redraw();
        }
        self.is_suspended = false;
    }

    fn suspended(&mut self, _event_loop: &dyn ActiveEventLoop) {
        self.is_suspended = true;
    }

    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, id: WindowId, event: WindowEvent) {
        #[cfg(feature = "a11y")]
        {
            if let Some(adapter) = &mut self.accesskit_adapter {
                if let Some(st) = (*self.state.borrow()).as_ref() {
                    if st.window.id() == id {
                        adapter.process_event(st.window.as_ref(), &event);
                    }
                }
            }
            if let Some(win) = self.secondaries.borrow().get(&id).map(|s| s.window.clone()) {
                if let Some(adapter) = self.secondary_accesskit.get_mut(&id) {
                    adapter.process_event(win.as_ref(), &event);
                }
            }
        }
        if let Some(win) = self.lifecycle_window(id) {
            super::window_lifecycle::emit_event(win.as_ref(), &event);
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
            WindowEvent::PointerMoved { position, source, .. } => {
                if matches!(PointerKind::from(source), PointerKind::Touch(_)) {
                    // Touch motion is handled with PointerButton / FingerId below.
                }
                if self.is_secondary_window(id) {
                    self.handle_secondary_cursor_moved(id, position, event_loop);
                } else {
                    self.with_primary_input(id, |app| {
                        app.handle_cursor_moved(id, position, event_loop)
                    });
                }
            }
            WindowEvent::PointerLeft { .. } => {
                if self.is_secondary_window(id) {
                    self.handle_secondary_cursor_left(id, event_loop);
                } else {
                    self.with_primary_input(id, |app| app.handle_cursor_left(id, event_loop));
                }
            }
            WindowEvent::PointerButton {
                state: element_state,
                button,
                position,
                ..
            } => {
                if let winit::event::ButtonSource::Touch { finger_id, .. } = &button {
                    match element_state {
                        ElementState::Pressed => {
                            self.active_touches.insert(*finger_id, position);
                            if self.primary_touch_id.is_none() {
                                self.primary_touch_id = Some(*finger_id);
                            }
                            if self.active_touches.len() >= 2 {
                                self.content_scroll_drag = None;
                                self.pinch_prev_dist = Self::touch_pinch_distance(&self.active_touches);
                            }
                            self.sync_pointer_action_touches();
                        }
                        ElementState::Released => {
                            self.active_touches.remove(finger_id);
                            if self.active_touches.len() < 2 {
                                self.pinch_prev_dist = None;
                            }
                            if self.primary_touch_id == Some(*finger_id) {
                                self.primary_touch_id = None;
                                if let Some((&nid, &npos)) = self.active_touches.iter().next() {
                                    self.primary_touch_id = Some(nid);
                                    self.cursor_pos = npos;
                                }
                            }
                            self.sync_pointer_action_touches();
                        }
                    }
                }
                let Some(button) = button.mouse_button() else {
                    if matches!(element_state, ElementState::Pressed | ElementState::Released) {
                        let mapped = MouseButton::Left;
                        if self.is_secondary_window(id) {
                            self.handle_secondary_mouse_button(id, element_state, mapped);
                        } else {
                            self.with_primary_input(id, |app| {
                                app.handle_mouse_button(id, element_state, mapped, Some(event_loop))
                            });
                        }
                    }
                    return;
                };
                if self.is_secondary_window(id) {
                    self.handle_secondary_mouse_button(id, element_state, button);
                } else {
                    self.with_primary_input(id, |app| {
                        app.handle_mouse_button(id, element_state, button, Some(event_loop))
                    });
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // Wheel only mutates scroll signals; CursorMoved already runs hover
                // walks on the PE stack. Do not maybe_grow 8 MiB every tick.
                if self.is_secondary_window(id) {
                    self.handle_secondary_mouse_wheel(id, delta);
                } else {
                    self.with_primary_input(id, |app| app.handle_mouse_wheel(id, delta));
                }
            }
            WindowEvent::DragDropped { .. } => {}
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
                            crate::platform::glass_on_focus(sec.window.as_ref(), true);
                            sec.os_focused = true;
                            sec.ime_os_blur_at = None;
                            sec.window.request_redraw();
                        }
                    } else if let Some(sec) = self.secondaries.borrow_mut().get_mut(&id) {
                        crate::platform::glass_on_focus(sec.window.as_ref(), false);
                        // Same TIP-candidate soft-blur as primary: do not drop IME immediately.
                        sec.os_focused = false;
                        let at = std::time::Instant::now() + std::time::Duration::from_millis(200);
                        sec.ime_os_blur_at = Some(at);
                        event_loop.set_control_flow(ControlFlow::WaitUntil(at));
                    }
                } else if focused {
                    self.bind_pointer_window(id);
                    super::window_scope::force_primary();
                    self.os_window_focused = true;
                    self.ime_os_blur_at = None;
                    if let Some(state) = (*self.state.borrow()).as_ref() {
                        crate::platform::glass_on_focus(state.window.as_ref(), true);
                    }
                } else {
                    self.os_window_focused = false;
                    if let Some(state) = (*self.state.borrow()).as_ref() {
                        crate::platform::glass_on_focus(state.window.as_ref(), false);
                    }
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
            WindowEvent::Occluded(occluded) => {
                let is_primary = (*self.state.borrow())
                    .as_ref()
                    .is_some_and(|state| state.window.id() == id);
                if occluded {
                    if is_primary {
                        // Minimize / covered: close floating before the next layout so it
                        // does not follow the shrinking / restoring window.
                        let _ = super::portal_overlay::dismiss_exclusive_on_viewport_change();
                    }
                } else if is_primary {
                    if let Some(state) = (*self.state.borrow_mut()).as_mut() {
                        state.window.set_visible(true);
                        // One redraw; avoid extra `configure` / forced multi-frame redraw (flicker).
                        state.window.request_redraw();
                    }
                }
            }
            WindowEvent::SurfaceResized(physical_size) => {
                let is_primary = (*self.state.borrow())
                    .as_ref()
                    .is_some_and(|state| state.window.id() == id);
                if is_primary {
                    // Edge-resize / maximize fire Resized before the next floating
                    // sync. Close first so the panel does not chase the trigger.
                    let _ = super::portal_overlay::dismiss_exclusive_on_viewport_change();
                }
                if physical_size.width > 0 && physical_size.height > 0 {
                    if let Some(sec) = self.secondaries.borrow_mut().get_mut(&id) {
                        sec.resize(physical_size);
                        sec.window.request_redraw();
                    } else if is_primary {
                        if let Some(state) = (*self.state.borrow_mut()).as_mut() {
                            state.note_winit_resized(physical_size);
                            state.resize(physical_size);
                            state.window.request_redraw();
                            self.force_redraw_count = self.force_redraw_count.max(1);
                        }
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
            WindowEvent::Moved(_) => {
                if let Some(sec) = self.secondaries.borrow().get(&id) {
                    crate::platform::glass_on_moved(sec.window.as_ref());
                } else if let Some(state) = self.state.borrow().as_ref() {
                    if state.window.id() == id {
                        crate::platform::glass_on_moved(state.window.as_ref());
                    }
                }
            }
            WindowEvent::ThemeChanged(theme) => {
                crate::platform::glass_on_os_theme(matches!(theme, winit::window::Theme::Dark));
            }
            WindowEvent::RedrawRequested => {
                // Layout / collect / GPU stay on the PE stack. User closures
                // grow at the callback (`fire_file_drop`, HMR `app_builder`,
                // `run_effect`) — wrapping the whole frame used to maybe_grow
                // 8 MiB on every redraw of a 1 MiB GUI thread.
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

impl Drop for super::App {
    fn drop(&mut self) {
        self.shutdown_renderer();
    }
}

pub(super) fn run<F: FnMut() -> Element + 'static>(
    window_options: super::WindowOptions,
    app_builder: F,
) {
    crate::logging::init();

    let window_options = crate::platform::apply_ohos_window_options(window_options);
    window_options.install_text_defaults();
    crate::init();

    if let Err(e) = crate::platform::ensure_winit_host() {
        log::error!("{e}");
        return;
    }
    let event_loop = crate::platform::build_event_loop();
    let event_loop_proxy = event_loop.create_proxy();
    let (user_tx, user_rx) = std::sync::mpsc::channel();
    super::ui_bridge::install_proxy(event_loop_proxy.clone(), user_tx);
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = super::App {
        state: std::rc::Rc::new(std::cell::RefCell::new(None)),
        is_suspended: false,
        force_redraw_count: 0,
        paint_clock_active: false,
        prev_scroll_paint_only: false,
        app_builder: std::rc::Rc::new(std::cell::RefCell::new(app_builder)),
        cursor_pos: winit::dpi::PhysicalPosition::new(0.0, 0.0),
        cursor_move_pending: false,
        cursor_inside_window: false,
        last_frame_time: std::time::Instant::now(),
        modifiers: ModifiersState::default(),
        left_button_down: false,
        mouse_buttons: 0,
        last_client_pos: None,
        last_mouse_target: None,
        last_mouse_path: None,
        click_press_path: None,
        click_press_scrolled: false,
        outside_dismiss_on_down: false,
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
        input_drag_multi: false,
        input_drag_multi_base: Vec::new(),
        label_drag_key: None,
        label_drag_anchor: 0,
        scroll_accum: std::collections::HashMap::new(),
        wheel_sticky_rest: None,
        wheel_sticky_cursor: [0.0, 0.0],
        hover_scroll_rest: None,
        hover_scroll_axis: None,
        scrollbar_visible_until: std::collections::HashMap::new(),
        scrollbar_hide_wake: false,
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
        user_events: user_rx,
        #[cfg(feature = "a11y")]
        accesskit_adapter: None,
        #[cfg(feature = "a11y")]
        secondary_accesskit: std::collections::HashMap::new(),
        ime_owner_window: None,
        custom_cursors: crate::style::CustomCursorCache::new(),
        window_options,
        #[cfg(debug_assertions)]
        hot_reload: super::hot_reload::HotReloadState::new(),
    };
    #[cfg(not(any(target_arch = "wasm32", target_env = "ohos")))]
    {
        crate::platform::run_winit_app(event_loop, app);
    }
    #[cfg(target_env = "ohos")]
    {
        crate::platform::run_winit_app(event_loop, app);
    }
    #[cfg(target_arch = "wasm32")]
    {
        crate::platform::spawn_winit_app(event_loop, app);
    }
}

#[cfg(test)]
mod idle_control_flow_tests {
    use super::{about_to_wait_force_immediate, idle_control_flow, scroll_settle_must_pump};
    use std::time::{Duration, Instant};
    use winit::event_loop::ControlFlow;

    #[test]
    fn page_css_does_not_sleep_on_scrollbar_wait_until() {
        let scrollbar_hide = Instant::now() + Duration::from_millis(1500);
        assert_eq!(
            idle_control_flow(Some(scrollbar_hide), true),
            ControlFlow::Poll,
            "spin / shimmer after scroll stop must Poll, not WaitUntil scrollbar hide"
        );
    }

    #[test]
    fn resize_settle_does_not_sleep_after_maximize() {
        let scrollbar_hide = Instant::now() + Duration::from_millis(1500);
        assert_eq!(
            idle_control_flow(Some(scrollbar_hide), true),
            ControlFlow::Poll,
            "needs_text_settle / force_redraw_count after maximize must Poll; coalesced request_redraw cannot be the only wake"
        );
        assert_eq!(
            idle_control_flow(None, true),
            ControlFlow::Poll,
            "maximize without a wheel still Polls — no scrollbar deadline to hide behind"
        );
    }

    #[test]
    fn scroll_settle_force_redraw_must_poll_not_caret_wait_until() {
        assert!(
            scroll_settle_must_pump(true, false),
            "wheel stop is the settle frame"
        );
        assert!(!scroll_settle_must_pump(true, true));
        assert!(!scroll_settle_must_pump(false, false));
        assert!(!scroll_settle_must_pump(false, true));
        // Gallery forms card 2: live scroll stays fluid (paint_clock Poll). The first
        // idle frame used to plant caret WaitUntil; Windows coalesced later clicks
        // and froze focus chrome / caret blink / overlay scrollbar.
        let caret = Instant::now() + Duration::from_millis(520);
        let force =
            about_to_wait_force_immediate(false, false, false, false, false, true, false, false);
        assert!(force, "scroll-settle force_redraw_count must Poll");
        assert_eq!(
            idle_control_flow(Some(caret), force),
            ControlFlow::Poll,
            "first idle frame after scroll must not WaitUntil caret blink"
        );
    }

    #[test]
    fn coalesced_cursor_move_must_poll_not_caret_wait_until() {
        // `handle_cursor_moved` defers hit-test to present and arms `force_redraw_count`.
        // WaitUntil caret would drop that redraw, so the MouseEvent log never flushes.
        let caret = Instant::now() + Duration::from_millis(520);
        let force =
            about_to_wait_force_immediate(false, false, false, false, false, true, false, false);
        assert!(
            force,
            "CursorMoved pending must arm force_redraw_count / Poll"
        );
        assert_eq!(
            idle_control_flow(Some(caret), force),
            ControlFlow::Poll,
            "gallery MouseEvent mousemove flush cannot WaitUntil caret blink"
        );
    }

    #[test]
    fn paint_only_focus_chrome_does_not_sleep_on_caret_wait_until() {
        // Clicking Search after a sliver-clip: `force_redraw_count` is the existing
        // Windows coalescing pump (same as maximize). Caret WaitUntil on the previous
        // field must not win.
        let caret = Instant::now() + Duration::from_millis(520);
        let force =
            about_to_wait_force_immediate(false, false, false, false, false, true, false, false);
        assert!(force, "text-edit click arming force_redraw_count must Poll");
        assert_eq!(
            idle_control_flow(Some(caret), force),
            ControlFlow::Poll,
            "paint-only focus_id.set cannot WaitUntil the previous field's caret blink"
        );
    }

    #[test]
    fn ime_jumped_stale_caret_wake_must_poll_via_text_edit_pump() {
        // FALLBACK contract: if dropping WaitUntil is not enough, re-enable
        // `arm_text_edit_force_immediate` in about_to_wait. This latch must Poll.
        let caret = Instant::now() + Duration::from_millis(520);
        let force =
            about_to_wait_force_immediate(false, false, false, true, false, false, false, false);
        assert!(
            force,
            "caret_wake_stale arm_text_edit_force_immediate must Poll"
        );
        assert_eq!(
            idle_control_flow(Some(caret), force),
            ControlFlow::Poll,
            "IME-jumped chrome frame cannot WaitUntil the Placeholder caret blink"
        );
    }

    #[test]
    fn stale_caret_wake_idles_with_wait_not_previous_blink() {
        // Root: about_to_wait clears caret_wake_at. Idle is Wait so the click's
        // request_redraw stays queued — not WaitUntil the previous field's blink.
        assert_eq!(
            idle_control_flow(None, false),
            ControlFlow::Wait,
            "cleared caret deadline must not plant WaitUntil"
        );
        let caret = Instant::now() + Duration::from_millis(520);
        assert_eq!(
            idle_control_flow(Some(caret), false),
            ControlFlow::WaitUntil(caret),
            "a live matching caret may still WaitUntil blink"
        );
    }

    #[test]
    fn modal_open_does_not_sleep_on_caret_wait_until() {
        let caret = Instant::now() + Duration::from_millis(520);
        assert_eq!(
            idle_control_flow(Some(caret), true),
            ControlFlow::Poll,
            "first Dialog open must Poll, not WaitUntil caret ~520ms"
        );
        assert_eq!(
            idle_control_flow(Some(caret), false),
            ControlFlow::WaitUntil(caret)
        );
        assert_eq!(idle_control_flow(None, false), ControlFlow::Wait);
        assert_eq!(idle_control_flow(None, true), ControlFlow::Poll);
    }

    #[test]
    fn modal_css_pump_polls_after_one_shot_consumed() {
        // One-shot Poll is taken on the first about_to_wait. Mid enter/exit still
        // needs Poll or caret WaitUntil (~520ms) freezes the CSS clock.
        let caret = Instant::now() + Duration::from_millis(520);
        let force =
            about_to_wait_force_immediate(false, true, false, false, false, false, false, false);
        assert!(
            force,
            "any_modal_css_needs_frame must latch Poll after take_modal_force_immediate_wake"
        );
        assert_eq!(
            idle_control_flow(Some(caret), force),
            ControlFlow::Poll,
            "mid Dialog/Sheet CSS must Poll; request_redraw + WaitUntil caret is the hitch"
        );
        assert!(!about_to_wait_force_immediate(
            false, false, false, false, false, false, false, false,
        ));
    }

    #[test]
    fn nested_select_expand_polls_instead_of_caret_wait_until() {
        // ColorPicker hex Input plants caret WaitUntil (~520ms). Nested format Select
        // Expand is not Dialog CSS; about_to_wait must still Poll on live floating
        // motion — a one-shot armed at parent Fade Enter is already gone.
        use crate::core::reactivity::create_root;
        use crate::utils::floating_motion::{
            any_floating_motion_needs_frame, sync_floating_presence, use_floating_presence,
            FloatingMotionKind,
        };

        create_root(|| {
            let fade = use_floating_presence("cp-fade", FloatingMotionKind::Fade);
            sync_floating_presence(&fade, true);
            let nested = use_floating_presence("cp-fmt", FloatingMotionKind::Expand);
            sync_floating_presence(&nested, true);
            assert!(
                any_floating_motion_needs_frame(),
                "nested Expand open must be visible before the first tick"
            );
            let caret = Instant::now() + Duration::from_millis(520);
            let force = about_to_wait_force_immediate(
                false,
                any_floating_motion_needs_frame(),
                false,
                false,
                false,
                false,
                false,
                false,
            );
            assert!(force, "floating Expand busy must Poll like modal CSS");
            assert_eq!(
                idle_control_flow(Some(caret), force),
                ControlFlow::Poll,
                "ColorPicker nested Select cannot WaitUntil the hex caret blink"
            );
        });
    }

    #[test]
    fn scrollbar_hide_expiry_polls_like_dialog() {
        assert_eq!(
            idle_control_flow(None, true),
            ControlFlow::Poll,
            "prune-expired auto-hide must Poll; Wait coalesces the fade-start redraw on Windows"
        );
        let leftover = Instant::now() + Duration::from_millis(400);
        assert_eq!(
            idle_control_flow(Some(leftover), true),
            ControlFlow::Poll,
            "a remaining axis deadline must not win WaitUntil over the expired-axis wake"
        );
    }

    #[test]
    fn scrollbar_hide_wake_survives_second_about_to_wait_before_redraw() {
        // Windows: WaitUntil expiry → about_to_wait (prune) → about_to_wait again
        // → RedrawRequested. The latch is App.scrollbar_hide_wake; it is not
        // consumed by about_to_wait (unlike take_modal_force_immediate_wake).
        let mut hide_wake = false;
        let prune_first = true;
        if prune_first {
            hide_wake = true;
        }
        assert_eq!(
            idle_control_flow(None, hide_wake),
            ControlFlow::Poll,
            "prune must Poll so the fade-start request_redraw is not coalesced"
        );
        let prune_second = false;
        if prune_second {
            hide_wake = true;
        }
        assert!(
            hide_wake,
            "a local prune flag would be false on the second wait; latch must hold"
        );
        assert_eq!(
            idle_control_flow(None, hide_wake),
            ControlFlow::Poll,
            "second about_to_wait before RedrawRequested must still Poll"
        );
        hide_wake = false;
        assert_eq!(
            idle_control_flow(None, hide_wake),
            ControlFlow::Wait,
            "RedrawRequested consumes the latch; later idle may Wait"
        );
    }
}
