//! Inspector chrome state. Uses `Cell` / `RefCell`, **not** Signals.
//!
//! Signal `set` flushes render effects, which call `try_reconcile_subtree` while
//! input/render may already hold `AppState`. Cells + `request_redraw` avoid that.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};

use super::snapshot::{key_to_path, path_to_key, ElementPath};

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static OPEN: Cell<bool> = const { Cell::new(false) };
    static PICK: Cell<bool> = const { Cell::new(false) };
    static TAB: Cell<usize> = const { Cell::new(0) };
    static PATH: RefCell<String> = const { RefCell::new(String::new()) };
    /// Stable identity for the selected node (same meaning as [`HOVER_IDENTITY`]).
    static SEL_IDENTITY: RefCell<String> = const { RefCell::new(String::new()) };
    /// Relative path under [`SEL_IDENTITY`] for the selected node.
    static SEL_REL: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    /// Snapshot border-box for the selected node.
    static SEL_SIZE: Cell<(f32, f32)> = const { Cell::new((0.0, 0.0)) };
    /// Tree / pick hover drives the page overlay (child-index path; may drift after rebuilds).
    static HOVER_PATH: RefCell<String> = const { RefCell::new(String::new()) };
    static HIGHLIGHT_UNTIL: Cell<Option<std::time::Instant>> = const { Cell::new(None) };
    /// Stable identity for the hover anchor (`template_id`, else `focus_id`).
    static HOVER_IDENTITY: RefCell<String> = const { RefCell::new(String::new()) };
    /// Child indices from the identity anchor down to the hovered node (empty = anchor itself).
    static HOVER_REL: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    /// Snapshot border-box size for the active overlay target (`0` = unset).
    static OVERLAY_SIZE_HINT: Cell<(f32, f32)> = const { Cell::new((0.0, 0.0)) };
    static PANEL_W: Cell<f32> = const { Cell::new(640.0) };
    static EXPANDED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    static OUTLINE_INSTANCES: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
    static TREE_REVEAL: RefCell<Option<String>> = const { RefCell::new(None) };
    static TREE_FILTER: RefCell<String> = const { RefCell::new(String::new()) };
    static COMPUTED: Cell<bool> = const { Cell::new(false) };
    static CHANGES: RefCell<VecDeque<(String, String)>> = const { RefCell::new(VecDeque::new()) };
    static BODY_GEN: Cell<u32> = const { Cell::new(0) };
    static OUTLINE_GEN: Cell<u32> = const { Cell::new(0) };
    static DETAILS_GEN: Cell<u32> = const { Cell::new(0) };
    static GEN: Cell<u32> = const { Cell::new(0) };
    static LAST_SYNC_GEN: Cell<u32> = const { Cell::new(u32::MAX) };
    static HL_LAYER: Cell<u8> = const { Cell::new(0) };
    static HL_HOVER_LAYER: Cell<Option<u8>> = const { Cell::new(None) };
}

/// 0 = Elements, 1 = Reactivity, 2 = Layout
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DevToolsTab {
    Elements = 0,
    Reactivity = 1,
    Layout = 2,
    Sources = 3,
    Changes = 4,
}

impl DevToolsTab {
    pub fn from_index(i: usize) -> Self {
        match i {
            1 => Self::Reactivity,
            2 => Self::Layout,
            3 => Self::Sources,
            4 => Self::Changes,
            _ => Self::Elements,
        }
    }
}

#[cfg(test)]
#[path = "tests/identity.rs"]
mod identity_tests;

/// Which box-model ring is emphasized on the page overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HighlightLayer {
    All = 0,
    Margin = 1,
    Border = 2,
    Padding = 3,
    Content = 4,
    Position = 5,
}

impl HighlightLayer {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Margin,
            2 => Self::Border,
            3 => Self::Padding,
            4 => Self::Content,
            5 => Self::Position,
            _ => Self::All,
        }
    }
}

pub struct DevToolsState;

impl DevToolsState {
    pub fn enable_for_window() {
        ENABLED.set(true);
    }

    pub fn is_enabled() -> bool {
        ENABLED.get()
    }

    pub fn bump() {
        BODY_GEN.set(BODY_GEN.get().wrapping_add(1));
        OUTLINE_GEN.set(OUTLINE_GEN.get().wrapping_add(1));
        DETAILS_GEN.set(DETAILS_GEN.get().wrapping_add(1));
        Self::bump_chrome();
    }

