use winit::{
    event::{ElementState, MouseButton, MouseScrollDelta},
    event_loop::ActiveEventLoop,
    window::WindowId,
};

use super::state::ScrollbarUpdateMode;
use crate::components::scrollbar::SCROLLBAR_TRACK_FOCUS_PREFIX;
use crate::renderer::wgpu_compat::SurfaceAcquireError;

/// Edge auto-scroll + selection update for an active input text drag.
/// Free function so secondary paths can call it while `secondaries` is borrowed.
pub(super) fn tick_input_drag_select_at(
    root: &mut crate::core::tree::Element,
    font_system: &mut glyphon::FontSystem,
    scale: f32,
    dt: f32,
    x: f32,
    y: f32,
    rest: &str,
    anchor: usize,
) -> bool {
    let hscroll = super::tree_hit::nudge_textarea_hscroll_for_drag(root, rest, x, y, dt);
    let vscroll = super::tree_hit::nudge_textarea_vscroll_for_drag(root, rest, y, dt);

    let Some((bounds, display_el, style, leaf_scroll)) =
        super::tree_hit::find_input_text_leaf_for_rest(root, None, rest)
    else {
        return matches!(hscroll, super::tree_hit::TextDragHScrollTick::Scrolled)
            || matches!(vscroll, super::tree_hit::TextDragVScrollTick::Scrolled);
    };
    let vk = crate::components::input::input_value_signal_key(rest);
    let pk = crate::components::input::input_preedit_signal_key(rest);
    let committed = crate::core::reactivity::use_signal(&vk, String::new).peek();
    let pre = crate::core::reactivity::use_signal(&pk, String::new).peek();
    let ui_scale = scale.max(1.0);
    let (mx, my, hit_bounds, hit_style) =
        super::tree_hit::input_drag_select_hit_space(root, rest, x, y, bounds, &style, leaf_scroll);
    // Same mapping as pointer-down. Raw `hit_test_input_text_index` clamps Y onto the
    // last ink line, so a press in empty chrome (caret → doc end) plus this tick
    // became a selection from that line to the end.
    let ix = if !style.overflow.allows_caret_scroll() {
        crate::renderer::hit_test_textarea_click_caret_index(
            font_system,
            &display_el,
            x + leaf_scroll[0],
            y + leaf_scroll[1],
            bounds,
            &hit_style,
            ui_scale,
            mx,
            my,
        )
    } else {
        crate::renderer::hit_test_input_text_index(
            font_system,
            &display_el,
            mx,
            my,
            hit_bounds,
            &hit_style,
            ui_scale,
        )
    };
    let caret_ix = if committed.is_empty() && pre.is_empty() {
        0
    } else {
        crate::components::input::caret_index_from_display_hit(rest, ix)
    };
    let lo = anchor.min(caret_ix);
    let hi = anchor.max(caret_ix);
    let sk = crate::components::input::input_selection_signal_key(rest);
    crate::core::reactivity::use_signal(&sk, || None::<(usize, usize)>).set_silent(Some((lo, hi)));
    let ck = crate::components::input::input_caret_signal_key(rest);
    crate::core::reactivity::use_signal(&ck, || None::<usize>).set_silent(Some(caret_ix));

    matches!(hscroll, super::tree_hit::TextDragHScrollTick::Scrolled)
        || matches!(vscroll, super::tree_hit::TextDragVScrollTick::Scrolled)
}

impl super::App {
    pub(super) fn bind_pointer_window(&mut self, id: WindowId) {
        if self.pointer_window == Some(id) {
            return;
        }
        let prev = self.pointer_window;
        self.pointer_window = Some(id);
        self.last_mouse_path = None;
        self.last_mouse_target = None;
        self.click_press_path = None;
        self.scrollbar_drag = None;
        self.content_scroll_drag = None;
        self.textarea_resize_drag = None;
        self.input_drag_rest = None;
        crate::utils::text_field::set_text_drag_select_active(false);
        self.label_drag_key = None;
        self.drag_reorder = None;
        self.left_button_down = false;
        self.mouse_buttons = 0;
        self.hover_scroll_rest = None;
        self.hover_scroll_axis = None;
        self.long_press = None;

        // Leaving the primary for a secondary: clear primary hover immediately so a later
        // primary redraw (animation / shared signals) cannot re-hit-test with secondary
        // client coords still sitting in `cursor_pos`.
        if let Some(prev_id) = prev {
            if !self.is_secondary_window(prev_id) && self.is_secondary_window(id) {
                self.clear_primary_hovers_out_of_window();
            }
        }
    }

    /// Force primary-tree hover leave (`x/y` outside any element). Safe when `state` is free.
    fn clear_primary_hovers_out_of_window(&mut self) {
        let Ok(mut borrow) = self.state.try_borrow_mut() else {
            // Caller holds `state`; primary redraw will clear via pointer_window check.
            if let Ok(borrow) = self.state.try_borrow() {
                if let Some(st) = borrow.as_ref() {
                    st.window.request_redraw();
                }
            }
            return;
        };
        let Some(state) = borrow.as_mut() else {
            return;
        };
        let mut hover_to_run = Vec::new();
        super::tree_hit::update_hovers(
            &state.root_element,
            -1.0,
            -1.0,
            [0.0, 0.0],
            &mut hover_to_run,
        );
        let window = state.window.clone();
        window.request_redraw();
        drop(borrow);
        for (cb, is_hover) in hover_to_run {
            (*cb.borrow_mut())(is_hover);
        }
        crate::app::reactive::drain_deferred_view_remounts();
        crate::components::internal::modal_motion::drain_pending_modal_unmounts();
        crate::components::internal::floating_motion::drain_pending_floating_unmounts();
        if crate::components::bubble::flush_reaction_hover_dismiss() {
            window.request_redraw();
        }
        if crate::components::avatar::flush_avatar_group_hover_dismiss() {
            window.request_redraw();
        }
    }

    /// Primary-only input wrapper (sets primary window scope). Secondary events must use
    /// [`Self::handle_secondary_*`] — they never borrow or swap the primary UI tree.
    pub(super) fn with_primary_input<R>(
        &mut self,
        id: WindowId,
        f: impl FnOnce(&mut Self) -> R,
    ) -> Option<R> {
        if self.is_secondary_window(id) {
            return None;
        }
        // Always accept primary-window events and re-bind pointer ownership. Do **not**
        // drop them while `pointer_window` is a secondary — that permanently freezes the
        // primary after the first secondary hover/click (Windows focus handoff).
        self.bind_pointer_window(id);
        // Commit primary scope for the rest of the event-loop turn. A restore-on-drop
        // guard would reinstate a prior secondary token and leave portal/focus routing
        // stuck on the secondary after every primary click.
        super::window_scope::force_primary();
        Some(f(self))
    }

