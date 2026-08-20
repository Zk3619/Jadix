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
    /// Tree hover drives page overlay; empty falls back to [`PATH`] (click selection).
    static HOVER_PATH: RefCell<String> = const { RefCell::new(String::new()) };
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
                Self::clear_hover_highlight();
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

    /// Path used for the page overlay (tree hover only).
    pub fn highlight_path() -> ElementPath {
        HOVER_PATH.with(|h| key_to_path(&h.borrow()))
    }

    /// Whether tree hover currently targets a node (including the root).
    pub fn has_hover_highlight() -> bool {
        HOVER_PATH.with(|h| !h.borrow().is_empty())
    }

    pub fn set_hover_highlight_path(path: ElementPath) {
        let key = path_to_key(&path);
        HOVER_PATH.with(|h| {
            if *h.borrow() != key {
                *h.borrow_mut() = key;
                crate::app::runtime::request_redraw();
            }
        });
    }

    pub fn clear_hover_highlight() {
        HOVER_PATH.with(|h| {
            if !h.borrow().is_empty() {
                h.borrow_mut().clear();
                crate::app::runtime::request_redraw();
            }
        });
    }

    /// Clear overlay hover only when leaving the row that currently owns it.
    pub fn clear_hover_highlight_if_path(path: &ElementPath) {
        let key = path_to_key(path);
        HOVER_PATH.with(|h| {
            if *h.borrow() == key {
                h.borrow_mut().clear();
                crate::app::runtime::request_redraw();
            }
        });
    }

    pub fn hover_path_key() -> String {
        HOVER_PATH.with(|h| h.borrow().clone())
    }

    pub fn set_selected_path(path: ElementPath) {
        let key = path_to_key(&path);
        let mut changed = false;
        PATH.with(|p| {
            if *p.borrow() != key {
                *p.borrow_mut() = key;
                HL_LAYER.set(0);
                DevToolsState::bump();
                changed = true;
            }
        });
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
