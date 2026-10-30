use std::sync::Arc;
use std::time::Instant;

#[cfg(feature = "a11y")]
use crate::a11y::winit_adapter::Adapter;
#[cfg(feature = "a11y")]
use accesskit::{Node, NodeId, Role, Tree, TreeId, TreeUpdate};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, PointerKind, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow},
    keyboard::ModifiersState,
    window::{Window, WindowId},
};

use crate::core::tree::Element;

pub(super) struct DetachedGpu {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    config: wgpu::SurfaceConfiguration,
    _font_system: Option<Box<glyphon::FontSystem>>,
}

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

    pub(super) fn shutdown_renderer(&mut self) {
        // Idempotent: CloseRequested / Destroyed / exiting / post-run_app may all call us.
        if self.state.borrow().is_none()
            && self.secondaries.borrow().is_empty()
            && self.detached_gpu.is_none()
        {
            return;
        }
        self.persist_window_state();
        if let Some(state) = self.state.borrow().as_ref() {
            state.window.set_visible(false);
        }
        super::tray::dispose_trays_for_shutdown();
        super::native_menu::dispose_all();

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
        super::window_session::drain_tasks();
        self.window_registry = Default::default();

        let state = self.state.borrow_mut().take();
        self.primary_owner.take();
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
        let window = state.as_ref().map(|state| state.window.clone());
        drop(state);
        crate::authoring::window::forget(super::window_scope::WindowScope::Primary);
        self.detached_gpu.take();
        self.instance_guard.take();
        if let Some(window) = window {
            crate::platform::glass_detach(window.as_ref());
        }
        self.pointer_window = None;
    }

    fn drop_secondary(&mut self, id: WindowId) {
        super::frame_budget::forget_window(id);
        super::native_menu::remove_window(id);
        self.frame_pacers.remove(&id);
        self.window_modifiers.remove(&id);
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
        super::runtime::unregister_window(id);
    }

    fn close_targets(&self, id: Option<WindowId>) -> Vec<WindowId> {
        let primary = self.window_registry.primary_id();
        if id.is_none() || id == primary {
            super::windows()
                .into_iter()
                .filter(|window| {
                    id.is_none()
                        || Some(window.id()) == primary
                        || window.closes_with_primary().unwrap_or(true)
                })
                .map(|window| window.id())
                .collect()
        } else {
            id.into_iter().collect()
        }
    }

    fn finish_close(&mut self, event_loop: &dyn ActiveEventLoop, closing: Vec<WindowId>) {
        for id in &closing {
            if let Some(window) = self.lifecycle_window(*id) {
                window.set_visible(false);
            }
        }
        if closing.len() == self.window_registry.len() {
            self.shutdown_renderer();
            event_loop.exit();
            return;
        }
        let primary = self.window_registry.primary_id();
        for &id in &closing {
            if Some(id) != primary {
                self.drop_secondary(id);
                if let Some(slot) = self.window_registry.remove_secondary(id) {
                    slot.window.set_visible(false);
                }
            }
        }
        if primary.is_some_and(|id| closing.contains(&id)) {
            let _scope = super::window_scope::enter_primary();
            let state = self.state.borrow_mut().take();
            self.primary_owner.take();
            if let Some(state) = state {
                let window = state.window.clone();
                super::window_lifecycle::emit_closing(window.as_ref());
                window.set_visible(false);
                let id = window.id();
                super::native_menu::remove_window(id);
                super::frame_budget::forget_window(id);
                self.frame_pacers.remove(&id);
                self.window_modifiers.remove(&id);
                super::runtime::unregister_window(id);
                self.window_registry.remove_primary();
                let mut scroll_rests = std::collections::HashSet::new();
                collect_scroll_rests(&state.root_element, &mut scroll_rests);
                for rest in scroll_rests {
                    crate::core::scroll_viewport::remove_scroll_viewport_signals(&rest);
                    super::scroll_support::cancel_scroll_item_into_view(&rest);
                }
                #[cfg(feature = "a11y")]
                {
                    self.accesskit_adapter = None;
                }
                self.detached_gpu = Some(DetachedGpu {
                    instance: state.instance,
                    adapter: state.adapter,
                    device: state.device,
                    queue: state.queue,
                    config: state.config,
                    _font_system: state.text_renderer.into_owned_font_system(),
                });
                super::portal_overlay::drop_overlay_stack(
                    super::window_scope::WindowScope::Primary,
                );
                crate::platform::glass_detach(window.as_ref());
                self.pointer_window = None;
                self.ime_owner_window = None;
                self.paint_clock_active = false;
                self.force_redraw_count = 0;
            }
        }
    }

    fn request_close(&mut self, event_loop: &dyn ActiveEventLoop, id: Option<WindowId>) {
        if id.is_some_and(|id| !self.window_registry.contains(id)) {
            return;
        }
        let primary = self.window_registry.primary_id();
        let closing = self.close_targets(id);
        for &wid in &closing {
            let Some(window) = self.lifecycle_window(wid) else {
                continue;
            };
            let _scope = if Some(wid) == primary {
                super::window_scope::enter_primary()
            } else {
                super::window_scope::enter_secondary_id(wid)
            };
            if !super::window_lifecycle::close_requested(window.as_ref()) {
                return;
            }
        }
        self.finish_close(event_loop, closing);
    }

    fn create_secondary_state(
        &self,
        window: Arc<dyn Window>,
        root: Element,
        options: super::WindowOptions,
    ) -> Result<super::secondary_window::SecondaryWindowState, String> {
        let state = self.state.borrow();
        if let Some(primary) = state.as_ref() {
            super::secondary_window::SecondaryWindowState::new(
                window,
                primary.device.clone(),
                primary.queue.clone(),
                &primary.instance,
                &primary.adapter,
                &primary.config,
                root,
                options,
            )
        } else if let Some(gpu) = &self.detached_gpu {
            super::secondary_window::SecondaryWindowState::new(
                window,
                gpu.device.clone(),
                gpu.queue.clone(),
                &gpu.instance,
                &gpu.adapter,
                &gpu.config,
                root,
                options,
            )
        } else {
            Err("application GPU is not ready".into())
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

    fn sync_pointer_action_touches(&self) {
        let scale = (*self.state.borrow())
            .as_ref()
            .map(|s| s.window.scale_factor() as f32)
            .unwrap_or(1.0)
            .max(1.0);
        let pts: Vec<(u64, [f32; 2])> = self
            .active_touches
            .iter()
            .map(|(&id, p)| {
                (
                    id.into_raw() as u64,
                    [p.x as f32 / scale, p.y as f32 / scale],
                )
            })
            .collect();
        for &(id, pos) in &pts {
            if Some(id) != self.primary_touch_id.map(|f| f.into_raw() as u64) {
                crate::utils::drag::pointer::tick_pointer(id, pos[0], pos[1]);
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
        // Drag activation can precede the long-press movement threshold. A slow
        // live drag must not synthesize a context click at the press position.
        if crate::utils::drag::runtime::is_active() {
            self.long_press = None;
            return;
        }
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
    pub(super) fn handle_user_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        event: super::JadixUserEvent,
    ) {
        match event {
            #[cfg(all(debug_assertions, feature = "hot-reload"))]
            super::JadixUserEvent::HotReloadWake => {
                if let Some(state) = (*self.state.borrow_mut()).as_mut() {
                    state.window.request_redraw();
                    self.force_redraw_count = self.force_redraw_count.max(2);
                }
            }
            super::JadixUserEvent::OpenWindow { options, builder } => {
                let options = *options;
                if !crate::platform::host_capabilities().multi_window {
                    log::warn!(
                        "multi-window: secondary windows are not supported on {}",
                        crate::platform::host_kind()
                    );
                    return;
                }
                let wants_glass = options.compositor_alpha();
                let mut composition = options.wants_dcomp_swapchain();
                let Ok(created) =
                    event_loop.create_window(super::window_lifecycle::prepare_attributes(
                        options.window_attributes(composition),
                        options.persistence_label(false),
                    ))
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
                super::window_lifecycle::emit_created(
                    window.as_ref(),
                    options.persistence_label(false),
                );
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
                            let Ok(created) = event_loop.create_window(
                                super::window_lifecycle::prepare_attributes(
                                    options.window_attributes(false),
                                    options.persistence_label(false),
                                ),
                            ) else {
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
                            super::window_chrome::apply_created_window(
                                window.as_ref(),
                                &options,
                                false,
                            );
                            super::window_lifecycle::emit_created(
                                window.as_ref(),
                                options.persistence_label(false),
                            );
                        }
                        match self.create_secondary_state(
                            window.clone(),
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
                    match self.create_secondary_state(window.clone(), placeholder, options.clone())
                    {
                        Ok(s) => s,
                        Err(e) => {
                            log::warn!("multi-window: secondary init failed: {e}");
                            return;
                        }
                    }
                };

                let id = window.id();
                self.window_registry.insert_secondary_slot(window.clone());
                super::runtime::register_window(&window, false);
                if let Some(handle) = super::runtime::window_handle(window.id()) {
                    if let Err(e) = handle.set_resize_border(options.resize_border) {
                        log::warn!("resize_border: {e}");
                    }
                }
                super::entry::seed_window_config(id, &options);
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
                        super::invalidation::register_root(
                            crate::core::reactivity::current_effect_id().unwrap(),
                        );
                        crate::style::track_style_changes();
                        let new_root = (builder_rc.borrow_mut())();
                        let new_root = crate::style::apply_stylesheet_rules_tree(new_root);
                        if let Some(sec) = secondaries.borrow_mut().get_mut(&id) {
                            sec.replace_root(new_root);
                            if !super::invalidation::in_prepass() {
                                sec.window.request_redraw();
                            }
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
            super::JadixUserEvent::CloseWindow { id } => {
                self.request_close(event_loop, Some(id));
            }
            super::JadixUserEvent::Exit => {
                self.request_close(event_loop, None);
            }
            super::JadixUserEvent::SecondInstance(launch) => {
                if let Some(window) = super::windows().first() {
                    let _scope = if self.window_registry.primary_id() == Some(window.id()) {
                        super::window_scope::enter_primary()
                    } else {
                        super::window_scope::enter_secondary_id(window.id())
                    };
                    let _ = window.focus();
                    super::instance::dispatch(launch);
                } else {
                    super::instance::dispatch(launch);
                }
            }
            #[cfg(feature = "a11y")]
            super::JadixUserEvent::AccessKit(ev) => {
                use crate::a11y::winit_adapter::AdapterWindowEvent as AkWindowEvent;
                let wid = ev.window_id;
                if !ev.is_current() || self.lifecycle_window(wid).is_none() {
                    return;
                }
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
                                        "__jadix_button_focus_id",
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
        if self.state.borrow().is_none() && self.detached_gpu.is_none() {
            super::window_chrome::update_system_theme(event_loop.system_theme());
            let startup = std::time::Instant::now();
            let attrs = super::window_lifecycle::prepare_attributes(
                self.window_options.to_window_attributes(),
                self.window_options.persistence_label(true),
            );
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
            super::runtime::register_window(&window, true);
            if let Some(handle) = super::runtime::window_handle(window.id()) {
                if let Err(e) = handle.set_resize_border(self.window_options.resize_border) {
                    log::warn!("resize_border: {e}");
                }
            }
            super::entry::seed_window_config(window.id(), &self.window_options);

            let state_ref = self.state.clone();
            crate::app::runtime::set_app_runtime(crate::app::runtime::AppRuntime::new(
                std::rc::Rc::downgrade(&state_ref),
                std::rc::Rc::downgrade(&self.secondaries),
            ));
            crate::app::reactive::reset_registered_anchors();

            log::debug!("lifecycle: before root create_effect");
            #[cfg(all(debug_assertions, feature = "devtools"))]
            if self.window_options.devtools {
                crate::app::devtools::DevToolsState::enable_for_window();
            }
            let builder_ref = self.app_builder.clone();
            let (owner, _) = crate::core::reactivity::create_root(|| {
                crate::core::reactivity::mark_next_effect_as_ui_root();
                crate::core::reactivity::create_effect(move || {
                    if state_ref.borrow().is_none() {
                        return;
                    }
                    super::invalidation::register_root(
                        crate::core::reactivity::current_effect_id().unwrap(),
                    );
                    log::debug!("root_effect: before app_builder");
                    crate::style::track_style_changes();
                    let new_root = super::prepare_primary_ui((builder_ref.borrow_mut())());
                    log::debug!(
                        "root_effect: after app_builder children={}",
                        new_root.children.len()
                    );
                    if let Some(st) = (*state_ref.borrow_mut()).as_mut() {
                        log::debug!("root_effect: before rebuild_ui");
                        st.rebuild_ui(new_root);
                        log::debug!("root_effect: after rebuild_ui");
                        if !super::invalidation::in_prepass() {
                            st.window.request_redraw();
                        }
                    }
                    log::debug!("root_effect: body done (flush_deferred next)");
                });
            });
            self.primary_owner = Some(owner);
            log::debug!("lifecycle: after root create_effect");
            log::info!(
                "startup primary_ui_mounted_ms={:.1}",
                startup.elapsed().as_secs_f64() * 1000.0
            );

            // Prime the real UI while hidden so ShowWindow never exposes the
            // unpainted native client area. Some surfaces require a visible
            // window: the normal redraw below remains their retry path.
            #[cfg(target_os = "windows")]
            self.handle_redraw_requested(window.id(), event_loop);
            window.set_visible(true);
            window.focus_window();
            window.request_redraw();
            crate::platform::glass_on_shown(window.as_ref());
            let composition = self
                .state
                .borrow()
                .as_ref()
                .is_some_and(|s| s.composition_swapchain);
            super::window_chrome::apply_created_window(
                window.as_ref(),
                &self.window_options,
                composition,
            );
        }
    }
}

impl ApplicationHandler for super::App {
    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        let _work = super::frame_budget::begin_work(super::frame_budget::WorkKind::Shared);
        // A producer must not keep us in try_recv forever, starving OS input.
        const EVENT_BATCH: usize = 64;
        let mut processed = 0;
        for _ in 0..EVENT_BATCH {
            let Ok(event) = self.user_events.try_recv() else {
                break;
            };
            self.handle_user_event(event_loop, event);
            processed += 1;
            if event_loop.exiting() {
                return;
            }
        }
        if super::ui_bridge::drain_ui_tasks() != 0 {
            crate::app::runtime::request_redraw_all();
        }
        // wake_up may coalesce. Re-arm even if exactly EVENT_BATCH were queued;
        // the occasional empty turn costs less than losing a trailing event.
        // The UI queue independently re-arms itself while callbacks remain.
        if processed == EVENT_BATCH {
            self.event_loop_proxy.wake_up();
        }
    }

    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.create_primary_if_needed(event_loop);
        #[cfg(all(feature = "a11y", target_env = "ohos"))]
        if self.accesskit_adapter.is_none() {
            if let Some(state) = self.state.borrow().as_ref() {
                self.accesskit_adapter = Some(create_accesskit_adapter(
                    event_loop,
                    state.window.as_ref(),
                    self.event_loop_proxy.clone(),
                ));
                state.window.request_redraw();
            }
        }
        #[cfg(all(feature = "a11y", target_env = "ohos"))]
        for (id, secondary) in self.secondaries.borrow().iter() {
            self.secondary_accesskit.entry(*id).or_insert_with(|| {
                create_accesskit_adapter(
                    event_loop,
                    secondary.window.as_ref(),
                    self.event_loop_proxy.clone(),
                )
            });
        }
    }

    #[cfg(all(feature = "a11y", target_env = "ohos"))]
    fn destroy_surfaces(&mut self, _event_loop: &dyn ActiveEventLoop) {
        self.accesskit_adapter = None;
        self.secondary_accesskit.clear();
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        let _window = super::window_scope::enter_primary();
        let _work = super::frame_budget::begin_work(super::frame_budget::WorkKind::Shared);
        if crate::style::take_system_theme_refresh() {
            super::window_chrome::update_system_theme(event_loop.system_theme());
        }
        #[cfg(debug_assertions)]
        if self.hot_reload.has_pending() {
            if let Some(state) = (*self.state.borrow()).as_ref() {
                state.window.request_redraw();
            }
        }

        let now = std::time::Instant::now();
        let mut next_wake: Option<std::time::Instant> = None;
        #[cfg(all(debug_assertions, feature = "devtools"))]
        if let Some(deadline) = super::devtools::DevToolsState::pick_highlight_deadline() {
            if deadline <= now {
                if super::devtools::DevToolsState::expire_pick_highlight(now) {
                    if let Some(state) = self.state.borrow().as_ref() {
                        state.window.request_redraw();
                    }
                }
            } else {
                next_wake = Some(deadline);
            }
        }
        #[cfg(all(feature = "webview", not(windows)))]
        if let Some(state) = self.state.borrow_mut().as_mut() {
            if state.webviews.pump(&state.window) {
                next_wake = Some(now + std::time::Duration::from_millis(16));
            }
        }
        // Follow-up requests made during WM_PAINT use the paced animation wake.
        if super::invalidation::pending(super::window_scope::WindowScope::Primary) {
            if let Some(state) = self.state.borrow().as_ref() {
                self.frame_pacers
                    .entry(state.window.id())
                    .or_default()
                    .pending = true;
            }
        }
        for (&id, secondary) in self.secondaries.borrow().iter() {
            let scope = super::window_scope::WindowScope::Secondary(
                super::window_scope::token_for_secondary(id),
            );
            if super::invalidation::pending(scope) || secondary.scroll_animation_active() {
                self.frame_pacers.entry(id).or_default().pending = true;
            }
        }

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
                let fid =
                    crate::core::reactivity::use_signal("__jadix_button_focus_id", String::new)
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

        let can_present = |window: &dyn Window| {
            !self.is_suspended
                && window.is_visible() != Some(false)
                && window.is_minimized() != Some(true)
                && window.surface_size().width > 0
                && window.surface_size().height > 0
        };
        let mut deadline_work = Vec::new();
        if let Some(state) = self.state.borrow_mut().as_mut() {
            let pending = state
                .layout_tree
                .deadlines
                .prepare(can_present(state.window.as_ref()));
            deadline_work.push((state.window.clone(), pending));
        }
        for state in self.secondaries.borrow_mut().values_mut() {
            let pending = state
                .layout_tree
                .deadlines
                .prepare(can_present(state.window.as_ref()));
            deadline_work.push((state.window.clone(), pending));
        }
        for (window, pending) in deadline_work {
            for task in pending {
                let poll = task.dispatch(now);
                if poll.redraw || poll.next_wake.is_some_and(|wake| wake <= now) {
                    window.request_redraw();
                }
                if let Some(wake) = poll.next_wake.filter(|wake| *wake > now) {
                    next_wake = Some(next_wake.map_or(wake, |at| at.min(wake)));
                }
            }
        }

        let mut drag_work = Vec::new();
        let mut sample_drag = |root: &Element, window: &Arc<dyn Window>| {
            if !can_present(window.as_ref()) {
                return;
            }
            let state = crate::utils::drag::runtime::state();
            let size = window.surface_size();
            let scale = (window.scale_factor() as f32).max(1.0);
            let viewport = [size.width as f32 / scale, size.height as f32 / scale];
            if let Some(wake) = state.activation_deadline {
                if wake <= now {
                    if let Some(pointer) = state.pointer {
                        drag_work.push((
                            window.clone(),
                            crate::utils::drag::runtime::prepare_move(
                                Some(root),
                                pointer,
                                viewport,
                                crate::utils::drag::runtime::MoveMode::Reconcile,
                            ),
                        ));
                    }
                } else {
                    next_wake = Some(next_wake.map_or(wake, |at| at.min(wake)));
                }
            }
            if state.active {
                drag_work.push((
                    window.clone(),
                    crate::utils::drag::runtime::prepare_autoscroll(root, viewport),
                ));
            }
            if state.coasting {
                drag_work.push((
                    window.clone(),
                    crate::utils::drag::runtime::prepare_inertia(root, viewport),
                ));
            }
            if state.active || state.coasting {
                let wake = now + std::time::Duration::from_millis(16);
                next_wake = Some(next_wake.map_or(wake, |at| at.min(wake)));
            }
        };
        {
            let _scope = super::window_scope::enter_primary();
            if let Some(state) = self.state.borrow().as_ref() {
                sample_drag(&state.root_element, &state.window);
            }
        }
        for (id, state) in self.secondaries.borrow().iter() {
            let _scope = super::window_scope::enter_secondary_id(*id);
            sample_drag(&state.root_element, &state.window);
        }
        for (window, work) in drag_work {
            if work.dispatch() {
                window.request_redraw();
            }
        }
        {
            let _scope = super::window_scope::enter_primary();
            if crate::utils::pointer_action::edge_scroll_active() {
                let scrolled = self.state.borrow().as_ref().is_some_and(|state| {
                    crate::utils::pointer_action::pump_edge_scroll(&state.root_element)
                });
                if scrolled {
                    if let Some(state) = self.state.borrow().as_ref() {
                        state.window.request_redraw();
                    }
                }
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
        {
            let _scope = super::window_scope::enter_primary();
            crate::utils::drag::routing::drain_frame();
        }
        let secondary_ids: Vec<_> = self.secondaries.borrow().keys().copied().collect();
        for id in secondary_ids {
            let _scope = super::window_scope::enter_secondary_id(id);
            crate::utils::drag::routing::drain_frame();
        }
        // Theme presentation is a window-level animation, independent of element CSS clocks.
        let can_animate = |window: &dyn winit::window::Window| {
            !self.is_suspended
                && window.is_visible() != Some(false)
                && window.is_minimized() != Some(true)
                && window.surface_size().width > 0
                && window.surface_size().height > 0
        };
        let window_animation_needs_frame = self.state.borrow().as_ref().is_some_and(|s| {
            can_animate(s.window.as_ref())
                && (s.theme_transition.needs_frame() || s.shader_frame_active)
        }) || self
            .secondaries
            .borrow()
            .values()
            .any(|s| can_animate(s.window.as_ref()) && s.theme_transition.needs_frame());
        let page_css_needs =
            crate::style::any_page_css_needs_frame() || window_animation_needs_frame;
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
                self.frame_pacers
                    .entry(state.window.id())
                    .or_default()
                    .pending = true;
            }
        }

        // Number input 增减触发器长按连发。
        if crate::utils::text_edit::field::tick_number_step_hold() {
            if let Some(state) = (*self.state.borrow()).as_ref() {
                state.window.request_redraw();
            }
        }
        if let Some(wake) = crate::utils::text_edit::field::number_step_hold_next_wake_at() {
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

        let interval = super::frame_pacing::frame_interval();
        let mut frame_due = false;
        for (&id, pacer) in &self.frame_pacers {
            if !pacer.pending {
                continue;
            }
            let deadline = pacer.deadline(now, interval);
            if deadline <= now {
                if let Some(window) = self.lifecycle_window(id) {
                    window.request_redraw();
                    // Windows may coalesce WM_PAINT with WaitUntil: Poll only
                    // when a frame is due, then return to sleeping after paint.
                    frame_due = true;
                }
            } else {
                next_wake = Some(next_wake.map_or(deadline, |wake| wake.min(deadline)));
            }
        }
        event_loop.set_control_flow(idle_control_flow(next_wake, frame_due));
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
        if !self.window_registry.contains(id) {
            return;
        }
        if crate::style::take_system_theme_refresh() {
            super::window_chrome::update_system_theme(event_loop.system_theme());
        }
        let _frame = super::frame_budget::begin_event(id, &event);
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
        if matches!(event, WindowEvent::RedrawRequested)
            && !self
                .frame_pacers
                .entry(id)
                .or_default()
                .begin_frame(Instant::now(), super::frame_pacing::frame_interval())
        {
            return;
        }
        if let Some(win) = self.lifecycle_window(id) {
            super::window_lifecycle::emit_event(win.as_ref(), &event);
        }
        if self.handle_native_resize_border(id, &event, event_loop) {
            return;
        }
        if let WindowEvent::KeyboardInput {
            event,
            is_synthetic: false,
            ..
        } = &event
        {
            if self.handle_fullscreen_shortcut(id, event) {
                return;
            }
        }
        #[cfg(feature = "webview")]
        {
            let mut state = self.state.borrow_mut();
            if let Some(state) = state.as_mut().filter(|state| state.window.id() == id) {
                let _scope = super::window_scope::enter_primary();
                if state.webviews.handle_event(&event) {
                    return;
                }
            }
        }
        match event {
            WindowEvent::CloseRequested => {
                if self.pointer_window == Some(id) {
                    self.cancel_pointer_session();
                }
                self.request_close(event_loop, Some(id));
            }
            WindowEvent::Destroyed => {
                if self.pointer_window == Some(id) {
                    self.cancel_pointer_session();
                }
                self.finish_close(event_loop, self.close_targets(Some(id)));
            }
            WindowEvent::PointerMoved {
                position, source, ..
            } => {
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
                if self.pointer_window != Some(id) {
                    return;
                }
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
                    if self.primary_touch_id.is_none() || self.primary_touch_id == Some(*finger_id) {
                        if self.is_secondary_window(id) {
                            self.handle_secondary_cursor_moved(id, position, event_loop);
                        } else {
                            self.with_primary_input(id, |app| {
                                app.handle_cursor_moved(id, position, event_loop)
                            });
                        }
                    }
                    match element_state {
                        ElementState::Pressed => {
                            self.active_touches.insert(*finger_id, position);
                            if self.primary_touch_id.is_none() {
                                self.primary_touch_id = Some(*finger_id);
                            }
                            if self.active_touches.len() >= 2 {
                                self.content_scroll_drag = None;
                                self.pinch_prev_dist =
                                    Self::touch_pinch_distance(&self.active_touches);
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
                    if matches!(
                        element_state,
                        ElementState::Pressed | ElementState::Released
                    ) {
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
                self.window_modifiers.insert(id, modifiers.state());
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
                if !focused && self.pointer_window == Some(id) {
                    self.cancel_pointer_session();
                }
                if !focused {
                    self.window_modifiers.remove(&id);
                    self.modifiers = ModifiersState::default();
                }
                if focused {
                    super::native_menu::focus_window(id);
                }
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
                #[cfg(all(debug_assertions, feature = "devtools"))]
                super::devtools::detached_window_resized(id);
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
                super::window_chrome::update_system_theme(Some(theme));
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

    let (user_tx, user_rx) = std::sync::mpsc::channel();
    let launch_tx = user_tx.clone();
    let instance_guard =
        match super::instance::start(&window_options.instance_policy, move |launch| {
            let accepted = launch_tx
                .send(super::JadixUserEvent::SecondInstance(launch))
                .is_ok();
            super::ui_bridge::wake();
            accepted
        }) {
            Ok(None)
                if matches!(
                    window_options.instance_policy,
                    super::InstancePolicy::Single { .. }
                ) =>
            {
                return
            }
            Ok(guard) => guard,
            Err(error) => {
                log::error!("instance startup failed: {error}");
                return;
            }
        };

    let window_options = crate::platform::apply_ohos_window_options(window_options);
    window_options.install_text_defaults();
    crate::init();

    if let Err(e) = crate::platform::ensure_winit_host() {
        log::error!("{e}");
        return;
    }
    let event_loop = crate::platform::build_event_loop();
    let event_loop_proxy = event_loop.create_proxy();
    super::ui_bridge::install_proxy(event_loop_proxy.clone(), user_tx);
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = create_app(
        window_options,
        app_builder,
        event_loop_proxy.clone(),
        user_rx,
    );
    app.instance_guard = instance_guard;
    event_loop_proxy.wake_up();
    #[cfg(not(target_arch = "wasm32"))]
    crate::platform::run_winit_app(event_loop, app);
    #[cfg(target_arch = "wasm32")]
    crate::platform::spawn_winit_app(event_loop, app);
}

pub(super) fn create_app(
    window_options: super::WindowOptions,
    app_builder: impl FnMut() -> Element + 'static,
    event_loop_proxy: winit::event_loop::EventLoopProxy,
    user_rx: std::sync::mpsc::Receiver<super::JadixUserEvent>,
) -> super::App {
    crate::authoring::window::activate(super::window_scope::WindowScope::Primary);
    super::App {
        detached_gpu: None,
        primary_owner: None,
        instance_guard: None,
        window_modifiers: Default::default(),
        frame_pacers: Default::default(),
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
        scrollbar_geometry_pending: false,
        content_scroll_drag: None,
        textarea_resize_drag: None,
        primary_touch_id: None,
        active_touches: std::collections::HashMap::new(),
        pinch_prev_dist: None,
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
        smooth_scroll_motion: std::collections::HashMap::new(),
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
    }
}

#[cfg(test)]
#[path = "tests/idle_control_flow.rs"]
mod idle_control_flow_tests;
