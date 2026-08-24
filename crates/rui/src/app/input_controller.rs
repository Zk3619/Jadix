use winit::event::{ElementState, Ime, KeyEvent};
use winit::keyboard::{Key, NamedKey};
use winit::window::WindowId;

use crate::components::input::{
    filter_insert_for_type, input_type_signal_key, strip_control_insert, InputType,
};
use crate::utils::clipboard::{
    edit_shortcut_letter, get_text as clipboard_get_text, sanitize_pasted_line,
    set_text as clipboard_set_text, shortcut_modifiers as clipboard_shortcut_modifiers,
};
use crate::utils::input_hit::collect_focusable_ids;
use crate::utils::keyboard::{
    caret_line_end, caret_line_home, caret_move_left, caret_move_right, caret_move_vertical,
    caret_move_vertical_visual_pref, caret_visual_line_end, caret_visual_line_home,
    delete_char_forward, extend_selection, normalized_selection, select_all_range,
};

use super::ime_support::{self, apply_ime_commit, prepare_compose_anchor, text_edit_rest};

fn input_type_for(rest: &str) -> InputType {
    let key = input_type_signal_key(rest);
    let raw =
        crate::core::reactivity::use_signal(&key, || InputType::Text.as_str().to_string()).peek();
    InputType::parse(&raw)
}

/// 按字段 `type` 过滤将要插入的文本；空串表示全部被拒。
fn filter_typed_insert(rest: &str, existing: &str, text: &str) -> String {
    filter_insert_for_type(input_type_for(rest), existing, text)
}

/// Windows 等平台给 Esc / F 键等 Named 键附带 `event.text`（Esc → U+001B）。
/// 只有 Space / Enter 的 Named 键才允许走字符插入（空格、Textarea 换行）。
pub(crate) fn named_key_blocks_text_insert(key: &Key) -> bool {
    match key {
        Key::Named(NamedKey::Space | NamedKey::Enter) => false,
        Key::Named(_) => true,
        _ => false,
    }
}

