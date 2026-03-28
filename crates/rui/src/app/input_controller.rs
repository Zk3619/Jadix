use winit::event::{ElementState, Ime, KeyEvent};
use winit::keyboard::{Key, NamedKey};

use crate::utils::clipboard::{
    get_text as clipboard_get_text, sanitize_pasted_line, set_text as clipboard_set_text,
    shortcut_modifiers as clipboard_shortcut_modifiers,
};
use crate::utils::input_hit::collect_focusable_ids;
use crate::utils::keyboard::{caret_move_left, caret_move_right, delete_char_forward};

impl super::App {
    pub(super) fn handle_ime_event(&mut self, ime: Ime) {
        let focused = crate::core::reactivity::use_signal("__rui_button_focus_id", String::new);
        let ime_active = crate::core::reactivity::use_signal(crate::components::IME_ACTIVE_SIGNAL, || false);
        let fid = focused.get();
        match ime {
            Ime::Enabled => {
                ime_active.set(true);
            }
            Ime::Disabled => {
                ime_active.set(false);
                if let Some(rest) = fid.strip_prefix(crate::components::INPUT_FOCUS_PREFIX) {
                    let pk = crate::components::input::input_preedit_signal_key(rest);
                    let sk = crate::components::input::input_preedit_sel_signal_key(rest);
                    let tsk = crate::components::input::input_selection_signal_key(rest);
                    let ak = crate::components::input::input_ime_anchor_signal_key(rest);
                    let pre = crate::core::reactivity::use_signal(&pk, String::new);
                    let sel = crate::core::reactivity::use_signal(&sk, || Option::<(usize, usize)>::None);
                    let text_sel = crate::core::reactivity::use_signal(&tsk, || None::<(usize, usize)>);
                    let ime_anchor_sig = crate::core::reactivity::use_signal(&ak, || None::<usize>);
                    pre.set(String::new());
                    sel.set(None);
                    text_sel.set(None);
                    ime_anchor_sig.set(None);
                }
            }
            Ime::Preedit(s, cursor) => {
                if let Some(rest) = fid.strip_prefix(crate::components::INPUT_FOCUS_PREFIX) {
                    let vk = crate::components::input::input_value_signal_key(rest);
                    let val_sig = crate::core::reactivity::use_signal(&vk, String::new);
                    let pk = crate::components::input::input_preedit_signal_key(rest);
                    let sk = crate::components::input::input_preedit_sel_signal_key(rest);
                    let tsk = crate::components::input::input_selection_signal_key(rest);
                    let ck = crate::components::input::input_caret_signal_key(rest);
                    let ak = crate::components::input::input_ime_anchor_signal_key(rest);
                    let pre = crate::core::reactivity::use_signal(&pk, String::new);
                    let sel = crate::core::reactivity::use_signal(&sk, || Option::<(usize, usize)>::None);
                    let text_sel = crate::core::reactivity::use_signal(&tsk, || None::<(usize, usize)>);
                    let caret_sig = crate::core::reactivity::use_signal(&ck, || None::<usize>);
                    let ime_anchor_sig = crate::core::reactivity::use_signal(&ak, || None::<usize>);
                    let old_pre = pre.get();
                    if old_pre.is_empty() && !s.is_empty() {
                        let committed = val_sig.get();
                        let mut a = caret_sig.get().unwrap_or(committed.len());
                        a = a.min(committed.len());
                        a = committed.floor_char_boundary(a);
                        ime_anchor_sig.set(Some(a));
                    }
                    pre.set(s);
                    sel.set(cursor);
                    text_sel.set(None);
                }
            }
            Ime::Commit(s) => {
                if let Some(rest) = fid.strip_prefix(crate::components::INPUT_FOCUS_PREFIX) {
                    let vk = crate::components::input::input_value_signal_key(rest);
                    let val_sig = crate::core::reactivity::use_signal(&vk, String::new);
                    let v = val_sig.get();
                    let ak = crate::components::input::input_ime_anchor_signal_key(rest);
                    let ime_anchor_sig = crate::core::reactivity::use_signal(&ak, || None::<usize>);
                    let mut anchor = ime_anchor_sig.get().unwrap_or(v.len()).min(v.len());
                    anchor = v.floor_char_boundary(anchor);
                    let mut new = String::new();
                    new.push_str(&v[..anchor]);
                    new.push_str(&s);
                    new.push_str(&v[anchor..]);
                    let new_caret = anchor + s.len();
                    val_sig.set(new);
                    let pk = crate::components::input::input_preedit_signal_key(rest);
                    let sk = crate::components::input::input_preedit_sel_signal_key(rest);
                    let tsk = crate::components::input::input_selection_signal_key(rest);
                    let ck = crate::components::input::input_caret_signal_key(rest);
                    let pre = crate::core::reactivity::use_signal(&pk, String::new);
                    let sel = crate::core::reactivity::use_signal(&sk, || Option::<(usize, usize)>::None);
                    let text_sel = crate::core::reactivity::use_signal(&tsk, || None::<(usize, usize)>);
                    let caret_sig = crate::core::reactivity::use_signal(&ck, || None::<usize>);
                    pre.set(String::new());
                    sel.set(None);
                    text_sel.set(None);
                    ime_anchor_sig.set(None);
                    caret_sig.set(Some(new_caret));
                }
            }
        }
        if let Some(st) = (&mut *self.state.borrow_mut()).as_mut() {
            st.window.request_redraw();
        }
    }

