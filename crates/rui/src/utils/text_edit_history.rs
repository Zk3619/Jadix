//! Per-field undo / redo for Input and Textarea.
//!
//! Gated by the `text-undo` Cargo feature (off by default). History lives in a named
//! [`use_signal`](crate::core::reactivity::use_signal) slot (`__rui_input_undo_{id}`), the same
//! identity model as the field value — **not** a process-level `HashMap`. Element unmount
//! resets that slot so dynamic lists do not leak or mix stacks.
//!
//! P2 stores [`TextChange`] deltas (instantaneous coordinates, one change per group by
//! default). Consecutive typing **lengthens** that one change; it does not append a second.
//! A replacement whose `old+new` exceeds the hard cap is a [`UndoPayload::LargeReplaceMarker`]
//! — earlier groups are kept. A Marker with [`RetainedSide::Old`] undoes to that text;
//! without Old it is a **wall** (`undo` returns `None`, document unchanged).
//! [`EditTransactionGuard`] closes the open group on Drop
//! (including panic unwind).
//!
//! Without the feature the recording APIs are no-ops. Ctrl/Cmd+Z/Y are still consumed.

#[cfg(feature = "text-undo")]
use std::ops::Range;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    Insert,
    Delete,
    Replace,
    Paste,
    Cut,
    /// Number steppers / hold-repeat: consecutive steps merge.
    Step,
    Programmatic,
    Other,
}

impl EditKind {
    #[cfg(feature = "text-undo")]
    fn can_time_merge(self) -> bool {
        matches!(self, Self::Insert | Self::Delete | Self::Step)
    }
}

/// How a programmatic write interacts with the undo stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetTextUndoPolicy {
    /// Do not record (form binding, unfocused store pull). [`commit_value`](crate::utils::text_field::commit_value) default.
    NotUndoable,
    /// Record as one undoable edit (find/replace, autocomplete).
    Undoable,
    /// Clear this field's history after the write (form reset / change document). Not for a large paste.
    ClearHistory,
}

/// Public knobs. Call [`TextUndoConfig::clamp`] (done by install) so `0` / huge values cannot silently break undo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextUndoConfig {
    pub enabled: bool,
    pub max_entries: usize,
    /// History payload bytes, **not** the document. `0` = entry-count only.
    pub max_bytes: usize,
    pub typing_merge: Duration,
    /// Password / secret fields: never record (Z/Y still consumed).
    pub secure_disable: bool,
}

const DEFAULT_MERGE: Duration = Duration::from_millis(800);
const MIN_BYTES_IF_SET: usize = 64 * 1024;
const MAX_ENTRIES_CAP: usize = 10_000;
#[cfg(feature = "text-undo")]
mod undo_caps {
    /// Single-edit `old+new` above this is a Marker, not a full delta (plan §9.2).
    pub(super) const LARGE_REPLACE_HARD: usize = 16 * 1024 * 1024;
    /// When emitting a Marker, keep the smaller side if it fits (undo huge paste over a short field).
    pub(super) const RETAIN_SIDE_MAX: usize = 1024 * 1024;
}

impl TextUndoConfig {
    pub fn input_default() -> Self {
        Self {
            enabled: true,
            max_entries: 200,
            max_bytes: 2 * 1024 * 1024,
            typing_merge: DEFAULT_MERGE,
            secure_disable: false,
        }
    }

    pub fn textarea_default() -> Self {
        Self {
            enabled: true,
            max_entries: 500,
            max_bytes: 8 * 1024 * 1024,
            typing_merge: DEFAULT_MERGE,
            secure_disable: false,
        }
    }

    pub fn password_default() -> Self {
        Self {
            enabled: false,
            max_entries: 1,
            max_bytes: 0,
            typing_merge: DEFAULT_MERGE,
            secure_disable: true,
        }
    }

    pub fn clamp(mut self) -> Self {
        self.max_entries = self.max_entries.clamp(1, MAX_ENTRIES_CAP);
        if self.max_bytes != 0 {
            self.max_bytes = self.max_bytes.max(MIN_BYTES_IF_SET);
        }
        self
    }
}

impl Default for TextUndoConfig {
    fn default() -> Self {
        Self::input_default()
    }
}

#[cfg(feature = "text-undo")]
/// One replacement at the document coordinates that existed **when it happened**.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextChange {
    pub range: Range<usize>,
    pub old_text: String,
    pub new_text: String,
}

#[cfg(feature = "text-undo")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditPoint {
    pub caret: usize,
    pub selection: Option<(usize, usize)>,
    pub extra_carets: Vec<(usize, usize)>,
}

#[cfg(feature = "text-undo")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UndoPayload {
    /// L1/L2: usually `len == 1`. Undo walks **reverse**, Redo **forward**.
    Changes(Vec<TextChange>),
    /// Over the hard cap: do not store both full texts; do not clear earlier groups.
    LargeReplaceMarker {
        old_hash: u64,
        new_hash: u64,
        byte_len: usize,
        /// The side that still fit in [`RETAIN_SIDE_MAX`]. Undo can restore `Old`.
        retained: Option<RetainedSide>,
    },
}

#[cfg(feature = "text-undo")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RetainedSide {
    Old(String),
    New(String),
}

#[cfg(feature = "text-undo")]
impl RetainedSide {
    fn len(&self) -> usize {
        match self {
            Self::Old(s) | Self::New(s) => s.len(),
        }
    }
}

#[cfg(feature = "text-undo")]
#[derive(Clone, Debug)]
struct UndoGroup {
    payload: UndoPayload,
    state_before: EditPoint,
    state_after: EditPoint,
    kind: EditKind,
    at: std::time::Instant,
}

#[cfg(feature = "text-undo")]
impl UndoGroup {
    fn payload_bytes(&self) -> usize {
        match &self.payload {
            UndoPayload::Changes(chs) => chs
                .iter()
                .map(|c| c.old_text.len() + c.new_text.len() + 32)
                .sum(),
            UndoPayload::LargeReplaceMarker { retained, .. } => {
                32 + retained.as_ref().map(RetainedSide::len).unwrap_or(0)
            }
        }
    }
}

