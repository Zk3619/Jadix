//! Fine-grained reactive UI: stable `template_id` anchor + `create_effect` + `reconcile_subtree`.
//!
//! **Ergonomic entry:** use the crate-root `reactive!` macro (e.g. `use rui::reactive;` then
//! `reactive!(id, || ui! { ... })`), which calls [`reactive_anchor`] + [`reactive_placeholder`]
//! together.
//!
//! **Manual:** 1) put [`reactive_placeholder`] (same `template_id`) in your `ui!` tree; 2) call
//! [`reactive_anchor`] with that id and a closure that returns the subtree (`ui! { ... }`).
//!
//! The closure’s `signal.get()` subscriptions attach to **this** effect, not the root `app_builder`
//! effect, so updates are localized after `reconcile_subtree`.
//!
//! **Multi-instance:** use a unique `anchor_id` per instance (e.g. `format!("my_block:{id}")`).
//! **Idempotency:** `reactive_anchor` registers the effect only on the first call with a given id
//! (later calls from repeated `app_builder` runs are ignored).

use std::cell::RefCell;
use std::collections::HashSet;

use crate::core::reactivity::create_effect;
use crate::core::tree::Element;

use super::runtime;

thread_local! {
    static REGISTERED_ANCHORS: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

pub(crate) const REACTIVE_PLACEHOLDER_HASH: &str = "__rui_reactive_placeholder_v1";

/// Reset in-memory reactive anchor registrations (used by app lifecycle re-init).
pub fn reset_registered_anchors() {
    REGISTERED_ANCHORS.with(|reg| reg.borrow_mut().clear());
}

/// Empty subtree root used only to reserve a stable [`Element::template_id`] in the declarative tree.
/// Must use the **same** string as [`reactive_anchor`].
pub fn reactive_placeholder(anchor_id: impl Into<String>) -> Element {
    Element::new()
        .template_id(anchor_id)
        .template_hash(REACTIVE_PLACEHOLDER_HASH)
        .w("100%")
}

/// Register a **single** effect (first call per `anchor_id`) that rebuilds `fragment` and runs
/// [`runtime::try_reconcile_subtree`], then [`runtime::request_redraw`] when reconciliation runs.
pub fn reactive_anchor<F>(anchor_id: impl Into<String>, mut f: F)
where
    F: FnMut() -> Element + 'static,
{
    // Only register from tracked effect execution (root/parent effect run). This avoids
    // registering during the non-tracked pre-runtime initial tree build.
    if !crate::core::reactivity::is_tracking() {
        return;
    }

    let id = anchor_id.into();
    let register = REGISTERED_ANCHORS.with(|reg| {
        let mut reg = reg.borrow_mut();
        if reg.contains(&id) {
            false
        } else {
            reg.insert(id.clone());
            true
        }
    });
    if !register {
        return;
    }

    create_effect(move || {
        let mut fragment = f();
        fragment.template_id = Some(id.clone());
        if runtime::try_reconcile_subtree(&id, fragment).is_some() {
            runtime::request_redraw();
        }
    });
}

/// Conditional rendering helper for fine-grained blocks.
pub fn show<F>(when: bool, render: F) -> Element
where
    F: FnOnce() -> Element,
{
    if when {
        render()
    } else {
        Element::new()
    }
}

/// Like [`show`], but wraps result in a stable anchor element so branch toggles do not
/// perturb sibling identity/order during reconciliation.
pub fn show_scoped<F>(anchor_id: impl Into<String>, when: bool, render: F) -> Element
where
    F: FnOnce() -> Element,
{
    let mut root = Element::new().template_id(anchor_id);
    if when {
        root.children.push(render());
    }
    root
}

/// Keyed list helper: each rendered child gets a stable `template_id` of
/// `{scope_id}:{key}`, enabling keyed child reconciliation in `AppState`.
pub fn for_keyed<T, K, KF, RF>(
    scope_id: impl Into<String>,
    items: &[T],
    mut key_of: KF,
    mut render: RF,
) -> Vec<Element>
where
    K: ToString,
    KF: FnMut(&T) -> K,
    RF: FnMut(&T) -> Element,
{
    let scope_id = scope_id.into();
    items
        .iter()
        .map(|item| {
            let mut el = render(item);
            el.template_id = Some(format!("{scope_id}:{}", key_of(item).to_string()));
            el
        })
        .collect()
}

/// List helper with index-based stable keys (`{scope_id}:{index}`).
/// Useful as a lower-boilerplate `<For each={...}>` fallback when explicit keys are omitted.
pub fn for_each<T, RF>(scope_id: impl Into<String>, items: &[T], mut render: RF) -> Vec<Element>
where
    RF: FnMut(&T) -> Element,
{
    let scope_id = scope_id.into();
    items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let mut el = render(item);
            el.template_id = Some(format!("{scope_id}:{idx}"));
            el
        })
        .collect()
}

/// Iterator variant for keyed list rendering. Allows `<For each={1..=8} ...>` without `.collect()`.
pub fn for_keyed_iter<I, T, K, KF, RF>(
    scope_id: impl Into<String>,
    items: I,
    key_of: KF,
    render: RF,
) -> Vec<Element>
where
    I: IntoIterator<Item = T>,
    K: ToString,
    KF: FnMut(&T) -> K,
    RF: FnMut(&T) -> Element,
{
    let items_vec: Vec<T> = items.into_iter().collect();
    for_keyed(scope_id, &items_vec, key_of, render)
}

/// Iterator variant for index-keyed list rendering.
pub fn for_each_iter<I, T, RF>(scope_id: impl Into<String>, items: I, render: RF) -> Vec<Element>
where
    I: IntoIterator<Item = T>,
    RF: FnMut(&T) -> Element,
{
    let items_vec: Vec<T> = items.into_iter().collect();
    for_each(scope_id, &items_vec, render)
}
