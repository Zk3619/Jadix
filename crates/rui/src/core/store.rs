//! Store with path triggers (WP-R.3 / I-8).
//!
//! - [`Store::field`] / [`store_field`]: named path node. Sibling `set` does **not** re-run
//!   sibling compute (Solid proxy / Leptos `reactive_stores` idea; no JS Proxy in Element).
//! - [`store_select`] + [`Store::produce`]: transitional whole-root path. Root `produce` still
//!   re-runs every select closure; slice [`PartialEq`] gates downstream effects (WP-2.5).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::reactivity::{create_memo, create_signal, on_cleanup, Memo, Signal, SignalId};

thread_local! {
    static STORE_PATHS: RefCell<HashMap<SignalId, StorePathTable>> =
        RefCell::new(HashMap::new());
}

struct StorePathTable {
    triggers: HashMap<String, Signal<u32>>,
}

fn ensure_path_trigger(id: SignalId, path: &str) -> Signal<u32> {
    STORE_PATHS.with(|map| {
        let mut map = map.borrow_mut();
        let table = map.entry(id).or_insert_with(|| StorePathTable {
            triggers: HashMap::new(),
        });
        *table
            .triggers
            .entry(path.to_string())
            .or_insert_with(|| create_signal(0u32))
    })
}

fn notify_all_paths(id: SignalId) {
    let signals: Vec<Signal<u32>> = STORE_PATHS.with(|map| {
        map.borrow()
            .get(&id)
            .map(|table| table.triggers.values().copied().collect())
            .unwrap_or_default()
    });
    for signal in signals {
        signal.update(|n| *n = n.wrapping_add(1));
    }
}

/// Notify `path`, its dotted ancestors, and registered descendants (`path.` prefix).
fn notify_path_tree(id: SignalId, path: &str) {
    let signals: Vec<Signal<u32>> = STORE_PATHS.with(|map| {
        let map = map.borrow();
        let Some(table) = map.get(&id) else {
            return Vec::new();
        };
        let mut keys = Vec::new();
        keys.push(path.to_string());
        let mut rest = path;
        while let Some(i) = rest.rfind('.') {
            rest = &rest[..i];
            keys.push(rest.to_string());
        }
        let prefix = format!("{path}.");
        for key in table.triggers.keys() {
            if key.starts_with(&prefix) {
                keys.push(key.clone());
            }
        }
        keys.sort();
        keys.dedup();
        keys.iter()
            .filter_map(|key| table.triggers.get(key).copied())
            .collect()
    });
    for signal in signals {
        signal.update(|n| *n = n.wrapping_add(1));
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

impl<T: Clone + PartialEq + 'static> Store<T> {
    pub fn signal(&self) -> Signal<T> {
        self.root
    }

    pub fn get(&self) -> T {
        self.root.get()
    }

    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.root.with(f)
    }

    pub fn set(&self, value: T) {
        self.root.set(value);
        notify_all_paths(self.root.id());
    }

    pub fn update(&self, f: impl FnOnce(&mut T)) {
        self.root.update(f);
        notify_all_paths(self.root.id());
    }

    /// Solid-style `produce`: in-place mutable update (alias of [`Self::update`]).
    ///
    /// Coarse: notifies the root signal **and every registered path**. Prefer [`Self::field`]
    /// when sibling computes must not re-run (I-8).
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
        ensure_path_trigger(self.root.id(), &path);
        StoreField {
            root: self.root,
            path: Rc::from(path),
            get: Rc::new(get),
            set: Rc::new(set),
        }
    }
}

/// Fine-grained store path (I-8). Clone freely; not `Copy` (holds getters).
pub struct StoreField<T, U> {
    root: Signal<T>,
    path: Rc<str>,
    get: Rc<dyn Fn(&T) -> U>,
    set: Rc<dyn Fn(&mut T, U)>,
}

