use winit::event::{ElementState, Ime, KeyEvent};
use winit::keyboard::{Key, NamedKey};

use crate::components::input::{filter_insert_for_type, input_type_signal_key, InputType};
use crate::utils::clipboard::{
    get_text as clipboard_get_text, sanitize_pasted_line, set_text as clipboard_set_text,
    shortcut_modifiers as clipboard_shortcut_modifiers,
};
use crate::utils::input_hit::collect_focusable_ids;
use crate::utils::keyboard::{
    caret_line_end, caret_line_home, caret_move_left, caret_move_right, caret_move_vertical,
    delete_char_forward,
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

impl super::App {
    pub(super) fn handle_ime_event(&mut self, ime: Ime) {
        let focused = crate::core::reactivity::use_signal("__rui_button_focus_id", String::new);
        let ime_active =
            crate::core::reactivity::use_signal(crate::components::IME_ACTIVE_SIGNAL, || false);
        let fid = focused.get();
        match ime {
            Ime::Enabled => {
                ime_active.set(true);
                if ime_support::ime_diag_enabled() {
                    log::info!("RUI_IME_DIAG Enabled fid={}", fid);
                }
            }
            Ime::Disabled => {
                ime_active.set(false);
                if let Some(rest) = text_edit_rest(&fid) {
                    Self::clear_ime_compose_state(rest);
                }
                if ime_support::ime_diag_enabled() {
                    log::info!("RUI_IME_DIAG Disabled fid={}", fid);
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
                if let Some(rest) = text_edit_rest(&fid) {
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
        if let Some(st) = (*self.state.borrow_mut()).as_mut() {
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
    }

    pub(super) fn handle_keyboard_input(&mut self, event: KeyEvent) -> bool {
        if event.state == ElementState::Pressed {
            if matches!(event.logical_key, Key::Named(NamedKey::Tab)) && !event.repeat {
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

                if event.state == ElementState::Pressed
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

                if super::window_keys::try_dispatch_window_key(&event) {
                    if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                        st.window.request_redraw();
                    }
                    return true;
                }

                if text_edit_rest(&fid).is_none() {
                    if clipboard_shortcut_modifiers(&self.modifiers) {
                        if let Key::Character(ch) = &event.logical_key {
                            if ch.as_str().eq_ignore_ascii_case("c") {
                                if let Some(st) = (*self.state.borrow()).as_ref() {
                                    if crate::components::label::try_copy_label_selection(
                                        &st.root_element,
                                    ) {
                                        if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                                            st.window.request_redraw();
                                        }
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                    // Clone handlers under borrow, then drop before running them. Handlers may
                    // `signal.set` → reactive reconcile → `AppState::borrow_mut`.
                    let key_handlers = (*self.state.borrow()).as_ref().and_then(|st| {
                        super::tree_hit::focused_key_handlers(&st.root_element, &fid)
                    });
                    if let Some(handlers) = key_handlers {
                        if super::tree_hit::run_focused_key_handlers(&handlers, &event) {
                            if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                                st.window.request_redraw();
                            }
                            return true;
                        }
                    }
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
                    let pre = preedit_sig.get();
                    let composing = ime_active.get() && !pre.is_empty();
                    if clipboard_shortcut_modifiers(&self.modifiers) {
                        if let Key::Character(ch) = &event.logical_key {
                            let k = ch.as_str();
                            let mut handled = false;
                            if !composing {
                                if k.eq_ignore_ascii_case("c") {
                                    let s = val_sig.get();
                                    if let Some((a, b)) = text_sel_sig.get() {
                                        let lo = a.min(b);
                                        let hi = a.max(b);
                                        if lo < hi && hi <= s.len() {
                                            clipboard_set_text(&s[lo..hi]);
                                        }
                                    }
                                    handled = true;
                                } else if k.eq_ignore_ascii_case("v") {
                                    if let Some(text) = clipboard_get_text() {
                                        let text = if multiline {
                                            text.replace('\r', "")
                                        } else {
                                            sanitize_pasted_line(&text)
                                        };
                                        if !text.is_empty() {
                                            let mut s = val_sig.get();
                                            if let Some((a, b)) = text_sel_sig.get() {
                                                let lo = a.min(b);
                                                let hi = a.max(b);
                                                if lo < hi && hi <= s.len() {
                                                    s.replace_range(lo..hi, "");
                                                }
                                                let mut insert_at = lo.min(s.len());
                                                insert_at = s.floor_char_boundary(insert_at);
                                                let text = filter_typed_insert(rest, &s, &text);
                                                if !text.is_empty() {
                                                    s.insert_str(insert_at, &text);
                                                    crate::utils::text_field::commit_value_sig(rest, val_sig, s);
                                                    text_sel_sig.set(None);
                                                    caret_sig.set(Some(insert_at + text.len()));
                                                }
                                            } else {
                                                let mut insert_at =
                                                    caret_sig.get().unwrap_or(s.len());
                                                insert_at = insert_at.min(s.len());
                                                insert_at = s.floor_char_boundary(insert_at);
                                                let text = filter_typed_insert(rest, &s, &text);
                                                if !text.is_empty() {
                                                    s.insert_str(insert_at, &text);
                                                    crate::utils::text_field::commit_value_sig(rest, val_sig, s);
                                                    caret_sig.set(Some(insert_at + text.len()));
                                                }
                                            }
                                        }
                                    }
                                    handled = true;
                                } else if k.eq_ignore_ascii_case("x") {
                                    let mut s = val_sig.get();
                                    if let Some((a, b)) = text_sel_sig.get() {
                                        let lo = a.min(b);
                                        let hi = a.max(b);
                                        if lo < hi && hi <= s.len() {
                                            clipboard_set_text(&s[lo..hi]);
                                            s.replace_range(lo..hi, "");
                                            crate::utils::text_field::commit_value_sig(rest, val_sig, s);
                                            text_sel_sig.set(None);
                                            caret_sig.set(Some(lo));
                                        }
                                    }
                                    handled = true;
                                }
                            }
                            if handled {
                                if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                                    st.window.request_redraw();
                                }
                                return true;
                            }
                        }
                    }
                    if !composing {
                        let s = val_sig.get();
                        let caret = caret_sig.get().unwrap_or(s.len()).min(s.len());
                        let caret = s.floor_char_boundary(caret);
                        let mut nav_handled = false;
                        match &event.logical_key {
                            Key::Named(NamedKey::ArrowLeft) => {
                                if let Some((a, b)) = text_sel_sig.get() {
                                    let lo = a.min(b);
                                    let hi = a.max(b);
                                    if lo < hi {
                                        text_sel_sig.set(None);
                                        caret_sig.set(Some(lo));
                                    } else {
                                        text_sel_sig.set(None);
                                        caret_sig.set(Some(caret_move_left(&s, caret)));
                                    }
                                } else {
                                    text_sel_sig.set(None);
                                    caret_sig.set(Some(caret_move_left(&s, caret)));
                                }
                                nav_handled = true;
                            }
                            Key::Named(NamedKey::ArrowRight) => {
                                if let Some((a, b)) = text_sel_sig.get() {
                                    let lo = a.min(b);
                                    let hi = a.max(b);
                                    if lo < hi {
                                        text_sel_sig.set(None);
                                        caret_sig.set(Some(hi));
                                    } else {
                                        text_sel_sig.set(None);
                                        caret_sig.set(Some(caret_move_right(&s, caret)));
                                    }
                                } else {
                                    text_sel_sig.set(None);
                                    caret_sig.set(Some(caret_move_right(&s, caret)));
                                }
                                nav_handled = true;
                            }
                            Key::Named(NamedKey::Home) => {
                                text_sel_sig.set(None);
                                let dest = if self.modifiers.control_key() || !multiline {
                                    0
                                } else {
                                    caret_line_home(&s, caret)
                                };
                                caret_sig.set(Some(dest));
                                nav_handled = true;
                            }
                            Key::Named(NamedKey::End) => {
                                text_sel_sig.set(None);
                                let dest = if self.modifiers.control_key() || !multiline {
                                    s.len()
                                } else {
                                    caret_line_end(&s, caret)
                                };
                                caret_sig.set(Some(dest));
                                nav_handled = true;
                            }
                            Key::Named(NamedKey::ArrowUp) => {
                                text_sel_sig.set(None);
                                let dest = if multiline {
                                    caret_move_vertical(&s, caret, -1)
                                } else {
                                    // Single-line: Up jumps to start (common field behavior).
                                    0
                                };
                                caret_sig.set(Some(dest));
                                nav_handled = true;
                            }
                            Key::Named(NamedKey::ArrowDown) => {
                                text_sel_sig.set(None);
                                let dest = if multiline {
                                    caret_move_vertical(&s, caret, 1)
                                } else {
                                    s.len()
                                };
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
                                        crate::utils::text_field::commit_value_sig(rest, val_sig, s);
                                        text_sel_sig.set(None);
                                        caret_sig.set(Some(lo));
                                    }
                                } else {
                                    let mut c = caret_sig.get().unwrap_or(s.len()).min(s.len());
                                    c = s.floor_char_boundary(c);
                                    if delete_char_forward(&mut s, c) {
                                        crate::utils::text_field::commit_value_sig(rest, val_sig, s);
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
                            if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                                st.window.request_redraw();
                            }
                            return true;
                        }
                    }
                    if matches!(event.logical_key, Key::Named(NamedKey::Backspace)) {
                        let pre = preedit_sig.get();
                        if !ime_active.get() || pre.is_empty() {
                            let mut s = val_sig.get();
                            if let Some((a, b)) = text_sel_sig.get() {
                                let lo = a.min(b);
                                let hi = a.max(b);
                                if lo < hi && hi <= s.len() {
                                    s.replace_range(lo..hi, "");
                                    crate::utils::text_field::commit_value_sig(rest, val_sig, s.clone());
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
                    } else if let Some(t) = event.text.as_ref() {
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
                                    insert.to_string()
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
                                    insert.to_string()
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
                                        crate::utils::text_field::commit_value_sig(rest, val_sig, s);
                                        caret_sig.set(Some(insert_at + text.len()));
                                    } else {
                                        s.insert_str(insert_at, &text);
                                        crate::utils::text_field::commit_value_sig(rest, val_sig, s);
                                        caret_sig.set(Some(insert_at + text.len()));
                                    }
                                }
                            }
                        }
                    }
                    if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                        st.window.request_redraw();
                    }
                }
                return true;
            }
        }
        false
    }
}
