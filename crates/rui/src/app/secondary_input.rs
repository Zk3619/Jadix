//! Secondary-window input channel — **never** hit-tests or mutates the primary [`AppState`] tree.
//!
//! Primary and secondary share the process reactive runtime and optional [`super::multi_window::shared_slot`]
//! bridges; pointer / keyboard / Esc stacks are scoped per [`super::window_scope`].

use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::WindowId;

use crate::core::reactivity::use_signal;
use crate::utils::input_hit::collect_focusable_ids;

impl super::App {
    /// Route an event to the secondary UI tree only (no AppState tree swap).
    pub(super) fn handle_secondary_cursor_moved(
        &mut self,
        id: WindowId,
        position: PhysicalPosition<f64>,
        event_loop: &ActiveEventLoop,
    ) {
        self.bind_pointer_window(id);
        super::window_scope::force_secondary_id(id);
        self.cursor_pos = position;

        let (x, y, path, cursor_pick, hover_cbs, window) = {
            let mut secs = self.secondaries.borrow_mut();
            let Some(sec) = secs.get_mut(&id) else {
                return;
            };
            let scale = sec.window.scale_factor() as f32;
            let x = position.x as f32 / scale.max(1.0);
            let y = position.y as f32 / scale.max(1.0);
            let path = super::pointer_events::hit_path(&sec.root_element, x, y);
            let cursor_pick =
                super::tree_hit::pick_cursor(&sec.root_element, x, y, [0.0, 0.0]);
            let mut hover_cbs = Vec::new();
            super::tree_hit::update_hovers(&sec.root_element, x, y, [0.0, 0.0], &mut hover_cbs);
            let window = sec.window.clone();
            (x, y, path, cursor_pick, hover_cbs, window)
        };

        super::tree_hit::apply_cursor(
            &window,
            Some(event_loop),
            &mut self.custom_cursors,
            cursor_pick,
        );

        for (cb, is_hover) in hover_cbs {
            (*cb.borrow_mut())(is_hover);
        }
        self.sync_mouse_enter_leave(x, y, &path, MouseButton::Left);
        let mut pev = self.build_mouse_event(x, y, MouseButton::Left, 0, "mousemove", None);
        super::pointer_events::dispatch_pointer(
            &path,
            &mut pev,
            super::pointer_events::PointerKind::Move,
        );
        self.last_client_pos = Some([x, y]);
        if let Some(sec) = self.secondaries.borrow().get(&id) {
            sec.window.request_redraw();
        }
    }

    pub(super) fn handle_secondary_cursor_left(
        &mut self,
        id: WindowId,
        event_loop: &ActiveEventLoop,
    ) {
        self.bind_pointer_window(id);
        super::window_scope::force_secondary_id(id);
        if let Some(old_path) = self.last_mouse_path.take() {
            let [x, y] = self.last_client_pos.unwrap_or([0.0, 0.0]);
            let _ = self.dispatch_mouse_path(
                &old_path,
                x,
                y,
                MouseButton::Left,
                0,
                "mouseout",
                super::pointer_events::PointerKind::Out,
                None,
            );
            self.last_mouse_target = None;
        }
        if let Some(sec) = self.secondaries.borrow().get(&id) {
            super::tree_hit::apply_cursor(
                &sec.window,
                Some(event_loop),
                &mut self.custom_cursors,
                None,
            );
            sec.window.request_redraw();
        }
    }