impl super::App {
    pub(super) fn handle_ime_event(&mut self, ime: Ime, window_id: WindowId) {
        let focused = crate::core::reactivity::use_signal("__rui_button_focus_id", String::new);
        let ime_active =
            crate::core::reactivity::use_signal(crate::components::IME_ACTIVE_SIGNAL, || false);
        let fid = focused.get();
        let password_focus = text_edit_rest(&fid)
            .map(|rest| input_type_for(rest).is_password())
            .unwrap_or(false);
        match ime {
            Ime::Enabled => {
                ime_support::clear_ime_end_drain();
                if password_focus {
                    // Password must stay ASCII-only; refuse OS IME association.
                    ime_active.set(false);
                    self.refuse_ime_on_window(window_id);
                    if ime_support::ime_diag_enabled() {
                        log::info!("RUI_IME_DIAG Enabled ignored (password) fid={}", fid);
                    }
                    return;
                }
                ime_active.set(true);
                if ime_support::ime_diag_enabled() {
                    log::info!("RUI_IME_DIAG Enabled fid={}", fid);
                }
            }
            Ime::Disabled => {
                ime_support::clear_ime_end_drain();
                ime_active.set(false);
                if let Some(rest) = text_edit_rest(&fid) {
                    Self::clear_ime_compose_state(rest);
                }
                if ime_support::ime_diag_enabled() {
                    log::info!("RUI_IME_DIAG Disabled fid={}", fid);
                }
            }
            Ime::Preedit(_, _) | Ime::Commit(_) if password_focus => {
                // Drop composition against password fields (IME should already be disallowed).
                if let Some(rest) = text_edit_rest(&fid) {
                    Self::clear_ime_compose_state(rest);
                }
                ime_active.set(false);
                if ime_support::ime_diag_enabled() {
                    log::info!("RUI_IME_DIAG compose dropped (password) fid={}", fid);
                }
            }
            Ime::Preedit(_, _) | Ime::Commit(_) if ime_support::ime_end_drain_active() => {
                // App already ended composition; ignore OS CPS_CANCEL / teardown echoes
                // until Disabled clears the drain.
                if ime_support::ime_diag_enabled() {
                    log::info!(
                        "RUI_IME_DIAG drop compose echo during end-drain fid={}",
                        fid
                    );
                }
            }
            Ime::Preedit(s, cursor) => {
                if let Some(rest) = text_edit_rest(&fid) {
                    let vk = crate::components::input::input_value_signal_key(rest);
                    let val_sig = crate::core::reactivity::use_signal(&vk, String::new);
                    let pk = crate::components::input::input_preedit_signal_key(rest);
                    let sk = crate::components::input::input_preedit_sel_signal_key(rest);
                    let tsk = crate::components::input::input_selection_signal_key(rest);
                    let ck = crate::components::input::input_caret_signal_key(rest);
                    let ak = crate::components::input::input_ime_anchor_signal_key(rest);
                    let pre = crate::core::reactivity::use_signal(&pk, String::new);
                    let sel =
                        crate::core::reactivity::use_signal(&sk, || Option::<(usize, usize)>::None);
                    let text_sel =
                        crate::core::reactivity::use_signal(&tsk, || None::<(usize, usize)>);
                    let caret_sig = crate::core::reactivity::use_signal(&ck, || None::<usize>);
                    let ime_anchor_sig = crate::core::reactivity::use_signal(&ak, || None::<usize>);
                    let old_pre = pre.get();
                    // 组字开始：删选区并锁定 anchor，避免 Commit 插错位 / 「吞字」。
                    if old_pre.is_empty() && !s.is_empty() {
                        let committed = val_sig.get();
                        let (new_val, anchor) =
                            prepare_compose_anchor(&committed, caret_sig.get(), text_sel.get());
                        if new_val != committed {
                            crate::utils::text_field::commit_value_sig(rest, val_sig, new_val);
                            text_sel.set(None);
                            caret_sig.set(Some(anchor));
                        }
                        ime_anchor_sig.set(Some(anchor));
                    }
                    // 空 Preedit（取消组字预览）只清显示；anchor 保留到 Commit / Disabled，
                    // 避免 Win 上 Preedit("") 紧接 Commit 时丢锚点。
                    pre.set(s.clone());
                    sel.set(cursor);
                    if !s.is_empty() {
                        text_sel.set(None);
                    }
                    if ime_support::ime_diag_enabled() {
                        log::debug!(
                            "RUI_IME_DIAG Preedit rest={} len={} cursor={:?}",
                            rest,
                            s.chars().count(),
                            cursor
                        );
                    }
                }
            }
            Ime::Commit(s) => {
                if s.is_empty() {
                    // CPS_CANCEL / teardown often echoes an empty Commit after in-app
                    // focus switch (drain already cleared so typing works).
                    if let Some(rest) = text_edit_rest(&fid) {
                        Self::clear_ime_compose_state(rest);
                    }
                } else if let Some(rest) = text_edit_rest(&fid) {
                    let vk = crate::components::input::input_value_signal_key(rest);
                    let val_sig = crate::core::reactivity::use_signal(&vk, String::new);
                    let v = val_sig.get();
                    let ak = crate::components::input::input_ime_anchor_signal_key(rest);
                    let ime_anchor_sig = crate::core::reactivity::use_signal(&ak, || None::<usize>);
                    let mut anchor = ime_anchor_sig.get().unwrap_or_else(|| {
                        let ck = crate::components::input::input_caret_signal_key(rest);
                        crate::core::reactivity::use_signal(&ck, || None::<usize>)
                            .get()
                            .unwrap_or(v.len())
                    });
                    anchor = anchor.min(v.len());
                    anchor = v.floor_char_boundary(anchor);
                    let filtered = filter_typed_insert(rest, &v[..anchor], &s);
                    let new_caret = if filtered.is_empty() && !s.is_empty() {
                        // 全部被 type 过滤：仍清 preedit，不改 value。
                        anchor
                    } else {
                        let (new, new_caret) = apply_ime_commit(&v, anchor, &filtered);
                        crate::utils::text_field::commit_value_sig(rest, val_sig, new);
                        new_caret
                    };
                    Self::clear_ime_compose_state(rest);
                    let ck = crate::components::input::input_caret_signal_key(rest);
                    let caret_sig = crate::core::reactivity::use_signal(&ck, || None::<usize>);
                    caret_sig.set(Some(new_caret));
                    ime_support::record_ime_commit(rest, &filtered);
                }
            }
        }
        self.request_ime_redraw(window_id);
    }

