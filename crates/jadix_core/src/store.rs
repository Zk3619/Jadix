//! Store with path triggers (WP-R.3 / I-8).
//!
//! - [`Store::field`] / [`store_field`]: named path node. Sibling `set` does **not** re-run
//!   sibling compute (Solid proxy / Leptos `reactive_stores` idea; no JS Proxy in Element).
//! - [`store_select`] + [`Store::produce`]: transitional whole-root path. Root `produce` still
//!   re-runs every select closure; slice [`PartialEq`] gates downstream effects (WP-2.5).
//!   **G.3**：业务代码用 [`Store::field`]；仓内零 `store_select` 调用（仅本模块测试 + 本 API）。
//!   Whole-root writes compare registered field projections before notifying them.

use std::any::TypeId;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::ops::Bound;
use std::rc::Rc;

use super::reactivity::{
    batch, create_memo, create_signal, current_owner, on_cleanup, untrack, with_owner, Memo,
    OwnerId, Signal, SignalId,
};

thread_local! {
    static STORE_PATHS: RefCell<HashMap<SignalId, StorePathTable>> =
        RefCell::new(HashMap::new());
}

struct StorePathTable {
    owner: OwnerId,
    paths: BTreeMap<String, StorePath>,
}

#[derive(Clone)]
struct StorePath {
    trigger: Signal<u32>,
    value_type: TypeId,
    refresh: Rc<dyn Fn() -> bool>,
}

fn register_path<T, U>(root: Signal<T>, path: &str, get: Rc<dyn Fn(&T) -> U>) -> Signal<u32>
where
    T: Clone + 'static,
    U: Clone + PartialEq + 'static,
{
    let (owner, existing) = STORE_PATHS.with(|map| {
        let map = map.borrow();
        let table = map.get(&root.id()).expect("Store::field on disposed store");
        (table.owner, table.paths.get(path).cloned())
    });
    if let Some(existing) = existing {
        assert_eq!(
            existing.value_type,
            TypeId::of::<U>(),
            "Store path type mismatch"
        );
        return existing.trigger;
    }
    let previous = RefCell::new(untrack(|| root.with(|value| get(value))));
    let trigger = with_owner(owner, || create_signal(0u32));
    let refresh = Rc::new(move || {
        let next = root.with(|value| get(value));
        let mut previous = previous.borrow_mut();
        if *previous == next {
            false
        } else {
            *previous = next;
            true
        }
    });
    STORE_PATHS.with(|map| {
        map.borrow_mut()
            .get_mut(&root.id())
            .expect("store disposed")
            .paths
            .insert(
                path.to_string(),
                StorePath {
                    trigger,
                    value_type: TypeId::of::<U>(),
                    refresh,
                },
            );
    });
    trigger
}

/// Compare all paths for a root write, or only overlapping dotted paths for a field write.
fn notify_changed_paths(id: SignalId, path: Option<&str>) {
    let paths: Vec<StorePath> = STORE_PATHS.with(|map| {
        let map = map.borrow();
        let Some(table) = map.get(&id) else {
            return Vec::new();
        };
        let Some(path) = path else {
            return table.paths.values().cloned().collect();
        };
        let mut paths = Vec::new();
        for (boundary, _) in path.match_indices('.') {
            if let Some(entry) = table.paths.get(&path[..boundary]) {
                paths.push(entry.clone());
            }
        }
        if let Some(entry) = table.paths.get(path) {
            paths.push(entry.clone());
        }
        let prefix = format!("{path}.");
        paths.extend(
            table
                .paths
                .range::<str, _>((Bound::Included(prefix.as_str()), Bound::Unbounded))
                .take_while(|(key, _)| key.starts_with(&prefix))
                .map(|(_, entry)| entry.clone()),
        );
        paths
    });
    // Evaluate user projections outside the table borrow and without subscribing the writer.
    for entry in paths {
        if untrack(|| (entry.refresh)()) {
            entry.trigger.update(|n| *n = n.wrapping_add(1));
        }
    }
}

/// Reactive store root. Whole-value mutations go through [`Store::update`] / [`Store::set`].
/// Field-level reads/writes use [`Store::field`].
pub struct Store<T> {
    root: Signal<T>,
}

