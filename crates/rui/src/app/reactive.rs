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
//! **Root effect hygiene:** any `signal.get()` evaluated while building the root `app_builder`
//! tree (outside a `<Reactive>` / `reactive!` / [`bind_text`] block) subscribes the **entire** app
//! to that signal. Prefer localized boundaries instead of wrapping whole pages:
//!
//! - **Text / style-only state**: use [`bind_text`] / [`bind_with`] (Solid-style single-binding
//!   effects that patch a mounted node in place).
//! - **Style/layout-only component state** (RadioGroup, Slider, Switch, …): attach
//!   [`crate::core::tree::ReactiveSync`] on the component root so the existing tree is updated in
//!   the frame prepass without a subtree rebuild.
//! - **Structural state** (Select menu, Popover panel, NavigatorHost, …): return
//!   [`reactive_view`] or [`reactive_component`] from the component.
//! - **App-level derived labels** can use `<Reactive>` / `reactive!`, or let `ui!` auto-lower
//!   `{signal.get()}` text children to [`bind_text`].
//!
//! **Multi-instance:** use a unique `anchor_id` per instance (e.g. `format!("my_block:{id}")`).
//! **Idempotency:** `reactive_anchor` / bindings register the effect only on the first call with a
//! given id (later calls from repeated `app_builder` runs are ignored). Unmount disposes the
//! effect so the same id can register again on remount.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use crate::core::reactivity::{create_effect, dispose_effect, untrack, use_signal, EffectId};
use crate::core::tree::{Element, LayoutStatus, UnmountHook};

use super::runtime;

thread_local! {
    static REGISTERED_ANCHORS: RefCell<HashMap<String, AnchorRegistration>> =
        RefCell::new(HashMap::new());
    static REGISTERED_BINDINGS: RefCell<HashMap<String, BindingRegistration>> =
        RefCell::new(HashMap::new());
    static COMPONENT_PROPS: RefCell<HashMap<String, Rc<RefCell<Box<dyn Any>>>>> =
        RefCell::new(HashMap::new());
    static COMPONENT_CHILDREN: RefCell<HashMap<String, Option<Element>>> =
        RefCell::new(HashMap::new());
}

pub(crate) const REACTIVE_PLACEHOLDER_HASH: &str = "__rui_reactive_placeholder_v1";

enum AnchorHook {
    /// Registration happened before its placeholder was created.
    Pending(Rc<UnmountHook>),
    /// Mounted placeholder/fragment owns the hook.
    Mounted(Weak<UnmountHook>),
}

impl AnchorHook {
    fn upgrade(&self) -> Option<Rc<UnmountHook>> {
        match self {
            Self::Pending(hook) => Some(Rc::clone(hook)),
            Self::Mounted(hook) => hook.upgrade(),
        }
    }
}

struct AnchorRegistration {
    effect_id: Option<EffectId>,
    hook: AnchorHook,
}

struct BindingRegistration {
    effect_id: Option<EffectId>,
    hook: Rc<UnmountHook>,
}

/// Reset in-memory reactive anchor registrations (used by app lifecycle re-init).
pub fn reset_registered_anchors() {
    let registrations: Vec<AnchorRegistration> = REGISTERED_ANCHORS.with(|registry| {
        registry
            .borrow_mut()
            .drain()
            .map(|(_, registration)| registration)
            .collect()
    });
    for registration in registrations {
        if let Some(hook) = registration.hook.upgrade() {
            hook.disarm();
        }
        if let Some(effect_id) = registration.effect_id {
            dispose_effect(effect_id);
        }
    }

    let bindings: Vec<BindingRegistration> = REGISTERED_BINDINGS.with(|registry| {
        registry
            .borrow_mut()
            .drain()
            .map(|(_, registration)| registration)
            .collect()
    });
    for registration in bindings {
        registration.hook.disarm();
        if let Some(effect_id) = registration.effect_id {
            dispose_effect(effect_id);
        }
    }

    COMPONENT_PROPS.with(|store| store.borrow_mut().clear());
    COMPONENT_CHILDREN.with(|store| store.borrow_mut().clear());
}