    fn refuse_ime_on_window(&mut self, window_id: WindowId) {
        if self.is_secondary_window(window_id) {
            if let Some(sec) = self.secondaries.borrow_mut().get_mut(&window_id) {
                ime_support::set_ime_allowed_latched(
                    &sec.window,
                    false,
                    &mut sec.ime_allowed_latched,
                );
            }
        } else if let Some(st) = (*self.state.borrow_mut()).as_mut() {
            ime_support::set_ime_allowed_latched(&st.window, false, &mut self.ime_allowed_latched);
        }
    }

    fn request_ime_redraw(&mut self, window_id: WindowId) {
        if self.is_secondary_window(window_id) {
            if let Some(sec) = self.secondaries.borrow().get(&window_id) {
                sec.window.request_redraw();
            }
        } else if let Some(st) = (*self.state.borrow_mut()).as_mut() {
            st.window.request_redraw();
        }
    }

    fn clear_ime_compose_state(rest: &str) {
        let pk = crate::components::input::input_preedit_signal_key(rest);
        let sk = crate::components::input::input_preedit_sel_signal_key(rest);
        let tsk = crate::components::input::input_selection_signal_key(rest);
        let ak = crate::components::input::input_ime_anchor_signal_key(rest);
        crate::core::reactivity::use_signal(&pk, String::new).set(String::new());
        crate::core::reactivity::use_signal(&sk, || Option::<(usize, usize)>::None).set(None);
        crate::core::reactivity::use_signal(&tsk, || None::<(usize, usize)>).set(None);
        crate::core::reactivity::use_signal(&ak, || None::<usize>).set(None);
        ime_support::on_ime_composition_ended();
    }

    pub(super) fn handle_keyboard_input(&mut self, event: KeyEvent) -> bool {
        self.handle_keyboard_input_ex(event, true)
    }

    /// Signal-scoped text-field keys only — no primary tree walk / primary redraw.
    pub(super) fn handle_keyboard_text_signals(&mut self, event: KeyEvent) -> bool {
        self.handle_keyboard_input_ex(event, false)
    }

    fn handle_keyboard_input_ex(&mut self, event: KeyEvent, allow_primary_tree: bool) -> bool {
        if event.state == ElementState::Pressed {
            #[cfg(feature = "devtools")]
            if crate::app::devtools::try_toggle_from_key(&event, self.modifiers) {
                crate::app::runtime::request_redraw();
                return true;
            }
        }
        if event.state == ElementState::Pressed {
            if matches!(event.logical_key, Key::Named(NamedKey::Tab)) && !event.repeat {
                if !allow_primary_tree {
                    return false;
                }
                let mut ids = crate::utils::focus_trap_ring_ids();
                // Explicit focus trap wins; else Tab prefers top stacked overlay panel.
                if ids.is_empty() {
                    if let Some(st) = (*self.state.borrow()).as_ref() {
                        if let Some(z) = crate::app::portal_overlay::top_stacked_panel_z() {
                            crate::utils::input_hit::collect_focusable_ids_in_portal_z(
                                &st.root_element,
                                z,
                                &mut ids,
                            );
                        }
                        if ids.is_empty() {
                            collect_focusable_ids(&st.root_element, &mut ids);
                        }
                    }
                }
                if !ids.is_empty() {
                    let focused =
                        crate::core::reactivity::use_signal("__rui_button_focus_id", String::new);
                    let focus_visible =
                        crate::core::reactivity::use_signal("__rui_focus_visible", || false);
                    // peek: event path must not subscribe focus into a reactive effect.
                    let current = focused.peek();
                    let shift = self.modifiers.shift_key();
                    let idx = ids.iter().position(|id| id == &current);
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
                    let next_id = ids[next_idx].clone();
                    focus_visible.set(true);
                    focused.set(next_id);
                    // Focus chrome is paint-synced (peek), not rebuild-driven. Without a redraw,
                    // snapped focus rings never repaint after Tab (springs used to keep frames alive).
                    if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                        st.window.request_redraw();
                    }
                    return true;
                }
            } else if !matches!(event.logical_key, Key::Named(NamedKey::Tab)) {
                let focused =
                    crate::core::reactivity::use_signal("__rui_button_focus_id", String::new);
                let ime_active = crate::core::reactivity::use_signal(
                    crate::components::IME_ACTIVE_SIGNAL,
                    || false,
                );
                let fid = focused.get();

                if allow_primary_tree
                    && event.state == ElementState::Pressed
                    && matches!(event.logical_key, Key::Named(NamedKey::Escape))
                {
                    let closed = crate::app::portal_overlay::try_close_top()
                        || crate::app::navigator::try_pop_active();
                    if closed {
                        if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                            st.window.request_redraw();
                        }
                        return true;
                    }
                }

                if allow_primary_tree && super::window_keys::try_dispatch_window_key(&event) {
                    if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                        st.window.request_redraw();
                    }
                    return true;
                }