#[cfg(feature = "text-undo")]
fn hash_text(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

#[cfg(feature = "text-undo")]
fn large_replace_hard() -> usize {
    #[cfg(test)]
    {
        TEST_LARGE_REPLACE_HARD.with(|c| c.get())
    }
    #[cfg(not(test))]
    {
        undo_caps::LARGE_REPLACE_HARD
    }
}

#[cfg(feature = "text-undo")]
fn retain_side_max() -> usize {
    #[cfg(test)]
    {
        TEST_RETAIN_SIDE_MAX.with(|c| c.get())
    }
    #[cfg(not(test))]
    {
        undo_caps::RETAIN_SIDE_MAX
    }
}

#[cfg(all(test, feature = "text-undo"))]
std::thread_local! {
    static TEST_LARGE_REPLACE_HARD: std::cell::Cell<usize> =
        const { std::cell::Cell::new(undo_caps::LARGE_REPLACE_HARD) };
    static TEST_RETAIN_SIDE_MAX: std::cell::Cell<usize> =
        const { std::cell::Cell::new(undo_caps::RETAIN_SIDE_MAX) };
}

/// Test-only caps so Marker / retain-side tests do not allocate 1–16 MiB.
#[cfg(all(test, feature = "text-undo"))]
pub fn with_test_replace_caps(hard: usize, retain: usize, f: impl FnOnce()) {
    TEST_LARGE_REPLACE_HARD.with(|h| {
        let prev_h = h.get();
        h.set(hard);
        TEST_RETAIN_SIDE_MAX.with(|r| {
            let prev_r = r.get();
            r.set(retain);
            f();
            r.set(prev_r);
        });
        h.set(prev_h);
    });
}

#[cfg(feature = "text-undo")]
/// Byte diff with UTF-8-safe prefix/suffix. `range` is on `before`.
fn diff_change(before: &str, after: &str) -> Option<TextChange> {
    if before == after {
        return None;
    }
    let b = before.as_bytes();
    let a = after.as_bytes();
    let mut prefix = 0;
    let max_pre = b.len().min(a.len());
    while prefix < max_pre && b[prefix] == a[prefix] {
        prefix += 1;
    }
    while prefix > 0 && !before.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let mut suffix = 0;
    let max_suf = (b.len() - prefix).min(a.len() - prefix);
    while suffix < max_suf && b[b.len() - 1 - suffix] == a[a.len() - 1 - suffix] {
        suffix += 1;
    }
    while suffix > 0
        && (!before.is_char_boundary(b.len() - suffix) || !after.is_char_boundary(a.len() - suffix))
    {
        suffix -= 1;
    }
    let old_hi = b.len() - suffix;
    let new_hi = a.len() - suffix;
    Some(TextChange {
        range: prefix..old_hi,
        old_text: before[prefix..old_hi].to_string(),
        new_text: after[prefix..new_hi].to_string(),
    })
}

#[cfg(feature = "text-undo")]
fn apply_forward(doc: &mut String, ch: &TextChange) {
    crate::utils::text_buffer::TextBuffer::replace(doc, ch.range.clone(), &ch.new_text);
}

#[cfg(feature = "text-undo")]
fn apply_reverse(doc: &mut String, ch: &TextChange) {
    let start = ch.range.start.min(doc.len());
    let end = start.saturating_add(ch.new_text.len());
    crate::utils::text_buffer::TextBuffer::replace(doc, start..end, &ch.old_text);
}

#[cfg(feature = "text-undo")]
fn try_merge_change(open: &mut TextChange, next: &TextChange, kind: EditKind) -> bool {
    match kind {
        EditKind::Insert => {
            if !next.old_text.is_empty() || next.range.start != next.range.end {
                return false;
            }
            if next.range.start != open.range.start + open.new_text.len() {
                return false;
            }
            open.new_text.push_str(&next.new_text);
            true
        }
        EditKind::Delete => {
            if !next.new_text.is_empty() || !open.new_text.is_empty() {
                return false;
            }
            // Backspace: next deletes immediately before the open range.
            if next.range.end == open.range.start {
                open.old_text = format!("{}{}", next.old_text, open.old_text);
                open.range.start = next.range.start;
                return true;
            }
            // Forward delete: same start, grows end.
            if next.range.start == open.range.start {
                open.old_text.push_str(&next.old_text);
                open.range.end += next.old_text.len();
                return true;
            }
            false
        }
        EditKind::Step => {
            open.new_text.clone_from(&next.new_text);
            true
        }
        _ => false,
    }
}

#[cfg(feature = "text-undo")]
fn payload_for_change(ch: TextChange) -> UndoPayload {
    let combined = ch.old_text.len().saturating_add(ch.new_text.len());
    if combined <= large_replace_hard() {
        return UndoPayload::Changes(vec![ch]);
    }
    let retained = if ch.old_text.len() <= retain_side_max() {
        Some(RetainedSide::Old(ch.old_text.clone()))
    } else if ch.new_text.len() <= retain_side_max() {
        Some(RetainedSide::New(ch.new_text.clone()))
    } else {
        None
    };
    UndoPayload::LargeReplaceMarker {
        old_hash: hash_text(&ch.old_text),
        new_hash: hash_text(&ch.new_text),
        byte_len: combined,
        retained,
    }
}

#[cfg(feature = "text-undo")]
mod stack {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;
    use std::time::Instant;

    use crate::core::reactivity::{remove_named_signal, use_signal};
    use crate::utils::text_field::{
        commit_value_sig, extra_carets_signal, input_caret_signal_key, input_selection_signal_key,
        input_value_signal_key,
    };

    use super::{
        apply_forward, apply_reverse, diff_change, payload_for_change, try_merge_change, EditKind,
        EditPoint, EditTransactionGuard, RetainedSide, SetTextUndoPolicy, TextUndoConfig,
        UndoGroup, UndoPayload,
    };

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct TextEditSnapshot {
        pub value: String,
        pub caret: usize,
        pub selection: Option<(usize, usize)>,
        pub extra_carets: Vec<(usize, usize)>,
    }

    impl Default for TextEditSnapshot {
        fn default() -> Self {
            Self {
                value: String::new(),
                caret: 0,
                selection: None,
                extra_carets: Vec::new(),
            }
        }
    }

    impl TextEditSnapshot {
        fn clamp(mut self) -> Self {
            let len = self.value.len();
            self.caret = self.caret.min(len);
            if let Some((a, b)) = self.selection {
                let lo = a.min(b).min(len);
                let hi = a.max(b).min(len);
                self.selection = if lo < hi { Some((lo, hi)) } else { None };
            }
            for (c, a) in &mut self.extra_carets {
                *c = (*c).min(len);
                *a = (*a).min(len);
            }
            self
        }

        fn point(&self) -> EditPoint {
            EditPoint {
                caret: self.caret,
                selection: self.selection,
                extra_carets: self.extra_carets.clone(),
            }
        }
    }

    struct UndoManager {
        past: VecDeque<UndoGroup>,
        current: Option<TextEditSnapshot>,
        future: VecDeque<UndoGroup>,
        last_kind: Option<EditKind>,
        last_at: Instant,
        config: TextUndoConfig,
        current_bytes: usize,
        applying: bool,
        is_internal_sync: bool,
        open_txn: bool,
        pending_before: Option<TextEditSnapshot>,
        pending_kind: Option<EditKind>,
        coalesce_next: bool,
    }

    impl UndoManager {
        fn new(config: TextUndoConfig) -> Self {
            Self {
                past: VecDeque::new(),
                current: None,
                future: VecDeque::new(),
                last_kind: None,
                last_at: Instant::now(),
                config,
                current_bytes: 0,
                applying: false,
                is_internal_sync: false,
                open_txn: false,
                pending_before: None,
                pending_kind: None,
                coalesce_next: false,
            }
        }

        fn records(&self) -> bool {
            self.config.enabled && !self.config.secure_disable
        }

        fn blocked(&self) -> bool {
            self.applying || self.is_internal_sync || !self.records()
        }

        fn push_past(&mut self, group: UndoGroup) {
            self.current_bytes = self.current_bytes.saturating_add(group.payload_bytes());
            self.past.push_back(group);
            self.trim();
        }

        fn trim(&mut self) {
            while self.past.len() > self.config.max_entries {
                if let Some(old) = self.past.pop_front() {
                    self.current_bytes = self.current_bytes.saturating_sub(old.payload_bytes());
                }
            }
            if self.config.max_bytes == 0 {
                return;
            }
            // Keep a single oversized group so a large paste is still one undo step.
            while self.current_bytes > self.config.max_bytes && self.past.len() > 1 {
                if let Some(old) = self.past.pop_front() {
                    self.current_bytes = self.current_bytes.saturating_sub(old.payload_bytes());
                }
            }
        }

        fn can_coalesce(&self, kind: EditKind, before: &TextEditSnapshot) -> bool {
            if self.open_txn {
                return true;
            }
            if self.last_kind != Some(kind) || !kind.can_time_merge() {
                return false;
            }
            if !self.future.is_empty() {
                return false;
            }
            let merge = self.config.typing_merge;
            if !merge.is_zero() && self.last_at.elapsed() >= merge {
                return false;
            }
            if !matches!(kind, EditKind::Step) {
                let Some(cur) = self.current.as_ref() else {
                    return false;
                };
                if cur.caret != before.caret || cur.extra_carets != before.extra_carets {
                    return false;
                }
            }
            true
        }

        fn close_txn(&mut self) {
            self.open_txn = false;
            self.coalesce_next = false;
            self.last_kind = None;
            self.pending_before = None;
            self.pending_kind = None;
        }
    }

    pub fn undo_manager_signal_key(field_id: &str) -> String {
        format!("__rui_input_undo_{field_id}")
    }

    fn manager_cell(field_id: &str) -> Rc<RefCell<UndoManager>> {
        use_signal(&undo_manager_signal_key(field_id), || {
            Rc::new(RefCell::new(UndoManager::new(
                TextUndoConfig::input_default().clamp(),
            )))
        })
        .peek()
    }

    pub fn install_field_config(field_id: &str, config: TextUndoConfig) {
        let cell = manager_cell(field_id);
        cell.borrow_mut().config = config.clamp();
    }

    pub fn capture_field(field_id: &str) -> TextEditSnapshot {
        let value = use_signal(&input_value_signal_key(field_id), String::new).peek();
        let caret = use_signal(&input_caret_signal_key(field_id), || None::<usize>)
            .peek()
            .unwrap_or(value.len())
            .min(value.len());
        let selection = use_signal(&input_selection_signal_key(field_id), || {
            None::<(usize, usize)>
        })
        .peek();
        let extra_carets = extra_carets_signal(field_id).peek();
        TextEditSnapshot {
            value,
            caret,
            selection,
            extra_carets,
        }
        .clamp()
    }

    pub fn apply_snapshot(field_id: &str, snap: &TextEditSnapshot) {
        let snap = snap.clone().clamp();
        let store = use_signal(&input_value_signal_key(field_id), String::new);
        commit_value_sig(field_id, store, snap.value.clone());
        use_signal(&input_caret_signal_key(field_id), || None::<usize>).set(Some(snap.caret));
        use_signal(&input_selection_signal_key(field_id), || {
            None::<(usize, usize)>
        })
        .set(snap.selection);
        extra_carets_signal(field_id).set(snap.extra_carets);
    }

    struct FlagGuard {
        field_id: String,
        applying: bool,
    }

    impl Drop for FlagGuard {
        fn drop(&mut self) {
            let cell = manager_cell(&self.field_id);
            let mut st = cell.borrow_mut();
            if self.applying {
                st.applying = false;
            } else {
                st.is_internal_sync = false;
            }
        }
    }

    pub fn with_internal_sync(field_id: &str, f: impl FnOnce()) {
        manager_cell(field_id).borrow_mut().is_internal_sync = true;
        let _g = FlagGuard {
            field_id: field_id.to_string(),
            applying: false,
        };
        f();
    }

    fn with_applying(field_id: &str, f: impl FnOnce()) {
        manager_cell(field_id).borrow_mut().applying = true;
        let _g = FlagGuard {
            field_id: field_id.to_string(),
            applying: true,
        };
        f();
    }

    pub fn will_user_edit_field(field_id: &str, kind: EditKind) {
        will_user_edit(field_id, capture_field(field_id), kind);
    }

    pub fn note_field_after_edit(field_id: &str) {
        note_after_edit(field_id, capture_field(field_id));
    }

    pub fn will_user_edit(field_id: &str, before: TextEditSnapshot, kind: EditKind) {
        let before = before.clamp();
        let cell = manager_cell(field_id);
        let mut st = cell.borrow_mut();
        if st.blocked() {
            st.pending_before = None;
            st.pending_kind = None;
            st.coalesce_next = false;
            return;
        }
        let coalesce = st.can_coalesce(kind, &before);
        if !coalesce {
            st.future.clear();
        }
        st.current = Some(before.clone());
        st.pending_before = Some(before);
        st.pending_kind = Some(kind);
        st.coalesce_next = coalesce;
        st.last_kind = Some(kind);
        st.last_at = Instant::now();
    }

    pub fn note_after_edit(field_id: &str, after: TextEditSnapshot) {
        let after = after.clamp();
        let cell = manager_cell(field_id);
        let mut st = cell.borrow_mut();
        if !st.records() {
            return;
        }
        let Some(before) = st.pending_before.take() else {
            st.current = Some(after);
            return;
        };
        let kind = st.pending_kind.take().unwrap_or(EditKind::Other);
        let coalesce = st.coalesce_next;
        st.coalesce_next = false;
        let Some(ch) = diff_change(&before.value, &after.value) else {
            st.current = Some(after);
            return;
        };
        if st.blocked() {
            st.current = Some(after);
            return;
        }
        if coalesce {
            if let Some(mut g) = st.past.pop_back() {
                st.current_bytes = st.current_bytes.saturating_sub(g.payload_bytes());
                let merged = if st.open_txn {
                    if let UndoPayload::Changes(chs) = &mut g.payload {
                        chs.push(ch.clone());
                        g.state_after = after.point();
                        g.at = Instant::now();
                        true
                    } else {
                        false
                    }
                } else {
                    match &mut g.payload {
                        UndoPayload::Changes(chs) if chs.len() == 1 && g.kind == kind => {
                            if try_merge_change(&mut chs[0], &ch, kind) {
                                g.state_after = after.point();
                                g.at = Instant::now();
                                true
                            } else {
                                false
                            }
                        }
                        _ => false,
                    }
                };
                if merged {
                    st.push_past(g);
                    st.current = Some(after);
                    return;
                }
                st.push_past(g);
            }
        }
        let group = UndoGroup {
            payload: payload_for_change(ch),
            state_before: before.point(),
            state_after: after.point(),
            kind,
            at: Instant::now(),
        };
        st.future.clear();
        st.push_past(group);
        st.current = Some(after);
        st.last_kind = Some(kind);
        st.last_at = Instant::now();
    }

    fn snapshot_after_undo(current: &TextEditSnapshot, group: &UndoGroup) -> TextEditSnapshot {
        let mut doc = current.clone();
        match &group.payload {
            UndoPayload::Changes(chs) => {
                for ch in chs.iter().rev() {
                    apply_reverse(&mut doc.value, ch);
                }
            }
            UndoPayload::LargeReplaceMarker { retained, .. } => {
                if let Some(RetainedSide::Old(s)) = retained {
                    doc.value.clone_from(s);
                }
            }
        }
        doc.caret = group.state_before.caret;
        doc.selection = group.state_before.selection;
        doc.extra_carets = group.state_before.extra_carets.clone();
        doc.clamp()
    }

    fn snapshot_after_redo(current: &TextEditSnapshot, group: &UndoGroup) -> TextEditSnapshot {
        let mut doc = current.clone();
        match &group.payload {
            UndoPayload::Changes(chs) => {
                for ch in chs {
                    apply_forward(&mut doc.value, ch);
                }
            }
            UndoPayload::LargeReplaceMarker { retained, .. } => {
                if let Some(RetainedSide::New(s)) = retained {
                    doc.value.clone_from(s);
                }
            }
        }
        doc.caret = group.state_after.caret;
        doc.selection = group.state_after.selection;
        doc.extra_carets = group.state_after.extra_carets.clone();
        doc.clamp()
    }

    fn marker_unrestorable_on_undo(payload: &UndoPayload) -> bool {
        matches!(
            payload,
            UndoPayload::LargeReplaceMarker {
                retained: None | Some(RetainedSide::New(_)),
                ..
            }
        )
    }

    fn marker_unrestorable_on_redo(payload: &UndoPayload) -> bool {
        matches!(
            payload,
            UndoPayload::LargeReplaceMarker {
                retained: None | Some(RetainedSide::Old(_)),
                ..
            }
        )
    }

    pub fn undo(field_id: &str) -> Option<TextEditSnapshot> {
        let cell = manager_cell(field_id);
        let mut st = cell.borrow_mut();
        if !st.records() {
            return None;
        }
        // Unrestorable Marker is a wall: Z does not apply earlier deltas onto the
        // current (post-replace) document — those ranges belong to the pre-marker text.
        if st
            .past
            .back()
            .is_some_and(|g| marker_unrestorable_on_undo(&g.payload))
        {
            return None;
        }
        let group = st.past.pop_back()?;
        st.current_bytes = st.current_bytes.saturating_sub(group.payload_bytes());
        let cur = st.current.clone().unwrap_or_default();
        let next = snapshot_after_undo(&cur, &group);
        st.future.push_back(group);
        st.current = Some(next.clone());
        st.last_kind = None;
        Some(next)
    }

    pub fn redo(field_id: &str) -> Option<TextEditSnapshot> {
        let cell = manager_cell(field_id);
        let mut st = cell.borrow_mut();
        if !st.records() {
            return None;
        }
        if st
            .future
            .back()
            .is_some_and(|g| marker_unrestorable_on_redo(&g.payload))
        {
            return None;
        }
        let group = st.future.pop_back()?;
        let cur = st.current.clone().unwrap_or_default();
        let next = snapshot_after_redo(&cur, &group);
        st.push_past(group);
        st.current = Some(next.clone());
        st.last_kind = None;
        Some(next)
    }

    pub fn undo_field(field_id: &str) -> bool {
        let Some(snap) = undo(field_id) else {
            return false;
        };
        with_applying(field_id, || apply_snapshot(field_id, &snap));
        true
    }

    pub fn redo_field(field_id: &str) -> bool {
        let Some(snap) = redo(field_id) else {
            return false;
        };
        with_applying(field_id, || apply_snapshot(field_id, &snap));
        true
    }

    pub fn can_undo(field_id: &str) -> bool {
        let cell = manager_cell(field_id);
        let st = cell.borrow();
        st.records()
            && st
                .past
                .back()
                .is_some_and(|g| !marker_unrestorable_on_undo(&g.payload))
    }

    pub fn can_redo(field_id: &str) -> bool {
        let cell = manager_cell(field_id);
        let st = cell.borrow();
        st.records()
            && st
                .future
                .back()
                .is_some_and(|g| !marker_unrestorable_on_redo(&g.payload))
    }

    /// Does **not** create a slot. Used by L3 close-tab memory checks.
    pub fn undo_slot_exists(field_id: &str) -> bool {
        crate::core::reactivity::named_signal_exists(&undo_manager_signal_key(field_id))
    }

    pub fn apply_set_text_policy(
        field_id: &str,
        policy: SetTextUndoPolicy,
        before: TextEditSnapshot,
    ) {
        match policy {
            SetTextUndoPolicy::NotUndoable => {}
            SetTextUndoPolicy::Undoable => will_user_edit(field_id, before, EditKind::Replace),
            SetTextUndoPolicy::ClearHistory => reset(field_id),
        }
    }

    /// Drop this field's undo slot (unmount, form reset, change document).
    pub fn reset(field_id: &str) {
        let key = undo_manager_signal_key(field_id);
        remove_named_signal(&key);
    }

    pub fn edit_group(field_id: &str, kind: EditKind) -> EditTransactionGuard {
        let cell = manager_cell(field_id);
        let mut st = cell.borrow_mut();
        let armed = !st.blocked();
        if armed {
            st.open_txn = true;
            st.last_kind = None;
            st.future.clear();
            st.coalesce_next = false;
            st.pending_before = None;
            st.pending_kind = None;
            let _ = kind;
        }
        EditTransactionGuard {
            field_id: field_id.to_string(),
            armed,
        }
    }

    pub(super) fn close_edit_group(field_id: &str) {
        let cell = manager_cell(field_id);
        cell.borrow_mut().close_txn();
    }

    #[cfg(test)]
    pub fn past_len(field_id: &str) -> usize {
        manager_cell(field_id).borrow().past.len()
    }

    #[cfg(test)]
    pub fn current_bytes(field_id: &str) -> usize {
        manager_cell(field_id).borrow().current_bytes
    }

    #[cfg(test)]
    pub fn last_payload_is_marker(field_id: &str) -> bool {
        manager_cell(field_id)
            .borrow()
            .past
            .back()
            .is_some_and(|g| matches!(g.payload, UndoPayload::LargeReplaceMarker { .. }))
    }

    #[cfg(test)]
    pub fn last_group_change_count(field_id: &str) -> usize {
        match manager_cell(field_id)
            .borrow()
            .past
            .back()
            .map(|g| &g.payload)
        {
            Some(UndoPayload::Changes(chs)) => chs.len(),
            _ => 0,
        }
    }
}

#[cfg(feature = "text-undo")]
pub use stack::*;

/// RAII edit transaction. Drop (including unwind) closes the open group so the next
/// keystroke is a new undo step. Not a public `begin`/`end` pair.
pub struct EditTransactionGuard {
    field_id: String,
    armed: bool,
}

impl Drop for EditTransactionGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        self.armed = false;
        #[cfg(feature = "text-undo")]
        stack::close_edit_group(&self.field_id);
        let _ = &self.field_id;
    }
}