    pub(super) fn handle_secondary_mouse_button(
        &mut self,
        id: WindowId,
        element_state: ElementState,
        button: MouseButton,
    ) {
        self.bind_pointer_window(id);
        super::window_scope::force_secondary_id(id);
        self.update_mouse_buttons(button, element_state);

        let scale = {
            let secs = self.secondaries.borrow();
            let Some(sec) = secs.get(&id) else {
                return;
            };
            sec.window.scale_factor() as f32
        };
        let x = self.cursor_pos.x as f32 / scale.max(1.0);
        let y = self.cursor_pos.y as f32 / scale.max(1.0);

        match button {
            MouseButton::Left => {
                if element_state == ElementState::Pressed {
                    let now = std::time::Instant::now();
                    const DBL_MS: u128 = 400;
                    const DBL_PX: f32 = 6.0;
                    let px = self.cursor_pos.x as f32;
                    let py = self.cursor_pos.y as f32;
                    let is_double = self.last_click_at.is_some_and(|t| {
                        now.duration_since(t).as_millis() <= DBL_MS
                            && (px - self.last_click_pos[0]).abs() <= DBL_PX
                            && (py - self.last_click_pos[1]).abs() <= DBL_PX
                    });
                    self.click_count = if is_double {
                        self.click_count.saturating_add(1).max(2)
                    } else {
                        1
                    };
                    self.last_click_at = Some(now);
                    self.last_click_pos = [px, py];

                    let (path, input_drag) = {
                        let mut secs = self.secondaries.borrow_mut();
                        let sec = secs.get_mut(&id).unwrap();
                        let path = super::pointer_events::hit_path(&sec.root_element, x, y);
                        let input_drag = super::tree_hit::find_input_text_at(
                            &sec.root_element,
                            None,
                            x,
                            y,
                            [0.0, 0.0],
                        )
                        .map(|(rest, bounds, display_el, style, leaf_scroll, focus_id)| {
                            let ui_scale = scale.max(1.0);
                            let ix = crate::renderer::hit_test_input_text_index(
                                sec.text_renderer.font_system_mut(),
                                &display_el,
                                x + leaf_scroll[0],
                                y + leaf_scroll[1],
                                bounds,
                                &style,
                                ui_scale,
                            );
                            let vk = crate::components::input::input_value_signal_key(&rest);
                            let pk = crate::components::input::input_preedit_signal_key(&rest);
                            let committed = use_signal(&vk, String::new).get();
                            let pre = use_signal(&pk, String::new).get();
                            let caret_ix = if committed.is_empty() && pre.is_empty() {
                                0
                            } else {
                                crate::components::input::caret_index_from_display_hit(&rest, ix)
                            };
                            (rest, caret_ix, focus_id)
                        });
                        (path, input_drag)
                    };

                    self.click_press_path = Some(path.clone());
                    let mut down =
                        self.build_mouse_event(x, y, MouseButton::Left, 1, "mousedown", None);
                    super::pointer_events::dispatch_pointer(
                        &path,
                        &mut down,
                        super::pointer_events::PointerKind::Down,
                    );

                    use_signal("__rui_focus_visible", || false).set(false);

                    let input_drag_fid = input_drag.as_ref().map(|(_, _, fid)| fid.clone());
                    if let Some((rest, caret_ix, focus_id)) = input_drag {
                        use_signal("__rui_button_focus_id", String::new).set(focus_id);
                        let ck = crate::components::input::input_caret_signal_key(&rest);
                        let sk = crate::components::input::input_selection_signal_key(&rest);
                        use_signal(&ck, || None::<usize>).set(Some(caret_ix));
                        use_signal(&sk, || None::<(usize, usize)>)
                            .set(Some((caret_ix, caret_ix)));
                        self.input_drag_rest = Some(rest);
                        self.input_drag_anchor = caret_ix;
                        self.left_button_down = true;
                    } else if let Some(fid) = path.iter().rev().find_map(|e| e.focus_id.clone()) {
                        use_signal("__rui_button_focus_id", String::new).set(fid);
                    }

                    // Blur text edit when pressing outside the focused field (secondary tree).
                    {
                        let focused = use_signal("__rui_button_focus_id", String::new).peek();
                        if super::ime_support::is_text_edit_focus(&focused) {
                            let keep = {
                                let secs = self.secondaries.borrow();
                                secs.get(&id)
                                    .map(|sec| {
                                        Self::pointer_keeps_text_edit_focus(
                                            &sec.root_element,
                                            &focused,
                                            &path,
                                            input_drag_fid.as_deref(),
                                            x,
                                            y,
                                        )
                                    })
                                    .unwrap_or(true)
                            };
                            if !keep {
                                use_signal("__rui_button_focus_id", String::new).set(String::new());
                            }
                        }
                    }

                    // Dismiss floating overlays on press outside (same as primary).
                    let should_outside = {
                        let secs = self.secondaries.borrow();
                        match secs.get(&id) {
                            Some(sec) => {
                                let on_exclusive =
                                    super::portal_overlay::hit_path_on_top_exclusive_surface(&path);
                                let on_trigger = crate::utils::popup::path_hits_floating_trigger(
                                    &sec.root_element,
                                    &path,
                                );
                                !on_exclusive && !on_trigger
                            }
                            None => false,
                        }
                    };
                    if should_outside {
                        let _ = super::portal_overlay::try_dismiss_outside_press();
                    }
                }

                if element_state == ElementState::Released {
                    let (path, click_path, cb, label_for, on_exclusive, on_trigger) = {
                        let secs = self.secondaries.borrow();
                        let Some(sec) = secs.get(&id) else {
                            return;
                        };
                        let path = super::pointer_events::hit_path(&sec.root_element, x, y);
                        let click_path = match self.click_press_path.take() {
                            Some(down) => {
                                super::pointer_events::click_dispatch_path(&down, &path)
                            }
                            None => Vec::new(),
                        };
                        let (cb, label_for) = super::pointer_events::resolve_click_from_path(
                            &sec.root_element,
                            &click_path,
                        );
                        let on_exclusive =
                            super::portal_overlay::hit_path_on_top_exclusive_surface(&path);
                        let on_trigger = crate::utils::popup::path_hits_floating_trigger(
                            &sec.root_element,
                            &path,
                        );
                        (path, click_path, cb, label_for, on_exclusive, on_trigger)
                    };

                    let detail = self.click_count.max(1);
                    let mut pev =
                        self.build_mouse_event(x, y, MouseButton::Left, detail, "mouseup", None);
                    super::pointer_events::dispatch_pointer(
                        &path,
                        &mut pev,
                        super::pointer_events::PointerKind::Up,
                    );
                    if !pev.is_consumed() {
                        if let Some(fid) = label_for {
                            use_signal("__rui_button_focus_id", String::new).set(fid);
                        }
                        if let Some(cb) = cb {
                            // Callbacks must run without borrowing `secondaries` (may open Dialog /
                            // reconcile into this secondary via window_scope).
                            (*cb.borrow_mut())();
                        }
                    }
                    let _ = click_path;
                    let should_outside = !on_exclusive && !on_trigger;
                    if should_outside {
                        let _ = super::portal_overlay::try_dismiss_outside_press();
                    }
                    crate::utils::popup::arm_underlay_close();
                    self.input_drag_rest = None;
                    self.left_button_down = false;
                }
            }
            MouseButton::Right => {
                let path = {
                    let secs = self.secondaries.borrow();
                    let Some(sec) = secs.get(&id) else {
                        return;
                    };
                    super::pointer_events::hit_path(&sec.root_element, x, y)
                };
                if element_state == ElementState::Pressed {
                    let mut down =
                        self.build_mouse_event(x, y, MouseButton::Right, 1, "mousedown", None);
                    super::pointer_events::dispatch_pointer(
                        &path,
                        &mut down,
                        super::pointer_events::PointerKind::Down,
                    );
                }
            }
            _ => {}
        }

        if let Some(sec) = self.secondaries.borrow().get(&id) {
            sec.window.request_redraw();
        }
    }

