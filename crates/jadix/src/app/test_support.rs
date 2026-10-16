//! Helpers for crate-local scenario tests under `src/crate_tests/`.

use super::state::AppState;
use crate::core::tree::{Element, LayoutStatus, LayoutTree};
use std::collections::HashMap;

pub fn reconcile_elements(new_el: &mut Element, old_el: &mut Element) -> LayoutStatus {
    AppState::reconcile_elements_for_test(new_el, old_el)
}

/// Production bind lookup (WP-N.5). `false` means miss and increments [`super::reactive::bind_miss_count`].
pub fn resolve_bind_template_id(root: &Element, template_id: &str) -> bool {
    let mut paths = HashMap::new();
    AppState::resolve_template_path(root, &mut paths, template_id).is_some()
}

pub fn is_bootstrap_root(el: &Element) -> bool {
    AppState::is_bootstrap_root(el)
}

pub const BOOTSTRAP_ROOT_ATTR: &str = AppState::BOOTSTRAP_ROOT_ATTR;

pub fn sync_reactive_elements_subtree(
    element: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    crate::core::tree::sync_reactive_elements_subtree(
        element,
        layout_tree,
        impact,
        10_000.0,
        10_000.0,
        [0.0, 0.0],
    );
}