/// Stash RSX `children` for a reactive component between `From`/function calls.
pub fn stash_component_children(id: &str, children: Option<Element>) {
    COMPONENT_CHILDREN.with(|store| {
        store.borrow_mut().insert(id.to_string(), children);
    });
}

/// Take stashed children inside a reactive component render pass.
pub fn take_component_children(id: &str) -> Option<Element> {
    COMPONENT_CHILDREN.with(|store| store.borrow_mut().remove(id).flatten())
}

fn mounted_anchor_hook(id: &str) -> Option<Rc<UnmountHook>> {
    REGISTERED_ANCHORS.with(|registry| {
        let registry = registry.borrow();
        let registration = registry.get(id)?;
        registration.hook.upgrade()
    })
}

/// Empty subtree root used only to reserve a stable [`Element::template_id`] in the declarative tree.
/// Must use the **same** string as [`reactive_anchor`].
pub fn reactive_placeholder(anchor_id: impl Into<String>) -> Element {
    let id = anchor_id.into();
    let mut placeholder = Element::new()
        .template_id(id.clone())
        .template_hash(REACTIVE_PLACEHOLDER_HASH)
        .w("100%")
        .h("100%");

    if let Some(hook) = mounted_anchor_hook(&id) {
        placeholder = placeholder.on_unmount_hook(Rc::clone(&hook));
        REGISTERED_ANCHORS.with(|registry| {
            if let Some(registration) = registry.borrow_mut().get_mut(&id) {
                registration.hook = AnchorHook::Mounted(Rc::downgrade(&hook));
            }
        });
    }
    placeholder
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
    let already_registered =
        REGISTERED_ANCHORS.with(|registry| registry.borrow().contains_key(&id));
    if already_registered {
        return;
    }

    let hook_id = id.clone();
    let hook = UnmountHook::new(move || {
        let effect_id = REGISTERED_ANCHORS.with(|registry| {
            registry
                .borrow_mut()
                .remove(&hook_id)
                .and_then(|registration| registration.effect_id)
        });
        if let Some(effect_id) = effect_id {
            dispose_effect(effect_id);
        }
    });
    REGISTERED_ANCHORS.with(|registry| {
        registry.borrow_mut().insert(
            id.clone(),
            AnchorRegistration {
                effect_id: None,
                hook: AnchorHook::Pending(Rc::clone(&hook)),
            },
        );
    });

    let effect_anchor_id = id.clone();
    let effect_id = create_effect(move || {
        let mut fragment = f();
        fragment.template_id = Some(effect_anchor_id.clone());
        if let Some(hook) = mounted_anchor_hook(&effect_anchor_id) {
            fragment = fragment.on_unmount_hook(hook);
        }
        if runtime::try_reconcile_subtree(&effect_anchor_id, fragment).is_some() {
            runtime::request_redraw();
        }
    });
    REGISTERED_ANCHORS.with(|registry| {
        if let Some(registration) = registry.borrow_mut().get_mut(&id) {
            registration.effect_id = Some(effect_id);
        }
    });
}

/// Component/application reusable structural reactive boundary.
///
/// Stateful components can return this directly when signal changes must mount/unmount descendants.
/// For style/layout-only changes prefer [`crate::core::tree::ReactiveSync`], which avoids subtree
/// reconstruction entirely.
pub fn reactive_view<F>(anchor_id: impl Into<String>, render: F) -> Element
where
    F: FnMut() -> Element + 'static,
{
    let id = anchor_id.into();
    reactive_anchor(id.clone(), render);
    reactive_placeholder(id)
}