#[cfg(not(feature = "text-undo"))]
mod stubs {
    use super::{EditKind, EditTransactionGuard, TextUndoConfig};

    #[inline]
    pub fn will_user_edit_field(_field_id: &str, _kind: EditKind) {}

    #[inline]
    pub fn note_field_after_edit(_field_id: &str) {}

    #[inline]
    pub fn undo_field(_field_id: &str) -> bool {
        false
    }

    #[inline]
    pub fn redo_field(_field_id: &str) -> bool {
        false
    }

    #[inline]
    pub fn can_undo(_field_id: &str) -> bool {
        false
    }

    #[inline]
    pub fn can_redo(_field_id: &str) -> bool {
        false
    }

    #[inline]
    pub fn undo_slot_exists(_field_id: &str) -> bool {
        false
    }

    #[inline]
    pub fn reset(_field_id: &str) {}

    #[inline]
    pub fn install_field_config(_field_id: &str, _config: TextUndoConfig) {}

    #[inline]
    pub fn with_internal_sync(_field_id: &str, f: impl FnOnce()) {
        f();
    }

    #[inline]
    pub fn edit_group(_field_id: &str, _kind: EditKind) -> EditTransactionGuard {
        EditTransactionGuard {
            field_id: String::new(),
            armed: false,
        }
    }
}

