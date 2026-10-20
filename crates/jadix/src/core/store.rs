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
use std::collections::HashMap;
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
    paths: HashMap<String, StorePath>,
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
        table
            .paths
            .iter()
            .filter(|(key, _)| {
                path.is_none_or(|path| {
                    key.as_str() == path
                        || key
                            .strip_prefix(path)
                            .is_some_and(|suffix| suffix.starts_with('.'))
                        || path
                            .strip_prefix(key.as_str())
                            .is_some_and(|suffix| suffix.starts_with('.'))
                })
            })
            .map(|(_, entry)| entry.clone())
            .collect()
    });
    // Evaluate user projections outside the table borrow and without subscribing the writer.
    // ponytail: root writes scan registered paths; use field writes for large hot stores.
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

impl<T: Clone + PartialEq + 'static> Store<T> {
    /// Raw root signal. Write through the store/fields to also invalidate path subscribers.
    pub fn signal(&self) -> Signal<T> {
        self.root
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

/// Fine-grained store path (I-8). Clone freely; not `Copy` (holds getters).
pub struct StoreField<T, U> {
    root: Signal<T>,
    trigger: Signal<u32>,
    path: Rc<str>,
    get: Rc<dyn Fn(&T) -> U>,
    set: Rc<dyn Fn(&mut T, U)>,
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
        let next = value.clone();
        batch(|| {
            self.root.update(move |root| setter(root, next));
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
                paths: HashMap::new(),
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
    fn store_write_flushes_root_and_overlapping_paths_together() {
        let (_owner, ()) = create_root(|| {
            let store = create_store(((1, 2), 0));
            let point = store.field("point", |s| s.0, |s, v| s.0 = v);
            let x = store.field("point.x", |s| s.0 .0, |s, v| s.0 .0 = v);
            let y = store.field("point.y", |s| s.0 .1, |s, v| s.0 .1 = v);
            let root_x = create_memo(move || store.get().0 .0);
            let field_x = create_memo({
                let x = x.clone();
                move || x.get()
            });
            let seen = Rc::new(RefCell::new(Vec::new()));
            create_effect({
                let seen = Rc::clone(&seen);
                let point = point.clone();
                move || {
                    seen.borrow_mut().push((
                        store.get().0,
                        point.get(),
                        root_x.get(),
                        field_x.get(),
                        y.get(),
                    ));
                }
            });

            x.set(3);
            store.set(((4, 5), 0));
            store.update(|s| s.0 = (6, 7));
            store.produce(|s| s.0 = (8, 9));
            batch(|| {
                x.update(|v| *v = 10);
                store.update(|s| s.0 .1 = 11);
                assert_eq!(seen.borrow().len(), 5, "outer batch must defer effects");
            });
            point.set((12, 13));

            assert_eq!(
                *seen.borrow(),
                vec![
                    ((1, 2), (1, 2), 1, 1, 2),
                    ((3, 2), (3, 2), 3, 3, 2),
                    ((4, 5), (4, 5), 4, 4, 5),
                    ((6, 7), (6, 7), 6, 6, 7),
                    ((8, 9), (8, 9), 8, 8, 9),
                    ((10, 11), (10, 11), 10, 10, 11),
                    ((12, 13), (12, 13), 12, 12, 13),
                ],
                "one write must expose one consistent root/path/memo snapshot"
            );
        });
    }

    #[test]
    #[allow(deprecated)]
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
            let name = store.field("name", |f| f.name.clone(), |f, v| f.name = v);
            let age = store.field("age", |f| f.age, |f, v| f.age = v);
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
                1,
                "whole-store produce must skip unchanged field observers"
            );
            age.set(3);
            assert_eq!(runs.get(), 2);
            assert_eq!(store.peek().age, 3);
            assert_eq!(store.peek().name, "b");
        });
    }

    #[test]
    fn whole_store_writes_only_notify_changed_projections() {
        let (_owner, ()) = create_root(|| {
            let store = create_store(((1, 2), 0));
            let point = store.field("point", |s| s.0, |s, v| s.0 = v);
            let x = store.field("point.x", |s| s.0 .0, |s, v| s.0 .0 = v);
            let y = store.field("point.y", |s| s.0 .1, |s, v| s.0 .1 = v);
            let title = store.field("title", |s| s.1, |s, v| s.1 = v);
            let runs = Rc::new(Cell::new([0; 4]));
            for (index, read) in [
                Box::new(move || {
                    point.get();
                }) as Box<dyn Fn()>,
                Box::new(move || {
                    x.get();
                }),
                Box::new(move || {
                    y.get();
                }),
                Box::new(move || {
                    title.get();
                }),
            ]
            .into_iter()
            .enumerate()
            {
                let runs = Rc::clone(&runs);
                create_effect(move || {
                    read();
                    let mut counts = runs.get();
                    counts[index] += 1;
                    runs.set(counts);
                });
            }
            store.set(((3, 2), 0));
            assert_eq!(runs.get(), [2, 2, 1, 1]);
            store.update(|s| s.0 .1 = 4);
            assert_eq!(runs.get(), [3, 2, 2, 1]);
            store.produce(|s| s.1 = 5);
            assert_eq!(runs.get(), [3, 2, 2, 2]);
            store.set(store.peek());
            store.update(|_| {});
            assert_eq!(runs.get(), [3, 2, 2, 2], "equal writes do not notify");
            batch(|| {
                store.update(|s| s.0 .0 = 6);
                store.update(|s| s.0 .0 = 7);
            });
            assert_eq!(
                runs.get(),
                [4, 3, 2, 2],
                "outer batch deduplicates observers"
            );
        });
    }

    #[test]
    fn registered_path_outlives_first_reader_and_does_not_clone_root() {
        #[derive(PartialEq)]
        struct Data {
            value: i32,
            clones: Rc<Cell<usize>>,
        }
        impl Clone for Data {
            fn clone(&self) -> Self {
                self.clones.set(self.clones.get() + 1);
                Self {
                    value: self.value,
                    clones: Rc::clone(&self.clones),
                }
            }
        }
        let (owner, (store, field, clones)) = create_root(|| {
            let clones = Rc::new(Cell::new(0));
            let store = create_store(Data {
                value: 1,
                clones: Rc::clone(&clones),
            });
            let (reader, field) =
                create_root(|| store.field("value", |d| d.value, |d, v| d.value = v));
            reader.dispose();
            (store, field, clones)
        });
        let runs = Rc::new(Cell::new(0));
        let (reader, ()) = create_root({
            let field = field.clone();
            let runs = Rc::clone(&runs);
            move || {
                create_effect(move || {
                    field.get();
                    runs.set(runs.get() + 1);
                });
            }
        });
        assert_eq!(field.get(), 1);
        assert_eq!(
            clones.get(),
            0,
            "path reads/comparisons must borrow the root"
        );
        store.set(Data {
            value: 2,
            clones: Rc::clone(&clones),
        });
        assert_eq!(runs.get(), 2);
        assert_eq!(clones.get(), 0, "whole-value set needs no extra root clone");
        reader.dispose();
        owner.dispose();
        assert!(!store.signal().is_alive());
        assert!(!field.trigger.is_alive());
        STORE_PATHS.with(|map| assert!(!map.borrow().contains_key(&store.root.id())));
    }
}
