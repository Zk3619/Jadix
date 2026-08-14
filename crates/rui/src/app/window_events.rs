use winit::{
    event::{ElementState, MouseButton, MouseScrollDelta},
    event_loop::ActiveEventLoop,
    window::{CursorIcon, WindowId},
};

use super::state::ScrollbarUpdateMode;
use crate::components::scrollbar::SCROLLBAR_TRACK_FOCUS_PREFIX;

impl super::App {
    /// Complete deferred OS-blur IME release once `ime_os_blur_at` elapses.
    fn flush_ime_os_blur_if_due(&mut self, window: &winit::window::Window) {
        let Some(at) = self.ime_os_blur_at else {
            return;
        };
        if self.os_window_focused {
            self.ime_os_blur_at = None;
            return;
        }
        if std::time::Instant::now() < at {
            return;
        }
        self.ime_os_blur_at = None;
        log::warn!("RUI_IME_DIAG os blur hard-release (stop system IME drive)");
        super::ime_support::on_os_focus_lost(window);
        super::ime_support::sync_ime_focus_owner(window, None);
        super::ime_support::set_ime_allowed_latched(window, false, &mut self.ime_allowed_latched);
    }

    /// Whether a pointer hit should keep the current Input/Textarea focus.
    fn pointer_keeps_text_edit_focus(
        root: &crate::core::tree::Element,
        focused: &str,
        path: &[super::pointer_events::HitPathEntry],
        input_drag_fid: Option<&str>,
        x: f32,
        y: f32,
    ) -> bool {
        if input_drag_fid == Some(focused) {
            return true;
        }
        let Some(rest) = super::ime_support::text_edit_rest(focused) else {
            return false;
        };
        let resize_fid = crate::utils::text_field::textarea_resize_focus_id(rest);
        let scroll_rest = crate::utils::text_field::textarea_scroll_rest(rest);
        let scroll_fid = crate::core::scroll_viewport::focus_id_for_scroll_viewport(&scroll_rest);
        for entry in path {
            let Some(fid) = entry.focus_id.as_deref() else {
                continue;
            };
            if fid == focused || fid == resize_fid || fid == scroll_fid {
                return true;
            }
            if let Some((srest, _, _)) = super::scroll_support::parse_scrollbar_focus_id(fid) {
                if srest == scroll_rest {
                    return true;
                }
            }
        }
        // Fallback: click inside the focused field's layout box (padding / empty chrome).
        if let Some((bounds, scroll_acc)) =
            super::tree_hit::find_bounds_by_focus_id(root, focused, [0.0, 0.0])
        {
            let vx = bounds[0] - scroll_acc[0];
            let vy = bounds[1] - scroll_acc[1];
            if x >= vx && x <= vx + bounds[2] && y >= vy && y <= vy + bounds[3] {
                return true;
            }
        }
        false
    }

