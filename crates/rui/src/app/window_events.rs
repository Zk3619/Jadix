use winit::{
    event::{ElementState, MouseButton, MouseScrollDelta},
    event_loop::ActiveEventLoop,
    window::{CursorIcon, WindowId},
};

use super::state::ScrollbarUpdateMode;
use crate::components::scrollbar::SCROLLBAR_TRACK_FOCUS_PREFIX;

impl super::App {
    fn fmt_lp(v: taffy::prelude::LengthPercentage) -> String {
        match v {
            taffy::prelude::LengthPercentage::Length(px) => format!("{px:.1}px"),
            taffy::prelude::LengthPercentage::Percent(p) => format!("{:.1}%", p * 100.0),
        }
    }

    fn fmt_padding_rect(rect: taffy::prelude::Rect<taffy::prelude::LengthPercentage>) -> String {
        format!(
            "l={},r={},t={},b={}",
            Self::fmt_lp(rect.left),
            Self::fmt_lp(rect.right),
            Self::fmt_lp(rect.top),
            Self::fmt_lp(rect.bottom)
        )
    }

    fn fmt_lp_auto(v: taffy::prelude::LengthPercentageAuto) -> String {
        match v {
            taffy::prelude::LengthPercentageAuto::Auto => "auto".to_string(),
            taffy::prelude::LengthPercentageAuto::Length(px) => format!("{px:.1}px"),
            taffy::prelude::LengthPercentageAuto::Percent(p) => format!("{:.1}%", p * 100.0),
        }
    }

    fn fmt_inset_rect(rect: taffy::prelude::Rect<taffy::prelude::LengthPercentageAuto>) -> String {
        format!(
            "l={},r={},t={},b={}",
            Self::fmt_lp_auto(rect.left),
            Self::fmt_lp_auto(rect.right),
            Self::fmt_lp_auto(rect.top),
            Self::fmt_lp_auto(rect.bottom)
        )
    }