#[cfg(not(feature = "text-undo"))]
pub use stubs::*;

#[cfg(all(test, not(feature = "text-undo")))]
mod feature_off_tests {
    use super::*;

    #[test]
    fn history_apis_are_nops_without_feature() {
        will_user_edit_field("off", EditKind::Insert);
        note_field_after_edit("off");
        assert!(!undo_field("off"));
        assert!(!redo_field("off"));
        reset("off");
        install_field_config("off", TextUndoConfig::input_default());
        drop(edit_group("off", EditKind::Replace));
    }
}

#[cfg(all(test, feature = "text-undo"))]
mod tests {
    use super::*;
    use crate::core::reactivity::{create_root, use_signal};
    use crate::core::tree::UnmountHook;
    use crate::utils::text_field::input_value_signal_key;
    use std::panic::{catch_unwind, AssertUnwindSafe};

    fn snap(value: &str, caret: usize) -> TextEditSnapshot {
        TextEditSnapshot {
            value: value.into(),
            caret,
            ..Default::default()
        }
    }

    fn snap_ex(value: &str, caret: usize, extra_carets: Vec<(usize, usize)>) -> TextEditSnapshot {
        TextEditSnapshot {
            value: value.into(),
            caret,
            extra_carets,
            ..Default::default()
        }
    }