    pub(super) fn handle_secondary_redraw(&mut self, id: WindowId) {
        let Some(sec) = self.secondaries.get_mut(&id) else {
            return;
        };
        let inner = sec.window.inner_size();
        if self.is_suspended || inner.width == 0 || inner.height == 0 {
            if !self.is_suspended && (inner.width == 0 || inner.height == 0) {
                sec.window.request_redraw();
            }
            return;
        }
        match sec.render() {
            Ok(()) => {}
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                sec.recover_surface();
                sec.window.request_redraw();
            }
            Err(wgpu::SurfaceError::OutOfMemory) => {
                log::error!("secondary window: GPU out of memory");
            }
            Err(wgpu::SurfaceError::Timeout) => {
                sec.window.request_redraw();
            }
        }
    }

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

    /// Returns `(caret_x_in_field, compose_origin_x_in_field, line_height_logical)`.
    fn estimate_ime_caret_geom(
        &mut self,
        focus_id: &str,
        visual: [f32; 4],
        ui_scale: f32,
        pad_override: Option<f32>,
    ) -> (f32, f32, f32) {
        let pad = pad_override.unwrap_or_else(|| {
            let rb = self.state.borrow();
            let Some(st) = rb.as_ref() else {
                return 8.0;
            };
            super::tree_hit::find_element_by_focus_id(&st.root_element, focus_id)
                .map(|e| {
                    let [_, br, _, bl] = e.style.resolved_border_widths();
                    // CSS % padding resolves against the containing block width. For a
                    // stretched field, border-box ≈ CB; subtract own border so we don't
                    // treat the stroke as part of the percentage basis.
                    let basis = (visual[2] - bl - br).max(0.0);
                    let pad_l = match e.style.padding.left {
                        taffy::style::LengthPercentage::Length(v) => v,
                        taffy::style::LengthPercentage::Percent(p) => basis * p,
                    };
                    pad_l + bl
                })
                .unwrap_or(8.0)
        });

        let mut rb = self.state.borrow_mut();
        let Some(st) = rb.as_mut() else {
            return (pad, pad, 20.0);
        };

        let rest = super::ime_support::text_edit_rest(focus_id).unwrap_or("");
        let vk = crate::components::input::input_value_signal_key(rest);
        let pk = crate::components::input::input_preedit_signal_key(rest);
        let ak = crate::components::input::input_ime_anchor_signal_key(rest);
        let ck = crate::components::input::input_caret_signal_key(rest);
        let committed = crate::core::reactivity::use_signal(&vk, String::new).peek();
        let pre = crate::core::reactivity::use_signal(&pk, String::new).peek();
        let anchor = crate::core::reactivity::use_signal(&ak, || None::<usize>).peek();
        let caret = crate::core::reactivity::use_signal(&ck, || None::<usize>).peek();
        let display = super::tree_hit::input_display_text(&committed, &pre, anchor);
        let mut compose_byte = if !pre.is_empty() {
            let mut a = anchor.unwrap_or(committed.len()).min(committed.len());
            a = committed.floor_char_boundary(a);
            a
        } else {
            caret.unwrap_or(display.len()).min(display.len())
        };
        compose_byte = display.floor_char_boundary(compose_byte.min(display.len()));
        let caret_byte = if !pre.is_empty() {
            (compose_byte + pre.len()).min(display.len())
        } else {
            compose_byte
        };
        let caret_byte = display.floor_char_boundary(caret_byte);
        let caret_prefix = &display[..caret_byte];
        let compose_prefix = &display[..compose_byte];

        fn first_text_style(el: &crate::core::tree::Element) -> Option<crate::style::TextStyle> {
            if el.text.is_some() {
                return Some(el.text_style.clone());
            }
            for c in &el.children {
                if let Some(s) = first_text_style(c) {
                    return Some(s);
                }
            }
            None
        }

        let text_style =
            super::tree_hit::find_input_text_leaf_for_rest(&st.root_element, None, rest)
                .map(|(_, _, style, _)| style)
                .or_else(|| {
                    super::tree_hit::find_element_by_focus_id(&st.root_element, focus_id)
                        .and_then(first_text_style)
                })
                .unwrap_or_default();

        let line_h = crate::utils::text_field::text_edit_line_box_px(
            text_style.size,
            text_style.line_height_px,
            text_style.line_height_scale,
        )
        .max(1.0);

        let caret_x = super::ime_support::estimate_caret_x_in_field(
            Some(st.text_renderer.font_system_mut()),
            caret_prefix,
            &text_style,
            visual,
            ui_scale,
            pad,
        );
        let compose_x = if compose_byte == caret_byte {
            caret_x
        } else {
            super::ime_support::estimate_caret_x_in_field(
                Some(st.text_renderer.font_system_mut()),
                compose_prefix,
                &text_style,
                visual,
                ui_scale,
                pad,
            )
        };
        (caret_x, compose_x, line_h)
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

    /// Advance content pan / sample fling velocity while primary pointer is down.
    fn tick_content_scroll_drag(&mut self, x: f32, y: f32) {
        const ARM_PX: f32 = 4.0;
        let now = std::time::Instant::now();
        let Some((rest, dx, dy, just_armed)) = (|| {
            let drag = self.content_scroll_drag.as_mut()?;
            let dt = now.duration_since(drag.last_t).as_secs_f32().max(1e-4);
            let dx = x - drag.last_x;
            let dy = y - drag.last_y;
            let mut just_armed = false;
            if !drag.armed {
                let dist = (x - drag.origin_x).hypot(y - drag.origin_y);
                if dist < ARM_PX {
                    drag.last_x = x;
                    drag.last_y = y;
                    drag.last_t = now;
                    return None;
                }
                drag.armed = true;
                just_armed = true;
            }
            // Finger move (dx,dy) → content follows → offset changes by (-dx,-dy).
            let inst_vx = -dx / dt;
            let inst_vy = -dy / dt;
            drag.vel[0] = drag.vel[0] * 0.65 + inst_vx * 0.35;
            drag.vel[1] = drag.vel[1] * 0.65 + inst_vy * 0.35;
            let rest = drag.rest.clone();
            drag.last_x = x;
            drag.last_y = y;
            drag.last_t = now;
            Some((rest, dx, dy, just_armed))
        })() else {
            return;
        };

        if just_armed {
            self.scroll_velocity.insert(rest.clone(), [0.0, 0.0]);
        }

        let (max_x, max_y) = self
            .scroll_metrics_cache
            .get(&rest)
            .map(|(_, [mx, my], _)| (*mx, *my))
            .unwrap_or((f32::MAX, f32::MAX));
        let sx_key = crate::core::scroll_viewport::scroll_offset_x_signal_key(&rest);
        let sy_key = crate::core::scroll_viewport::scroll_offset_y_signal_key(&rest);
        let cur_x = crate::core::reactivity::use_signal(&sx_key, || 0.0_f32).peek();
        let cur_y = crate::core::reactivity::use_signal(&sy_key, || 0.0_f32).peek();
        let next_x = (cur_x - dx).clamp(0.0, max_x);
        let next_y = (cur_y - dy).clamp(0.0, max_y);
        super::scroll_support::set_scroll_offset_with_behavior(&rest, next_x, next_y, true);
        self.bump_smooth_active_until(&rest, now);
        crate::core::reactivity::use_signal(
            &crate::core::scroll_viewport::scroll_activity_signal_key(&rest),
            || 0.0_f32,
        )
        .set_silent(1.0);
        if let Some(st) = (*self.state.borrow_mut()).as_mut() {
            st.window.request_redraw();
        }
    }

    /// On pointer-up after an armed content pan: seed `scroll_velocity` for inertia.
    fn finish_content_scroll_drag(&mut self) {
        let Some(drag) = self.content_scroll_drag.take() else {
            return;
        };
        if !drag.armed {
            return;
        }
        let now = std::time::Instant::now();
        let [vx, vy] = drag.vel;
        // Drop tiny flicks (likely tap jitter).
        if vx.hypot(vy) < 80.0 {
            self.scroll_velocity.insert(drag.rest.clone(), [0.0, 0.0]);
            return;
        }
        self.scroll_velocity.insert(drag.rest.clone(), [vx, vy]);
        // Nudge targets so smooth easing has somewhere to go while velocity integrates.
        let (max_x, max_y) = self
            .scroll_metrics_cache
            .get(&drag.rest)
            .map(|(_, [mx, my], _)| (*mx, *my))
            .unwrap_or((0.0, 0.0));
        let sx_key = crate::core::scroll_viewport::scroll_offset_x_signal_key(&drag.rest);
        let sy_key = crate::core::scroll_viewport::scroll_offset_y_signal_key(&drag.rest);
        let cur_x = crate::core::reactivity::use_signal(&sx_key, || 0.0_f32).peek();
        let cur_y = crate::core::reactivity::use_signal(&sy_key, || 0.0_f32).peek();
        // ~120ms of coast projected into the target.
        let coast = 0.12;
        let tx = (cur_x + vx * coast).clamp(0.0, max_x);
        let ty = (cur_y + vy * coast).clamp(0.0, max_y);
        super::scroll_support::set_scroll_offset_with_behavior(&drag.rest, tx, ty, false);
        self.bump_smooth_active_until(&drag.rest, now);
        if let Some(st) = (*self.state.borrow_mut()).as_mut() {
            st.window.request_redraw();
        }
    }

    fn hover_key(rest: &str, axis: super::scroll_support::ScrollAxis) -> String {
        let a = match axis {
            super::scroll_support::ScrollAxis::X => "x",
            super::scroll_support::ScrollAxis::Y => "y",
        };
        format!("__rui_scroll_hover_{rest}:{a}")
    }

    fn scroll_axis_enabled_by_overflow(
        rest: &str,
        axis: super::scroll_support::ScrollAxis,
    ) -> bool {
        let ox = crate::core::reactivity::use_signal(
            &crate::core::scroll_viewport::scroll_overflow_x_signal_key(rest),
            || "auto".to_string(),
        )
        .peek();
        let oy = crate::core::reactivity::use_signal(
            &crate::core::scroll_viewport::scroll_overflow_y_signal_key(rest),
            || "auto".to_string(),
        )
        .peek();
        let o = match axis {
            super::scroll_support::ScrollAxis::X => ox.as_str(),
            super::scroll_support::ScrollAxis::Y => oy.as_str(),
        };
        crate::core::scroll_viewport::scroll_overflow_allows_scrollbar(o)
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
        crate::style::use_scrollbar_visibility_spring(&Self::spring_key(rest, axis), 0.0).set(1.0);
    }

    pub(super) fn handle_cursor_moved(
        &mut self,
        id: WindowId,
        position: winit::dpi::PhysicalPosition<f64>,
    ) {
        let mut state_borrow = self.state.borrow_mut();
        let state = match (*state_borrow).as_mut() {
            Some(s) if s.window.id() == id => s,
            _ => return,
        };

        self.cursor_pos = position;
        let phys_x = position.x as f32;
        let phys_y = position.y as f32;
        let scale = state.window.scale_factor() as f32;
        let x = position.x as f32 / scale.max(1.0);
        let y = position.y as f32 / scale.max(1.0);
        let window = state.window.clone();

        // Hover-driven overlay scrollbar visibility: pick the topmost scroll area (or scrollbar target)
        // under the cursor and keep its hover signal set while we remain inside.
        let mut hover_rest: Option<String> = None;
        let mut hover_axis: Option<(String, super::scroll_support::ScrollAxis)> = None;
        if let Some(fid) = super::tree_hit::find_focus_id_at(&state.root_element, x, y, [0.0, 0.0])
        {
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
                let key = crate::core::scroll_viewport::scroll_hover_mode_signal_key(rest);
                let mode =
                    crate::core::reactivity::use_signal(&key, || "scrollbar".to_string()).get();
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
                let thumb_pos =
                    (cursor_axis - drag.track_start - drag.grab_offset).clamp(0.0, span);
                let t = (thumb_pos / span).clamp(0.0, 1.0);
                Some((drag.rest.clone(), drag.axis, drag.max_offset * t))
            } else {
                None
            };

        // Slider 拖拽在 drop(state_borrow) 之后再 set Signal，避免 bind_text →
        // try_update_element_by_template_id 与本处 borrow_mut 重入。
        let slider_drag_x = x;

        let drag_sel_update: Option<(String, usize, usize)> = if self.left_button_down
            && self.scrollbar_drag.is_none()
        {
            if let Some(rest) = self.input_drag_rest.as_ref() {
                if let Some((bounds, display_el, style, leaf_scroll)) =
                    super::tree_hit::find_input_text_leaf_for_rest(&state.root_element, None, rest)
                {
                    let vk = crate::components::input::input_value_signal_key(rest);
                    let pk = crate::components::input::input_preedit_signal_key(rest);
                    let committed = crate::core::reactivity::use_signal(&vk, String::new).get();
                    let pre = crate::core::reactivity::use_signal(&pk, String::new).get();
                    let ui_scale = scale.max(1.0);
                    let mx = (x + leaf_scroll[0]).clamp(bounds[0], bounds[0] + bounds[2]);
                    let my = (y + leaf_scroll[1]).clamp(bounds[1], bounds[1] + bounds[3]);
                    // Hit against the painted leaf text (password may be masked `•`).
                    let ix = crate::renderer::hit_test_input_text_index(
                        state.text_renderer.font_system_mut(),
                        &display_el,
                        mx,
                        my,
                        bounds,
                        &style,
                        ui_scale,
                    );
                    let caret_ix = if committed.is_empty() && pre.is_empty() {
                        0
                    } else {
                        crate::components::input::caret_index_from_display_hit(rest, ix)
                    };
                    let lo = self.input_drag_anchor.min(caret_ix);
                    let hi = self.input_drag_anchor.max(caret_ix);
                    Some((rest.clone(), lo, hi))
                } else {
                    None
                }
            } else if let Some(key) = self.label_drag_key.as_ref() {
                if let Some((bounds, display_el, style, leaf_scroll)) =
                    super::tree_hit::find_selectable_text_leaf_for_key(&state.root_element, key)
                {
                    let ui_scale = scale.max(1.0);
                    let mx = (x + leaf_scroll[0]).clamp(bounds[0], bounds[0] + bounds[2]);
                    let my = (y + leaf_scroll[1]).clamp(bounds[1], bounds[1] + bounds[3]);
                    let ix = crate::renderer::hit_test_input_text_index(
                        state.text_renderer.font_system_mut(),
                        &display_el,
                        mx,
                        my,
                        bounds,
                        &style,
                        ui_scale,
                    );
                    let lo = self.label_drag_anchor.min(ix);
                    let hi = self.label_drag_anchor.max(ix);
                    Some((key.clone(), lo, hi))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let label_drag_sel_update = drag_sel_update.as_ref().and_then(|(key, lo, hi)| {
            if self.label_drag_key.as_deref() == Some(key.as_str()) {
                Some((key.clone(), *lo, *hi))
            } else {
                None
            }
        });
        let input_drag_sel_update = drag_sel_update.and_then(|(key, lo, hi)| {
            if self.input_drag_rest.as_deref() == Some(key.as_str()) {
                Some((key, lo, hi))
            } else {
                None
            }
        });

        if self.left_button_down && self.scrollbar_drag.is_none() {
            if let Some(drag) = self.drag_reorder.as_mut() {
                // 指针不在本列表任一拖拽手柄上时，回到「源行」下标，避免 hover 卡在最后一根有效行。
                let mut new_hover = drag.from_index;
                if let Some(fid) =
                    super::tree_hit::find_focus_id_at(&state.root_element, x, y, [0.0, 0.0])
                {
                    if let Some((lid, idx)) =
                        crate::utils::drag_reorder::parse_drag_item_focus_id(&fid)
                    {
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

        // Always apply: occluders (modal underlay) return None and must reset to Default,
        // otherwise gallery scrollbar Pointer cursors stick through the mask.
        window.set_cursor(
            match cursor_pick.unwrap_or(crate::core::tree::CursorKind::Default) {
                crate::core::tree::CursorKind::Pointer => winit::window::CursorIcon::Pointer,
                crate::core::tree::CursorKind::Text => winit::window::CursorIcon::Text,
                crate::core::tree::CursorKind::NotAllowed => winit::window::CursorIcon::NotAllowed,
                crate::core::tree::CursorKind::NwseResize => winit::window::CursorIcon::NwseResize,
                crate::core::tree::CursorKind::Default => winit::window::CursorIcon::Default,
            },
        );

        drop(state_borrow);
        self.note_long_press_move(phys_x, phys_y);

        let slider_dragging = crate::components::slider::tick_slider_drag(slider_drag_x);
        let toast_dragging = crate::components::toast::tick_toast_drag(x, y);
        let color_dragging = crate::components::color_picker::tick_color_drag(x, y);
        if slider_dragging || toast_dragging || color_dragging {
            window.request_redraw();
        }

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
                if let Some(st) = (*self.state.borrow_mut()).as_mut() {
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
            if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                st.window.request_redraw();
            }
        }

        if let Some((rest, axis, new_offset)) = scrollbar_drag_update {
            self.bump_scrollbar_visible_until_axis(&rest, axis, std::time::Instant::now());
            match axis {
                super::scroll_support::ScrollAxis::X => {
                    super::scroll_support::set_scroll_offset_with_behavior(
                        &rest,
                        new_offset,
                        crate::core::reactivity::use_signal(
                            &crate::core::scroll_viewport::scroll_offset_y_signal_key(&rest),
                            || 0.0_f32,
                        )
                        .get(),
                        true,
                    )
                }
                super::scroll_support::ScrollAxis::Y => {
                    super::scroll_support::set_scroll_offset_with_behavior(
                        &rest,
                        crate::core::reactivity::use_signal(
                            &crate::core::scroll_viewport::scroll_offset_x_signal_key(&rest),
                            || 0.0_f32,
                        )
                        .get(),
                        new_offset,
                        true,
                    )
                }
            }
            if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                st.window.request_redraw();
            }
        } else if let Some(drag) = self.textarea_resize_drag.as_ref() {
            let dy = y - drag.start_cursor_y;
            let new_h = (drag.start_h + dy).clamp(drag.min_h, drag.max_h);
            let h_key = crate::utils::text_field::textarea_height_signal_key(&drag.field_id);
            let sig = crate::core::reactivity::use_signal(&h_key, || drag.start_h);
            if (sig.peek() - new_h).abs() > 0.5 {
                sig.set(new_h);
                if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                    st.window.request_redraw();
                }
            }
        } else if self.content_scroll_drag.is_some()
            && self.scrollbar_drag.is_none()
            && self.textarea_resize_drag.is_none()
            && self.input_drag_rest.is_none()
            && self.label_drag_key.is_none()
            && self.drag_reorder.is_none()
            && !crate::components::slider::is_slider_dragging()
            && !crate::components::color_picker::is_color_dragging()
        {
            self.tick_content_scroll_drag(x, y);
        }
        if let Some((rest, lo, hi)) = input_drag_sel_update {
            let sk = crate::components::input::input_selection_signal_key(&rest);
            crate::core::reactivity::use_signal(&sk, || None::<(usize, usize)>).set(Some((lo, hi)));
            let ck = crate::components::input::input_caret_signal_key(&rest);
            crate::core::reactivity::use_signal(&ck, || None::<usize>).set(Some(hi));
            // Selection chrome is paint-synced via peek — must request a frame or the
            // highlight never appears until an unrelated redraw.
            if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                st.window.request_redraw();
            }
        }
        if let Some((key, lo, hi)) = label_drag_sel_update {
            let sk = crate::components::label::label_selection_signal_key(&key);
            crate::core::reactivity::use_signal(&sk, || None::<(usize, usize)>).set(Some((lo, hi)));
            if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                st.window.request_redraw();
            }
        }
        for (cb, is_hover) in to_run {
            (*cb.borrow_mut())(is_hover);
        }
        if crate::primitives::interactive::take_hover_paint_dirty() {
            window.request_redraw();
        }
        if self.textarea_resize_drag.is_some() {
            window.set_cursor(CursorIcon::NwseResize);
        } else {
            match cursor_pick {
                Some(k) => window.set_cursor(super::tree_hit::cursor_kind_to_winit(k)),
                None => window.set_cursor(CursorIcon::Default),
            }
        }
    }

    pub(super) fn handle_cursor_left(&mut self, id: WindowId) {
        let mut state_borrow = self.state.borrow_mut();
        let state = match (*state_borrow).as_mut() {
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
        let state = match (*state_borrow).as_mut() {
            Some(s) if s.window.id() == id => s,
            _ => return,
        };

        let scale = state.window.scale_factor() as f32;
        let x = self.cursor_pos.x as f32 / scale.max(1.0);
        let y = self.cursor_pos.y as f32 / scale.max(1.0);

        let mut input_drag: Option<(String, usize, String)> = None;
        let mut label_drag: Option<(String, usize)> = None;
        if element_state == ElementState::Pressed {
            if let Some((rest, bounds, display_el, style, leaf_scroll, focus_id)) =
                super::tree_hit::find_input_text_at(&state.root_element, None, x, y, [0.0, 0.0])
            {
                let ui_scale = scale.max(1.0);
                let ix = crate::renderer::hit_test_input_text_index(
                    state.text_renderer.font_system_mut(),
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
                let caret_ix = if committed.is_empty() && pre.is_empty() {
                    0
                } else {
                    crate::components::input::caret_index_from_display_hit(&rest, ix)
                };
                input_drag = Some((rest, caret_ix, focus_id));
            } else if let Some((key, bounds, display_el, style, leaf_scroll)) =
                super::tree_hit::find_selectable_text_at(&state.root_element, x, y, [0.0, 0.0])
            {
                let ui_scale = scale.max(1.0);
                let ix = crate::renderer::hit_test_input_text_index(
                    state.text_renderer.font_system_mut(),
                    &display_el,
                    x + leaf_scroll[0],
                    y + leaf_scroll[1],
                    bounds,
                    &style,
                    ui_scale,
                );
                label_drag = Some((key, ix));
            }
        }

        if element_state == ElementState::Pressed {
            if let Some(fid) =
                super::tree_hit::find_focus_id_at(&state.root_element, x, y, [0.0, 0.0])
            {
                if let Some((rest, axis, is_thumb)) =
                    super::scroll_support::parse_scrollbar_focus_id(&fid)
                {
                    let metrics = self.scroll_metrics_cache.get(&rest).copied();
                    if let Some((_area_bounds, [max_x, max_y], _)) = metrics {
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
                        if let (
                            Some((track_bounds, track_scroll)),
                            Some((thumb_bounds, thumb_scroll)),
                        ) = (
                            super::tree_hit::find_bounds_by_focus_id(
                                &state.root_element,
                                &track_fid,
                                [0.0, 0.0],
                            ),
                            super::tree_hit::find_bounds_by_focus_id(
                                &state.root_element,
                                &thumb_fid,
                                [0.0, 0.0],
                            ),
                        ) {
                            let (
                                track_start,
                                track_len,
                                thumb_len,
                                max_offset,
                                cursor_axis,
                                thumb_start,
                            ) = match axis {
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
                                crate::core::reactivity::use_signal("__rui_focus_visible", || {
                                    false
                                });
                            focus_visible.set(false);
                            if is_thumb {
                                self.scrollbar_drag =
                                    Some(super::scroll_support::ScrollbarDragState {
                                        rest,
                                        axis,
                                        track_start,
                                        track_len,
                                        thumb_len,
                                        max_offset,
                                        grab_offset: (cursor_axis - thumb_start)
                                            .clamp(0.0, thumb_len),
                                    });
                            } else {
                                let span = (track_len - thumb_len).max(1.0);
                                let thumb_pos =
                                    (cursor_axis - track_start - thumb_len * 0.5).clamp(0.0, span);
                                let t = (thumb_pos / span).clamp(0.0, 1.0);
                                let target = max_offset * t;
                                let behavior_key =
                                    crate::core::scroll_viewport::scroll_behavior_signal_key(&rest);
                                let behavior =
                                    crate::core::reactivity::use_signal(&behavior_key, || {
                                        crate::core::scroll_viewport::DEFAULT_SCROLL_BEHAVIOR
                                            .to_string()
                                    })
                                    .get();
                                let immediate =
                                    crate::core::scroll_viewport::scroll_behavior_is_jump(
                                        &behavior,
                                    );
                                match axis {
                                    super::scroll_support::ScrollAxis::X => {
                                        let cur_y = crate::core::reactivity::use_signal(
                                            &crate::core::scroll_viewport::scroll_offset_y_signal_key(
                                                &rest,
                                            ),
                                            || 0.0_f32,
                                        )
                                        .get();
                                        super::scroll_support::set_scroll_offset_with_behavior(
                                            &rest, target, cur_y, immediate,
                                        );
                                    }
                                    super::scroll_support::ScrollAxis::Y => {
                                        let cur_x = crate::core::reactivity::use_signal(
                                            &crate::core::scroll_viewport::scroll_offset_x_signal_key(
                                                &rest,
                                            ),
                                            || 0.0_f32,
                                        )
                                        .get();
                                        super::scroll_support::set_scroll_offset_with_behavior(
                                            &rest, cur_x, target, immediate,
                                        );
                                    }
                                }
                            }
                            if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                                st.window.request_redraw();
                            }
                            return;
                        }
                    }
                }
            }
            if self.scrollbar_drag.is_none() && self.drag_reorder.is_none() {
                if let Some(fid) =
                    super::tree_hit::find_focus_id_at(&state.root_element, x, y, [0.0, 0.0])
                {
                    if let Some(field_id) =
                        fid.strip_prefix(crate::utils::text_field::TEXTAREA_RESIZE_FOCUS_PREFIX)
                    {
                        let h_key = crate::utils::text_field::textarea_height_signal_key(field_id);
                        let start_h =
                            crate::core::reactivity::use_signal(&h_key, || 0.0_f32).peek();
                        let (min_h, max_h) =
                            super::tree_hit::find_element_by_focus_id(&state.root_element, &fid)
                                .map(|grip| {
                                    let min = grip
                                        .attrs
                                        .get("__rui_ta_resize_min")
                                        .and_then(|s| s.parse::<f32>().ok())
                                        .unwrap_or(start_h.max(32.0));
                                    let max = grip
                                        .attrs
                                        .get("__rui_ta_resize_max")
                                        .and_then(|s| s.parse::<f32>().ok())
                                        .unwrap_or(480.0)
                                        .max(min);
                                    (min, max)
                                })
                                .unwrap_or((start_h.max(32.0), 480.0_f32.max(start_h)));
                        drop(state_borrow);
                        self.textarea_resize_drag = Some(super::TextareaResizeDrag {
                            field_id: field_id.to_string(),
                            start_cursor_y: y,
                            start_h,
                            min_h,
                            max_h,
                        });
                        self.content_scroll_drag = None;
                        self.input_drag_rest = None;
                        self.label_drag_key = None;
                        let focus_visible =
                            crate::core::reactivity::use_signal("__rui_focus_visible", || false);
                        focus_visible.set(false);
                        self.left_button_down = true;
                        if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                            st.window.set_cursor(winit::window::CursorIcon::NwseResize);
                            st.window.request_redraw();
                        }
                        return;
                    }
                    if let Some((list_id, idx)) =
                        crate::utils::drag_reorder::parse_drag_item_focus_id(&fid)
                    {
                        drop(state_borrow);
                        self.drag_reorder = Some(crate::utils::drag_reorder::DragReorderState {
                            list_id: list_id.clone(),
                            from_index: idx,
                            hover_index: Some(idx),
                        });
                        let hk = crate::utils::drag_reorder::drag_hover_signal_key(&list_id);
                        crate::core::reactivity::use_signal(&hk, || None::<usize>).set(Some(idx));
                        let focus_visible =
                            crate::core::reactivity::use_signal("__rui_focus_visible", || false);
                        focus_visible.set(false);
                        self.left_button_down = true;
                        if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                            st.window.request_redraw();
                        }
                        return;
                    }
                }
            }

            // Arm a potential content pan (deepest scroll viewport under the pointer).
            // Touch always; mouse only when `set_mouse_content_pan_enabled(true)`.
            let allow_content_pan =
                self.primary_touch_id.is_some() || super::mouse_content_pan_enabled();
            if allow_content_pan
                && self.scrollbar_drag.is_none()
                && self.drag_reorder.is_none()
                && self.textarea_resize_drag.is_none()
                && input_drag.is_none()
                && label_drag.is_none()
            {
                let mut chain = Vec::new();
                super::scroll_support::collect_scroll_rest_chain_at(
                    &state.root_element,
                    x,
                    y,
                    [0.0, 0.0],
                    &mut chain,
                );
                if let Some(rest) = chain.first() {
                    let now = std::time::Instant::now();
                    self.content_scroll_drag = Some(super::scroll_support::ContentScrollDrag {
                        rest: rest.clone(),
                        origin_x: x,
                        origin_y: y,
                        last_x: x,
                        last_y: y,
                        last_t: now,
                        vel: [0.0, 0.0],
                        armed: false,
                    });
                }
            }

            let capture = super::pointer_capture_id();
            let path = if let Some(ref id) = capture {
                if let Some(el) =
                    super::pointer_events::find_by_focus_id(&state.root_element, id.as_str())
                {
                    vec![super::pointer_events::HitPathEntry {
                        focus_id: el.focus_id.clone(),
                        label_for: el
                            .attrs
                            .get(crate::components::label::ATTR_LABEL_FOR)
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty()),
                        on_pointer_down: el.on_pointer_down.clone(),
                        on_pointer_up: el.on_pointer_up.clone(),
                        on_click: el.on_click.clone(),
                        on_context_menu: el.on_context_menu.clone(),
                        on_double_click: el.on_double_click.clone(),
                        hoisted_portal: el
                            .attrs
                            .get(crate::layout::layout_tree::ATTR_HOISTED_PORTAL)
                            .map(|s| s.as_str())
                            == Some("1"),
                        portal_key: el
                            .attrs
                            .get(crate::layout::layout_tree::ATTR_PORTAL_KEY)
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty()),
                        floating_id: el
                            .attrs
                            .get(crate::utils::popup::ATTR_FLOATING)
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty()),
                        bounds: el.layout_bounds,
                    }]
                } else {
                    super::pointer_events::hit_path(&state.root_element, x, y)
                }
            } else {
                super::pointer_events::hit_path(&state.root_element, x, y)
            };
            drop(state_borrow);
            let focus_visible =
                crate::core::reactivity::use_signal("__rui_focus_visible", || false);
            focus_visible.set(false);
            // Focus chrome is applied in render via `peek`; without a redraw, click-to-clear / click-to-focus
            // would lag until the next unrelated frame.
            let mut need_paint_sync_redraw = true;

            let detail = self.click_count.max(1);
            let mut pev = super::pointer_events::PointerEvent::new(x, y, MouseButton::Left, detail);
            super::pointer_events::dispatch_pointer(
                &path,
                &mut pev,
                super::pointer_events::PointerKind::Down,
            );
            // Slider / other controls that stop_propagation must not also pan the scroll parent.
            if pev.is_consumed() {
                self.content_scroll_drag = None;
            }
            // `on_click` / Label `for` 在 pointer up（抬起）时触发，对标 DOM click。

            let input_drag_fid = input_drag.as_ref().map(|(_, _, fid)| fid.clone());
            if let Some((rest, caret_ix, focus_id)) = input_drag {
                // Hit-tested caret placement: ensure DOM focus even if `on_click` was consumed
                // (e.g. by an ancestor pointer handler), so caret/selection paint can run.
                crate::core::reactivity::use_signal("__rui_button_focus_id", String::new)
                    .set(focus_id);
                let ck = crate::components::input::input_caret_signal_key(&rest);
                let sk = crate::components::input::input_selection_signal_key(&rest);
                crate::core::reactivity::use_signal(&ck, || None::<usize>).set(Some(caret_ix));
                crate::core::reactivity::use_signal(&sk, || None::<(usize, usize)>)
                    .set(Some((caret_ix, caret_ix)));
                self.input_drag_rest = Some(rest);
                self.input_drag_anchor = caret_ix;
                self.label_drag_key = None;
                self.content_scroll_drag = None;
                self.left_button_down = true;
                need_paint_sync_redraw = true;
            } else if let Some((key, caret_ix)) = label_drag {
                let sk = crate::components::label::label_selection_signal_key(&key);
                crate::core::reactivity::use_signal(&sk, || None::<(usize, usize)>)
                    .set(Some((caret_ix, caret_ix)));
                self.label_drag_key = Some(key);
                self.label_drag_anchor = caret_ix;
                self.input_drag_rest = None;
                self.content_scroll_drag = None;
                self.left_button_down = true;
                need_paint_sync_redraw = true;
            }

            // Browser-like: pointer down outside the focused text field clears its focus.
            // Keep focus when hitting the field chrome, caret, adornments, resize, or inner scroll.
            {
                let focused =
                    crate::core::reactivity::use_signal("__rui_button_focus_id", String::new)
                        .peek();
                if super::ime_support::is_text_edit_focus(&focused) {
                    let keep = if let Some(st) = (*self.state.borrow()).as_ref() {
                        Self::pointer_keeps_text_edit_focus(
                            &st.root_element,
                            &focused,
                            &path,
                            input_drag_fid.as_deref(),
                            x,
                            y,
                        )
                    } else {
                        true
                    };
                    if !keep {
                        crate::core::reactivity::use_signal("__rui_button_focus_id", String::new)
                            .set(String::new());
                        need_paint_sync_redraw = true;
                    }
                }
            }

            // Radix-like: dismiss Menu/Popover/Tooltip on pointer *down* outside the panel
            // and its trigger — same gesture as text-field blur (Combobox must not require
            // a second click to close after the first click only cleared focus).
            //
            // Never call `open.set` while AppState is borrowed.
            {
                let should_outside_dismiss = if let Some(st) = (*self.state.borrow()).as_ref() {
                    !super::portal_overlay::hit_path_on_top_exclusive_surface(&path)
                        && !crate::utils::popup::path_hits_floating_trigger(&st.root_element, &path)
                } else {
                    false
                };
                if should_outside_dismiss && super::portal_overlay::try_dismiss_outside_press() {
                    need_paint_sync_redraw = true;
                }
            }

            // After click handlers may move focus (Button / other Input), sync IME owner
            // immediately so in-app blur folds preedit (stripping `'`) this event — not
            // only on the next redraw.
            if let Some(st) = (*self.state.borrow()).as_ref() {
                let fid = crate::core::reactivity::use_signal("__rui_button_focus_id", String::new)
                    .peek();
                if super::ime_support::is_text_edit_focus(&fid) {
                    super::ime_support::sync_ime_focus_owner(&st.window, Some(&fid));
                } else {
                    super::ime_support::sync_ime_focus_owner(&st.window, None);
                }
            }

            if need_paint_sync_redraw {
                if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                    st.window.request_redraw();
                }
            }
        } else {
            drop(state_borrow);
            self.left_button_down = false;
            self.scrollbar_drag = None;
            self.textarea_resize_drag = None;
            crate::components::slider::clear_slider_drag_if_any();
            crate::components::toast::clear_toast_drag_if_any();
            crate::components::color_picker::clear_color_drag_if_any();
            self.finish_content_scroll_drag();
            if let Some(st_drag) = self.drag_reorder.take() {
                let hk = crate::utils::drag_reorder::drag_hover_signal_key(&st_drag.list_id);
                crate::core::reactivity::use_signal(&hk, || None::<usize>).set(None);
                let h = st_drag.hover_index.unwrap_or(st_drag.from_index);
                if h != st_drag.from_index {
                    let ck = crate::utils::drag_reorder::drag_commit_signal_key(&st_drag.list_id);
                    crate::core::reactivity::use_signal(&ck, || None::<(usize, usize)>)
                        .set(Some((st_drag.from_index, h)));
                    if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                        st.window.request_redraw();
                    }
                }
            }
            if let Some(rest) = self.input_drag_rest.take() {
                let sk = crate::components::input::input_selection_signal_key(&rest);
                let sel = crate::core::reactivity::use_signal(&sk, || None::<(usize, usize)>).get();
                if let Some((a, b)) = sel {
                    if a == b {
                        crate::core::reactivity::use_signal(&sk, || None::<(usize, usize)>)
                            .set(None);
                    }
                }
                // Always repaint after an input drag so collapsed ranges clear and
                // non-empty selections get a final highlight frame.
                if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                    st.window.request_redraw();
                }
            }
            if let Some(key) = self.label_drag_key.take() {
                let sk = crate::components::label::label_selection_signal_key(&key);
                let sel = crate::core::reactivity::use_signal(&sk, || None::<(usize, usize)>).get();
                if let Some((a, b)) = sel {
                    if a == b {
                        crate::core::reactivity::use_signal(&sk, || None::<(usize, usize)>)
                            .set(None);
                    }
                }
                if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                    st.window.request_redraw();
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
            MouseScrollDelta::LineDelta(dx, dy) => {
                (dx * lines * line_height, dy * lines * line_height)
            }
            MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
        };

        let mut state_borrow = self.state.borrow_mut();
        let state = match (*state_borrow).as_mut() {
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

        let metrics_for_rest = |rest: &str| self.scroll_metrics_cache.get(rest).copied();

        // When wheel happens over a scrollbar track/thumb, prioritize that target scroll area.
        // This fixes cases where hit-chain returns an unrelated ancestor (or none), causing
        // wheel input to be ignored while cursor is on the scrollbar itself.
        if let Some(fid) = super::tree_hit::find_focus_id_at(&state.root_element, x, y, [0.0, 0.0])
        {
            if let Some((rest, _, _)) = super::scroll_support::parse_scrollbar_focus_id(&fid) {
                if metrics_for_rest(&rest).is_some() {
                    target_rests.retain(|r| r != &rest);
                    target_rests.insert(0, rest);
                }
            }
        }

        // Focused multiline field: prefer its inner scrollport when the pointer is over the
        // field chrome. Outer ScrollArea must not steal wheel while content overflows.
        {
            let focused =
                crate::core::reactivity::use_signal("__rui_button_focus_id", String::new).peek();
            if let Some(field) = crate::utils::text_field::text_edit_rest(&focused) {
                if focused.starts_with(crate::utils::text_field::TEXTAREA_FOCUS_PREFIX) {
                    let ta_rest = crate::utils::text_field::textarea_scroll_rest(field);
                    let resize_fid = crate::utils::text_field::textarea_resize_focus_id(field);
                    let scroll_fid =
                        crate::core::scroll_viewport::focus_id_for_scroll_viewport(&ta_rest);
                    let over_field =
                        super::tree_hit::find_focus_id_at(&state.root_element, x, y, [0.0, 0.0])
                            .is_some_and(|hit| {
                                hit == focused || hit == scroll_fid || hit == resize_fid
                            });
                    if over_field
                        && metrics_for_rest(&ta_rest).is_some_and(|(_, [_, max_y], _)| max_y > 0.5)
                    {
                        target_rests.retain(|r| r != &ta_rest);
                        target_rests.insert(0, ta_rest);
                    }
                }
            }
        }

        // We'll capture at most one scroll area (deepest first); metrics are loaded lazily
        // to avoid precomputing extent for every ancestor on each wheel tick.
        let mut captured: Option<(String, f32, f32, f32, f32)> = None;
        #[cfg(debug_assertions)]
        let candidate_count = target_rests.len();
        for rest in target_rests {
            let Some((_, [max_x, max_y], _)) = metrics_for_rest(&rest) else {
                continue;
            };
            let keys = crate::core::scroll_viewport::signal_keys_for_rest_cached(
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
                self.bump_scrollbar_visible_until_axis(
                    &rest,
                    super::scroll_support::ScrollAxis::X,
                    now,
                );
            }
            if dy.abs() > 0.1 {
                self.bump_scrollbar_visible_until_axis(
                    &rest,
                    super::scroll_support::ScrollAxis::Y,
                    now,
                );
            }

            // Smooth policy (per requirement): wheel updates target; content animates toward it (no sudden jumps).
            let (
                offset_x_key,
                offset_y_key,
                target_x_key,
                target_y_key,
                behavior_key,
                activity_key,
            ) = {
                let keys = crate::core::scroll_viewport::signal_keys_for_rest_cached(
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
            let behavior = crate::core::reactivity::use_signal(&behavior_key, || {
                crate::core::scroll_viewport::DEFAULT_SCROLL_BEHAVIOR.to_string()
            })
            .peek();
            let is_smooth_mode = crate::core::scroll_viewport::scroll_behavior_is_smooth(&behavior);

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
                    Self::maybe_wheel_bump_scrollbar_spring(
                        &rest,
                        super::scroll_support::ScrollAxis::X,
                    );
                }
                if dy.abs() > 0.1 {
                    Self::maybe_wheel_bump_scrollbar_spring(
                        &rest,
                        super::scroll_support::ScrollAxis::Y,
                    );
                }
            } else {
                // Non-smooth: keep existing target-based accumulation behavior.
                let rest_str = rest.clone();
                let entry = self.scroll_accum.entry(rest_str).or_insert([0.0, 0.0]);
                entry[0] += dx;
                entry[1] += dy;
                if dx.abs() > 0.1 {
                    Self::maybe_wheel_bump_scrollbar_spring(
                        &rest,
                        super::scroll_support::ScrollAxis::X,
                    );
                }
                if dy.abs() > 0.1 {
                    Self::maybe_wheel_bump_scrollbar_spring(
                        &rest,
                        super::scroll_support::ScrollAxis::Y,
                    );
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
        let state = match (*state_borrow).as_mut() {
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
                let template_match = has_templates
                    && super::hot_reload::has_any_template_id(&state.root_element, &ids);

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
                    let replacement_count: usize =
                        merged.iter().map(|t| t.text_replacements.len()).sum();
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
                        let preview = existing_ids
                            .into_iter()
                            .take(5)
                            .collect::<Vec<_>>()
                            .join(", ");
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
        // UI spring/cubic 与滚动共用夹紧口径（`tick_animations` 内部亦会再夹一次）。
        let anim_dt = scroll_dt;

        let window = state.window.clone();
        drop(state_borrow);

        let anim_tick = crate::style::tick_animations(anim_dt);
        let nav_animating = crate::app::navigator::tick_active(anim_dt);
        if anim_tick.any || nav_animating {
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
            let sx_key = crate::core::scroll_viewport::scroll_offset_x_signal_key(&rest);
            let sy_key = crate::core::scroll_viewport::scroll_offset_y_signal_key(&rest);
            let tx_key = crate::core::scroll_viewport::scroll_target_x_signal_key(&rest);
            let ty_key = crate::core::scroll_viewport::scroll_target_y_signal_key(&rest);
            let sx_sig = crate::core::reactivity::use_signal(&sx_key, || 0.0_f32);
            let sy_sig = crate::core::reactivity::use_signal(&sy_key, || 0.0_f32);
            let tx = crate::core::reactivity::use_signal(&tx_key, || sx_sig.peek());
            let ty = crate::core::reactivity::use_signal(&ty_key, || sy_sig.peek());
            let tx_cur = tx.peek();
            let ty_cur = ty.peek();
            let nx = (tx_cur - adx).clamp(0.0, max_x);
            let ny = (ty_cur - ady).clamp(0.0, max_y);
            if (nx - tx_cur).abs() > 0.01 {
                tx.set_silent(nx);
            }
            if (ny - ty_cur).abs() > 0.01 {
                ty.set_silent(ny);
            }
            if adx.abs() > 0.1 || ady.abs() > 0.1 {
                crate::core::reactivity::use_signal(
                    &crate::core::scroll_viewport::scroll_activity_signal_key(&rest),
                    || 0.0_f32,
                )
                .set_silent(1.0);
            }
        }

        // Fling inertia: integrate `scroll_velocity` into targets, then exponential friction decay.
        // Wheel must not write here (see handle_mouse_wheel); only content-pan release seeds velocity.
        const INERTIA_FRICTION_PER_SEC: f32 = 4.5;
        let mut inertia_ops = Vec::new();
        for (rest, vel) in self.scroll_velocity.iter_mut() {
            if vel[0].abs() < 0.01 && vel[1].abs() < 0.01 {
                continue;
            }
            let max_pair = self
                .scroll_metrics_cache
                .get(rest)
                .map(|(_, [mx, my], _)| (*mx, *my));
            let Some((max_x, max_y)) = max_pair else {
                continue;
            };
            let [nvx, nvy] = super::scroll_support::decay_scroll_velocity(
                vel[0],
                vel[1],
                scroll_dt,
                INERTIA_FRICTION_PER_SEC,
            );
            let dx = vel[0] * scroll_dt;
            let dy = vel[1] * scroll_dt;
            *vel = [nvx, nvy];
            inertia_ops.push((rest.clone(), dx, dy, max_x, max_y, nvx, nvy));
        }
        let mut inertia_animating = false;
        for (rest, dx, dy, max_x, max_y, nvx, nvy) in inertia_ops {
            let tx_key = crate::core::scroll_viewport::scroll_target_x_signal_key(&rest);
            let ty_key = crate::core::scroll_viewport::scroll_target_y_signal_key(&rest);
            let sx_key = crate::core::scroll_viewport::scroll_offset_x_signal_key(&rest);
            let sy_key = crate::core::scroll_viewport::scroll_offset_y_signal_key(&rest);
            let sx = crate::core::reactivity::use_signal(&sx_key, || 0.0_f32);
            let sy = crate::core::reactivity::use_signal(&sy_key, || 0.0_f32);
            let tx = crate::core::reactivity::use_signal(&tx_key, || sx.peek());
            let ty = crate::core::reactivity::use_signal(&ty_key, || sy.peek());
            let next_x = (tx.peek() + dx).clamp(0.0, max_x);
            let next_y = (ty.peek() + dy).clamp(0.0, max_y);
            tx.set_silent(next_x);
            ty.set_silent(next_y);
            if nvx.abs() > 0.01 || nvy.abs() > 0.01 {
                inertia_animating = true;
                self.bump_smooth_active_until(&rest, now);
            }
        }
        if inertia_animating {
            smooth_scroll_animating = true;
        }

        // First-order exponential smoothing toward wheel targets — no velocity ring / overshoot
        // (previous mass–spring used ζ≈0.98, slightly under-damped). τ chosen near Chromium smooth-scroll.
        // Content-pan fling seeds `scroll_velocity` above; this loop eases `offset` toward `target`.
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
            let keys = crate::core::scroll_viewport::signal_keys_for_rest_cached(
                rest,
                &mut self.scroll_signal_key_cache,
            );
            let sx = crate::core::reactivity::use_signal(keys.offset_x.as_str(), || 0.0_f32);
            let sy = crate::core::reactivity::use_signal(keys.offset_y.as_str(), || 0.0_f32);
            let tx = crate::core::reactivity::use_signal(keys.target_x.as_str(), || sx.peek());
            let ty = crate::core::reactivity::use_signal(keys.target_y.as_str(), || sy.peek());
            let behavior = crate::core::reactivity::use_signal(keys.behavior.as_str(), || {
                crate::core::scroll_viewport::DEFAULT_SCROLL_BEHAVIOR.to_string()
            })
            .peek();
            let old_sx = sx.peek();
            let old_sy = sy.peek();
            let raw_max_x = mx.max(0.0);
            let raw_max_y = my.max(0.0);
            let lp = self
                .scroll_max_lowpass
                .entry(rest.clone())
                .or_insert([raw_max_x, raw_max_y]);
            // Content grew (e.g. textarea newlines) — raise soft max before target clamp.
            if raw_max_x > lp[0] + 0.5 {
                lp[0] = raw_max_x;
            }
            if raw_max_y > lp[1] + 0.5 {
                lp[1] = raw_max_y;
            }
            // Persist clamped targets when max shrinks (e.g. viewport grew until content
            // fits). Offset-only clamp left a stale target; smooth-scroll then re-sought it
            // after a later shrink and scrolled the top out of view.
            let (sx_clamped, tx_clamped) =
                super::scroll_support::clamp_scroll_offset_and_target(old_sx, tx.peek(), raw_max_x);
            let (sy_clamped, ty_clamped) =
                super::scroll_support::clamp_scroll_offset_and_target(old_sy, ty.peek(), raw_max_y);
            if (tx.peek() - tx_clamped).abs() > 0.01 {
                tx.set_silent(tx_clamped);
            }
            if (ty.peek() - ty_clamped).abs() > 0.01 {
                ty.set_silent(ty_clamped);
            }
            let _ = (sx_clamped, sy_clamped);
            let target_x_lp = tx_clamped.clamp(0.0, lp[0].min(raw_max_x));
            let target_y_lp = ty_clamped.clamp(0.0, lp[1].min(raw_max_y));

            let is_dragging_this = self
                .scrollbar_drag
                .as_ref()
                .is_some_and(|d| d.rest == *rest);

            if crate::core::scroll_viewport::scroll_behavior_is_smooth(&behavior)
                && !is_dragging_this
            {
                // Clamp to raw extent only: low-pass max lags behind during layout changes and can
                // fight the smooth animator (perceived stutter).
                let mut target_x = tx_clamped;
                let mut target_y = ty_clamped;
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
                if (target_x - old_sx).abs() < SMOOTH_IDLE_EPS
                    && (target_y - old_sy).abs() < SMOOTH_IDLE_EPS
                {
                    // Idle, but still clamp offset into max (stale offset after max shrink).
                    pre_render_data.push((
                        rest.clone(),
                        old_sx,
                        old_sy,
                        sx_clamped,
                        sy_clamped,
                        sx,
                        sy,
                    ));
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
                // Do not zero `scroll_velocity` here: fling inertia decays in the block above.
                // Thumb drag still clears velocity in the `else` branch.
                if (target_x - cur_x).abs() > SMOOTH_SCROLL_ANIM_EPS
                    || (target_y - cur_y).abs() > SMOOTH_SCROLL_ANIM_EPS
                {
                    smooth_scroll_animating = true;
                }
                pre_render_data.push((
                    rest.clone(),
                    old_sx,
                    old_sy,
                    cur_x.clamp(0.0, raw_max_x),
                    cur_y.clamp(0.0, raw_max_y),
                    sx,
                    sy,
                ));
            } else {
                if is_dragging_this {
                    if let Some(v) = self.scroll_velocity.get_mut(rest) {
                        v[0] = 0.0;
                        v[1] = 0.0;
                    }
                }
                pre_render_data.push((
                    rest.clone(),
                    old_sx,
                    old_sy,
                    target_x_lp,
                    target_y_lp,
                    sx,
                    sy,
                ));
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
            || anim_tick.scrollbar_overlay
        {
            ScrollbarUpdateMode::Full
        } else {
            ScrollbarUpdateMode::ViewportsOnly
        };
        // `scrollbar_overlay` 与 Full 滚动条预更新同帧时，文本用 Fast 路径降低 prepare/GPU 压力。
        let text_force_fast_mode = self.scrollbar_drag.is_some()
            || smooth_scroll_animating
            || !self.scroll_accum.is_empty()
            || anim_tick.scrollbar_overlay;

        // `scroll_accum` 在本函数开头已 drain，此处恒空；平滑滚仍会使 `scrollbar_pass == Full`，
        // 与 overlay 弹簧一样可在 `Clean` 下跳过整树 Taffy（collect 用信号画滚动条）。
        let scrollbar_skip_flex_layout = self.scrollbar_drag.is_none()
            && (anim_tick.scrollbar_overlay || smooth_scroll_animating);

        let active_drag_rest = self.scrollbar_drag.as_ref().map(|d| d.rest.as_str());
        match state.render(
            [hx, hy],
            scrollbar_pass,
            active_drag_rest,
            text_force_fast_mode,
            scrollbar_skip_flex_layout,
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
                        let viewport_fid = format!(
                            "{}{}",
                            crate::core::scroll_viewport::SCROLL_AREA_FOCUS_PREFIX,
                            rest
                        );
                        let thx = format!(
                            "{}{}:x",
                            crate::components::scrollbar::SCROLLBAR_THUMB_FOCUS_PREFIX,
                            rest
                        );
                        let thy = format!(
                            "{}{}:y",
                            crate::components::scrollbar::SCROLLBAR_THUMB_FOCUS_PREFIX,
                            rest
                        );
                        let bx = super::tree_hit::find_bounds_by_focus_id(
                            &state.root_element,
                            &fx,
                            [0.0, 0.0],
                        )
                        .map(|(b, s)| [b[0] - s[0], b[1] - s[1], b[2], b[3]])
                        .unwrap_or([0.0, 0.0, 0.0, 0.0]);
                        let by = super::tree_hit::find_bounds_by_focus_id(
                            &state.root_element,
                            &fy,
                            [0.0, 0.0],
                        )
                        .map(|(b, s)| [b[0] - s[0], b[1] - s[1], b[2], b[3]])
                        .unwrap_or([0.0, 0.0, 0.0, 0.0]);
                        let th_bx = super::tree_hit::find_bounds_by_focus_id(
                            &state.root_element,
                            &thx,
                            [0.0, 0.0],
                        )
                        .map(|(b, s)| [b[0] - s[0], b[1] - s[1], b[2], b[3]])
                        .unwrap_or([0.0, 0.0, 0.0, 0.0]);
                        let th_by = super::tree_hit::find_bounds_by_focus_id(
                            &state.root_element,
                            &thy,
                            [0.0, 0.0],
                        )
                        .map(|(b, s)| [b[0] - s[0], b[1] - s[1], b[2], b[3]])
                        .unwrap_or([0.0, 0.0, 0.0, 0.0]);
                        let inset_x =
                            super::tree_hit::find_element_by_focus_id(&state.root_element, &fx)
                                .map(|e| Self::fmt_inset_rect(e.style.inset))
                                .unwrap_or_else(|| "n/a".to_string());
                        let inset_y =
                            super::tree_hit::find_element_by_focus_id(&state.root_element, &fy)
                                .map(|e| Self::fmt_inset_rect(e.style.inset))
                                .unwrap_or_else(|| "n/a".to_string());
                        let viewport_padding = super::tree_hit::find_element_by_focus_id(
                            &state.root_element,
                            &viewport_fid,
                        )
                        .map(|e| Self::fmt_padding_rect(e.style.padding))
                        .unwrap_or_else(|| "n/a".to_string());
                        let outer_padding =
                            Self::find_element_by_template_id(&state.root_element, rest)
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

                // Prefer hit-test over paint-order `last_hover_cursor`: layout DFS can leave a
                // sticky Pointer from gallery scrollbars when a claiming underlay has no cursor.
                let hover_cursor_pick =
                    super::tree_hit::pick_cursor(&state.root_element, hx, hy, [0.0, 0.0]);
                let mut hover_to_run = Vec::new();
                super::tree_hit::update_hovers(
                    &state.root_element,
                    hx,
                    hy,
                    [0.0, 0.0],
                    &mut hover_to_run,
                );

                window.set_cursor(
                    match hover_cursor_pick.unwrap_or(crate::core::tree::CursorKind::Default) {
                        crate::core::tree::CursorKind::Pointer => {
                            winit::window::CursorIcon::Pointer
                        }
                        crate::core::tree::CursorKind::Text => winit::window::CursorIcon::Text,
                        crate::core::tree::CursorKind::NotAllowed => {
                            winit::window::CursorIcon::NotAllowed
                        }
                        crate::core::tree::CursorKind::NwseResize => {
                            winit::window::CursorIcon::NwseResize
                        }
                        crate::core::tree::CursorKind::Default => {
                            winit::window::CursorIcon::Default
                        }
                    },
                );

                let focused =
                    crate::core::reactivity::use_signal("__rui_button_focus_id", String::new);
                let fid = focused.get();
                let ui_scale = state.ui_scale();
                let (win_w, win_h) = {
                    let s = state.window.inner_size();
                    let scale = ui_scale.max(0.001);
                    (s.width as f32 / scale, s.height as f32 / scale)
                };
                // Prefer painted text leaf (render_x/y already scroll-adjusted). Focus-box
                // − scroll_acc is fallback when the leaf was culled / not collected.
                let ime_data = if super::ime_support::is_text_edit_focus(&fid) {
                    let painted = super::ime_support::painted_text_edit_visual_bounds(
                        &state.text_nodes,
                        &fid,
                    );
                    let focus_fallback = super::tree_hit::find_bounds_by_focus_id(
                        &state.root_element,
                        &fid,
                        [0.0, 0.0],
                    );
                    match (painted, focus_fallback) {
                        (Some(visual), fb) => {
                            let scroll_acc = fb.map(|(_, sa)| sa).unwrap_or([0.0, 0.0]);
                            Some((visual, scroll_acc, fid.clone(), "painted"))
                        }
                        (None, Some((b, sa))) => {
                            let visual = [b[0] - sa[0], b[1] - sa[1], b[2], b[3]];
                            Some((visual, sa, fid.clone(), "focus_bounds"))
                        }
                        (None, None) => None,
                    }
                } else {
                    None
                };
                // Browser-like: scrolling the field out of the client does **not** blur.
                // IME anchor Y is clamped to the viewport later (candidate sticks to top/bottom).
                if let Some((visual, _, ref off_fid, _)) = ime_data.as_ref() {
                    if super::ime_support::visual_fully_outside_window(*visual, win_w, win_h) {
                        log::warn!(
                            "RUI_IME_DIAG offscreen-clamp fid={off_fid} visual=({:.1},{:.1} {:.1}x{:.1}) win={win_w:.0}x{win_h:.0}",
                            visual[0],
                            visual[1],
                            visual[2],
                            visual[3]
                        );
                    }
                }
                // Blink wake must follow painted carets, not only IME geometry. If
                // `find_bounds_by_focus_id` misses for a frame, clearing `caret_wake_at`
                // freezes the caret in its current on/off phase forever.
                let has_input_caret = state
                    .text_nodes
                    .iter()
                    .any(|n| n.style.input_caret.is_some());

                drop(re_borrow); // CRITICAL: Free the borrow before updating signals!

                // Post-render: low-pass scroll extents, viewport/content metrics, overlay scrollbar visibility.
                // Offsets were integrated before `render()` using metrics cached from the previous frame.
                let mut scrollbar_fade_animating = false;
                for (rest, bounds, [max_x, max_y], [content_w, content_h]) in scroll_metrics.iter()
                {
                    let keys = crate::core::scroll_viewport::signal_keys_for_rest_cached(
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
                    // Snap *up* immediately when content grows (textarea hard newlines) — a stale
                    // lp≈0 after an empty field would clamp wheel targets to 0 and look unscrollable.
                    const LP: f32 = 0.78;
                    if raw_max_x > lp[0] + 0.5 {
                        lp[0] = raw_max_x;
                    } else {
                        lp[0] = lp[0] * LP + raw_max_x * (1.0 - LP);
                    }
                    if raw_max_y > lp[1] + 0.5 {
                        lp[1] = raw_max_y;
                    } else {
                        lp[1] = lp[1] * LP + raw_max_y * (1.0 - LP);
                    }
                    let soft_max_x = lp[0].min(raw_max_x);
                    let soft_max_y = lp[1].min(raw_max_y);

                    let sx =
                        crate::core::reactivity::use_signal(keys.offset_x.as_str(), || 0.0_f32);
                    let sy =
                        crate::core::reactivity::use_signal(keys.offset_y.as_str(), || 0.0_f32);
                    let vw =
                        crate::core::reactivity::use_signal(keys.viewport_w.as_str(), || 0.0_f32);
                    let vh =
                        crate::core::reactivity::use_signal(keys.viewport_h.as_str(), || 0.0_f32);
                    let cw =
                        crate::core::reactivity::use_signal(keys.content_w.as_str(), || 0.0_f32);
                    let ch =
                        crate::core::reactivity::use_signal(keys.content_h.as_str(), || 0.0_f32);
                    let tx =
                        crate::core::reactivity::use_signal(keys.target_x.as_str(), || sx.peek());
                    let ty =
                        crate::core::reactivity::use_signal(keys.target_y.as_str(), || sy.peek());
                    let behavior =
                        crate::core::reactivity::use_signal(keys.behavior.as_str(), || {
                            crate::core::scroll_viewport::DEFAULT_SCROLL_BEHAVIOR.to_string()
                        })
                        .peek();
                    let is_smooth_mode =
                        crate::core::scroll_viewport::scroll_behavior_is_smooth(&behavior);
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
                    // Keep target signals inside the live max (same expand→shrink bug as pre-render).
                    if (tx.peek() - target_x).abs() > 0.01 {
                        tx.set_silent(target_x);
                    }
                    if (ty.peek() - target_y).abs() > 0.01 {
                        ty.set_silent(target_y);
                    }
                    let new_sx = sx.peek().clamp(0.0, raw_max_x);
                    let new_sy = sy.peek().clamp(0.0, raw_max_y);
                    // Signal-only: `re_borrow` was dropped so we cannot touch the element tree here;
                    // next pre-render applies offsets via `update_scroll_offset_in_place`.
                    if (sx.peek() - new_sx).abs() > 0.01 {
                        sx.set_silent(new_sx);
                    }
                    if (sy.peek() - new_sy).abs() > 0.01 {
                        sy.set_silent(new_sy);
                    }

                    let overflow_x =
                        crate::core::reactivity::use_signal(keys.overflow_x.as_str(), || {
                            "auto".to_string()
                        })
                        .peek();
                    let overflow_y =
                        crate::core::reactivity::use_signal(keys.overflow_y.as_str(), || {
                            "auto".to_string()
                        })
                        .peek();
                    let axis_x_enabled =
                        crate::core::scroll_viewport::scroll_overflow_allows_scrollbar(
                            overflow_x.as_str(),
                        );
                    let axis_y_enabled =
                        crate::core::scroll_viewport::scroll_overflow_allows_scrollbar(
                            overflow_y.as_str(),
                        );
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
                                .is_none_or(|t| now_log.duration_since(*t).as_millis() >= 600);
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
                                    &crate::core::scroll_viewport::scroll_scrollbar_inset_x_signal_key(rest),
                                    || "inside".to_string(),
                                )
                                .peek();
                                let inset_mode_y = crate::core::reactivity::use_signal(
                                    &crate::core::scroll_viewport::scroll_scrollbar_inset_y_signal_key(rest),
                                    || "inside".to_string(),
                                )
                                .peek();
                                let side_x = crate::core::reactivity::use_signal(
                                    &crate::core::scroll_viewport::scroll_scrollbar_x_side_signal_key(rest),
                                    || "bottom".to_string(),
                                )
                                .peek();
                                let side_y = crate::core::reactivity::use_signal(
                                    &crate::core::scroll_viewport::scroll_scrollbar_y_side_signal_key(rest),
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
                    let activity =
                        crate::core::reactivity::use_signal(keys.activity.as_str(), || 0.0_f32);
                    let is_hover =
                        crate::core::reactivity::use_signal(keys.hover_legacy.as_str(), || false)
                            .peek();
                    let is_hover_x =
                        crate::core::reactivity::use_signal(keys.hover_axis_x.as_str(), || false)
                            .peek();
                    let is_hover_y =
                        crate::core::reactivity::use_signal(keys.hover_axis_y.as_str(), || false)
                            .peek();
                    let at_target =
                        (target_x - new_sx).abs() < 0.1 && (target_y - new_sy).abs() < 0.1;
                    let is_dragging = self
                        .scrollbar_drag
                        .as_ref()
                        .is_some_and(|d| d.rest == *rest);
                    let visible_until_x = self
                        .scrollbar_visible_until
                        .get(&Self::scrollbar_axis_key(
                            rest,
                            super::scroll_support::ScrollAxis::X,
                        ))
                        .copied()
                        .unwrap_or(now);
                    let visible_until_y = self
                        .scrollbar_visible_until
                        .get(&Self::scrollbar_axis_key(
                            rest,
                            super::scroll_support::ScrollAxis::Y,
                        ))
                        .copied()
                        .unwrap_or(now);

                    let is_dragging_x = self.scrollbar_drag.as_ref().is_some_and(|d| {
                        d.rest == *rest && d.axis == super::scroll_support::ScrollAxis::X
                    });
                    let is_dragging_y = self.scrollbar_drag.as_ref().is_some_and(|d| {
                        d.rest == *rest && d.axis == super::scroll_support::ScrollAxis::Y
                    });

                    // Independent per-axis visibility.
                    // - hover over content: allow both
                    // - hover over scrollbar: only that axis
                    // - wheel: axis-specific visible_until
                    let keep_visible_x =
                        (is_hover || is_hover_x) || is_dragging_x || now < visible_until_x;
                    let keep_visible_y =
                        (is_hover || is_hover_y) || is_dragging_y || now < visible_until_y;

                    // Keep the old activity signal (used elsewhere) as “any axis visible”.
                    let keep_visible = is_hover
                        || is_dragging
                        || now < visible_until_x
                        || now < visible_until_y
                        || !at_target;
                    let target = if keep_visible || !at_target { 1.0 } else { 0.0 };
                    if activity.get() != target {
                        activity.set(target);
                    }

                    // Update springs per-axis: only `auto` uses hover/scroll fade; `always`/`never` are fixed.
                    let vis_x = crate::core::reactivity::use_signal(
                        keys.scrollbar_visibility_x.as_str(),
                        || "auto".to_string(),
                    )
                    .peek()
                    .to_ascii_lowercase();
                    let vis_y = crate::core::reactivity::use_signal(
                        keys.scrollbar_visibility_y.as_str(),
                        || "auto".to_string(),
                    )
                    .peek()
                    .to_ascii_lowercase();

                    if axis_x_enabled {
                        let spring_x = crate::style::use_scrollbar_visibility_spring(
                            keys.spring_x.as_str(),
                            0.0,
                        );
                        // `raw_max_*` can sit at 0 briefly (stale metrics / sub-pixel) while the axis is
                        // still hoverable. Never force the spring to 0 when that axis is actively hovered,
                        // dragged, or in the post-wheel visibility window — otherwise `auto` bars only
                        // appear after scrolling bumps `raw_max` above the threshold.
                        let spring_eligible_x =
                            raw_max_x > 0.5 || is_hover_x || is_dragging_x || now < visible_until_x;
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
                        let spring_y = crate::style::use_scrollbar_visibility_spring(
                            keys.spring_y.as_str(),
                            0.0,
                        );
                        let spring_eligible_y =
                            raw_max_y > 0.5 || is_hover_y || is_dragging_y || now < visible_until_y;
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
                    if super::scroll_support::apply_pending_scroll_item_into_view(
                        rest, new_vh, new_ch,
                    ) {
                        scrollbar_fade_animating = true;
                    }
                }
                self.scroll_metrics_cache = scroll_metrics
                    .into_iter()
                    .map(|(rest, b, max, content)| (rest, (b, max, content)))
                    .collect();
                let scrollbar_visibility_pending = self
                    .scrollbar_visible_until
                    .values()
                    .any(|&until| now < until);
                // Keep the redraw pump alive only while content is still animating.
                // For the scrollbar `visible_until` hold window, sleep until the deadline via
                // `about_to_wait` + WaitUntil (one wake starts fade-out) instead of ~1.5s of frames.
                // Switch 等 chrome 在本帧 render/sync 里才 `set` tween（晚于上方 tick），
                // 必须在此用 `any_ui_animation_active` 补泵，否则停在第 0 帧。
                if smooth_scroll_animating
                    || scrollbar_fade_animating
                    || super::scroll_support::has_pending_scroll_item_align()
                    || crate::style::any_ui_animation_active()
                {
                    window.request_redraw();
                } else if scrollbar_visibility_pending {
                    let earliest = self
                        .scrollbar_visible_until
                        .values()
                        .copied()
                        .filter(|&until| until > now)
                        .min();
                    if let Some(until) = earliest {
                        event_loop
                            .set_control_flow(winit::event_loop::ControlFlow::WaitUntil(until));
                    }
                }

                for (cb, is_hover) in hover_to_run {
                    (*cb.borrow_mut())(is_hover);
                }
                if crate::primitives::interactive::take_hover_paint_dirty() {
                    window.request_redraw();
                }
                match hover_cursor_pick {
                    Some(k) => window.set_cursor(super::tree_hit::cursor_kind_to_winit(k)),
                    None => window.set_cursor(CursorIcon::Default),
                }

                // Soft-blur (`ime_os_blur_at`): do not cancel yet — TIP candidate click can
                // briefly steal winit focus. Hard release runs after the deadline.
                self.flush_ime_os_blur_if_due(&window);
                if let Some((visual, scroll_acc, fid_clone, source)) = ime_data {
                    if self.os_window_focused {
                        let allow_ime = super::ime_support::text_edit_rest(&fid_clone)
                            .map(|rest| {
                                let key = crate::components::input::input_type_signal_key(rest);
                                let raw = crate::core::reactivity::use_signal(&key, || {
                                    crate::components::input::InputType::Text
                                        .as_str()
                                        .to_string()
                                })
                                .peek();
                                crate::components::input::InputType::parse(&raw).allows_ime()
                            })
                            .unwrap_or(true);
                        // Keep focus-owner sync even for password (field-switch cancel);
                        // only `set_ime_allowed` / cursor drive depend on `allows_ime`.
                        super::ime_support::sync_ime_focus_owner(&window, Some(&fid_clone));
                        super::ime_support::set_ime_allowed_latched(
                            &window,
                            allow_ime,
                            &mut self.ime_allowed_latched,
                        );
                        if allow_ime {
                            // Painted leaf is already inset; skip outer padding so caret_x is relative to
                            // the text box. Focus-bounds fallback still needs field padding.
                            let pad_zero = source == "painted";
                            let (caret_x_in_field, compose_x_in_field, line_h) = self
                                .estimate_ime_caret_geom(
                                    &fid_clone,
                                    visual,
                                    ui_scale,
                                    if pad_zero { Some(0.0) } else { None },
                                );
                            // IME caret box = text line, not the whole field chrome (avoids huge
                            // vertical gap under the input / wrong cLineHeight for TIP).
                            let caret_visual_y = super::ime_support::clamp_ime_anchor_y(
                                visual[1] + (visual[3] - line_h).max(0.0) * 0.5,
                                line_h,
                                win_h,
                            );
                            let compose_visual_x =
                                (visual[0] + compose_x_in_field).clamp(0.0, (win_w - 1.0).max(0.0));
                            let caret_w = 2.0_f32.max(1.0 / ui_scale.max(0.001));
                            let composing = {
                                let rest = super::ime_support::text_edit_rest(&fid_clone);
                                rest.map(|r| {
                                    let pk = crate::components::input::input_preedit_signal_key(r);
                                    let pre = crate::core::reactivity::use_signal(&pk, String::new)
                                        .peek();
                                    let active = crate::core::reactivity::use_signal(
                                        crate::components::IME_ACTIVE_SIGNAL,
                                        || false,
                                    )
                                    .peek();
                                    active && !pre.is_empty()
                                })
                                .unwrap_or(false)
                            };
                            // IME area / candidate anchor = composition origin (not the advancing
                            // insert caret — that caused horizontal candidate creep while typing).
                            let (px, py, pw, ph) = super::ime_support::physical_ime_cursor_area(
                                [visual[0], caret_visual_y, visual[2], line_h],
                                compose_x_in_field,
                                ui_scale,
                            );
                            super::ime_support::apply_ime_cursor_area(
                                &window,
                                compose_visual_x,
                                caret_visual_y,
                                caret_w,
                                line_h,
                                ui_scale,
                                scroll_acc,
                                composing,
                            );
                            super::ime_support::record_ime_cursor(
                                super::ime_support::ImeCursorSnapshot {
                                    focus_id: fid_clone,
                                    physical_x: px,
                                    physical_y: py,
                                    physical_w: pw,
                                    physical_h: ph,
                                    ui_scale,
                                    visual_x: visual[0],
                                    visual_y: caret_visual_y,
                                    caret_x_in_field,
                                    source,
                                    scroll_acc_y: scroll_acc[1],
                                },
                            );
                        }
                    } else if self.ime_os_blur_at.is_some() {
                        // Soft blur: Win IME path already refuses foreign-app HWND moves.
                    } else {
                        super::ime_support::sync_ime_focus_owner(&window, None);
                        super::ime_support::set_ime_allowed_latched(
                            &window,
                            false,
                            &mut self.ime_allowed_latched,
                        );
                    }
                } else {
                    super::ime_support::sync_ime_focus_owner(&window, None);
                    super::ime_support::set_ime_allowed_latched(
                        &window,
                        false,
                        &mut self.ime_allowed_latched,
                    );
                }

                // Caret blink: wake ~once per phase (~520ms), not every frame.
                if has_input_caret {
                    let wake = crate::renderer::caret_blink_next_toggle_at();
                    self.caret_wake_at = Some(wake);
                    event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(wake));
                } else {
                    self.caret_wake_at = None;
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
                    let _ = state.render([0.0, 0.0], ScrollbarUpdateMode::Full, None, false, false);
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

    pub(super) fn handle_mouse_button(
        &mut self,
        id: WindowId,
        element_state: ElementState,
        button: MouseButton,
    ) {
        match button {
            MouseButton::Left => {
                if element_state == ElementState::Pressed {
                    let pos = self.cursor_pos;
                    let x = pos.x as f32;
                    let y = pos.y as f32;
                    const DBL_MS: u128 = 400;
                    const DBL_PX: f32 = 6.0;
                    let now = std::time::Instant::now();
                    let is_double = self.last_click_at.is_some_and(|t| {
                        now.duration_since(t).as_millis() <= DBL_MS
                            && (x - self.last_click_pos[0]).abs() <= DBL_PX
                            && (y - self.last_click_pos[1]).abs() <= DBL_PX
                    });
                    self.click_count = if is_double {
                        self.click_count.saturating_add(1).max(2)
                    } else {
                        1
                    };
                    self.last_click_at = Some(now);
                    self.last_click_pos = [x, y];
                    self.begin_long_press(id, [x, y]);
                }
                if element_state == ElementState::Released {
                    self.cancel_long_press();
                }
                self.handle_mouse_input(id, element_state);
                if element_state == ElementState::Released {
                    // Order matters (matches comment historically intended here):
                    // 1) `on_click` first — opening a Menu/Combobox disarms underlay close
                    // 2) outside dismiss only if still armed (same release must not dismiss
                    //    what it just opened)
                    // 3) arm for the *next* gesture
                    //
                    // Previously arm ran *before* on_click, so open left the stack disarmed
                    // and the first outside click only cleared text focus (Combobox).
                    //
                    // Never call `open.set` / reconcile while AppState is borrowed — that
                    // panics at `runtime::try_reconcile_subtree` (`RefCell already borrowed`).
                    let scale = {
                        let state_borrow = self.state.borrow();
                        match state_borrow.as_ref() {
                            Some(state) if state.window.id() == id => {
                                state.window.scale_factor() as f32
                            }
                            _ => return,
                        }
                    };
                    let x = self.cursor_pos.x as f32 / scale.max(1.0);
                    let y = self.cursor_pos.y as f32 / scale.max(1.0);
                    let capture = super::pointer_capture_id();
                    let (path, cb_to_run, label_for_focus, on_exclusive, on_trigger) = {
                        let state_borrow = self.state.borrow();
                        let state = match state_borrow.as_ref() {
                            Some(s) if s.window.id() == id => s,
                            _ => return,
                        };
                        let path = if let Some(ref cap_id) = capture {
                            if let Some(el) =
                                super::pointer_events::find_by_focus_id(&state.root_element, cap_id)
                            {
                                vec![super::pointer_events::HitPathEntry {
                                    focus_id: el.focus_id.clone(),
                                    label_for: el
                                        .attrs
                                        .get(crate::components::label::ATTR_LABEL_FOR)
                                        .map(|s| s.trim().to_string())
                                        .filter(|s| !s.is_empty()),
                                    on_pointer_down: el.on_pointer_down.clone(),
                                    on_pointer_up: el.on_pointer_up.clone(),
                                    on_click: el.on_click.clone(),
                                    on_context_menu: el.on_context_menu.clone(),
                                    on_double_click: el.on_double_click.clone(),
                                    hoisted_portal: el
                                        .attrs
                                        .get(crate::layout::layout_tree::ATTR_HOISTED_PORTAL)
                                        .map(|s| s.as_str())
                                        == Some("1"),
                                    portal_key: el
                                        .attrs
                                        .get(crate::layout::layout_tree::ATTR_PORTAL_KEY)
                                        .map(|s| s.trim().to_string())
                                        .filter(|s| !s.is_empty()),
                                    floating_id: el
                                        .attrs
                                        .get(crate::utils::popup::ATTR_FLOATING)
                                        .map(|s| s.trim().to_string())
                                        .filter(|s| !s.is_empty()),
                                    bounds: el.layout_bounds,
                                }]
                            } else {
                                super::pointer_events::hit_path(&state.root_element, x, y)
                            }
                        } else {
                            super::pointer_events::hit_path(&state.root_element, x, y)
                        };
                        let cb_resolved = {
                            let (from_path, focus_via_label) =
                                super::pointer_events::resolve_click_from_path(
                                    &state.root_element,
                                    &path,
                                );
                            if from_path.is_some() {
                                (from_path, focus_via_label)
                            } else {
                                (
                                    super::pointer_events::resolve_click_with_capture(
                                        &state.root_element,
                                        x,
                                        y,
                                        capture.as_deref(),
                                    ),
                                    None,
                                )
                            }
                        };
                        let on_exclusive =
                            super::portal_overlay::hit_path_on_top_exclusive_surface(&path);
                        let on_trigger = crate::utils::popup::path_hits_floating_trigger(
                            &state.root_element,
                            &path,
                        );
                        (path, cb_resolved.0, cb_resolved.1, on_exclusive, on_trigger)
                    };
                    let detail = self.click_count.max(1);
                    let mut pev =
                        super::pointer_events::PointerEvent::new(x, y, MouseButton::Left, detail);
                    super::pointer_events::dispatch_pointer(
                        &path,
                        &mut pev,
                        super::pointer_events::PointerKind::Up,
                    );
                    if detail >= 2 && !pev.is_consumed() {
                        let mut pev2 = super::pointer_events::PointerEvent::new(
                            x,
                            y,
                            MouseButton::Left,
                            detail,
                        );
                        super::pointer_events::dispatch_pointer(
                            &path,
                            &mut pev2,
                            super::pointer_events::PointerKind::DoubleClick,
                        );
                    }
                    // Legacy `on_click` / Label `for`：在 pointer up 触发（对标 DOM click）。
                    if !pev.is_consumed() {
                        let via_label = label_for_focus.is_some();
                        if via_label || detail < 2 {
                            if let Some(fid) = label_for_focus {
                                crate::core::reactivity::use_signal(
                                    "__rui_button_focus_id",
                                    String::new,
                                )
                                .set(fid);
                            }
                            if let Some(cb) = cb_to_run {
                                (*cb.borrow_mut())();
                            }
                        }
                    }
                    // Use the pre-click path. Re-hitting after on_click is wrong: opening is
                    // already gated by `disarm_underlay_close`, and a nested Select item click
                    // unmounts the Menu so a second hit looks like an outside press on the parent
                    // DatePicker / ColorPicker popover.
                    let should_outside_dismiss = !on_exclusive && !on_trigger;
                    if should_outside_dismiss && super::portal_overlay::try_dismiss_outside_press()
                    {
                        super::runtime::request_redraw();
                    }
                    crate::utils::popup::arm_underlay_close();
                    if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                        st.window.request_redraw();
                    }
                }
            }
            MouseButton::Right => {
                if element_state != ElementState::Pressed {
                    return;
                }
                let (x, y, path) = {
                    let state_borrow = self.state.borrow();
                    let Some(state) = state_borrow.as_ref() else {
                        return;
                    };
                    if state.window.id() != id {
                        return;
                    }
                    let scale = state.window.scale_factor() as f32;
                    let x = self.cursor_pos.x as f32 / scale.max(1.0);
                    let y = self.cursor_pos.y as f32 / scale.max(1.0);
                    let path = super::pointer_events::hit_path(&state.root_element, x, y);
                    (x, y, path)
                };
                let mut pev = super::pointer_events::PointerEvent::new(x, y, MouseButton::Right, 1);
                super::pointer_events::dispatch_pointer(
                    &path,
                    &mut pev,
                    super::pointer_events::PointerKind::ContextMenu,
                );
                if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                    st.window.request_redraw();
                }
            }
            _ => {}
        }
    }
}