impl<T> Clone for Store<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Store<T> {}

/// A live, read-only view of a store. Mutations must use the store or its fields.
///
/// ```compile_fail
/// let (_owner, ()) = jadix_core::create_root(|| {
///     let store = jadix_core::create_store(0);
///     store.read_only().set(1);
/// });
/// ```
pub struct ReadStore<T> {
    root: Signal<T>,
}

impl<T> Clone for ReadStore<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for ReadStore<T> {}

impl<T: Clone + 'static> ReadStore<T> {
    pub fn get(&self) -> T {
        self.root.get()
    }

    pub fn try_get(&self) -> Option<T> {
        self.root.try_get()
    }

    pub fn with<R>(&self, read: impl FnOnce(&T) -> R) -> R {
        self.root.with(read)
    }

    pub fn peek(&self) -> T {
        self.root.peek()
    }

    pub fn try_peek(&self) -> Option<T> {
        self.root.try_peek()
    }

    pub fn is_alive(&self) -> bool {
        self.root.is_alive()
    }
}

impl<T: Clone + PartialEq + 'static> Store<T> {
    /// A live view that cannot bypass field notification on writes.
    pub fn read_only(&self) -> ReadStore<T> {
        ReadStore { root: self.root }
    }

    /// Compatibility read access. Use [`Self::read_only`] for new code.
    ///
    /// This now returns a read-only view; write through the store or a field.
    #[deprecated(
        since = "0.1.0",
        note = "Use Store::read_only; mutate through Store or StoreField"
    )]
    pub fn signal(&self) -> ReadStore<T> {
        self.read_only()
    }

    pub fn get(&self) -> T {
        self.root.get()
    }

    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.root.with(f)
    }

    /// Batch the root notification with only the registered paths whose projections changed.
    pub fn set(&self, value: T) {
        batch(|| {
            self.root.set(value);
            notify_changed_paths(self.root.id(), None);
        });
    }

    pub fn update(&self, f: impl FnOnce(&mut T)) {
        batch(|| {
            self.root.update(f);
            notify_changed_paths(self.root.id(), None);
        });
    }

    /// Solid-style `produce`: in-place mutable update (alias of [`Self::update`]).
    ///
    /// Compares all registered field projections, notifying only changed paths.
    /// Prefer [`Self::field`] to limit projection comparisons to overlapping paths.
    ///
    /// Transitional contract (WP-2.5): [`store_select`] memos re-run their closure; they notify
    /// downstream only when the selected slice's `PartialEq` value changes.
    ///
    /// ```ignore
    /// let store = create_store(vec![1, 2]);
    /// let len = store_select(store, |v| v.len());
    /// store.produce(|v| v.push(3)); // len observers update (2 → 3)
    /// ```
    pub fn produce(&self, f: impl FnOnce(&mut T)) {
        self.update(f);
    }

    /// Peek without tracking (Solid `unwrap` / untracked read).
    pub fn peek(&self) -> T {
        self.root.peek()
    }

    /// Named path node. `get` subscribes only to `path`; sibling paths do not share a trigger.
    /// Getters must be pure projections of the root; handles for the same path must agree
    /// on its type and projection. One cached projected value is retained per registered path.
    ///
    /// Dotted paths (`"user.name"`) notify ancestors and registered descendants on `set`.
    /// Does not copy JS Proxy into [`Element`](crate::core::tree::Element).
    pub fn field<U, G, S>(&self, path: impl Into<String>, get: G, set: S) -> StoreField<T, U>
    where
        U: Clone + PartialEq + 'static,
        G: Fn(&T) -> U + 'static,
        S: Fn(&mut T, U) + 'static,
    {
        let path = path.into();
        let get: Rc<dyn Fn(&T) -> U> = Rc::new(get);
        let trigger = register_path(self.root, &path, Rc::clone(&get));
        StoreField {
            root: self.root,
            trigger,
            path: Rc::from(path),
            get,
            set: Rc::new(set),
        }
    }
}

type FieldSetter<T, U> = Rc<dyn Fn(&mut T, U)>;

