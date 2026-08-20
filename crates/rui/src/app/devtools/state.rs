//! Inspector chrome state. Uses `Cell` / `RefCell`, **not** Signals.
//!
//! Signal `set` flushes render effects, which call `try_reconcile_subtree` while
//! input/render may already hold `AppState`. Cells + `request_redraw` avoid that.

use std::cell::{Cell, RefCell};

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
    /// Stable identity for the hover anchor (`template_id`, else `focus_id`).
    static HOVER_IDENTITY: RefCell<String> = const { RefCell::new(String::new()) };
    /// Child indices from the identity anchor down to the hovered node (empty = anchor itself).
    static HOVER_REL: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    /// Snapshot border-box size for the active overlay target (`0` = unset).
    static OVERLAY_SIZE_HINT: Cell<(f32, f32)> = const { Cell::new((0.0, 0.0)) };
    static PANEL_W: Cell<f32> = const { Cell::new(380.0) };
    static GEN: Cell<u32> = const { Cell::new(0) };
    static LAST_SYNC_GEN: Cell<u32> = const { Cell::new(u32::MAX) };
    static HL_LAYER: Cell<u8> = const { Cell::new(0) };
}

/// 0 = Elements, 1 = Reactivity, 2 = Layout
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DevToolsTab {
    Elements = 0,
    Reactivity = 1,
    Layout = 2,
}

impl DevToolsTab {
    pub fn from_index(i: usize) -> Self {
        match i {
            1 => Self::Reactivity,
            2 => Self::Layout,
            _ => Self::Elements,
        }
    }
}

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
        GEN.set(GEN.get().wrapping_add(1));
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
        OPEN.set(!OPEN.get());
        Self::bump();
    }

    pub fn set_open(open: bool) {
        if OPEN.get() != open {
            OPEN.set(open);
            Self::bump();
        }
    }

    pub fn is_pick() -> bool {
        PICK.get()
    }

    pub fn toggle_pick() {
        PICK.set(!PICK.get());
        Self::bump();
    }

    pub fn set_pick(on: bool) {
        if PICK.get() != on {
            PICK.set(on);
            if !on {
                // Drop transient pick hover; keep overlay on the committed selection.
                Self::sync_overlay_to_selection();
            }
            Self::bump();
        }
    }

    pub fn selected_path() -> ElementPath {
        PATH.with(|p| key_to_path(&p.borrow()))
    }

    /// Raw selection key (`""` = none, [`super::snapshot::ROOT_PATH_KEY`] = tree root).
    /// Prefer this over `path_to_key(&selected_path())`, which cannot distinguish none vs root.
    pub fn selected_path_key() -> String {
        PATH.with(|p| p.borrow().clone())
    }

    pub fn selected_identity() -> String {
        SEL_IDENTITY.with(|id| id.borrow().clone())
    }

    pub fn selected_relative_path() -> ElementPath {
        SEL_REL.with(|r| r.borrow().clone())
    }

    /// Whether tree / pick hover currently overrides the selection overlay.
    pub fn has_hover_highlight() -> bool {
        HOVER_PATH.with(|h| !h.borrow().is_empty())
            || HOVER_IDENTITY.with(|id| !id.borrow().is_empty())
    }

    /// Page overlay target: transient hover if set, otherwise the selected node.
    ///
    /// Styles follows selection; the blue page box must not stick to a stale pick/tree
    /// hover while the Elements row shows a different selected path.
    pub fn overlay_path() -> ElementPath {
        if Self::has_hover_highlight() {
            HOVER_PATH.with(|h| key_to_path(&h.borrow()))
        } else {
            Self::selected_path()
        }
    }

    pub fn overlay_path_key() -> String {
        if Self::has_hover_highlight() {
            Self::hover_path_key()
        } else {
            Self::selected_path_key()
        }
    }

    pub fn overlay_identity() -> String {
        if Self::has_hover_highlight() {
            Self::hover_identity()
        } else {
            Self::selected_identity()
        }
    }

    pub fn overlay_relative_path() -> ElementPath {
        if Self::has_hover_highlight() {
            Self::hover_relative_path()
        } else {
            Self::selected_relative_path()
        }
    }

    /// Snapshot border-box `[width, height]` for the current overlay target (`0` = unset).
    pub fn overlay_size_hint() -> (f32, f32) {
        if Self::has_hover_highlight() {
            OVERLAY_SIZE_HINT.get()
        } else {
            SEL_SIZE.get()
        }
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

    /// Align transient hover with the current selection (keeps identity + size hint).
    pub fn sync_overlay_to_selection() {
        let key = Self::selected_path_key();
        if key.is_empty() {
            Self::clear_hover_highlight();
            return;
        }
        Self::set_hover_highlight(
            Self::selected_path(),
            Self::selected_identity(),
            Self::selected_relative_path(),
            SEL_SIZE.get(),
        );
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
            crate::app::runtime::request_redraw();
        }
    }

    pub fn clear_hover_highlight() {
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
            crate::app::runtime::request_redraw();
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
                crate::app::runtime::request_redraw();
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
        let identity = identity.into();
        let size_hint = (size_hint.0.max(0.0), size_hint.1.max(0.0));
        let mut changed = false;
        PATH.with(|p| {
            if *p.borrow() != key {
                *p.borrow_mut() = key.clone();
                HL_LAYER.set(0);
                DevToolsState::bump();
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
        // Keep page overlay in lockstep with Styles (selection), not a stale pick hover.
        Self::set_hover_highlight(path, identity, relative, size_hint);
        if changed {
            crate::app::runtime::request_redraw();
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
        let w = w.clamp(280.0, 720.0);
        if (PANEL_W.get() - w).abs() > 0.5 {
            PANEL_W.set(w);
            Self::bump();
        }
    }

    pub fn highlight_layer() -> HighlightLayer {
        HighlightLayer::from_u8(HL_LAYER.get())
    }

    pub fn set_highlight_layer(layer: HighlightLayer) {
        let v = layer as u8;
        if HL_LAYER.get() != v {
            HL_LAYER.set(v);
            crate::app::runtime::request_redraw();
        }
    }
}