    pub(super) fn revisions() -> [u32; 3] {
        [BODY_GEN.get(), OUTLINE_GEN.get(), DETAILS_GEN.get()]
    }

    pub(super) fn refresh_outline() {
        OUTLINE_GEN.set(OUTLINE_GEN.get().wrapping_add(1));
        Self::bump_chrome();
    }

    fn refresh_details() {
        DETAILS_GEN.set(DETAILS_GEN.get().wrapping_add(1));
        Self::bump_chrome();
    }

    pub(super) fn bump_chrome() {
        GEN.set(GEN.get().wrapping_add(1));
        crate::app::runtime::request_redraw_all();
    }

    pub fn generation() -> u32 {
        GEN.get()
    }

    pub fn last_sync_gen() -> u32 {
        LAST_SYNC_GEN.get()
    }

    pub fn mark_synced(gen: u32) {
        LAST_SYNC_GEN.set(gen);
    }

    pub fn is_open() -> bool {
        OPEN.get()
    }

    pub fn toggle_open() {
        Self::set_open(!OPEN.get());
    }

    pub fn set_open(open: bool) {
        if OPEN.get() != open {
            OPEN.set(open);
            if !open {
                PICK.set(false);
                super::device::disable();
                Self::clear_hover_highlight();
            }
            Self::bump();
        }
    }

    pub fn is_pick() -> bool {
        PICK.get()
    }

    pub fn toggle_pick() {
        PICK.set(!PICK.get());
        Self::bump_chrome();
    }

    pub fn set_pick(on: bool) {
        if PICK.get() != on {
            PICK.set(on);
            if !on {
                Self::clear_hover_highlight();
            }
            Self::bump_chrome();
        }
    }

    pub(crate) fn record_commit(acknowledgement: &jadix_hot_reload::CommitAck) {
        if acknowledgement.committed() {
            let identity = Self::selected_identity();
            if let Some(instance) = identity
                .strip_prefix("runtime:")
                .and_then(|value| value.parse::<u64>().ok())
            {
                if let Some(change) = acknowledgement
                    .changes
                    .iter()
                    .find(|change| change.instance == instance)
                {
                    PATH.with(|path| {
                        *path.borrow_mut() = change
                            .after
                            .as_ref()
                            .map(|path| path_to_key(path))
                            .unwrap_or_default()
                    });
                }
            }
        }
        Self::record_change(
            "HMR commit",
            serde_json::to_string(acknowledgement)
                .unwrap_or_else(|_| acknowledgement.reason.clone()),
        );
    }

    pub(crate) fn resolve_selection(root: &crate::core::tree::Element) {
        let instances = crate::core::hot_reload::instances(root);
        let current: HashMap<_, _> = instances
            .iter()
            .map(|(instance, path)| (path_to_key(path), format!("runtime:{instance}")))
            .collect();
        let identities: HashSet<_> = current.values().cloned().collect();
        let previous = OUTLINE_INSTANCES.with(|outline| outline.replace(current.clone()));
        EXPANDED.with(|expanded| {
            let mut expanded = expanded.borrow_mut();
            let retained: HashSet<_> = expanded
                .drain()
                .filter_map(|key| {
                    let identity = if key.starts_with("runtime:") {
                        Some(key)
                    } else {
                        previous
                            .get(&key)
                            .or_else(|| {
                                if previous.is_empty() {
                                    current.get(&key)
                                } else {
                                    None
                                }
                            })
                            .cloned()
                    }?;
                    identities.contains(&identity).then_some(identity)
                })
                .collect();
            *expanded = retained;
        });
        let resolve = |identity: &str| {
            current
                .iter()
                .find_map(|(path, candidate)| (candidate == identity).then(|| path.clone()))
                .unwrap_or_default()
        };
        let identity = Self::selected_identity();
        if identity.starts_with("runtime:") {
            PATH.with(|path| *path.borrow_mut() = resolve(&identity));
        }
        let hover = Self::hover_identity();
        if hover.starts_with("runtime:") {
            let path = resolve(&hover);
            if path.is_empty() {
                Self::clear_hover_highlight();
            } else {
                HOVER_PATH.with(|current| *current.borrow_mut() = path);
            }
        }
    }

