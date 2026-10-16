//! Global stylesheet registry (registration order + priority).

use std::cell::RefCell;

use super::ir::Stylesheet;
use crate::style::{install_theme_overrides, ThemeOverrideError};

thread_local! {
    static REGISTRY: RefCell<Vec<Stylesheet>> = const { RefCell::new(Vec::new()) };
}

/// Store `sheet` and, when present, install its theme patch (WP-STY.0).
pub fn register_stylesheet(sheet: Stylesheet) -> Result<(), ThemeOverrideError> {
    if let Some(ref theme) = sheet.theme {
        install_theme_overrides(theme.clone())?;
    }
    REGISTRY.with(|r| r.borrow_mut().push(sheet));
    super::watch::mark_style_hot_rebuild();
    Ok(())
}

pub fn clear_stylesheets() {
    REGISTRY.with(|r| r.borrow_mut().clear());
    super::watch::mark_style_hot_rebuild();
}

pub(super) fn snapshot_stylesheets() -> Vec<Stylesheet> {
    REGISTRY.with(|r| r.borrow().clone())
}

pub(super) fn restore_stylesheets(sheets: Vec<Stylesheet>) {
    REGISTRY.with(|r| *r.borrow_mut() = sheets);
    super::watch::mark_style_hot_rebuild();
}

pub fn stylesheet_count() -> usize {
    REGISTRY.with(|r| r.borrow().len())
}

pub(super) fn with_registry<R>(f: impl FnOnce(&[Stylesheet]) -> R) -> R {
    REGISTRY.with(|r| f(&r.borrow()))
}

pub(super) fn registry_is_empty() -> bool {
    REGISTRY.with(|r| r.borrow().is_empty())
}