    pub(super) fn handle_secondary_mouse_wheel(
        &mut self,
        id: WindowId,
        delta: MouseScrollDelta,
    ) {
        self.bind_pointer_window(id);
        super::window_scope::force_secondary_id(id);
        let lines = super::scroll_support::get_system_scroll_lines();
        let line_height = 25.0;
        let (dx, dy) = match delta {
            MouseScrollDelta::LineDelta(dx, dy) => {
                (dx * lines * line_height, dy * lines * line_height)
            }
            MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
        };

        let mut secs = self.secondaries.borrow_mut();
        let Some(sec) = secs.get_mut(&id) else {
            return;
        };
        let scale = sec.window.scale_factor() as f32;
        let x = self.cursor_pos.x as f32 / scale.max(1.0);
        let y = self.cursor_pos.y as f32 / scale.max(1.0);
        let mut rests = Vec::new();
        super::scroll_support::collect_scroll_rest_chain_at(
            &sec.root_element,
            x,
            y,
            [0.0, 0.0],
            &mut rests,
        );
        let metrics = &sec.scroll_metrics_cache;
        for rest in rests {
            let Some((_, [max_x, max_y], _)) = metrics.get(&rest) else {
                continue;
            };
            if *max_y > 0.5 && dy.abs() > f32::EPSILON {
                let key = crate::core::scroll_viewport::scroll_offset_y_signal_key(&rest);
                let cur = use_signal(&key, || 0.0f32).peek();
                let next = (cur - dy).clamp(0.0, *max_y);
                use_signal(&key, || 0.0f32).set(next);
                break;
            }
            if *max_x > 0.5 && dx.abs() > f32::EPSILON {
                let key = crate::core::scroll_viewport::scroll_offset_x_signal_key(&rest);
                let cur = use_signal(&key, || 0.0f32).peek();
                let next = (cur - dx).clamp(0.0, *max_x);
                use_signal(&key, || 0.0f32).set(next);
                break;
            }
        }
        sec.window.request_redraw();
    }