    fn outline_identity(path: &[usize]) -> String {
        let key = path_to_key(path);
        OUTLINE_INSTANCES
            .with(|instances| instances.borrow().get(&key).cloned())
            .unwrap_or(key)
    }

    pub fn selected_path() -> ElementPath {
        PATH.with(|p| key_to_path(&p.borrow()))
    }

    /// Raw selection key (`""` = none, [`super::snapshot::ROOT_PATH_KEY`] = tree root).
    /// Prefer this over `path_to_key(&selected_path())`, which cannot distinguish none vs root.
    pub fn selected_path_key() -> String {
        PATH.with(|p| p.borrow().clone())
    }

    pub(super) fn is_selected_path_key(key: &str) -> bool {
        PATH.with(|path| !key.is_empty() && path.borrow().as_str() == key)
    }

    pub(super) fn is_hover_path_key(key: &str) -> bool {
        HOVER_PATH.with(|path| !key.is_empty() && path.borrow().as_str() == key)
    }

    pub fn selected_identity() -> String {
        SEL_IDENTITY.with(|id| id.borrow().clone())
    }

    pub fn selected_relative_path() -> ElementPath {
        SEL_REL.with(|r| r.borrow().clone())
    }

    /// Whether a tree row or pick target currently owns the page overlay.
    pub fn has_hover_highlight() -> bool {
        HOVER_PATH.with(|h| !h.borrow().is_empty())
            || HOVER_IDENTITY.with(|id| !id.borrow().is_empty())
    }

    /// Page overlay target owned by a tree-row or pick hover.
    pub fn overlay_path() -> ElementPath {
        HOVER_PATH.with(|h| key_to_path(&h.borrow()))
    }

    pub fn overlay_path_key() -> String {
        Self::hover_path_key()
    }

    pub fn overlay_identity() -> String {
        Self::hover_identity()
    }

    pub fn overlay_relative_path() -> ElementPath {
        Self::hover_relative_path()
    }

    /// Snapshot border-box size for the current overlay target (zero when unset).
    pub fn overlay_size_hint() -> (f32, f32) {
        OVERLAY_SIZE_HINT.get()
    }

    /// Child-index path for the page overlay (may be stale; prefer [`Self::overlay_identity`]).
    pub fn highlight_path() -> ElementPath {
        Self::overlay_path()
    }

    /// `template_id` / `focus_id` of the hover anchor (empty if anonymous).
    pub fn hover_identity() -> String {
        HOVER_IDENTITY.with(|id| id.borrow().clone())
    }

    /// Path from the identity anchor to the hovered descendant (empty = highlight the anchor).
    pub fn hover_relative_path() -> ElementPath {
        HOVER_REL.with(|r| r.borrow().clone())
    }

    /// Clear the page overlay when no pointer-owned inspector target remains.
    pub fn sync_overlay_to_selection() {
        Self::clear_hover_highlight();
    }

    pub fn set_hover_highlight_path(path: ElementPath) {
        // Preserve selection identity when Styles rings re-drive the overlay by path only.
        if path_to_key(&path) == Self::selected_path_key() {
            Self::set_hover_highlight(
                path,
                Self::selected_identity(),
                Self::selected_relative_path(),
                SEL_SIZE.get(),
            );
        } else {
            Self::set_hover_highlight(path, String::new(), Vec::new(), (0.0, 0.0));
        }
    }

    /// Set page-overlay hover.
    ///
    /// - `identity`: nearest stable `template_id` / `focus_id` (may be an ancestor).
    /// - `relative`: child indices from that anchor to the target (empty = the anchor).
    /// - `path`: absolute child-index path (fallback only).
    /// - `size_hint`: snapshot border-box; rejects drifted path hits (e.g. Outline vs card).
    pub fn set_hover_highlight(
        path: ElementPath,
        identity: impl Into<String>,
        relative: ElementPath,
        size_hint: (f32, f32),
    ) {
        HIGHLIGHT_UNTIL.set(None);
        let key = path_to_key(&path);
        let identity = identity.into();
        let size_hint = (size_hint.0.max(0.0), size_hint.1.max(0.0));
        let mut changed = false;
        HOVER_PATH.with(|h| {
            if *h.borrow() != key {
                *h.borrow_mut() = key;
                changed = true;
            }
        });
        HOVER_IDENTITY.with(|id| {
            if *id.borrow() != identity {
                *id.borrow_mut() = identity;
                changed = true;
            }
        });
        HOVER_REL.with(|r| {
            if *r.borrow() != relative {
                *r.borrow_mut() = relative;
                changed = true;
            }
        });
        if OVERLAY_SIZE_HINT.get() != size_hint {
            OVERLAY_SIZE_HINT.set(size_hint);
            changed = true;
        }
        if changed {
            crate::app::runtime::request_redraw_all();
        }
    }

