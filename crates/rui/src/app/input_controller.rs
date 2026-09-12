use winit::event::{ElementState, Ime, KeyEvent};
use winit::keyboard::{Key, NamedKey};
use winit::window::WindowId;

use crate::components::input::{
    filter_insert_for_type, input_type_signal_key, strip_control_insert, InputType,
};
use crate::utils::clipboard::{
    consumes_undo_redo_letter, edit_shortcut_letter, get_text as clipboard_get_text,
    is_redo_shortcut, is_undo_shortcut, sanitize_pasted_line, set_text as clipboard_set_text,
    shortcut_modifiers as clipboard_shortcut_modifiers,
};
use crate::utils::input_hit::collect_focusable_ids;
use crate::utils::keyboard::{
    caret_line_end, caret_line_home, caret_move_left_multiline, caret_move_page_vertical,
    caret_move_right_multiline, caret_move_vertical, caret_move_vertical_visual_pref,
    caret_visual_line_end, caret_visual_line_home, delete_char_forward, extend_selection,
    normalized_selection,
};

use super::ime_support::{self, apply_ime_commit, prepare_compose_anchor, text_edit_rest};
use std::cell::Cell;

thread_local! {
    /// Hold-repeat (`event.repeat`) coalescing: winit delivers every queued KeyDown
    /// before `RedrawRequested`, so applying all of them makes N glyphs appear at once.
    static TEXT_EDIT_FRAME_MUTATED: Cell<bool> = const { Cell::new(false) };
    /// One-shot `Poll` so caret `WaitUntil` (~520ms) cannot delay the paint after a mutation.
    static TEXT_EDIT_FORCE_IMMEDIATE: Cell<bool> = const { Cell::new(false) };
}

fn mark_text_edit_frame_mutated() {
    TEXT_EDIT_FRAME_MUTATED.with(|c| c.set(true));
    TEXT_EDIT_FORCE_IMMEDIATE.with(|c| c.set(true));
}

pub(super) fn take_text_edit_force_immediate() -> bool {
    TEXT_EDIT_FORCE_IMMEDIATE.with(|c| c.replace(false))
}

pub(super) fn clear_text_edit_frame_mutated() {
    TEXT_EDIT_FRAME_MUTATED.with(|c| c.set(false));
}

/// Printable insert / Backspace / Delete — OS key-repeat of these must be 1-per-frame.
fn is_text_hold_mutation_key(key: &Key) -> bool {
    match key {
        Key::Named(NamedKey::Backspace | NamedKey::Delete | NamedKey::Space | NamedKey::Enter) => {
            true
        }
        Key::Character(s) => !s.is_empty() && s.chars().any(|c| !c.is_control()),
        _ => false,
    }
}

/// `false` → consume the OS repeat without mutating (paint the previous mutation first).
fn text_edit_repeat_allow(repeat: bool, is_mutation_key: bool, already_mutated: bool) -> bool {
    !(repeat && is_mutation_key && already_mutated)
}

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

fn commit_user_edit(
    rest: &str,
    val_sig: crate::core::reactivity::Signal<String>,
    caret_sig: crate::core::reactivity::Signal<Option<usize>>,
    text_sel_sig: crate::core::reactivity::Signal<Option<(usize, usize)>>,
    new_value: String,
    new_caret: usize,
    kind: crate::utils::text_edit_history::EditKind,
) {
    #[cfg(feature = "text-undo")]
    crate::utils::text_edit_history::will_user_edit_field(rest, kind);
    crate::utils::text_field::commit_value_sig(rest, val_sig, new_value);
    text_sel_sig.set(None);
    caret_sig.set(Some(new_caret));
    mark_text_edit_frame_mutated();
    #[cfg(feature = "text-undo")]
    crate::utils::text_edit_history::note_field_after_edit(rest);
    #[cfg(not(feature = "text-undo"))]
    let _ = kind;
}

fn is_tab_key(key: &Key) -> bool {
    matches!(key, Key::Named(NamedKey::Tab))
        || matches!(key, Key::Character(s) if s.as_str() == "\t")
}

/// Arrow / Home / End / Page: need visual-line metrics. Insert / Backspace / Delete do not.
fn text_edit_key_needs_visual_metrics(key: &Key) -> bool {
    matches!(
        key,
        Key::Named(
            NamedKey::ArrowLeft
                | NamedKey::ArrowRight
                | NamedKey::ArrowUp
                | NamedKey::ArrowDown
                | NamedKey::Home
                | NamedKey::End
                | NamedKey::PageUp
                | NamedKey::PageDown
        )
    )
}