    /// Live secondary UI slot (preferred) or registry secondary flag.
    pub(super) fn is_secondary_window(&self, id: WindowId) -> bool {
        self.secondaries.borrow().contains_key(&id) || self.window_registry.is_secondary(id)
    }

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
        let wid = window.id();
        if self.ime_owner_window.is_some() && self.ime_owner_window != Some(wid) {
            return;
        }
        log::warn!("RUI_IME_DIAG os blur hard-release (stop system IME drive)");
        super::ime_support::on_os_focus_lost(window);
        super::ime_support::sync_ime_focus_owner(window, None);
        super::ime_support::set_ime_allowed_latched(window, false, &mut self.ime_allowed_latched);
        if self.ime_owner_window == Some(wid) {
            self.ime_owner_window = None;
        }
    }

    /// Whether a pointer hit should keep the current Input/Textarea focus.
    pub(super) fn pointer_keeps_text_edit_focus(
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

    /// Keep InputOTP focus when the press hits the OTP root (or lands in its layout box).
    pub(super) fn pointer_keeps_otp_focus(
        root: &crate::core::tree::Element,
        focused: &str,
        path: &[super::pointer_events::HitPathEntry],
        x: f32,
        y: f32,
    ) -> bool {
        if !crate::components::input_otp::is_input_otp_focus(focused) {
            return false;
        }
        if path.iter().any(|e| e.focus_id.as_deref() == Some(focused)) {
            return true;
        }
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

    pub(super) fn handle_secondary_redraw(&mut self, id: WindowId, event_loop: &ActiveEventLoop) {
        super::window_scope::force_secondary_id(id);
        let suspended = self.is_suspended;
        let (has_caret, render_ok) = {
            let owner_slot = &mut self.ime_owner_window;
            let mut secs = self.secondaries.borrow_mut();
            let Some(sec) = secs.get_mut(&id) else {
                return;
            };
            let inner = sec.window.inner_size();
            if suspended || inner.width == 0 || inner.height == 0 {
                if !suspended && (inner.width == 0 || inner.height == 0) {
                    sec.window.request_redraw();
                }
                return;
            }
            // Continuous wrap=off edge auto-scroll while text drag-select is held still.
            let scale = sec.window.scale_factor() as f32;
            let scroll_dt = std::time::Instant::now()
                .duration_since(self.last_frame_time)
                .as_secs_f32()
                .clamp(1.0 / 240.0, 0.05);
            let input_drag_keep = if self.left_button_down {
                if let Some(rest) = self.input_drag_rest.as_deref() {
                    let x = self.cursor_pos.x as f32 / scale.max(1.0);
                    let y = self.cursor_pos.y as f32 / scale.max(1.0);
                    let anchor = self.input_drag_anchor;
                    tick_input_drag_select_at(
                        &mut sec.root_element,
                        sec.text_renderer.font_system_mut(),
                        scale,
                        scroll_dt,
                        x,
                        y,
                        rest,
                        anchor,
                    )
                } else {
                    false
                }
            } else {
                false
            };
            if input_drag_keep {
                sec.window.request_redraw();
            }
            let render_ok = match sec.render() {
                Ok(()) => true,
                Err(SurfaceAcquireError::Lost | SurfaceAcquireError::Outdated) => {
                    sec.recover_surface();
                    sec.window.request_redraw();
                    false
                }
                Err(SurfaceAcquireError::Validation) => {
                    log::error!("secondary window: surface validation error");
                    false
                }
                Err(SurfaceAcquireError::Timeout | SurfaceAcquireError::Occluded) => {
                    sec.window.request_redraw();
                    false
                }
            };
            let has_caret = render_ok && sec.has_input_caret;
            if render_ok {
                if let Some(at) = sec.ime_os_blur_at {
                    if sec.os_focused {
                        sec.ime_os_blur_at = None;
                    } else if std::time::Instant::now() >= at {
                        sec.ime_os_blur_at = None;
                        if owner_slot.is_none() || *owner_slot == Some(id) {
                            log::warn!("RUI_IME_DIAG os blur hard-release (secondary)");
                            super::ime_support::on_os_focus_lost(&sec.window);
                            super::ime_support::sync_ime_focus_owner(&sec.window, None);
                            super::ime_support::set_ime_allowed_latched(
                                &sec.window,
                                false,
                                &mut sec.ime_allowed_latched,
                            );
                            *owner_slot = None;
                        }
                    }
                }
                Self::drive_ime_on_painted_window(
                    &sec.window,
                    id,
                    &sec.root_element,
                    &sec.text_nodes,
                    Some(sec.text_renderer.font_system_mut()),
                    sec.os_focused,
                    sec.ime_os_blur_at.is_some(),
                    &mut sec.ime_allowed_latched,
                    owner_slot,
                );
            }
            (has_caret, render_ok)
        };

        if has_caret {
            let wake = crate::renderer::caret_blink_next_toggle_at();
            self.caret_wake_at = Some(wake);
            event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(wake));
        }

        if render_ok {
            if let Some(adapter) = self.secondary_accesskit.get_mut(&id) {
                let secondaries = &self.secondaries;
                adapter.update_if_active(|| {
                    let secs = secondaries.borrow();
                    let Some(sec) = secs.get(&id) else {
                        return crate::a11y::tree_update_from_elements(
                            &crate::core::tree::Element::new(),
                            None,
                        );
                    };
                    let focused =
                        crate::core::reactivity::use_signal("__rui_button_focus_id", String::new)
                            .peek();
                    let fid = if focused.is_empty() {
                        None
                    } else {
                        Some(focused.as_str())
                    };
                    crate::a11y::tree_update_from_elements(&sec.root_element, fid)
                });
            }
        }

        // Re-apply cursor after render when the pointer owns this window — primary redraw
        // (shared signals / animations) must not leave a stale default on secondary hover.
        if render_ok && self.pointer_window == Some(id) {
            let scale = {
                let secs = self.secondaries.borrow();
                secs.get(&id)
                    .map(|s| s.window.scale_factor() as f32)
                    .unwrap_or(1.0)
            };
            let x = self.cursor_pos.x as f32 / scale.max(1.0);
            let y = self.cursor_pos.y as f32 / scale.max(1.0);
            let (cursor_pick, window) = {
                let secs = self.secondaries.borrow();
                let Some(sec) = secs.get(&id) else {
                    return;
                };
                (
                    super::tree_hit::pick_cursor(&sec.root_element, x, y, [0.0, 0.0]),
                    sec.window.clone(),
                )
            };
            super::tree_hit::apply_cursor(
                &window,
                Some(event_loop),
                &mut self.custom_cursors,
                cursor_pick,
            );
        }
    }

    /// Bind / release OS IME on `window` from this frame's painted text-edit focus.
    fn drive_ime_on_painted_window(
        window: &winit::window::Window,
        window_id: WindowId,
        root: &crate::core::tree::Element,
        text_nodes: &[crate::core::tree::TextLayoutInfo],
        font_system: Option<&mut glyphon::FontSystem>,
        os_focused: bool,
        ime_os_blur_pending: bool,
        ime_allowed_latched: &mut bool,
        ime_owner_window: &mut Option<WindowId>,
    ) {
        let focused = crate::core::reactivity::use_signal("__rui_button_focus_id", String::new);
        let fid = focused.get();
        let ui_scale = window.scale_factor() as f32;
        let (win_w, win_h) = {
            let s = window.inner_size();
            let scale = ui_scale.max(0.001);
            (s.width as f32 / scale, s.height as f32 / scale)
        };
        let ime_data = if super::ime_support::is_text_edit_focus(&fid) {
            let painted = super::ime_support::painted_text_edit_visual_bounds(text_nodes, &fid);
            let focus_fallback = super::tree_hit::find_bounds_by_focus_id(root, &fid, [0.0, 0.0]);
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

        if let Some((visual, scroll_acc, fid_clone, source)) = ime_data {
            if os_focused {
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
                super::ime_support::sync_ime_focus_owner(window, Some(&fid_clone));
                super::ime_support::set_ime_allowed_latched(window, allow_ime, ime_allowed_latched);
                *ime_owner_window = Some(window_id);
                if allow_ime {
                    let pad_zero = source == "painted";
                    let (caret_x_in_field, compose_x_in_field, line_h) =
                        Self::estimate_ime_caret_geom_in(
                            root,
                            font_system,
                            &fid_clone,
                            visual,
                            ui_scale,
                            if pad_zero { Some(0.0) } else { None },
                        );
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
                            let pre = crate::core::reactivity::use_signal(&pk, String::new).peek();
                            let active = crate::core::reactivity::use_signal(
                                crate::components::IME_ACTIVE_SIGNAL,
                                || false,
                            )
                            .peek();
                            active && !pre.is_empty()
                        })
                        .unwrap_or(false)
                    };
                    let (px, py, pw, ph) = super::ime_support::physical_ime_cursor_area(
                        [visual[0], caret_visual_y, visual[2], line_h],
                        compose_x_in_field,
                        ui_scale,
                    );
                    super::ime_support::apply_ime_cursor_area(
                        window,
                        compose_visual_x,
                        caret_visual_y,
                        caret_w,
                        line_h,
                        ui_scale,
                        scroll_acc,
                        composing,
                    );
                    super::ime_support::record_ime_cursor(super::ime_support::ImeCursorSnapshot {
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
                    });
                }
            } else if ime_os_blur_pending {
                // Soft blur: TIP candidate may have stolen winit focus briefly.
            } else if *ime_owner_window == Some(window_id) || ime_owner_window.is_none() {
                super::ime_support::sync_ime_focus_owner(window, None);
                super::ime_support::set_ime_allowed_latched(window, false, ime_allowed_latched);
                *ime_owner_window = None;
            }
        } else if *ime_owner_window == Some(window_id) {
            super::ime_support::sync_ime_focus_owner(window, None);
            super::ime_support::set_ime_allowed_latched(window, false, ime_allowed_latched);
            *ime_owner_window = None;
        }
    }

    fn fmt_lp(v: taffy::prelude::LengthPercentage) -> String {
        match crate::layout::lp_kind(v) {
            crate::layout::DimKind::Length(px) => format!("{px:.1}px"),
            crate::layout::DimKind::Percent(p) => format!("{:.1}%", p * 100.0),
            crate::layout::DimKind::Auto => "0px".to_string(),
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
        match crate::layout::lpa_kind(v) {
            crate::layout::DimKind::Auto => "auto".to_string(),
            crate::layout::DimKind::Length(px) => format!("{px:.1}px"),
            crate::layout::DimKind::Percent(p) => format!("{:.1}%", p * 100.0),
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
        let mut rb = self.state.borrow_mut();
        let Some(st) = rb.as_mut() else {
            return (8.0, 8.0, 20.0);
        };
        Self::estimate_ime_caret_geom_in(
            &st.root_element,
            Some(st.text_renderer.font_system_mut()),
            focus_id,
            visual,
            ui_scale,
            pad_override,
        )
    }

    fn estimate_ime_caret_geom_in(
        root: &crate::core::tree::Element,
        mut font_system: Option<&mut glyphon::FontSystem>,
        focus_id: &str,
        visual: [f32; 4],
        ui_scale: f32,
        pad_override: Option<f32>,
    ) -> (f32, f32, f32) {
        let pad = pad_override.unwrap_or_else(|| {
            super::tree_hit::find_element_by_focus_id(root, focus_id)
                .map(|e| {
                    let [_, br, _, bl] = e.style.resolved_border_widths();
                    // CSS % padding resolves against the containing block width. For a
                    // stretched field, border-box → CB; subtract own border so we don't
                    // treat the stroke as part of the percentage basis.
                    let basis = (visual[2] - bl - br).max(0.0);
                    let pad_l = match crate::layout::lp_kind(e.style.padding.left) {
                        crate::layout::DimKind::Length(v) => v,
                        crate::layout::DimKind::Percent(p) => basis * p,
                        crate::layout::DimKind::Auto => 0.0,
                    };
                    pad_l + bl
                })
                .unwrap_or(8.0)
        });

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
        // Hard line containing the caret — needed for CSS text-align inline offset.
        let (_, caret_local) = crate::utils::text_field::caret_line_and_local(&display, caret_byte);
        let line_start = caret_byte.saturating_sub(caret_local);
        let line_end = display[line_start..]
            .find('\n')
            .map(|i| line_start + i)
            .unwrap_or(display.len());
        let line_full = &display[line_start..line_end];

        fn first_text_style(el: &crate::core::tree::Element) -> Option<crate::style::TextStyle> {
            crate::core::tree::find_in_tree(el, |n| n.text.is_some())
                .map(|n| n.text_style.as_ref().clone())
        }

        let text_style = super::tree_hit::find_input_text_leaf_for_rest(root, None, rest)
            .map(|(_, _, style, _)| style)
            .or_else(|| {
                super::tree_hit::find_element_by_focus_id(root, focus_id).and_then(first_text_style)
            })
            .unwrap_or_default();

        let line_h = crate::utils::text_field::text_edit_line_box_px(
            text_style.size_resolved(),
            text_style.line_height_px,
            text_style.line_height_scale,
        )
        .max(1.0);

        let caret_x = super::ime_support::estimate_caret_x_in_field(
            font_system.as_deref_mut(),
            caret_prefix,
            line_full,
            &text_style,
            visual,
            ui_scale,
            pad,
        );
        let compose_x = if compose_byte == caret_byte {
            caret_x
        } else {
            super::ime_support::estimate_caret_x_in_field(
                font_system.as_deref_mut(),
                compose_prefix,
                line_full,
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
        crate::core::tree::find_in_tree(el, |n| n.template_id.as_deref() == Some(template_id))
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

    /// Remove auto-hide deadlines that have already passed.
    /// Returns true if any expired (caller must request **one** redraw to start fade-out).
    pub(super) fn prune_expired_scrollbar_visibility(&mut self, now: std::time::Instant) -> bool {
        super::scroll_support::retain_future_instants(&mut self.scrollbar_visible_until, now)
    }

    pub(super) fn bump_smooth_active_until(&mut self, rest: &str, now: std::time::Instant) {
        // Small window to bridge discrete wheel ticks (prevents "move-stop-move").
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

    pub(super) fn update_mouse_buttons(&mut self, button: MouseButton, state: ElementState) {
        let bit = super::pointer_events::buttons_mask_bit(button);
        match state {
            ElementState::Pressed => self.mouse_buttons |= bit,
            ElementState::Released => self.mouse_buttons &= !bit,
        }
    }

    fn movement_delta(&self, x: f32, y: f32) -> [f32; 2] {
        match self.last_client_pos {
            Some([lx, ly]) => [x - lx, y - ly],
            None => [0.0, 0.0],
        }
    }

    pub(super) fn build_mouse_event(
        &self,
        x: f32,
        y: f32,
        button: MouseButton,
        detail: u8,
        event_type: &'static str,
        related_target: Option<String>,
    ) -> super::pointer_events::MouseEvent {
        let [mx, my] = self.movement_delta(x, y);
        let mut ev = super::pointer_events::MouseEvent::new(x, y, button, detail);
        ev.event_type = event_type;
        ev.buttons = self.mouse_buttons;
        ev.alt_key = self.modifiers.alt_key();
        ev.ctrl_key = self.modifiers.control_key();
        ev.meta_key = self.modifiers.super_key();
        ev.shift_key = self.modifiers.shift_key();
        ev.movement_x = mx;
        ev.movement_y = my;
        ev.screen_x = self.cursor_pos.x as f32;
        ev.screen_y = self.cursor_pos.y as f32;
        ev.page_x = x;
        ev.page_y = y;
        ev.related_target = related_target;
        ev
    }

    pub(super) fn dispatch_mouse_path(
        &mut self,
        path: &[super::pointer_events::HitPathEntry],
        x: f32,
        y: f32,
        button: MouseButton,
        detail: u8,
        event_type: &'static str,
        kind: super::pointer_events::PointerKind,
        related_target: Option<String>,
    ) -> super::pointer_events::MouseEvent {
        let mut ev = self.build_mouse_event(x, y, button, detail, event_type, related_target);
        super::pointer_events::dispatch_pointer(path, &mut ev, kind);
        ev
    }

    pub(super) fn sync_mouse_enter_leave(
        &mut self,
        x: f32,
        y: f32,
        path: &[super::pointer_events::HitPathEntry],
        button: MouseButton,
    ) {
        let new_key = path.last().and_then(|e| e.target_key());
        if self.last_mouse_target.as_deref() != new_key.as_deref() {
            if let Some(old_path) = self.last_mouse_path.take() {
                let related = new_key.clone();
                let _ = self.dispatch_mouse_path(
                    &old_path,
                    x,
                    y,
                    button,
                    0,
                    "mouseout",
                    super::pointer_events::PointerKind::Out,
                    related,
                );
            }
            if !path.is_empty() {
                let related = self.last_mouse_target.clone();
                let _ = self.dispatch_mouse_path(
                    path,
                    x,
                    y,
                    button,
                    0,
                    "mouseover",
                    super::pointer_events::PointerKind::Over,
                    related,
                );
            }
            self.last_mouse_target = new_key;
        }
        self.last_mouse_path = if path.is_empty() {
            None
        } else {
            Some(path.to_vec())
        };
    }

    pub(super) fn handle_cursor_moved(
        &mut self,
        id: WindowId,
        position: winit::dpi::PhysicalPosition<f64>,
        event_loop: &ActiveEventLoop,
    ) {
        let mut state_borrow = self.state.borrow_mut();
        let state = match (*state_borrow).as_mut() {
            Some(s) if s.window.id() == id => s,
            _ => return,
        };

        self.cursor_pos = position;
        self.cursor_inside_window = true;
        let phys_x = position.x as f32;
        let phys_y = position.y as f32;
        let scale = state.window.scale_factor() as f32;
        let x = position.x as f32 / scale.max(1.0);
        let y = position.y as f32 / scale.max(1.0);

        if (x - self.wheel_sticky_cursor[0]).abs() > 2.0
            || (y - self.wheel_sticky_cursor[1]).abs() > 2.0
        {
            self.wheel_sticky_rest = None;
        }
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

        let mut input_drag_keep_redraw = false;
        let mut text_drag_scrollbar_bump: Option<String> = None;
        let drag_sel_update: Option<(String, usize, usize, usize)> =
            if self.left_button_down && self.scrollbar_drag.is_none() {
                if self.input_drag_rest.is_some() {
                    let now = std::time::Instant::now();
                    let dt = now
                        .duration_since(self.last_frame_time)
                        .as_secs_f32()
                        .clamp(1.0 / 240.0, 0.05);
                    let rest = self.input_drag_rest.as_deref().unwrap();
                    let x = self.cursor_pos.x as f32 / scale.max(1.0);
                    let y = self.cursor_pos.y as f32 / scale.max(1.0);
                    let anchor = self.input_drag_anchor;
                    input_drag_keep_redraw = tick_input_drag_select_at(
                        &mut state.root_element,
                        state.text_renderer.font_system_mut(),
                        scale,
                        dt,
                        x,
                        y,
                        rest,
                        anchor,
                    );
                    if input_drag_keep_redraw {
                        text_drag_scrollbar_bump =
                            Some(crate::utils::text_field::textarea_scroll_rest(rest));
                    }
                    None
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
                        Some((key.clone(), lo, hi, ix))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

        let label_drag_sel_update = drag_sel_update.as_ref().and_then(|(key, lo, hi, _caret)| {
            if self.label_drag_key.as_deref() == Some(key.as_str()) {
                Some((key.clone(), *lo, *hi))
            } else {
                None
            }
        });
        let input_drag_needs_redraw = self.input_drag_rest.is_some() && self.left_button_down;

        if self.left_button_down && self.scrollbar_drag.is_none() {
            if let Some(drag) = self.drag_reorder.as_mut() {
                // When the pointer is not over any drag handle in this list, fall back to the
                // source-row index so hover does not stick on the last valid row.
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
        let mouse_path = super::pointer_events::hit_path(&state.root_element, x, y);

        // Always apply: occluders (modal underlay) return None and must reset to Default,
        // otherwise gallery scrollbar Pointer cursors stick through the mask.
        super::tree_hit::apply_cursor(
            &window,
            Some(event_loop),
            &mut self.custom_cursors,
            cursor_pick.clone(),
        );

        let drawer_dragging =
            crate::components::drawer::tick_drawer_drag(&mut state.root_element, x, y);
        if drawer_dragging {
            state.mark_full_redraw();
        }

        drop(state_borrow);
        if let Some(scroll_rest) = text_drag_scrollbar_bump {
            let now = std::time::Instant::now();
            self.bump_scrollbar_visible_until_axis(
                &scroll_rest,
                super::scroll_support::ScrollAxis::X,
                now,
            );
            self.bump_scrollbar_visible_until_axis(
                &scroll_rest,
                super::scroll_support::ScrollAxis::Y,
                now,
            );
        }
        self.note_long_press_move(phys_x, phys_y);
        self.sync_mouse_enter_leave(x, y, &mouse_path, MouseButton::Left);
        // Canvas ink: keep dispatching to the press target so the stroke follows the pointer
        // even if hit-test leaves the leaf (fast flicks / 1px miss).
        let canvas_press_path = (self.mouse_buttons & 1 != 0)
            .then(|| self.click_press_path.clone())
            .flatten()
            .filter(|p| p.iter().any(|e| e.canvas_paint.is_some()));
        let move_path = canvas_press_path.as_deref().unwrap_or(&mouse_path);
        self.dispatch_mouse_path(
            move_path,
            x,
            y,
            MouseButton::Left,
            0,
            "mousemove",
            super::pointer_events::PointerKind::Move,
            None,
        );
        self.last_client_pos = Some([x, y]);

        let slider_dragging = crate::components::slider::tick_slider_drag(x, y);
        let toast_dragging = crate::components::toast::tick_toast_drag(x, y);
        let color_dragging = crate::components::color_picker::tick_color_drag(x, y);
        let carousel_dragging = crate::components::carousel::tick_carousel_drag(x, y);
        let resizable_dragging = crate::components::resizable::tick_resizable_drag(x, y);
        let floating_panel_dragging =
            crate::utils::anchored_overlay::tick_floating_panel_drag(x, y);
        if slider_dragging
            || toast_dragging
            || color_dragging
            || carousel_dragging
            || resizable_dragging
            || floating_panel_dragging
            || drawer_dragging
            || crate::components::internal::modal_motion::tick_modal_presence()
            || crate::components::internal::floating_motion::tick_floating_presence()
            || crate::components::accordion::tick_accordion_panel_presence()
            || crate::components::collapsible::tick_collapsible_panel_presence()
            || crate::primitives::canvas::take_canvas_wants_redraw()
        {
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
                        .peek(),
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
                        .peek(),
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
            && !crate::components::resizable::is_resizable_dragging()
            && !crate::utils::anchored_overlay::is_floating_panel_dragging()
        {
            self.tick_content_scroll_drag(x, y);
        }
        if input_drag_needs_redraw || input_drag_keep_redraw {
            // Selection chrome is paint-synced via peek — must request a frame.
            // `input_drag_keep_redraw` keeps the loop alive while edge auto-scroll runs
            // with a stationary pointer.
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
        crate::app::reactive::drain_deferred_view_remounts();
        crate::components::internal::modal_motion::drain_pending_modal_unmounts();
        crate::components::internal::floating_motion::drain_pending_floating_unmounts();
        if crate::components::bubble::flush_reaction_hover_dismiss() {
            window.request_redraw();
        }
        if crate::components::avatar::flush_avatar_group_hover_dismiss() {
            window.request_redraw();
        }

        // Utility-class `:hover` (e.g. gallery `hover:scale-x-150`) has no `on_hover`.
        // Wake a frame when pointer geometry would change those samples — otherwise enter
        // waits for scrollbar auto-hide redraws and leave sticks until another chrome hover.
        if let Some(st) = (*self.state.borrow()).as_ref() {
            if crate::style::class_variant_hover_needs_frame(&st.root_element, [x, y]) {
                window.request_redraw();
            }
        }

        // Pick hover must run after tree-row on_hover callbacks (see try_pick_hover docs).
        #[cfg(feature = "devtools")]
        if crate::app::devtools::DevToolsState::is_pick() {
            let mut state_borrow = self.state.borrow_mut();
            if let Some(state) = (*state_borrow).as_mut() {
                let scale = state.window.scale_factor() as f32;
                let px = self.cursor_pos.x as f32 / scale.max(1.0);
                let py = self.cursor_pos.y as f32 / scale.max(1.0);
                let hover_changed =
                    crate::app::devtools::try_pick_hover(&state.root_element, px, py);
                if hover_changed {
                    let _ = crate::app::devtools::refresh_highlight(&mut state.root_element);
                    let _ = crate::app::devtools::sync_tree_row_hovers(&mut state.root_element);
                }
                // Center the Elements row only when the hovered path changes. Doing this on
                // every move fights smooth metrics and used to feedback into scroll→re-pick.
                let mut scrolled = false;
                if hover_changed {
                    let key = crate::app::devtools::DevToolsState::hover_path_key();
                    if !key.is_empty() {
                        scrolled = crate::app::devtools::scroll_elements_tree_row_into_center(
                            &state.root_element,
                            &key,
                        );
                    }
                }
                if hover_changed || scrolled {
                    window.request_redraw();
                }
            }
        }

        if crate::primitives::interactive::take_hover_paint_dirty() {
            window.request_redraw();
        }
        if self.textarea_resize_drag.is_some() {
            super::tree_hit::apply_cursor(
                &window,
                Some(event_loop),
                &mut self.custom_cursors,
                Some(crate::core::tree::CursorKind::NwseResize.into()),
            );
        } else if self.drag_reorder.is_some() && self.left_button_down {
            super::tree_hit::apply_cursor(
                &window,
                Some(event_loop),
                &mut self.custom_cursors,
                Some(crate::core::tree::CursorKind::Grabbing.into()),
            );
        } else {
            super::tree_hit::apply_cursor(
                &window,
                Some(event_loop),
                &mut self.custom_cursors,
                cursor_pick,
            );
        }
    }

    pub(super) fn handle_cursor_left(&mut self, id: WindowId, event_loop: &ActiveEventLoop) {
        let mut state_borrow = self.state.borrow_mut();
        let state = match (*state_borrow).as_mut() {
            Some(s) if s.window.id() == id => s,
            _ => return,
        };
        let window = state.window.clone();
        drop(state_borrow);

        self.cursor_inside_window = false;

        #[cfg(feature = "devtools")]
        if crate::app::devtools::DevToolsState::is_pick() {
            crate::app::devtools::DevToolsState::clear_hover_highlight();
            if let Some(state) = (*self.state.borrow_mut()).as_mut() {
                let _ = crate::app::devtools::refresh_highlight(&mut state.root_element);
                let _ = crate::app::devtools::sync_tree_row_hovers(&mut state.root_element);
                window.request_redraw();
            }
        }

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

        if let Some((prev_rest, prev_axis)) = self.hover_scroll_axis.take() {
            let k = Self::hover_key(&prev_rest, prev_axis);
            crate::core::reactivity::use_signal(&k, || false).set(false);
        }
        if let Some(prev) = self.hover_scroll_rest.take() {
            let k = format!("__rui_scroll_hover_{}", prev);
            crate::core::reactivity::use_signal(&k, || false).set(false);
        }
        // Always wake a frame: utility-class `:hover` must clear when the pointer leaves
        // the window, even if no scroll-area hover latch was set.
        window.request_redraw();
        super::tree_hit::apply_cursor(&window, Some(event_loop), &mut self.custom_cursors, None);
    }

    pub(super) fn handle_mouse_input(
        &mut self,
        id: WindowId,
        element_state: ElementState,
        event_loop: Option<&ActiveEventLoop>,
    ) {
        if element_state == ElementState::Pressed {
            // Drop a stale press path if this gesture never reaches mousedown dispatch
            // (scrollbar / resize / reorder early-return).
            self.click_press_path = None;
        }
        let mut state_borrow = self.state.borrow_mut();
        let state = match (*state_borrow).as_mut() {
            Some(s) if s.window.id() == id => s,
            _ => return,
        };

        let scale = state.window.scale_factor() as f32;
        let x = self.cursor_pos.x as f32 / scale.max(1.0);
        let y = self.cursor_pos.y as f32 / scale.max(1.0);

        #[cfg(feature = "devtools")]
        if element_state == ElementState::Pressed
            && crate::app::devtools::try_pick_click(&state.root_element, x, y)
        {
            let _ = crate::app::devtools::sync_highlight_after_pick_commit(&mut state.root_element);
            state.window.request_redraw();
            return;
        }

        let mut input_drag: Option<(String, usize, String, Option<(usize, usize)>)> = None;
        let mut label_drag: Option<(String, usize)> = None;
        if element_state == ElementState::Pressed {
            if let Some((rest, bounds, display_el, style, leaf_scroll, focus_id)) =
                super::tree_hit::find_input_text_at(&state.root_element, None, x, y, [0.0, 0.0])
            {
                let ui_scale = scale.max(1.0);
                let is_textarea = focus_id.starts_with(crate::components::TEXTAREA_FOCUS_PREFIX);
                let (mx, my, hit_bounds, hit_style) = if is_textarea {
                    super::tree_hit::input_text_edit_pointer_space(
                        &state.root_element,
                        &rest,
                        x,
                        y,
                        bounds,
                        &style,
                        leaf_scroll,
                    )
                } else {
                    (
                        x + leaf_scroll[0],
                        y + leaf_scroll[1],
                        bounds,
                        style.clone(),
                    )
                };
                let ix = if is_textarea {
                    let raw_mx = x + leaf_scroll[0];
                    crate::renderer::hit_test_textarea_click_caret_index(
                        state.text_renderer.font_system_mut(),
                        &display_el,
                        raw_mx,
                        y + leaf_scroll[1],
                        bounds,
                        &hit_style,
                        ui_scale,
                        mx,
                        my,
                    )
                } else {
                    crate::renderer::hit_test_input_text_index(
                        state.text_renderer.font_system_mut(),
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
                let committed = crate::core::reactivity::use_signal(&vk, String::new).get();
                let pre = crate::core::reactivity::use_signal(&pk, String::new).get();
                let caret_ix = if committed.is_empty() && pre.is_empty() {
                    0
                } else {
                    crate::components::input::caret_index_from_display_hit(&rest, ix)
                };
                // Double-click in the blank beside the line ink — select that hard line.
                let line_sel = if self.click_count >= 2
                    && crate::renderer::hit_test_input_is_line_side_blank(
                        state.text_renderer.font_system_mut(),
                        &display_el,
                        if is_textarea { x + leaf_scroll[0] } else { mx },
                        my,
                        bounds,
                        if is_textarea { &hit_style } else { &style },
                        ui_scale,
                    ) {
                    crate::utils::keyboard::select_line_range(&committed, caret_ix)
                } else {
                    None
                };
                input_drag = Some((rest, caret_ix, focus_id, line_sel));
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
                        crate::utils::text_field::set_text_drag_select_active(false);
                        self.label_drag_key = None;
                        let focus_visible =
                            crate::core::reactivity::use_signal("__rui_focus_visible", || false);
                        focus_visible.set(false);
                        self.left_button_down = true;
                        if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                            super::tree_hit::apply_cursor(
                                &st.window,
                                event_loop,
                                &mut self.custom_cursors,
                                Some(crate::core::tree::CursorKind::NwseResize.into()),
                            );
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
                    vec![super::pointer_events::HitPathEntry::from_element(
                        el,
                        el.layout_bounds,
                    )]
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
            let mut pev =
                self.build_mouse_event(x, y, MouseButton::Left, detail, "mousedown", None);
            super::pointer_events::dispatch_pointer(
                &path,
                &mut pev,
                super::pointer_events::PointerKind::Down,
            );
            self.click_press_path = Some(path.clone());
            // Slider / other controls that stop_propagation must not also pan the scroll parent.
            if pev.is_consumed() {
                self.content_scroll_drag = None;
            }
            // `on_click` / Label `for` fire on pointer up (DOM click timing).

            let input_drag_fid = input_drag.as_ref().map(|(_, _, fid, _)| fid.clone());
            if let Some((rest, caret_ix, focus_id, line_sel)) = input_drag {
                let is_textarea_drag =
                    focus_id.starts_with(crate::components::TEXTAREA_FOCUS_PREFIX);
                // Hit-tested caret placement: ensure DOM focus even if `on_click` was consumed
                // (e.g. by an ancestor pointer handler), so caret/selection paint can run.
                crate::core::reactivity::use_signal("__rui_button_focus_id", String::new)
                    .set(focus_id);
                let ck = crate::components::input::input_caret_signal_key(&rest);
                let sk = crate::components::input::input_selection_signal_key(&rest);
                if let Some((lo, hi)) = line_sel {
                    crate::core::reactivity::use_signal(&ck, || None::<usize>).set(Some(hi));
                    crate::core::reactivity::use_signal(&sk, || None::<(usize, usize)>)
                        .set(Some((lo, hi)));
                    self.input_drag_anchor = lo;
                } else {
                    crate::core::reactivity::use_signal(&ck, || None::<usize>).set(Some(caret_ix));
                    crate::core::reactivity::use_signal(&sk, || None::<(usize, usize)>)
                        .set(Some((caret_ix, caret_ix)));
                    self.input_drag_anchor = caret_ix;
                }
                self.input_drag_rest = Some(rest.clone());
                crate::utils::text_field::set_text_drag_select_active(true);
                if is_textarea_drag {
                    let scroll_rest = crate::utils::text_field::textarea_scroll_rest(&rest);
                    let now = std::time::Instant::now();
                    self.bump_scrollbar_visible_until_axis(
                        &scroll_rest,
                        super::scroll_support::ScrollAxis::X,
                        now,
                    );
                    self.bump_scrollbar_visible_until_axis(
                        &scroll_rest,
                        super::scroll_support::ScrollAxis::Y,
                        now,
                    );
                }
                self.label_drag_key = None;
                self.content_scroll_drag = None;
                // End wheel-bridge so the focus frame is not stuck in text_force_fast.
                self.smooth_active_until.clear();
                self.left_button_down = true;
                need_paint_sync_redraw = true;
            } else if let Some((key, caret_ix)) = label_drag {
                let sk = crate::components::label::label_selection_signal_key(&key);
                crate::core::reactivity::use_signal(&sk, || None::<(usize, usize)>)
                    .set(Some((caret_ix, caret_ix)));
                self.label_drag_key = Some(key);
                self.label_drag_anchor = caret_ix;
                self.input_drag_rest = None;
                crate::utils::text_field::set_text_drag_select_active(false);
                self.content_scroll_drag = None;
                self.left_button_down = true;
                need_paint_sync_redraw = true;
            }

            // Browser-like: pointer down outside the focused text field / InputOTP clears focus.
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
                } else if crate::components::input_otp::is_input_otp_focus(&focused) {
                    let keep = if let Some(st) = (*self.state.borrow()).as_ref() {
                        Self::pointer_keeps_otp_focus(&st.root_element, &focused, &path, x, y)
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
            crate::components::carousel::clear_carousel_drag_if_any();
            crate::components::resizable::clear_resizable_drag_if_any();
            crate::components::drawer::clear_drawer_drag_if_any();
            crate::utils::anchored_overlay::clear_floating_panel_drag();
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
                crate::utils::text_field::set_text_drag_select_active(false);
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
    fn commit_wheel_delta(&mut self, rest: String, dx: f32, dy: f32, max_x: f32, max_y: f32) {
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

        let keys = crate::core::scroll_viewport::signal_keys_for_rest_cached(
            &rest,
            &mut self.scroll_signal_key_cache,
        )
        .clone();
        let behavior = crate::core::reactivity::use_signal(keys.behavior.as_str(), || {
            crate::core::scroll_viewport::DEFAULT_SCROLL_BEHAVIOR.to_string()
        })
        .peek();
        let is_smooth_mode = crate::core::scroll_viewport::scroll_behavior_is_smooth(&behavior);

        let sx = crate::core::reactivity::use_signal(keys.offset_x.as_str(), || 0.0_f32);
        let sy = crate::core::reactivity::use_signal(keys.offset_y.as_str(), || 0.0_f32);
        let tx = crate::core::reactivity::use_signal(keys.target_x.as_str(), || sx.peek());
        let ty = crate::core::reactivity::use_signal(keys.target_y.as_str(), || sy.peek());

        if is_smooth_mode {
            crate::components::chart::set_chart_hover_frozen(true);
            let next_x = (tx.get() - dx).clamp(0.0, max_x);
            let next_y = (ty.get() - dy).clamp(0.0, max_y);
            tx.set_silent(next_x);
            ty.set_silent(next_y);
            self.bump_smooth_active_until(&rest, now);
            crate::core::reactivity::use_signal(keys.activity.as_str(), || 0.0_f32).set_silent(1.0);
        } else {
            let entry = self.scroll_accum.entry(rest.clone()).or_insert([0.0, 0.0]);
            entry[0] += dx;
            entry[1] += dy;
        }
        // `auto` bar spring targets are set post-render from `visible_until`, same beat as exit:
        // setting 1 before tick lets the first idle-frame dt clamp still hit ~0 and accumulate a jump.
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

        // Command overlay / context menu / dropdown: page behind the surface must not scroll.
        // Overlay-internal ScrollAreas still receive wheel when the pointer is on the hoisted panel.
        let wheel_path = super::pointer_events::hit_path(&state.root_element, x, y);
        if super::portal_overlay::overlay_blocks_page_wheel(&wheel_path) {
            self.wheel_sticky_rest = None;
            drop(state_borrow);
            return;
        }

        let cursor_stable = (x - self.wheel_sticky_cursor[0]).abs() < 2.0
            && (y - self.wheel_sticky_cursor[1]).abs() < 2.0;
        if let Some(sticky) = self.wheel_sticky_rest.clone() {
            if cursor_stable
                && super::scroll_support::wheel_sticky_matches_deepest(&sticky, &target_rests)
            {
                if let Some((_, [max_x, max_y], _)) = super::scroll_support::wheel_metrics_for_rest(
                    &sticky,
                    &self.scroll_metrics_cache,
                ) {
                    let keys = crate::core::scroll_viewport::signal_keys_for_rest_cached(
                        &sticky,
                        &mut self.scroll_signal_key_cache,
                    )
                    .clone();
                    let sx =
                        crate::core::reactivity::use_signal(keys.offset_x.as_str(), || 0.0_f32)
                            .peek();
                    let sy =
                        crate::core::reactivity::use_signal(keys.offset_y.as_str(), || 0.0_f32)
                            .peek();
                    let bx =
                        crate::core::reactivity::use_signal(keys.target_x.as_str(), || sx).peek();
                    let by =
                        crate::core::reactivity::use_signal(keys.target_y.as_str(), || sy).peek();
                    let can_move_x = (dx > 0.0 && bx > 0.0) || (dx < 0.0 && bx < max_x);
                    let can_move_y = (dy > 0.0 && by > 0.0) || (dy < 0.0 && by < max_y);
                    if can_move_x || can_move_y {
                        let window = state.window.clone();
                        drop(state_borrow);
                        self.commit_wheel_delta(sticky, dx, dy, max_x, max_y);
                        window.request_redraw();
                        return;
                    }
                }
            }
            self.wheel_sticky_rest = None;
        }

        let metrics_for_rest = |rest: &str| {
            super::scroll_support::wheel_metrics_for_rest(rest, &self.scroll_metrics_cache)
        };

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
            self.wheel_sticky_rest = Some(rest.clone());
            self.wheel_sticky_cursor = [x, y];
            self.commit_wheel_delta(rest, dx, dy, max_x, max_y);

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

    /// Keyboard scroll: arrows / PageUp·Down / Home·End on the deepest scrollport under
    /// the cursor (same chain as wheel). `prefer_rest` pins a viewport (textarea inner).
    /// Returns true when the key was consumed.
    pub(super) fn try_keyboard_scroll(
        &mut self,
        event: &winit::event::KeyEvent,
        prefer_rest: Option<&str>,
    ) -> bool {
        use winit::event::ElementState;
        use winit::keyboard::{Key, NamedKey};

        if event.state != ElementState::Pressed {
            return false;
        }
        if self.modifiers.shift_key() || self.modifiers.alt_key() {
            return false;
        }
        // Ctrl+scroll keys scroll the page while a field keeps focus (caret nav uses plain keys).
        if self.modifiers.control_key()
            && !matches!(
                &event.logical_key,
                Key::Named(NamedKey::PageUp | NamedKey::PageDown | NamedKey::Home | NamedKey::End)
            )
        {
            return false;
        }

        enum ScrollAction {
            Delta(f32, f32),
            Absolute(f32, f32),
        }

        let lines = super::scroll_support::get_system_scroll_lines();
        let line_height = 25.0;
        let action = match &event.logical_key {
            Key::Named(NamedKey::ArrowUp) => ScrollAction::Delta(0.0, lines * line_height),
            Key::Named(NamedKey::ArrowDown) => ScrollAction::Delta(0.0, -lines * line_height),
            Key::Named(NamedKey::ArrowLeft) => ScrollAction::Delta(lines * line_height, 0.0),
            Key::Named(NamedKey::ArrowRight) => ScrollAction::Delta(-lines * line_height, 0.0),
            Key::Named(NamedKey::PageUp) => ScrollAction::Delta(0.0, 0.0), // resolved per viewport
            Key::Named(NamedKey::PageDown) => ScrollAction::Delta(0.0, 0.0),
            Key::Named(NamedKey::Home) => ScrollAction::Absolute(0.0, 0.0),
            Key::Named(NamedKey::End) => ScrollAction::Absolute(f32::NAN, f32::NAN),
            _ => return false,
        };

        let mut state_borrow = self.state.borrow_mut();
        let state = match (*state_borrow).as_mut() {
            Some(s) => s,
            None => return false,
        };

        let scale_factor = state.window.scale_factor() as f32;
        let x = self.cursor_pos.x as f32 / scale_factor.max(1.0);
        let y = self.cursor_pos.y as f32 / scale_factor.max(1.0);

        let wheel_path = super::pointer_events::hit_path(&state.root_element, x, y);
        if super::portal_overlay::overlay_blocks_page_wheel(&wheel_path) {
            return false;
        }

        let mut target_rests = Vec::new();
        if let Some(rest) = prefer_rest.filter(|r| !r.is_empty()) {
            target_rests.push(rest.to_string());
        }
        super::scroll_support::collect_scroll_rest_chain_at(
            &state.root_element,
            x,
            y,
            [0.0, 0.0],
            &mut target_rests,
        );
        if let Some(rest) = prefer_rest.filter(|r| !r.is_empty()) {
            target_rests.retain(|r| r != rest);
            target_rests.insert(0, rest.to_string());
        }

        let metrics_for_rest = |rest: &str| {
            super::scroll_support::wheel_metrics_for_rest(rest, &self.scroll_metrics_cache)
        };

        let mut captured: Option<(String, ScrollAction, f32, f32)> = None;
        for rest in target_rests {
            let Some((bounds, [max_x, max_y], _)) = metrics_for_rest(&rest) else {
                continue;
            };
            let vh = bounds[3].max(1.0);
            let resolved = match action {
                ScrollAction::Delta(dx, dy) => {
                    let (dx, dy) = match &event.logical_key {
                        Key::Named(NamedKey::PageUp) => (0.0, vh),
                        Key::Named(NamedKey::PageDown) => (0.0, -vh),
                        _ => (dx, dy),
                    };
                    ScrollAction::Delta(dx, dy)
                }
                ScrollAction::Absolute(tx, ty) => {
                    let tx = if tx.is_nan() { max_x } else { tx };
                    let ty = if ty.is_nan() { max_y } else { ty };
                    ScrollAction::Absolute(tx, ty)
                }
            };

            let keys = crate::core::scroll_viewport::signal_keys_for_rest_cached(
                &rest,
                &mut self.scroll_signal_key_cache,
            );
            let sx = crate::core::reactivity::use_signal(keys.offset_x.as_str(), || 0.0_f32).peek();
            let sy = crate::core::reactivity::use_signal(keys.offset_y.as_str(), || 0.0_f32).peek();
            let bx = crate::core::reactivity::use_signal(keys.target_x.as_str(), || sx).peek();
            let by = crate::core::reactivity::use_signal(keys.target_y.as_str(), || sy).peek();

            let can_apply = match resolved {
                ScrollAction::Delta(dx, dy) => {
                    let can_move_x = (dx > 0.0 && bx > 0.0) || (dx < 0.0 && bx < max_x);
                    let can_move_y = (dy > 0.0 && by > 0.0) || (dy < 0.0 && by < max_y);
                    can_move_x || can_move_y
                }
                ScrollAction::Absolute(tx, ty) => {
                    (tx - bx).abs() > 0.5
                        || (ty - by).abs() > 0.5
                        || (tx > 0.5 && bx < tx)
                        || (ty > 0.5 && by < ty)
                }
            };
            if can_apply {
                captured = Some((rest, resolved, max_x, max_y));
                break;
            }
        }

        let window = state.window.clone();
        drop(state_borrow);
        if let Some((rest, resolved, max_x, max_y)) = captured {
            let now = std::time::Instant::now();
            match resolved {
                ScrollAction::Delta(dx, dy) => {
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
                    self.commit_wheel_delta(rest, dx, dy, max_x, max_y);
                }
                ScrollAction::Absolute(tx, ty) => {
                    if tx > 0.5 {
                        self.bump_scrollbar_visible_until_axis(
                            &rest,
                            super::scroll_support::ScrollAxis::X,
                            now,
                        );
                    }
                    if ty > 0.5 {
                        self.bump_scrollbar_visible_until_axis(
                            &rest,
                            super::scroll_support::ScrollAxis::Y,
                            now,
                        );
                    }
                    let smooth = super::scroll_support::apply_scroll_navigation_absolute(
                        &rest, tx, ty, max_x, max_y,
                    );
                    if smooth {
                        crate::components::chart::set_chart_hover_frozen(true);
                        self.bump_smooth_active_until(&rest, now);
                    }
                }
            }
            window.request_redraw();
            return true;
        }
        false
    }

    pub(super) fn handle_redraw_requested(&mut self, id: WindowId, event_loop: &ActiveEventLoop) {
        let _scope = super::window_scope::enter_primary();
        crate::app::reactive::drain_deferred_view_remounts();
        crate::components::internal::modal_motion::drain_pending_modal_unmounts();
        crate::components::internal::floating_motion::drain_pending_floating_unmounts();
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

                let mut requested_redraw = assets_reloaded;
                let file = super::hmr_log::display_path(&hot_reload.last_patch_file);

                if hot_reload.force_reload {
                    let new_root = super::with_ui_build_stack(|| (self.app_builder.borrow_mut())());
                    state.rebuild_ui(new_root);
                    requested_redraw = true;
                    super::hmr_log::update("full reload");
                } else if has_templates {
                    // Store even when the patched `ui!` is not in the live tree
                    // (unvisited Gallery section). Skip used to drop the patch,
                    // so entering the page later still rendered compile-time text.
                    let _injected = state.inject_hot_reload_partial(merged);
                    let n = merged.len();
                    let unit = if n == 1 { "template" } else { "templates" };
                    let detail = if file.is_empty() {
                        format!("{n} {unit}")
                    } else {
                        format!("{file} ({n} {unit})")
                    };
                    if template_match {
                        requested_redraw = true;
                        super::hmr_log::update(&detail);
                    } else {
                        super::hmr_log::update(&format!("{detail}, deferred"));
                    }
                } else if assets_reloaded {
                    let asset = hot_reload
                        .asset_paths
                        .first()
                        .map(|p| super::hmr_log::display_path(p))
                        .unwrap_or_else(|| "asset".into());
                    super::hmr_log::update(&format!("{asset} (asset)"));
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
        self.smooth_active_until.retain(|_, until| *until > now);
        // Scroll / spring paths still clamp so a long pause does not teleport inertia.
        // CSS `transition-*` samples a monotonic Instant timeline (not this floored dt).
        let scroll_dt = dt.clamp(1.0 / 240.0, 0.08);
        let anim_dt = scroll_dt;

        // Continuous wrap=off edge auto-scroll while text drag-select is held still.
        let scale = state.window.scale_factor() as f32;
        let input_drag_keep = if self.left_button_down && self.scrollbar_drag.is_none() {
            if let Some(rest) = self.input_drag_rest.as_deref() {
                let x = self.cursor_pos.x as f32 / scale.max(1.0);
                let y = self.cursor_pos.y as f32 / scale.max(1.0);
                let anchor = self.input_drag_anchor;
                tick_input_drag_select_at(
                    &mut state.root_element,
                    state.text_renderer.font_system_mut(),
                    scale,
                    scroll_dt.min(0.05),
                    x,
                    y,
                    rest,
                    anchor,
                )
            } else {
                false
            }
        } else {
            false
        };

        let window = state.window.clone();
        crate::utils::lucide_animated::sync_animated_icon_hover(&mut state.root_element);
        let anim_icons = crate::utils::lucide_animated::tick_animated_icons_subtree(
            &mut state.root_element,
            anim_dt,
        );
        let gif_tick = crate::app::pending_images::tick_gif_textures(anim_dt);
        drop(state_borrow);

        if input_drag_keep {
            window.request_redraw();
        }

        crate::style::utility::set_frame_dt(dt.max(0.0));
        let anim_tick = crate::style::tick_animations(anim_dt);
        let nav_animating = crate::app::navigator::tick_active(anim_dt);
        if anim_tick.any || nav_animating || anim_icons || gif_tick {
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

        // First-order exponential smoothing toward wheel targets, quantized to device
        // pixels so glyphon glyphs and synthetic-bold shadow quads share one grid.
        let ui_scale = {
            let g = self.state.borrow();
            g.as_ref()
                .map(|s| s.window.scale_factor() as f32)
                .filter(|s| s.is_finite() && *s > 0.0)
                .unwrap_or(1.0)
        };
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
            let old_sx = if sx.peek().is_finite() {
                sx.peek()
            } else {
                0.0
            };
            let old_sy = if sy.peek().is_finite() {
                sy.peek()
            } else {
                0.0
            };
            if !sx.peek().is_finite() {
                sx.set_silent(0.0);
            }
            if !sy.peek().is_finite() {
                sy.set_silent(0.0);
            }
            let raw_max_x = if mx.is_finite() { mx.max(0.0) } else { 0.0 };
            let raw_max_y = if my.is_finite() { my.max(0.0) } else { 0.0 };
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
            let tx_raw = if tx.peek().is_finite() {
                tx.peek()
            } else {
                0.0
            };
            let ty_raw = if ty.peek().is_finite() {
                ty.peek()
            } else {
                0.0
            };
            let (sx_clamped, tx_clamped) =
                super::scroll_support::clamp_scroll_offset_and_target(old_sx, tx_raw, raw_max_x);
            let (sy_clamped, ty_clamped) =
                super::scroll_support::clamp_scroll_offset_and_target(old_sy, ty_raw, raw_max_y);
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
                target_x =
                    super::scroll_support::snap_scroll_offset_to_device_px(target_x, ui_scale)
                        .clamp(0.0, raw_max_x);
                target_y =
                    super::scroll_support::snap_scroll_offset_to_device_px(target_y, ui_scale)
                        .clamp(0.0, raw_max_y);
                if (tx.peek() - target_x).abs() > 0.001 {
                    tx.set_silent(target_x);
                }
                if (ty.peek() - target_y).abs() > 0.001 {
                    ty.set_silent(target_y);
                }
                // Nested ScrollArea: skip work for areas already at target (outer idle while inner scrolls).
                let idle_eps = (1.0 / ui_scale.max(0.01)) * 0.51;
                if (target_x - old_sx).abs() < idle_eps && (target_y - old_sy).abs() < idle_eps {
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
                let (cur_x, ax) = super::scroll_support::smooth_scroll_step(
                    old_sx, target_x, scroll_dt, ui_scale,
                );
                let (cur_y, ay) = super::scroll_support::smooth_scroll_step(
                    old_sy, target_y, scroll_dt, ui_scale,
                );
                if ax || ay {
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
        let mut rests_offset_moved = Vec::new();
        let mut overlay_scroll_moved = false;
        const OVERLAY_DISMISS_SCROLL_PX: f32 = 0.5;
        {
            let mut state_borrow = self.state.borrow_mut();
            if let Some(state) = state_borrow.as_mut() {
                for (rest, old_sx, old_sy, new_sx, new_sy, sx_sig, sy_sig) in pre_render_data {
                    state.update_scroll_offset_in_place(&rest, [new_sx, new_sy]);
                    if (new_sx - old_sx).abs() > 0.01 || (new_sy - old_sy).abs() > 0.01 {
                        rests_offset_moved.push(rest);
                    }
                    if (new_sx - old_sx).abs() > OVERLAY_DISMISS_SCROLL_PX
                        || (new_sy - old_sy).abs() > OVERLAY_DISMISS_SCROLL_PX
                    {
                        overlay_scroll_moved = true;
                    }
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
        // Bridge discrete wheel ticks + settle frame: keep paint-only / text-fast for 200ms after
        // any offset move so Input chrome + nowrap prepare do not hitch on every notch.
        for rest in &rests_offset_moved {
            self.bump_smooth_active_until(rest, now);
        }
        // Only freeze while this frame actually moved ≥0.5px. Tying freeze to
        // `smooth_scroll_animating` (any viewport catching up, including a 0.06px tail)
        // left hover-open stuck: +N enter fires once, `same_identity` will not re-enter.
        if overlay_scroll_moved {
            let _ = crate::components::avatar::dismiss_avatar_group_expands_on_scroll();
        }
        if crate::components::avatar::set_avatar_group_expand_hover_frozen(overlay_scroll_moved) {
            crate::app::runtime::request_redraw();
        }

        let mut re_borrow = match self.state.try_borrow_mut() {
            Ok(b) => b,
            Err(_) => {
                // Borrow contention can happen transiently during reactive callbacks.
                // Do not self-pump redraw here, otherwise we can end up in an idle
                // busy loop (high CPU while no visible state is changing).
                return;
            }
        };
        let state = re_borrow.as_mut().unwrap();

        // `cursor_pos` is shared across windows (last physical move). Primary redraw must
        // never hit-test with another window's client coords — that re-hovers Command/etc.
        // (secondary Input Y → "Show toast", Select Y → "Calendar").
        let pointer_on_primary = self.pointer_window == Some(state.window.id());
        let (hx, hy) = if pointer_on_primary && self.cursor_inside_window {
            (
                self.cursor_pos.x as f32 / state.window.scale_factor() as f32,
                self.cursor_pos.y as f32 / state.window.scale_factor() as f32,
            )
        } else {
            (-1.0, -1.0)
        };

        // Idle frames use ViewportsOnly so `update_scrollbar_elements` only syncs `scroll:` offsets
        // (plus subtree pruning via `Element::contains_scroll_viewport`). Full pass is needed while
        // dragging, smoothing scroll, coalescing wheel delta, or fading auto-scrollbar alpha.
        // Smooth scroll moves thumbs via overlay collect (live offset signals). Full prepass is
        // only required while dragging, fading auto-scrollbar alpha, or coalescing wheel.
        let scrollbar_fade_busy = self.scrollbar_fade_active;
        let scrollbar_pass = if self.scrollbar_drag.is_some()
            || !self.scroll_accum.is_empty()
            || scrollbar_fade_busy
        {
            ScrollbarUpdateMode::Full
        } else {
            ScrollbarUpdateMode::ViewportsOnly
        };
        // Live scroll / drag / wheel coalescing only — do NOT keep force_fast for the
        // 200ms `smooth_active_until` bridge. That flip invalidates non-input keys and,
        // with miss deferral, blanked Input placeholders right when the user starts
        // clicking fields after scroll stops.
        let text_force_fast_mode = self.scrollbar_drag.is_some()
            || smooth_scroll_animating
            || !self.scroll_accum.is_empty()
            || scrollbar_fade_busy;

        let scrollbar_drag_active = self.scrollbar_drag.is_some();
        let scrollbar_skip_flex_layout =
            scrollbar_drag_active || (self.scrollbar_drag.is_none() && scrollbar_fade_busy);
        // Do NOT fold `smooth_active_until` into paint-only: the 200ms wheel bridge would skip
        // `sync_text_edit` / focus ring after scroll stops, so a click on Input sets focus_id
        // but the caret does not install until the bridge expires.
        // Unfocused Input prepare skip already covers the chrome hitch bridging targeted.
        let scroll_paint_only =
            smooth_scroll_animating || scrollbar_fade_busy || scrollbar_drag_active;
        crate::components::chart::set_chart_hover_frozen(scroll_paint_only);

        let active_drag_rest = self.scrollbar_drag.as_ref().map(|d| d.rest.as_str());
        match state.render(
            [hx, hy],
            scrollbar_pass,
            active_drag_rest,
            text_force_fast_mode,
            scrollbar_skip_flex_layout,
            scroll_paint_only,
        ) {
            Ok(_) => {
                if let Some(adapter) = &mut self.accesskit_adapter {
                    // Build the a11y tree only when a screen reader is actually subscribed.
                    // Eager `tree_update_from_elements` on every scroll frame is a full-tree walk.
                    adapter.update_if_active(|| state.accesskit_tree_update());
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
                // Keep the previous cache until post-render so a 0-max cull sample cannot
                // clamp a nested scroller's offset before merge_scroll_metrics_cache runs.

                // Prefer hit-test over paint-order `last_hover_cursor`: layout DFS can leave a
                // sticky Pointer from gallery scrollbars when a claiming underlay has no cursor.
                let hover_cursor_pick =
                    super::tree_hit::pick_cursor(&state.root_element, hx, hy, [0.0, 0.0]);
                let mut hover_to_run = Vec::new();
                // Content moves under a stationary cursor during smooth scroll — re-diff
                // signal-driven hover (`on_hover` / Button chrome). CSS `:hover` variants
                // stay frozen via `tick_active_css_transitions_subtree` only.
                super::tree_hit::reconcile_pointer_hovers(
                    &state.root_element,
                    hx,
                    hy,
                    [0.0, 0.0],
                    &mut hover_to_run,
                    scroll_paint_only,
                );

                // Never reset the OS cursor from a primary redraw while the pointer lives on
                // another window — that would wipe secondary I-beam / Pointer mid-hover.
                if pointer_on_primary {
                    super::tree_hit::apply_cursor(
                        &window,
                        Some(event_loop),
                        &mut self.custom_cursors,
                        hover_cursor_pick.clone(),
                    );
                }

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
                // + scroll_acc is fallback when the leaf was culled / not collected.
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
                    .any(|n| n.style.input_caret.is_some())
                    || crate::components::input_otp::take_otp_caret_blink_needed();
                let svg_smil_active =
                    crate::primitives::svg::subtree_has_svg_smil(&state.root_element);
                let anim_icon_active =
                    crate::utils::lucide_animated::subtree_needs_animated_icon_frame(
                        &state.root_element,
                    );

                drop(re_borrow); // CRITICAL: Free the borrow before updating signals!

                // Post-render: low-pass scroll extents, viewport/content metrics, overlay scrollbar visibility.
                // Offsets were integrated before `render()` using metrics cached from the previous frame.
                let mut scrollbar_fade_animating = false;
                for (rest, bounds, [max_x, max_y], [content_w, content_h]) in scroll_metrics.iter()
                {
                    if super::scroll_support::is_stale_zero_max_sample(
                        [*max_x, *max_y],
                        self.scroll_metrics_cache.get(rest).map(|e| e.1),
                    ) {
                        continue;
                    }
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
                    // lp[1] after an empty field would clamp wheel targets to 0 and look unscrollable.
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

                    let text_drag_on_rest = crate::utils::text_field::text_drag_select_active()
                        && self.input_drag_rest.as_ref().is_some_and(|id| {
                            crate::utils::text_field::textarea_scroll_rest(id) == *rest
                        });

                    // Independent per-axis visibility.
                    // - hover over content: allow both
                    // - hover over scrollbar: only that axis
                    // - wheel: axis-specific visible_until
                    // - text drag-select edge auto-scroll: keep bars visible while scrolling
                    let keep_visible_x = (is_hover || is_hover_x)
                        || is_dragging_x
                        || now < visible_until_x
                        || text_drag_on_rest;
                    let keep_visible_y = (is_hover || is_hover_y)
                        || is_dragging_y
                        || now < visible_until_y
                        || text_drag_on_rest;

                    // Keep the old activity signal (used elsewhere) as "any axis visible".
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
                        let fade_x =
                            crate::style::use_ui_transition_cubic(keys.spring_x.as_str(), 0.0);
                        // `raw_max_*` can sit at 0 briefly (stale metrics / sub-pixel) while the axis is
                        // still hoverable. Never force fade to 0 when that axis is actively hovered,
                        // dragged, or in the post-wheel visibility window — otherwise `auto` bars only
                        // appear after scrolling bumps `raw_max` above the threshold.
                        let spring_eligible_x = raw_max_x > 0.5
                            || is_hover_x
                            || is_dragging_x
                            || now < visible_until_x
                            || text_drag_on_rest;
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
                            "never" => fade_x.set(0.0),
                            "always" => fade_x.set(1.0),
                            _ => {
                                if spring_eligible_x {
                                    fade_x.set(if keep_visible_x { 1.0 } else { 0.0 });
                                } else {
                                    fade_x.set(0.0);
                                }
                            }
                        }
                        if (fade_x.peek() - target_x_vis).abs() > 0.002 {
                            scrollbar_fade_animating = true;
                        }
                    }
                    if axis_y_enabled {
                        let fade_y =
                            crate::style::use_ui_transition_cubic(keys.spring_y.as_str(), 0.0);
                        let spring_eligible_y = raw_max_y > 0.5
                            || is_hover_y
                            || is_dragging_y
                            || now < visible_until_y
                            || text_drag_on_rest;
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
                            "never" => fade_y.set(0.0),
                            "always" => fade_y.set(1.0),
                            _ => {
                                if spring_eligible_y {
                                    fade_y.set(if keep_visible_y { 1.0 } else { 0.0 });
                                } else {
                                    fade_y.set(0.0);
                                }
                            }
                        }
                        if (fade_y.peek() - target_y_vis).abs() > 0.002 {
                            scrollbar_fade_animating = true;
                        }
                    }

                    let new_vw = bounds[2].max(0.0);
                    let new_vh = bounds[3].max(0.0);
                    let new_cw = content_w.max(0.0);
                    let new_ch = content_h.max(0.0);

                    // Update vw/vh/cw/ch **atomically** when any one crosses the noise threshold.
                    // Per-field updates let e.g. vh refresh while ch stays stale — `ch - vh` becomes 0,
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
                super::scroll_support::merge_scroll_metrics_cache(
                    &mut self.scroll_metrics_cache,
                    scroll_metrics,
                );
                let scrollbar_visibility_pending = self
                    .scrollbar_visible_until
                    .values()
                    .any(|&until| now < until);
                // Keep the redraw pump alive only while content is still animating.
                // For the scrollbar `visible_until` hold window, sleep until the deadline via
                // `about_to_wait` + WaitUntil (one wake starts fade-out) instead of ~1.5s of frames.
                // Switch chrome only sets its tween in this render/sync (after the prior tick);
                // pump `any_ui_animation_active` here or it stalls on frame 0.
                let ui_anim_active = crate::style::any_ui_animation_active();
                if smooth_scroll_animating
                    || scrollbar_fade_animating
                    || svg_smil_active
                    || anim_icon_active
                    || crate::app::pending_images::any_gif_playing()
                    || super::scroll_support::has_pending_scroll_item_align()
                    // `any_ui_animation_active()` is a post-tick safety net for late-set tweens.
                    // Gate it with `force_redraw_count` to avoid long-running stale animation
                    // flags keeping an idle redraw pump alive indefinitely.
                    || (ui_anim_active && self.force_redraw_count > 0)
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
                self.scrollbar_fade_active = scrollbar_fade_animating;

                for (cb, is_hover) in hover_to_run {
                    (*cb.borrow_mut())(is_hover);
                }
                crate::app::reactive::drain_deferred_view_remounts();
                crate::components::internal::modal_motion::drain_pending_modal_unmounts();
                crate::components::internal::floating_motion::drain_pending_floating_unmounts();
                if crate::components::bubble::flush_reaction_hover_dismiss() {
                    window.request_redraw();
                }
                if crate::components::avatar::flush_avatar_group_hover_dismiss() {
                    window.request_redraw();
                }
                if crate::primitives::interactive::take_hover_paint_dirty() {
                    window.request_redraw();
                }
                if pointer_on_primary {
                    super::tree_hit::apply_cursor(
                        &window,
                        Some(event_loop),
                        &mut self.custom_cursors,
                        hover_cursor_pick,
                    );
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
                        self.ime_owner_window = Some(window.id());
                    } else if self.ime_os_blur_at.is_some() {
                        // Soft blur: Win IME path already refuses foreign-app HWND moves.
                    } else if self.ime_owner_window == Some(window.id())
                        || self.ime_owner_window.is_none()
                    {
                        super::ime_support::sync_ime_focus_owner(&window, None);
                        super::ime_support::set_ime_allowed_latched(
                            &window,
                            false,
                            &mut self.ime_allowed_latched,
                        );
                        self.ime_owner_window = None;
                    }
                } else if self.ime_owner_window == Some(window.id()) {
                    super::ime_support::sync_ime_focus_owner(&window, None);
                    super::ime_support::set_ime_allowed_latched(
                        &window,
                        false,
                        &mut self.ime_allowed_latched,
                    );
                    self.ime_owner_window = None;
                }

                // Caret blink: wake ~once per phase (~520ms), not every frame.
                if has_input_caret {
                    let wake = crate::renderer::caret_blink_next_toggle_at();
                    self.caret_wake_at = Some(wake);
                    event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(wake));
                } else if !self
                    .secondaries
                    .borrow()
                    .values()
                    .any(|s| s.has_input_caret)
                {
                    self.caret_wake_at = None;
                }

                if self.force_redraw_count > 0 {
                    self.force_redraw_count -= 1;
                    window.request_redraw();
                }
            }
            Err(SurfaceAcquireError::Lost | SurfaceAcquireError::Outdated) => {
                let mut rb = self.state.borrow_mut();
                if let Some(state) = rb.as_mut() {
                    let inner = state.window.inner_size();
                    if inner.width > 0 && inner.height > 0 {
                        state.resize(inner);
                    }
                    state.recover_surface();
                    let _ = state.render(
                        [0.0, 0.0],
                        ScrollbarUpdateMode::Full,
                        None,
                        false,
                        false,
                        false,
                    );
                    state.window.request_redraw();
                    self.force_redraw_count = 3;
                }
            }
            Err(SurfaceAcquireError::Timeout | SurfaceAcquireError::Occluded) => {
                window.request_redraw();
            }
            Err(SurfaceAcquireError::Validation) => {
                log::error!("primary window: surface validation error");
            }
        }
    }

    pub(super) fn handle_mouse_button(
        &mut self,
        id: WindowId,
        element_state: ElementState,
        button: MouseButton,
        event_loop: Option<&ActiveEventLoop>,
    ) {
        self.update_mouse_buttons(button, element_state);
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
                self.handle_mouse_input(id, element_state, event_loop);
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
                    let (path, click_path, cb_to_run, label_for_focus, on_exclusive, on_trigger) = {
                        let state_borrow = self.state.borrow();
                        let state = match state_borrow.as_ref() {
                            Some(s) if s.window.id() == id => s,
                            _ => return,
                        };
                        let path = if let Some(ref cap_id) = capture {
                            if let Some(el) =
                                super::pointer_events::find_by_focus_id(&state.root_element, cap_id)
                            {
                                vec![super::pointer_events::HitPathEntry::from_element(
                                    el,
                                    el.layout_bounds,
                                )]
                            } else {
                                super::pointer_events::hit_path(&state.root_element, x, y)
                            }
                        } else {
                            super::pointer_events::hit_path(&state.root_element, x, y)
                        };
                        let click_path = match self.click_press_path.take() {
                            Some(down) => super::pointer_events::click_dispatch_path(&down, &path),
                            None => Vec::new(),
                        };
                        let cb_resolved = super::pointer_events::resolve_click_from_path(
                            &state.root_element,
                            &click_path,
                        );
                        let on_exclusive =
                            super::portal_overlay::hit_path_on_top_exclusive_surface(&path);
                        let on_trigger = crate::utils::popup::path_hits_floating_trigger(
                            &state.root_element,
                            &path,
                        );
                        (
                            path,
                            click_path,
                            cb_resolved.0,
                            cb_resolved.1,
                            on_exclusive,
                            on_trigger,
                        )
                    };
                    let detail = self.click_count.max(1);
                    let mut pev =
                        self.build_mouse_event(x, y, MouseButton::Left, detail, "mouseup", None);
                    super::pointer_events::dispatch_pointer(
                        &path,
                        &mut pev,
                        super::pointer_events::PointerKind::Up,
                    );
                    if detail >= 2 && !pev.is_consumed() && !click_path.is_empty() {
                        let mut pev2 = self.build_mouse_event(
                            x,
                            y,
                            MouseButton::Left,
                            detail,
                            "dblclick",
                            None,
                        );
                        super::pointer_events::dispatch_pointer(
                            &click_path,
                            &mut pev2,
                            super::pointer_events::PointerKind::DoubleClick,
                        );
                    }
                    // Legacy `on_click` / Label `for`: fire on pointer up (DOM click timing).
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
                            crate::app::reactive::drain_deferred_view_remounts();
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
                if element_state == ElementState::Pressed {
                    let mut down =
                        self.build_mouse_event(x, y, MouseButton::Right, 1, "mousedown", None);
                    super::pointer_events::dispatch_pointer(
                        &path,
                        &mut down,
                        super::pointer_events::PointerKind::Down,
                    );
                    if !down.is_consumed() {
                        let mut pev = self.build_mouse_event(
                            x,
                            y,
                            MouseButton::Right,
                            1,
                            "contextmenu",
                            None,
                        );
                        super::pointer_events::dispatch_pointer(
                            &path,
                            &mut pev,
                            super::pointer_events::PointerKind::ContextMenu,
                        );
                    }
                } else {
                    let mut up =
                        self.build_mouse_event(x, y, MouseButton::Right, 1, "mouseup", None);
                    super::pointer_events::dispatch_pointer(
                        &path,
                        &mut up,
                        super::pointer_events::PointerKind::Up,
                    );
                    // ContextMenu opens on right-press; arm dismiss so the next outside
                    // left-click can close (primary mouseup is the usual arm path).
                    crate::utils::popup::arm_underlay_close();
                }
                if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                    st.window.request_redraw();
                }
            }
            _ => {
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
                let (kind, ty) = if element_state == ElementState::Pressed {
                    (super::pointer_events::PointerKind::Down, "mousedown")
                } else {
                    (super::pointer_events::PointerKind::Up, "mouseup")
                };
                let mut ev = self.build_mouse_event(x, y, button, 1, ty, None);
                super::pointer_events::dispatch_pointer(&path, &mut ev, kind);
            }
        }
    }
}