/// Stateful component boundary with refreshable props.
///
/// Each call updates a shared props cell keyed by `anchor_id`. When props change, a generation
/// signal bumps so the registered effect re-runs with fresh props even though [`reactive_anchor`]
/// is idempotent. The same `anchor_id` must always use the same props type `P`.
pub fn reactive_component<P, F>(anchor_id: impl Into<String>, props: P, mut render: F) -> Element
where
    P: Clone + PartialEq + 'static,
    F: FnMut(P) -> Element + 'static,
{
    let id = anchor_id.into();
    let gen_key = format!("{id}__component_gen");
    let generation = use_signal(gen_key.as_str(), || 0u32);

    let props_cell = COMPONENT_PROPS.with(|store| {
        let mut store = store.borrow_mut();
        if let Some(cell) = store.get(&id) {
            Rc::clone(cell)
        } else {
            let cell = Rc::new(RefCell::new(Box::new(props.clone()) as Box<dyn Any>));
            store.insert(id.clone(), Rc::clone(&cell));
            cell
        }
    });

    let props_changed = {
        let mut boxed = props_cell.borrow_mut();
        let slot = boxed
            .downcast_mut::<P>()
            .expect("reactive_component props type mismatch for anchor id");
        if *slot != props {
            *slot = props;
            true
        } else {
            false
        }
    };
    if props_changed {
        generation.set(generation.peek().wrapping_add(1));
    }

    let gen_for_effect = generation;
    let props_for_effect = props_cell;
    reactive_anchor(id.clone(), move || {
        let _ = gen_for_effect.get();
        let props = props_for_effect
            .borrow()
            .downcast_ref::<P>()
            .expect("reactive_component props type mismatch for anchor id")
            .clone();
        // Field-level: keep tracking on for bind_*/Show/For registration, but do not let
        // naked `signal.get()` in the body subscribe this component effect (only `generation`).
        crate::core::reactivity::isolate_reads(|| render(props))
    });
    reactive_placeholder(id)
}

/// Solid-style text binding: register one effect that patches `text` on a mounted node in place.
///
/// Unlike [`reactive_view`], this does **not** rebuild a subtree. Prefer this for labels/counters.
pub fn bind_text<F>(anchor_id: impl Into<String>, compute: F) -> Element
where
    F: FnMut() -> String + 'static,
{
    let id = anchor_id.into();
    bind_prop(
        Element::new().template_id(id.clone()),
        id,
        compute,
        |el, next| {
            if el.text.as_deref() == Some(next.as_str()) {
                return LayoutStatus::Clean;
            }
            // Counter-style labels usually keep the same intrinsic width. When they do, taffy has
            // nothing to recompute and a paint-only pass is enough.
            let same_width = text_intrinsic_width(el, el.text.as_deref().unwrap_or(""))
                .zip(text_intrinsic_width(el, next.as_str()))
                .is_some_and(|(before, after)| (before - after).abs() < 0.5);
            el.text = Some(next);
            el.style.flex_shrink = 0.0;
            if same_width {
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::LayoutChanged
            }
        },
    )
}

/// Intrinsic single-line width of `text` under `el`'s text style.
///
/// `None` when the node is not a plain text leaf, in which case callers must assume the layout
/// changed.
fn text_intrinsic_width(el: &Element, text: &str) -> Option<f32> {
    if !el.children.is_empty()
        || el.text_shape_suffix.is_some()
        || el.text_spans.is_some()
        || el.text_style.font_family.as_deref() == Some("lucide")
    {
        return None;
    }
    let size = el.text_style.size;
    let line_height = el
        .text_style
        .line_height_px
        .or_else(|| el.text_style.line_height_scale.map(|s| size * s))
        .unwrap_or_else(|| size * crate::style::tailwind_default_leading_scale(size));
    let prepared = crate::text::prepare(text, size, el.text_style.weight, line_height);
    Some(crate::text::intrinsic_width(&prepared))
}