    #[test]
    fn insert_coalesces_then_undo_restores_start() {
        reset("h1");
        let id = "h1";
        will_user_edit(id, snap("", 0), EditKind::Insert);
        note_after_edit(id, snap("a", 1));
        will_user_edit(id, snap("a", 1), EditKind::Insert);
        note_after_edit(id, snap("ab", 2));
        will_user_edit(id, snap("ab", 2), EditKind::Insert);
        note_after_edit(id, snap("abc", 3));
        assert_eq!(past_len(id), 1);
        let undone = undo(id).expect("undo");
        assert_eq!(undone.value, "");
        assert_eq!(undo(id), None);
        let redone = redo(id).expect("redo");
        assert_eq!(redone.value, "abc");
        reset(id);
    }

    #[test]
    fn replace_does_not_coalesce_with_insert() {
        reset("h2");
        let id = "h2";
        will_user_edit(id, snap("", 0), EditKind::Insert);
        note_after_edit(id, snap("a", 1));
        will_user_edit(id, snap("a", 1), EditKind::Replace);
        note_after_edit(id, snap("hello", 5));
        assert_eq!(undo(id).unwrap().value, "a");
        assert_eq!(undo(id).unwrap().value, "");
        reset(id);
    }

    #[test]
    fn delete_coalesces() {
        reset("h3");
        let id = "h3";
        will_user_edit(id, snap("ab", 2), EditKind::Delete);
        note_after_edit(id, snap("a", 1));
        will_user_edit(id, snap("a", 1), EditKind::Delete);
        note_after_edit(id, snap("", 0));
        assert_eq!(past_len(id), 1);
        assert_eq!(undo(id).unwrap().value, "ab");
        assert_eq!(undo(id), None);
        reset(id);
    }

