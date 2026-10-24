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
        event_loop: &dyn ActiveEventLoop,
    ) {
        self.bind_pointer_window(id);
        super::window_scope::force_secondary_id(id);
        self.cursor_pos = position;
        self.cursor_inside_window = true;

        let scale = {
            let secs = self.secondaries.borrow();
            secs.get(&id)
                .map(|s| s.window.scale_factor() as f32)
                .unwrap_or(1.0)
        };
        let x = position.x as f32 / scale.max(1.0);
        let y = position.y as f32 / scale.max(1.0);

        if self.left_button_down {
            if let Some(drag) = self.scrollbar_drag.as_ref() {
                let pointer = if drag.axis == super::scroll_support::ScrollAxis::X { x } else { y };
                let span = (drag.track_len - drag.thumb_len).max(1.0);
                let target = ((pointer - drag.track_start - drag.grab_offset) / span)
                    .clamp(0.0, 1.0) * drag.max_offset;
                let keys = crate::core::scroll_viewport::ScrollViewportSignalKeys::new(&drag.rest);
                let mut offset = [use_signal(&keys.offset_x, || 0.0_f32).peek(),
                    use_signal(&keys.offset_y, || 0.0_f32).peek()];
                offset[if drag.axis == super::scroll_support::ScrollAxis::X { 0 } else { 1 }] = target;
                super::scroll_support::set_scroll_offset_with_behavior(&drag.rest, offset[0], offset[1], true);
                if let Some(sec) = self.secondaries.borrow().get(&id) {
                    sec.window.request_redraw();
                }
                return;
            }
        }

        let drag_sel_update: Option<(String, usize, usize, usize, bool)> = if self.left_button_down
        {
            let mut secs = self.secondaries.borrow_mut();
            let Some(sec) = secs.get_mut(&id) else {
                return;
            };
            let ui_scale = scale.max(1.0);
            if let Some(rest) = self.input_drag_rest.clone() {
                let now = std::time::Instant::now();
                let dt = now
                    .duration_since(self.last_frame_time)
                    .as_secs_f32()
                    .clamp(1.0 / 240.0, 0.05);
                let anchor = self.input_drag_anchor;
                let keep = super::window_events::tick_input_drag_select_at(
                    &mut sec.root_element,
                    sec.text_renderer.font_system_mut(),
                    scale,
                    dt,
                    x,
                    y,
                    &rest,
                    anchor,
                    self.input_drag_multi,
                    &self.input_drag_multi_base,
                    self.input_drag_multi && self.modifiers.shift_key(),
                );
                if keep {
                    sec.window.request_redraw();
                }
                None
            } else if let Some(key) = self.label_drag_key.as_ref() {
                if let Some((bounds, display_el, style, leaf_scroll)) =
                    super::tree_hit::find_selectable_text_leaf_for_key(&sec.root_element, key)
                {
                    let layout_ptr = super::tree_hit::world_to_selectable_text_layout_pointer(
                        &sec.root_element,
                        key,
                        [x, y],
                        [0.0, 0.0],
                    );
                    let mx =
                        (layout_ptr[0] + leaf_scroll[0]).clamp(bounds[0], bounds[0] + bounds[2]);
                    let my =
                        (layout_ptr[1] + leaf_scroll[1]).clamp(bounds[1], bounds[1] + bounds[3]);
                    let ix = crate::renderer::hit_test_input_text_index(
                        sec.text_renderer.font_system_mut(),
                        &display_el,
                        mx,
                        my,
                        bounds,
                        &style,
                        ui_scale,
                    );
                    let lo = self.label_drag_anchor.min(ix);
                    let hi = self.label_drag_anchor.max(ix);
                    Some((key.clone(), lo, hi, ix, false))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let input_drag_active = self.input_drag_rest.is_some() && self.left_button_down;

        let (x, y, path, captured_move_path, cursor_pick, hover_cbs, window) = {
            let mut secs = self.secondaries.borrow_mut();
            let Some(sec) = secs.get_mut(&id) else {
                return;
            };
            let path = super::pointer_events::hit_path(&sec.root_element, x, y);
            let captured_move_path = super::pointer_events::captured_path(&sec.root_element);
            let cursor_pick = super::tree_hit::pick_cursor(&sec.root_element, x, y, [0.0, 0.0]);
            let mut hover_cbs = Vec::new();
            super::tree_hit::update_hovers(&sec.root_element, x, y, [0.0, 0.0], &mut hover_cbs);
            let window = sec.window.clone();
            (
                x,
                y,
                path,
                captured_move_path,
                cursor_pick,
                hover_cbs,
                window,
            )
        };

        super::tree_hit::apply_cursor(
            window.as_ref(),
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
            captured_move_path.as_deref().unwrap_or(&path),
            &mut pev,
            super::pointer_events::PointerKind::Move,
        );
        self.last_client_pos = Some([x, y]);

        let mut need_redraw = input_drag_active;
        if let Some((key, lo, hi, caret, is_input)) = drag_sel_update {
            if is_input {
                if self.input_drag_rest.as_deref() == Some(key.as_str()) {
                    let sk = crate::components::input::input_selection_signal_key(&key);
                    use_signal(&sk, || None::<(usize, usize)>).set(Some((lo, hi)));
                    let ck = crate::components::input::input_caret_signal_key(&key);
                    use_signal(&ck, || None::<usize>).set(Some(caret));
                    need_redraw = true;
                }
            } else if self.label_drag_key.as_deref() == Some(key.as_str()) {
                let sk = crate::components::label::label_selection_signal_key(&key);
                use_signal(&sk, || None::<(usize, usize)>).set(Some((lo, hi)));
                need_redraw = true;
            }
        }

        if need_redraw || crate::primitives::canvas::take_canvas_wants_redraw() {
            window.request_redraw();
        } else if let Some(sec) = self.secondaries.borrow().get(&id) {
            sec.window.request_redraw();
        }
    }

    pub(super) fn handle_secondary_cursor_left(
        &mut self,
        id: WindowId,
        event_loop: &dyn ActiveEventLoop,
    ) {
        self.bind_pointer_window(id);
        super::window_scope::force_secondary_id(id);
        self.cursor_inside_window = false;
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
                sec.window.as_ref(),
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

        if button == MouseButton::Left {
            if element_state == ElementState::Released {
                if let Some(drag) = self.scrollbar_drag.take() {
                    let keys = crate::core::scroll_viewport::ScrollViewportSignalKeys::new(&drag.rest);
                    use_signal(&keys.hover_axis_x, || false).set_silent(false);
                    use_signal(&keys.hover_axis_y, || false).set_silent(false);
                    if let Some(sec) = self.secondaries.borrow().get(&id) {
                        sec.window.request_redraw();
                    }
                    return;
                }
            } else if self.begin_secondary_scrollbar_drag(id, x, y) {
                return;
            }
        }

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

                    let click_count = self.click_count;
                    let (path, input_drag, label_drag) = {
                        let mut secs = self.secondaries.borrow_mut();
                        let sec = secs.get_mut(&id).unwrap();
                        let path = super::pointer_events::captured_path(&sec.root_element)
                            .unwrap_or_else(|| {
                                super::pointer_events::hit_path(&sec.root_element, x, y)
                            });
                        let input_drag = super::tree_hit::find_input_text_at(
                            &sec.root_element,
                            None,
                            x,
                            y,
                            [0.0, 0.0],
                        )
                        .map(
                            |(rest, bounds, display_el, style, leaf_scroll, focus_id)| {
                                let ui_scale = scale.max(1.0);
                                let is_textarea =
                                    focus_id.starts_with(crate::components::TEXTAREA_FOCUS_PREFIX);
                                let layout_ptr =
                                    super::tree_hit::world_to_input_text_layout_pointer(
                                        &sec.root_element,
                                        &rest,
                                        [x, y],
                                        [0.0, 0.0],
                                    );
                                let (mx, my, hit_bounds, hit_style) = if is_textarea {
                                    super::tree_hit::input_text_edit_pointer_space(
                                        &sec.root_element,
                                        &rest,
                                        x,
                                        y,
                                        bounds,
                                        &style,
                                        leaf_scroll,
                                    )
                                } else {
                                    (
                                        layout_ptr[0] + leaf_scroll[0],
                                        layout_ptr[1] + leaf_scroll[1],
                                        bounds,
                                        style.clone(),
                                    )
                                };
                                let ix = if is_textarea {
                                    crate::renderer::hit_test_textarea_click_caret_index(
                                        sec.text_renderer.font_system_mut(),
                                        &display_el,
                                        layout_ptr[0] + leaf_scroll[0],
                                        layout_ptr[1] + leaf_scroll[1],
                                        bounds,
                                        &hit_style,
                                        ui_scale,
                                        mx,
                                        my,
                                    )
                                } else {
                                    crate::renderer::hit_test_input_text_index(
                                        sec.text_renderer.font_system_mut(),
                                        &display_el,
                                        mx,
                                        my,
                                        hit_bounds,
                                        &hit_style,
                                        ui_scale,
                                    )
                                };
                                let vk = crate::components::input::input_value_signal_key(&rest);
                                let pk = crate::components::input::input_preedit_signal_key(&rest);
                                let committed = use_signal(&vk, String::new).get();
                                let pre = use_signal(&pk, String::new).get();
                                let caret_ix = if committed.is_empty() && pre.is_empty() {
                                    0
                                } else {
                                    crate::components::input::caret_index_from_display_hit(
                                        &rest, ix,
                                    )
                                };
                                let line_sel = if click_count >= 2
                                    && crate::renderer::hit_test_input_is_line_side_blank(
                                        sec.text_renderer.font_system_mut(),
                                        &display_el,
                                        if is_textarea {
                                            layout_ptr[0] + leaf_scroll[0]
                                        } else {
                                            mx
                                        },
                                        my,
                                        bounds,
                                        if is_textarea { &hit_style } else { &style },
                                        ui_scale,
                                    ) {
                                    crate::utils::keyboard::select_line_range(&committed, caret_ix)
                                } else {
                                    None
                                };
                                (rest, caret_ix, focus_id, line_sel)
                            },
                        );
                        let label_drag = if input_drag.is_none() {
                            super::tree_hit::find_selectable_text_at(
                                &sec.root_element,
                                x,
                                y,
                                [0.0, 0.0],
                            )
                            .map(
                                |(key, bounds, display_el, style, leaf_scroll)| {
                                    let ui_scale = scale.max(1.0);
                                    let layout_ptr =
                                        super::tree_hit::world_to_selectable_text_layout_pointer(
                                            &sec.root_element,
                                            &key,
                                            [x, y],
                                            [0.0, 0.0],
                                        );
                                    let ix = crate::renderer::hit_test_input_text_index(
                                        sec.text_renderer.font_system_mut(),
                                        &display_el,
                                        layout_ptr[0] + leaf_scroll[0],
                                        layout_ptr[1] + leaf_scroll[1],
                                        bounds,
                                        &style,
                                        ui_scale,
                                    );
                                    (key, ix)
                                },
                            )
                        } else {
                            None
                        };
                        (path, input_drag, label_drag)
                    };

                    self.click_press_path = Some(path.clone());
                    self.click_press_scrolled = false;
                    let mut down =
                        self.build_mouse_event(x, y, MouseButton::Left, 1, "mousedown", None);
                    super::pointer_events::dispatch_pointer(
                        &path,
                        &mut down,
                        super::pointer_events::PointerKind::Down,
                    );

                    if crate::utils::drag::is_dragging() {
                        let (hits, viewport) = {
                            let secs = self.secondaries.borrow();
                            if let Some(sec) = secs.get(&id) {
                                let size = sec.window.surface_size();
                                let scale = sec.window.scale_factor() as f32;
                                (
                                    crate::utils::drag::collect_hits(&sec.root_element),
                                    [
                                        size.width as f32 / scale.max(1.0),
                                        size.height as f32 / scale.max(1.0),
                                    ],
                                )
                            } else {
                                (Vec::new(), [0.0_f32, 0.0])
                            }
                        };
                        let _ = crate::utils::drag::tick(x, y, &hits, viewport);
                    }

                    use_signal("__jadix_focus_visible", || false).set(false);

                    let input_drag_fid = input_drag.as_ref().map(|(_, _, fid, _)| fid.clone());
                    if let Some((rest, caret_ix, focus_id, line_sel)) = input_drag {
                        let was_focused =
                            use_signal("__jadix_button_focus_id", String::new).peek() == focus_id;
                        use_signal("__jadix_button_focus_id", String::new).set(focus_id);
                        let ck = crate::components::input::input_caret_signal_key(&rest);
                        let sk = crate::components::input::input_selection_signal_key(&rest);
                        let pk = crate::components::input::input_preedit_signal_key(&rest);
                        let ty_key = crate::components::input::input_type_signal_key(&rest);
                        let pre_empty = use_signal(&pk, String::new).peek().is_empty();
                        let existing = use_signal(&sk, || None::<(usize, usize)>).get();
                        let password = crate::components::input::InputType::parse(
                            &use_signal(&ty_key, || {
                                crate::components::input::InputType::Text
                                    .as_str()
                                    .to_string()
                            })
                            .peek(),
                        )
                        .is_password();
                        let sel_range = existing.and_then(|(a, b)| {
                            let lo = a.min(b);
                            let hi = a.max(b);
                            (lo < hi).then_some((lo, hi))
                        });
                        let alt_multi = self.modifiers.alt_key() && pre_empty;
                        self.input_drag_multi = false;
                        self.input_drag_multi_base.clear();
                        if alt_multi {
                            let vk = crate::components::input::input_value_signal_key(&rest);
                            let committed = use_signal(&vk, String::new).peek();
                            let shift_box = self.modifiers.shift_key();
                            if let Some((anchor, base)) =
                                crate::utils::text_field::begin_alt_pointer(
                                    &rest, &committed, caret_ix, shift_box,
                                )
                            {
                                crate::renderer::restart_caret_blink();
                                self.input_drag_multi_base = base;
                                self.input_drag_multi = true;
                                self.input_drag_anchor = anchor;
                                self.input_drag_rest = Some(rest);
                                crate::utils::text_field::set_text_drag_select_active(true);
                            } else {
                                crate::renderer::restart_caret_blink();
                            }
                        } else {
                            let restore_selection = !was_focused
                                && line_sel.is_none()
                                && pre_empty
                                && sel_range.is_some();
                            if !restore_selection {
                                crate::utils::text_field::clear_extra_carets(&rest);
                            }
                            let can_move = was_focused
                                && line_sel.is_none()
                                && pre_empty
                                && !password
                                && sel_range
                                    .is_some_and(|(lo, hi)| caret_ix >= lo && caret_ix <= hi);
                            if can_move {
                                let (lo, hi) = sel_range.expect("can_move implies non-empty range");
                                let vk = crate::components::input::input_value_signal_key(&rest);
                                let committed = use_signal(&vk, String::new).peek();
                                let chunk = committed.get(lo..hi).unwrap_or("").to_string();
                                let (size, weight, color, lh) = {
                                    let secs = self.secondaries.borrow();
                                    secs.get(&id)
                                    .and_then(|sec| {
                                        super::tree_hit::find_input_text_leaf_for_rest(
                                            &sec.root_element,
                                            None,
                                            &rest,
                                        )
                                    })
                                    .map(|(_, _, style, _)| {
                                        let lh = style.line_height_px.unwrap_or_else(|| {
                                            style.size_resolved()
                                                * style.line_height_scale.unwrap_or(
                                                    crate::utils::text_field::text_edit_default_leading_scale(),
                                                )
                                        });
                                        (
                                            style.size_resolved(),
                                            style.weight_resolved(),
                                            style.color_resolved(),
                                            lh,
                                        )
                                    })
                                    .unwrap_or((14.0, 400, [15, 23, 42, 255], 21.0))
                                };
                                crate::utils::text_sel_move::begin_potential(
                                    rest.clone(),
                                    (lo, hi),
                                    chunk,
                                    caret_ix,
                                    (x, y),
                                    size,
                                    weight,
                                    color,
                                    lh,
                                );
                                self.input_drag_anchor = caret_ix;
                            } else if restore_selection {
                                self.input_drag_anchor = caret_ix;
                                crate::renderer::restart_caret_blink();
                            } else if let Some((lo, hi)) = line_sel {
                                use_signal(&ck, || None::<usize>).set(Some(hi));
                                use_signal(&sk, || None::<(usize, usize)>).set(Some((lo, hi)));
                                self.input_drag_anchor = lo;
                                crate::renderer::restart_caret_blink();
                            } else {
                                use_signal(&ck, || None::<usize>).set(Some(caret_ix));
                                use_signal(&sk, || None::<(usize, usize)>)
                                    .set(Some((caret_ix, caret_ix)));
                                self.input_drag_anchor = caret_ix;
                                crate::renderer::restart_caret_blink();
                            }
                            self.input_drag_rest = (!restore_selection).then_some(rest);
                            crate::utils::text_field::set_text_drag_select_active(
                                !restore_selection,
                            );
                        }
                        self.label_drag_key = None;
                        self.left_button_down = true;
                    } else if let Some((key, caret_ix)) = label_drag {
                        let sk = crate::components::label::label_selection_signal_key(&key);
                        use_signal(&sk, || None::<(usize, usize)>).set(Some((caret_ix, caret_ix)));
                        self.label_drag_key = Some(key);
                        self.label_drag_anchor = caret_ix;
                        self.input_drag_rest = None;
                        self.input_drag_multi = false;
                        self.input_drag_multi_base.clear();
                        crate::utils::text_field::set_text_drag_select_active(false);
                        crate::utils::text_sel_move::cancel();
                        self.left_button_down = true;
                    } else if let Some(fid) = path.iter().rev().find_map(|e| e.focus_id.clone()) {
                        use_signal("__jadix_button_focus_id", String::new).set(fid);
                    }

                    // Blur text edit / InputOTP when pressing outside the focused control.
                    {
                        let focused = use_signal("__jadix_button_focus_id", String::new).peek();
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
                                use_signal("__jadix_button_focus_id", String::new)
                                    .set(String::new());
                            }
                        } else if crate::components::input_otp::is_input_otp_focus(&focused) {
                            let keep = {
                                let secs = self.secondaries.borrow();
                                secs.get(&id)
                                    .map(|sec| {
                                        Self::pointer_keeps_otp_focus(
                                            &sec.root_element,
                                            &focused,
                                            &path,
                                            x,
                                            y,
                                        )
                                    })
                                    .unwrap_or(true)
                            };
                            if !keep {
                                use_signal("__jadix_button_focus_id", String::new)
                                    .set(String::new());
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
                        if super::portal_overlay::try_dismiss_outside_press() {
                            self.outside_dismiss_on_down = true;
                        }
                    }
                }

                if element_state == ElementState::Released {
                    let (path, click_path, cb, label_for, on_exclusive, on_trigger) = {
                        let secs = self.secondaries.borrow();
                        let Some(sec) = secs.get(&id) else {
                            return;
                        };
                        let path = super::pointer_events::captured_path(&sec.root_element)
                            .unwrap_or_else(|| {
                                super::pointer_events::hit_path(&sec.root_element, x, y)
                            });
                        let click_path = match self.click_press_path.take() {
                            Some(down) => {
                                let scrolled = std::mem::take(&mut self.click_press_scrolled);
                                super::pointer_events::click_gesture_path(
                                    &down,
                                    &path,
                                    super::pointer_events::click_pointer_moved(
                                        self.last_click_pos,
                                        [self.cursor_pos.x as f32, self.cursor_pos.y as f32],
                                    ),
                                    scrolled,
                                )
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
                            use_signal("__jadix_button_focus_id", String::new).set(fid);
                        }
                        if let Some(cb) = cb {
                            // Callbacks must run without borrowing `secondaries` (may open Dialog /
                            // reconcile into this secondary via window_scope).
                            (*cb.borrow_mut())();
                        }
                    }
                    let _ = click_path;
                    let should_outside = !on_exclusive && !on_trigger;
                    let ended_panel_drag =
                        crate::utils::anchored_overlay::take_ended_floating_panel_drag();
                    if should_outside && !self.outside_dismiss_on_down && !ended_panel_drag {
                        let _ = super::portal_overlay::try_dismiss_outside_press();
                    }
                    self.outside_dismiss_on_down = false;
                    crate::utils::popup::arm_underlay_close();
                    if let Some(rest) = self.input_drag_rest.take() {
                        self.input_drag_multi = false;
                        self.input_drag_multi_base.clear();
                        super::window_events::apply_input_drag_release(&rest);
                    }
                    if let Some(key) = self.label_drag_key.take() {
                        let sk = crate::components::label::label_selection_signal_key(&key);
                        let sel = use_signal(&sk, || None::<(usize, usize)>).get();
                        if let Some((a, b)) = sel {
                            if a == b {
                                use_signal(&sk, || None::<(usize, usize)>).set(None);
                            }
                        }
                    }
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

    fn begin_secondary_scrollbar_drag(&mut self, id: WindowId, x: f32, y: f32) -> bool {
        let drag = {
            let windows = self.secondaries.borrow();
            let Some(window) = windows.get(&id) else { return false };
            let Some(focus) = super::tree_hit::find_focus_id_at(&window.root_element, x, y, [0.0; 2])
                else { return false };
            let Some((rest, axis, is_thumb)) = super::scroll_support::parse_scrollbar_focus_id(&focus)
                else { return false };
            let Some((_, maximum, _)) = window.scroll_metrics_cache.get(&rest)
                else { return false };
            let index = if axis == super::scroll_support::ScrollAxis::X { 0 } else { 1 };
            let Some((track, thumb)) = super::scroll_support::scrollbar_drag_bounds(&window.root_element, &rest, axis)
                else { return false };
            let pointer = if index == 0 { x } else { y };
            super::scroll_support::ScrollbarDragState {
                rest, axis,
                track_start: track[index],
                track_len: track[index + 2],
                thumb_len: thumb[index + 2],
                max_offset: maximum[index],
                grab_offset: if is_thumb {
                    (pointer - thumb[index]).clamp(0.0, thumb[index + 2])
                } else { thumb[index + 2] * 0.5 },
            }
        };
        let keys = crate::core::scroll_viewport::ScrollViewportSignalKeys::new(&drag.rest);
        let pointer = if drag.axis == super::scroll_support::ScrollAxis::X { x } else { y };
        let target = ((pointer - drag.track_start - drag.grab_offset)
            / (drag.track_len - drag.thumb_len).max(1.0)).clamp(0.0, 1.0) * drag.max_offset;
        let mut offset = [use_signal(&keys.offset_x, || 0.0_f32).peek(),
            use_signal(&keys.offset_y, || 0.0_f32).peek()];
        let horizontal = drag.axis == super::scroll_support::ScrollAxis::X;
        offset[if horizontal { 0 } else { 1 }] = target;
        use_signal(if horizontal { &keys.hover_axis_x } else { &keys.hover_axis_y }, || false).set_silent(true);
        super::scroll_support::set_scroll_offset_with_behavior(&drag.rest, offset[0], offset[1], true);
        self.click_press_path = None;
        self.scrollbar_drag = Some(drag);
        if let Some(window) = self.secondaries.borrow().get(&id) {
            window.window.request_redraw();
        }
        true
    }

    pub(super) fn handle_secondary_mouse_wheel(&mut self, id: WindowId, delta: MouseScrollDelta) {
        self.bind_pointer_window(id);
        super::window_scope::force_secondary_id(id);

        let mut secs = self.secondaries.borrow_mut();
        let Some(sec) = secs.get_mut(&id) else {
            return;
        };
        let scale = sec.window.scale_factor() as f32;
        let (dx, dy) = super::scroll_support::wheel_delta_logical_px(delta, scale);
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
        let mut scrolled = false;
        for rest in rests {
            let Some((_, [max_x, max_y], _)) = metrics.get(&rest) else {
                continue;
            };
            let (dx, dy) =
                super::scroll_support::wheel_delta_for_scroll_range(dx, dy, *max_x, *max_y);
            if *max_y > 0.5 && dy.abs() > f32::EPSILON {
                let key = crate::core::scroll_viewport::scroll_offset_y_signal_key(&rest);
                let cur = use_signal(&key, || 0.0f32).peek();
                let next = (cur - dy).clamp(0.0, *max_y);
                let horizontal = use_signal(&crate::core::scroll_viewport::scroll_offset_x_signal_key(&rest), || 0.0_f32).peek();
                super::scroll_support::set_scroll_offset_with_behavior(&rest, horizontal, next, true);
                scrolled = true;
                break;
            }
            if *max_x > 0.5 && dx.abs() > f32::EPSILON {
                let key = crate::core::scroll_viewport::scroll_offset_x_signal_key(&rest);
                let cur = use_signal(&key, || 0.0f32).peek();
                let next = (cur - dx).clamp(0.0, *max_x);
                let vertical = use_signal(&crate::core::scroll_viewport::scroll_offset_y_signal_key(&rest), || 0.0_f32).peek();
                super::scroll_support::set_scroll_offset_with_behavior(&rest, next, vertical, true);
                scrolled = true;
                break;
            }
        }
        sec.window.request_redraw();
        drop(secs);
        if scrolled {
            self.note_user_scroll_after_press();
        }
    }

    pub(super) fn handle_secondary_keyboard(&mut self, id: WindowId, event: KeyEvent) {
        self.bind_pointer_window(id);
        super::window_scope::force_secondary_id(id);
        #[cfg(all(debug_assertions, feature = "devtools"))]
        if super::devtools::is_detached_window(id)
            && super::devtools::try_toggle_from_key(&event, self.modifiers)
        {
            return;
        }
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
                let focused = use_signal("__jadix_button_focus_id", String::new);
                let focus_visible = use_signal("__jadix_focus_visible", || false);
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
        let fid = use_signal("__jadix_button_focus_id", String::new).peek();
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