/// Attach a Solid-style binding to an existing element (style/class/text patches).
///
/// `bind_key` uniquely identifies the effect (idempotent across root rebuilds). The mounted node is
/// looked up by `el.template_id` (set to `bind_key` when missing).
pub fn bind_prop<T, C, A>(
    mut el: Element,
    bind_key: impl Into<String>,
    mut compute: C,
    apply: A,
) -> Element
where
    T: 'static,
    C: FnMut() -> T + 'static,
    A: Fn(&mut Element, T) -> LayoutStatus + 'static,
{
    let bind_key = bind_key.into();
    if el.template_id.is_none() {
        el.template_id = Some(bind_key.clone());
    }
    let node_id = el
        .template_id
        .clone()
        .expect("bind_prop: template_id assigned above");

    let initial = untrack(&mut compute);
    let _ = apply(&mut el, initial);

    if !crate::core::reactivity::is_tracking() {
        return el;
    }

    let existing_hook = REGISTERED_BINDINGS.with(|registry| {
        registry
            .borrow()
            .get(&bind_key)
            .map(|registration| Rc::clone(&registration.hook))
    });
    if let Some(hook) = existing_hook {
        return chain_unmount_hook(el, hook);
    }

    let hook_key = bind_key.clone();
    let hook = UnmountHook::new(move || {
        let effect_id = REGISTERED_BINDINGS.with(|registry| {
            registry
                .borrow_mut()
                .remove(&hook_key)
                .and_then(|registration| registration.effect_id)
        });
        if let Some(effect_id) = effect_id {
            dispose_effect(effect_id);
        }
    });
    REGISTERED_BINDINGS.with(|registry| {
        registry.borrow_mut().insert(
            bind_key.clone(),
            BindingRegistration {
                effect_id: None,
                hook: Rc::clone(&hook),
            },
        );
    });

    let effect_node_id = node_id;
    let effect_id = create_effect(move || {
        let next = compute();
        if let Some(status) =
            runtime::try_update_element_by_template_id(&effect_node_id, |mounted| {
                apply(mounted, next)
            })
        {
            if status != LayoutStatus::Clean {
                runtime::request_redraw();
            }
        }
    });
    REGISTERED_BINDINGS.with(|registry| {
        if let Some(registration) = registry.borrow_mut().get_mut(&bind_key) {
            registration.effect_id = Some(effect_id);
        }
    });

    chain_unmount_hook(el, hook)
}

fn chain_unmount_hook(mut el: Element, hook: Rc<UnmountHook>) -> Element {
    if let Some(prev) = el.on_unmount.take() {
        let chained = UnmountHook::new(move || {
            hook.fire();
            prev.fire();
        });
        el.on_unmount = Some(chained);
    } else {
        el.on_unmount = Some(hook);
    }
    el
}

/// Solid-style single-binding effect for style/text patches on a stable `template_id` node.
///
/// `compute` runs inside a dedicated effect (so `signal.get()` does not subscribe the root tree).
/// `apply` mutates the already-mounted element and returns the layout impact.
pub fn bind_with<T, C, A>(anchor_id: impl Into<String>, compute: C, apply: A) -> Element
where
    T: 'static,
    C: FnMut() -> T + 'static,
    A: Fn(&mut Element, T) -> LayoutStatus + 'static,
{
    let id = anchor_id.into();
    bind_prop(Element::new().template_id(id.clone()), id, compute, apply)
}

/// Bind a dynamic utility-class string (`class` / `tw` / `uno`) onto an element in place.
pub fn bind_class<F>(el: Element, bind_key: impl Into<String>, compute: F) -> Element
where
    F: FnMut() -> String + 'static,
{
    bind_prop(el, bind_key, compute, |el, classes| {
        crate::style::apply_utility_classes(el, &classes)
    })
}