    #[test]
    fn redo_cleared_by_new_edit() {
        reset("h4");
        let id = "h4";
        will_user_edit(id, snap("", 0), EditKind::Insert);
        note_after_edit(id, snap("a", 1));
        let _ = undo(id);
        will_user_edit(id, snap("", 0), EditKind::Insert);
        note_after_edit(id, snap("b", 1));
        assert_eq!(redo(id), None);
        assert_eq!(undo(id).unwrap().value, "");
        reset(id);
    }

    #[test]
    fn step_coalesces() {
        reset("h5");
        let id = "h5";
        will_user_edit(id, snap("1", 1), EditKind::Step);
        note_after_edit(id, snap("2", 1));
        will_user_edit(id, snap("2", 1), EditKind::Step);
        note_after_edit(id, snap("3", 1));
        assert_eq!(undo(id).unwrap().value, "1");
        assert_eq!(undo(id), None);
        reset(id);
    }

    #[test]
    fn paste_replace_does_not_merge_with_typing() {
        reset("h_paste");
        let id = "h_paste";
        will_user_edit(id, snap("", 0), EditKind::Insert);
        note_after_edit(id, snap("hi", 2));
        will_user_edit(id, snap("hi", 2), EditKind::Replace);
        note_after_edit(id, snap("hipasted", 8));
        will_user_edit(id, snap("hipasted", 8), EditKind::Insert);
        note_after_edit(id, snap("hipasted!", 9));
        assert_eq!(undo(id).unwrap().value, "hipasted");
        assert_eq!(undo(id).unwrap().value, "hi");
        assert_eq!(undo(id).unwrap().value, "");
        reset(id);
    }

    #[test]
    fn ime_commit_replace_is_own_group() {
        reset("h_ime");
        let id = "h_ime";
        will_user_edit(id, snap("", 0), EditKind::Insert);
        note_after_edit(id, snap("a", 1));
        will_user_edit(id, snap("a", 1), EditKind::Replace);
        note_after_edit(id, snap("a你好", 7));
        will_user_edit(id, snap("a你好", 7), EditKind::Insert);
        note_after_edit(id, snap("a你好b", 8));
        assert_eq!(undo(id).unwrap().value, "a你好");
        assert_eq!(undo(id).unwrap().value, "a");
        assert_eq!(undo(id).unwrap().value, "");
        reset(id);
    }

    #[test]
    fn caret_jump_starts_new_insert_group() {
        reset("h_jump");
        let id = "h_jump";
        will_user_edit(id, snap("", 0), EditKind::Insert);
        note_after_edit(id, snap("a", 1));
        will_user_edit(id, snap("a", 0), EditKind::Insert);
        note_after_edit(id, snap("xa", 1));
        assert_eq!(past_len(id), 2);
        assert_eq!(undo(id).unwrap().value, "a");
        assert_eq!(undo(id).unwrap().value, "");
        reset(id);
    }

    #[test]
    fn undo_restores_extra_carets() {
        reset("h_ex");
        let id = "h_ex";
        will_user_edit(id, snap_ex("ab", 0, vec![(1, 1)]), EditKind::Insert);
        note_after_edit(id, snap_ex("XaXb", 1, vec![(3, 3)]));
        let undone = undo(id).expect("undo");
        assert_eq!(undone.value, "ab");
        assert_eq!(undone.caret, 0);
        assert_eq!(undone.extra_carets, vec![(1, 1)]);
        let redone = redo(id).expect("redo");
        assert_eq!(redone.value, "XaXb");
        assert_eq!(redone.caret, 1);
        assert_eq!(redone.extra_carets, vec![(3, 3)]);
        reset(id);
    }