    pub(super) fn handle_keyboard_input(&mut self, event: KeyEvent) -> bool {
        if event.state == ElementState::Pressed {
            if matches!(event.logical_key, Key::Named(NamedKey::Tab)) && !event.repeat {
                let trap_ring =
                    crate::core::reactivity::use_signal(crate::components::FOCUS_TRAP_RING_KEY, String::new);
                let ring = trap_ring.get();
                let mut ids: Vec<String> = if !ring.is_empty() {
                    ring.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                } else {
                    Vec::new()
                };
                if ids.is_empty() {
                    if let Some(st) = (&*self.state.borrow()).as_ref() {
                        collect_focusable_ids(&st.root_element, &mut ids);
                    }
                }
                if !ids.is_empty() {
                    let focused = crate::core::reactivity::use_signal("__rui_button_focus_id", String::new);
                    let focus_visible = crate::core::reactivity::use_signal("__rui_focus_visible", || false);
                    let current = focused.get();
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
                }
            } else if !matches!(event.logical_key, Key::Named(NamedKey::Tab)) {
                let focused = crate::core::reactivity::use_signal("__rui_button_focus_id", String::new);
                let ime_active = crate::core::reactivity::use_signal(crate::components::IME_ACTIVE_SIGNAL, || false);
                let fid = focused.get();

                if event.state == ElementState::Pressed
                    && matches!(event.logical_key, Key::Named(NamedKey::Escape))
                    && crate::primitives::popup::try_close_top_popup()
                {
                    if let Some(st) = (&mut *self.state.borrow_mut()).as_mut() {
                        st.window.request_redraw();
                    }
                    return true;
                }

                if !fid.starts_with(crate::components::INPUT_FOCUS_PREFIX) {
                    if let Some(st) = (&*self.state.borrow()).as_ref() {
                        if super::tree_hit::dispatch_key_for_focused(&st.root_element, &fid, &event) {
                            if let Some(st) = (&mut *self.state.borrow_mut()).as_mut() {
                                st.window.request_redraw();
                            }
                            return true;
                        }
                    }
                }

                if let Some(rest) = fid.strip_prefix(crate::components::INPUT_FOCUS_PREFIX) {
                    let value_key = crate::components::input::input_value_signal_key(rest);
                    let val_sig = crate::core::reactivity::use_signal(&value_key, String::new);
                    let pk = crate::components::input::input_preedit_signal_key(rest);
                    let preedit_sig = crate::core::reactivity::use_signal(&pk, String::new);
                    let ck = crate::components::input::input_caret_signal_key(rest);
                    let caret_sig = crate::core::reactivity::use_signal(&ck, || None::<usize>);
                    let tsk = crate::components::input::input_selection_signal_key(rest);
                    let text_sel_sig = crate::core::reactivity::use_signal(&tsk, || None::<(usize, usize)>);
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
                                        let text = sanitize_pasted_line(&text);
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
                                                s.insert_str(insert_at, &text);
                                                val_sig.set(s);
                                                text_sel_sig.set(None);
                                                caret_sig.set(Some(insert_at + text.len()));
                                            } else {
                                                let mut insert_at = caret_sig.get().unwrap_or(s.len());
                                                insert_at = insert_at.min(s.len());
                                                insert_at = s.floor_char_boundary(insert_at);
                                                s.insert_str(insert_at, &text);
                                                val_sig.set(s);
                                                caret_sig.set(Some(insert_at + text.len()));
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
                                            val_sig.set(s);
                                            text_sel_sig.set(None);
                                            caret_sig.set(Some(lo));
                                        }
                                    }
                                    handled = true;
                                }
                            }
                            if handled {
                                if let Some(st) = (&mut *self.state.borrow_mut()).as_mut() {
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
                            Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::Home) => {
                                text_sel_sig.set(None);
                                caret_sig.set(Some(0));
                                nav_handled = true;
                            }
                            Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::End) => {
                                text_sel_sig.set(None);
                                caret_sig.set(Some(s.len()));
                                nav_handled = true;
                            }
                            Key::Named(NamedKey::Delete) => {
                                let mut s = val_sig.get();
                                if let Some((a, b)) = text_sel_sig.get() {
                                    let lo = a.min(b);
                                    let hi = a.max(b);
                                    if lo < hi && hi <= s.len() {
                                        s.replace_range(lo..hi, "");
                                        val_sig.set(s);
                                        text_sel_sig.set(None);
                                        caret_sig.set(Some(lo));
                                    }
                                } else {
                                    let mut c = caret_sig.get().unwrap_or(s.len()).min(s.len());
                                    c = s.floor_char_boundary(c);
                                    if delete_char_forward(&mut s, c) {
                                        val_sig.set(s);
                                        caret_sig.set(Some(c));
                                    }
                                }
                                nav_handled = true;
                            }
                            Key::Named(NamedKey::Insert) => {
                                let ow_key = crate::components::input::input_overwrite_signal_key(rest);
                                let ow = crate::core::reactivity::use_signal(&ow_key, || false);
                                ow.set(!ow.get());
                                nav_handled = true;
                            }
                            _ => {}
                        }
                        if nav_handled {
                            if let Some(st) = (&mut *self.state.borrow_mut()).as_mut() {
                                st.window.request_redraw();
                            }
                            return true;
                        }
                    }
                    if matches!(event.logical_key, Key::Named(NamedKey::Backspace)) {
                        let pre = preedit_sig.get();
                        if !(ime_active.get() && !pre.is_empty()) {
                            let mut s = val_sig.get();
                            if let Some((a, b)) = text_sel_sig.get() {
                                let lo = a.min(b);
                                let hi = a.max(b);
                                if lo < hi && hi <= s.len() {
                                    s.replace_range(lo..hi, "");
                                    val_sig.set(s.clone());
                                    text_sel_sig.set(None);
                                    caret_sig.set(Some(lo));
                                } else if lo == hi && lo <= s.len() && lo > 0 {
                                    let prev = s.floor_char_boundary(lo - 1);
                                    s.replace_range(prev..lo, "");
                                    val_sig.set(s);
                                    text_sel_sig.set(None);
                                    caret_sig.set(Some(prev));
                                }
                            } else {
                                let caret = caret_sig.get().unwrap_or(s.len());
                                let caret = caret.min(s.len());
                                if caret > 0 {
                                    let prev = s.floor_char_boundary(caret - 1);
                                    s.replace_range(prev..caret, "");
                                    val_sig.set(s);
                                    caret_sig.set(Some(prev));
                                }
                            }
                        }
                    } else if let Some(t) = event.text.as_ref() {
                        if !ime_active.get() && !t.is_empty() && !self.modifiers.control_key() {
                            if t.as_str() != "\r" && t.as_str() != "\n" {
                                let mut s = val_sig.get();
                                if let Some((a, b)) = text_sel_sig.get() {
                                    let lo = a.min(b);
                                    let hi = a.max(b);
                                    if lo < hi && hi <= s.len() {
                                        s.replace_range(lo..hi, "");
                                    }
                                    let mut insert_at = lo.min(s.len());
                                    insert_at = s.floor_char_boundary(insert_at);
                                    s.insert_str(insert_at, t);
                                    val_sig.set(s);
                                    text_sel_sig.set(None);
                                    caret_sig.set(Some(insert_at + t.len()));
                                } else {
                                    let mut insert_at = caret_sig.get().unwrap_or(s.len());
                                    insert_at = insert_at.min(s.len());
                                    insert_at = s.floor_char_boundary(insert_at);
                                    let ow_key = crate::components::input::input_overwrite_signal_key(rest);
                                    let overwrite_sig = crate::core::reactivity::use_signal(&ow_key, || false);
                                    let overwrite = overwrite_sig.get();
                                    if overwrite
                                        && text_sel_sig.get().is_none()
                                        && insert_at < s.len()
                                        && t.chars().count() == 1
                                    {
                                        let first = s[insert_at..].chars().next().unwrap();
                                        let olen = first.len_utf8();
                                        s.replace_range(insert_at..insert_at + olen, t);
                                        val_sig.set(s);
                                        caret_sig.set(Some(insert_at + t.len()));
                                    } else {
                                        s.insert_str(insert_at, t);
                                        val_sig.set(s);
                                        caret_sig.set(Some(insert_at + t.len()));
                                    }
                                }
                            }
                        }
                    }
                    if let Some(st) = (&mut *self.state.borrow_mut()).as_mut() {
                        st.window.request_redraw();
                    }
                }
                return true;
            }
        }
        false
    }
}