/// Fine-grained store path (I-8). Clone freely; not `Copy` (holds getters).
pub struct StoreField<T, U> {
    root: Signal<T>,
    trigger: Signal<u32>,
    path: Rc<str>,
    get: Rc<dyn Fn(&T) -> U>,
    set: FieldSetter<T, U>,
}

impl<T, U> Clone for StoreField<T, U> {
    fn clone(&self) -> Self {
        Self {
            root: self.root,
            trigger: self.trigger,
            path: Rc::clone(&self.path),
            get: Rc::clone(&self.get),
            set: Rc::clone(&self.set),
        }
    }
}

impl<T: Clone + PartialEq + 'static, U: Clone + PartialEq + 'static> StoreField<T, U> {
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Track this path only, then project from a peeked root (does not subscribe the root signal).
    pub fn get(&self) -> U {
        let _gen = self.trigger.get();
        self.peek()
    }

    pub fn peek(&self) -> U {
        untrack(|| self.root.with(|root| (self.get)(root)))
    }

    /// Compare this path and its ancestors/descendants; batch changed paths with the root.
    /// Unchanged descendants and unrelated siblings are not notified.
    pub fn set(&self, value: U) {
        if self.peek() == value {
            return;
        }
        let setter = Rc::clone(&self.set);
        batch(|| {
            self.root.update(move |root| setter(root, value));
            notify_changed_paths(self.root.id(), Some(&self.path));
        });
    }

    pub fn update(&self, f: impl FnOnce(&mut U)) {
        let mut value = self.peek();
        f(&mut value);
        self.set(value);
    }
}

/// Create a store owned by the current reactive owner.
pub fn create_store<T: Clone + PartialEq + 'static>(initial: T) -> Store<T> {
    let root = create_signal(initial);
    let id = root.id();
    STORE_PATHS.with(|map| {
        map.borrow_mut().insert(
            id,
            StorePathTable {
                owner: current_owner().expect("store owner"),
                paths: BTreeMap::new(),
            },
        );
    });
    on_cleanup(move || {
        STORE_PATHS.with(|map| {
            map.borrow_mut().remove(&id);
        });
    });
    Store { root }
}

/// Derive a fine-grained view from a store (Solid `createMemo` over a path).
///
/// Transitional: the memo still **subscribes the root**, so every [`Store::produce`] re-runs
/// `select`. Downstream effects fire only when the slice changes (`PartialEq`).
/// Prefer [`Store::field`] when sibling computes must not re-run.
#[deprecated(
    since = "0.1.0",
    note = "Use `Store::field` for path-level granularity (WP-R.3). `store_select` subscribes the root."
)]
pub fn store_select<T, U, F>(store: Store<T>, mut select: F) -> Memo<U>
where
    T: Clone + PartialEq + 'static,
    U: Clone + PartialEq + 'static,
    F: FnMut(&T) -> U + 'static,
{
    create_memo(move || store.with(|v| select(v)))
}

/// Alias of [`Store::field`].
pub fn store_field<T, U, G, S>(
    store: Store<T>,
    path: impl Into<String>,
    get: G,
    set: S,
) -> StoreField<T, U>
where
    T: Clone + PartialEq + 'static,
    U: Clone + PartialEq + 'static,
    G: Fn(&T) -> U + 'static,
    S: Fn(&mut T, U) + 'static,
{
    store.field(path, get, set)
}

/// Create a field handle with a checked Rust field path and matching setter.
///
/// ```
/// use jadix_core::{create_root, create_store, store_field};
/// #[derive(Clone, PartialEq)]
/// struct User { name: String }
/// let (_owner, ()) = create_root(|| {
///     let store = create_store(User { name: "Ada".into() });
///     let name = store_field!(store, name);
///     name.set("Grace".into());
///     assert_eq!(store.read_only().get().name, "Grace");
/// });
/// ```
#[macro_export]
macro_rules! store_field {
    ($store:expr, $first:ident $(.$rest:ident)* $(,)?) => {
        ($store).field(
            concat!(stringify!($first) $(, ".", stringify!($rest))*),
            |root| root.$first$(.$rest)*.clone(),
            |root, value| root.$first$(.$rest)* = value,
        )
    };
}

#[cfg(test)]
#[path = "tests/store.rs"]
mod tests;