    fn find_element_by_template_id<'a>(
        el: &'a crate::core::tree::Element,
        template_id: &str,
    ) -> Option<&'a crate::core::tree::Element> {
        if el.template_id.as_deref() == Some(template_id) {
            return Some(el);
        }
        for child in &el.children {
            if let Some(found) = Self::find_element_by_template_id(child, template_id) {
                return Some(found);
            }
        }
        None
    }

    fn scrollbar_axis_key(rest: &str, axis: super::scroll_support::ScrollAxis) -> String {
        let a = match axis {
            super::scroll_support::ScrollAxis::X => "x",
            super::scroll_support::ScrollAxis::Y => "y",
        };
        format!("{rest}:{a}")
    }

    fn bump_scrollbar_visible_until_axis(
        &mut self,
        rest: &str,
        axis: super::scroll_support::ScrollAxis,
        now: std::time::Instant,
    ) {
        let until = now + std::time::Duration::from_millis(1500);
        self.scrollbar_visible_until
            .insert(Self::scrollbar_axis_key(rest, axis), until);
    }

    fn spring_key(rest: &str, axis: super::scroll_support::ScrollAxis) -> String {
        let a = match axis {
            super::scroll_support::ScrollAxis::X => "x",
            super::scroll_support::ScrollAxis::Y => "y",
        };
        format!("scrollbar_spring_{rest}:{a}")
    }

    fn bump_smooth_active_until(&mut self, rest: &str, now: std::time::Instant) {
        // Small window to bridge discrete wheel ticks (prevents “move-stop-move”).
        let until = now + std::time::Duration::from_millis(200);
        self.smooth_active_until.insert(rest.to_string(), until);
    }

    fn hover_key(rest: &str, axis: super::scroll_support::ScrollAxis) -> String {
        let a = match axis {
            super::scroll_support::ScrollAxis::X => "x",
            super::scroll_support::ScrollAxis::Y => "y",
        };
        format!("__rui_scroll_hover_{rest}:{a}")
    }

    fn scroll_axis_enabled_by_overflow(rest: &str, axis: super::scroll_support::ScrollAxis) -> bool {
        let ox = crate::core::reactivity::use_signal(
            &crate::components::scroll_area::scroll_overflow_x_signal_key(rest),
            || "auto".to_string(),
        )
        .peek();
        let oy = crate::core::reactivity::use_signal(
            &crate::components::scroll_area::scroll_overflow_y_signal_key(rest),
            || "auto".to_string(),
        )
        .peek();
        let o = match axis {
            super::scroll_support::ScrollAxis::X => ox.as_str(),
            super::scroll_support::ScrollAxis::Y => oy.as_str(),
        };
        crate::components::scroll_area::scroll_overflow_allows_scrollbar(o)
    }

    fn peek_scrollbar_visibility(rest: &str, axis: super::scroll_support::ScrollAxis) -> String {
        let a = match axis {
            super::scroll_support::ScrollAxis::X => "x",
            super::scroll_support::ScrollAxis::Y => "y",
        };
        crate::core::reactivity::use_signal(
            &crate::components::scrollbar::scrollbar_visibility_signal_key(rest, a),
            || "auto".to_string(),
        )
        .peek()
        .to_ascii_lowercase()
    }

    /// Only `auto` scrollbars react to wheel-driven show; axis must allow scrolling via overflow.
    fn maybe_wheel_bump_scrollbar_spring(rest: &str, axis: super::scroll_support::ScrollAxis) {
        if !Self::scroll_axis_enabled_by_overflow(rest, axis) {
            return;
        }
        if Self::peek_scrollbar_visibility(rest, axis) != "auto" {
            return;
        }
        crate::style::use_spring(&Self::spring_key(rest, axis), 0.0).set(1.0);
    }

    pub(super) fn handle_cursor_moved(&mut self, id: WindowId, position: winit::dpi::PhysicalPosition<f64>) {
        let mut state_borrow = self.state.borrow_mut();
        let state = match (&mut *state_borrow).as_mut() {
            Some(s) if s.window.id() == id => s,
            _ => return,
        };

        self.cursor_pos = position;
        let scale = state.window.scale_factor() as f32;
        let x = position.x as f32 / scale.max(1.0);
        let y = position.y as f32 / scale.max(1.0);
        let window = state.window.clone();

        // Hover-driven overlay scrollbar visibility: pick the topmost scroll area (or scrollbar target)
        // under the cursor and keep its hover signal set while we remain inside.
        let mut hover_rest: Option<String> = None;
        let mut hover_axis: Option<(String, super::scroll_support::ScrollAxis)> = None;
        if let Some(fid) = super::tree_hit::find_focus_id_at(&state.root_element, x, y, [0.0, 0.0]) {
            if let Some((rest, axis, _)) = super::scroll_support::parse_scrollbar_focus_id(&fid) {
                // Hovering scrollbar track/thumb should only affect that axis.
                // Do not set area-hover here, otherwise both axes are treated as hovered.
                hover_axis = Some((rest, axis));
            }
        }
        if hover_rest.is_none() {
            let mut targets = Vec::new();
            super::scroll_support::collect_scroll_chain_at(
                &state.root_element,
                x,
                y,
                [0.0, 0.0],
                &mut targets,
            );
            // `collect_scroll_chain_at` only keeps the topmost branch; first is deepest.
            if let Some((rest, _, _, _)) = targets.first() {
                let key = crate::components::scroll_area::scroll_hover_mode_signal_key(rest);
                let mode = crate::core::reactivity::use_signal(&key, || "scrollbar".to_string()).get();
                if mode.eq_ignore_ascii_case("area") {
                    hover_rest = Some(rest.clone());
                }
            }
        }

        let scrollbar_drag_update: Option<(String, super::scroll_support::ScrollAxis, f32)> =
            if let Some(drag) = self.scrollbar_drag.as_ref() {
                let cursor_axis = match drag.axis {
                    super::scroll_support::ScrollAxis::X => x,
                    super::scroll_support::ScrollAxis::Y => y,
                };
                let span = (drag.track_len - drag.thumb_len).max(1.0);
                let thumb_pos = (cursor_axis - drag.track_start - drag.grab_offset).clamp(0.0, span);
                let t = (thumb_pos / span).clamp(0.0, 1.0);
                Some((drag.rest.clone(), drag.axis, drag.max_offset * t))
            } else {
                None
            };

        let drag_sel_update: Option<(String, usize, usize)> =
            if self.left_button_down && self.scrollbar_drag.is_none() {
                if let Some(rest) = self.input_drag_rest.as_ref() {
                    if let Some((bounds, display_el, style, leaf_scroll)) =
                        super::tree_hit::find_input_text_leaf_for_rest(&state.root_element, None, rest)
                    {
                        let vk = crate::components::input::input_value_signal_key(rest);
                        let pk = crate::components::input::input_preedit_signal_key(rest);
                        let committed = crate::core::reactivity::use_signal(&vk, String::new).get();
                        let pre = crate::core::reactivity::use_signal(&pk, String::new).get();
                        let ak = crate::components::input::input_ime_anchor_signal_key(rest);
                        let ime_anchor =
                            crate::core::reactivity::use_signal(&ak, || None::<usize>).get();
                        let display_owned = if committed.is_empty() && pre.is_empty() {
                            display_el.clone()
                        } else {
                            super::tree_hit::input_display_text(&committed, &pre, ime_anchor)
                        };
                        let ui_scale = scale.max(1.0);
                        let mx = (x + leaf_scroll[0]).clamp(bounds[0], bounds[0] + bounds[2]);
                        let my = (y + leaf_scroll[1]).clamp(bounds[1], bounds[1] + bounds[3]);
                        let ix = crate::renderer::hit_test_input_text_index(
                            &mut state.text_renderer.font_system,
                            &display_owned,
                            mx,
                            my,
                            bounds,
                            &style,
                            ui_scale,
                        );
                        let caret_ix = if committed.is_empty() && pre.is_empty() { 0 } else { ix };
                        let lo = self.input_drag_anchor.min(caret_ix);
                        let hi = self.input_drag_anchor.max(caret_ix);
                        Some((rest.clone(), lo, hi))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

        if self.left_button_down && self.scrollbar_drag.is_none() {
            if let Some(drag) = self.drag_reorder.as_mut() {
                // 指针不在本列表任一拖拽手柄上时，回到「源行」下标，避免 hover 卡在最后一根有效行。
                let mut new_hover = drag.from_index;
                if let Some(fid) = super::tree_hit::find_focus_id_at(&state.root_element, x, y, [0.0, 0.0]) {
                    if let Some((lid, idx)) = crate::utils::drag_reorder::parse_drag_item_focus_id(&fid) {
                        if lid == drag.list_id {
                            new_hover = idx;
                        }
                    }
                }
                if drag.hover_index != Some(new_hover) {
                    drag.hover_index = Some(new_hover);
                    let hk = crate::utils::drag_reorder::drag_hover_signal_key(&drag.list_id);
                    crate::core::reactivity::use_signal(&hk, || None::<usize>).set(Some(new_hover));
                    window.request_redraw();
                }
            }
        }

        let cursor_pick = super::tree_hit::pick_cursor(&state.root_element, x, y, [0.0, 0.0]);
        let mut to_run = Vec::new();
        super::tree_hit::update_hovers(&state.root_element, x, y, [0.0, 0.0], &mut to_run);

        
        if let Some(cursor) = cursor_pick {
            window.set_cursor(match cursor {
                crate::core::tree::CursorKind::Pointer => winit::window::CursorIcon::Pointer,
                crate::core::tree::CursorKind::Text => winit::window::CursorIcon::Text,
                crate::core::tree::CursorKind::NotAllowed => winit::window::CursorIcon::NotAllowed,
                _ => winit::window::CursorIcon::Default,
            });
        }

        drop(state_borrow);

        // Apply hover state after releasing the state borrow.
        // 1) Clear previous axis-hover if changed
        let mut hover_state_changed = false;
        if self.hover_scroll_axis != hover_axis {
            if let Some((prev_rest, prev_axis)) = self.hover_scroll_axis.take() {
                let k = Self::hover_key(&prev_rest, prev_axis);
                crate::core::reactivity::use_signal(&k, || false).set(false);
                hover_state_changed = true;
            }
            if let Some((r, a)) = hover_axis.clone() {
                let k = Self::hover_key(&r, a);
                crate::core::reactivity::use_signal(&k, || false).set(true);
                hover_state_changed = true;
            }
            self.hover_scroll_axis = hover_axis;
        } else if let Some((r, a)) = hover_axis.as_ref() {
            let k = Self::hover_key(r, *a);
            crate::core::reactivity::use_signal(&k, || false).set(true);
        }

        // 2) Scroll-area hover (keeps both eligible, but still axis-specific visibility depends on can_scroll).
        if hover_rest != self.hover_scroll_rest {
            if let Some(prev) = self.hover_scroll_rest.take() {
                let k = format!("__rui_scroll_hover_{}", prev);
                crate::core::reactivity::use_signal(&k, || false).set(false);
                hover_state_changed = true;
            }
            if let Some(rest) = hover_rest.clone() {
                let k = format!("__rui_scroll_hover_{}", rest);
                crate::core::reactivity::use_signal(&k, || false).set(true);
                hover_state_changed = true;
                if let Some(st) = (&mut *self.state.borrow_mut()).as_mut() {
                    st.window.request_redraw();
                }
            }
            self.hover_scroll_rest = hover_rest;
        } else if let Some(rest) = hover_rest.as_ref() {
            // Keep it latched on while hovering.
            let k = format!("__rui_scroll_hover_{}", rest);
            crate::core::reactivity::use_signal(&k, || false).set(true);
        }

        // Hover-driven visibility must trigger at least one redraw; otherwise scrollbar spring
        // won't advance from hidden alpha until another unrelated redraw happens.
        if hover_state_changed {
            if let Some(st) = (&mut *self.state.borrow_mut()).as_mut() {
                st.window.request_redraw();
            }
        }

        if let Some((rest, axis, new_offset)) = scrollbar_drag_update {
            self.bump_scrollbar_visible_until_axis(&rest, axis, std::time::Instant::now());
            match axis {
                super::scroll_support::ScrollAxis::X => super::scroll_support::set_scroll_offset_with_behavior(
                    &rest,
                    new_offset,
                    crate::core::reactivity::use_signal(
                        &crate::components::scroll_area::scroll_offset_y_signal_key(&rest),
                        || 0.0_f32,
                    )
                    .get(),
                    true,
                ),
                super::scroll_support::ScrollAxis::Y => super::scroll_support::set_scroll_offset_with_behavior(
                    &rest,
                    crate::core::reactivity::use_signal(
                        &crate::components::scroll_area::scroll_offset_x_signal_key(&rest),
                        || 0.0_f32,
                    )
                    .get(),
                    new_offset,
                    true,
                ),
            }
            if let Some(st) = (&mut *self.state.borrow_mut()).as_mut() {
                st.window.request_redraw();
            }
        }
        if let Some((rest, lo, hi)) = drag_sel_update {
            let sk = crate::components::input::input_selection_signal_key(&rest);
            crate::core::reactivity::use_signal(&sk, || None::<(usize, usize)>).set(Some((lo, hi)));
        }
        for (cb, is_hover) in to_run {
            (&mut *cb.borrow_mut())(is_hover);
        }
        if crate::primitives::interactive::take_hover_paint_dirty() {
            window.request_redraw();
        }
        match cursor_pick {
            Some(k) => window.set_cursor(super::tree_hit::cursor_kind_to_winit(k)),
            None => window.set_cursor(CursorIcon::Default),
        }
    }

    pub(super) fn handle_cursor_left(&mut self, id: WindowId) {
        let mut state_borrow = self.state.borrow_mut();
        let state = match (&mut *state_borrow).as_mut() {
            Some(s) if s.window.id() == id => s,
            _ => return,
        };
        let window = state.window.clone();
        drop(state_borrow);

        let mut changed = false;
        if let Some((prev_rest, prev_axis)) = self.hover_scroll_axis.take() {
            let k = Self::hover_key(&prev_rest, prev_axis);
            crate::core::reactivity::use_signal(&k, || false).set(false);
            changed = true;
        }
        if let Some(prev) = self.hover_scroll_rest.take() {
            let k = format!("__rui_scroll_hover_{}", prev);
            crate::core::reactivity::use_signal(&k, || false).set(false);
            changed = true;
        }
        if changed {
            window.request_redraw();
        }
    }

    pub(super) fn handle_mouse_input(&mut self, id: WindowId, element_state: ElementState) {
        let mut state_borrow = self.state.borrow_mut();
        let state = match (&mut *state_borrow).as_mut() {
            Some(s) if s.window.id() == id => s,
            _ => return,
        };

        let scale = state.window.scale_factor() as f32;
        let x = self.cursor_pos.x as f32 / scale.max(1.0);
        let y = self.cursor_pos.y as f32 / scale.max(1.0);

        let mut input_drag: Option<(String, usize)> = None;
        if element_state == ElementState::Pressed {
            if let Some((rest, bounds, display_el, style, leaf_scroll)) =
                super::tree_hit::find_input_text_at(&state.root_element, None, x, y, [0.0, 0.0])
            {
                let ui_scale = scale.max(1.0);
                let ix = crate::renderer::hit_test_input_text_index(
                    &mut state.text_renderer.font_system,
                    &display_el,
                    x + leaf_scroll[0],
                    y + leaf_scroll[1],
                    bounds,
                    &style,
                    ui_scale,
                );
                let vk = crate::components::input::input_value_signal_key(&rest);
                let pk = crate::components::input::input_preedit_signal_key(&rest);
                let committed = crate::core::reactivity::use_signal(&vk, String::new).get();
                let pre = crate::core::reactivity::use_signal(&pk, String::new).get();
                let caret_ix = if committed.is_empty() && pre.is_empty() { 0 } else { ix };
                input_drag = Some((rest, caret_ix));
            }
        }

        if element_state == ElementState::Pressed {
            if let Some(fid) = super::tree_hit::find_focus_id_at(&state.root_element, x, y, [0.0, 0.0]) {
                if let Some((rest, axis, is_thumb)) = super::scroll_support::parse_scrollbar_focus_id(&fid) {
                    let metrics = self.scroll_metrics_cache.get(&rest).copied();
                    if let Some((_area_bounds, [max_x, max_y], _)) =
                        metrics
                    {
                        let axis_ch = match axis {
                            super::scroll_support::ScrollAxis::X => "x",
                            super::scroll_support::ScrollAxis::Y => "y",
                        };
                        let track_fid = format!(
                            "{}{}:{}",
                            crate::components::scrollbar::SCROLLBAR_TRACK_FOCUS_PREFIX,
                            rest,
                            axis_ch
                        );
                        let thumb_fid = format!(
                            "{}{}:{}",
                            crate::components::scrollbar::SCROLLBAR_THUMB_FOCUS_PREFIX,
                            rest,
                            axis_ch
                        );
                         if let (Some((track_bounds, track_scroll)), Some((thumb_bounds, thumb_scroll))) = (
                            super::tree_hit::find_bounds_by_focus_id(&state.root_element, &track_fid, [0.0, 0.0]),
                            super::tree_hit::find_bounds_by_focus_id(&state.root_element, &thumb_fid, [0.0, 0.0]),
                        ) {
                            let (track_start, track_len, thumb_len, max_offset, cursor_axis, thumb_start) =
                                match axis {
                                    super::scroll_support::ScrollAxis::X => (
                                        track_bounds[0] - track_scroll[0],
                                        track_bounds[2].max(1.0),
                                        thumb_bounds[2].max(1.0),
                                        max_x,
                                        x,
                                        thumb_bounds[0] - thumb_scroll[0],
                                    ),
                                    super::scroll_support::ScrollAxis::Y => (
                                        track_bounds[1] - track_scroll[1],
                                        track_bounds[3].max(1.0),
                                        thumb_bounds[3].max(1.0),
                                        max_y,
                                        y,
                                        thumb_bounds[1] - thumb_scroll[1],
                                    ),
                                };
                            drop(state_borrow);
                            let focus_visible =
                                crate::core::reactivity::use_signal("__rui_focus_visible", || false);
                            focus_visible.set(false);
                            if is_thumb {
                                self.scrollbar_drag = Some(super::scroll_support::ScrollbarDragState {
                                    rest,
                                    axis,
                                    track_start,
                                    track_len,
                                    thumb_len,
                                    max_offset,
                                    grab_offset: (cursor_axis - thumb_start).clamp(0.0, thumb_len),
                                });
                            } else {
                                let span = (track_len - thumb_len).max(1.0);
                                let thumb_pos =
                                    (cursor_axis - track_start - thumb_len * 0.5).clamp(0.0, span);
                                let t = (thumb_pos / span).clamp(0.0, 1.0);
                                let target = max_offset * t;
                                let behavior_key =
                                    crate::components::scroll_area::scroll_behavior_signal_key(&rest);
                                let behavior = crate::core::reactivity::use_signal(&behavior_key, || {
                                    "auto".to_string()
                                })
                                .get();
                                let immediate = !behavior.eq_ignore_ascii_case("smooth");
                                match axis {
                                    super::scroll_support::ScrollAxis::X => {
                                        let cur_y = crate::core::reactivity::use_signal(
                                            &crate::components::scroll_area::scroll_offset_y_signal_key(
                                                &rest,
                                            ),
                                            || 0.0_f32,
                                        )
                                        .get();
                                        super::scroll_support::set_scroll_offset_with_behavior(
                                            &rest,
                                            target,
                                            cur_y,
                                            immediate,
                                        );
                                    }
                                    super::scroll_support::ScrollAxis::Y => {
                                        let cur_x = crate::core::reactivity::use_signal(
                                            &crate::components::scroll_area::scroll_offset_x_signal_key(
                                                &rest,
                                            ),
                                            || 0.0_f32,
                                        )
                                        .get();
                                        super::scroll_support::set_scroll_offset_with_behavior(
                                            &rest,
                                            cur_x,
                                            target,
                                            immediate,
                                        );
                                    }
                                }
                            }
                            if let Some(st) = (&mut *self.state.borrow_mut()).as_mut() {
                                st.window.request_redraw();
                            }
                            return;
                        }
                    }
                }
            }
            if self.scrollbar_drag.is_none() && self.drag_reorder.is_none() {
                if let Some(fid) = super::tree_hit::find_focus_id_at(&state.root_element, x, y, [0.0, 0.0]) {
                    if let Some((list_id, idx)) = crate::utils::drag_reorder::parse_drag_item_focus_id(&fid) {
                        drop(state_borrow);
                        self.drag_reorder = Some(crate::utils::drag_reorder::DragReorderState {
                            list_id: list_id.clone(),
                            from_index: idx,
                            hover_index: Some(idx),
                        });
                        let hk = crate::utils::drag_reorder::drag_hover_signal_key(&list_id);
                        crate::core::reactivity::use_signal(&hk, || None::<usize>).set(Some(idx));
                        let focus_visible = crate::core::reactivity::use_signal("__rui_focus_visible", || false);
                        focus_visible.set(false);
                        self.left_button_down = true;
                        if let Some(st) = (&mut *self.state.borrow_mut()).as_mut() {
                            st.window.request_redraw();
                        }
                        return;
                    }
                }
            }

            let cb_to_run = super::tree_hit::find_click(&state.root_element, x, y, [0.0, 0.0]);
            drop(state_borrow);
            let focus_visible = crate::core::reactivity::use_signal("__rui_focus_visible", || false);
            focus_visible.set(false);
            // Focus chrome is applied in render via `peek`; without a redraw, click-to-clear / click-to-focus
            // would lag until the next unrelated frame.
            let mut need_paint_sync_redraw = true;

            if let Some(cb) = cb_to_run {
                (&mut *cb.borrow_mut())();
            }

            if let Some((rest, caret_ix)) = input_drag {
                let ck = crate::components::input::input_caret_signal_key(&rest);
                let sk = crate::components::input::input_selection_signal_key(&rest);
                crate::core::reactivity::use_signal(&ck, || None::<usize>).set(Some(caret_ix));
                crate::core::reactivity::use_signal(&sk, || None::<(usize, usize)>)
                    .set(Some((caret_ix, caret_ix)));
                self.input_drag_rest = Some(rest);
                self.input_drag_anchor = caret_ix;
                self.left_button_down = true;
                need_paint_sync_redraw = true;
            }

            if need_paint_sync_redraw {
                if let Some(st) = (&mut *self.state.borrow_mut()).as_mut() {
                    st.window.request_redraw();
                }
            }
        } else {
            self.left_button_down = false;
            self.scrollbar_drag = None;
            if let Some(st_drag) = self.drag_reorder.take() {
                let hk = crate::utils::drag_reorder::drag_hover_signal_key(&st_drag.list_id);
                crate::core::reactivity::use_signal(&hk, || None::<usize>).set(None);
                let h = st_drag.hover_index.unwrap_or(st_drag.from_index);
                if h != st_drag.from_index {
                    let ck = crate::utils::drag_reorder::drag_commit_signal_key(&st_drag.list_id);
                    crate::core::reactivity::use_signal(&ck, || None::<(usize, usize)>)
                        .set(Some((st_drag.from_index, h)));
                    if let Some(st) = (&mut *self.state.borrow_mut()).as_mut() {
                        st.window.request_redraw();
                    }
                }
            }
            drop(state_borrow);
            if let Some(rest) = self.input_drag_rest.take() {
                let sk = crate::components::input::input_selection_signal_key(&rest);
                let sel = crate::core::reactivity::use_signal(&sk, || None::<(usize, usize)>).get();
                if let Some((a, b)) = sel {
                    if a == b {
                        crate::core::reactivity::use_signal(&sk, || None::<(usize, usize)>).set(None);
                    }
                }
            }
        }
    }
    pub(super) fn handle_mouse_wheel(&mut self, id: WindowId, delta: MouseScrollDelta) {
        #[cfg(debug_assertions)]
        let wheel_start = std::time::Instant::now();
        let lines = super::scroll_support::get_system_scroll_lines();
        let line_height = 25.0; // Base line height for scrolling
        let (dx, dy) = match delta {
            MouseScrollDelta::LineDelta(dx, dy) => (dx * lines * line_height, dy * lines * line_height),
            MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
        };

        let mut state_borrow = self.state.borrow_mut();
        let state = match (&mut *state_borrow).as_mut() {
            Some(s) if s.window.id() == id => s,
            _ => return,
        };

        let scale_factor = state.window.scale_factor() as f32;
        let x = self.cursor_pos.x as f32 / scale_factor.max(1.0);
        let y = self.cursor_pos.y as f32 / scale_factor.max(1.0);

        let mut target_rests = Vec::new();
        super::scroll_support::collect_scroll_rest_chain_at(
            &state.root_element,
            x,
            y,
            [0.0, 0.0],
            &mut target_rests,
        );

        let metrics_for_rest = |rest: &str| {
            self.scroll_metrics_cache
                .get(rest)
                .copied()
        };

        // When wheel happens over a scrollbar track/thumb, prioritize that target scroll area.
        // This fixes cases where hit-chain returns an unrelated ancestor (or none), causing
        // wheel input to be ignored while cursor is on the scrollbar itself.
        if let Some(fid) = super::tree_hit::find_focus_id_at(&state.root_element, x, y, [0.0, 0.0]) {
            if let Some((rest, _, _)) = super::scroll_support::parse_scrollbar_focus_id(&fid) {
                if metrics_for_rest(&rest).is_some() {
                    target_rests.retain(|r| r != &rest);
                    target_rests.insert(0, rest);
                }
            }
        }

        // We'll capture at most one scroll area (deepest first); metrics are loaded lazily
        // to avoid precomputing extent for every ancestor on each wheel tick.
        let mut captured: Option<(String, f32, f32, f32, f32)> = None;
        let candidate_count = target_rests.len();
        for rest in target_rests {
            let Some((_, [max_x, max_y], _)) = metrics_for_rest(&rest) else {
                continue;
            };
            let keys = crate::components::scroll_area::signal_keys_for_rest_cached(
                &rest,
                &mut self.scroll_signal_key_cache,
            );
            let sx = crate::core::reactivity::use_signal(keys.offset_x.as_str(), || 0.0_f32).peek();
            let sy = crate::core::reactivity::use_signal(keys.offset_y.as_str(), || 0.0_f32).peek();
            let bx = crate::core::reactivity::use_signal(keys.target_x.as_str(), || sx).peek();
            let by = crate::core::reactivity::use_signal(keys.target_y.as_str(), || sy).peek();

            // Scroll Trapping: If the scroll area can move at all in the requested direction,
            // it consumes the entire delta and we stop bubbling to parents.
            let can_move_x = (dx > 0.0 && bx > 0.0) || (dx < 0.0 && bx < max_x);
            let can_move_y = (dy > 0.0 && by > 0.0) || (dy < 0.0 && by < max_y);

            if can_move_x || can_move_y {
                captured = Some((rest, dx, dy, max_x, max_y));
                break;
            }
        }

        let window = state.window.clone();
        drop(state_borrow);
        if let Some((rest, dx, dy, max_x, max_y)) = captured {
            // Show scrollbar immediately and keep it visible for a while.
            let now = std::time::Instant::now();
            if dx.abs() > 0.1 {
                self.bump_scrollbar_visible_until_axis(&rest, super::scroll_support::ScrollAxis::X, now);
            }
            if dy.abs() > 0.1 {
                self.bump_scrollbar_visible_until_axis(&rest, super::scroll_support::ScrollAxis::Y, now);
            }

            // Smooth policy (per requirement): wheel updates target; content animates toward it (no sudden jumps).
            let (offset_x_key, offset_y_key, target_x_key, target_y_key, behavior_key, activity_key) = {
                let keys = crate::components::scroll_area::signal_keys_for_rest_cached(
                    &rest,
                    &mut self.scroll_signal_key_cache,
                );
                (
                    keys.offset_x.clone(),
                    keys.offset_y.clone(),
                    keys.target_x.clone(),
                    keys.target_y.clone(),
                    keys.behavior.clone(),
                    keys.activity.clone(),
                )
            };
            let behavior = crate::core::reactivity::use_signal(&behavior_key, || "auto".to_string())
                .peek();
            let is_smooth_mode = behavior.eq_ignore_ascii_case("smooth");

            let sx = crate::core::reactivity::use_signal(&offset_x_key, || 0.0_f32);
            let sy = crate::core::reactivity::use_signal(&offset_y_key, || 0.0_f32);
            let tx = crate::core::reactivity::use_signal(&target_x_key, || sx.peek());
            let ty = crate::core::reactivity::use_signal(&target_y_key, || sy.peek());

            if is_smooth_mode {
                let next_x = (tx.get() - dx).clamp(0.0, max_x);
                let next_y = (ty.get() - dy).clamp(0.0, max_y);
                tx.set_silent(next_x);
                ty.set_silent(next_y);
                self.bump_smooth_active_until(&rest, now);

                // Wheel moves `tx`/`ty` only; each frame `sx`/`sy` ease toward targets (exponential).
                // Do not add wheel impulse to `scroll_velocity`: that fought the old spring and is
                // unnecessary for the current smoother.

                crate::core::reactivity::use_signal(&activity_key, || 0.0_f32).set_silent(1.0);
                if dx.abs() > 0.1 {
                    Self::maybe_wheel_bump_scrollbar_spring(&rest, super::scroll_support::ScrollAxis::X);
                }
                if dy.abs() > 0.1 {
                    Self::maybe_wheel_bump_scrollbar_spring(&rest, super::scroll_support::ScrollAxis::Y);
                }
            } else {
                // Non-smooth: keep existing target-based accumulation behavior.
                let rest_str = rest.clone();
                let entry = self.scroll_accum.entry(rest_str).or_insert([0.0, 0.0]);
                entry[0] += dx;
                entry[1] += dy;
                if dx.abs() > 0.1 {
                    Self::maybe_wheel_bump_scrollbar_spring(&rest, super::scroll_support::ScrollAxis::X);
                }
                if dy.abs() > 0.1 {
                    Self::maybe_wheel_bump_scrollbar_spring(&rest, super::scroll_support::ScrollAxis::Y);
                }
            }

            #[cfg(debug_assertions)]
            {
                let elapsed_ms = wheel_start.elapsed().as_millis();
                if elapsed_ms > 2 {
                    log::warn!(
                        "handle_mouse_wheel slow: {}ms, candidates={}, captured=1",
                        elapsed_ms,
                        candidate_count
                    );
                }
            }
            window.request_redraw();
            return;
        }

        #[cfg(debug_assertions)]
        {
            let elapsed_ms = wheel_start.elapsed().as_millis();
            if elapsed_ms > 2 {
                log::warn!(
                    "handle_mouse_wheel slow: {}ms, candidates={}, captured=0",
                    elapsed_ms,
                    candidate_count
                );
            }
        }
        window.request_redraw();
    }

    pub(super) fn handle_redraw_requested(&mut self, id: WindowId, event_loop: &ActiveEventLoop) {
        let mut state_borrow = self.state.borrow_mut();
        let state = match (&mut *state_borrow).as_mut() {
            Some(s) if s.window.id() == id => s,
            _ => return,
        };
        #[cfg(debug_assertions)]
        {
            let hot_reload = self.hot_reload.drain_pending();
            if hot_reload.count > 0 {
                let assets_reloaded = state.reload_wgsl_assets(&hot_reload.asset_paths);

                let merged = &hot_reload.merged_templates;
                let has_templates = !merged.is_empty();
                let ids: Vec<String> = merged.iter().map(|v| v.id.clone()).collect();
                let template_match =
                    has_templates && super::hot_reload::has_any_template_id(&state.root_element, &ids);

                if !hot_reload.asset_paths.is_empty() {
                    log::warn!(
                        "hot-reload asset paths={} reloaded={}",
                        hot_reload.asset_paths.len(),
                        assets_reloaded
                    );
                }

                if has_templates {
                    if !hot_reload.last_patch_file.is_empty() {
                        log::warn!(
                            "hot-reload merged from file={} unique_templates={}",
                            hot_reload.last_patch_file,
                            merged.len()
                        );
                    }
                    if let Some(first) = merged.first() {
                        log::warn!(
                            "hot-reload first template id={} hash={} len={}",
                            first.id,
                            first.hash,
                            first.len
                        );
                    }
                    let replacement_count: usize = merged.iter().map(|t| t.text_replacements.len()).sum();
                    let style_count: usize = merged.iter().map(|t| t.style_attrs.len()).sum();
                    log::warn!(
                        "hot-reload payload replacements={} styles={}",
                        replacement_count,
                        style_count
                    );
                    let literals_count: usize = merged.iter().map(|t| t.text_literals.len()).sum();
                    log::warn!("hot-reload payload text_literals={}", literals_count);
                }

                let mut requested_redraw = assets_reloaded;

                if hot_reload.force_reload {
                    let new_root = (self.app_builder.borrow_mut())();
                    state.rebuild_ui(new_root);
                    requested_redraw = true;
                } else if template_match {
                    let new_root = (self.app_builder.borrow_mut())();
                    let injected = state.inject_hot_reload_partial(new_root, merged);
                    log::warn!("hot-reload injected={}", injected);
                    if !injected {
                        let new_root = (self.app_builder.borrow_mut())();
                        state.rebuild_ui(new_root);
                    }
                    requested_redraw = true;
                } else if has_templates {
                    log::warn!("hot-reload patch skipped: no template id matched current UI tree");
                    let mut existing_ids = Vec::new();
                    super::hot_reload::collect_template_ids(&state.root_element, &mut existing_ids);
                    if !existing_ids.is_empty() {
                        let preview = existing_ids.into_iter().take(5).collect::<Vec<_>>().join(", ");
                        log::warn!("hot-reload existing template ids (preview): {}", preview);
                    }
                }

                if requested_redraw {
                    state.window.request_redraw();
                    self.force_redraw_count = self.force_redraw_count.max(2);
                }
            }
        }

        let inner_sz = state.window.inner_size();
        let suspended = self.is_suspended;
        let win_for_retry = state.window.clone();
        if suspended || inner_sz.width == 0 || inner_sz.height == 0 {
            drop(state_borrow);
            if !suspended && (inner_sz.width == 0 || inner_sz.height == 0) {
                win_for_retry.request_redraw();
            }
            return;
        }

        let now = std::time::Instant::now();
        let dt = now.duration_since(self.last_frame_time).as_secs_f32();
        self.last_frame_time = now;
        // Avoid α→0 on duplicate redraws / timer quirks; cap so a long pause does not “teleport”.
        let scroll_dt = dt.clamp(1.0 / 240.0, 0.08);

        let window = state.window.clone();
        drop(state_borrow);

        let animations_running = crate::style::tick_animations(dt);
        if animations_running {
            window.request_redraw();
        }
        let mut smooth_scroll_animating = false;

        let mut accum_ops = Vec::new();
        if !self.scroll_accum.is_empty() {
            for (rest, [adx, ady]) in self.scroll_accum.drain() {
                let max_pair = self
                    .scroll_metrics_cache
                    .get(&rest)
                    .map(|(_, [mx, my], _)| (*mx, *my));
                if let Some((max_x, max_y)) = max_pair {
                    accum_ops.push((rest, adx, ady, max_x, max_y));
                }
            }
        }

        for (rest, adx, ady, max_x, max_y) in accum_ops {
            let sx_key = crate::components::scroll_area::scroll_offset_x_signal_key(&rest);
            let sy_key = crate::components::scroll_area::scroll_offset_y_signal_key(&rest);
            let tx_key = crate::components::scroll_area::scroll_target_x_signal_key(&rest);
            let ty_key = crate::components::scroll_area::scroll_target_y_signal_key(&rest);
            let sx_sig = crate::core::reactivity::use_signal(&sx_key, || 0.0_f32);
            let sy_sig = crate::core::reactivity::use_signal(&sy_key, || 0.0_f32);
            let tx = crate::core::reactivity::use_signal(&tx_key, || sx_sig.peek());
            let ty = crate::core::reactivity::use_signal(&ty_key, || sy_sig.peek());
            let tx_cur = tx.peek();
            let ty_cur = ty.peek();
            let nx = (tx_cur - adx).clamp(0.0, max_x);
            let ny = (ty_cur - ady).clamp(0.0, max_y);
            if (nx - tx_cur).abs() > 0.01 { tx.set_silent(nx); }
            if (ny - ty_cur).abs() > 0.01 { ty.set_silent(ny); }
            if adx.abs() > 0.1 || ady.abs() > 0.1 {
                crate::core::reactivity::use_signal(&crate::components::scroll_area::scroll_activity_signal_key(&rest), || 0.0_f32).set_silent(1.0);
            }
        }

        // First-order exponential smoothing toward wheel targets — no velocity ring / overshoot
        // (previous mass–spring used ζ≈0.98, slightly under-damped). τ chosen near Chromium smooth-scroll.
        const SMOOTH_SCROLL_TAU_SEC: f32 = 0.095;
        /// Cap per-frame blend so a long `dt` (e.g. first frame after idle) or a single wheel tick
        /// never moves more than ~this fraction of remaining distance in one frame — avoids a visible
        /// “jump” on one tick; nested outer scroll (large range) benefits most.
        const SMOOTH_SCROLL_MAX_ALPHA: f32 = 0.20;
        const SMOOTH_SCROLL_SNAP_PX: f32 = 0.20;
        /// Keep requesting redraws until this close to target (slightly tighter than before).
        const SMOOTH_SCROLL_ANIM_EPS: f32 = 0.06;
        const SMOOTH_SCROLL_EDGE_SNAP_PX: f32 = 0.45;

        let mut pre_render_data = Vec::new();
        for (rest, (_, [mx, my], _)) in &self.scroll_metrics_cache {
            let keys = crate::components::scroll_area::signal_keys_for_rest_cached(
                rest,
                &mut self.scroll_signal_key_cache,
            );
            let sx = crate::core::reactivity::use_signal(keys.offset_x.as_str(), || 0.0_f32);
            let sy = crate::core::reactivity::use_signal(keys.offset_y.as_str(), || 0.0_f32);
            let tx = crate::core::reactivity::use_signal(keys.target_x.as_str(), || sx.peek());
            let ty = crate::core::reactivity::use_signal(keys.target_y.as_str(), || sy.peek());
            let behavior =
                crate::core::reactivity::use_signal(keys.behavior.as_str(), || "auto".to_string()).peek();
            let old_sx = sx.peek();
            let old_sy = sy.peek();
            let raw_max_x = mx.max(0.0);
            let raw_max_y = my.max(0.0);
            let lp = self.scroll_max_lowpass.entry(rest.clone()).or_insert([raw_max_x, raw_max_y]);
            let target_x_lp = tx.peek().clamp(0.0, lp[0].min(raw_max_x));
            let target_y_lp = ty.peek().clamp(0.0, lp[1].min(raw_max_y));

            let is_dragging_this = self.scrollbar_drag.as_ref().is_some_and(|d| d.rest == *rest);

            if behavior.eq_ignore_ascii_case("smooth") && !is_dragging_this {
                // Clamp to raw extent only: low-pass max lags behind during layout changes and can
                // fight the smooth animator (perceived stutter).
                let mut target_x = tx.peek().clamp(0.0, raw_max_x);
                let mut target_y = ty.peek().clamp(0.0, raw_max_y);
                // Edge snap: avoid tiny residual oscillation near 0/max bounds.
                if target_x <= SMOOTH_SCROLL_EDGE_SNAP_PX {
                    target_x = 0.0;
                } else if (raw_max_x - target_x).abs() <= SMOOTH_SCROLL_EDGE_SNAP_PX {
                    target_x = raw_max_x;
                }
                if target_y <= SMOOTH_SCROLL_EDGE_SNAP_PX {
                    target_y = 0.0;
                } else if (raw_max_y - target_y).abs() <= SMOOTH_SCROLL_EDGE_SNAP_PX {
                    target_y = raw_max_y;
                }
                // Nested ScrollArea: skip work for areas already at target (outer idle while inner scrolls).
                const SMOOTH_IDLE_EPS: f32 = 0.04;
                if (target_x - old_sx).abs() < SMOOTH_IDLE_EPS && (target_y - old_sy).abs() < SMOOTH_IDLE_EPS {
                    pre_render_data.push((rest.clone(), old_sx, old_sy, old_sx, old_sy, sx, sy));
                    continue;
                }
                let raw_alpha = 1.0 - (-scroll_dt / SMOOTH_SCROLL_TAU_SEC).exp();
                let alpha = raw_alpha.min(SMOOTH_SCROLL_MAX_ALPHA);
                let mut cur_x = old_sx + (target_x - old_sx) * alpha;
                let mut cur_y = old_sy + (target_y - old_sy) * alpha;
                if (target_x - cur_x).abs() < SMOOTH_SCROLL_SNAP_PX {
                    cur_x = target_x;
                }
                if (target_y - cur_y).abs() < SMOOTH_SCROLL_SNAP_PX {
                    cur_y = target_y;
                }
                // Tail settle: once we are very close, snap offsets+targets together to stop micro-jitter.
                if (target_x - cur_x).abs() <= SMOOTH_SCROLL_ANIM_EPS {
                    cur_x = target_x;
                    tx.set_silent(target_x);
                }
                if (target_y - cur_y).abs() <= SMOOTH_SCROLL_ANIM_EPS {
                    cur_y = target_y;
                    ty.set_silent(target_y);
                }
                if let Some(v) = self.scroll_velocity.get_mut(rest) {
                    v[0] = 0.0;
                    v[1] = 0.0;
                }
                if (target_x - cur_x).abs() > SMOOTH_SCROLL_ANIM_EPS
                    || (target_y - cur_y).abs() > SMOOTH_SCROLL_ANIM_EPS
                {
                    smooth_scroll_animating = true;
                }
                pre_render_data.push((rest.clone(), old_sx, old_sy, cur_x.clamp(0.0, raw_max_x), cur_y.clamp(0.0, raw_max_y), sx, sy));
            } else {
                if is_dragging_this {
                    if let Some(v) = self.scroll_velocity.get_mut(rest) { v[0] = 0.0; v[1] = 0.0; }
                }
                pre_render_data.push((rest.clone(), old_sx, old_sy, target_x_lp, target_y_lp, sx, sy));
            }
        }

        let mut signal_updates = Vec::new();
        {
            let mut state_borrow = self.state.borrow_mut();
            if let Some(state) = state_borrow.as_mut() {
                for (rest, old_sx, old_sy, new_sx, new_sy, sx_sig, sy_sig) in pre_render_data {
                    state.update_scroll_offset_in_place(&rest, [new_sx, new_sy]);
                    signal_updates.push((sx_sig, sy_sig, old_sx, old_sy, new_sx, new_sy));
                }
            }
        }
        // Must keep offset signals in lockstep with `update_scroll_offset_in_place`. Use silent
        // writes to avoid triggering full reactive rebuilds on every scroll animation frame.
        for (sx, sy, _old_sx, _old_sy, new_sx, new_sy) in signal_updates {
            sx.set_silent(new_sx);
            sy.set_silent(new_sy);
        }

        let mut re_borrow = match self.state.try_borrow_mut() {
            Ok(b) => b,
            Err(_) => {
                window.request_redraw();
                return;
            }
        };
        let state = re_borrow.as_mut().unwrap();

        let hx = self.cursor_pos.x as f32 / state.window.scale_factor() as f32;
        let hy = self.cursor_pos.y as f32 / state.window.scale_factor() as f32;

        // Idle frames use ViewportsOnly so `update_scrollbar_elements` only syncs `scroll:` offsets
        // (plus subtree pruning via `Element::contains_scroll_viewport`). Full pass is needed while
        // dragging, smoothing scroll, coalescing wheel delta, or — crucially — spring-animating
        // scrollbar fade-in/out (hover show / auto-hide). Without this, ViewportsOnly skips the
        // alpha/color multiplication and tracks stay frozen (invisible on hover, opaque after idle).
        let scrollbar_pass = if self.scrollbar_drag.is_some()
            || smooth_scroll_animating
            || !self.scroll_accum.is_empty()
            || animations_running
        {
            ScrollbarUpdateMode::Full
        } else {
            ScrollbarUpdateMode::ViewportsOnly
        };
        let text_force_fast_mode =
            self.scrollbar_drag.is_some() || smooth_scroll_animating || !self.scroll_accum.is_empty();

        let active_drag_rest = self.scrollbar_drag.as_ref().map(|d| d.rest.as_str());
        match state.render(
            [hx, hy],
            scrollbar_pass,
            active_drag_rest,
            text_force_fast_mode,
        ) {
            Ok(_) => {
                if let Some(adapter) = &mut self.accesskit_adapter {
                    let update = state.accesskit_tree_update();
                    adapter.update_if_active(|| update);
                }
                let scroll_metrics: Vec<_> = state.last_render_scroll_metrics.drain(..).collect();
                let mut diag_track_bounds: std::collections::HashMap<String, ([f32; 4], [f32; 4])> =
                    std::collections::HashMap::new();
                let mut diag_thumb_bounds: std::collections::HashMap<String, ([f32; 4], [f32; 4])> =
                    std::collections::HashMap::new();
                let mut diag_track_inset: std::collections::HashMap<String, (String, String)> =
                    std::collections::HashMap::new();
                let mut diag_padding: std::collections::HashMap<String, (String, String)> =
                    std::collections::HashMap::new();
                if std::env::var("RUI_SCROLLBAR_DIAG").ok().is_some() {
                    for (rest, _, _, _) in scroll_metrics.iter() {
                        let fx = format!("{SCROLLBAR_TRACK_FOCUS_PREFIX}{rest}:x");
                        let fy = format!("{SCROLLBAR_TRACK_FOCUS_PREFIX}{rest}:y");
                        let viewport_fid =
                            format!("{}{}", crate::components::scroll_area::SCROLL_AREA_FOCUS_PREFIX, rest);
                        let thx = format!("{}{}:x", crate::components::scrollbar::SCROLLBAR_THUMB_FOCUS_PREFIX, rest);
                        let thy = format!("{}{}:y", crate::components::scrollbar::SCROLLBAR_THUMB_FOCUS_PREFIX, rest);
                        let bx = super::tree_hit::find_bounds_by_focus_id(&state.root_element, &fx, [0.0, 0.0])
                            .map(|(b, s)| [b[0] - s[0], b[1] - s[1], b[2], b[3]])
                            .unwrap_or([0.0, 0.0, 0.0, 0.0]);
                        let by = super::tree_hit::find_bounds_by_focus_id(&state.root_element, &fy, [0.0, 0.0])
                            .map(|(b, s)| [b[0] - s[0], b[1] - s[1], b[2], b[3]])
                            .unwrap_or([0.0, 0.0, 0.0, 0.0]);
                        let th_bx = super::tree_hit::find_bounds_by_focus_id(&state.root_element, &thx, [0.0, 0.0])
                            .map(|(b, s)| [b[0] - s[0], b[1] - s[1], b[2], b[3]])
                            .unwrap_or([0.0, 0.0, 0.0, 0.0]);
                        let th_by = super::tree_hit::find_bounds_by_focus_id(&state.root_element, &thy, [0.0, 0.0])
                            .map(|(b, s)| [b[0] - s[0], b[1] - s[1], b[2], b[3]])
                            .unwrap_or([0.0, 0.0, 0.0, 0.0]);
                        let inset_x = super::tree_hit::find_element_by_focus_id(&state.root_element, &fx)
                            .map(|e| Self::fmt_inset_rect(e.style.inset))
                            .unwrap_or_else(|| "n/a".to_string());
                        let inset_y = super::tree_hit::find_element_by_focus_id(&state.root_element, &fy)
                            .map(|e| Self::fmt_inset_rect(e.style.inset))
                            .unwrap_or_else(|| "n/a".to_string());
                        let viewport_padding =
                            super::tree_hit::find_element_by_focus_id(&state.root_element, &viewport_fid)
                                .map(|e| Self::fmt_padding_rect(e.style.padding))
                                .unwrap_or_else(|| "n/a".to_string());
                        let outer_padding = Self::find_element_by_template_id(&state.root_element, rest)
                            .map(|e| Self::fmt_padding_rect(e.style.padding))
                            .unwrap_or_else(|| "n/a".to_string());
                        diag_track_bounds.insert(rest.clone(), (bx, by));
                        diag_thumb_bounds.insert(rest.clone(), (th_bx, th_by));
                        diag_track_inset.insert(rest.clone(), (inset_x, inset_y));
                        diag_padding.insert(rest.clone(), (viewport_padding, outer_padding));
                    }
                }
                // Duplicate `ScrollArea id` => metrics/signals collide by key, causing some
                // instances to use another area's viewport/content sizes (track length mismatch).
                // Warn once per duplicated id to surface this root cause directly.
                {
                    let mut counts: std::collections::HashMap<&str, usize> =
                        std::collections::HashMap::new();
                    for (rest, _, _, _) in scroll_metrics.iter() {
                        *counts.entry(rest.as_str()).or_insert(0) += 1;
                    }
                    for (rest, n) in counts {
                        if n > 1 && self.scroll_metric_duplicate_warned.insert(rest.to_string()) {
                            log::error!(
                                "Detected duplicated ScrollArea id '{}' ({} instances). \
All scroll signals/metrics use this id as key, so instances overwrite each other and \
scrollbar track length can become incorrect. Please make each ScrollArea id unique.",
                                rest,
                                n
                            );
                        }
                    }
                }
                self.scroll_metrics_cache = scroll_metrics
                    .iter()
                    .map(|(rest, b, max, content)| (rest.clone(), (*b, *max, *content)))
                    .collect();

                let hover_cursor_pick = state.last_hover_cursor;
                let mut hover_to_run = Vec::new();
                super::tree_hit::update_hovers(&state.root_element, hx, hy, [0.0, 0.0], &mut hover_to_run);

                if let Some(cursor) = hover_cursor_pick {
                    window.set_cursor(match cursor {
                        crate::core::tree::CursorKind::Pointer => winit::window::CursorIcon::Pointer,
                        crate::core::tree::CursorKind::Text => winit::window::CursorIcon::Text,
                        crate::core::tree::CursorKind::NotAllowed => winit::window::CursorIcon::NotAllowed,
                        _ => winit::window::CursorIcon::Default,
                    });
                }

                let focused = crate::core::reactivity::use_signal("__rui_button_focus_id", String::new);
                let fid = focused.get();
                let ime_data = if fid.starts_with(crate::components::INPUT_FOCUS_PREFIX) {
                    super::tree_hit::find_bounds_by_focus_id(&state.root_element, &fid, [0.0, 0.0])
                        .map(|(b, sa)| (b, sa, fid.clone()))
                } else {
                    None
                };

                let ui_scale = state.ui_scale();

                drop(re_borrow); // CRITICAL: Free the borrow before updating signals!

                // Post-render: low-pass scroll extents, viewport/content metrics, overlay scrollbar visibility.
                // Offsets were integrated before `render()` using metrics cached from the previous frame.
                let mut scrollbar_fade_animating = false;
                for (rest, bounds, [max_x, max_y], [content_w, content_h]) in scroll_metrics.iter() {
                    let keys = crate::components::scroll_area::signal_keys_for_rest_cached(
                        rest,
                        &mut self.scroll_signal_key_cache,
                    );
                    let raw_max_x = max_x.max(0.0);
                    let raw_max_y = max_y.max(0.0);
                    let lp = self
                        .scroll_max_lowpass
                        .entry(rest.clone())
                        .or_insert([raw_max_x, raw_max_y]);
                    // Low-pass raw extent so clamp(tx, max) does not jitter frame-to-frame (middle-scroll bounce).
                    const LP: f32 = 0.78;
                    lp[0] = lp[0] * LP + raw_max_x * (1.0 - LP);
                    lp[1] = lp[1] * LP + raw_max_y * (1.0 - LP);
                    let soft_max_x = lp[0].min(raw_max_x);
                    let soft_max_y = lp[1].min(raw_max_y);

                    let sx = crate::core::reactivity::use_signal(keys.offset_x.as_str(), || 0.0_f32);
                    let sy = crate::core::reactivity::use_signal(keys.offset_y.as_str(), || 0.0_f32);
                    let vw = crate::core::reactivity::use_signal(keys.viewport_w.as_str(), || 0.0_f32);
                    let vh = crate::core::reactivity::use_signal(keys.viewport_h.as_str(), || 0.0_f32);
                    let cw = crate::core::reactivity::use_signal(keys.content_w.as_str(), || 0.0_f32);
                    let ch = crate::core::reactivity::use_signal(keys.content_h.as_str(), || 0.0_f32);
                    let tx = crate::core::reactivity::use_signal(keys.target_x.as_str(), || sx.peek());
                    let ty = crate::core::reactivity::use_signal(keys.target_y.as_str(), || sy.peek());
                    let behavior =
                        crate::core::reactivity::use_signal(keys.behavior.as_str(), || "auto".to_string()).peek();
                    let is_smooth_mode = behavior.eq_ignore_ascii_case("smooth");
                    let target_x = if is_smooth_mode {
                        tx.peek().clamp(0.0, soft_max_x)
                    } else {
                        tx.peek().clamp(0.0, raw_max_x)
                    };
                    let target_y = if is_smooth_mode {
                        ty.peek().clamp(0.0, soft_max_y)
                    } else {
                        ty.peek().clamp(0.0, raw_max_y)
                    };
                    let new_sx = sx.peek();
                    let new_sy = sy.peek();

                    let overflow_x =
                        crate::core::reactivity::use_signal(keys.overflow_x.as_str(), || "auto".to_string()).peek();
                    let overflow_y =
                        crate::core::reactivity::use_signal(keys.overflow_y.as_str(), || "auto".to_string()).peek();
                    let axis_x_enabled =
                        crate::components::scroll_area::scroll_overflow_allows_scrollbar(overflow_x.as_str());
                    let axis_y_enabled =
                        crate::components::scroll_area::scroll_overflow_allows_scrollbar(overflow_y.as_str());
                    let can_scroll_x = axis_x_enabled && *content_w > bounds[2] + 0.5;
                    let can_scroll_y = axis_y_enabled && *content_h > bounds[3] + 0.5;

                    // Optional deep diagnostics:
                    // set env `RUI_SCROLLBAR_DIAG=1` (all) or `RUI_SCROLLBAR_DIAG=<id-substring>` (filtered).
                    if let Ok(diag) = std::env::var("RUI_SCROLLBAR_DIAG") {
                        let diag = diag.trim();
                        if !diag.is_empty() && (diag == "1" || rest.contains(diag)) {
                            let key = rest.clone();
                            let now_log = std::time::Instant::now();
                            let should_log = self
                                .scroll_diag_last_log
                                .get(&key)
                                .map_or(true, |t| now_log.duration_since(*t).as_millis() >= 600);
                            if should_log {
                                self.scroll_diag_last_log.insert(key, now_log);
                                let (tx, ty) = diag_track_bounds
                                    .get(rest)
                                    .copied()
                                    .unwrap_or(([0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0]));
                                let (ix, iy) = diag_track_inset
                                    .get(rest)
                                    .cloned()
                                    .unwrap_or_else(|| ("n/a".to_string(), "n/a".to_string()));
                                let (vp_pad, outer_pad) = diag_padding
                                    .get(rest)
                                    .cloned()
                                    .unwrap_or_else(|| ("n/a".to_string(), "n/a".to_string()));
                                let inset_mode_x = crate::core::reactivity::use_signal(
                                    &crate::components::scroll_area::scroll_scrollbar_inset_x_signal_key(rest),
                                    || "inside".to_string(),
                                )
                                .peek();
                                let inset_mode_y = crate::core::reactivity::use_signal(
                                    &crate::components::scroll_area::scroll_scrollbar_inset_y_signal_key(rest),
                                    || "inside".to_string(),
                                )
                                .peek();
                                let side_x = crate::core::reactivity::use_signal(
                                    &crate::components::scroll_area::scroll_scrollbar_x_side_signal_key(rest),
                                    || "bottom".to_string(),
                                )
                                .peek();
                                let side_y = crate::core::reactivity::use_signal(
                                    &crate::components::scroll_area::scroll_scrollbar_y_side_signal_key(rest),
                                    || "right".to_string(),
                                )
                                .peek();
                                log::warn!(
                                    "scrollbar diag rest='{}' viewport=({:.1},{:.1}) content=({:.1},{:.1}) max=({:.1},{:.1}) overflow=({}, {}) can_scroll=({}, {}) inset_mode=({}, {}) side=({}, {}) track_x=({:.1},{:.1},{:.1},{:.1}) track_y=({:.1},{:.1},{:.1},{:.1}) track_style_inset_x={} track_style_inset_y={} viewport_padding={} outer_padding={}",
                                    rest,
                                    bounds[2],
                                    bounds[3],
                                    content_w,
                                    content_h,
                                    max_x,
                                    max_y,
                                    overflow_x,
                                    overflow_y,
                                    can_scroll_x,
                                    can_scroll_y,
                                    inset_mode_x,
                                    inset_mode_y,
                                    side_x,
                                    side_y,
                                    tx[0],
                                    tx[1],
                                    tx[2],
                                    tx[3],
                                    ty[0],
                                    ty[1],
                                    ty[2],
                                    ty[3],
                                    ix,
                                    iy,
                                    vp_pad,
                                    outer_pad
                                );
                            }
                        }
                    }

                    // Handle activity/hover: keep visible while hovering, scrolling, or animating.
                    let activity = crate::core::reactivity::use_signal(keys.activity.as_str(), || 0.0_f32);
                    let is_hover =
                        crate::core::reactivity::use_signal(keys.hover_legacy.as_str(), || false).peek();
                    let is_hover_x =
                        crate::core::reactivity::use_signal(keys.hover_axis_x.as_str(), || false).peek();
                    let is_hover_y =
                        crate::core::reactivity::use_signal(keys.hover_axis_y.as_str(), || false).peek();
                    let at_target =
                        (target_x - new_sx).abs() < 0.1 && (target_y - new_sy).abs() < 0.1;
                    let is_dragging = self
                        .scrollbar_drag
                        .as_ref()
                        .is_some_and(|d| d.rest == *rest);
                    let visible_until_x = self
                        .scrollbar_visible_until
                        .get(&Self::scrollbar_axis_key(&rest, super::scroll_support::ScrollAxis::X))
                        .copied()
                        .unwrap_or(now);
                    let visible_until_y = self
                        .scrollbar_visible_until
                        .get(&Self::scrollbar_axis_key(&rest, super::scroll_support::ScrollAxis::Y))
                        .copied()
                        .unwrap_or(now);

                    let is_dragging_x = self
                        .scrollbar_drag
                        .as_ref()
                        .is_some_and(|d| d.rest == *rest && d.axis == super::scroll_support::ScrollAxis::X);
                    let is_dragging_y = self
                        .scrollbar_drag
                        .as_ref()
                        .is_some_and(|d| d.rest == *rest && d.axis == super::scroll_support::ScrollAxis::Y);

                    // Independent per-axis visibility.
                    // - hover over content: allow both
                    // - hover over scrollbar: only that axis
                    // - wheel: axis-specific visible_until
                    let keep_visible_x = (is_hover || is_hover_x)
                        || is_dragging_x
                        || now < visible_until_x;
                    let keep_visible_y = (is_hover || is_hover_y)
                        || is_dragging_y
                        || now < visible_until_y;

                    // Keep the old activity signal (used elsewhere) as “any axis visible”.
                    let keep_visible = is_hover || is_dragging || now < visible_until_x || now < visible_until_y || !at_target;
                    let target = if keep_visible || !at_target { 1.0 } else { 0.0 };
                    if activity.get() != target {
                        activity.set(target);
                    }

                    // Update springs per-axis: only `auto` uses hover/scroll fade; `always`/`never` are fixed.
                    let vis_x = crate::core::reactivity::use_signal(keys.scrollbar_visibility_x.as_str(), || {
                        "auto".to_string()
                    })
                    .peek()
                    .to_ascii_lowercase();
                    let vis_y = crate::core::reactivity::use_signal(keys.scrollbar_visibility_y.as_str(), || {
                        "auto".to_string()
                    })
                    .peek()
                    .to_ascii_lowercase();

                    if axis_x_enabled {
                        let spring_x = crate::style::use_spring(keys.spring_x.as_str(), 0.0);
                        // `raw_max_*` can sit at 0 briefly (stale metrics / sub-pixel) while the axis is
                        // still hoverable. Never force the spring to 0 when that axis is actively hovered,
                        // dragged, or in the post-wheel visibility window — otherwise `auto` bars only
                        // appear after scrolling bumps `raw_max` above the threshold.
                        let spring_eligible_x = raw_max_x > 0.5
                            || is_hover_x
                            || is_dragging_x
                            || now < visible_until_x;
                        let target_x_vis = match vis_x.as_str() {
                            "never" => 0.0,
                            "always" => 1.0,
                            _ => {
                                if spring_eligible_x && keep_visible_x {
                                    1.0
                                } else {
                                    0.0
                                }
                            }
                        };
                        match vis_x.as_str() {
                            "never" => spring_x.set(0.0),
                            "always" => spring_x.set(1.0),
                            _ => {
                                if spring_eligible_x {
                                    spring_x.set(if keep_visible_x { 1.0 } else { 0.0 });
                                } else {
                                    spring_x.set(0.0);
                                }
                            }
                        }
                        // Match spring settle tolerance (~0.01): smaller eps misses near-end frames and
                        // can stop requesting redraws while `v` is still slightly > 0.
                        if (spring_x.peek() - target_x_vis).abs() > 0.002 {
                            scrollbar_fade_animating = true;
                        }
                    }
                    if axis_y_enabled {
                        let spring_y = crate::style::use_spring(keys.spring_y.as_str(), 0.0);
                        let spring_eligible_y = raw_max_y > 0.5
                            || is_hover_y
                            || is_dragging_y
                            || now < visible_until_y;
                        let target_y_vis = match vis_y.as_str() {
                            "never" => 0.0,
                            "always" => 1.0,
                            _ => {
                                if spring_eligible_y && keep_visible_y {
                                    1.0
                                } else {
                                    0.0
                                }
                            }
                        };
                        match vis_y.as_str() {
                            "never" => spring_y.set(0.0),
                            "always" => spring_y.set(1.0),
                            _ => {
                                if spring_eligible_y {
                                    spring_y.set(if keep_visible_y { 1.0 } else { 0.0 });
                                } else {
                                    spring_y.set(0.0);
                                }
                            }
                        }
                        if (spring_y.peek() - target_y_vis).abs() > 0.002 {
                            scrollbar_fade_animating = true;
                        }
                    }

                    let new_vw = bounds[2].max(0.0);
                    let new_vh = bounds[3].max(0.0);
                    let new_cw = content_w.max(0.0);
                    let new_ch = content_h.max(0.0);

                    // Update vw/vh/cw/ch **atomically** when any one crosses the noise threshold.
                    // Per-field updates let e.g. vh refresh while ch stays stale → `ch - vh` becomes 0,
                    // thumb jumps to the track start and `!can_scroll` collapses hit targets.
                    let pv = vw.peek();
                    let ph = vh.peek();
                    let pcw = cw.peek();
                    let pch = ch.peek();
                    let dirty = (new_vw - pv).abs() > 0.1
                        || (new_vh - ph).abs() > 0.1
                        || (new_cw - pcw).abs() > 0.1
                        || (new_ch - pch).abs() > 0.1;
                    if dirty {
                        vw.set_silent(new_vw);
                        vh.set_silent(new_vh);
                        cw.set_silent(new_cw);
                        ch.set_silent(new_ch);
                    }
                }
                self.scroll_metrics_cache = scroll_metrics
                    .into_iter()
                    .map(|(rest, b, max, content)| (rest, (b, max, content)))
                    .collect();
                // Keep the redraw pump alive when:
                // - a scrollbar spring is still settling (fade-in / fade-out animation)
                // - a `visible_until` timer is still ticking (scroll ended < 1500ms ago):
                //   once it expires we need one more frame to flip the spring target to 0
                //   and start the fade-out. Without this, the event loop goes idle and the
                //   scrollbar stays opaque until an unrelated input event triggers a redraw.
                let scrollbar_visibility_pending = self
                    .scrollbar_visible_until
                    .values()
                    .any(|&until| now < until);
                if smooth_scroll_animating || scrollbar_fade_animating || scrollbar_visibility_pending {
                    window.request_redraw();
                }

                for (cb, is_hover) in hover_to_run {
                    (&mut *cb.borrow_mut())(is_hover);
                }
                if crate::primitives::interactive::take_hover_paint_dirty() {
                    window.request_redraw();
                }
                match hover_cursor_pick {
                    Some(k) => window.set_cursor(super::tree_hit::cursor_kind_to_winit(k)),
                    None => window.set_cursor(CursorIcon::Default),
                }

                if let Some((bounds, scroll_acc, _fid_clone)) = ime_data {
                    window.request_redraw();
                    window.set_ime_allowed(true);
                    let px = (bounds[0] - scroll_acc[0]) * ui_scale;
                    let py = (bounds[1] - scroll_acc[1]) * ui_scale;
                    let pw = (bounds[2] * ui_scale).max(1.0);
                    let ph = (bounds[3] * ui_scale).max(1.0);
                    window.set_ime_cursor_area(
                        winit::dpi::PhysicalPosition::new(px as i32, py as i32),
                        winit::dpi::PhysicalSize::new(pw as u32, ph as u32),
                    );
                } else {
                    window.set_ime_allowed(false);
                }

                if self.force_redraw_count > 0 {
                    self.force_redraw_count -= 1;
                    window.request_redraw();
                }
            }
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                let mut rb = self.state.borrow_mut();
                if let Some(state) = rb.as_mut() {
                    let inner = state.window.inner_size();
                    if inner.width > 0 && inner.height > 0 {
                        state.resize(inner);
                    }
                    state.recover_surface();
                    let _ = state.render([0.0, 0.0], ScrollbarUpdateMode::Full, None, false);
                    state.window.request_redraw();
                    self.force_redraw_count = 3;
                }
            }
            Err(wgpu::SurfaceError::Timeout) => {
                window.request_redraw();
            }
            Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
        }
    }

    pub(super) fn handle_non_left_mouse_input(
        &mut self,
        id: WindowId,
        element_state: ElementState,
        button: MouseButton,
    ) {
        if button != MouseButton::Left {
            return;
        }
        self.handle_mouse_input(id, element_state);
    }
}

