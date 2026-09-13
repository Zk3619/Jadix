//! WP-R.5 keyed / index list reactivity (I-7 / I-11).
//!
//! - [`for_keyed_reactive`]: one child [`OwnerHandle`] per key; list changes only insert / remove /
//!   reorder item owners; unchanged keys do not re-run item `render`.
//! - [`index_reactive`]: fixed index identity; per-index [`Signal`] accessor; same-index value
//!   updates propagate through `bind_*` without rebuilding the item subtree.

use std::any::Any;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::rc::Rc;

use crate::core::reactivity::{
    create_effect, create_root, on_cleanup, use_signal, OwnerHandle, Signal,
};
use crate::core::tree::{Element, UnmountHook};

use super::reactive::{
    reactive_anchor, reactive_placeholder, reactive_placeholder_sized, reactive_view,
    reactive_view_sized, window_scoped_registry_key,
};

thread_local! {
    static KEYED_LIST_REGISTRY: RefCell<HashMap<String, KeyedListRegistryEntry>> =
        RefCell::new(HashMap::new());
    static INDEX_LIST_REGISTRY: RefCell<HashMap<String, Box<dyn FnMut()>>> =
        RefCell::new(HashMap::new());
    static INDEX_STATES: RefCell<HashMap<String, Box<dyn Any>>> = RefCell::new(HashMap::new());
}

struct KeyedItemSlot {
    owner: OwnerHandle,
    /// Extra strong refs so dropping a headless fragment cannot dispose item `bind_*` effects.
    keepalive: Rc<RefCell<Vec<Rc<UnmountHook>>>>,
}

struct KeyedListRegistryEntry {
    items: HashMap<String, KeyedItemSlot>,
}

impl KeyedListRegistryEntry {
    fn dispose_all(&mut self) {
        for (_, slot) in self.items.drain() {
            slot.owner.dispose();
            drop(slot.keepalive);
        }
    }
}

struct IndexSlot<T> {
    owner: OwnerHandle,
    item: Signal<T>,
    keepalive: Rc<RefCell<Vec<Rc<UnmountHook>>>>,
}

struct IndexListState<T: Clone + PartialEq + 'static> {
    scope_id: String,
    slots: Vec<IndexSlot<T>>,
}

impl<T: Clone + PartialEq + 'static> IndexListState<T> {
    fn dispose_all(&mut self) {
        for slot in self.slots.drain(..) {
            slot.owner.dispose();
            drop(slot.keepalive);
        }
    }

    fn sync<RF>(&mut self, list: &[T], render: Rc<RefCell<RF>>)
    where
        RF: FnMut(usize, Signal<T>) -> Element + 'static,
    {
        while self.slots.len() < list.len() {
            let index = self.slots.len();
            let signal_key = format!("{}__val", index_item_id(&self.scope_id, index));
            let item_signal = use_signal(signal_key.as_str(), || list[index].clone());
            let anchor_id = index_item_id(&self.scope_id, index);
            let render = Rc::clone(&render);
            let keepalive = Rc::new(RefCell::new(Vec::new()));
            let keepalive_for_effect = Rc::clone(&keepalive);
            let (owner, _) = create_root(move || {
                create_effect(move || {
                    let inner = render.borrow_mut()(index, item_signal);
                    *keepalive_for_effect.borrow_mut() = retain_unmount_hooks(&inner);
                    let cache = Rc::new(RefCell::new(Some(wrap_item_fragment(inner))));
                    reactive_anchor(anchor_id.clone(), move || take_cached_element(&cache));
                });
            });
            self.slots.push(IndexSlot {
                owner,
                item: item_signal,
                keepalive,
            });
        }

        while self.slots.len() > list.len() {
            let slot = self.slots.pop().expect("len > 0");
            slot.owner.dispose();
            drop(slot.keepalive);
        }

        for (i, value) in list.iter().enumerate() {
            if self.slots[i].item.peek() != *value {
                self.slots[i].item.set(value.clone());
            }
        }
    }
}

fn index_item_id(scope_id: &str, index: usize) -> String {
    format!("{scope_id}:{index}")
}

fn keyed_item_id(scope_id: &str, key: &str) -> String {
    format!("{scope_id}:{key}")
}

fn take_cached_element(cache: &Rc<RefCell<Option<Element>>>) -> Element {
    cache
        .borrow_mut()
        .take()
        .expect("item fragment already taken")
}

fn retain_unmount_hooks(el: &Element) -> Vec<Rc<UnmountHook>> {
    let mut out = Vec::new();
    collect_unmount_hooks(el, &mut out);
    out
}

fn collect_unmount_hooks(el: &Element, out: &mut Vec<Rc<UnmountHook>>) {
    crate::core::tree::walk_elements(el, |el| {
        if let Some(hook) = &el.on_unmount {
            out.push(Rc::clone(hook));
        }
    });
}