    pub(crate) fn expire_pick_highlight(now: std::time::Instant) -> bool {
        if HIGHLIGHT_UNTIL.get().is_some_and(|until| now >= until) {
            Self::clear_hover_highlight();
            true
        } else {
            false
        }
    }

    pub(crate) fn pick_highlight_deadline() -> Option<std::time::Instant> {
        HIGHLIGHT_UNTIL.get()
    }

    pub(super) fn defer_pick_highlight_clear() {
        HIGHLIGHT_UNTIL.set(Some(
            std::time::Instant::now() + std::time::Duration::from_millis(1200),
        ));
    }

    pub fn clear_hover_highlight() {
        HIGHLIGHT_UNTIL.set(None);
        let mut changed = false;
        HOVER_PATH.with(|h| {
            if !h.borrow().is_empty() {
                h.borrow_mut().clear();
                changed = true;
            }
        });
        HOVER_IDENTITY.with(|id| {
            if !id.borrow().is_empty() {
                id.borrow_mut().clear();
                changed = true;
            }
        });
        HOVER_REL.with(|r| {
            if !r.borrow().is_empty() {
                r.borrow_mut().clear();
                changed = true;
            }
        });
        if OVERLAY_SIZE_HINT.get() != (0.0, 0.0) {
            OVERLAY_SIZE_HINT.set((0.0, 0.0));
            changed = true;
        }
        if changed {
            crate::app::runtime::request_redraw_all();
        }
    }

    /// Clear overlay hover only when leaving the row that currently owns it.
    pub fn clear_hover_highlight_if_path(path: &ElementPath) {
        let key = path_to_key(path);
        HOVER_PATH.with(|h| {
            if *h.borrow() == key {
                h.borrow_mut().clear();
                HOVER_IDENTITY.with(|id| id.borrow_mut().clear());
                HOVER_REL.with(|r| r.borrow_mut().clear());
                OVERLAY_SIZE_HINT.set((0.0, 0.0));
                crate::app::runtime::request_redraw_all();
            }
        });
    }

    pub fn hover_path_key() -> String {
        HOVER_PATH.with(|h| h.borrow().clone())
    }

    /// Select a node and pin the page overlay to it (with stable identity).
    pub fn set_selected_path(path: ElementPath) {
        Self::set_selected(path, String::new(), Vec::new(), (0.0, 0.0));
    }

    /// Select a node. `identity` / `relative` / `size_hint` match [`Self::set_hover_highlight`].
    pub fn set_selected(
        path: ElementPath,
        identity: impl Into<String>,
        relative: ElementPath,
        size_hint: (f32, f32),
    ) {
        let key = path_to_key(&path);
        if Self::hover_path_key() != key {
            Self::clear_hover_highlight();
        }
        let expanded = EXPANDED.with(|expanded| {
            let mut paths = expanded.borrow_mut();
            let mut expanded = false;
            for depth in 0..path.len() {
                expanded |= paths.insert(Self::outline_identity(&path[..depth]));
            }
            expanded
        });
        if expanded {
            Self::refresh_outline();
        }
        let identity = identity.into();
        let size_hint = (size_hint.0.max(0.0), size_hint.1.max(0.0));
        let mut changed = false;
        PATH.with(|p| {
            if *p.borrow() != key {
                *p.borrow_mut() = key.clone();
                HL_LAYER.set(0);
                Self::refresh_details();
                changed = true;
            }
        });
        SEL_IDENTITY.with(|id| {
            if *id.borrow() != identity {
                *id.borrow_mut() = identity.clone();
                changed = true;
            }
        });
        SEL_REL.with(|r| {
            if *r.borrow() != relative {
                *r.borrow_mut() = relative.clone();
                changed = true;
            }
        });
        if SEL_SIZE.get() != size_hint {
            SEL_SIZE.set(size_hint);
            changed = true;
        }
        if changed {
            crate::app::runtime::request_redraw_all();
        }
    }

