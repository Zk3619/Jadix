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
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};

use crate::core::reactivity::{
    create_render_effect, dispose_effect, effect_is_alive, untrack, use_signal, EffectId, Signal,
};
use crate::core::tree::{
    Element, IntoCssDim, IntoNodeBg, IntoNodeClass, IntoNodeFg, IntoNodeOpacity, IntoNodeSize,
    IntoNodeText, IntoOpacity, LayoutStatus, UnitSlot, UnmountHook,
};
use crate::style::{IntoPaintColor, IntoTextColor};

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
    /// `{sid}__view` reconcile miss → bump `{sid}__remount` once borrow/tree settles.
    /// Drained after hover callbacks (AppState released) on pointer / redraw paths.
    static DEFERRED_VIEW_REMOUNTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// Cap deferred remounts per sub id so a permanently missing anchor cannot pump forever.
    static DEFERRED_VIEW_RETRY_COUNTS: RefCell<HashMap<String, u8>> =
        RefCell::new(HashMap::new());
    /// Overlay ghost clone re-runs author bodies during paint (`reactive_sync`),
    /// which is not a tracked effect. Islands must inline `render()` instead of
    /// returning an empty placeholder (same contract as drag's `assemble_*`).
    static INLINE_ISLAND_RENDER: Cell<bool> = const { Cell::new(false) };
    /// `try_update_element_by_template_id` DFS miss (WP-N.5 / D4).
    static BIND_MISS_COUNT: Cell<usize> = const { Cell::new(0) };
    /// Tracked `<For>` without `key` rebuilt the whole list (WP-N.4 / Wave 2).
    static UNKEYED_FOR_REBUILD_COUNT: Cell<usize> = const { Cell::new(0) };
    static UNKEYED_FOR_WARNED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    /// WP-N.6 interned static skeletons keyed by `template_hash` (or Show fallback id).
    static STATIC_TEMPLATES: RefCell<HashMap<String, Element>> = RefCell::new(HashMap::new());
    static STATIC_TEMPLATE_CLONE_HITS: Cell<usize> = const { Cell::new(0) };
}

/// See [`INLINE_ISLAND_RENDER`]. Entered by drag overlay source-preview clone.
pub(crate) struct InlineIslandGuard(bool);

impl InlineIslandGuard {
    pub fn enter() -> Self {
        INLINE_ISLAND_RENDER.with(|c| {
            let prev = c.get();
            c.set(true);
            Self(prev)
        })
    }
}

impl Drop for InlineIslandGuard {
    fn drop(&mut self) {
        INLINE_ISLAND_RENDER.with(|c| c.set(self.0));
    }
}

fn is_inline_island_render() -> bool {
    INLINE_ISLAND_RENDER.with(|c| c.get())
}

pub(crate) const REACTIVE_PLACEHOLDER_HASH: &str = "__rui_reactive_placeholder_v1";
/// Compile-time `ui!` `file:line:column`, preserved when [`reactive_anchor`] overwrites
/// `template_id` with a stable island id (e.g. Gallery `App:section_body`).
pub(crate) const ATTR_SOURCE_TEMPLATE_ID: &str = "rui_ui_template_id";

/// Registry keys must not collide across windows when component ids match.
pub(crate) fn window_scoped_registry_key(id: &str) -> String {
    let scope = crate::core::reactivity::signal_key_scope();
    if scope == 0 {
        id.to_string()
    } else {
        format!("{id}__w{scope}")
    }
}

fn restore_window_scope(
    scope: super::window_scope::WindowScope,
    window: Option<winit::window::WindowId>,
) -> super::window_scope::WindowScopeGuard {
    if let Some(id) = window {
        super::window_scope::enter_secondary_id(id)
    } else {
        super::window_scope::enter(scope)
    }
}

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
    hook: Weak<UnmountHook>,
}

/// Current size of [`REGISTERED_ANCHORS`] (WP-2.4 diagnostics / leak tests).
pub fn registered_anchor_count() -> usize {
    REGISTERED_ANCHORS.with(|registry| registry.borrow().len())
}

/// Whether a [`reactive_anchor`] for `anchor_id` is registered and its effect is still alive.
///
/// Singleton hosts (Toaster) use this so a second mount in the same tree can warn, without a
/// process-wide counter that would leak across `create_root` tests.
pub fn reactive_anchor_is_live(anchor_id: &str) -> bool {
    let registry_key = window_scoped_registry_key(anchor_id);
    REGISTERED_ANCHORS.with(|registry| {
        registry
            .borrow()
            .get(&registry_key)
            .and_then(|registration| registration.effect_id)
            .is_some_and(effect_is_alive)
    })
}

/// Queue a remount for a menu/submenu content island (`{sid}__view` → `{sid}__remount`).
///
/// Used when `try_reconcile_subtree` misses (borrow contention, or anchor not yet in the live
/// tree). Hover highlight can stick while the pointer stays on the trigger — without a deferred
/// bump, no further `on_hover(true)` fires and the panel never appears.
pub fn defer_view_remount(view_anchor_id: &str) {
    let Some(sid) = view_anchor_id.strip_suffix("__view") else {
        return;
    };
    if sid.is_empty() {
        return;
    }
    const MAX_RETRIES: u8 = 4;
    let allow = DEFERRED_VIEW_RETRY_COUNTS.with(|c| {
        let mut c = c.borrow_mut();
        let n = c.entry(sid.to_string()).or_insert(0);
        if *n >= MAX_RETRIES {
            return false;
        }
        *n = n.saturating_add(1);
        true
    });
    if !allow {
        return;
    }
    DEFERRED_VIEW_REMOUNTS.with(|q| {
        let mut q = q.borrow_mut();
        if !q.iter().any(|s| s == sid) {
            q.push(sid.to_string());
        }
    });
    runtime::request_redraw();
}

/// Clear deferred-remount retry budget after a successful reconcile into `{sid}__view`.
fn clear_view_remount_retries(view_anchor_id: &str) {
    if let Some(sid) = view_anchor_id.strip_suffix("__view") {
        DEFERRED_VIEW_RETRY_COUNTS.with(|c| {
            c.borrow_mut().remove(sid);
        });
    }
}

/// Drain [`defer_view_remount`] bumps and idle island retries. Call only when `AppState` is not borrowed.
pub fn drain_deferred_view_remounts() {
    let ids = DEFERRED_VIEW_REMOUNTS.with(|q| std::mem::take(&mut *q.borrow_mut()));
    if !ids.is_empty() {
        for sid in ids {
            let bump = use_signal(format!("{sid}__remount").as_str(), || 0u32);
            bump.set(bump.peek().wrapping_add(1));
        }
        runtime::request_redraw();
    }
    crate::core::reactivity::drain_idle_effect_retries();
}

/// Current size of [`REGISTERED_BINDINGS`] (WP-2.4 diagnostics / leak tests).
pub fn registered_binding_count() -> usize {
    REGISTERED_BINDINGS.with(|registry| registry.borrow().len())
}

/// Current size of [`COMPONENT_PROPS`] (WP-2.4 diagnostics / leak tests).
pub fn component_props_count() -> usize {
    COMPONENT_PROPS.with(|store| store.borrow().len())
}

/// Current size of [`COMPONENT_CHILDREN`] stash (WP-2.4 diagnostics / leak tests).
pub fn component_children_count() -> usize {
    COMPONENT_CHILDREN.with(|store| store.borrow().len())
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
        if let Some(hook) = registration.hook.upgrade() {
            hook.disarm();
        }
        if let Some(effect_id) = registration.effect_id {
            dispose_effect(effect_id);
        }
    }

    DEFERRED_VIEW_REMOUNTS.with(|q| q.borrow_mut().clear());
    DEFERRED_VIEW_RETRY_COUNTS.with(|c| c.borrow_mut().clear());
    crate::core::reactivity::clear_idle_effect_retry_queues();

    COMPONENT_PROPS.with(|store| store.borrow_mut().clear());
    COMPONENT_CHILDREN.with(|store| store.borrow_mut().clear());
    super::keyed_list::reset_keyed_list_registry();
    reset_unkeyed_for_rebuild_count();
    reset_static_templates();
}

