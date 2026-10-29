//! Helpers for crate-local scenario tests under `src/crate_tests/`.

use super::state::AppState;
#[cfg(any(feature = "product-tests", feature = "devtools"))]
use crate::core::tree::LayoutTree;
use crate::core::tree::{Element, LayoutStatus};
#[cfg(feature = "product-tests")]
use std::collections::HashMap;

#[cfg(feature = "gallery-tests")]
pub const FILTER_DEMO_LOGO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/firefox-logo.svg");

pub fn reconcile_elements(new_el: &mut Element, old_el: &mut Element) -> LayoutStatus {
    AppState::reconcile_elements_for_test(new_el, old_el)
}

/// Production bind lookup (WP-N.5). `false` means miss and increments [`super::reactive::bind_miss_count`].
#[cfg(all(test, feature = "product-tests"))]
pub fn resolve_bind_template_id(root: &Element, template_id: &str) -> bool {
    let mut paths = HashMap::new();
    AppState::resolve_template_path(root, &mut paths, template_id).is_some()
}

pub fn is_bootstrap_root(el: &Element) -> bool {
    AppState::is_bootstrap_root(el)
}

pub const BOOTSTRAP_ROOT_ATTR: &str = AppState::BOOTSTRAP_ROOT_ATTR;

#[cfg(any(feature = "product-tests", feature = "devtools"))]
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