    #[test]
    fn extra_carets_coalesce_across_typing() {
        reset("h_exc");
        let id = "h_exc";
        // Extra caret sits before a contiguous tail insert so the payloads merge,
        // while extra_carets stay in the snapshot (and must not break coalesce).
        will_user_edit(id, snap_ex("ab", 2, vec![(0, 0)]), EditKind::Insert);
        note_after_edit(id, snap_ex("abx", 3, vec![(0, 0)]));
        will_user_edit(id, snap_ex("abx", 3, vec![(0, 0)]), EditKind::Insert);
        note_after_edit(id, snap_ex("abxy", 4, vec![(0, 0)]));
        assert_eq!(past_len(id), 1);
        let undone = undo(id).expect("undo");
        assert_eq!(undone.value, "ab");
        assert_eq!(undone.caret, 2);
        assert_eq!(undone.extra_carets, vec![(0, 0)]);
        reset(id);
    }

    #[test]
    fn extra_caret_change_starts_new_insert_group() {
        reset("h_exj");
        let id = "h_exj";
        will_user_edit(id, snap_ex("ab", 0, vec![]), EditKind::Insert);
        note_after_edit(id, snap_ex("Xab", 1, vec![]));
        will_user_edit(id, snap_ex("Xab", 1, vec![(2, 2)]), EditKind::Insert);
        note_after_edit(id, snap_ex("XaXb", 2, vec![(3, 3)]));
        assert_eq!(past_len(id), 2);
        let undone = undo(id).expect("undo");
        assert_eq!(undone.value, "Xab");
        assert_eq!(undone.extra_carets, vec![(2, 2)]);
        reset(id);
    }

    #[test]
    fn password_config_records_nothing() {
        reset("h_pw");
        install_field_config("h_pw", TextUndoConfig::password_default());
        will_user_edit("h_pw", snap("", 0), EditKind::Insert);
        note_after_edit("h_pw", snap("secret", 6));
        assert_eq!(past_len("h_pw"), 0);
        assert_eq!(undo("h_pw"), None);
        reset("h_pw");
    }

    #[test]
    fn max_entries_is_clamped_and_evicts_oldest() {
        reset("h_cap");
        install_field_config(
            "h_cap",
            TextUndoConfig {
                enabled: true,
                max_entries: 2,
                max_bytes: 0,
                typing_merge: Duration::ZERO,
                secure_disable: false,
            },
        );
        will_user_edit("h_cap", snap("", 0), EditKind::Insert);
        note_after_edit("h_cap", snap("a", 1));
        will_user_edit("h_cap", snap("a", 1), EditKind::Delete);
        note_after_edit("h_cap", snap("", 0));
        will_user_edit("h_cap", snap("", 0), EditKind::Replace);
        note_after_edit("h_cap", snap("z", 1));
        assert_eq!(past_len("h_cap"), 2);
        assert_eq!(undo("h_cap").unwrap().value, "");
        assert_eq!(undo("h_cap").unwrap().value, "a");
        assert_eq!(undo("h_cap"), None);
        reset("h_cap");
    }

    #[test]
    fn max_bytes_evicts_oldest() {
        reset("h_mem");
        install_field_config(
            "h_mem",
            TextUndoConfig {
                enabled: true,
                max_entries: 50,
                max_bytes: 64 * 1024,
                typing_merge: Duration::ZERO,
                secure_disable: false,
            },
        );
        let big = "x".repeat(50_000);
        will_user_edit("h_mem", snap("", 0), EditKind::Insert);
        note_after_edit("h_mem", snap(&big, big.len()));
        will_user_edit("h_mem", snap(&big, big.len()), EditKind::Replace);
        let big2 = "y".repeat(50_000);
        note_after_edit("h_mem", snap(&big2, big2.len()));
        will_user_edit("h_mem", snap(&big2, big2.len()), EditKind::Delete);
        note_after_edit("h_mem", snap("", 0));
        assert!(current_bytes("h_mem") <= 64 * 1024);
        assert!(past_len("h_mem") <= 2);
        reset("h_mem");
    }

    #[test]
    fn internal_sync_does_not_record() {
        reset("h_sync");
        with_internal_sync("h_sync", || {
            will_user_edit("h_sync", snap("", 0), EditKind::Insert);
            note_after_edit("h_sync", snap("a", 1));
        });
        assert_eq!(past_len("h_sync"), 0);
        assert_eq!(undo("h_sync"), None);
        reset("h_sync");
    }

    #[test]
    fn unmount_hook_releases_history() {
        reset("h_drop");
        let hook = UnmountHook::new(|| reset("h_drop"));
        will_user_edit("h_drop", snap("", 0), EditKind::Insert);
        note_after_edit("h_drop", snap("stay", 4));
        assert_eq!(past_len("h_drop"), 1);
        drop(hook);
        assert_eq!(past_len("h_drop"), 0);
        assert_eq!(undo("h_drop"), None);
        reset("h_drop");
    }

    #[test]
    fn undo_field_restores_value_and_caret() {
        reset("h6");
        let (_owner, ()) = create_root(|| {
            let id = "h6";
            use_signal(&input_value_signal_key(id), String::new).set("ab".into());
            use_signal(
                &crate::utils::text_field::input_caret_signal_key(id),
                || None::<usize>,
            )
            .set(Some(2));
            will_user_edit_field(id, EditKind::Insert);
            use_signal(&input_value_signal_key(id), String::new).set("abc".into());
            use_signal(
                &crate::utils::text_field::input_caret_signal_key(id),
                || None::<usize>,
            )
            .set(Some(3));
            note_field_after_edit(id);
            assert!(undo_field(id));
            assert_eq!(
                use_signal(&input_value_signal_key(id), String::new).peek(),
                "ab"
            );
            assert!(redo_field(id));
            assert_eq!(
                use_signal(&input_value_signal_key(id), String::new).peek(),
                "abc"
            );
            reset(id);
        });
    }