/// Alias of [`bind_prop`] — preferred name when binding a single non-style attribute
/// (e.g. Button `variant`, Input `invalid`, Icon `name`).
#[inline]
pub fn bind_attr<T, C, A>(el: Element, bind_key: impl Into<String>, compute: C, apply: A) -> Element
where
    T: 'static,
    C: FnMut() -> T + 'static,
    A: Fn(&mut Element, T) -> LayoutStatus + 'static,
{
    bind_prop(el, bind_key, compute, apply)
}

/// Batch style / class patch applied in place by [`bind_style`].
#[derive(Clone, Debug, Default)]
pub struct StylePatch {
    pub bg: Option<[f32; 4]>,
    pub text_color: Option<[u8; 4]>,
    pub border_color: Option<[f32; 4]>,
    pub class: Option<String>,
}

impl StylePatch {
    /// Apply non-`None` fields; returns the strongest layout impact among applied patches.
    pub fn apply(self, el: &mut Element) -> LayoutStatus {
        let mut impact = LayoutStatus::Clean;
        if let Some(bg) = self.bg {
            if el.style.bg_color != bg {
                el.style.bg_color = bg;
                impact = impact.merge(LayoutStatus::StyleChanged);
            }
        }
        if let Some(color) = self.text_color {
            if el.text_style.color != color {
                el.text_style.color = color;
                impact = impact.merge(LayoutStatus::StyleChanged);
            }
        }
        if let Some(color) = self.border_color {
            if el.style.border_color != color {
                el.style.border_color = color;
                impact = impact.merge(LayoutStatus::StyleChanged);
            }
        }
        if let Some(class) = self.class {
            impact = impact.merge(crate::style::apply_utility_classes(el, &class));
        }
        impact
    }
}

/// Solid-style multi-field style binding via [`StylePatch`].
pub fn bind_style<F>(el: Element, bind_key: impl Into<String>, compute: F) -> Element
where
    F: FnMut() -> StylePatch + 'static,
{
    bind_prop(el, bind_key, compute, |el, patch| patch.apply(el))
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

/// Solid-style `<Show>`: `when` is evaluated inside a dedicated [`reactive_view`] effect so
/// `signal.get()` in the condition does not subscribe the root `app_builder`.
pub fn show_reactive<W, F>(anchor_id: impl Into<String>, mut when: W, render: F) -> Element
where
    W: FnMut() -> bool + 'static,
    F: Fn() -> Element + 'static,
{
    let id = anchor_id.into();
    reactive_view(id, move || if when() { render() } else { Element::new() })
}

/// Solid-style `<For each={list.get()}>`: rebuild the keyed children list inside a dedicated
/// [`reactive_view`] so reading the list signal does not subscribe the root tree.
pub fn for_reactive<F>(anchor_id: impl Into<String>, mut render_list: F) -> Element
where
    F: FnMut() -> Vec<Element> + 'static,
{
    let id = anchor_id.into();
    reactive_view(id, move || {
        let mut root = Element::new();
        root.children = render_list();
        root
    })
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

#[cfg(test)]
mod intrinsic_width_tests {
    use super::*;

    fn label(text: &str) -> Element {
        let mut el = Element::new();
        el.text = Some(text.to_string());
        el
    }

    #[test]
    fn same_glyph_count_keeps_the_same_intrinsic_width() {
        let el = label("10");
        let before = text_intrinsic_width(&el, "10").expect("measurable");
        let after = text_intrinsic_width(&el, "11").expect("measurable");
        assert!(
            (before - after).abs() < 0.5,
            "digit-only counters should not resize: {before} vs {after}"
        );
    }

    #[test]
    fn longer_text_reports_a_wider_intrinsic_width() {
        let el = label("1");
        let before = text_intrinsic_width(&el, "1").expect("measurable");
        let after = text_intrinsic_width(&el, "1000000").expect("measurable");
        assert!(after > before + 0.5, "{before} vs {after}");
    }

    #[test]
    fn non_leaf_nodes_are_not_comparable() {
        let el = Element::new().children(vec![label("x")]);
        assert!(text_intrinsic_width(&el, "x").is_none());
    }
}