    pub fn active_tab() -> DevToolsTab {
        DevToolsTab::from_index(TAB.get())
    }

    pub fn set_tab(tab: DevToolsTab) {
        let i = tab as usize;
        if TAB.get() != i {
            TAB.set(i);
            Self::bump();
        }
    }

    pub fn panel_width() -> f32 {
        PANEL_W.get()
    }

    pub fn set_panel_width(w: f32) {
        let w = if w.is_finite() {
            w.clamp(320.0, 1100.0)
        } else {
            640.0
        };
        if (PANEL_W.get() - w).abs() > 0.5 {
            let reflow = (PANEL_W.get() >= 600.0) != (w >= 600.0);
            PANEL_W.set(w);
            if reflow {
                Self::bump();
            } else {
                Self::bump_chrome();
            }
        }
    }

    pub fn highlight_layer() -> HighlightLayer {
        HighlightLayer::from_u8(HL_HOVER_LAYER.get().unwrap_or_else(|| HL_LAYER.get()))
    }

    pub(crate) fn is_collapsed(path: &[usize]) -> bool {
        EXPANDED.with(|expanded| !expanded.borrow().contains(&Self::outline_identity(path)))
    }

    pub(crate) fn toggle_collapsed(path: &[usize]) {
        EXPANDED.with(|expanded| {
            let mut expanded = expanded.borrow_mut();
            let key = Self::outline_identity(path);
            if !expanded.remove(&key) {
                expanded.insert(key);
            }
        });
        Self::refresh_outline();
    }

    pub(crate) fn tree_filter() -> String {
        TREE_FILTER.with(|filter| filter.borrow().clone())
    }

    pub(crate) fn request_tree_reveal(path: &[usize]) {
        TREE_REVEAL.with(|pending| *pending.borrow_mut() = Some(path_to_key(path)));
        Self::refresh_outline();
    }

    pub(super) fn pending_tree_reveal() -> Option<String> {
        TREE_REVEAL.with(|pending| pending.borrow().clone())
    }

    pub(super) fn finish_tree_reveal() {
        TREE_REVEAL.with(|pending| pending.borrow_mut().take());
    }

    pub(crate) fn set_tree_filter(value: String) {
        TREE_FILTER.with(|filter| *filter.borrow_mut() = value);
        Self::refresh_outline();
    }

    pub(crate) fn show_computed() -> bool {
        COMPUTED.get()
    }

    pub(crate) fn set_computed(value: bool) {
        COMPUTED.set(value);
        Self::refresh_details();
    }

    pub(crate) fn record_change(kind: &str, detail: impl Into<String>) {
        CHANGES.with(|changes| {
            let mut changes = changes.borrow_mut();
            if changes.len() >= 128 {
                changes.pop_front();
            }
            changes.push_back((kind.into(), detail.into()));
        });
        Self::refresh_if_visible();
    }

    pub(crate) fn changes() -> Vec<(String, String)> {
        CHANGES.with(|changes| changes.borrow().iter().rev().cloned().collect())
    }

    pub(crate) fn clear_changes() {
        CHANGES.with(|changes| changes.borrow_mut().clear());
        Self::refresh_if_visible();
    }

    pub(crate) fn refresh() {
        if Self::active_tab() == DevToolsTab::Sources {
            BODY_GEN.set(BODY_GEN.get().wrapping_add(1));
        }
        Self::bump_chrome();
    }

    fn refresh_if_visible() {
        if Self::active_tab() == DevToolsTab::Changes {
            BODY_GEN.set(BODY_GEN.get().wrapping_add(1));
        }
        Self::bump_chrome();
    }

    pub fn set_highlight_layer(layer: HighlightLayer) {
        let value = layer as u8;
        if HL_LAYER.get() != value {
            HL_LAYER.set(value);
            crate::app::runtime::request_redraw_all();
        }
    }

    pub fn set_highlight_hover_layer(layer: Option<HighlightLayer>) {
        let value = layer.map(|layer| layer as u8);
        if HL_HOVER_LAYER.get() != value {
            HL_HOVER_LAYER.set(value);
            crate::app::runtime::request_redraw_all();
        }
    }
}