    #[test]
    fn undo_field_restores_extra_carets() {
        reset("h_exf");
        let (_owner, ()) = create_root(|| {
            let id = "h_exf";
            use_signal(&input_value_signal_key(id), String::new).set("ab".into());
            use_signal(
                &crate::utils::text_field::input_caret_signal_key(id),
                || None::<usize>,
            )
            .set(Some(0));
            crate::utils::text_field::extra_carets_signal(id).set(vec![(1, 1)]);
            will_user_edit_field(id, EditKind::Insert);
            use_signal(&input_value_signal_key(id), String::new).set("XaXb".into());
            use_signal(
                &crate::utils::text_field::input_caret_signal_key(id),
                || None::<usize>,
            )
            .set(Some(1));
            crate::utils::text_field::extra_carets_signal(id).set(vec![(3, 3)]);
            note_field_after_edit(id);
            assert!(undo_field(id));
            assert_eq!(
                use_signal(&input_value_signal_key(id), String::new).peek(),
                "ab"
            );
            assert_eq!(
                use_signal(
                    &crate::utils::text_field::input_caret_signal_key(id),
                    || None::<usize>,
                )
                .peek(),
                Some(0)
            );
            assert_eq!(
                crate::utils::text_field::extra_carets_signal(id).peek(),
                vec![(1, 1)]
            );
            assert!(redo_field(id));
            assert_eq!(
                crate::utils::text_field::extra_carets_signal(id).peek(),
                vec![(3, 3)]
            );
            reset(id);
        });
    }

    #[test]
    fn clear_history_policy_drops_stack() {
        reset("h_pol");
        will_user_edit("h_pol", snap("", 0), EditKind::Insert);
        note_after_edit("h_pol", snap("a", 1));
        apply_set_text_policy("h_pol", SetTextUndoPolicy::ClearHistory, snap("a", 1));
        assert_eq!(undo("h_pol"), None);
        reset("h_pol");
    }

    #[test]
    fn commit_value_policy_records_only_when_undoable() {
        reset("h_cv");
        let (_owner, ()) = create_root(|| {
            let id = "h_cv";
            use_signal(&input_value_signal_key(id), String::new).set("a".into());
            crate::utils::text_field::commit_value(id, "b".into());
            assert_eq!(past_len(id), 0);
            crate::utils::text_field::commit_value_with_policy(
                id,
                "c".into(),
                SetTextUndoPolicy::Undoable,
            );
            assert_eq!(past_len(id), 1);
            assert_eq!(undo(id).unwrap().value, "b");
            crate::utils::text_field::commit_value_with_policy(
                id,
                "reset".into(),
                SetTextUndoPolicy::ClearHistory,
            );
            assert_eq!(undo(id), None);
            reset(id);
        });
    }

    #[test]
    fn txn_multi_change_undo_is_reverse_order() {
        reset("h_txn");
        let id = "h_txn";
        {
            let _g = edit_group(id, EditKind::Replace);
            will_user_edit(id, snap("a", 1), EditKind::Replace);
            note_after_edit(id, snap("ab", 2));
            will_user_edit(id, snap("ab", 2), EditKind::Replace);
            note_after_edit(id, snap("abc", 3));
        }
        assert_eq!(past_len(id), 1);
        assert_eq!(last_group_change_count(id), 2);
        assert_eq!(undo(id).unwrap().value, "a");
        assert_eq!(redo(id).unwrap().value, "abc");
        reset(id);
    }

    #[test]
    fn txn_drop_on_panic_starts_new_group() {
        reset("h_panic");
        let id = "h_panic";
        let panicked = catch_unwind(AssertUnwindSafe(|| {
            let _g = edit_group(id, EditKind::Insert);
            will_user_edit(id, snap("", 0), EditKind::Insert);
            note_after_edit(id, snap("a", 1));
            panic!("txn unwind");
        }));
        assert!(panicked.is_err());
        will_user_edit(id, snap("a", 1), EditKind::Insert);
        note_after_edit(id, snap("ab", 2));
        assert_eq!(past_len(id), 2);
        assert_eq!(undo(id).unwrap().value, "a");
        assert_eq!(undo(id).unwrap().value, "");
        reset(id);
    }

    #[test]
    fn internal_sync_flag_clears_on_panic() {
        reset("h_sync_p");
        let id = "h_sync_p";
        let panicked = catch_unwind(AssertUnwindSafe(|| {
            with_internal_sync(id, || panic!("sync unwind"));
        }));
        assert!(panicked.is_err());
        will_user_edit(id, snap("", 0), EditKind::Insert);
        note_after_edit(id, snap("a", 1));
        assert_eq!(past_len(id), 1);
        reset(id);
    }

    #[test]
    fn large_replace_is_marker_and_keeps_earlier_steps() {
        reset("h_mark");
        let id = "h_mark";
        TEST_LARGE_REPLACE_HARD.with(|c| c.set(64));
        will_user_edit(id, snap("", 0), EditKind::Insert);
        note_after_edit(id, snap("hello", 5));
        let huge = "x".repeat(200);
        will_user_edit(id, snap("hello", 5), EditKind::Replace);
        note_after_edit(id, snap(&huge, huge.len()));
        assert_eq!(past_len(id), 2);
        assert!(last_payload_is_marker(id));
        assert_eq!(undo(id).unwrap().value, "hello");
        assert_eq!(undo(id).unwrap().value, "");
        TEST_LARGE_REPLACE_HARD.with(|c| c.set(undo_caps::LARGE_REPLACE_HARD));
        reset(id);
    }

    #[test]
    fn unrestorable_marker_is_a_wall() {
        reset("h_skip");
        let id = "h_skip";
        TEST_LARGE_REPLACE_HARD.with(|c| c.set(64));
        TEST_RETAIN_SIDE_MAX.with(|c| c.set(4));
        will_user_edit(id, snap("", 0), EditKind::Insert);
        note_after_edit(id, snap("hello", 5));
        let huge = "x".repeat(200);
        will_user_edit(id, snap("hello", 5), EditKind::Replace);
        note_after_edit(id, snap(&huge, huge.len()));
        assert!(last_payload_is_marker(id));
        assert_eq!(past_len(id), 2);
        assert!(
            !can_undo(id),
            "unrestorable Marker stays on top; Z must not apply earlier deltas to the huge buffer"
        );
        assert_eq!(undo(id), None);
        assert_eq!(past_len(id), 2, "wall does not pop earlier groups");
        TEST_RETAIN_SIDE_MAX.with(|c| c.set(undo_caps::RETAIN_SIDE_MAX));
        TEST_LARGE_REPLACE_HARD.with(|c| c.set(undo_caps::LARGE_REPLACE_HARD));
        reset(id);
    }
}