/// Keep item `bind_*` `template_id`s intact: `reactive_anchor` overwrites the fragment root id.
fn wrap_item_fragment(inner: Element) -> Element {
    let mut root = Element::new();
    root.children.push(inner);
    root
}

fn index_state<T: Clone + PartialEq + 'static>(
    registry_key: &str,
    scope_id: &str,
) -> Rc<RefCell<IndexListState<T>>> {
    INDEX_STATES.with(|map| {
        let mut map = map.borrow_mut();
        if let Some(stored) = map.get(registry_key) {
            return stored
                .downcast_ref::<Rc<RefCell<IndexListState<T>>>>()
                .expect("index_reactive type mismatch for list id")
                .clone();
        }
        let state = Rc::new(RefCell::new(IndexListState {
            scope_id: scope_id.to_string(),
            slots: Vec::new(),
        }));
        map.insert(registry_key.to_string(), Box::new(Rc::clone(&state)));
        state
    })
}

fn mount_keyed_item<T, RF>(
    item_anchor_id: String,
    item: T,
    render: RF,
) -> (OwnerHandle, Rc<RefCell<Vec<Rc<UnmountHook>>>>)
where
    T: Clone + 'static,
    RF: FnOnce(&T) -> Element + 'static,
{
    let anchor = item_anchor_id.clone();
    let keepalive = Rc::new(RefCell::new(Vec::new()));
    let keepalive_for_effect = Rc::clone(&keepalive);
    let (owner, _) = create_root(move || {
        let mut render = Some(render);
        create_effect(move || {
            let inner = render.take().expect("keyed item render runs once")(&item);
            *keepalive_for_effect.borrow_mut() = retain_unmount_hooks(&inner);
            let cache = Rc::new(RefCell::new(Some(wrap_item_fragment(inner))));
            reactive_anchor(anchor.clone(), move || take_cached_element(&cache));
        });
    });
    (owner, keepalive)
}

fn sync_keyed_list<T, K, KF, RF>(
    registry_key: &str,
    scope_id: &str,
    items: &[T],
    key_of: &mut KF,
    render: Rc<RefCell<RF>>,
) where
    T: Clone + 'static,
    K: ToString + Eq + Hash,
    KF: FnMut(&T) -> K,
    RF: FnMut(&T) -> Element + 'static,
{
    KEYED_LIST_REGISTRY.with(|registry| {
        let mut registry = registry.borrow_mut();
        if !registry.contains_key(registry_key) {
            let cleanup_key = registry_key.to_string();
            on_cleanup(move || {
                KEYED_LIST_REGISTRY.with(|registry| {
                    if let Some(mut entry) = registry.borrow_mut().remove(&cleanup_key) {
                        entry.dispose_all();
                    }
                });
            });
            registry.insert(
                registry_key.to_string(),
                KeyedListRegistryEntry {
                    items: HashMap::new(),
                },
            );
        }
        let entry = registry.get_mut(registry_key).expect("just inserted");

        let new_keys: Vec<String> = items
            .iter()
            .map(|item| keyed_item_id(scope_id, &key_of(item).to_string()))
            .collect();
        let new_set: HashSet<&str> = new_keys.iter().map(String::as_str).collect();

        let removed: Vec<String> = entry
            .items
            .keys()
            .filter(|k| !new_set.contains(k.as_str()))
            .cloned()
            .collect();
        for key in removed {
            if let Some(slot) = entry.items.remove(&key) {
                slot.owner.dispose();
                drop(slot.keepalive);
            }
        }

        for item in items {
            let key = keyed_item_id(scope_id, &key_of(item).to_string());
            if entry.items.contains_key(&key) {
                continue;
            }
            let render = Rc::clone(&render);
            let (owner, keepalive) =
                mount_keyed_item(key.clone(), item.clone(), move |t| render.borrow_mut()(t));
            entry.items.insert(key, KeyedItemSlot { owner, keepalive });
        }
    });
}

/// Solid-style keyed `<For>` with item-level child owners (I-7 / WP-R.5b).
///
/// Reads `list_source` inside a dedicated [`reactive_view`]. Inserts allocate one
/// [`OwnerHandle`] per key and run `render` once; removals dispose that owner; reorders only
/// rearrange placeholders (unchanged keys do not re-run `render`).
///
/// Prefer `bind_*` inside `render` for per-item live fields. Replacing an item while keeping
/// the same key is intentionally out of scope (Solid keyed For identity = key).
///
/// [`for_reactive`](super::reactive::for_reactive) + [`for_keyed`](super::reactive::for_keyed)
/// remain the **降级** whole-list rebuild path (Element reconcile only).
pub fn for_keyed_reactive<T, K, LS, KF, RF>(
    anchor_id: impl Into<String>,
    list_source: LS,
    key_of: KF,
    render: RF,
) -> Element
where
    T: Clone + 'static,
    K: ToString + Eq + Hash,
    LS: FnMut() -> Vec<T> + 'static,
    KF: FnMut(&T) -> K + 'static,
    RF: FnMut(&T) -> Element + 'static,
{
    for_keyed_reactive_with_fallback(anchor_id, list_source, key_of, render, Vec::new)
}