/// Leaf `bind_*` apply must not claim a structural rebuild (D4 / WP-N.5).
pub(crate) fn sanitize_leaf_bind_status(status: LayoutStatus) -> LayoutStatus {
    if matches!(status, LayoutStatus::StructureChanged) {
        #[cfg(debug_assertions)]
        log::warn!(
            target: "rui::bind",
            "leaf bind apply returned StructureChanged; coercing to LayoutChanged \
             (structure changes belong on Show/For/Portal islands)"
        );
        LayoutStatus::LayoutChanged
    } else {
        status
    }
}

/// Count of bind `template_id` lookups that missed the live tree (WP-N.5).
pub fn bind_miss_count() -> usize {
    BIND_MISS_COUNT.with(Cell::get)
}

/// Clear [`bind_miss_count`] (tests).
pub fn reset_bind_miss_count() {
    BIND_MISS_COUNT.with(|c| c.set(0));
}

pub(crate) fn note_bind_miss(template_id: &str) {
    BIND_MISS_COUNT.with(|c| c.set(c.get().saturating_add(1)));
    #[cfg(debug_assertions)]
    log::warn!(
        target: "rui::bind",
        "bind miss template_id={template_id} (unmounted or not in the live tree)"
    );
}

/// Whole-list rebuilds from tracked `<For>` without `key` (WP-N.4).
pub fn unkeyed_for_rebuild_count() -> usize {
    UNKEYED_FOR_REBUILD_COUNT.with(Cell::get)
}

/// Clear [`unkeyed_for_rebuild_count`] (tests).
pub fn reset_unkeyed_for_rebuild_count() {
    UNKEYED_FOR_REBUILD_COUNT.with(|c| c.set(0));
    UNKEYED_FOR_WARNED.with(|s| s.borrow_mut().clear());
}

fn note_unkeyed_for_rebuild(anchor_id: &str) {
    UNKEYED_FOR_REBUILD_COUNT.with(|c| c.set(c.get().saturating_add(1)));
    let first = UNKEYED_FOR_WARNED.with(|s| s.borrow_mut().insert(anchor_id.to_string()));
    #[cfg(debug_assertions)]
    if first {
        log::warn!(
            target: "rui::list",
            "tracked <For> {anchor_id} has no key; each list change rebuilds the whole table. Add `key={{…}}` to use for_keyed_reactive (item-level owners)."
        );
    }
    #[cfg(not(debug_assertions))]
    let _ = first;
}

/// How many Show remounts reused an interned static skeleton (WP-N.6).
pub fn static_template_clone_hit_count() -> usize {
    STATIC_TEMPLATE_CLONE_HITS.with(Cell::get)
}

/// Clear interned static templates (tests).
pub fn reset_static_templates() {
    STATIC_TEMPLATES.with(|m| m.borrow_mut().clear());
    STATIC_TEMPLATE_CLONE_HITS.with(|c| c.set(0));
}

fn intern_static_template(key: &str, el: &Element) {
    STATIC_TEMPLATES.with(|m| {
        m.borrow_mut()
            .entry(key.to_string())
            .or_insert_with(|| el.clone_static_skeleton());
    });
}

fn clone_interned_template(key: &str) -> Option<Element> {
    STATIC_TEMPLATES.with(|m| m.borrow().get(key).map(Element::clone_static_skeleton))
}

fn subtree_is_clone_safe(root: &Element) -> bool {
    let mut stack = vec![root];
    while let Some(el) = stack.pop() {
        if el.on_click.is_some()
            || el.on_hover.is_some()
            || el.on_mousedown.is_some()
            || el.on_mouseup.is_some()
            || el.on_context_menu.is_some()
            || el.on_double_click.is_some()
            || el.on_mousemove.is_some()
            || el.on_mouseover.is_some()
            || el.on_mouseout.is_some()
            || el.on_key_down.is_some()
            || el.on_unmount.is_some()
            || el.button_hover_paint.is_some()
            || el.input_paint_sync.is_some()
            || el.switch_checked_sync.is_some()
            || el.checkbox_sync.is_some()
            || el.radio_selected_sync.is_some()
            || el.reactive_sync.is_some()
        {
            return false;
        }
        stack.extend(el.children.iter());
    }
    true
}

fn has_static_payload(el: &Element) -> bool {
    el.text.is_some() || !el.children.is_empty() || !el.attrs.is_empty() || el.template_id.is_some()
}

/// Stash RSX `children` for a reactive component between `From`/function calls.
pub fn stash_component_children(id: &str, children: Option<Element>) {
    let key = window_scoped_registry_key(id);
    COMPONENT_CHILDREN.with(|store| {
        store.borrow_mut().insert(key, children);
    });
}

/// Take stashed children inside a reactive component render pass.
pub fn take_component_children(id: &str) -> Option<Element> {
    let key = window_scoped_registry_key(id);
    COMPONENT_CHILDREN.with(|store| store.borrow_mut().remove(&key).flatten())
}

fn mounted_anchor_hook(id: &str) -> Option<Rc<UnmountHook>> {
    REGISTERED_ANCHORS.with(|registry| {
        let registry = registry.borrow();
        let registration = registry.get(id)?;
        registration.hook.upgrade()
    })
}

/// Re-run a registered reactive-island effect (parent rebuild recreated an empty placeholder).
fn schedule_registered_anchor_effect(registry_key: &str) {
    let effect_id = REGISTERED_ANCHORS.with(|registry| {
        registry
            .borrow()
            .get(registry_key)
            .and_then(|registration| registration.effect_id)
    });
    if let Some(effect_id) = effect_id {
        crate::core::reactivity::schedule_effect(effect_id);
    }
}

/// Empty subtree root used only to reserve a stable [`Element::template_id`] in the declarative tree.
/// Must use the **same** string as [`reactive_anchor`].
pub fn reactive_placeholder(anchor_id: impl Into<String>) -> Element {
    reactive_placeholder_sized(anchor_id, "100%", "100%")
}

/// Like [`reactive_placeholder`], but with explicit size (e.g. `0`×`0` for portal anchors
/// inside ScrollArea so a `100%` shell cannot inflate scroll content metrics).
pub fn reactive_placeholder_sized(
    anchor_id: impl Into<String>,
    width: impl crate::core::tree::IntoCssDim,
    height: impl crate::core::tree::IntoCssDim,
) -> Element {
    let id = anchor_id.into();
    let registry_key = window_scoped_registry_key(&id);
    let mut placeholder = Element::new()
        .template_id(id)
        .template_hash(REACTIVE_PLACEHOLDER_HASH)
        .w(width)
        .h(height);

    if let Some(hook) = mounted_anchor_hook(&registry_key) {
        placeholder = placeholder.on_unmount_hook(Rc::clone(&hook));
        REGISTERED_ANCHORS.with(|registry| {
            if let Some(registration) = registry.borrow_mut().get_mut(&registry_key) {
                registration.hook = AnchorHook::Mounted(Rc::downgrade(&hook));
            }
        });
    }
    placeholder
}