    pub(super) fn handle_secondary_keyboard(&mut self, id: WindowId, event: KeyEvent) {
        self.bind_pointer_window(id);
        super::window_scope::force_secondary_id(id);
        if event.state != ElementState::Pressed {
            return;
        }

        if matches!(event.logical_key, Key::Named(NamedKey::Escape)) {
            if super::portal_overlay::try_close_top() {
                if let Some(sec) = self.secondaries.borrow().get(&id) {
                    sec.window.request_redraw();
                }
                return;
            }
        }

        if matches!(event.logical_key, Key::Named(NamedKey::Tab)) && !event.repeat {
            let mut ids = Vec::new();
            {
                let secs = self.secondaries.borrow();
                let Some(sec) = secs.get(&id) else {
                    return;
                };
                if let Some(z) = super::portal_overlay::top_stacked_panel_z() {
                    crate::utils::input_hit::collect_focusable_ids_in_portal_z(
                        &sec.root_element,
                        z,
                        &mut ids,
                    );
                }
                if ids.is_empty() {
                    collect_focusable_ids(&sec.root_element, &mut ids);
                }
            }
            if !ids.is_empty() {
                let focused = use_signal("__rui_button_focus_id", String::new);
                let focus_visible = use_signal("__rui_focus_visible", || false);
                let current = focused.peek();
                let shift = self.modifiers.shift_key();
                let idx = ids.iter().position(|i| i == &current);
                let next_idx = if shift {
                    match idx {
                        Some(0) | None => ids.len() - 1,
                        Some(i) => i - 1,
                    }
                } else {
                    match idx {
                        Some(i) => (i + 1) % ids.len(),
                        None => 0,
                    }
                };
                focus_visible.set(true);
                focused.set(ids[next_idx].clone());
                if let Some(sec) = self.secondaries.borrow().get(&id) {
                    sec.window.request_redraw();
                }
                return;
            }
        }

        // Focused widget key handlers (Select / Combobox list) — secondary tree only.
        let fid = use_signal("__rui_button_focus_id", String::new).peek();
        let key_handlers = {
            let secs = self.secondaries.borrow();
            secs.get(&id)
                .and_then(|sec| super::tree_hit::focused_key_handlers(&sec.root_element, &fid))
        };
        if let Some(handlers) = key_handlers {
            let editing = crate::utils::text_field::text_edit_rest(&fid).is_some();
            let handled = if editing {
                super::tree_hit::run_focused_keydown_only(&handlers, &event)
            } else {
                super::tree_hit::run_focused_key_handlers(&handlers, &event)
            };
            if handled {
                if let Some(sec) = self.secondaries.borrow().get(&id) {
                    sec.window.request_redraw();
                }
                return;
            }
        }

        // Text field insert/delete uses window-scoped focus signals (no primary tree walk).
        if crate::utils::text_field::text_edit_rest(&fid).is_some() {
            let _ = self.handle_keyboard_text_signals(event);
        }

        if let Some(sec) = self.secondaries.borrow().get(&id) {
            sec.window.request_redraw();
        }
    }
}