/// [`for_keyed_reactive`] plus an empty-list fallback subtree (RSX `<For fallback={…}>`).
pub fn for_keyed_reactive_with_fallback<T, K, LS, KF, RF, FF>(
    anchor_id: impl Into<String>,
    mut list_source: LS,
    mut key_of: KF,
    render: RF,
    mut fallback: FF,
) -> Element
where
    T: Clone + 'static,
    K: ToString + Eq + Hash,
    LS: FnMut() -> Vec<T> + 'static,
    KF: FnMut(&T) -> K + 'static,
    RF: FnMut(&T) -> Element + 'static,
    FF: FnMut() -> Vec<Element> + 'static,
{
    let list_id = anchor_id.into();
    let registry_key = window_scoped_registry_key(&list_id);
    let scope_id = format!("{list_id}:items");
    let scope_for_effect = scope_id.clone();
    let registry_for_effect = registry_key.clone();
    let render = Rc::new(RefCell::new(render));

    reactive_view_sized(list_id.clone(), "100%", "max-content", move || {
        let items = list_source();
        sync_keyed_list(
            &registry_for_effect,
            &scope_for_effect,
            &items,
            &mut key_of,
            Rc::clone(&render),
        );

        let mut root = Element::new();
        if items.is_empty() {
            root.children = fallback();
        } else {
            root.children = items
                .iter()
                .map(|item| {
                    let key = keyed_item_id(&scope_for_effect, &key_of(item).to_string());
                    reactive_placeholder_sized(key, "100%", "max-content")
                })
                .collect();
        }
        root
    })
}

/// Solid-style `<Index>`: index-fixed rows with per-index accessor signals (I-11 / WP-R.5c).
///
/// `render(index, item_signal)` runs once when index `index` first appears. Later updates to
/// `list_source()[index]` only `.set` the accessor signal so `bind_*` on that row re-run.
///
/// [`for_each`](super::reactive::for_each) remains index-as-`template_id` (not Index).
pub fn index_reactive<T, LS, RF>(
    anchor_id: impl Into<String>,
    mut list_source: LS,
    render: RF,
) -> Element
where
    T: Clone + PartialEq + 'static,
    LS: FnMut() -> Vec<T> + 'static,
    RF: FnMut(usize, Signal<T>) -> Element + 'static,
{
    let list_id = anchor_id.into();
    let registry_key = window_scoped_registry_key(&list_id);
    let scope_id = format!("{list_id}:index");
    let state = index_state::<T>(&registry_key, &scope_id);
    let render = Rc::new(RefCell::new(render));

    INDEX_LIST_REGISTRY.with(|registry| {
        let mut registry = registry.borrow_mut();
        if !registry.contains_key(&registry_key) {
            let cleanup_key = registry_key.clone();
            on_cleanup({
                let cleanup_key = cleanup_key.clone();
                move || {
                    INDEX_LIST_REGISTRY.with(|registry| {
                        if let Some(mut dispose) = registry.borrow_mut().remove(&cleanup_key) {
                            dispose();
                        }
                    });
                }
            });
            let state_for_cleanup = Rc::clone(&state);
            registry.insert(
                registry_key.clone(),
                Box::new(move || {
                    state_for_cleanup.borrow_mut().dispose_all();
                    INDEX_STATES.with(|map| {
                        map.borrow_mut().remove(&cleanup_key);
                    });
                }),
            );
        }
    });

    let state_for_effect = Rc::clone(&state);
    let render_for_effect = Rc::clone(&render);
    reactive_view(list_id.clone(), move || {
        let list = list_source();
        state_for_effect
            .borrow_mut()
            .sync(&list, Rc::clone(&render_for_effect));

        let scope = state_for_effect.borrow().scope_id.clone();
        let mut root = Element::new();
        root.children = (0..list.len())
            .map(|i| reactive_placeholder(index_item_id(&scope, i)))
            .collect();
        root
    })
}

/// Number of mounted keyed-list item owners (diagnostics / WP-R.5 tests).
pub fn keyed_list_item_owner_count() -> usize {
    KEYED_LIST_REGISTRY.with(|registry| {
        registry
            .borrow()
            .values()
            .map(|entry| entry.items.len())
            .sum()
    })
}

/// Drop keyed/index list registries (called from [`super::reactive::reset_registered_anchors`]).
pub(crate) fn reset_keyed_list_registry() {
    KEYED_LIST_REGISTRY.with(|registry| {
        for (_, mut entry) in registry.borrow_mut().drain() {
            entry.dispose_all();
        }
    });
    INDEX_LIST_REGISTRY.with(|registry| {
        for (_, mut dispose) in registry.borrow_mut().drain() {
            dispose();
        }
    });
    INDEX_STATES.with(|map| map.borrow_mut().clear());
}