/// In-flow content slot for Accordion / Collapsible / Tabs (ShadCN `hidden` / CSS `display:none`).
///
/// Collapsed or unselected panels must **not** use [`crate::utils::popup::flow_hidden_portal_host`]
/// (`position:absolute`): when the island mounts they have to take height in the document flow.
/// A static `0×0` leftover still occupies parent flex `gap`; `display:none` does not.
/// Open / selected panels stay sized `0×0` until reconcile fills them so a root `gap` between
/// the trigger row and the visible panel is already reserved.
pub fn reactive_placeholder_in_flow(anchor_id: impl Into<String>, visible: bool) -> Element {
    let mut placeholder = reactive_placeholder_sized(anchor_id, 0.0, 0.0);
    if !visible {
        placeholder.style.display = taffy::style::Display::None;
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
    let registry_key = window_scoped_registry_key(&id);
    // Zombie registration: id still in the map but effect was disposed (e.g. owner teardown
    // raced with unmount). Re-register so nested SubContent can mount again.
    let already_registered = REGISTERED_ANCHORS.with(|registry| {
        let mut map = registry.borrow_mut();
        if let Some(registration) = map.get(&registry_key) {
            let alive = registration.effect_id.is_some_and(effect_is_alive);
            if alive {
                return true;
            }
            if let Some(hook) = registration.hook.upgrade() {
                hook.disarm();
            }
            if let Some(effect_id) = registration.effect_id {
                dispose_effect(effect_id);
            }
            map.remove(&registry_key);
        }
        false
    });
    if already_registered {
        // Island already registered but the live tree may have a fresh empty placeholder
        // (Show / switch_reactive section remount). Re-run the island effect so it reconciles
        // into the new shell — not only `{sid}__view` menu panels.
        schedule_registered_anchor_effect(&registry_key);
        return;
    }

    let hook_id = registry_key.clone();
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
            registry_key.clone(),
            AnchorRegistration {
                effect_id: None,
                hook: AnchorHook::Pending(Rc::clone(&hook)),
            },
        );
    });

    // Owner dispose also drops the registration (in addition to element unmount).
    let cleanup_id = registry_key.clone();
    crate::core::reactivity::on_cleanup(move || {
        let removed = REGISTERED_ANCHORS.with(|registry| registry.borrow_mut().remove(&cleanup_id));
        if let Some(registration) = removed {
            if let Some(hook) = registration.hook.upgrade() {
                hook.disarm();
            }
            if let Some(effect_id) = registration.effect_id {
                dispose_effect(effect_id);
            }
        }
    });

    let effect_anchor_id = id.clone();
    let effect_registry_key = registry_key.clone();
    let effect_scope = super::window_scope::active();
    let effect_window = super::window_scope::active_window_id();
    let effect_id = create_render_effect(move || {
        let _scope = restore_window_scope(effect_scope, effect_window);
        let mut fragment = f();
        if let Some(orig) = fragment.template_id.as_ref() {
            if orig != &effect_anchor_id {
                fragment.set_attr(ATTR_SOURCE_TEMPLATE_ID, orig.clone());
            }
        }
        fragment.template_id = Some(effect_anchor_id.clone());
        if let Some(hook) = mounted_anchor_hook(&effect_registry_key) {
            fragment = fragment.on_unmount_hook(hook);
        }
        if runtime::try_reconcile_subtree(&effect_anchor_id, fragment).is_some() {
            clear_view_remount_retries(&effect_anchor_id);
            if let Some(id) = crate::core::reactivity::current_effect_id() {
                crate::core::reactivity::clear_idle_effect_retry(id);
            }
            runtime::request_redraw();
        } else {
            // Miss: borrow held, or anchor not in the live tree yet. Do **not**
            // `schedule_effect` into the in-flight `flush_deferred` drain — that
            // 100k-looped (gallery overlays page, `show_reactive` tests).
            crate::core::reactivity::queue_running_effect_for_idle_retry();
            if effect_anchor_id.ends_with("__view") {
                defer_view_remount(&effect_anchor_id);
            }
            runtime::request_redraw();
            log::debug!(
                target: "rui::reconcile",
                "reconcile MISS anchor={effect_anchor_id} (idle retry)"
            );
        }
    });
    REGISTERED_ANCHORS.with(|registry| {
        if let Some(registration) = registry.borrow_mut().get_mut(&registry_key) {
            registration.effect_id = Some(effect_id);
        }
    });
}

/// Component/application reusable structural reactive boundary.
///
/// Allowed: Show / For / Match / Portal / a composite root that must insert or delete
/// descendants. Not allowed: flashing a label, `variant`, or class — use [`bind_prop`] /
/// [`bind_text`] / [`crate::core::tree::ReactiveSync`] instead (WP-N.4).
///
/// Table / Stepper / Timeline / Carousel still rebuild through this island — known
/// structure, not a leaf-bind gap (true-fine-grained-closeout: not this wave).
///
/// Stateful components can return this directly when signal changes must mount/unmount descendants.
/// For style/layout-only changes prefer [`crate::core::tree::ReactiveSync`], which avoids subtree
/// reconstruction entirely.
pub fn reactive_view<F>(anchor_id: impl Into<String>, mut render: F) -> Element
where
    F: FnMut() -> Element + 'static,
{
    if is_inline_island_render() {
        return render();
    }
    let id = anchor_id.into();
    reactive_anchor(id.clone(), render);
    reactive_placeholder(id)
}