impl<T, U> Clone for StoreField<T, U> {
    fn clone(&self) -> Self {
        Self {
            root: self.root,
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
        let _gen = ensure_path_trigger(self.root.id(), &self.path).get();
        (self.get)(&self.root.peek())
    }

    pub fn peek(&self) -> U {
        (self.get)(&self.root.peek())
    }

    /// Write this path. Notifies this path, ancestors, and registered descendants — not siblings.
    pub fn set(&self, value: U) {
        if self.peek() == value {
            return;
        }
        let setter = Rc::clone(&self.set);
        let next = value.clone();
        self.root.update(move |root| setter(root, next));
        notify_path_tree(self.root.id(), &self.path);
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
                triggers: HashMap::new(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::{batch, create_effect, create_memo, create_root};
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn store_update_and_select() {
        let (_owner, ()) = create_root(|| {
            let store = create_store(vec![1, 2, 3]);
            let len = store_select(store, |v| v.len());
            assert_eq!(len.get(), 3);
            batch(|| {
                store.update(|v| v.push(4));
            });
            assert_eq!(len.get(), 4);
            assert_eq!(store.get(), vec![1, 2, 3, 4]);
            store.produce(|v| v.push(5));
            assert_eq!(store.get(), vec![1, 2, 3, 4, 5]);
        });
    }

    #[test]
    fn produce_notifies_select_only_when_slice_changes() {
        #[derive(Clone, Debug, PartialEq)]
        struct Form {
            name: String,
            age: u32,
        }
        let (_owner, ()) = create_root(|| {
            let store = create_store(Form {
                name: "a".into(),
                age: 1,
            });
            let name = store_select(store, |f| f.name.clone());
            let age = store_select(store, |f| f.age);
            assert_eq!(name.get(), "a");
            assert_eq!(age.get(), 1);

            store.produce(|f| f.age = 2);
            assert_eq!(age.get(), 2);
            assert_eq!(name.get(), "a", "name select must stay equal");

            store.produce(|f| f.name = "b".into());
            assert_eq!(name.get(), "b");
            assert_eq!(age.get(), 2);
        });
    }

    #[test]
    fn field_sibling_set_does_not_rerun_sibling_compute() {
        #[derive(Clone, Debug, PartialEq)]
        struct Form {
            name: String,
            age: u32,
        }
        let (_owner, ()) = create_root(|| {
            let store = create_store(Form {
                name: "a".into(),
                age: 1,
            });
            let name = store.field("name", |f| f.name.clone(), |f, v| f.name = v);
            let age = store.field("age", |f| f.age, |f, v| f.age = v);

            let name_computes = Rc::new(Cell::new(0u32));
            let age_computes = Rc::new(Cell::new(0u32));
            let name_memo = create_memo({
                let name = name.clone();
                let name_computes = Rc::clone(&name_computes);
                move || {
                    name_computes.set(name_computes.get() + 1);
                    name.get()
                }
            });
            let age_memo = create_memo({
                let age = age.clone();
                let age_computes = Rc::clone(&age_computes);
                move || {
                    age_computes.set(age_computes.get() + 1);
                    age.get()
                }
            });

            assert_eq!(name_memo.get(), "a");
            assert_eq!(age_memo.get(), 1);
            let name_baseline = name_computes.get();
            let age_baseline = age_computes.get();
            assert!(name_baseline >= 1);
            assert!(age_baseline >= 1);

            age.set(2);
            assert_eq!(age_memo.get(), 2);
            assert!(
                age_computes.get() > age_baseline,
                "age compute must re-run on age.set"
            );
            assert_eq!(
                name_computes.get(),
                name_baseline,
                "changing age must not re-run name compute"
            );
            assert_eq!(name_memo.get(), "a");

            let age_after = age_computes.get();
            name.set("b".into());
            assert_eq!(name_memo.get(), "b");
            assert!(name_computes.get() > name_baseline);
            assert_eq!(
                age_computes.get(),
                age_after,
                "changing name must not re-run age compute"
            );
            assert_eq!(age_memo.get(), 2);
        });
    }

    #[test]
    fn field_nested_parent_set_notifies_descendants_not_unrelated() {
        #[derive(Clone, Debug, PartialEq)]
        struct User {
            name: String,
            age: u32,
        }
        #[derive(Clone, Debug, PartialEq)]
        struct App {
            user: User,
            title: String,
        }
        let (_owner, ()) = create_root(|| {
            let store = create_store(App {
                user: User {
                    name: "a".into(),
                    age: 1,
                },
                title: "t".into(),
            });
            let user = store.field("user", |a| a.user.clone(), |a, v| a.user = v);
            let user_name =
                store.field("user.name", |a| a.user.name.clone(), |a, v| a.user.name = v);
            let title = store.field("title", |a| a.title.clone(), |a, v| a.title = v);

            let name_computes = Rc::new(Cell::new(0u32));
            let title_computes = Rc::new(Cell::new(0u32));
            let name_memo = create_memo({
                let user_name = user_name.clone();
                let name_computes = Rc::clone(&name_computes);
                move || {
                    name_computes.set(name_computes.get() + 1);
                    user_name.get()
                }
            });
            let title_memo = create_memo({
                let title = title.clone();
                let title_computes = Rc::clone(&title_computes);
                move || {
                    title_computes.set(title_computes.get() + 1);
                    title.get()
                }
            });
            assert_eq!(name_memo.get(), "a");
            assert_eq!(title_memo.get(), "t");
            let name_baseline = name_computes.get();
            let title_baseline = title_computes.get();

            user.set(User {
                name: "b".into(),
                age: 9,
            });
            assert_eq!(name_memo.get(), "b");
            assert!(
                name_computes.get() > name_baseline,
                "replacing user must re-run user.name"
            );
            assert_eq!(
                title_computes.get(),
                title_baseline,
                "replacing user must not re-run unrelated title"
            );
            assert_eq!(title_memo.get(), "t");
        });
    }

    #[test]
    fn field_set_updates_root_peek_without_tracking_root_in_effect() {
        #[derive(Clone, Debug, PartialEq)]
        struct Form {
            name: String,
            age: u32,
        }
        let (_owner, ()) = create_root(|| {
            let store = create_store(Form {
                name: "a".into(),
                age: 1,
            });
            let age = store.field("age", |f| f.age, |f, v| f.age = v);
            let runs = Rc::new(Cell::new(0u32));
            create_effect({
                let age = age.clone();
                let runs = Rc::clone(&runs);
                move || {
                    runs.set(runs.get() + 1);
                    let _ = age.get();
                }
            });
            assert_eq!(runs.get(), 1);
            store.produce(|f| f.name = "b".into());
            assert_eq!(
                runs.get(),
                2,
                "whole-store produce is coarse and notifies all paths"
            );
            age.set(3);
            assert_eq!(runs.get(), 3);
            assert_eq!(store.peek().age, 3);
            assert_eq!(store.peek().name, "b");
        });
    }
}