                let editing = text_edit_rest(&fid).is_some();
                // Command / Combobox attach `on_key_down` on the focused Input. That handler
                // must run even while editing — otherwise ↑↓/Enter move the caret (or no-op)
                // and never reach the list. Do **not** fall through to Space/Enter→click
                // (that would swallow Space in the field).
                if allow_primary_tree {
                    let key_handlers = (*self.state.borrow()).as_ref().and_then(|st| {
                        super::tree_hit::focused_key_handlers(&st.root_element, &fid)
                    });
                    if let Some(handlers) = key_handlers {
                        let handled = if editing {
                            super::tree_hit::run_focused_keydown_only(&handlers, &event)
                        } else {
                            super::tree_hit::run_focused_key_handlers(&handlers, &event)
                        };
                        if handled {
                            if allow_primary_tree {
                                if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                                    st.window.request_redraw();
                                }
                            }
                            return true;
                        }
                    }

                    if !editing
                        && clipboard_shortcut_modifiers(&self.modifiers)
                        && edit_shortcut_letter(&event) == Some('c')
                    {
                        if let Some(st) = (*self.state.borrow()).as_ref() {
                            if crate::components::label::try_copy_label_selection(&st.root_element)
                            {
                                if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                                    st.window.request_redraw();
                                }
                                return true;
                            }
                        }
                    }
                } else if !editing {
                    return false;
                }

                let edit_target = fid
                    .strip_prefix(crate::components::INPUT_FOCUS_PREFIX)
                    .map(|r| (r, false))
                    .or_else(|| {
                        fid.strip_prefix(crate::components::TEXTAREA_FOCUS_PREFIX)
                            .map(|r| (r, true))
                    });
                if let Some((rest, multiline)) = edit_target {
                    let value_key = crate::components::input::input_value_signal_key(rest);
                    let val_sig = crate::core::reactivity::use_signal(&value_key, String::new);
                    let pk = crate::components::input::input_preedit_signal_key(rest);
                    let preedit_sig = crate::core::reactivity::use_signal(&pk, String::new);
                    let ck = crate::components::input::input_caret_signal_key(rest);
                    let caret_sig = crate::core::reactivity::use_signal(&ck, || None::<usize>);
                    let tsk = crate::components::input::input_selection_signal_key(rest);
                    let text_sel_sig =
                        crate::core::reactivity::use_signal(&tsk, || None::<(usize, usize)>);
                    let wrap_w = if multiline {
                        crate::core::reactivity::use_signal(
                            &crate::utils::text_field::textarea_wrap_width_signal_key(rest),
                            || 0.0_f32,
                        )
                        .peek()
                    } else {
                        0.0
                    };
                    // Live leaf metrics (avoid hardcoded 0.875×16).
                    let (nav_size, nav_weight, nav_lh, nav_wrap) = if allow_primary_tree {
                        (*self.state.borrow()).as_ref().map_or(
                            (14.0_f32, 400_u16, 21.0_f32, crate::text::WrapMode::Word),
                            |st| {
                                super::tree_hit::find_input_text_leaf_for_rest(
                                    &st.root_element,
                                    None,
                                    rest,
                                )
                                .map(|(_, _, style, _)| {
                                    let sz = style.size.max(1.0);
                                    let w = style.weight.max(100);
                                    let lh = style
                                        .line_height_px
                                        .unwrap_or_else(|| {
                                            crate::utils::text_field::text_edit_line_box_px(
                                                sz, None, None,
                                            )
                                        })
                                        .max(1.0);
                                    let wrap = if style.white_space
                                        == crate::style::WhiteSpace::Nowrap
                                    {
                                        crate::text::WrapMode::None
                                    } else {
                                        crate::core::reactivity::use_signal(
                                            &crate::utils::text_field::textarea_wrap_mode_signal_key(
                                                rest,
                                            ),
                                            || crate::text::WrapMode::Word,
                                        )
                                        .peek()
                                    };
                                    (sz, w, lh, wrap)
                                })
                                .unwrap_or((14.0, 400, 21.0, crate::text::WrapMode::Word))
                            },
                        )
                    } else {
                        (14.0, 400, 21.0, crate::text::WrapMode::Word)
                    };
                    let pref_x_key =
                        crate::utils::text_field::input_caret_preferred_x_signal_key(rest);
                    let pref_x_sig =
                        crate::core::reactivity::use_signal(&pref_x_key, || None::<f32>);
                    let pre = preedit_sig.get();
                    let composing = ime_active.get() && !pre.is_empty();
                    let password_field = input_type_for(rest).is_password();
                    if clipboard_shortcut_modifiers(&self.modifiers) {
                        if let Some(k) = edit_shortcut_letter(&event) {
                            let mut handled = false;
                            if !composing {
                                match k {
                                    'a' => {
                                        // Select All
                                        let s = val_sig.get();
                                        if let Some(range) = select_all_range(&s) {
                                            text_sel_sig.set(Some(range));
                                            caret_sig.set(Some(range.1));
                                        } else {
                                            text_sel_sig.set(None);
                                            caret_sig.set(Some(0));
                                        }
                                        handled = true;
                                    }
                                    'c' => {
                                        // Copy (blocked for password fields)
                                        if !password_field {
                                            let s = val_sig.get();
                                            if let Some((lo, hi)) =
                                                normalized_selection(&s, text_sel_sig.get())
                                            {
                                                clipboard_set_text(&s[lo..hi]);
                                            }
                                        }
                                        handled = true;
                                    }
                                    'v' => {
                                        // Paste
                                        if let Some(text) = clipboard_get_text() {
                                            let text = if multiline {
                                                text.replace('\r', "")
                                            } else {
                                                sanitize_pasted_line(&text)
                                            };
                                            if !text.is_empty() {
                                                let mut s = val_sig.get();
                                                let insert_at = if let Some((lo, hi)) =
                                                    normalized_selection(&s, text_sel_sig.get())
                                                {
                                                    s.replace_range(lo..hi, "");
                                                    lo
                                                } else {
                                                    let mut at = caret_sig.get().unwrap_or(s.len());
                                                    at = at.min(s.len());
                                                    s.floor_char_boundary(at)
                                                };
                                                let text = filter_typed_insert(rest, &s, &text);
                                                if !text.is_empty() {
                                                    s.insert_str(insert_at, &text);
                                                    crate::utils::text_field::commit_value_sig(
                                                        rest, val_sig, s,
                                                    );
                                                    text_sel_sig.set(None);
                                                    caret_sig.set(Some(insert_at + text.len()));
                                                }
                                            }
                                        }
                                        handled = true;
                                    }
                                    'x' => {
                                        // Cut (blocked for password fields)
                                        let mut s = val_sig.get();
                                        if let Some((lo, hi)) =
                                            normalized_selection(&s, text_sel_sig.get())
                                        {
                                            if !password_field {
                                                clipboard_set_text(&s[lo..hi]);
                                            }
                                            s.replace_range(lo..hi, "");
                                            crate::utils::text_field::commit_value_sig(
                                                rest, val_sig, s,
                                            );
                                            text_sel_sig.set(None);
                                            caret_sig.set(Some(lo));
                                        }
                                        handled = true;
                                    }
                                    _ => {}
                                }
                            }
                            if handled {
                                if allow_primary_tree {
                                    if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                                        st.window.request_redraw();
                                    }
                                }
                                return true;
                            }
                        }
                    }
                    if !composing {
                        let s = val_sig.get();
                        let caret = caret_sig.get().unwrap_or(s.len()).min(s.len());
                        let caret = s.floor_char_boundary(caret);
                        let shift = self.modifiers.shift_key();
                        let soft_visual_model = if multiline && wrap_w > 1.0 && nav_wrap.soft_wrap()
                        {
                            Some(crate::text::build_visual_lines(
                                &s,
                                nav_size,
                                nav_weight,
                                nav_lh,
                                wrap_w,
                                nav_wrap.to_line_break_source(),
                            ))
                        } else {
                            None
                        };
                        let mut nav_handled = false;
                        match &event.logical_key {
                            Key::Named(NamedKey::ArrowLeft) => {
                                let dest = if !shift {
                                    if let Some((a, b)) = text_sel_sig.get() {
                                        let lo = a.min(b);
                                        let hi = a.max(b);
                                        if lo < hi {
                                            lo
                                        } else {
                                            caret_move_left(&s, caret)
                                        }
                                    } else {
                                        caret_move_left(&s, caret)
                                    }
                                } else {
                                    caret_move_left(&s, caret)
                                };
                                if shift {
                                    text_sel_sig.set(Some(extend_selection(
                                        caret,
                                        text_sel_sig.get(),
                                        dest,
                                    )));
                                } else {
                                    text_sel_sig.set(None);
                                }
                                caret_sig.set(Some(dest));
                                if let Some(ref model) = soft_visual_model {
                                    pref_x_sig.set_silent(Some(model.caret_x_on_line(dest)));
                                }
                                nav_handled = true;
                            }
                            Key::Named(NamedKey::ArrowRight) => {
                                let dest = if !shift {
                                    if let Some((a, b)) = text_sel_sig.get() {
                                        let lo = a.min(b);
                                        let hi = a.max(b);
                                        if lo < hi {
                                            hi
                                        } else {
                                            caret_move_right(&s, caret)
                                        }
                                    } else {
                                        caret_move_right(&s, caret)
                                    }
                                } else {
                                    caret_move_right(&s, caret)
                                };
                                if shift {
                                    text_sel_sig.set(Some(extend_selection(
                                        caret,
                                        text_sel_sig.get(),
                                        dest,
                                    )));
                                } else {
                                    text_sel_sig.set(None);
                                }
                                caret_sig.set(Some(dest));
                                if let Some(ref model) = soft_visual_model {
                                    pref_x_sig.set_silent(Some(model.caret_x_on_line(dest)));
                                }
                                nav_handled = true;
                            }
                            Key::Named(NamedKey::Home) => {
                                let dest = if self.modifiers.control_key() || !multiline {
                                    0
                                } else if let Some(ref model) = soft_visual_model {
                                    caret_visual_line_home(&model.ranges, caret)
                                } else {
                                    caret_line_home(&s, caret)
                                };
                                if shift {
                                    text_sel_sig.set(Some(extend_selection(
                                        caret,
                                        text_sel_sig.get(),
                                        dest,
                                    )));
                                } else {
                                    text_sel_sig.set(None);
                                }
                                caret_sig.set(Some(dest));
                                if let Some(ref model) = soft_visual_model {
                                    pref_x_sig.set_silent(Some(model.caret_x_on_line(dest)));
                                }
                                nav_handled = true;
                            }
                            Key::Named(NamedKey::End) => {
                                let dest = if self.modifiers.control_key() || !multiline {
                                    s.len()
                                } else if let Some(ref model) = soft_visual_model {
                                    caret_visual_line_end(&model.ranges, caret)
                                } else {
                                    caret_line_end(&s, caret)
                                };
                                if shift {
                                    text_sel_sig.set(Some(extend_selection(
                                        caret,
                                        text_sel_sig.get(),
                                        dest,
                                    )));
                                } else {
                                    text_sel_sig.set(None);
                                }
                                caret_sig.set(Some(dest));
                                if let Some(ref model) = soft_visual_model {
                                    pref_x_sig.set_silent(Some(model.caret_x_on_line(dest)));
                                }
                                nav_handled = true;
                            }
                            Key::Named(NamedKey::ArrowUp) => {
                                let dest = if multiline {
                                    if let Some(ref model) = soft_visual_model {
                                        let px = pref_x_sig
                                            .peek()
                                            .unwrap_or_else(|| model.caret_x_on_line(caret));
                                        if pref_x_sig.peek().is_none() {
                                            pref_x_sig.set_silent(Some(px));
                                        }
                                        caret_move_vertical_visual_pref(
                                            &s,
                                            &model.ranges,
                                            Some(&model.caret_xs),
                                            caret,
                                            -1,
                                            Some(px),
                                        )
                                    } else {
                                        caret_move_vertical(&s, caret, -1)
                                    }
                                } else {
                                    // Single-line: Up jumps to start (common field behavior).
                                    0
                                };
                                if shift {
                                    text_sel_sig.set(Some(extend_selection(
                                        caret,
                                        text_sel_sig.get(),
                                        dest,
                                    )));
                                } else {
                                    text_sel_sig.set(None);
                                }
                                caret_sig.set(Some(dest));
                                nav_handled = true;
                            }
                            Key::Named(NamedKey::ArrowDown) => {
                                let dest = if multiline {
                                    if let Some(ref model) = soft_visual_model {
                                        let px = pref_x_sig
                                            .peek()
                                            .unwrap_or_else(|| model.caret_x_on_line(caret));
                                        if pref_x_sig.peek().is_none() {
                                            pref_x_sig.set_silent(Some(px));
                                        }
                                        caret_move_vertical_visual_pref(
                                            &s,
                                            &model.ranges,
                                            Some(&model.caret_xs),
                                            caret,
                                            1,
                                            Some(px),
                                        )
                                    } else {
                                        caret_move_vertical(&s, caret, 1)
                                    }
                                } else {
                                    s.len()
                                };
                                if shift {
                                    text_sel_sig.set(Some(extend_selection(
                                        caret,
                                        text_sel_sig.get(),
                                        dest,
                                    )));
                                } else {
                                    text_sel_sig.set(None);
                                }
                                caret_sig.set(Some(dest));
                                nav_handled = true;
                            }
                            Key::Named(NamedKey::Delete) => {
                                let mut s = val_sig.get();
                                if let Some((a, b)) = text_sel_sig.get() {
                                    let lo = a.min(b);
                                    let hi = a.max(b);
                                    if lo < hi && hi <= s.len() {
                                        s.replace_range(lo..hi, "");
                                        crate::utils::text_field::commit_value_sig(
                                            rest, val_sig, s,
                                        );
                                        text_sel_sig.set(None);
                                        caret_sig.set(Some(lo));
                                    }
                                } else {
                                    let mut c = caret_sig.get().unwrap_or(s.len()).min(s.len());
                                    c = s.floor_char_boundary(c);
                                    if delete_char_forward(&mut s, c) {
                                        crate::utils::text_field::commit_value_sig(
                                            rest, val_sig, s,
                                        );
                                        caret_sig.set(Some(c));
                                    }
                                }
                                nav_handled = true;
                            }
                            Key::Named(NamedKey::Insert) => {
                                let ow_key =
                                    crate::components::input::input_overwrite_signal_key(rest);
                                let ow = crate::core::reactivity::use_signal(&ow_key, || false);
                                ow.set(!ow.get());
                                nav_handled = true;
                            }
                            _ => {}
                        }
                        if nav_handled {
                            if allow_primary_tree {
                                if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                                    st.window.request_redraw();
                                }
                            }
                            return true;
                        }
                    }
                    if matches!(event.logical_key, Key::Named(NamedKey::Backspace)) {
                        if ime_support::ime_end_drain_active() {
                            if ime_support::ime_diag_enabled() {
                                log::info!(
                                    "RUI_IME_DIAG drop Backspace during end-drain rest={}",
                                    rest
                                );
                            }
                            if allow_primary_tree {
                                if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                                    st.window.request_redraw();
                                }
                            }
                            return true;
                        }
                        let pre = preedit_sig.get();
                        if !ime_active.get() || pre.is_empty() {
                            let mut s = val_sig.get();
                            if let Some((a, b)) = text_sel_sig.get() {
                                let lo = a.min(b);
                                let hi = a.max(b);
                                if lo < hi && hi <= s.len() {
                                    s.replace_range(lo..hi, "");
                                    crate::utils::text_field::commit_value_sig(
                                        rest,
                                        val_sig,
                                        s.clone(),
                                    );
                                    text_sel_sig.set(None);
                                    caret_sig.set(Some(lo));
                                } else if lo == hi && lo <= s.len() && lo > 0 {
                                    let prev = s.floor_char_boundary(lo - 1);
                                    s.replace_range(prev..lo, "");
                                    crate::utils::text_field::commit_value_sig(rest, val_sig, s);
                                    text_sel_sig.set(None);
                                    caret_sig.set(Some(prev));
                                }
                            } else {
                                let caret = caret_sig.get().unwrap_or(s.len());
                                let caret = caret.min(s.len());
                                if caret > 0 {
                                    let prev = s.floor_char_boundary(caret - 1);
                                    s.replace_range(prev..caret, "");
                                    crate::utils::text_field::commit_value_sig(rest, val_sig, s);
                                    caret_sig.set(Some(prev));
                                }
                            }
                        }
                    } else if named_key_blocks_text_insert(&event.logical_key) {
                        // Esc 等 Named 键：不要把 `event.text`（U+001B）写进字段。
                    } else if let Some(t) = event.text.as_ref() {
                        if ime_support::ime_end_drain_active() {
                            // Field→field switch keeps IME allowed, so `Disabled` may never
                            // clear the drain. Still drop control/teardown echoes; allow
                            // printable typing on the newly focused field.
                            let printable = t.chars().any(|c| !c.is_control());
                            if !printable {
                                if ime_support::ime_diag_enabled() {
                                    log::info!(
                                        "RUI_IME_DIAG drop control text during end-drain rest={}",
                                        rest
                                    );
                                }
                                if allow_primary_tree {
                                    if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                                        st.window.request_redraw();
                                    }
                                }
                                return true;
                            }
                            ime_support::clear_ime_end_drain();
                        }
                        let is_nl = t.as_str() == "\r" || t.as_str() == "\n";
                        let allow = !ime_active.get()
                            && !t.is_empty()
                            && !self.modifiers.control_key()
                            && (multiline || !is_nl);
                        if allow {
                            let insert = if is_nl { "\n" } else { t.as_str() };
                            let mut s = val_sig.get();
                            if let Some((a, b)) = text_sel_sig.get() {
                                let lo = a.min(b);
                                let hi = a.max(b);
                                if lo < hi && hi <= s.len() {
                                    s.replace_range(lo..hi, "");
                                }
                                let mut insert_at = lo.min(s.len());
                                insert_at = s.floor_char_boundary(insert_at);
                                let text = if multiline {
                                    strip_control_insert(insert)
                                } else {
                                    filter_typed_insert(rest, &s, insert)
                                };
                                if !text.is_empty() {
                                    s.insert_str(insert_at, &text);
                                    crate::utils::text_field::commit_value_sig(rest, val_sig, s);
                                    text_sel_sig.set(None);
                                    caret_sig.set(Some(insert_at + text.len()));
                                }
                            } else {
                                let mut insert_at = caret_sig.get().unwrap_or(s.len());
                                insert_at = insert_at.min(s.len());
                                insert_at = s.floor_char_boundary(insert_at);
                                let text = if multiline {
                                    strip_control_insert(insert)
                                } else {
                                    filter_typed_insert(rest, &s, insert)
                                };
                                if text.is_empty() {
                                    // rejected by type filter
                                } else {
                                    let ow_key =
                                        crate::components::input::input_overwrite_signal_key(rest);
                                    let overwrite_sig =
                                        crate::core::reactivity::use_signal(&ow_key, || false);
                                    let overwrite = overwrite_sig.get();
                                    if overwrite
                                        && text_sel_sig.get().is_none()
                                        && insert_at < s.len()
                                        && text.chars().count() == 1
                                    {
                                        let first = s[insert_at..].chars().next().unwrap();
                                        let olen = first.len_utf8();
                                        s.replace_range(insert_at..insert_at + olen, &text);
                                        crate::utils::text_field::commit_value_sig(
                                            rest, val_sig, s,
                                        );
                                        caret_sig.set(Some(insert_at + text.len()));
                                    } else {
                                        s.insert_str(insert_at, &text);
                                        crate::utils::text_field::commit_value_sig(
                                            rest, val_sig, s,
                                        );
                                        caret_sig.set(Some(insert_at + text.len()));
                                    }
                                }
                            }
                        }
                    }
                    if allow_primary_tree {
                        if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                            st.window.request_redraw();
                        }
                    }
                    return true;
                }
                return allow_primary_tree;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_and_function_keys_do_not_insert_text() {
        assert!(named_key_blocks_text_insert(&Key::Named(NamedKey::Escape)));
        assert!(named_key_blocks_text_insert(&Key::Named(NamedKey::F1)));
        assert!(named_key_blocks_text_insert(&Key::Named(NamedKey::Tab)));
        assert!(!named_key_blocks_text_insert(&Key::Named(NamedKey::Space)));
        assert!(!named_key_blocks_text_insert(&Key::Named(NamedKey::Enter)));
        assert!(!named_key_blocks_text_insert(&Key::Character("a".into())));
        // Character(U+001B) 仍可能出现；由 strip_control_insert 丢掉，不在 Named 守卫里。
        assert!(!named_key_blocks_text_insert(&Key::Character(
            "\u{1b}".into()
        )));
    }
}