/// Caret move, Delete, or Insert — not printable typing / Backspace (those have their own path).
fn text_edit_key_is_caret_or_delete(key: &Key) -> bool {
    text_edit_key_needs_visual_metrics(key)
        || matches!(key, Key::Named(NamedKey::Delete | NamedKey::Insert))
}

/// Platforms such as Windows come with `event.text` (Esc → U+001B) for Named keys such as Esc / F keys.
/// Only the Named key of Space / Enter allows character insertion (space, Textarea newline).
pub(crate) fn named_key_blocks_text_insert(key: &Key) -> bool {
    if is_tab_key(key) {
        return true;
    }
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
                            #[cfg(feature = "text-undo")]
                            crate::utils::text_edit_history::will_user_edit_field(
                                rest,
                                crate::utils::text_edit_history::EditKind::Replace,
                            );
                            crate::utils::text_field::commit_value_sig(rest, val_sig, new_val);
                            text_sel.set(None);
                            caret_sig.set(Some(anchor));
                            #[cfg(feature = "text-undo")]
                            crate::utils::text_edit_history::note_field_after_edit(rest);
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
                    let ck = crate::components::input::input_caret_signal_key(rest);
                    let caret_sig = crate::core::reactivity::use_signal(&ck, || None::<usize>);
                    let new_caret = if filtered.is_empty() && !s.is_empty() {
                        // All filtered by type: still clear preedit, do not change value.
                        anchor
                    } else {
                        #[cfg(feature = "text-undo")]
                        crate::utils::text_edit_history::will_user_edit_field(
                            rest,
                            crate::utils::text_edit_history::EditKind::Replace,
                        );
                        let (new, new_caret) = apply_ime_commit(&v, anchor, &filtered);
                        crate::utils::text_field::commit_value_sig(rest, val_sig, new);
                        caret_sig.set(Some(new_caret));
                        #[cfg(feature = "text-undo")]
                        crate::utils::text_edit_history::note_field_after_edit(rest);
                        new_caret
                    };
                    Self::clear_ime_compose_state(rest);
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
            if crate::components::sidebar::try_toggle_sidebar_from_key(&event, &self.modifiers) {
                return true;
            }
        }
        if event.state == ElementState::Pressed {
            if is_tab_key(&event.logical_key) && !event.repeat {
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
                    focused.set(next_id.clone());
                    if crate::utils::text_field::select_all_in_text_edit(&next_id) {
                        crate::renderer::restart_caret_blink();
                    }
                    // Focus chrome is paint-synced (peek), not rebuild-driven. Without a redraw,
                    // snapped focus rings never repaint after Tab (springs used to keep frames alive).
                    if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                        st.window.request_redraw();
                    }
                    return true;
                }
            } else if !is_tab_key(&event.logical_key) {
                let focused =
                    crate::core::reactivity::use_signal("__rui_button_focus_id", String::new);
                let ime_active = crate::core::reactivity::use_signal(
                    crate::components::IME_ACTIVE_SIGNAL,
                    || false,
                );
                let fid = focused.get();

                if allow_primary_tree
                    && event.state == ElementState::Pressed
                    && crate::utils::drag::is_active()
                {
                    match &event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            let _ = crate::utils::drag::cancel();
                            if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                                st.window.request_redraw();
                            }
                            return true;
                        }
                        Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::ArrowRight) => {
                            if crate::utils::drag::nudge_insert(1) {
                                if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                                    st.window.request_redraw();
                                }
                                return true;
                            }
                        }
                        Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowLeft) => {
                            if crate::utils::drag::nudge_insert(-1) {
                                if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                                    st.window.request_redraw();
                                }
                                return true;
                            }
                        }
                        _ => {}
                    }
                }

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

                    if allow_primary_tree
                        && !editing
                        && !ime_active.get()
                        && self.try_keyboard_scroll(&event, None)
                    {
                        return true;
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
                    if event.state == ElementState::Pressed
                        && !text_edit_repeat_allow(
                            event.repeat,
                            is_text_hold_mutation_key(&event.logical_key),
                            TEXT_EDIT_FRAME_MUTATED.with(|c| c.get()),
                        )
                    {
                        return true;
                    }
                    let value_key = crate::components::input::input_value_signal_key(rest);
                    let val_sig = crate::core::reactivity::use_signal(&value_key, String::new);
                    let pk = crate::components::input::input_preedit_signal_key(rest);
                    let preedit_sig = crate::core::reactivity::use_signal(&pk, String::new);
                    let ck = crate::components::input::input_caret_signal_key(rest);
                    let caret_sig = crate::core::reactivity::use_signal(&ck, || None::<usize>);
                    let tsk = crate::components::input::input_selection_signal_key(rest);
                    let text_sel_sig =
                        crate::core::reactivity::use_signal(&tsk, || None::<(usize, usize)>);
                    let pref_x_key =
                        crate::utils::text_field::input_caret_preferred_x_signal_key(rest);
                    let pref_x_sig =
                        crate::core::reactivity::use_signal(&pref_x_key, || None::<f32>);
                    let pre = preedit_sig.get();
                    let composing = ime_active.get() && !pre.is_empty();
                    let password_field = input_type_for(rest).is_password();
                    if !composing
                        && event.state == ElementState::Pressed
                        && is_undo_shortcut(&event, &self.modifiers)
                    {
                        // Always consume Ctrl/Cmd+Z so `z` is never inserted — even when
                        // `text-undo` is off or the stack is empty (same as today with no past).
                        #[cfg(feature = "text-undo")]
                        if crate::utils::text_edit_history::undo_field(rest) {
                            crate::renderer::restart_caret_blink();
                        }
                        if allow_primary_tree {
                            if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                                st.window.request_redraw();
                            }
                        }
                        return true;
                    }
                    if !composing
                        && event.state == ElementState::Pressed
                        && is_redo_shortcut(&event, &self.modifiers)
                    {
                        #[cfg(feature = "text-undo")]
                        if crate::utils::text_edit_history::redo_field(rest) {
                            crate::renderer::restart_caret_blink();
                        }
                        if allow_primary_tree {
                            if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                                st.window.request_redraw();
                            }
                        }
                        return true;
                    }
                    if clipboard_shortcut_modifiers(&self.modifiers) {
                        if let Some(k) = edit_shortcut_letter(&event) {
                            let mut handled = false;
                            if !composing {
                                match k {
                                    'a' => {
                                        crate::utils::text_field::select_all_in_text_edit(
                                            &crate::utils::text_field::focus_id_for_text_edit(
                                                rest, multiline,
                                            ),
                                        );
                                        crate::renderer::restart_caret_blink();
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
                                                    let caret = insert_at + text.len();
                                                    commit_user_edit(
                                                        rest,
                                                        val_sig,
                                                        caret_sig,
                                                        text_sel_sig,
                                                        s,
                                                        caret,
                                                        crate::utils::text_edit_history::EditKind::Replace,
                                                    );
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
                                            commit_user_edit(
                                                rest,
                                                val_sig,
                                                caret_sig,
                                                text_sel_sig,
                                                s,
                                                lo,
                                                crate::utils::text_edit_history::EditKind::Replace,
                                            );
                                        }
                                        handled = true;
                                    }
                                    _ if consumes_undo_redo_letter(Some(k), &self.modifiers) => {
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
                    if !composing && text_edit_key_is_caret_or_delete(&event.logical_key) {
                        let needs_visual = text_edit_key_needs_visual_metrics(&event.logical_key);
                        let wrap_w = if needs_visual && multiline {
                            crate::core::reactivity::use_signal(
                                &crate::utils::text_field::textarea_wrap_width_signal_key(rest),
                                || 0.0_f32,
                            )
                            .peek()
                        } else {
                            0.0
                        };
                        // Live leaf metrics (avoid hardcoded 0.875×16). Skip the tree walk
                        // on insert / Backspace / Delete hold-repeat — those keys never
                        // consume nav_size / visual lines.
                        let (nav_size, nav_weight, nav_lh, nav_wrap) = if needs_visual
                            && allow_primary_tree
                        {
                            (*self.state.borrow()).as_ref().map_or(
                                (14.0_f32, 400_u16, 21.0_f32, crate::text::WrapMode::Word),
                                |st| {
                                    super::tree_hit::find_input_text_leaf_for_rest(
                                        &st.root_element,
                                        None,
                                        rest,
                                    )
                                    .map(|(_, _, style, _)| {
                                        let sz = style.size_resolved().max(1.0);
                                        let w = style.weight_resolved().max(100);
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
                        let s = val_sig.get();
                        let caret = caret_sig.get().unwrap_or(s.len()).min(s.len());
                        let caret = s.floor_char_boundary(caret);
                        let shift = self.modifiers.shift_key();
                        let soft_visual_model = if needs_visual
                            && multiline
                            && wrap_w > 1.0
                            && nav_wrap.soft_wrap()
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
                        let visual_ranges = soft_visual_model.as_ref().map(|m| m.ranges.as_slice());
                        let lines_per_page = if multiline {
                            let ta_rest = crate::utils::text_field::textarea_scroll_rest(rest);
                            let vh_key = crate::core::scroll_viewport::scroll_viewport_h_signal_key(
                                &ta_rest,
                            );
                            let vh = crate::core::reactivity::use_signal(&vh_key, || 0.0_f32)
                                .peek()
                                .max(nav_lh);
                            (vh / nav_lh).max(1.0).floor() as usize
                        } else {
                            1
                        };
                        let mut nav_handled = false;
                        let mut boundary_scroll_key: Option<Key> = None;
                        match &event.logical_key {
                            Key::Named(NamedKey::ArrowLeft) => {
                                let dest = if !shift {
                                    if let Some((a, b)) = text_sel_sig.get() {
                                        let lo = a.min(b);
                                        let hi = a.max(b);
                                        if lo < hi {
                                            lo
                                        } else {
                                            caret_move_left_multiline(&s, caret, visual_ranges)
                                        }
                                    } else {
                                        caret_move_left_multiline(&s, caret, visual_ranges)
                                    }
                                } else {
                                    caret_move_left_multiline(&s, caret, visual_ranges)
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
                                            caret_move_right_multiline(&s, caret, visual_ranges)
                                        }
                                    } else {
                                        caret_move_right_multiline(&s, caret, visual_ranges)
                                    }
                                } else {
                                    caret_move_right_multiline(&s, caret, visual_ranges)
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
                                if dest == caret && multiline {
                                    boundary_scroll_key = Some(event.logical_key.clone());
                                }
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
                                if dest == caret && multiline {
                                    boundary_scroll_key = Some(event.logical_key.clone());
                                }
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
                            Key::Named(NamedKey::PageUp) => {
                                let dest = if multiline {
                                    let px = soft_visual_model.as_ref().map(|m| {
                                        pref_x_sig
                                            .peek()
                                            .unwrap_or_else(|| m.caret_x_on_line(caret))
                                    });
                                    if let Some(ref model) = soft_visual_model {
                                        if pref_x_sig.peek().is_none() {
                                            if let Some(px) = px {
                                                pref_x_sig.set_silent(Some(px));
                                            }
                                        }
                                        caret_move_page_vertical(
                                            &s,
                                            Some(&model.ranges),
                                            Some(&model.caret_xs),
                                            caret,
                                            -1,
                                            lines_per_page,
                                            px,
                                        )
                                    } else {
                                        caret_move_page_vertical(
                                            &s,
                                            None,
                                            None,
                                            caret,
                                            -1,
                                            lines_per_page,
                                            None,
                                        )
                                    }
                                } else {
                                    0
                                };
                                if dest == caret && multiline {
                                    boundary_scroll_key = Some(event.logical_key.clone());
                                }
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
                            Key::Named(NamedKey::PageDown) => {
                                let dest = if multiline {
                                    let px = soft_visual_model.as_ref().map(|m| {
                                        pref_x_sig
                                            .peek()
                                            .unwrap_or_else(|| m.caret_x_on_line(caret))
                                    });
                                    if let Some(ref model) = soft_visual_model {
                                        if pref_x_sig.peek().is_none() {
                                            if let Some(px) = px {
                                                pref_x_sig.set_silent(Some(px));
                                            }
                                        }
                                        caret_move_page_vertical(
                                            &s,
                                            Some(&model.ranges),
                                            Some(&model.caret_xs),
                                            caret,
                                            1,
                                            lines_per_page,
                                            px,
                                        )
                                    } else {
                                        caret_move_page_vertical(
                                            &s,
                                            None,
                                            None,
                                            caret,
                                            1,
                                            lines_per_page,
                                            None,
                                        )
                                    }
                                } else {
                                    s.len()
                                };
                                if dest == caret && multiline {
                                    boundary_scroll_key = Some(event.logical_key.clone());
                                }
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
                                        commit_user_edit(
                                            rest,
                                            val_sig,
                                            caret_sig,
                                            text_sel_sig,
                                            s,
                                            lo,
                                            crate::utils::text_edit_history::EditKind::Replace,
                                        );
                                    }
                                } else {
                                    let mut c = caret_sig.get().unwrap_or(s.len()).min(s.len());
                                    c = s.floor_char_boundary(c);
                                    if delete_char_forward(&mut s, c) {
                                        commit_user_edit(
                                            rest,
                                            val_sig,
                                            caret_sig,
                                            text_sel_sig,
                                            s,
                                            c,
                                            crate::utils::text_edit_history::EditKind::Delete,
                                        );
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
                            crate::renderer::restart_caret_blink();
                            if allow_primary_tree {
                                if multiline {
                                    if let Some(scroll_key) = boundary_scroll_key.as_ref() {
                                        let ta_rest =
                                            crate::utils::text_field::textarea_scroll_rest(rest);
                                        let delta = match scroll_key {
                                            Key::Named(NamedKey::PageUp) => {
                                                nav_lh * lines_per_page as f32
                                            }
                                            Key::Named(NamedKey::PageDown) => {
                                                -nav_lh * lines_per_page as f32
                                            }
                                            Key::Named(NamedKey::ArrowUp) => nav_lh,
                                            Key::Named(NamedKey::ArrowDown) => -nav_lh,
                                            _ => 0.0,
                                        };
                                        if delta.abs() > 0.5 {
                                            if let Some(smooth) =
                                                super::scroll_support::nudge_scroll_viewport_y(
                                                    &ta_rest, delta,
                                                )
                                            {
                                                if smooth {
                                                    self.bump_smooth_active_until(
                                                        &ta_rest,
                                                        std::time::Instant::now(),
                                                    );
                                                }
                                            }
                                        }
                                    } else if self.modifiers.control_key()
                                        && matches!(
                                            &event.logical_key,
                                            Key::Named(
                                                NamedKey::PageUp
                                                    | NamedKey::PageDown
                                                    | NamedKey::Home
                                                    | NamedKey::End
                                            )
                                        )
                                    {
                                        self.try_keyboard_scroll(&event, None);
                                    }
                                }
                                if let Some(st) = (*self.state.borrow_mut()).as_mut() {
                                    st.window.request_redraw();
                                }
                            }
                            return true;
                        }
                        if allow_primary_tree
                            && !composing
                            && matches!(
                                &event.logical_key,
                                Key::Named(
                                    NamedKey::PageUp
                                        | NamedKey::PageDown
                                        | NamedKey::Home
                                        | NamedKey::End
                                )
                            )
                        {
                            let prefer = if multiline {
                                Some(crate::utils::text_field::textarea_scroll_rest(rest))
                            } else {
                                None
                            };
                            if self.try_keyboard_scroll(&event, prefer.as_deref()) {
                                return true;
                            }
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
                                    commit_user_edit(
                                        rest,
                                        val_sig,
                                        caret_sig,
                                        text_sel_sig,
                                        s,
                                        lo,
                                        crate::utils::text_edit_history::EditKind::Replace,
                                    );
                                } else if lo == hi && lo <= s.len() && lo > 0 {
                                    let prev = s.floor_char_boundary(lo - 1);
                                    s.replace_range(prev..lo, "");
                                    commit_user_edit(
                                        rest,
                                        val_sig,
                                        caret_sig,
                                        text_sel_sig,
                                        s,
                                        prev,
                                        crate::utils::text_edit_history::EditKind::Delete,
                                    );
                                }
                            } else {
                                let caret = caret_sig.get().unwrap_or(s.len());
                                let caret = caret.min(s.len());
                                if caret > 0 {
                                    let prev = s.floor_char_boundary(caret - 1);
                                    s.replace_range(prev..caret, "");
                                    commit_user_edit(
                                        rest,
                                        val_sig,
                                        caret_sig,
                                        text_sel_sig,
                                        s,
                                        prev,
                                        crate::utils::text_edit_history::EditKind::Delete,
                                    );
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
                                    commit_user_edit(
                                        rest,
                                        val_sig,
                                        caret_sig,
                                        text_sel_sig,
                                        s,
                                        insert_at + text.len(),
                                        crate::utils::text_edit_history::EditKind::Replace,
                                    );
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
                                        commit_user_edit(
                                            rest,
                                            val_sig,
                                            caret_sig,
                                            text_sel_sig,
                                            s,
                                            insert_at + text.len(),
                                            crate::utils::text_edit_history::EditKind::Replace,
                                        );
                                    } else {
                                        s.insert_str(insert_at, &text);
                                        commit_user_edit(
                                            rest,
                                            val_sig,
                                            caret_sig,
                                            text_sel_sig,
                                            s,
                                            insert_at + text.len(),
                                            crate::utils::text_edit_history::EditKind::Insert,
                                        );
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
        assert!(named_key_blocks_text_insert(&Key::Character("\t".into())));
        assert!(is_tab_key(&Key::Named(NamedKey::Tab)));
        assert!(is_tab_key(&Key::Character("\t".into())));
        assert!(!is_tab_key(&Key::Character("a".into())));
        assert!(!named_key_blocks_text_insert(&Key::Named(NamedKey::Space)));
        assert!(!named_key_blocks_text_insert(&Key::Named(NamedKey::Enter)));
        assert!(!named_key_blocks_text_insert(&Key::Character("a".into())));
        // Character(U+001B) may still appear; discarded by strip_control_insert, not in the Named guard.
        assert!(!named_key_blocks_text_insert(&Key::Character(
            "\u{1b}".into()
        )));
    }

    #[test]
    fn hold_repeat_insert_and_delete_skip_visual_metrics() {
        assert!(!text_edit_key_needs_visual_metrics(&Key::Character(
            "a".into()
        )));
        assert!(!text_edit_key_needs_visual_metrics(&Key::Named(
            NamedKey::Backspace
        )));
        assert!(!text_edit_key_needs_visual_metrics(&Key::Named(
            NamedKey::Delete
        )));
        assert!(!text_edit_key_is_caret_or_delete(&Key::Character(
            "a".into()
        )));
        assert!(!text_edit_key_is_caret_or_delete(&Key::Named(
            NamedKey::Backspace
        )));
        assert!(text_edit_key_is_caret_or_delete(&Key::Named(
            NamedKey::Delete
        )));
        assert!(text_edit_key_needs_visual_metrics(&Key::Named(
            NamedKey::ArrowLeft
        )));
        assert!(text_edit_key_needs_visual_metrics(&Key::Named(
            NamedKey::ArrowDown
        )));
        assert!(text_edit_key_needs_visual_metrics(&Key::Named(
            NamedKey::Home
        )));
        assert!(text_edit_key_needs_visual_metrics(&Key::Named(
            NamedKey::PageUp
        )));
    }

    #[test]
    fn hold_repeat_coalesces_to_one_mutation_per_frame() {
        clear_text_edit_frame_mutated();
        let insert = Key::Character("a".into());
        let backspace = Key::Named(NamedKey::Backspace);
        assert!(is_text_hold_mutation_key(&insert));
        assert!(is_text_hold_mutation_key(&backspace));
        assert!(!is_text_hold_mutation_key(&Key::Named(NamedKey::ArrowLeft)));

        assert!(
            text_edit_repeat_allow(false, true, false),
            "first keydown (repeat=false) always applies"
        );
        mark_text_edit_frame_mutated();
        let already = TEXT_EDIT_FRAME_MUTATED.with(|c| c.get());
        assert!(already);
        assert!(
            !text_edit_repeat_allow(true, true, already),
            "queued OS repeats in the same pump must not mutate"
        );
        assert!(!text_edit_repeat_allow(true, true, already));
        assert!(
            text_edit_repeat_allow(true, false, already),
            "arrow repeats are not hold-insert mutations"
        );
        clear_text_edit_frame_mutated();
        assert!(
            text_edit_repeat_allow(true, true, TEXT_EDIT_FRAME_MUTATED.with(|c| c.get())),
            "after redraw, the next repeat applies once"
        );
        let _ = take_text_edit_force_immediate();
    }

    #[test]
    fn hold_repeat_burst_applies_once_until_redraw() {
        clear_text_edit_frame_mutated();
        let _ = take_text_edit_force_immediate();
        let mut applied = 0_u32;
        for i in 0..12 {
            let repeat = i > 0;
            let already = TEXT_EDIT_FRAME_MUTATED.with(|c| c.get());
            if text_edit_repeat_allow(repeat, true, already) {
                mark_text_edit_frame_mutated();
                applied += 1;
            }
        }
        assert_eq!(
            applied, 1,
            "12 OS repeats in one pump must show as one glyph, not a batch"
        );
        assert!(take_text_edit_force_immediate());
        clear_text_edit_frame_mutated();
        let already = TEXT_EDIT_FRAME_MUTATED.with(|c| c.get());
        assert!(text_edit_repeat_allow(true, true, already));
    }
}
