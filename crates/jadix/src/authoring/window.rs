//! Typed UI-thread storage partitioned by the active window and released at teardown.

use crate::app::window_scope::{self, WindowScope};
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Captured window context for deferred product callbacks and cleanup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Scope(WindowScope);

impl Scope {
    pub fn capture() -> Self {
        Self(window_scope::active())
    }
    pub fn enter(self) -> impl Drop {
        window_scope::enter(self.0)
    }
}

static NEXT_KEY: AtomicU64 = AtomicU64::new(1);

type Values = HashMap<u64, Rc<dyn Any>>;
thread_local! {
    static VALUES: RefCell<HashMap<WindowScope, Values>> = RefCell::new(HashMap::new());
    static RETIRED: RefCell<HashSet<WindowScope>> = RefCell::new(HashSet::new());
}

/// A lazy typed slot. Each UI thread and window has its own value. No storage
/// borrow is held while the caller runs, so access to other slots and nested
/// access to interior-mutable values follow their ordinary Rust borrow rules.
pub struct Local<Value: 'static> {
    key: AtomicU64,
    initialize: fn() -> Value,
}

impl<Value: 'static> Local<Value> {
    pub const fn new(initialize: fn() -> Value) -> Self {
        Self {
            key: AtomicU64::new(0),
            initialize,
        }
    }

    pub fn with<Result>(&self, callback: impl FnOnce(&Value) -> Result) -> Result {
        let mut key = self.key.load(Ordering::Relaxed);
        if key == 0 {
            let allocated = NEXT_KEY.fetch_add(1, Ordering::Relaxed);
            key = self
                .key
                .compare_exchange(0, allocated, Ordering::Relaxed, Ordering::Relaxed)
                .unwrap_or_else(|existing| existing);
            if key == 0 {
                key = allocated;
            }
        }
        let scope = window_scope::active();
        let existing = VALUES
            .try_with(|values| {
                values
                    .borrow()
                    .get(&scope)
                    .and_then(|values| values.get(&key))
                    .cloned()
            })
            .ok()
            .flatten();
        let value = if let Some(value) = existing {
            value
        } else {
            let value: Rc<dyn Any> = Rc::new((self.initialize)());
            if RETIRED
                .try_with(|retired| retired.borrow().contains(&scope))
                .unwrap_or(true)
            {
                value
            } else {
                VALUES
                    .try_with(|values| {
                        values
                            .borrow_mut()
                            .entry(scope)
                            .or_default()
                            .entry(key)
                            .or_insert_with(|| value.clone())
                            .clone()
                    })
                    .unwrap_or_else(|_| value.clone())
            }
        };
        callback(value.downcast_ref::<Value>().expect("typed window slot"))
    }
}

impl<Value: Copy + 'static> Local<Cell<Value>> {
    pub fn get(&self) -> Value {
        self.with(Cell::get)
    }
    pub fn set(&self, value: Value) {
        self.with(|slot| slot.set(value));
    }
    pub fn replace(&self, value: Value) -> Value {
        self.with(|slot| slot.replace(value))
    }
}

/// Define typed slots with the same callback shape as thread-local storage.
/// Window teardown removes values before releasing their captures. Late cleanup
/// in a retired window receives an ephemeral default and cannot resurrect it.
#[macro_export]
macro_rules! window_local {
    () => {};
    ($(#[$attribute:meta])* $visibility:vis static $name:ident: $value:ty = const { $($initialize:tt)* }; $($rest:tt)*) => {
        $(#[$attribute])* $visibility static $name: $crate::authoring::window::Local<$value> =
            $crate::authoring::window::Local::new(|| { $($initialize)* });
        $crate::window_local! { $($rest)* }
    };
    ($(#[$attribute:meta])* $visibility:vis static $name:ident: $value:ty = $initialize:expr; $($rest:tt)*) => {
        $(#[$attribute])* $visibility static $name: $crate::authoring::window::Local<$value> =
            $crate::authoring::window::Local::new(|| $initialize);
        $crate::window_local! { $($rest)* }
    };
}

pub use crate::window_local;

pub(crate) fn forget(scope: WindowScope) {
    RETIRED.with(|retired| retired.borrow_mut().insert(scope));
    let removed = VALUES.with(|values| values.borrow_mut().remove(&scope));
    let _scope = window_scope::enter(scope);
    drop(removed);
}

pub(crate) fn activate(scope: WindowScope) {
    RETIRED.with(|retired| retired.borrow_mut().remove(&scope));
}

#[cfg(test)]
mod tests {
    use super::*;
    window_local! {
        static COUNT: Cell<usize> = const { Cell::new(0) };
        static RETAINED: RefCell<Option<Rc<()>>> = const { RefCell::new(None) };
    }

    #[test]
    fn identical_slot_names_in_different_windows_do_not_share_values() {
        activate(WindowScope::Primary);
        let _primary = window_scope::enter_primary();
        COUNT.set(3);
        {
            let _secondary = window_scope::enter(WindowScope::Secondary(802));
            activate(WindowScope::Secondary(802));
            assert_eq!(COUNT.get(), 0);
            COUNT.set(7);
        }
        assert_eq!(COUNT.get(), 3);
        forget(WindowScope::Secondary(802));
    }

    #[test]
    fn closing_a_window_releases_captures_without_resurrection() {
        let scope = WindowScope::Secondary(803);
        activate(scope);
        let _scope = window_scope::enter(scope);
        let value = Rc::new(());
        let weak = Rc::downgrade(&value);
        RETAINED.with(|slot| *slot.borrow_mut() = Some(value));
        assert!(weak.upgrade().is_some());
        forget(scope);
        assert!(weak.upgrade().is_none());
        COUNT.set(12);
        assert_eq!(COUNT.get(), 0);
    }

    #[test]
    fn nested_access_does_not_hold_the_storage_map_borrow() {
        let _primary = window_scope::enter_primary();
        activate(WindowScope::Primary);
        COUNT.with(|count| {
            RETAINED.with(|retained| {
                count.set(9);
                retained.borrow_mut().take();
                assert_eq!(COUNT.get(), 9);
            })
        });
    }
}