/// Like [`reactive_view`], but the placeholder uses explicit width/height (e.g. fixed-size SVG islands).
pub fn reactive_view_sized<F>(
    anchor_id: impl Into<String>,
    width: impl crate::core::tree::IntoCssDim,
    height: impl crate::core::tree::IntoCssDim,
    mut render: F,
) -> Element
where
    F: FnMut() -> Element + 'static,
{
    if is_inline_island_render() {
        return render();
    }
    let id = anchor_id.into();
    reactive_anchor(id.clone(), render);
    reactive_placeholder_sized(id, width, height)
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
    if is_inline_island_render() {
        return render(props);
    }
    let id = anchor_id.into();
    let registry_key = window_scoped_registry_key(&id);
    let gen_key = format!("{registry_key}__component_gen");
    let generation = use_signal(gen_key.as_str(), || 0u32);

    let props_cell = COMPONENT_PROPS.with(|store| {
        let mut store = store.borrow_mut();
        if let Some(cell) = store.get(&registry_key) {
            Rc::clone(cell)
        } else {
            let cell = Rc::new(RefCell::new(Box::new(props.clone()) as Box<dyn Any>));
            store.insert(registry_key.clone(), Rc::clone(&cell));
            // Owner dispose must drop props/children stash (not only unmount / reset).
            let cleanup_id = registry_key.clone();
            crate::core::reactivity::on_cleanup(move || {
                COMPONENT_PROPS.with(|s| {
                    s.borrow_mut().remove(&cleanup_id);
                });
                COMPONENT_CHILDREN.with(|s| {
                    s.borrow_mut().remove(&cleanup_id);
                });
            });
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

/// Like [`reactive_component`], but `render` runs with normal signal tracking (no
/// [`crate::core::reactivity::isolate_reads`]).
///
/// Use for components whose internal signals (e.g. Select `open`) must rebuild the
/// subtree. Prefer [`reactive_component`] when the body should only refresh on props.
pub fn reactive_component_live<P, F>(anchor_id: impl Into<String>, props: P, render: F) -> Element
where
    P: Clone + PartialEq + 'static,
    F: FnMut(P) -> Element + 'static,
{
    reactive_component_live_sized(anchor_id, props, "100%", "100%", render)
}

/// Like [`reactive_component_live`], with explicit placeholder size.
///
/// Calendar 等放进 DateTime 浮层时须 `h: auto`：默认 `100%×100%` 会按面板高度拉伸，
/// The day grid covers the `Separator` below.
pub fn reactive_component_live_sized<P, F>(
    anchor_id: impl Into<String>,
    props: P,
    width: impl IntoCssDim,
    height: impl IntoCssDim,
    mut render: F,
) -> Element
where
    P: Clone + PartialEq + 'static,
    F: FnMut(P) -> Element + 'static,
{
    if is_inline_island_render() {
        return render(props);
    }
    let id = anchor_id.into();
    let registry_key = window_scoped_registry_key(&id);
    let gen_key = format!("{registry_key}__component_gen");
    let generation = use_signal(gen_key.as_str(), || 0u32);

    let props_cell = COMPONENT_PROPS.with(|store| {
        let mut store = store.borrow_mut();
        if let Some(cell) = store.get(&registry_key) {
            Rc::clone(cell)
        } else {
            let cell = Rc::new(RefCell::new(Box::new(props.clone()) as Box<dyn Any>));
            store.insert(registry_key.clone(), Rc::clone(&cell));
            let cleanup_id = registry_key.clone();
            crate::core::reactivity::on_cleanup(move || {
                COMPONENT_PROPS.with(|s| {
                    s.borrow_mut().remove(&cleanup_id);
                });
                COMPONENT_CHILDREN.with(|s| {
                    s.borrow_mut().remove(&cleanup_id);
                });
            });
            cell
        }
    });

    let props_changed = {
        let mut boxed = props_cell.borrow_mut();
        let slot = boxed
            .downcast_mut::<P>()
            .expect("reactive_component_live props type mismatch for anchor id");
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
            .expect("reactive_component_live props type mismatch for anchor id")
            .clone();
        render(props)
    });
    reactive_placeholder_sized(id, width, height)
}

/// Solid-style text binding: register one effect that patches `text` on a mounted node in place.
/// Solid-style in-place text binding (like setting a DOM text node). Prefer this for
/// labels/counters — it does **not** rebuild a subtree.
///
/// Layout impact mirrors the browser: when intrinsic width **or** layout height **changes**
/// (grow *or* shrink — content-sized badges / Labels must collapse; multiline echo Labels must
/// drop height when lines are deleted), or when new text exceeds the mounted box, return
/// [`LayoutStatus::LayoutChanged`]. Equal-metric updates (digit counters `"10"`→`"11"`) stay
/// paint-only [`LayoutStatus::StyleChanged`].
pub fn bind_text<F>(anchor_id: impl Into<String>, compute: F) -> Element
where
    F: FnMut() -> String + 'static,
{
    let id = anchor_id.into();
    bind_prop(
        Element::new().template_id(id.clone()),
        id,
        compute,
        |el, next| apply_bound_text_value(el, next),
    )
}

pub(crate) fn apply_bound_text_value(el: &mut Element, next: String) -> LayoutStatus {
    if el.text.as_deref() == Some(next.as_str()) {
        return LayoutStatus::Clean;
    }
    let old = el.text.as_deref().unwrap_or("");
    let needs_layout = bind_text_metric_needs_layout(el, old, next.as_str());
    el.text = Some(next);
    el.style.flex_shrink = 0.0;
    if needs_layout {
        LayoutStatus::LayoutChanged
    } else {
        LayoutStatus::StyleChanged
    }
}

/// Whether a `bind_text` string change must reflow (vs paint-only).
pub(crate) fn bind_text_metric_needs_layout(el: &Element, old: &str, next: &str) -> bool {
    if !plain_text_leaf(el) {
        return true;
    }
    // Specified width (px / %): the used box does not shrink-wrap glyphs.
    // Skip cosmic `prepare` on the pointer path. Length may change ("Play" /
    // "Pause") — wrap/height is the only geometry that can still move.
    if bind_text_width_is_specified(el) {
        if newline_bytes(old) == newline_bytes(next) {
            // Single line + authored width: used box does not wrap. Skip
            // cosmic `prepare` on the pointer path (Go/Reset, Play/Pause).
            if newline_bytes(old) == 0 {
                return false;
            }
            let box_w = el.layout_bounds[2];
            let box_h = el.layout_bounds[3];
            let avail = if box_w > 1.0 { box_w } else { f32::MAX };
            let before_h = text_layout_height(el, old, avail);
            let after_h = text_layout_height(el, next, avail);
            return match (before_h, after_h) {
                (Some(before), Some(after)) => {
                    (after - before).abs() > 0.01 || (box_h > 1.0 && after > box_h + 0.01)
                }
                _ => true,
            };
        }
        return true;
    }
    let before_w = text_intrinsic_width(el, old);
    let after_w = text_intrinsic_width(el, next);
    let box_w = el.layout_bounds[2];
    let box_h = el.layout_bounds[3];
    let avail = if box_w > 1.0 { box_w } else { f32::MAX };
    let before_h = text_layout_height(el, old, avail);
    let after_h = text_layout_height(el, next, avail);
    // Width grow *or* shrink: gallery Toggle badge ("Blocked"→"Ready") must collapse the chip.
    // Grow-only left content-sized parents stuck at the wider measure.
    let width_changed = match (before_w, after_w) {
        (Some(before), Some(after)) => {
            (after - before).abs() > 0.01 || (box_w > 1.0 && after > box_w + 0.01)
        }
        _ => true,
    };
    // Height grow *or* shrink: multiline echo Labels must collapse when lines are deleted.
    let height_changed = match (before_h, after_h) {
        (Some(before), Some(after)) => {
            (after - before).abs() > 0.01 || (box_h > 1.0 && after > box_h + 0.01)
        }
        _ => true,
    };
    width_changed || height_changed
}

fn newline_bytes(s: &str) -> usize {
    s.as_bytes().iter().filter(|&&b| b == b'\n').count()
}

fn bind_text_width_is_specified(el: &Element) -> bool {
    if !el.style.size.width.is_auto() {
        return true;
    }
    el.style.unit_specs.iter().any(|(slot, dim)| {
        matches!(*slot, UnitSlot::Width | UnitSlot::InlineSize) && dim.has_percent()
    })
}

/// Leaf `bind_text` LayoutChanged whose used box is clamped by a px/`rem`
/// ancestor (Play/Pause inside `min_w` Button). Skip promoting that to a page
/// `LayoutChanged` — root Taffy compute hitch other CSS animations for a frame.
///
/// Requires a definite inline size **and** a positive laid-out (or authored)
/// height so wrap rows under `%` width cannot absorb and leave footer Y stale.
pub(crate) fn bind_layout_absorbed_by_specified_ancestor(root: &Element, path: &[usize]) -> bool {
    let mut el = root;
    for &i in path {
        if i >= el.children.len() {
            return false;
        }
        el = &el.children[i];
        if element_absorbs_leaf_layout(el) {
            return true;
        }
    }
    false
}

fn element_absorbs_leaf_layout(el: &Element) -> bool {
    let definite_w = crate::layout::dim_length_px(el.style.size.width)
        .or_else(|| crate::layout::dim_length_px(el.style.min_size.width))
        .is_some_and(|w| w > 0.0);
    if !definite_w {
        return false;
    }
    let definite_h = crate::layout::dim_length_px(el.style.size.height)
        .or_else(|| crate::layout::dim_length_px(el.style.min_size.height))
        .is_some_and(|h| h > 0.0);
    definite_h || el.layout_bounds[3] > 0.5
}

fn plain_text_leaf(el: &Element) -> bool {
    el.children.is_empty()
        && el.text_shape_suffix.is_none()
        && el.text_spans.is_none()
        && el.text_style.font_family.as_deref() != Some("lucide")
}

fn text_line_height_px(el: &Element) -> f32 {
    let size = el.text_style.size_resolved();
    el.text_style
        .line_height_px
        .or_else(|| el.text_style.line_height_scale.map(|s| size * s))
        .unwrap_or_else(|| size * crate::style::tailwind_default_leading_scale(size))
}

/// Intrinsic single-line width of `text` under `el`'s text style.
///
/// `None` when the node is not a plain text leaf, in which case callers must assume the layout
/// changed.
pub(crate) fn text_intrinsic_width(el: &Element, text: &str) -> Option<f32> {
    if !plain_text_leaf(el) {
        return None;
    }
    let size = el.text_style.size_resolved();
    let line_height = text_line_height_px(el);
    let prepared = crate::text::prepare(text, size, el.text_style.weight_resolved(), line_height);
    Some(crate::text::intrinsic_width(&prepared))
}

/// Wrapped / hard-newline height under `avail_width` (use `f32::MAX` when unconstrained).
pub(crate) fn text_layout_height(el: &Element, text: &str, avail_width: f32) -> Option<f32> {
    if !plain_text_leaf(el) {
        return None;
    }
    let size = el.text_style.size_resolved();
    let line_height = text_line_height_px(el);
    let prepared = crate::text::prepare(text, size, el.text_style.weight_resolved(), line_height);
    Some(crate::text::layout(&prepared, avail_width.max(1.0)).height)
}

/// Attach a Solid-style binding to an existing element (style/class/text patches).
///
/// `bind_key` is the `ui!` site. The registry also includes `el.template_id` so
/// list items that share a site (same file:line) still get separate effects.
/// Lookup of the mounted node is by `template_id` (set to `bind_key` when missing).
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
    // `bind_key` is file:line of one `ui!` site. List items share that site and must
    // still get separate effects — key by the node's `template_id` as well.
    let registry_key = window_scoped_registry_key(&format!("{bind_key}\0{node_id}"));

    let initial = untrack(&mut compute);

    if crate::core::reactivity::is_tracking() {
        let existing_hook = REGISTERED_BINDINGS.with(|registry| {
            registry
                .borrow()
                .get(&registry_key)
                .and_then(|registration| registration.hook.upgrade())
        });
        if let Some(hook) = existing_hook {
            let effect_alive = REGISTERED_BINDINGS.with(|registry| {
                registry
                    .borrow()
                    .get(&registry_key)
                    .and_then(|registration| registration.effect_id)
                    .is_some_and(effect_is_alive)
            });
            if effect_alive {
                // Rebuild fragment: the live tree already has this binding. Applying
                // `initial` here would seed CSS transitions from paint values
                // (sidebar width 16rem) and skip the standing tween.
                let _ = initial;
                return chain_unmount_hook(el, hook);
            }
            REGISTERED_BINDINGS.with(|registry| {
                registry.borrow_mut().remove(&registry_key);
            });
        }
    }

    let _ = sanitize_leaf_bind_status(apply(&mut el, initial));

    if !crate::core::reactivity::is_tracking() {
        return el;
    }

    let hook_key = registry_key.clone();
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
            registry_key.clone(),
            BindingRegistration {
                effect_id: None,
                hook: Rc::downgrade(&hook),
            },
        );
    });

    // Owner dispose also drops the registration (unmount alone is not enough for headless /
    // create_root cycles — WP-2.4 / R-WP-1).
    let cleanup_key = registry_key.clone();
    crate::core::reactivity::on_cleanup(move || {
        let removed =
            REGISTERED_BINDINGS.with(|registry| registry.borrow_mut().remove(&cleanup_key));
        if let Some(registration) = removed {
            if let Some(hook) = registration.hook.upgrade() {
                hook.disarm();
            }
            if let Some(effect_id) = registration.effect_id {
                dispose_effect(effect_id);
            }
        }
    });

    let effect_node_id = node_id;
    let effect_scope = super::window_scope::active();
    let effect_window = super::window_scope::active_window_id();
    let effect_id = create_render_effect(move || {
        let _scope = restore_window_scope(effect_scope, effect_window);
        let next = compute();
        if let Some(status) =
            runtime::try_update_element_by_template_id(&effect_node_id, |mounted| {
                sanitize_leaf_bind_status(apply(mounted, next))
            })
        {
            if status != LayoutStatus::Clean {
                runtime::request_redraw();
            }
        } else {
            // Borrow held (or no tree yet): same idle retry as island reconcile miss.
            // Unmounted miss is `Some(Clean)` + `note_bind_miss` and must not retry.
            crate::core::reactivity::queue_running_effect_for_idle_retry();
            runtime::request_redraw();
        }
    });
    REGISTERED_BINDINGS.with(|registry| {
        if let Some(registration) = registry.borrow_mut().get_mut(&registry_key) {
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
            if el.text_style.color_resolved() != color {
                el.text_style.color = Some(color);
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

/// Patch one size axis from a fine-grained binding (`ui!` dynamic `w` / `h` / `min_w` / …).
///
/// Returns [`LayoutStatus::LayoutChanged`] when the dimension actually changes so
/// `update_element_by_template_id` queues Taffy `mark_dirty` + root compute.
pub fn apply_bound_size(el: &mut Element, axis: &str, next: impl IntoCssDim) -> LayoutStatus {
    let dim = next.into_css_dim();
    let slot = match axis {
        "w" | "width" => UnitSlot::Width,
        "inline_size" | "inlineSize" => UnitSlot::InlineSize,
        "h" | "height" => UnitSlot::Height,
        "block_size" | "blockSize" => UnitSlot::BlockSize,
        "min_w" | "minWidth" | "min_width" => UnitSlot::MinWidth,
        "min_h" | "minHeight" | "min_height" => UnitSlot::MinHeight,
        "max_w" | "maxWidth" | "max_width" => UnitSlot::MaxWidth,
        "max_h" | "maxHeight" | "max_height" => UnitSlot::MaxHeight,
        _ => return LayoutStatus::Clean,
    };
    let before_specs = el.style.unit_specs.clone();
    if slot.is_logical() {
        el.style.set_css_dim(slot, dim);
        return if before_specs == el.style.unit_specs {
            LayoutStatus::Clean
        } else {
            LayoutStatus::LayoutChanged
        };
    }
    let before_taffy = match slot {
        UnitSlot::Width => el.style.size.width,
        UnitSlot::Height => el.style.size.height,
        UnitSlot::MinWidth => el.style.min_size.width,
        UnitSlot::MinHeight => el.style.min_size.height,
        UnitSlot::MaxWidth => el.style.max_size.width,
        UnitSlot::MaxHeight => el.style.max_size.height,
        _ => return LayoutStatus::Clean,
    };
    el.style.set_css_dim(slot, dim);
    let after_taffy = match slot {
        UnitSlot::Width => el.style.size.width,
        UnitSlot::Height => el.style.size.height,
        UnitSlot::MinWidth => el.style.min_size.width,
        UnitSlot::MinHeight => el.style.min_size.height,
        UnitSlot::MaxWidth => el.style.max_size.width,
        UnitSlot::MaxHeight => el.style.max_size.height,
        _ => return LayoutStatus::Clean,
    };
    if before_taffy == after_taffy && before_specs == el.style.unit_specs {
        LayoutStatus::Clean
    } else {
        LayoutStatus::LayoutChanged
    }
}

/// Patch CSS opacity from a fine-grained binding (`ui!` dynamic `opacity` / Builder Signal).
pub fn apply_bound_opacity(el: &mut Element, next: impl IntoOpacity) -> LayoutStatus {
    let v = next.into_opacity();
    if (el.style.opacity - v).abs() <= f32::EPSILON {
        LayoutStatus::Clean
    } else {
        el.style.opacity = v;
        LayoutStatus::StyleChanged
    }
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
///
/// Static bodies (no `bind_*`, no instance hooks) intern by `template_hash` and remount via
/// [`Element::clone_static_skeleton`] (WP-N.6). Dynamic bodies still re-run `render`.
pub fn show_reactive<W, F>(anchor_id: impl Into<String>, mut when: W, render: F) -> Element
where
    W: FnMut() -> bool + 'static,
    F: Fn() -> Element + 'static,
{
    let id = anchor_id.into();
    let intern_key = Rc::new(RefCell::new(None::<String>));
    reactive_view(id.clone(), move || {
        if !when() {
            return Element::new();
        }
        if let Some(key) = intern_key.borrow().clone() {
            if let Some(cloned) = clone_interned_template(&key) {
                STATIC_TEMPLATE_CLONE_HITS.with(|c| c.set(c.get().saturating_add(1)));
                return cloned;
            }
        }
        let binds_before = registered_binding_count();
        let anchors_before = registered_anchor_count();
        let el = render();
        let clone_safe = registered_binding_count() == binds_before
            && registered_anchor_count() == anchors_before
            && subtree_is_clone_safe(&el)
            && has_static_payload(&el);
        if clone_safe {
            let key = el
                .template_hash
                .clone()
                .filter(|h| h != REACTIVE_PLACEHOLDER_HASH)
                .unwrap_or_else(|| format!("{id}__static_tmpl"));
            intern_static_template(&key, &el);
            *intern_key.borrow_mut() = Some(key);
        }
        el
    })
}

/// Solid-style `<Switch>`: read `key` inside a [`reactive_view`], then `render(&key)`.
///
/// Rebuilds only when `key` changes (`PartialEq`). Prefer over nested [`show_reactive`] for
/// multi-arm branches.
pub fn switch_reactive<K, F>(
    anchor_id: impl Into<String>,
    mut key: impl FnMut() -> K + 'static,
    mut render: F,
) -> Element
where
    K: PartialEq + Clone + 'static,
    F: FnMut(&K) -> Element + 'static,
{
    let id = anchor_id.into();
    reactive_view(id, move || {
        let k = key();
        render(&k)
    })
}

/// Solid-style `<Match>` / `<MatchArm when>`: first true `when` wins; all false → empty or fallback.
///
/// Re-evaluates inside a dedicated [`reactive_view`] so `when={…get()}` does not subscribe the
/// root / component boundary. Prefer this over nested [`show_reactive`] for multi-arm branches
/// that are boolean (key-based arms still use [`switch_reactive`]).
pub fn match_when_reactive<F>(anchor_id: impl Into<String>, mut pick: F) -> Element
where
    F: FnMut() -> Element + 'static,
{
    reactive_view(anchor_id, move || pick())
}

/// In-place `attrs[key] = value` for `ui!` auto-bind of otherwise unknown attributes.
pub fn apply_bound_attr(el: &mut Element, key: &str, value: String) -> LayoutStatus {
    match el.attrs.get(key) {
        Some(old) if old == &value => LayoutStatus::Clean,
        _ => {
            el.set_attr(key, value);
            LayoutStatus::StyleChanged
        }
    }
}

/// Fine-grained `animation-play-state` update (pause/resume without rebuilding the entry).
pub fn apply_bound_animation_play_state(el: &mut Element, value: String) -> LayoutStatus {
    use crate::style::AnimationPlayState;
    let states: Vec<AnimationPlayState> = value
        .split(',')
        .filter_map(|s| AnimationPlayState::from_css(s.trim()))
        .collect();
    if states.is_empty() {
        return LayoutStatus::Clean;
    }
    while el.style.animations.len() < states.len().max(1) {
        el.style
            .animations
            .push(crate::style::AnimationEntry::default());
    }
    let mut changed = false;
    for (i, s) in states.into_iter().enumerate() {
        let idx = i.min(el.style.animations.len() - 1);
        if el.style.animations[idx].play_state != s {
            el.style.animations[idx].play_state = s;
            changed = true;
        }
    }
    if changed {
        LayoutStatus::StyleChanged
    } else {
        LayoutStatus::Clean
    }
}

/// Fine-grained `z-index` update (`ui!` `z_index={…get()…}` ≡ Builder `.z_index(n)` + bind).
pub fn apply_bound_z_index(el: &mut Element, z: i32) -> LayoutStatus {
    if el.style.z_index == z {
        LayoutStatus::Clean
    } else {
        el.style.z_index = z;
        LayoutStatus::StyleChanged
    }
}

/// **降级** whole-list `<For>`: re-runs `render_list` then keyed Element reconcile.
///
/// Tracked `<For each={…get()}>` without `key` takes this path and logs a debug warn
/// (WP-N.4). Prefer [`for_keyed_reactive`] (item-level owners, I-7) or [`index_reactive`].
pub fn for_reactive<F>(anchor_id: impl Into<String>, mut render_list: F) -> Element
where
    F: FnMut() -> Vec<Element> + 'static,
{
    let id = anchor_id.into();
    let warn_id = id.clone();
    reactive_view(id, move || {
        note_unkeyed_for_rebuild(&warn_id);
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

/// **降级** index-keyed snapshot (`{scope_id}:{index}`). Not Solid `<Index>`.
///
/// Useful as a lower-boilerplate `<For each={...}>` fallback when explicit keys are omitted.
/// Prefer [`index_reactive`] for I-11 (per-index Accessor) and [`for_keyed_reactive`] when
/// items have stable keys.
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

pub use super::keyed_list::{
    for_keyed_reactive, for_keyed_reactive_with_fallback, index_reactive,
    keyed_list_item_owner_count,
};

/// Iterator variant for index-keyed list rendering.
pub fn for_each_iter<I, T, RF>(scope_id: impl Into<String>, items: I, render: RF) -> Vec<Element>
where
    I: IntoIterator<Item = T>,
    RF: FnMut(&T) -> Element,
{
    let items_vec: Vec<T> = items.into_iter().collect();
    for_each(scope_id, &items_vec, render)
}

impl<T: Clone + std::fmt::Display + 'static> IntoNodeText for Signal<T> {
    fn apply_node_text(self, el: Element) -> Element {
        let id = format!("__rui_text_sig:{:?}", self.id());
        bind_prop(
            el,
            id,
            move || self.get().to_string(),
            apply_bound_text_value,
        )
    }
}

impl<F> IntoNodeText for F
where
    F: FnMut() -> String + 'static,
{
    #[track_caller]
    fn apply_node_text(self, el: Element) -> Element {
        let loc = std::panic::Location::caller();
        let id = el
            .template_id
            .clone()
            .unwrap_or_else(|| format!("{}:{}:{}:bind_text", loc.file(), loc.line(), loc.column()));
        bind_prop(el, id, self, apply_bound_text_value)
    }
}

#[track_caller]
fn caller_bind_key(kind: &str) -> String {
    let loc = std::panic::Location::caller();
    format!("{}:{}:{}:bind_{kind}", loc.file(), loc.line(), loc.column())
}

impl<T: Clone + std::fmt::Display + 'static> IntoNodeClass for Signal<T> {
    fn apply_node_class(self, el: Element) -> Element {
        let id = format!("__rui_class_sig:{:?}", self.id());
        bind_class(el, id, move || self.get().to_string())
    }
}

impl<F, S> IntoNodeClass for F
where
    F: FnMut() -> S + 'static,
    S: Into<String>,
{
    #[track_caller]
    fn apply_node_class(self, el: Element) -> Element {
        let mut compute = self;
        bind_class(el, caller_bind_key("class"), move || compute().into())
    }
}

impl<T: Clone + IntoCssDim + 'static> IntoNodeSize for Signal<T> {
    fn apply_node_size(self, el: Element, axis: &'static str) -> Element {
        let id = format!("__rui_size_{axis}_sig:{:?}", self.id());
        bind_prop(
            el,
            id,
            move || self.get(),
            move |el, dim| apply_bound_size(el, axis, dim),
        )
    }
}

impl<F, D> IntoNodeSize for F
where
    F: FnMut() -> D + 'static,
    D: IntoCssDim + 'static,
{
    #[track_caller]
    fn apply_node_size(self, el: Element, axis: &'static str) -> Element {
        bind_prop(
            el,
            caller_bind_key(&format!("size_{axis}")),
            self,
            move |el, dim| apply_bound_size(el, axis, dim),
        )
    }
}

impl<T: Clone + IntoPaintColor + 'static> IntoNodeBg for Signal<T> {
    fn apply_node_bg(self, el: Element) -> Element {
        let id = format!("__rui_bg_sig:{:?}", self.id());
        bind_prop(
            el,
            id,
            move || self.get().into_paint_color(),
            |el, color| crate::style::apply_user_bg(el, color),
        )
    }
}

impl<F, C> IntoNodeBg for F
where
    F: FnMut() -> C + 'static,
    C: IntoPaintColor + 'static,
{
    #[track_caller]
    fn apply_node_bg(self, el: Element) -> Element {
        let mut compute = self;
        bind_prop(
            el,
            caller_bind_key("bg"),
            move || compute().into_paint_color(),
            |el, color| crate::style::apply_user_bg(el, color),
        )
    }
}

impl<T: Clone + IntoTextColor + 'static> IntoNodeFg for Signal<T> {
    fn apply_node_fg(self, el: Element) -> Element {
        let id = format!("__rui_fg_sig:{:?}", self.id());
        bind_prop(
            el,
            id,
            move || self.get().into_text_color(),
            |el, color| crate::style::apply_user_fg(el, color),
        )
    }
}

impl<F, C> IntoNodeFg for F
where
    F: FnMut() -> C + 'static,
    C: IntoTextColor + 'static,
{
    #[track_caller]
    fn apply_node_fg(self, el: Element) -> Element {
        let mut compute = self;
        bind_prop(
            el,
            caller_bind_key("fg"),
            move || compute().into_text_color(),
            |el, color| crate::style::apply_user_fg(el, color),
        )
    }
}

impl<T: Clone + IntoOpacity + 'static> IntoNodeOpacity for Signal<T> {
    fn apply_node_opacity(self, el: Element) -> Element {
        let id = format!("__rui_opacity_sig:{:?}", self.id());
        bind_prop(
            el,
            id,
            move || self.get(),
            |el, o| apply_bound_opacity(el, o),
        )
    }
}

impl<F, O> IntoNodeOpacity for F
where
    F: FnMut() -> O + 'static,
    O: IntoOpacity + 'static,
{
    #[track_caller]
    fn apply_node_opacity(self, el: Element) -> Element {
        let mut compute = self;
        bind_prop(
            el,
            caller_bind_key("opacity"),
            move || compute(),
            |el, o| apply_bound_opacity(el, o),
        )
    }
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

    #[test]
    fn hard_newline_grows_layout_height() {
        let el = label("a");
        let before = text_layout_height(&el, "a", f32::MAX).expect("measurable");
        let after = text_layout_height(&el, "a\nb", f32::MAX).expect("measurable");
        assert!(
            after > before + 0.5,
            "hard newline must grow height for bind_text LayoutChanged: {before} vs {after}"
        );
    }

    #[test]
    fn hard_newline_shrink_requests_layout() {
        let el = label("a\nb\nc");
        assert!(
            bind_text_metric_needs_layout(&el, "a\nb\nc", "a"),
            "deleting hard lines must LayoutChanged so echo Labels collapse"
        );
        let same = label("10");
        assert!(
            !bind_text_metric_needs_layout(&same, "10", "11"),
            "equal-metric digit counters stay paint-only"
        );
    }

    #[test]
    fn padded_mouse_log_coord_tick_layout_status() {
        // Gallery MouseEvent readout: fixed 7 padded rows, stretch width.
        fn log_at(x: f32, y: f32) -> String {
            format!(
                "{:<10} detail={:<2} button={} buttons={:<3} which={}\n\
client=({:>7.1},{:>7.1})  offset=({:>7.1},{:>7.1})\n\
page=({:>7.1},{:>7.1})    screen=({:>7.1},{:>7.1})\n\
movement=({:>7.1},{:>7.1})\n\
Alt={:<5} Ctrl={:<5} Shift={:<5} Meta={:<5} Accel={}\n\
region={}  related={}\n\
phase={:<8}  stop={}  prevent={}",
                "mousemove",
                0,
                0,
                0,
                1,
                x,
                y,
                x,
                y,
                x,
                y,
                x,
                y,
                1.7,
                0.9,
                false,
                false,
                false,
                false,
                false,
                format!("{:<18}", "-"),
                format!("{:<18}", "gal-mouse-pad"),
                "Target",
                false,
                false,
            )
        }
        let mut el = label("").w("100%");
        el.text_style.size = Some(12.0);
        el.text_style.line_height_scale = Some(1.5);
        el.layout_bounds = [12.0, 80.0, 400.0, 126.0];
        let a = log_at(410.0, 82.0);
        let b = log_at(411.7, 82.9);
        assert_eq!(a.len(), b.len(), "padded fields must keep byte length");
        let t0 = std::time::Instant::now();
        let mut needs_any = false;
        for i in 0..80 {
            let x = 410.0 + i as f32 * 1.7;
            let y = 82.0 + i as f32 * 0.9;
            needs_any |=
                bind_text_metric_needs_layout(&el, &log_at(x, y), &log_at(x + 1.7, y + 0.9));
        }
        let metric_ms = t0.elapsed().as_secs_f32() * 1000.0;
        assert!(
            !needs_any,
            "definite-width padded coords must stay paint-only"
        );
        assert!(
            metric_ms < 5.0,
            "pointer path must not cosmic-prepare every mousemove ({metric_ms:.2}ms / 80 ticks)"
        );
        let mut node = label("").w("100%");
        node.text_style.size = Some(12.0);
        node.text_style.line_height_scale = Some(1.5);
        node.layout_bounds = el.layout_bounds;
        node.text = Some(a);
        let status = apply_bound_text_value(&mut node, b);
        assert_eq!(status, LayoutStatus::StyleChanged);
    }

    #[test]
    fn specified_width_play_pause_stays_paint_only() {
        // Gallery play-state: "Pause"/"Play" different byte lengths, but the used
        // box is authored. Must not LayoutChanged (root Taffy hitch sibling CSS).
        let mut el = label("Pause").w(76.0);
        el.layout_bounds = [8.0, 8.0, 76.0, 28.0];
        assert!(
            !bind_text_metric_needs_layout(&el, "Pause", "Play"),
            "definite-width Play/Pause must stay paint-only"
        );
        let status = apply_bound_text_value(&mut el, "Play".into());
        assert_eq!(status, LayoutStatus::StyleChanged);
    }

    #[test]
    fn specified_width_go_reset_stays_paint_only() {
        let mut el = label("Go").w("100%");
        el.layout_bounds = [8.0, 8.0, 68.0, 28.0];
        let t0 = std::time::Instant::now();
        let mut needs = false;
        for _ in 0..80 {
            needs |= bind_text_metric_needs_layout(&el, "Go", "Reset");
            needs |= bind_text_metric_needs_layout(&el, "Reset", "Go");
        }
        let metric_ms = t0.elapsed().as_secs_f32() * 1000.0;
        assert!(!needs, "fixed-width Go/Reset must stay paint-only");
        assert!(
            metric_ms < 5.0,
            "Go/Reset must not cosmic-prepare on click ({metric_ms:.2}ms / 160 ticks)"
        );
        let status = apply_bound_text_value(&mut el, "Reset".into());
        assert_eq!(status, LayoutStatus::StyleChanged);
    }

    #[test]
    fn bind_layout_absorb_min_w_button_is_absorbed() {
        let mut text = Element::new().text("Pause");
        text.layout_bounds = [8.0, 80.0, 48.0, 20.0];
        let mut btn = Element::new().min_w(76.0).child(text);
        btn.layout_bounds = [8.0, 72.0, 76.0, 28.0];
        let mut root = Element::new().w("100%").child(btn);
        root.layout_bounds = [0.0, 0.0, 320.0, 200.0];
        assert!(
            bind_layout_absorbed_by_specified_ancestor(&root, &[0, 0]),
            "min_w Button on the bind_text path must absorb leaf LayoutChanged"
        );
        let mut chip = Element::new().text("Blocked");
        chip.layout_bounds = [8.0, 8.0, 64.0, 20.0];
        let mut page = Element::new().w("100%").child(chip);
        page.layout_bounds = [0.0, 0.0, 320.0, 200.0];
        assert!(
            !bind_layout_absorbed_by_specified_ancestor(&page, &[0]),
            "content-sized badge under % width page must still LayoutChanged"
        );
    }

    #[test]
    fn bind_layout_absorb_wrap_row_without_height_still_layouts() {
        let mut text = Element::new().text("hi");
        text.layout_bounds = [0.0, 0.0, 40.0, 16.0];
        let mut row = Element::new().w(320.0).child(text);
        row.layout_bounds = [0.0, 0.0, 320.0, 0.0];
        let mut page = Element::new().w("100%").child(row);
        page.layout_bounds = [0.0, 0.0, 320.0, 200.0];
        assert!(
            !bind_layout_absorbed_by_specified_ancestor(&page, &[0, 0]),
            "% / wrap row with no laid-out height must not swallow leaf LayoutChanged (footer Y)"
        );
    }

    #[test]
    fn shorter_label_shrink_requests_layout() {
        // Gallery ReactiveStatusBadge: "Blocked" → "Ready" must collapse chip width.
        let el = label("Blocked");
        let before = text_intrinsic_width(&el, "Blocked").expect("before");
        let after = text_intrinsic_width(&el, "Ready").expect("after");
        assert!(
            before > after + 0.5,
            "fixture must shrink: {before} → {after}"
        );
        assert!(
            bind_text_metric_needs_layout(&el, "Blocked", "Ready"),
            "width shrink must LayoutChanged (not paint-only StyleChanged)"
        );
        assert!(
            bind_text_metric_needs_layout(&el, "Ready", "Blocked"),
            "width grow must still LayoutChanged"
        );
    }

    #[test]
    fn switch_reactive_builds_placeholder() {
        use crate::core::reactivity::{create_effect, create_root, create_signal};
        let (_o, tid) = create_root(|| {
            let key = create_signal(0_i32);
            let out = std::rc::Rc::new(std::cell::RefCell::new(None::<String>));
            let out2 = out.clone();
            create_effect(move || {
                let el = switch_reactive(
                    "sw-test",
                    move || key.get(),
                    |k| Element::new().text(format!("arm-{k}")),
                );
                *out2.borrow_mut() = el.template_id.clone();
            });
            let tid = out.borrow().clone().expect("id");
            tid
        });
        assert_eq!(tid, "sw-test");
    }

    #[test]
    fn match_when_reactive_builds_placeholder() {
        use crate::core::reactivity::{create_effect, create_root, create_signal};
        let (_o, tid) = create_root(|| {
            let ok = create_signal(true);
            let out = std::rc::Rc::new(std::cell::RefCell::new(None::<String>));
            let out2 = out.clone();
            create_effect(move || {
                let el = match_when_reactive("match-test", move || {
                    if ok.get() {
                        Element::new().text("yes")
                    } else {
                        Element::new().text("no")
                    }
                });
                *out2.borrow_mut() = el.template_id.clone();
            });
            let tid = out.borrow().clone().expect("id");
            tid
        });
        assert_eq!(tid, "match-test");
    }

    #[test]
    fn element_text_signal_and_closure_bind() {
        use crate::core::reactivity::{create_effect, create_root, use_signal};
        reset_registered_anchors();
        let (_o, (sig_id, clos_id, snap_id)) = create_root(|| {
            let sig = use_signal("__rui_into_node_text", || "live".to_string());
            sig.set_silent("live".to_string());
            let out = std::rc::Rc::new(std::cell::RefCell::new(
                None::<(Option<String>, Option<String>, Option<String>)>,
            ));
            let out2 = out.clone();
            create_effect(move || {
                let bound = Element::new().text(sig);
                let clos = Element::new().text(move || sig.get());
                let snap = Element::new().text(sig.get());
                *out2.borrow_mut() = Some((
                    bound.template_id.clone(),
                    clos.template_id.clone(),
                    snap.template_id.clone(),
                ));
            });
            let ids = out.borrow().clone().expect("ids");
            ids
        });
        assert!(
            sig_id
                .as_deref()
                .is_some_and(|id| id.contains("__rui_text_sig")),
            "Element::text(sig) must bind; id={sig_id:?}"
        );
        assert!(
            clos_id.is_some(),
            "Element::text(|| sig.get()) must bind; id={clos_id:?}"
        );
        assert!(
            snap_id.is_none(),
            "Element::text(sig.get()) is a snapshot; id={snap_id:?}"
        );
        reset_registered_anchors();
    }

    #[test]
    fn reactive_anchor_registers_independently_per_window_scope() {
        use crate::app::window_scope::{enter, WindowScope};
        use crate::core::reactivity::{
            bind_current_owner_window_key_scope, create_effect, create_root, create_signal,
        };

        reset_registered_anchors();
        let before = registered_anchor_count();

        let (_o1, _) = create_root(|| {
            let _g = enter(WindowScope::Primary);
            bind_current_owner_window_key_scope(0);
            let open = create_signal(false);
            create_effect(move || {
                let _g = enter(WindowScope::Primary);
                reactive_anchor("shared_dialog_view", move || {
                    let _ = open.get();
                    Element::new().text("primary")
                });
            });
        });
        assert_eq!(registered_anchor_count(), before + 1);

        let (_o2, _) = create_root(|| {
            bind_current_owner_window_key_scope(7);
            let open = create_signal(false);
            create_effect(move || {
                let _g = enter(WindowScope::Secondary(7));
                reactive_anchor("shared_dialog_view", move || {
                    let _ = open.get();
                    Element::new().text("secondary")
                });
            });
        });
        assert_eq!(
            registered_anchor_count(),
            before + 2,
            "same anchor id must register once per window scope"
        );
        reset_registered_anchors();
    }
}

#[cfg(test)]
mod in_flow_placeholder_tests {
    use super::*;
    use crate::layout::layout_tree::LayoutTree;
    use crate::primitives::vstack;
    use taffy::style::Display;

    fn neighbor_gap_in_column(middle: Element) -> f32 {
        fn find_tid<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
            if el.template_id.as_deref() == Some(id) {
                Some(el)
            } else {
                el.children.iter().find_map(|c| find_tid(c, id))
            }
        }

        let mut a = Element::new().w(20.0).h(20.0);
        a.template_id = Some("gap-a".into());
        let mut b = Element::new().w(20.0).h(20.0);
        b.template_id = Some("gap-b".into());
        let mut col: Element = vstack().gap(8.0).child(a).child(middle).child(b).into();
        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut col, 80.0, 200.0, [0.0, 0.0]);
        let aa = find_tid(&col, "gap-a")
            .unwrap_or_else(|| panic!("missing gap-a"))
            .layout_bounds;
        let bb = find_tid(&col, "gap-b")
            .unwrap_or_else(|| panic!("missing gap-b"))
            .layout_bounds;
        bb[1] - (aa[1] + aa[3])
    }

    #[test]
    fn display_none_placeholder_does_not_occupy_flex_gap() {
        let in_flow = neighbor_gap_in_column(Element::new().w(0.0).h(0.0));
        let hidden = neighbor_gap_in_column(reactive_placeholder_in_flow("hide-gap", false));
        let visible = neighbor_gap_in_column(reactive_placeholder_in_flow("show-gap", true));
        assert!(
            (in_flow - 16.0).abs() < 1.0,
            "static 0×0 still takes a gap slot (expected ~16, got {in_flow})"
        );
        assert!(
            (hidden - 8.0).abs() < 1.0,
            "display:none must not occupy gap (expected ~8, got {hidden})"
        );
        assert!(
            (visible - 16.0).abs() < 1.0,
            "visible 0×0 still takes a gap slot (expected ~16, got {visible})"
        );
        assert_eq!(
            reactive_placeholder_in_flow("closed", false).style.display,
            Display::None
        );
        assert_ne!(
            reactive_placeholder_in_flow("open", true).style.display,
            Display::None
        );
    }
}
