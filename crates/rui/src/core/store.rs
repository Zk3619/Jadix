//! Solid-inspired `createStore` MVP: a root signal plus path selectors via memo.
//!
//! Full proxy-based path subscription (Solid `produce` / nested stores) is deferred; this closes the
//! P1 gap enough for forms / tables to share one source of truth and derive fine-grained views.

use super::reactivity::{create_memo, create_signal, Memo, Signal};

/// Reactive store root. Mutations go through [`Store::update`] / [`Store::set`].
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
    }

    pub fn update(&self, f: impl FnOnce(&mut T)) {
        self.root.update(f);
    }

    /// Solid-style `produce`: in-place mutable update (alias of [`Self::update`]).
    ///
    /// Contract (WP-2.5): same notification rules as [`Self::update`] — subscribers of the root
    /// signal and of [`store_select`] memos re-run only when the selected slice's `PartialEq`
    /// value changes.
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
}

/// Create a store owned by the current reactive owner.
pub fn create_store<T: Clone + PartialEq + 'static>(initial: T) -> Store<T> {
    Store {
        root: create_signal(initial),
    }
}

/// Derive a fine-grained view from a store (Solid `createMemo` over a path).
///
/// Prefer this over reading the whole store in many leaves: only subscribers of the memo re-run when
/// the selected slice changes (`PartialEq`).
pub fn store_select<T, U, F>(store: Store<T>, mut select: F) -> Memo<U>
where
    T: Clone + PartialEq + 'static,
    U: Clone + PartialEq + 'static,
    F: FnMut(&T) -> U + 'static,
{
    create_memo(move || store.with(|v| select(v)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::{batch, create_root};

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
}
